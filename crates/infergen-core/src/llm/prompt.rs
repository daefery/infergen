//! LLM prompt builder (E6.1).
//!
//! Produces a single text prompt suitable for any backend.
//! The prompt instructs the model to return a JSON object only — no prose.

use crate::llm::RefinementRequest;

/// Build the refinement prompt from `request`.
///
/// The prompt asks the LLM to review each event and return a JSON object
/// with suggested name, description, and property type improvements.
/// It explicitly forbids markdown and explanatory text so `extract_json`
/// can parse the response directly.
#[must_use]
pub fn build_prompt(request: &RefinementRequest) -> String {
    let events_json = serde_json::to_string_pretty(&request.events)
        .unwrap_or_else(|_| "[]".to_owned());

    format!(
        r#"You are an analytics event naming expert. Review the proposed analytics events below from a {ctx} project.

For each event suggest improvements to:
1. Event name — snake_case, format: entity_action (e.g. "user_signed_up", "checkout_completed").
   Only suggest a name if it is meaningfully better; set null to keep current.
2. Description — 5-10 words, present tense (e.g. "User completed checkout flow").
   Set null if description already exists.
3. Property types — fill "string", "number", or "boolean" where type is null.
   Leave unchanged if type is already known.

IMPORTANT: Return ONLY a raw JSON object — no markdown, no explanation, no code fences.

Output schema:
{{
  "events": [
    {{
      "id": "<same id from input>",
      "name": "<improved_name or null>",
      "description": "<concise description or null>",
      "properties": [
        {{"name": "<prop_name>", "prop_type": "<string|number|boolean|null>"}}
      ]
    }}
  ]
}}

Events to review:
{events_json}"#,
        ctx = request.project_context,
        events_json = events_json,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{EventInput, PropertyInput, RefinementRequest};

    fn make_request() -> RefinementRequest {
        RefinementRequest {
            project_context: "Next.js TypeScript".to_owned(),
            events: vec![EventInput {
                id: "evt_abc123".to_owned(),
                name: "page_page_viewed".to_owned(),
                kind: "pageView".to_owned(),
                confidence: 0.6,
                source_paths: vec!["src/pages/index.tsx".to_owned()],
                description: String::new(),
                properties: vec![PropertyInput {
                    name: "user_id".to_owned(),
                    prop_type: None,
                    pii: false,
                }],
            }],
        }
    }

    #[test]
    fn prompt_contains_event_id() {
        let p = build_prompt(&make_request());
        assert!(p.contains("evt_abc123"), "event id missing from prompt");
    }

    #[test]
    fn prompt_contains_event_name() {
        let p = build_prompt(&make_request());
        assert!(p.contains("page_page_viewed"), "event name missing from prompt");
    }

    #[test]
    fn prompt_contains_project_context() {
        let p = build_prompt(&make_request());
        assert!(p.contains("Next.js TypeScript"), "project context missing");
    }

    #[test]
    fn prompt_contains_json_schema_example() {
        let p = build_prompt(&make_request());
        assert!(p.contains("\"events\""), "JSON schema not in prompt");
    }

    #[test]
    fn prompt_is_deterministic() {
        let req = make_request();
        assert_eq!(build_prompt(&req), build_prompt(&req));
    }
}
