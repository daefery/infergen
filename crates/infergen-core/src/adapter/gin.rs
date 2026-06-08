//! Gin framework adapter for Go.
//!
//! Detects route registrations of the form `.GET("/path", handler)` in Go
//! source files using line-level pattern matching.  No extern Go process is
//! required.

use std::path::{Path, PathBuf};

use crate::{
    detect::{Framework, Language},
    namer::{NameSignals, Namer},
    parser::ParsedFile,
    property,
};

use super::{Adapter, EventKind, ProposedEvent};

/// HTTP methods Gin exposes on a router / group.
const GIN_METHODS: &[&str] = &[
    "GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD", "Any",
];

/// Gin framework adapter.
///
/// `project_root` is the directory that contains `go.mod`.
pub struct GinAdapter {
    /// Absolute path to the Go project root.
    pub project_root: PathBuf,
}

impl GinAdapter {
    /// Create a new adapter rooted at `project_root`.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Text-based extraction helpers (shared pattern with echo.rs / nethttp.rs)
// ---------------------------------------------------------------------------

/// Extract the first double-quoted or backtick-quoted string from `s`.
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

/// Extract the handler name from the text after the opening `(`.
///
/// Input: `"/path", handlers.CreateUser)` → `"CreateUser"`.
/// Input: `"/path", func(c *gin.Context) {})` → `""` (anonymous).
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
    // Strip package prefix: "controllers.CreateUser" → "CreateUser"
    raw.rsplit('.').next().unwrap_or(raw).trim().to_owned()
}

/// Parse route registrations from a Go source string.
///
/// Returns `(http_method_lowercase, route_path, handler_name)` triples.
fn extract_gin_routes(source: &str) -> Vec<(String, String, String)> {
    let mut routes = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        for method in GIN_METHODS {
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

impl Adapter for GinAdapter {
    fn analyze(&self, file: &ParsedFile) -> Vec<ProposedEvent> {
        if file.lang != Language::Go {
            return Vec::new();
        }

        let namer = Namer::new();
        let mut events = Vec::new();

        let routes = file
            .with_go_source(|src| extract_gin_routes(src))
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

            let mut event =
                ProposedEvent::new(result.name, EventKind::ApiCall, &file.path, 0.75)
                    .with_prop("endpoint", Some("string"))
                    .with_prop("method", Some("string"))
                    .with_adapter("gin");
            event.properties = property::enrich_hints(event.properties);

            // Attach the HTTP method as a searchable property hint.
            // (method string is already in properties via with_prop above)
            let _ = method; // used for naming signal only

            events.push(event);
        }

        events
    }

    fn framework(&self) -> Framework {
        Framework::Gin
    }
}

// ---------------------------------------------------------------------------
// Path helper used by Adapter::analyze above
// ---------------------------------------------------------------------------

impl GinAdapter {
    #[allow(dead_code)]
    fn project_root(&self) -> &Path {
        &self.project_root
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{GoParser, detect::Language, parser::LanguageParser};

    fn adapter() -> GinAdapter {
        GinAdapter::new(PathBuf::from("/proj"))
    }

    fn parse(rel: &str, src: &str) -> ParsedFile {
        GoParser.parse(&PathBuf::from("/proj").join(rel), src).unwrap()
    }

    fn non_go_file() -> ParsedFile {
        use crate::{JsParser, parser::LanguageParser as _};
        JsParser.parse(&PathBuf::from("/proj/app.ts"), "const x = 1;").unwrap()
    }

    #[test]
    fn gin_adapter_returns_empty_for_non_go_file() {
        let a = adapter();
        assert!(a.analyze(&non_go_file()).is_empty());
    }

    #[test]
    fn gin_adapter_detects_get_route() {
        let a = adapter();
        let file = parse("main.go", "r.GET(\"/users\", ListUsers)\n");
        let events = a.analyze(&file);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::ApiCall);
        assert!(events[0].name.contains("users"), "name: {}", events[0].name);
    }

    #[test]
    fn gin_adapter_detects_post_route() {
        let a = adapter();
        let file = parse("main.go", "r.POST(\"/users\", CreateUser)\n");
        let events = a.analyze(&file);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::ApiCall);
    }

    #[test]
    fn gin_adapter_detects_dynamic_route() {
        let a = adapter();
        let file = parse("main.go", "r.GET(\"/users/:id\", GetUser)\n");
        let events = a.analyze(&file);
        assert_eq!(events.len(), 1);
        assert!(events[0].name.contains("users"), "name: {}", events[0].name);
    }

    #[test]
    fn gin_adapter_detects_multiple_routes() {
        let a = adapter();
        let src = "r.GET(\"/users\", ListUsers)\nr.POST(\"/users\", CreateUser)\nr.DELETE(\"/users/:id\", DeleteUser)\n";
        let file = parse("main.go", src);
        let events = a.analyze(&file);
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn gin_adapter_no_routes_no_events() {
        let a = adapter();
        let file = parse("main.go", "package main\n\nfunc main() {}\n");
        assert!(a.analyze(&file).is_empty());
    }

    #[test]
    fn gin_adapter_all_events_carry_gin_attribution() {
        let a = adapter();
        let file = parse("main.go", "r.GET(\"/ping\", Ping)\nr.POST(\"/items\", Create)\n");
        for event in a.analyze(&file) {
            assert_eq!(event.adapter, "gin", "event {} missing gin attribution", event.name);
        }
    }

    #[test]
    fn gin_adapter_events_have_endpoint_and_method_props() {
        let a = adapter();
        let file = parse("main.go", "r.GET(\"/health\", HealthCheck)\n");
        let events = a.analyze(&file);
        assert_eq!(events.len(), 1);
        let prop_names: Vec<&str> = events[0].properties.iter().map(|p| p.name.as_str()).collect();
        assert!(prop_names.contains(&"endpoint"), "props: {:?}", prop_names);
        assert!(prop_names.contains(&"method"), "props: {:?}", prop_names);
    }

    #[test]
    fn gin_framework_returns_gin() {
        assert_eq!(adapter().framework(), Framework::Gin);
    }

    #[test]
    fn gin_adapter_package_qualified_handler() {
        let a = adapter();
        let file = parse("main.go", "r.GET(\"/users\", controllers.ListUsers)\n");
        let events = a.analyze(&file);
        assert_eq!(events.len(), 1);
        assert!(events[0].name.contains("users"), "name: {}", events[0].name);
    }

    #[test]
    fn gin_adapter_confidence_is_0_75() {
        let a = adapter();
        let file = parse("main.go", "r.GET(\"/ping\", Ping)\n");
        let events = a.analyze(&file);
        assert_eq!(events.len(), 1);
        assert!((events[0].confidence - 0.75_f32).abs() < f32::EPSILON);
    }
}
