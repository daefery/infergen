//! FastAPI framework adapter.
//!
//! Detection strategy:
//!
//! 1. **Path-based** — always runs.  Files under `routers/`, `api/`,
//!    `endpoints/` get a confidence boost; other `.py` files are still scanned.
//! 2. **AST-based** — runs via [`ParsedFile::with_py_ast`].  Detects HTTP
//!    route decorators (`@app.get`, `@router.post`, …) and auth imports.
//!
//! All proposals carry `adapter = "fastapi"`.

use std::path::{Path, PathBuf};

use crate::{
    detect::Framework,
    namer::{NameSignals, Namer},
    parser::{ParsedFile, py::PyStmt},
    property,
};

use super::{Adapter, EventKind, ProposedEvent};

/// HTTP methods that FastAPI route decorators can carry.
const HTTP_METHODS: &[&str] = &[
    "get", "post", "put", "delete", "patch", "head", "options", "trace",
];

/// Import names that indicate OAuth2 / auth usage in FastAPI.
const AUTH_IMPORTS: &[&str] = &[
    "OAuth2PasswordBearer",
    "OAuth2PasswordRequestForm",
    "HTTPBearer",
    "HTTPBasic",
    "Security",
    "get_current_user",
    "get_current_active_user",
];

/// FastAPI framework adapter.
pub struct FastApiAdapter {
    /// Absolute path to the Python project root.
    pub project_root: PathBuf,
}

impl FastApiAdapter {
    /// Create a new adapter rooted at `project_root`.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self { project_root: project_root.into() }
    }
}

// ---------------------------------------------------------------------------
// Decorator parsing helpers
// ---------------------------------------------------------------------------

/// Parse a FastAPI / APIRouter route decorator text into `(http_method, path)`.
///
/// Accepts both `app.get("/items")` and `router.post("/users/{id}")`.
/// Returns `None` when the text does not match a route decorator.
fn parse_route_decorator(text: &str) -> Option<(String, String)> {
    // Pattern: `<obj>.<method>("<path>"…)` or `<method>("<path>"…)` (bare router)
    let dot = text.find('.')?;
    let after_dot = &text[dot + 1..];
    let paren = after_dot.find('(')?;
    let method_str = after_dot[..paren].trim().to_ascii_lowercase();
    if !HTTP_METHODS.contains(&method_str.as_str()) {
        return None;
    }
    // Extract path string from first arg.
    let args_str = &after_dot[paren + 1..];
    let path = extract_first_string_arg(args_str)?;
    Some((method_str, path))
}

/// Extract the first string-literal argument from a decorator argument list.
///
/// Supports both `"..."` and `'...'` string literals.
fn extract_first_string_arg(args_str: &str) -> Option<String> {
    for quote in ['"', '\''] {
        if let Some(start) = args_str.find(quote) {
            let after = &args_str[start + 1..];
            if let Some(end) = after.find(quote) {
                return Some(after[..end].to_owned());
            }
        }
    }
    None
}

/// Derive a route string suitable for the namer from a FastAPI path template.
///
/// FastAPI uses `{param}` syntax; we normalise to `[param]` to match the
/// route naming convention used across adapters.
fn normalise_path(path: &str) -> String {
    path.replace('{', "[").replace('}', "]")
}

/// `true` if `path` ends with a known Python/FastAPI directory name.
fn in_api_dir(rel: &Path) -> bool {
    rel.components().any(|c| {
        matches!(
            c.as_os_str().to_str().unwrap_or(""),
            "routers" | "router" | "api" | "endpoints" | "endpoint" | "routes" | "route"
        )
    })
}

// ---------------------------------------------------------------------------
// Adapter impl
// ---------------------------------------------------------------------------

impl Adapter for FastApiAdapter {
    fn analyze(&self, file: &ParsedFile) -> Vec<ProposedEvent> {
        if file.lang != crate::detect::Language::Python {
            return Vec::new();
        }

        let mut events: Vec<ProposedEvent> = Vec::new();
        let namer = Namer::new();

        // Compute relative path for confidence boosting.
        let rel = file.path.strip_prefix(&self.project_root).unwrap_or(&file.path);
        let in_api = in_api_dir(rel);
        let base_confidence: f32 = if in_api { 0.9 } else { 0.85 };

        let source_path = file.path.clone();

        let ast_events: Vec<ProposedEvent> = file.with_py_ast(|stmts| {
            let mut result = Vec::new();
            detect_routes(stmts, &source_path, &namer, base_confidence, &mut result);
            detect_auth(stmts, &source_path, &namer, &mut result);
            result
        });
        events.extend(ast_events);

        // Enrich properties.
        for event in &mut events {
            let props = std::mem::take(&mut event.properties);
            event.properties = property::enrich_hints(props);
            event.adapter = "fastapi".to_owned();
        }

        events
    }

    fn framework(&self) -> Framework {
        Framework::FastApi
    }
}

// ---------------------------------------------------------------------------
// Detection helpers
// ---------------------------------------------------------------------------

fn detect_routes(
    stmts: &[PyStmt],
    source_path: &Path,
    namer: &Namer,
    base_confidence: f32,
    out: &mut Vec<ProposedEvent>,
) {
    for stmt in stmts {
        let PyStmt::FunctionDef { decorators, .. } = stmt else { continue };

        for dec_text in decorators {
            let Some((method, raw_path)) = parse_route_decorator(dec_text) else {
                continue;
            };
            let route = normalise_path(&raw_path);
            let result = namer.derive(&NameSignals {
                route: Some(&route),
                handler_name: Some(&method),
                kind: EventKind::ApiCall,
                component_name: None,
            });
            let event = ProposedEvent::new(
                result.name,
                EventKind::ApiCall,
                source_path,
                base_confidence,
            )
            .with_prop("endpoint", Some("string"))
            .with_prop("method", Some("string"));
            out.push(event);
        }
    }
}

