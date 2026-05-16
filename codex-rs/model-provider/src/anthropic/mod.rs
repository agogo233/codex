mod auth;
mod catalog;

use std::path::PathBuf;
use std::sync::Arc;

use codex_api::Provider;
use codex_api::SharedAuthProvider;
use codex_login::AuthManager;
use codex_login::CodexAuth;
use codex_model_provider_info::ModelProviderInfo;
use codex_models_manager::manager::SharedModelsManager;
use codex_models_manager::manager::StaticModelsManager;
use codex_protocol::account::ProviderAccount;
use codex_protocol::error::Result;
use codex_protocol::openai_models::ModelsResponse;

use crate::provider::ModelProvider;
use crate::provider::ProviderAccountResult;
use crate::provider::ProviderAccountState;
use crate::provider::ProviderCapabilities;
pub(crate) use catalog::static_anthropic_catalog;

/// Runtime provider for Anthropic's Messages API.
#[derive(Clone, Debug)]
pub(crate) struct AnthropicModelProvider {
    pub(crate) info: ModelProviderInfo,
}

impl AnthropicModelProvider {
    pub fn new(provider_info: ModelProviderInfo) -> Self {
        Self {
            info: provider_info,
        }
    }
}

#[async_trait::async_trait]
impl ModelProvider for AnthropicModelProvider {
    fn info(&self) -> &ModelProviderInfo {
        &self.info
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            namespace_tools: false,
            image_generation: false,
            web_search: false,
        }
    }

    fn approval_review_preferred_model(&self) -> &'static str {
        "claude-sonnet-4-20250514"
    }

    fn auth_manager(&self) -> Option<Arc<AuthManager>> {
        None
    }

    async fn auth(&self) -> Option<CodexAuth> {
        None
    }

    fn account_state(&self) -> ProviderAccountResult {
        Ok(ProviderAccountState {
            account: Some(ProviderAccount::ApiKey),
            requires_openai_auth: false,
        })
    }

    async fn api_provider(&self) -> Result<Provider> {
        self.info.to_api_provider(/*auth_mode*/ None)
    }

    async fn api_auth(&self) -> Result<SharedAuthProvider> {
        resolve_anthropic_auth(&self.info).await
    }

    fn models_manager(
        &self,
        _codex_home: PathBuf,
        config_model_catalog: Option<ModelsResponse>,
    ) -> SharedModelsManager {
        Arc::new(StaticModelsManager::new(
            /*auth_manager*/ None,
            config_model_catalog.unwrap_or_else(static_anthropic_catalog),
        ))
    }
}

async fn resolve_anthropic_auth(provider: &ModelProviderInfo) -> Result<SharedAuthProvider> {
    if let Some(api_key) = provider.api_key()? {
        return Ok(Arc::new(auth::AnthropicAuthProvider::new(api_key)));
    }
    if let Some(token) = provider.experimental_bearer_token.clone() {
        return Ok(Arc::new(auth::AnthropicAuthProvider::new(token)));
    }
    Err(codex_protocol::error::CodexErr::Fatal(
        "ANTHROPIC_API_KEY environment variable is required for the Anthropic provider".into(),
    ))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn capabilities_disable_unsupported_launch_features() {
        let provider = AnthropicModelProvider::new(ModelProviderInfo::create_anthropic_provider());

        assert_eq!(
            provider.capabilities(),
            ProviderCapabilities {
                namespace_tools: false,
                image_generation: false,
                web_search: false,
            }
        );
    }

    #[test]
    fn approval_review_preferred_model_uses_claude_sonnet() {
        let provider = AnthropicModelProvider::new(ModelProviderInfo::create_anthropic_provider());

        assert_eq!(
            provider.approval_review_preferred_model(),
            "claude-sonnet-4-20250514"
        );
    }
}
