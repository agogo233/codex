use codex_api::AuthProvider;
use http::HeaderMap;
use http::HeaderValue;

/// Anthropic API authentication using `x-api-key` header.
#[derive(Clone, Debug)]
pub struct AnthropicAuthProvider {
    pub api_key: String,
}

impl AnthropicAuthProvider {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

impl AuthProvider for AnthropicAuthProvider {
    fn add_auth_headers(&self, headers: &mut HeaderMap) {
        if let Ok(header) = HeaderValue::from_str(&self.api_key) {
            let _ = headers.insert("x-api-key", header);
        }
        let _ = headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn anthropic_auth_provider_adds_correct_headers() {
        let auth = AnthropicAuthProvider::new("sk-ant-test123".to_string());
        let mut headers = HeaderMap::new();

        auth.add_auth_headers(&mut headers);

        assert_eq!(
            headers.get("x-api-key").and_then(|v| v.to_str().ok()),
            Some("sk-ant-test123")
        );
        assert_eq!(
            headers
                .get("anthropic-version")
                .and_then(|v| v.to_str().ok()),
            Some("2023-06-01")
        );
    }
}
