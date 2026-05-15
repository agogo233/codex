use codex_protocol::config_types::ReasoningSummary;
use codex_protocol::openai_models::ApplyPatchToolType;
use codex_protocol::openai_models::ConfigShellToolType;
use codex_protocol::openai_models::InputModality;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::openai_models::ModelVisibility;
use codex_protocol::openai_models::ModelsResponse;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::openai_models::ReasoningEffortPreset;
use codex_protocol::openai_models::TruncationPolicyConfig;
use codex_protocol::openai_models::WebSearchToolType;

const CLAUDE_CONTEXT_WINDOW: i64 = 200_000;

pub(crate) fn static_anthropic_catalog() -> ModelsResponse {
    ModelsResponse {
        models: vec![
            claude_model("claude-sonnet-4-20250514", "Claude Sonnet 4", /*priority*/ 0),
            claude_model("claude-3-5-sonnet-20241022", "Claude 3.5 Sonnet", /*priority*/ 1),
            claude_model("claude-3-5-haiku-20241022", "Claude 3.5 Haiku", /*priority*/ 2),
            claude_model("claude-3-opus-20240229", "Claude 3 Opus", /*priority*/ 3),
            claude_model("claude-3-haiku-20240307", "Claude 3 Haiku", /*priority*/ 4),
        ],
    }
}

fn claude_model(slug: &str, display_name: &str, priority: i32) -> ModelInfo {
    ModelInfo {
        slug: slug.to_string(),
        display_name: display_name.to_string(),
        description: Some(display_name.to_string()),
        default_reasoning_level: None,
        supported_reasoning_levels: vec![
            reasoning_effort_preset(ReasoningEffort::None),
        ],
        shell_type: ConfigShellToolType::ShellCommand,
        visibility: ModelVisibility::List,
        supported_in_api: true,
        priority,
        additional_speed_tiers: Vec::new(),
        service_tiers: Vec::new(),
        availability_nux: None,
        upgrade: None,
        base_instructions: crate::provider::DEFAULT_APPROVAL_REVIEW_PREFERRED_MODEL.to_string(),
        model_messages: None,
        supports_reasoning_summaries: false,
        default_reasoning_summary: ReasoningSummary::None,
        support_verbosity: false,
        default_verbosity: None,
        apply_patch_tool_type: Some(ApplyPatchToolType::Freeform),
        web_search_tool_type: WebSearchToolType::Text,
        truncation_policy: TruncationPolicyConfig::tokens(/*limit*/ 10_000),
        supports_parallel_tool_calls: true,
        supports_image_detail_original: true,
        context_window: Some(CLAUDE_CONTEXT_WINDOW),
        max_context_window: Some(CLAUDE_CONTEXT_WINDOW),
        auto_compact_token_limit: None,
        effective_context_window_percent: 95,
        experimental_supported_tools: Vec::new(),
        input_modalities: vec![InputModality::Text, InputModality::Image],
        used_fallback_model_metadata: false,
        supports_search_tool: false,
    }
}

fn reasoning_effort_preset(effort: ReasoningEffort) -> ReasoningEffortPreset {
    ReasoningEffortPreset {
        effort,
        description: match effort {
            ReasoningEffort::None => "No reasoning",
            ReasoningEffort::Minimal => "Minimal reasoning",
            ReasoningEffort::Low => "Fast responses with lighter reasoning",
            ReasoningEffort::Medium => "Balances speed and reasoning depth for everyday tasks",
            ReasoningEffort::High => "Greater reasoning depth for complex problems",
            ReasoningEffort::XHigh => "Extra high reasoning depth for complex problems",
        }
        .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn catalog_includes_expected_claude_models() {
        let catalog = static_anthropic_catalog();

        assert_eq!(catalog.models.len(), 5);
        assert_eq!(catalog.models[0].slug, "claude-sonnet-4-20250514");
        assert_eq!(catalog.models[1].slug, "claude-3-5-sonnet-20241022");
        assert_eq!(catalog.models[2].slug, "claude-3-5-haiku-20241022");
        assert_eq!(catalog.models[3].slug, "claude-3-opus-20240229");
        assert_eq!(catalog.models[4].slug, "claude-3-haiku-20240307");
    }

    #[test]
    fn claude_sonnet_4_is_default() {
        let catalog = static_anthropic_catalog();
        let default_model = catalog
            .models
            .iter()
            .find(|m| m.priority == 0)
            .expect("should have a default model");

        assert_eq!(default_model.slug, "claude-sonnet-4-20250514");
    }
}