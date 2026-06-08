//! LLM provider configuration types (E6.1).
//!
//! Embedded as an optional `llm` field inside the top-level [`Config`].
//! Absent (or `enabled: false`) means no LLM pass — zero overhead.

use serde::{Deserialize, Serialize};

/// Which LLM backend to use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum LlmProviderKind {
    /// Ollama local server (default).  No API key needed.
    #[default]
    Ollama,
    /// Anthropic Claude API.  Requires `ANTHROPIC_API_KEY` or `apiKey` in config.
    Anthropic,
    /// OpenAI Chat Completions API.  Requires `OPENAI_API_KEY` or `apiKey`.
    #[serde(rename = "openai")]
    OpenAi,
    /// Any OpenAI-compatible endpoint (Groq, DeepSeek, Mistral, etc.).
    /// `baseUrl` is required; `apiKey` may be `"none"` for local servers.
    #[serde(rename = "openaiCompatible")]
    OpenAiCompatible,
}

/// LLM refinement configuration.
///
/// Add under the top-level `llm` key in `infergen.config.{json,toml}`:
/// ```json
/// { "llm": { "enabled": true, "provider": "ollama", "model": "llama3.2" } }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfig {
    /// Enable the LLM refinement pass.  Default: `false`.
    #[serde(default)]
    pub enabled: bool,

    /// LLM backend to use.  Default: `"ollama"`.
    #[serde(default)]
    pub provider: LlmProviderKind,

    /// Model name override.  `None` → provider-specific default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Base URL override.  `None` → provider-specific default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    /// API key.  `None` → fall back to environment variable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Only send events with confidence below this threshold to the LLM.
    /// Default: `0.75`.
    #[serde(default = "default_threshold")]
    pub confidence_threshold: f64,

    /// Maximum events per LLM request.  Default: `10`.
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Per-request HTTP timeout in seconds.  Default: `30`.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_threshold() -> f64 {
    0.75
}
fn default_batch_size() -> usize {
    10
}
fn default_timeout() -> u64 {
    30
}

impl Default for LlmConfig {
    fn default() -> Self {
        LlmConfig {
            enabled: false,
            provider: LlmProviderKind::default(),
            model: None,
            base_url: None,
            api_key: None,
            confidence_threshold: default_threshold(),
            batch_size: default_batch_size(),
            timeout_secs: default_timeout(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_provider_is_ollama() {
        let c = LlmConfig::default();
        assert_eq!(c.provider, LlmProviderKind::Ollama);
    }

    #[test]
    fn deserialize_empty_object_uses_defaults() {
        let c: LlmConfig = serde_json::from_str("{}").unwrap();
        assert!(!c.enabled);
        assert_eq!(c.provider, LlmProviderKind::Ollama);
        assert!((c.confidence_threshold - 0.75).abs() < 1e-9);
        assert_eq!(c.batch_size, 10);
        assert_eq!(c.timeout_secs, 30);
    }

    #[test]
    fn provider_kind_roundtrips_json() {
        let kinds = [
            (LlmProviderKind::Ollama, "\"ollama\""),
            (LlmProviderKind::Anthropic, "\"anthropic\""),
            (LlmProviderKind::OpenAi, "\"openai\""),
            (LlmProviderKind::OpenAiCompatible, "\"openaiCompatible\""),
        ];
        for (kind, expected) in &kinds {
            let s = serde_json::to_string(kind).unwrap();
            assert_eq!(&s, expected, "serialize {kind:?}");
            let back: LlmProviderKind = serde_json::from_str(&s).unwrap();
            assert_eq!(&back, kind, "deserialize {kind:?}");
        }
    }

    #[test]
    fn camel_case_keys_in_json() {
        let c = LlmConfig {
            enabled: true,
            confidence_threshold: 0.8,
            batch_size: 5,
            timeout_secs: 60,
            ..Default::default()
        };
        let s = serde_json::to_string(&c).unwrap();
        assert!(s.contains("confidenceThreshold"), "missing confidenceThreshold");
        assert!(s.contains("batchSize"), "missing batchSize");
        assert!(s.contains("timeoutSecs"), "missing timeoutSecs");
        assert!(!s.contains("confidence_threshold"), "snake_case leaked");
    }

    #[test]
    fn none_fields_omitted_from_json() {
        let c = LlmConfig::default();
        let s = serde_json::to_string(&c).unwrap();
        assert!(!s.contains("model"), "model should be absent when None");
        assert!(!s.contains("baseUrl"), "baseUrl should be absent when None");
        assert!(!s.contains("apiKey"), "apiKey should be absent when None");
    }
}
