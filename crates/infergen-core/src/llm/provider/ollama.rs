//! Ollama LLM backend (E6.1).
//!
//! Uses the Ollama native `/api/generate` endpoint.  No API key required.
//! Default base URL: `http://localhost:11434`.
//! Default model: `llama3.2`.

use std::time::Duration;

use crate::llm::{LlmBackend, LlmError, RefinementRequest, RefinementResponse, extract_json};
use crate::llm::config::LlmConfig;
use crate::llm::prompt::build_prompt;

const DEFAULT_BASE_URL: &str = "http://localhost:11434";
const DEFAULT_MODEL: &str = "llama3.2";

/// Ollama backend.
pub struct OllamaBackend {
    base_url: String,
    model: String,
    timeout: Duration,
}

impl OllamaBackend {
    /// Create from config.  No API key needed.
    pub fn new(config: &LlmConfig) -> Self {
        OllamaBackend {
            base_url: config
                .base_url
                .as_deref()
                .unwrap_or(DEFAULT_BASE_URL)
                .trim_end_matches('/')
                .to_owned(),
            model: config
                .model
                .as_deref()
                .unwrap_or(DEFAULT_MODEL)
                .to_owned(),
            timeout: Duration::from_secs(config.timeout_secs),
        }
    }
}

impl LlmBackend for OllamaBackend {
    fn refine_batch(&self, request: &RefinementRequest) -> Result<RefinementResponse, LlmError> {
        let prompt = build_prompt(request);
        let url = format!("{}/api/generate", self.base_url);

        let body: serde_json::Value = ureq::post(&url)
            .timeout(self.timeout)
            .send_json(serde_json::json!({
                "model": self.model,
                "prompt": prompt,
                "stream": false,
            }))
            .map_err(|e| LlmError::Http(e.to_string()))?
            .into_json()
            .map_err(|e| LlmError::Parse(e.to_string()))?;

        let text = body
            .get("response")
            .and_then(|v| v.as_str())
            .ok_or(LlmError::Empty)?;

        if text.trim().is_empty() {
            return Err(LlmError::Empty);
        }

        let json_str = extract_json(text);
        serde_json::from_str::<RefinementResponse>(json_str)
            .map_err(|e| LlmError::Parse(format!("ollama: {e} — raw: {json_str}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::config::LlmConfig;

    #[test]
    fn ollama_backend_default_url_and_model() {
        let b = OllamaBackend::new(&LlmConfig::default());
        assert_eq!(b.base_url, DEFAULT_BASE_URL);
        assert_eq!(b.model, DEFAULT_MODEL);
    }

    #[test]
    fn ollama_backend_respects_config_overrides() {
        let mut cfg = LlmConfig::default();
        cfg.base_url = Some("http://custom:11434".into());
        cfg.model = Some("mistral".into());
        let b = OllamaBackend::new(&cfg);
        assert_eq!(b.base_url, "http://custom:11434");
        assert_eq!(b.model, "mistral");
    }

    #[test]
    fn ollama_backend_strips_trailing_slash() {
        let mut cfg = LlmConfig::default();
        cfg.base_url = Some("http://localhost:11434/".into());
        let b = OllamaBackend::new(&cfg);
        assert_eq!(b.base_url, "http://localhost:11434");
    }
}
