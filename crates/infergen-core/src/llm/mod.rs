//! LLM refinement pass (E6.1).
//!
//! Optional post-scan pass that sends low-confidence `Proposed` events to a
//! language model for better names, descriptions, and property types.
//! Fully optional — absent or `enabled: false` config means zero overhead.
//!
//! Supported backends: Ollama (local), Anthropic Claude, OpenAI (+ compatible).

pub mod config;
pub mod prompt;
pub mod provider;
pub mod refine;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors from the LLM refinement pass.
///
/// All variants are soft failures — callers log and continue the scan.
#[derive(Debug, Error)]
pub enum LlmError {
    /// HTTP request to the LLM API failed (connect refused, timeout, 4xx/5xx).
    #[error("LLM HTTP error: {0}")]
    Http(String),

    /// The LLM response could not be parsed as the expected JSON structure.
    #[error("LLM response parse error: {0}")]
    Parse(String),

    /// The LLM returned a response with no usable content.
    #[error("LLM returned an empty response")]
    Empty,

    /// A required API key was not found in config or environment.
    #[error("missing API key for {provider} (set config.llm.apiKey or env var)")]
    MissingApiKey {
        /// Name of the provider that requires an API key.
        provider: &'static str,
    },

    /// Provider requires `baseUrl` but none was configured.
    #[error("missing baseUrl for OpenAI-compatible provider")]
    MissingBaseUrl,
}

// ---------------------------------------------------------------------------
// Backend trait
// ---------------------------------------------------------------------------

/// Implemented by each LLM backend (Ollama, Anthropic, OpenAI, …).
pub trait LlmBackend {
    /// Send a batch of low-confidence events and return suggested refinements.
    fn refine_batch(&self, request: &RefinementRequest) -> Result<RefinementResponse, LlmError>;
}

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// Batch of events sent to the LLM for refinement.
#[derive(Debug, Serialize)]
pub struct RefinementRequest {
    /// Events to refine.
    pub events: Vec<EventInput>,
    /// Human-readable project context, e.g. `"Next.js TypeScript project"`.
    pub project_context: String,
}

/// Single event as presented to the LLM.
#[derive(Debug, Serialize)]
pub struct EventInput {
    /// Stable catalog ID (`evt_{hex}`).
    pub id: String,
    /// Current heuristic name.
    pub name: String,
    /// Event kind as lowercase string, e.g. `"pageView"`.
    pub kind: String,
    /// Heuristic confidence `0.0`–`1.0`.
    pub confidence: f64,
    /// Source file paths that triggered this event.
    pub source_paths: Vec<String>,
    /// Current description (often empty).
    pub description: String,
    /// Current properties.
    pub properties: Vec<PropertyInput>,
}

/// Property as presented to the LLM.
#[derive(Debug, Serialize)]
pub struct PropertyInput {
    /// Property name.
    pub name: String,
    /// Inferred type, if known.
    pub prop_type: Option<String>,
    /// Whether this property likely contains PII.
    pub pii: bool,
}

/// LLM suggestions for a batch of events.
#[derive(Debug, Clone, Deserialize)]
pub struct RefinementResponse {
    /// Per-event suggestions (may be a subset of the request if LLM omits some).
    pub events: Vec<EventOutput>,
}

/// LLM suggestions for one event.
#[derive(Debug, Clone, Deserialize)]
pub struct EventOutput {
    /// Must match the original `EventInput.id`.
    pub id: String,
    /// Suggested name.  `None` or empty = keep current.
    #[serde(default)]
    pub name: Option<String>,
    /// Suggested description.  `None` or empty = keep current.
    #[serde(default)]
    pub description: Option<String>,
    /// Suggested property type updates.
    #[serde(default)]
    pub properties: Vec<PropertyOutput>,
}

/// LLM suggestion for one property.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PropertyOutput {
    /// Must match the original `PropertyInput.name`.
    pub name: String,
    /// Suggested type.  `None` = leave unchanged.
    #[serde(default)]
    pub prop_type: Option<String>,
}

// ---------------------------------------------------------------------------
// JSON extraction helper
// ---------------------------------------------------------------------------

/// Extract the first JSON object from LLM text that may contain markdown fences.
///
/// Handles:
/// - ` ```json ... ``` ` code fences
/// - ` ``` ... ``` ` plain fences
/// - Explanatory text before/after the JSON
///
/// Returns a sub-slice pointing to the first `{` through the last `}`.
/// If no braces are found, returns the original trimmed text.
#[must_use]
pub fn extract_json(text: &str) -> &str {
    // Strip markdown code fences if present.
    let text = text.trim();
    let text = if text.starts_with("```") {
        // Skip the opening fence line, strip closing fence.
        let after_open = text.find('\n').map(|i| &text[i + 1..]).unwrap_or(text);
        if let Some(close) = after_open.rfind("```") {
            after_open[..close].trim()
        } else {
            after_open.trim()
        }
    } else {
        text
    };

    // Find the outermost JSON object boundaries.
    let start = text.find('{');
    let end = text.rfind('}');
    match (start, end) {
        (Some(s), Some(e)) if e >= s => &text[s..=e],
        _ => text,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_plain_json() {
        let input = r#"{"events": []}"#;
        assert_eq!(extract_json(input), r#"{"events": []}"#);
    }

    #[test]
    fn extract_json_with_markdown_fence() {
        let input = "```\n{\"events\": []}\n```";
        assert_eq!(extract_json(input), r#"{"events": []}"#);
    }

    #[test]
    fn extract_json_with_json_fence() {
        let input = "```json\n{\"events\": []}\n```";
        assert_eq!(extract_json(input), r#"{"events": []}"#);
    }

    #[test]
    fn extract_json_with_leading_text() {
        let input = "Here is the result:\n{\"events\": []}";
        assert_eq!(extract_json(input), r#"{"events": []}"#);
    }

    #[test]
    fn extract_json_with_trailing_text() {
        let input = "{\"events\": []}\nHope that helps!";
        assert_eq!(extract_json(input), r#"{"events": []}"#);
    }

    #[test]
    fn extract_json_returns_original_when_no_braces() {
        let input = "no json here";
        assert_eq!(extract_json(input), "no json here");
    }

    #[test]
    fn extract_json_nested_objects() {
        let input = "{\"a\": {\"b\": 1}}";
        assert_eq!(extract_json(input), "{\"a\": {\"b\": 1}}");
    }
}
