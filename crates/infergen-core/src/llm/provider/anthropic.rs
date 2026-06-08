//! Anthropic Claude backend (E6.1).
//!
//! Uses the Anthropic Messages API (`POST /v1/messages`).
//! Default model: `claude-haiku-4-5-20251001` (fast, cost-effective for naming).
//!
//! API key resolution order:
//! 1. `config.api_key`
//! 2. `ANTHROPIC_API_KEY` environment variable
//! 3. `Err(MissingApiKey)`

use std::time::Duration;

use crate::llm::{LlmBackend, LlmError, RefinementRequest, RefinementResponse, extract_json};
use crate::llm::config::LlmConfig;
use crate::llm::prompt::build_prompt;

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const DEFAULT_MODEL: &str = "claude-haiku-4-5-20251001";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Anthropic Claude backend.
pub struct AnthropicBackend {
    api_key: String,
    model: String,
    base_url: String,
    timeout: Duration,
}

impl AnthropicBackend {
    /// Create from config.
    ///
    /// # Errors
    /// [`LlmError::MissingApiKey`] if no key found in config or `ANTHROPIC_API_KEY`.
    pub fn new(config: &LlmConfig) -> Result<Self, LlmError> {
        let api_key = resolve_key(config.api_key.as_deref(), "ANTHROPIC_API_KEY", "anthropic")?;
        Ok(AnthropicBackend {
            api_key,
            model: config
                .model
                .as_deref()
                .unwrap_or(DEFAULT_MODEL)
                .to_owned(),
            base_url: config
                .base_url
                .as_deref()
                .unwrap_or(DEFAULT_BASE_URL)
                .trim_end_matches('/')
                .to_owned(),
            timeout: Duration::from_secs(config.timeout_secs),
        })
    }
}

impl LlmBackend for AnthropicBackend {
    fn refine_batch(&self, request: &RefinementRequest) -> Result<RefinementResponse, LlmError> {
        let prompt = build_prompt(request);
        let url = format!("{}/v1/messages", self.base_url);

        let body: serde_json::Value = ureq::post(&url)
            .set("x-api-key", &self.api_key)
            .set("anthropic-version", ANTHROPIC_VERSION)
            .set("content-type", "application/json")
            .timeout(self.timeout)
            .send_json(serde_json::json!({
                "model": self.model,
                "max_tokens": 4096,
                "messages": [{"role": "user", "content": prompt}],
            }))
            .map_err(|e| LlmError::Http(e.to_string()))?
            .into_json()
            .map_err(|e| LlmError::Parse(e.to_string()))?;

        let text = body
            .pointer("/content/0/text")
            .and_then(|v| v.as_str())
            .ok_or(LlmError::Empty)?;

        if text.trim().is_empty() {
            return Err(LlmError::Empty);
        }

        let json_str = extract_json(text);
        serde_json::from_str::<RefinementResponse>(json_str)
            .map_err(|e| LlmError::Parse(format!("anthropic: {e} — raw: {json_str}")))
    }
}

// ---------------------------------------------------------------------------
// Shared key resolution
// ---------------------------------------------------------------------------

pub(super) fn resolve_key(
    config_key: Option<&str>,
    env_var: &str,
    provider: &'static str,
) -> Result<String, LlmError> {
    if let Some(k) = config_key {
        let k = k.trim();
        if !k.is_empty() {
            return Ok(k.to_owned());
        }
    }
    if let Ok(k) = std::env::var(env_var) {
        let k = k.trim().to_owned();
        if !k.is_empty() {
            return Ok(k);
        }
    }
    Err(LlmError::MissingApiKey { provider })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::config::{LlmConfig, LlmProviderKind};

    fn cfg_anthropic() -> LlmConfig {
        LlmConfig { provider: LlmProviderKind::Anthropic, ..Default::default() }
    }

    #[test]
    fn anthropic_backend_errors_without_key() {
        // Only asserts when ANTHROPIC_API_KEY is absent (expected in CI).
        if std::env::var("ANTHROPIC_API_KEY").is_err() {
            let c = cfg_anthropic();
            assert!(matches!(AnthropicBackend::new(&c), Err(LlmError::MissingApiKey { .. })));
        }
    }

    #[test]
    fn anthropic_backend_config_key_used() {
        let mut c = cfg_anthropic();
        c.api_key = Some("sk-test".into());
        let b = AnthropicBackend::new(&c).unwrap();
        assert_eq!(b.api_key, "sk-test");
    }

    #[test]
    fn anthropic_backend_default_model() {
        let mut c = cfg_anthropic();
        c.api_key = Some("sk-test".into());
        let b = AnthropicBackend::new(&c).unwrap();
        assert_eq!(b.model, DEFAULT_MODEL);
    }

    #[test]
    fn anthropic_backend_model_override() {
        let mut c = cfg_anthropic();
        c.api_key = Some("sk-test".into());
        c.model = Some("claude-opus-4-8".into());
        let b = AnthropicBackend::new(&c).unwrap();
        assert_eq!(b.model, "claude-opus-4-8");
    }
}
