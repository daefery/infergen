//! net/http adapter for Go.
//!
//! Detects two patterns:
//! 1. Explicit registrations — `http.HandleFunc("/path", handler)` or
//!    `mux.HandleFunc("/path", handler)`.
//! 2. Handler function signatures — `func Foo(w http.ResponseWriter,
//!    r *http.Request)` at the top level.
//!
//! Pattern 2 uses a lower confidence (0.55) because the function may never
//! be registered as a handler.  When the same handler appears in both passes,
//! only one event is emitted (dedup by handler name).

use std::path::PathBuf;

use crate::{
    detect::{Framework, Language},
    namer::{NameSignals, Namer},
    parser::ParsedFile,
    property,
};

use super::{Adapter, EventKind, ProposedEvent};

/// net/http adapter.
///
/// `project_root` is the directory that contains `go.mod`.
pub struct NetHttpAdapter {
    /// Absolute path to the Go project root.
    pub project_root: PathBuf,
}

impl NetHttpAdapter {
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

fn extract_second_arg(after_paren: &str) -> String {
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

/// Pass 1 — extract `http.HandleFunc` / `mux.HandleFunc` registrations.
fn extract_handlefuncs(source: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    for line in source.lines() {
        let t = line.trim();
        for needle in &["http.HandleFunc(", "http.Handle(", ".HandleFunc(", ".Handle("] {
            if let Some(pos) = t.find(needle) {
                let after = &t[pos + needle.len()..];
                if let Some(path) = extract_first_quoted(after) {
                    // Skip paths that look like package.Method patterns (false match on
                    // e.g. `mux.Handle` where the second segment contains ".Handle(").
                    if path.starts_with('/') || path.is_empty() {
                        let handler = extract_second_arg(after);
                        results.push((path, handler));
                    }
                }
                break; // only match one needle per line
            }
        }
    }
    results
}

/// Pass 2 — extract top-level `func Name(w http.ResponseWriter, r *http.Request)`.
fn extract_handler_funcs(source: &str) -> Vec<String> {
    let mut funcs = Vec::new();
    for line in source.lines() {
        let t = line.trim();
        if t.starts_with("func ") && t.contains("http.ResponseWriter") {
            if let Some(name) = extract_func_name(t) {
                // Exclude the generic `ServeHTTP` method name.
                if name != "ServeHTTP" && !name.is_empty() {
                    funcs.push(name);
                }
            }
        }
    }
    funcs
}

fn extract_func_name(line: &str) -> Option<String> {
    let after_func = line.strip_prefix("func ")?.trim();
    if after_func.starts_with('(') {
        // Method: `(r *Recv) MethodName(`
        let close = after_func.find(')')?;
        let rest = after_func[close + 1..].trim();
        let end = rest.find('(')?;
        let name = rest[..end].trim().to_owned();
        if name.is_empty() { None } else { Some(name) }
    } else {
        let end = after_func.find('(')?;
        let name = after_func[..end].trim().to_owned();
        if name.is_empty() { None } else { Some(name) }
    }
}

// ---------------------------------------------------------------------------
// Adapter implementation
// ---------------------------------------------------------------------------

impl Adapter for NetHttpAdapter {
    fn analyze(&self, file: &ParsedFile) -> Vec<ProposedEvent> {
        if file.lang != Language::Go {
            return Vec::new();
        }

        let namer = Namer::new();
        let mut events: Vec<ProposedEvent> = Vec::new();

        let (registrations, handler_funcs) = file
            .with_go_source(|src| (extract_handlefuncs(src), extract_handler_funcs(src)))
            .unwrap_or_default();

        // Pass 1 — registered routes (higher confidence).
        let mut registered_handlers: Vec<String> = Vec::new();
        for (path, handler) in registrations {
            let handler_ref: Option<&str> = if handler.is_empty() {
                None
            } else {
                registered_handlers.push(handler.clone());
                Some(&handler)
            };

            let result = namer.derive(&NameSignals {
                route: Some(&path),
                handler_name: handler_ref,
                kind: EventKind::ApiCall,
                component_name: None,
            });

            let mut event =
                ProposedEvent::new(result.name, EventKind::ApiCall, &file.path, 0.80)
                    .with_prop("endpoint", Some("string"))
                    .with_prop("method", Some("string"))
                    .with_adapter("net-http");
            event.properties = property::enrich_hints(event.properties);
            events.push(event);
        }

        // Pass 2 — handler function signatures not already registered.
        for func_name in handler_funcs {
            if registered_handlers.contains(&func_name) {
                continue; // already emitted via Pass 1
            }

            let result = namer.derive(&NameSignals {
                route: None,
                handler_name: Some(&func_name),
                kind: EventKind::ApiCall,
                component_name: None,
            });

            let mut event =
                ProposedEvent::new(result.name, EventKind::ApiCall, &file.path, 0.55)
                    .with_prop("endpoint", Some("string"))
                    .with_prop("method", Some("string"))
                    .with_adapter("net-http");
            event.properties = property::enrich_hints(event.properties);
            events.push(event);
        }

        events
    }

    fn framework(&self) -> Framework {
        Framework::NetHttp
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{GoParser, parser::LanguageParser};

    fn adapter() -> NetHttpAdapter {
        NetHttpAdapter::new(PathBuf::from("/proj"))
    }

    fn parse(rel: &str, src: &str) -> ParsedFile {
        GoParser.parse(&PathBuf::from("/proj").join(rel), src).unwrap()
    }

    fn non_go_file() -> ParsedFile {
        use crate::JsParser;
        JsParser.parse(&PathBuf::from("/proj/app.ts"), "const x = 1;").unwrap()
    }

    #[test]
    fn nethttp_returns_empty_for_non_go_file() {
        assert!(adapter().analyze(&non_go_file()).is_empty());
    }

    #[test]
    fn nethttp_detects_http_handlefunc() {
        let file = parse(
            "main.go",
            "http.HandleFunc(\"/hello\", HelloHandler)\n",
        );
        let events = adapter().analyze(&file);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::ApiCall);
        assert!((events[0].confidence - 0.80_f32).abs() < f32::EPSILON);
    }

    #[test]
    fn nethttp_detects_mux_handlefunc() {
        let file = parse(
            "main.go",
            "mux.HandleFunc(\"/api\", ApiHandler)\n",
        );
        let events = adapter().analyze(&file);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn nethttp_detects_handler_func_signature() {
        let file = parse(
            "handlers.go",
            "func HealthHandler(w http.ResponseWriter, r *http.Request) {}\n",
        );
        let events = adapter().analyze(&file);
        assert_eq!(events.len(), 1);
        assert!((events[0].confidence - 0.55_f32).abs() < f32::EPSILON);
    }

    #[test]
    fn nethttp_no_double_emit_for_registered_handler() {
        let src = "http.HandleFunc(\"/health\", HealthHandler)\nfunc HealthHandler(w http.ResponseWriter, r *http.Request) {}\n";
        let file = parse("main.go", src);
        let events = adapter().analyze(&file);
        // Handler registered AND declared → only one event (from Pass 1).
        assert_eq!(events.len(), 1, "expected dedup; got {} events", events.len());
        assert!((events[0].confidence - 0.80_f32).abs() < f32::EPSILON);
    }

    #[test]
    fn nethttp_all_events_carry_net_http_attribution() {
        let file = parse("main.go", "http.HandleFunc(\"/\", Root)\n");
        for event in adapter().analyze(&file) {
            assert_eq!(event.adapter, "net-http");
        }
    }

    #[test]
    fn nethttp_framework_returns_net_http() {
        assert_eq!(adapter().framework(), Framework::NetHttp);
    }

    #[test]
    fn nethttp_multiple_registrations() {
        let src = "http.HandleFunc(\"/\", Root)\nhttp.HandleFunc(\"/api/health\", Health)\n";
        let file = parse("main.go", src);
        assert_eq!(adapter().analyze(&file).len(), 2);
    }

    #[test]
    fn nethttp_no_events_for_plain_go_file() {
        let file = parse("main.go", "package main\n\nfunc main() {}\n");
        assert!(adapter().analyze(&file).is_empty());
    }
}
