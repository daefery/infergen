//! Echo framework adapter for Go.
//!
//! Detects route registrations of the form `.GET("/path", handler)` in Go
//! source files using line-level pattern matching.  Echo uses the same
//! calling convention as Gin, so the extraction logic is mirrored.

use std::path::PathBuf;

use crate::{
    detect::{Framework, Language},
    namer::{NameSignals, Namer},
    parser::ParsedFile,
    property,
};

use super::{Adapter, EventKind, ProposedEvent};

/// HTTP methods Echo exposes on an engine or group.
const ECHO_METHODS: &[&str] = &[
    "GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD", "Any",
];

/// Echo framework adapter.
///
/// `project_root` is the directory that contains `go.mod`.
pub struct EchoAdapter {
    /// Absolute path to the Go project root.
    pub project_root: PathBuf,
}

impl EchoAdapter {
    /// Create a new adapter rooted at `project_root`.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Text-based extraction helpers
// ---------------------------------------------------------------------------

fn extract_first_quoted(s: &str) -> Option<String> {
    for delim in ['"', '`'] {
        if let Some(open) = s.find(delim) {
            let inner = &s[open + 1..];
            if let Some(close) = inner.find(delim) {
                return Some(inner[..close].to_owned());
            }
        }
    }
    None
}

fn extract_handler_name(after_paren: &str) -> String {
    let comma = match after_paren.find(',') {
        Some(i) => i,
        None => return String::new(),
    };
    let rest = after_paren[comma + 1..].trim();
    let end = rest
        .find(|c: char| c == ')' || c == ',' || c == '{')
        .unwrap_or(rest.len());
    let raw = rest[..end].trim();
    raw.rsplit('.').next().unwrap_or(raw).trim().to_owned()
}

fn extract_echo_routes(source: &str) -> Vec<(String, String, String)> {
    let mut routes = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        for method in ECHO_METHODS {
            let needle = format!(".{}(", method);
            if let Some(pos) = trimmed.find(needle.as_str()) {
                let after = &trimmed[pos + needle.len()..];
                if let Some(path) = extract_first_quoted(after) {
                    let handler = extract_handler_name(after);
                    routes.push((method.to_lowercase(), path, handler));
                }
            }
        }
    }
    routes
}

// ---------------------------------------------------------------------------
// Adapter implementation
// ---------------------------------------------------------------------------

impl Adapter for EchoAdapter {
    fn analyze(&self, file: &ParsedFile) -> Vec<ProposedEvent> {
        if file.lang != Language::Go {
            return Vec::new();
        }

        let namer = Namer::new();
        let mut events = Vec::new();

        let routes = file
            .with_go_source(|src| extract_echo_routes(src))
            .unwrap_or_default();

        for (method, path, handler) in routes {
            let handler_ref: Option<&str> = if handler.is_empty() {
                None
            } else {
                Some(&handler)
            };

            let result = namer.derive(&NameSignals {
                route: Some(&path),
                handler_name: handler_ref,
                kind: EventKind::ApiCall,
                component_name: None,
            });

            let _ = method;

            let mut event =
                ProposedEvent::new(result.name, EventKind::ApiCall, &file.path, 0.75)
                    .with_prop("endpoint", Some("string"))
                    .with_prop("method", Some("string"))
                    .with_adapter("echo");
            event.properties = property::enrich_hints(event.properties);
            events.push(event);
        }

        events
    }

    fn framework(&self) -> Framework {
        Framework::Echo
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{GoParser, parser::LanguageParser};

    fn adapter() -> EchoAdapter {
        EchoAdapter::new(PathBuf::from("/proj"))
    }

    fn parse(rel: &str, src: &str) -> ParsedFile {
        GoParser.parse(&PathBuf::from("/proj").join(rel), src).unwrap()
    }

    fn non_go_file() -> ParsedFile {
        use crate::JsParser;
        JsParser.parse(&PathBuf::from("/proj/app.ts"), "const x = 1;").unwrap()
    }

    #[test]
    fn echo_adapter_returns_empty_for_non_go_file() {
        assert!(adapter().analyze(&non_go_file()).is_empty());
    }

    #[test]
    fn echo_adapter_detects_get_route() {
        let file = parse("main.go", "e.GET(\"/items\", ListItems)\n");
        let events = adapter().analyze(&file);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::ApiCall);
        assert!(events[0].name.contains("items"), "name: {}", events[0].name);
    }

    #[test]
    fn echo_adapter_detects_post_route() {
        let file = parse("main.go", "e.POST(\"/items\", CreateItem)\n");
        let events = adapter().analyze(&file);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn echo_adapter_detects_dynamic_route() {
        let file = parse("main.go", "e.GET(\"/items/:id\", GetItem)\n");
        let events = adapter().analyze(&file);
        assert_eq!(events.len(), 1);
        assert!(events[0].name.contains("items"), "name: {}", events[0].name);
    }

    #[test]
    fn echo_adapter_no_routes_no_events() {
        let file = parse("main.go", "package main\n\nfunc main() {}\n");
        assert!(adapter().analyze(&file).is_empty());
    }

    #[test]
    fn echo_adapter_all_events_carry_echo_attribution() {
        let file = parse("main.go", "e.GET(\"/ping\", Ping)\ne.POST(\"/login\", Login)\n");
        for event in adapter().analyze(&file) {
            assert_eq!(event.adapter, "echo", "event {} missing echo attribution", event.name);
        }
    }

    #[test]
    fn echo_framework_returns_echo() {
        assert_eq!(adapter().framework(), Framework::Echo);
    }

    #[test]
    fn echo_adapter_package_qualified_handler() {
        let file = parse("main.go", "e.GET(\"/users\", handlers.ListUsers)\n");
        let events = adapter().analyze(&file);
        assert_eq!(events.len(), 1);
        assert!(events[0].name.contains("users"), "name: {}", events[0].name);
    }
}