fn detect_auth(
    stmts: &[PyStmt],
    source_path: &Path,
    namer: &Namer,
    out: &mut Vec<ProposedEvent>,
) {
    let mut found_auth = false;
    for stmt in stmts {
        match stmt {
            PyStmt::ImportFrom { names, .. } | PyStmt::Import { names } => {
                if names.iter().any(|n| AUTH_IMPORTS.contains(&n.as_str())) {
                    found_auth = true;
                }
            }
            _ => {}
        }
    }

    if found_auth {
        // Propose a generic auth event — the catalog reviewer can refine.
        let result = namer.derive(&NameSignals {
            handler_name: Some("login"),
            kind: EventKind::AuthEvent,
            route: None,
            component_name: None,
        });
        out.push(
            ProposedEvent::new(result.name, EventKind::AuthEvent, source_path, 0.8)
                .with_prop("method", Some("string")),
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{parser::LanguageParser, parser::py::PyParser};

    fn adapter(root: &str) -> FastApiAdapter {
        FastApiAdapter::new(PathBuf::from(root))
    }

    fn parse(root: &str, rel_path: &str, source: &str) -> ParsedFile {
        let path = PathBuf::from(root).join(rel_path);
        PyParser.parse(&path, source).unwrap()
    }

    // -----------------------------------------------------------------------

    #[test]
    fn non_python_file_returns_empty() {
        let a = adapter("/proj");
        let file = crate::JsParser
            .parse(&PathBuf::from("/proj/app.ts"), "const x = 1;")
            .unwrap();
        assert!(a.analyze(&file).is_empty());
    }

    #[test]
    fn detects_app_get_route() {
        let a = adapter("/proj");
        let src = r#"
@app.get("/items")
async def list_items():
    pass
"#;
        let file = parse("/proj", "main.py", src);
        let events = a.analyze(&file);
        let api: Vec<_> = events.iter().filter(|e| e.kind == EventKind::ApiCall).collect();
        assert_eq!(api.len(), 1, "expected one ApiCall event");
        assert!(api[0].name.contains("items"), "name should contain route segment");
        assert!(api[0].name.contains("api_called") || api[0].name.contains("get"));
    }

    #[test]
    fn detects_router_post_route() {
        let a = adapter("/proj");
        let src = r#"
@router.post("/users")
async def create_user(user: UserCreate):
    pass
"#;
        let file = parse("/proj", "routers/users.py", src);
        let events = a.analyze(&file);
        let api: Vec<_> = events.iter().filter(|e| e.kind == EventKind::ApiCall).collect();
        assert!(!api.is_empty(), "expected ApiCall from @router.post");
        assert!(api[0].name.contains("users"));
    }

    #[test]
    fn detects_delete_route() {
        let a = adapter("/proj");
        let src = "@app.delete(\"/items/{item_id}\")\nasync def delete_item(item_id: int):\n    pass\n";
        let file = parse("/proj", "api/items.py", src);
        let events = a.analyze(&file);
        assert!(events.iter().any(|e| e.kind == EventKind::ApiCall));
    }

    #[test]
    fn confidence_is_high_in_api_dir() {
        let a = adapter("/proj");
        let src = "@app.get(\"/ping\")\nasync def ping():\n    pass\n";
        let file = parse("/proj", "api/health.py", src);
        let events = a.analyze(&file);
        let api: Vec<_> = events.iter().filter(|e| e.kind == EventKind::ApiCall).collect();
        assert!(!api.is_empty());
        assert!(api[0].confidence >= 0.88, "confidence in api/ dir should be high");
    }

    #[test]
    fn detects_auth_import() {
        let a = adapter("/proj");
        let src = "from fastapi.security import OAuth2PasswordBearer\n";
        let file = parse("/proj", "auth.py", src);
        let events = a.analyze(&file);
        let auth: Vec<_> = events.iter().filter(|e| e.kind == EventKind::AuthEvent).collect();
        assert!(!auth.is_empty(), "OAuth2PasswordBearer import should propose auth event");
    }

    #[test]
    fn empty_file_returns_empty() {
        let a = adapter("/proj");
        let file = parse("/proj", "empty.py", "");
        assert!(a.analyze(&file).is_empty());
    }

    #[test]
    fn no_decorator_returns_empty() {
        let a = adapter("/proj");
        let src = "def helper(x):\n    return x * 2\n";
        let file = parse("/proj", "utils.py", src);
        assert!(a.analyze(&file).is_empty());
    }

    #[test]
    fn all_proposals_carry_fastapi_attribution() {
        let a = adapter("/proj");
        let src = "@app.get(\"/items\")\nasync def list_items():\n    pass\n";
        let file = parse("/proj", "main.py", src);
        for event in a.analyze(&file) {
            assert_eq!(event.adapter, "fastapi");
        }
    }

    #[test]
    fn endpoint_and_method_props_present() {
        let a = adapter("/proj");
        let src = "@app.get(\"/items\")\nasync def list_items():\n    pass\n";
        let file = parse("/proj", "main.py", src);
        let events = a.analyze(&file);
        let api = events.iter().find(|e| e.kind == EventKind::ApiCall).unwrap();
        assert!(api.properties.iter().any(|p| p.name == "endpoint"), "endpoint prop missing");
        assert!(api.properties.iter().any(|p| p.name == "method"), "method prop missing");
    }
}
