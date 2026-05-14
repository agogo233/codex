use crate::common::ResponseEvent;
use crate::common::ResponseStream;
use crate::error::ApiError;
use crate::rate_limits::RateLimitSnapshot;
use codex_client::ByteStream;
use codex_client::StreamResponse;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::TokenUsage;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::debug;

const REQUEST_ID_HEADER: &str = "x-request-id";
const ANTHROPIC_REQUEST_ID_HEADER: &str = "x-request-id";

pub fn spawn_anthropic_response_stream(
    stream_response: StreamResponse,
    idle_timeout: Duration,
) -> ResponseStream {
    let upstream_request_id = stream_response
        .headers
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent, ApiError>>(1600);
    tokio::spawn(async move {
        process_anthropic_sse(stream_response.bytes, tx_event, idle_timeout).await;
    });
    ResponseStream {
        rx_event,
        upstream_request_id,
    }
}

#[derive(Debug, Deserialize)]
struct AnthropicMessageStart {
    #[serde(rename = "type")]
    kind: String,
    message: AnthropicMessage,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessage {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    role: String,
    content: Vec<AnthropicContentBlock>,
    model: String,
    stop_reason: Option<String>,
    stop_sequence: Option<String>,
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    kind: String,
    /// text content
    text: Option<String>,
    /// tool_use content
    name: Option<String>,
    input: Option<serde_json::Value>,
    id: Option<String>,
    /// tool_result content
    tool_use_id: Option<String>,
    content: Option<serde_json::Value>,
    /// thinking content
    thinking: Option<String>,
    signature: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlockDelta {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
    partial_json: Option<String>,
    thinking: Option<String>,
    signature: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessageDelta {
    stop_reason: Option<String>,
    stop_sequence: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicDeltaEvent {
    #[serde(rename = "type")]
    kind: String,
    index: Option<i64>,
    delta: Option<AnthropicContentBlockDelta>,
    content_block: Option<AnthropicContentBlock>,
    message: Option<AnthropicMessage>,
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: i64,
    output_tokens: i64,
    cache_creation_input_tokens: Option<i64>,
    cache_read_input_tokens: Option<i64>,
}

async fn process_anthropic_sse(
    byte_stream: ByteStream,
    tx_event: mpsc::Sender<Result<ResponseEvent, ApiError>>,
    idle_timeout: Duration,
) {
    let mut stream = byte_stream.eventsource();
    let mut current_item_id: i64 = 0;
    let mut response_id: Option<String> = None;
    let mut output_items: Vec<ResponseItem> = Vec::new();
    let mut token_usage: Option<TokenUsage> = None;
    let mut assistant_message_index: Option<i64> = None;

    loop {
        let next_event = tokio::select! {
            biased;
            event = stream.next() => event,
            _ = tokio::time::sleep(idle_timeout) => {
                debug!("anthropic sse idle timeout");
                let _ = tx_event.send(Err(ApiError::Stream("Anthropic SSE idle timeout".into()))).await;
                return;
            }
        };

        let Some(Ok(event)) = next_event else {
            break;
        };

        let event_data = event.data.trim().to_string();
        if event_data.is_empty() || event_data == "data: [DONE]" {
            continue;
        }

        let parsed: AnthropicDeltaEvent = match serde_json::from_str(&event_data) {
            Ok(p) => p,
            Err(e) => {
                debug!("failed to parse anthropic event: {e}, data: {event_data}");
                continue;
            }
        };

        match parsed.kind.as_str() {
            "message_start" => {
                if let Some(msg) = parsed.message {
                    response_id = Some(msg.id);
                    let _ = tx_event.send(Ok(ResponseEvent::Created)).await;
                }
            }
            "content_block_start" => {
                if let Some(block) = parsed.content_block {
                    current_item_id += 1;
                    match block.kind.as_str() {
                        "text" => {
                            if let Some(text) = &block.text {
                                let item = ResponseItem::Message {
                                    id: None,
                                    role: "assistant".into(),
                                    content: vec![ContentItem::OutputText { text: text.clone() }],
                                    phase: None,
                                };
                                let _ = tx_event.send(Ok(ResponseEvent::OutputItemAdded(item))).await;
                                output_items.push(ResponseItem::Message {
                                    id: None,
                                    role: "assistant".into(),
                                    content: vec![],
                                    phase: None,
                                });
                                assistant_message_index = Some((output_items.len() - 1) as i64);
                                let _ = tx_event
                                    .send(Ok(ResponseEvent::OutputTextDelta(text.clone())))
                                    .await;
                            }
                        }
                        "tool_use" => {
                            let item = ResponseItem::FunctionCall {
                                id: None,
                                name: block.name.clone().unwrap_or_default(),
                                namespace: None,
                                arguments: String::new(),
                                call_id: block.id.clone().unwrap_or_default(),
                            };
                            let _ = tx_event.send(Ok(ResponseEvent::OutputItemAdded(item))).await;
                            assistant_message_index = Some((output_items.len()) as i64);
                        }
                        "thinking" => {
                            if let Some(thinking_text) = &block.thinking {
                                let _ = tx_event
                                    .send(Ok(ResponseEvent::ReasoningContentDelta {
                                        delta: thinking_text.clone(),
                                        content_index: 0,
                                    }))
                                    .await;
                            }
                        }
                        _ => {}
                    }
                }
            }
            "content_block_delta" => {
                if let Some(delta) = parsed.delta {
                    match delta.kind.as_str() {
                        "text_delta" => {
                            if let Some(text) = delta.text {
                                let _ = tx_event
                                    .send(Ok(ResponseEvent::OutputTextDelta(text)))
                                    .await;
                            }
                        }
                        "input_json_delta" => {
                            if let Some(partial) = delta.partial_json {
                                let _ = tx_event
                                    .send(Ok(ResponseEvent::ToolCallInputDelta {
                                        item_id: String::new(),
                                        call_id: None,
                                        delta: partial,
                                    }))
                                    .await;
                            }
                        }
                        "thinking_delta" => {
                            if let Some(thinking_text) = delta.thinking {
                                let _ = tx_event
                                    .send(Ok(ResponseEvent::ReasoningContentDelta {
                                        delta: thinking_text,
                                        content_index: 0,
                                    }))
                                    .await;
                            }
                        }
                        "signature_delta" => {
                            if let Some(sig) = delta.signature {
                                let _ = tx_event
                                    .send(Ok(ResponseEvent::ReasoningContentDelta {
                                        delta: sig,
                                        content_index: 1,
                                    }))
                                    .await;
                            }
                        }
                        _ => {}
                    }
                }
            }
            "content_block_stop" => {
                if let Some(assistant_idx) = assistant_message_index
                    && assistant_idx < output_items.len() as i64
                {
                    let item = output_items[assistant_idx as usize].clone();
                    let _ = tx_event.send(Ok(ResponseEvent::OutputItemDone(item))).await;
                }
            }
            "message_delta" => {
                token_usage = parsed.usage.map(|u| TokenUsage {
                    input_tokens: u.input_tokens,
                    cached_input_tokens: u
                        .cache_read_input_tokens
                        .unwrap_or(0),
                    output_tokens: u.output_tokens,
                    reasoning_output_tokens: 0,
                    total_tokens: u.input_tokens + u.output_tokens,
                });
            }
            "message_stop" => {
                let _ = tx_event
                    .send(Ok(ResponseEvent::Completed {
                        response_id: response_id.clone().unwrap_or_default(),
                        token_usage: token_usage.clone(),
                        end_turn: Some(true),
                    }))
                    .await;
                return;
            }
            "ping" => {}
            "error" => {
                let _ = tx_event
                    .send(Err(ApiError::Stream(
                        format!("Anthropic API error: {event_data}"),
                    )))
                    .await;
                return;
            }
            _ => {
                debug!("unhandled anthropic event type: {}", parsed.kind);
            }
        }
    }
}