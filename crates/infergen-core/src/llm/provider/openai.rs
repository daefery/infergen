//! OpenAI Chat Completions backend (E6.1).
//!
//! Handles both the official OpenAI API and any OpenAI-compatible endpoint
//! (Groq, DeepSeek, Mistral, local Ollama /v1/, etc.).
//!
//! API key resolution order:
//! 1. `config.api_key`
//! 2. `OPENAI_API_KEY` environment variable
//! 3. For `is_compatible = true`: allow dummy key `"none"` when base_url is a
//!    local address (many local servers accept any key).
//! 4. Otherwise `Err(MissingApiKey)`

use std::time::Duration;

use crate::llm::{LlmBackend, LlmError, RefinementRequest, RefinementResponse, extract_json};
use crate::llm::config::LlmConfig;
use crate::llm::prompt::build_prompt;
use crate::llm::provider::anthropic::resolve_key;

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL_OPENAI: &str = "gpt-4o-mini";
const DEFAULT_MODEL_COMPAT: &str = "llama3.2";

/// OpenAI Chat Completions backend (official + compatible).
pub struct OpenAiBackend {
    api_key: String,
    model: String,
    base_url: String,
    timeout: Duration,
}

impl OpenAiBackend {
    /// Create from config.
    ///
    /// `is_compatible`: when `true`, treats as an OpenAI-compatible endpoint —
    /// `base_url` is required and a dummy key `"none"` is accepted for local
    /// servers.
    ///
    /// # Errors
    /// - [`LlmError::MissingBaseUrl`] when `is_compatible` but no `base_url` configured.
    /// - [`LlmError::MissingApiKey`] when no key found and server is remote.
    pub fn new(config: &LlmConfig, is_compatible: bool) -> Result<Self, LlmError> {
        let base_url = if is_compatible {
            config
                .base_url
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .ok_or(LlmError::MissingBaseUrl)?
                .trim_end_matches('/')
                .to_owned()
        } else {
            config
                .base_url
                .as_deref()
                .unwrap_or(DEFAULT_BASE_URL)
                .trim_end_matches('/')
                .to_owned()
        };

        let api_key = if is_compatible && is_local_url(&base_url) {
            // Local servers typically accept any non-empty key.
            config
                .api_key
                .as_deref()
                .filter(|k| !k.trim().is_empty())
                .map(str::to_owned)
                .or_else(|| std::env::var("OPENAI_API_KEY").ok().filter(|k| !k.trim().is_empty()))
                .unwrap_or_else(|| "none".to_owned())
        } else {
            resolve_key(config.api_key.as_deref(), "OPENAI_API_KEY", "openai")?
        };

        let default_model = if is_compatible { DEFAULT_MODEL_COMPAT } else { DEFAULT_MODEL_OPENAI };

        Ok(OpenAiBackend {
            api_key,
            model: config.model.as_deref().unwrap_or(default_model).to_owned(),
            base_url,
            timeout: Duration::from_secs(config.timeout_secs),
        })
    }
}

impl LlmBackend for OpenAiBackend {
    fn refine_batch(&self, request: &RefinementRequest) -> Result<RefinementResponse, LlmError> {
        let prompt = build_prompt(request);
        let url = format!("{}/chat/completions", self.base_url);

        let body: serde_json::Value = ureq::post(&url)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
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
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .ok_or(LlmError::Empty)?;

        if text.trim().is_empty() {
            return Err(LlmError::Empty);
        }

        let json_str = extract_json(text);
        serde_json::from_str::<RefinementResponse>(json_str)
            .map_err(|e| LlmError::Parse(format!("openai: {e} — raw: {json_str}")))
    }
}

/// Return `true` if `url` refers to a local / loopback address.
fn is_local_url(url: &str) -> bool {
    url.contains("localhost") || url.contains("127.0.0.1") || url.contains("::1")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::config::{LlmConfig, LlmProviderKind};

    fn cfg_openai() -> LlmConfig {
        LlmConfig { provider: LlmProviderKind::OpenAi, ..Default::default() }
    }

    fn cfg_compat() -> LlmConfig {
        LlmConfig { provider: LlmProviderKind::OpenAiCompatible, ..Default::default() }
    }

    #[test]
    fn openai_backend_errors_without_key() {
        // Only asserts when OPENAI_API_KEY is absent (expected in CI).
        if std::env::var("OPENAI_API_KEY").is_err() {
            let c = cfg_openai();
            assert!(matches!(OpenAiBackend::new(&c, false), Err(LlmError::MissingApiKey { .. })));
        }
    }

    #[test]
    fn openai_backend_config_key_used() {
        let mut c = cfg_openai();
        c.api_key = Some("sk-test".into());
        let b = OpenAiBackend::new(&c, false).unwrap();
        assert_eq!(b.api_key, "sk-test");
    }

    #[test]
    fn openai_backend_default_model() {
        let mut c = cfg_openai();
        c.api_key = Some("sk-test".into());
        let b = OpenAiBackend::new(&c, false).unwrap();
        assert_eq!(b.model, DEFAULT_MODEL_OPENAI);
    }

    #[test]
    fn openai_compatible_errors_without_base_url() {
        let mut c = cfg_compat();
        c.api_key = Some("dummy".into());
        assert!(matches!(OpenAiBackend::new(&c, true), Err(LlmError::MissingBaseUrl)));
    }

    #[test]
    fn openai_compatible_allows_dummy_key_with_local_url() {
        let mut c = cfg_compat();
        c.base_url = Some("http://localhost:11434/v1".into());
        // With local URL and no config key, falls back to dummy "none" key.
        // If OPENAI_API_KEY is set, it is used instead (also acceptable).
        if std::env::var("OPENAI_API_KEY").is_err() {
            let b = OpenAiBackend::new(&c, true).unwrap();
            assert_eq!(b.api_key, "none");
        } else {
            // env var key takes precedence — backend should still succeed.
            assert!(OpenAiBackend::new(&c, true).is_ok());
        }
    }

    #[test]
    fn openai_compatible_default_model() {
        let mut c = cfg_compat();
        c.base_url = Some("http://localhost:11434/v1".into());
        let b = OpenAiBackend::new(&c, true).unwrap();
        assert_eq!(b.model, DEFAULT_MODEL_COMPAT);
    }

    #[test]
    fn openai_backend_strips_trailing_slash() {
        let mut c = cfg_openai();
        c.api_key = Some("sk-test".into());
        c.base_url = Some("https://api.openai.com/v1/".into());
        let b = OpenAiBackend::new(&c, false).unwrap();
        assert!(!b.base_url.ends_with('/'));
    }

    #[test]
    fn is_local_url_detects_localhost() {
        assert!(is_local_url("http://localhost:11434"));
        assert!(is_local_url("http://127.0.0.1:8080"));
        assert!(!is_local_url("https://api.openai.com"));
        assert!(!is_local_url("https://api.groq.com"));
    }
}
