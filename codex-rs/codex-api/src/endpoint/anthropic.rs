use crate::auth::SharedAuthProvider;
use crate::common::ResponseStream;
use crate::error::ApiError;
use crate::provider::Provider;
use crate::sse::spawn_anthropic_response_stream;
use codex_client::HttpTransport;
use codex_client::RequestCompression;
use codex_client::RequestTelemetry;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use http::HeaderMap;
use http::HeaderValue;
use http::Method;
use serde_json::Value;
use std::sync::Arc;
use tracing::instrument;

use super::session::EndpointSession;

pub struct AnthropicClient<T: HttpTransport> {
    session: EndpointSession<T>,
}

impl<T: HttpTransport> AnthropicClient<T> {
    pub fn new(transport: T, provider: Provider, auth: SharedAuthProvider) -> Self {
        Self {
            session: EndpointSession::new(transport, provider, auth),
        }
    }

    pub fn with_request_telemetry(
        self,
        request: Option<Arc<dyn RequestTelemetry>>,
    ) -> Self {
        Self {
            session: self.session.with_request_telemetry(request),
        }
    }

    #[instrument(
        name = "anthropic.stream_messages",
        level = "info",
        skip_all,
        fields(
            transport = "anthropic_http",
            http.method = "POST",
            api.path = "messages"
        )
    )]
    pub async fn stream_messages(
        &self,
        model: String,
        system: Option<String>,
        messages: Vec<AnthropicMessage>,
        tools: Vec<AnthropicToolDef>,
        max_tokens: i64,
        extra_headers: HeaderMap,
    ) -> Result<ResponseStream, ApiError> {
        let mut body = serde_json::json!({
            "model": model,
            "max_tokens": max_tokens,
            "stream": true,
            "messages": messages,
        });

        if let Some(sys) = system {
            body["system"] = serde_json::json!(sys);
        }

        if !tools.is_empty() {
            body["tools"] = serde_json::to_value(tools)
                .map_err(|e| ApiError::Stream(format!("failed to encode anthropic tools: {e}")))?;
        }

        let stream_response = self
            .session
            .stream_with(
                Method::POST,
                "messages",
                extra_headers,
                Some(body),
                |req| {
                    req.headers.insert(
                        http::header::ACCEPT,
                        HeaderValue::from_static("text/event-stream"),
                    );
                    req.compression = RequestCompression::None;
                },
            )
            .await?;

        Ok(spawn_anthropic_response_stream(
            stream_response,
            self.session.provider().stream_idle_timeout,
        ))
    }
}

/// Anthropic Messages API message format.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: Vec<AnthropicContent>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type")]
pub enum AnthropicContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
    #[serde(rename = "image")]
    Image {
        source: AnthropicImageSource,
    },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AnthropicImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    pub data: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AnthropicToolDef {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
}

/// Convert Codex ResponseItem history to Anthropic Messages format.
pub fn response_items_to_anthropic_messages(
    items: &[ResponseItem],
) -> (Option<String>, Vec<AnthropicMessage>) {
    let mut system: Option<String> = None;
    let mut messages: Vec<AnthropicMessage> = Vec::new();
    let mut current_role: Option<String> = None;
    let mut current_content: Vec<AnthropicContent> = Vec::new();

    fn flush(
        messages: &mut Vec<AnthropicMessage>,
        role: &mut Option<String>,
        content: &mut Vec<AnthropicContent>,
    ) {
        if let Some(r) = role.take() {
            if !content.is_empty() {
                messages.push(AnthropicMessage {
                    role: r,
                    content: std::mem::take(content),
                });
            }
        }
        content.clear();
    }

    for item in items {
        match item {
            ResponseItem::Message { role, content, .. } => {
                if role == "system" {
                    // Collect system messages
                    for c in content {
                        if let ContentItem::InputText { text } = c {
                            system = Some(system.unwrap_or_default() + text);
                        }
                    }
                    continue;
                }

                if current_role.as_deref() != Some(role.as_str()) {
                    flush(&mut messages, &mut current_role, &mut current_content);
                    current_role = Some(role.clone());
                }

                for c in content {
                    match c {
                        ContentItem::InputText { text } => {
                            current_content.push(AnthropicContent::Text { text: text.clone() });
                        }
                        ContentItem::InputImage { image_url, detail: _ } => {
                            // Handle base64 data URLs
                            if let Some(data) = image_url.strip_prefix("data:image/") {
                                let (media_type, b64_data) = data.split_once(";base64,")
                                    .unwrap_or(("png", image_url));
                                current_content.push(AnthropicContent::Image {
                                    source: AnthropicImageSource {
                                        source_type: "base64".into(),
                                        media_type: format!("image/{media_type}"),
                                        data: b64_data.to_string(),
                                    },
                                });
                            }
                        }
                        ContentItem::OutputText { text } => {
                            current_content.push(AnthropicContent::Text { text: text.clone() });
                        }
                    }
                }
            }
            ResponseItem::FunctionCall { name, arguments, call_id, .. } => {
                flush(&mut messages, &mut current_role, &mut current_content);
                current_role = Some("assistant".into());
                current_content.push(AnthropicContent::ToolUse {
                    id: call_id.clone(),
                    name: name.clone(),
                    input: serde_json::from_str(arguments)
                        .unwrap_or(Value::Null),
                });
                flush(&mut messages, &mut current_role, &mut current_content);
            }
            ResponseItem::FunctionCallOutput { call_id, output } => {
                flush(&mut messages, &mut current_role, &mut current_content);
                current_role = Some("user".into());
                current_content.push(AnthropicContent::ToolResult {
                    tool_use_id: call_id.clone(),
                    content: output.to_string(),
                });
                flush(&mut messages, &mut current_role, &mut current_content);
            }
            _ => {
                // Skip other item types (reasoning, shell calls, etc.)
            }
        }
    }

    flush(&mut messages, &mut current_role, &mut current_content);
    (system, messages)
}