//! LLM backend factory (E6.1).

pub mod anthropic;
pub mod ollama;
pub mod openai;

use crate::llm::LlmBackend;
use crate::llm::LlmError;
use crate::llm::config::{LlmConfig, LlmProviderKind};

/// Construct the appropriate [`LlmBackend`] from `config`.
///
/// # Errors
/// - [`LlmError::MissingApiKey`] if the selected provider needs an API key
///   that is absent from both config and environment.
/// - [`LlmError::MissingBaseUrl`] if `OpenAiCompatible` is selected without a
///   `base_url`.
pub fn make_backend(config: &LlmConfig) -> Result<Box<dyn LlmBackend>, LlmError> {
    match config.provider {
        LlmProviderKind::Ollama => Ok(Box::new(ollama::OllamaBackend::new(config))),
        LlmProviderKind::Anthropic => {
            Ok(Box::new(anthropic::AnthropicBackend::new(config)?))
        }
        LlmProviderKind::OpenAi => {
            Ok(Box::new(openai::OpenAiBackend::new(config, false)?))
        }
        LlmProviderKind::OpenAiCompatible => {
            Ok(Box::new(openai::OpenAiBackend::new(config, true)?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> LlmConfig {
        LlmConfig::default()
    }

    #[test]
    fn make_backend_ollama_succeeds_without_key() {
        assert!(make_backend(&cfg()).is_ok());
    }

    #[test]
    fn make_backend_anthropic_errors_without_key() {
        // Only asserts when ANTHROPIC_API_KEY is absent (expected in CI).
        if std::env::var("ANTHROPIC_API_KEY").is_err() {
            let mut c = cfg();
            c.provider = LlmProviderKind::Anthropic;
            assert!(matches!(make_backend(&c), Err(LlmError::MissingApiKey { .. })));
        }
    }

    #[test]
    fn make_backend_openai_errors_without_key() {
        // Only asserts when OPENAI_API_KEY is absent (expected in CI).
        if std::env::var("OPENAI_API_KEY").is_err() {
            let mut c = cfg();
            c.provider = LlmProviderKind::OpenAi;
            assert!(matches!(make_backend(&c), Err(LlmError::MissingApiKey { .. })));
        }
    }

    #[test]
    fn make_backend_openai_compatible_errors_without_base_url() {
        let mut c = cfg();
        c.provider = LlmProviderKind::OpenAiCompatible;
        // No base_url — must error even if a key is present.
        c.api_key = Some("dummy".into());
        assert!(matches!(make_backend(&c), Err(LlmError::MissingBaseUrl)));
    }
}
