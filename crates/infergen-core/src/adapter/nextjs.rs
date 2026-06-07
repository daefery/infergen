//! Next.js framework adapter.
//!
//! Detection strategy:
//!
//! 1. **Path-based** — always runs. Recognises Pages Router (`pages/`)
//!    and App Router (`app/`) directory conventions.
//! 2. **AST-based** — runs when `file.with_js_program` succeeds (JS/TS only).
//!    Detects NextAuth imports and form handler function names.
//!
//! Event names are derived via the heuristic namer (E1.2). All proposals
//! carry `adapter = "nextjs"`.

use std::path::{Path, PathBuf};

use crate::{
    detect::Framework,
    namer::{NameSignals, Namer},
    parser::ParsedFile,
    property,
};

use super::{Adapter, EventKind, ProposedEvent};

/// HTTP methods that identify App Router route handlers.
const HTTP_METHODS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];

/// Function names that indicate a form submission handler.
const SUBMIT_HANDLER_NAMES: &[&str] = &[
    "handleSubmit",
    "onSubmit",
    "submitForm",
    "handleFormSubmit",
    "formSubmit",
    "submitHandler",
];

/// TS/JS file extensions recognised by the adapter.
const JS_EXTS: &[&str] = &["ts", "tsx", "js", "jsx"];

/// Next.js framework adapter.
///
/// `project_root` is the directory that contains `package.json` (i.e. the
/// Next.js project root). It is used to compute file paths relative to
/// `pages/` or `app/`.
pub struct NextjsAdapter {
    /// Absolute path to the Next.js project root.
    pub project_root: PathBuf,
}

impl NextjsAdapter {
    /// Create a new adapter rooted at `project_root`.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Path-based detection helpers
// ---------------------------------------------------------------------------

/// `true` if `path` has a JS/TS extension.
fn is_js_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| JS_EXTS.contains(&e))
        .unwrap_or(false)
}

/// Strip the JS/TS extension from `s` (last component only, no path sep).
///
/// `"index.tsx"` → `"index"`. Returns `s` unchanged if no known extension.
fn strip_js_ext(s: &str) -> &str {
    for ext in JS_EXTS {
        if let Some(stem) = s.strip_suffix(&format!(".{ext}")) {
            return stem;
        }
    }
    s
}

/// Derive the canonical Next.js route string from a Pages Router path.
///
/// `rel` is relative to the project root.
///
/// Returns `None` for API routes (`pages/api/`), Next.js internals
/// (`_app`, `_document`, `_error`, `_404`, `_500`), and non-JS files.
pub fn route_from_pages_path(rel: &Path) -> Option<String> {
    let mut components = rel.components().peekable();
    let first = components.next()?.as_os_str().to_str()?;
    if first != "pages" {
        return None;
    }
    let rest: Vec<&str> = components.filter_map(|c| c.as_os_str().to_str()).collect();

    if rest.is_empty() || !is_js_ext(Path::new(rest.last()?)) {
        return None;
    }
    if rest.first() == Some(&"api") {
        return None;
    }
    let last_stem = strip_js_ext(rest.last()?);
    if last_stem.starts_with('_') {
        return None;
    }

    let segments: Vec<&str> = rest
        .iter()
        .enumerate()
        .filter_map(|(i, seg)| {
            if i == rest.len() - 1 {
                let stem = strip_js_ext(seg);
                if stem == "index" { None } else { Some(stem) }
            } else {
                Some(*seg)
            }
        })
        .collect();

    if segments.is_empty() {
        Some("/".to_owned())
    } else {
        Some(format!("/{}", segments.join("/")))
    }
}

/// Derive the canonical route from an App Router path (page/layout files).
///
/// Returns `None` for `route.ts` files (API routes) and non-page/layout files.
pub fn route_from_app_path(rel: &Path) -> Option<String> {
    let mut components = rel.components().peekable();
    let first = components.next()?.as_os_str().to_str()?;
    if first != "app" {
        return None;
    }
    let rest: Vec<&str> = components.filter_map(|c| c.as_os_str().to_str()).collect();

    if rest.is_empty() {
        return None;
    }

    let filename = rest.last()?;
    let stem = strip_js_ext(filename);
    if !matches!(stem, "page" | "layout") {
        return None;
    }

    let segments: Vec<&str> = rest[..rest.len() - 1]
        .iter()
        .filter_map(|seg| {
            if seg.starts_with('(') && seg.ends_with(')') {
                None
            } else {
                Some(*seg)
            }
        })
        .collect();

    if segments.is_empty() {
        Some("/".to_owned())
    } else {
        Some(format!("/{}", segments.join("/")))
    }
}

/// Derive the API route from a Pages Router API path.
pub fn api_route_from_pages_path(rel: &Path) -> Option<String> {
    let mut components = rel.components();
    let first = components.next()?.as_os_str().to_str()?;
    if first != "pages" {
        return None;
    }
    let second = components.next()?.as_os_str().to_str()?;
    if second != "api" {
        return None;
    }
    let rest: Vec<&str> = components.filter_map(|c| c.as_os_str().to_str()).collect();

    if rest.is_empty() || !is_js_ext(Path::new(rest.last()?)) {
        return None;
    }

    let segments: Vec<&str> = rest
        .iter()
        .enumerate()
        .filter_map(|(i, seg)| {
            if i == rest.len() - 1 {
                let stem = strip_js_ext(seg);
                if stem == "index" { None } else { Some(stem) }
            } else {
                Some(*seg)
            }
        })
        .collect();

    Some(format!("/api/{}", segments.join("/")))
}

/// Derive the API route from an App Router `route.ts` path.
pub fn api_route_from_app_path(rel: &Path) -> Option<String> {
    let mut components = rel.components().peekable();
    let first = components.next()?.as_os_str().to_str()?;
    if first != "app" {
        return None;
    }
    let rest: Vec<&str> = components.filter_map(|c| c.as_os_str().to_str()).collect();

    let filename = rest.last()?;
    if strip_js_ext(filename) != "route" {
        return None;
    }

    let dir_segments: Vec<&str> = rest[..rest.len() - 1]
        .iter()
        .filter_map(|seg| {
            if seg.starts_with('(') && seg.ends_with(')') {
                None
            } else {
                Some(*seg)
            }
        })
        .collect();

    if dir_segments.is_empty() {
        Some("/".to_owned())
    } else {
        Some(format!("/{}", dir_segments.join("/")))
    }
}

// ---------------------------------------------------------------------------
// AST-based detection helpers
// ---------------------------------------------------------------------------

/// Collect auth events from a parsed OXC program.
///
/// Detects `import { signIn/signOut/signUp } from 'next-auth/react'`.
fn detect_auth_from_ast(
    prog: &oxc_ast::ast::Program<'_>,
    source_path: &Path,
    namer: &Namer,
) -> Vec<ProposedEvent> {
    use oxc_ast::ast::{ImportDeclarationSpecifier, ModuleExportName, Statement};

    let mut events = Vec::new();

    for stmt in &prog.body {
        let Statement::ImportDeclaration(import) = stmt else {
            continue;
        };
        let source = import.source.value.as_str();
        if !matches!(source, "next-auth/react" | "next-auth" | "@auth/nextjs") {
            continue;
        }
        let Some(specs) = &import.specifiers else {
            continue;
        };
        for spec in specs.iter() {
            let ImportDeclarationSpecifier::ImportSpecifier(named) = spec else {
                continue;
            };
            let imported_name = match &named.imported {
                ModuleExportName::IdentifierName(id) => id.name.as_str(),
                ModuleExportName::IdentifierReference(id) => id.name.as_str(),
                ModuleExportName::StringLiteral(s) => s.value.as_str(),
            };
            match imported_name {
                "signIn" | "login" => {
                    let result = namer.derive(&NameSignals {
                        handler_name: Some(imported_name),
                        kind: EventKind::AuthEvent,
                        route: None,
                        component_name: None,
                    });
                    events.push(
                        ProposedEvent::new(result.name, EventKind::AuthEvent, source_path, 0.85)
                            .with_prop("method", Some("string")),
                    );
                }
                "signOut" | "logout" => {
                    let result = namer.derive(&NameSignals {
                        handler_name: Some(imported_name),
                        kind: EventKind::AuthEvent,
                        route: None,
                        component_name: None,
                    });
                    events.push(ProposedEvent::new(
                        result.name,
                        EventKind::AuthEvent,
                        source_path,
                        0.85,
                    ));
                }
                "signUp" | "register" => {
                    let result = namer.derive(&NameSignals {
                        handler_name: Some(imported_name),
                        kind: EventKind::AuthEvent,
                        route: None,
                        component_name: None,
                    });
                    events.push(
                        ProposedEvent::new(result.name, EventKind::AuthEvent, source_path, 0.85)
                            .with_prop("method", Some("string")),
                    );
                }
                _ => {}
            }
        }
    }

    events
}

/// Collect form submit events from function / arrow-function names.
fn detect_forms_from_ast(
    prog: &oxc_ast::ast::Program<'_>,
    source_path: &Path,
    namer: &Namer,
) -> Vec<ProposedEvent> {
    use oxc_ast::ast::{BindingPattern, Expression, Statement};

    let mut events = Vec::new();
    let mut seen_names: Vec<String> = Vec::new();

    let is_submit_name = |name: &str| SUBMIT_HANDLER_NAMES.contains(&name);

    for stmt in &prog.body {
        match stmt {
            Statement::FunctionDeclaration(func) => {
                if let Some(id) = &func.id {
                    let name = id.name.as_str();
                    if is_submit_name(name) && !seen_names.contains(&name.to_owned()) {
                        seen_names.push(name.to_owned());
                        let result = namer.derive(&NameSignals {
                            handler_name: Some(name),
                            kind: EventKind::FormSubmit,
                            route: None,
                            component_name: None,
                        });
                        let param_hints = property::hints_from_params(&func.params);
                        let mut event = ProposedEvent::new(
                            result.name,
                            EventKind::FormSubmit,
                            source_path,
                            0.7,
                        );
                        event.properties = param_hints;
                        events.push(event);
                    }
                }
            }
            Statement::VariableDeclaration(decl) => {
                for declarator in &decl.declarations {
                    if let BindingPattern::BindingIdentifier(id) = &declarator.id {
                        let name = id.name.as_str();
                        if is_submit_name(name)
                            && !seen_names.contains(&name.to_owned())
                            && matches!(
                                &declarator.init,
                                Some(Expression::ArrowFunctionExpression(_))
                            )
                        {
                            seen_names.push(name.to_owned());
                            let result = namer.derive(&NameSignals {
                                handler_name: Some(name),
                                kind: EventKind::FormSubmit,
                                route: None,
                                component_name: None,
                            });
                            let param_hints =
                                if let Some(Expression::ArrowFunctionExpression(arrow)) =
                                    &declarator.init
                                {
                                    property::hints_from_params(&arrow.params)
                                } else {
                                    vec![]
                                };
                            let mut event = ProposedEvent::new(
                                result.name,
                                EventKind::FormSubmit,
                                source_path,
                                0.7,
                            );
                            event.properties = param_hints;
                            events.push(event);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    events
}

/// Collect API call events from exported HTTP method functions.
///
/// Detects `export async function GET/POST/…` — App Router convention.
fn detect_http_methods_from_ast(
    prog: &oxc_ast::ast::Program<'_>,
    source_path: &Path,
    route: &str,
    namer: &Namer,
) -> Vec<ProposedEvent> {
    use oxc_ast::ast::{Declaration, Statement};

    let mut events = Vec::new();

    for stmt in &prog.body {
        let Statement::ExportNamedDeclaration(export) = stmt else {
            continue;
        };
        let Some(Declaration::FunctionDeclaration(func)) = &export.declaration else {
            continue;
        };
        let Some(id) = &func.id else {
            continue;
        };
        let method = id.name.as_str();
        if !HTTP_METHODS.contains(&method) {
            continue;
        }

        let result = namer.derive(&NameSignals {
            route: Some(route),
            handler_name: Some(method),
            kind: EventKind::ApiCall,
            component_name: None,
        });
        events.push(
            ProposedEvent::new(result.name, EventKind::ApiCall, source_path, 0.95)
                .with_prop("endpoint", Some("string"))
                .with_prop("method", Some("string")),
        );
    }

    events
}

// ---------------------------------------------------------------------------
// Adapter implementation
// ---------------------------------------------------------------------------

impl Adapter for NextjsAdapter {
    fn analyze(&self, file: &ParsedFile) -> Vec<ProposedEvent> {
        let mut events = Vec::new();
        let namer = Namer::new();

        // --- Path-based detection ------------------------------------------
        if let Ok(rel) = file.path.strip_prefix(&self.project_root) {
            if let Some(route) = route_from_pages_path(rel) {
                let result = namer.derive(&NameSignals {
                    route: Some(&route),
                    kind: EventKind::PageView,
                    component_name: None,
                    handler_name: None,
                });
                events.push(
                    ProposedEvent::new(result.name, EventKind::PageView, &file.path, 0.9)
                        .with_prop("route", Some("string")),
                );
            } else if let Some(route) = route_from_app_path(rel) {
                let result = namer.derive(&NameSignals {
                    route: Some(&route),
                    kind: EventKind::PageView,
                    component_name: None,
                    handler_name: None,
                });
                events.push(
                    ProposedEvent::new(result.name, EventKind::PageView, &file.path, 0.9)
                        .with_prop("route", Some("string")),
                );
            } else if let Some(route) = api_route_from_pages_path(rel) {
                let result = namer.derive(&NameSignals {
                    route: Some(&route),
                    kind: EventKind::ApiCall,
                    component_name: None,
                    handler_name: None,
                });
                events.push(
                    ProposedEvent::new(result.name, EventKind::ApiCall, &file.path, 0.9)
                        .with_prop("endpoint", Some("string")),
                );
            } else if let Some(route) = api_route_from_app_path(rel) {
                let route_clone = route.clone();
                let path_clone = file.path.clone();
                let method_events = file
                    .with_js_program(|prog| {
                        detect_http_methods_from_ast(prog, &path_clone, &route_clone, &namer)
                    })
                    .unwrap_or_default();

                if method_events.is_empty() {
                    let result = namer.derive(&NameSignals {
                        route: Some(&route),
                        kind: EventKind::ApiCall,
                        component_name: None,
                        handler_name: None,
                    });
                    events.push(
                        ProposedEvent::new(result.name, EventKind::ApiCall, &file.path, 0.9)
                            .with_prop("endpoint", Some("string")),
                    );
                } else {
                    events.extend(method_events);
                }
            }
        }

        // --- AST-based detection (JS/TS files only) -------------------------
        let source_path = file.path.clone();
        let namer_ref = &namer;
        if let Some(ast_events) = file.with_js_program(|prog| {
            let mut v = Vec::new();
            v.extend(detect_auth_from_ast(prog, &source_path, namer_ref));

            let mut form_events = detect_forms_from_ast(prog, &source_path, namer_ref);
            // Merge JSX input field names into form events (dedup by name).
            if !form_events.is_empty() {
                let jsx_hints = property::hints_from_jsx_inputs(prog);
                if !jsx_hints.is_empty() {
                    for form_event in &mut form_events {
                        for hint in &jsx_hints {
                            if !form_event.properties.iter().any(|p| p.name == hint.name) {
                                form_event.properties.push(hint.clone());
                            }
                        }
                    }
                }
            }
            v.extend(form_events);
            v
        }) {
            events.extend(ast_events);
        }

        // Enrich all events' properties: fill missing types + set PII flags.
        for event in &mut events {
            let props = std::mem::take(&mut event.properties);
            event.properties = property::enrich_hints(props);
        }

        // Set adapter attribution on all proposals.
        for event in &mut events {
            event.adapter = "nextjs".to_owned();
        }

        events
    }

    fn framework(&self) -> Framework {
        Framework::NextJs
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{JsParser, namer::route_to_name_prefix, parser::LanguageParser};

    fn adapter(root: &str) -> NextjsAdapter {
        NextjsAdapter::new(PathBuf::from(root))
    }

    fn parse(root: &str, rel_path: &str, source: &str) -> ParsedFile {
        let path = PathBuf::from(root).join(rel_path);
        JsParser.parse(&path, source).unwrap()
    }

    // -----------------------------------------------------------------------
    // Path helpers
    // -----------------------------------------------------------------------

    #[test]
    fn route_from_pages_index() {
        assert_eq!(
            route_from_pages_path(Path::new("pages/index.tsx")),
            Some("/".to_owned())
        );
    }

    #[test]
    fn route_from_pages_about() {
        assert_eq!(
            route_from_pages_path(Path::new("pages/about.tsx")),
            Some("/about".to_owned())
        );
    }

    #[test]
    fn route_from_pages_nested() {
        assert_eq!(
            route_from_pages_path(Path::new("pages/blog/index.tsx")),
            Some("/blog".to_owned())
        );
    }

    #[test]
    fn route_from_pages_param() {
        assert_eq!(
            route_from_pages_path(Path::new("pages/blog/[slug].tsx")),
            Some("/blog/[slug]".to_owned())
        );
    }

    #[test]
    fn route_from_pages_api_is_none() {
        assert!(route_from_pages_path(Path::new("pages/api/users.ts")).is_none());
    }

    #[test]
    fn route_from_pages_internal_is_none() {
        assert!(route_from_pages_path(Path::new("pages/_app.tsx")).is_none());
        assert!(route_from_pages_path(Path::new("pages/_document.tsx")).is_none());
    }

    #[test]
    fn route_from_app_root() {
        assert_eq!(
            route_from_app_path(Path::new("app/page.tsx")),
            Some("/".to_owned())
        );
    }

    #[test]
    fn route_from_app_nested() {
        assert_eq!(
            route_from_app_path(Path::new("app/about/page.tsx")),
            Some("/about".to_owned())
        );
    }

    #[test]
    fn route_from_app_route_group_stripped() {
        assert_eq!(
            route_from_app_path(Path::new("app/(marketing)/features/page.tsx")),
            Some("/features".to_owned())
        );
    }

    #[test]
    fn route_from_app_route_is_none_for_route_file() {
        assert!(route_from_app_path(Path::new("app/api/users/route.ts")).is_none());
    }

    #[test]
    fn api_route_from_pages() {
        assert_eq!(
            api_route_from_pages_path(Path::new("pages/api/users.ts")),
            Some("/api/users".to_owned())
        );
    }

    #[test]
    fn api_route_from_app() {
        assert_eq!(
            api_route_from_app_path(Path::new("app/api/users/route.ts")),
            Some("/api/users".to_owned())
        );
    }

    #[test]
    fn route_to_name_prefix_slash() {
        assert_eq!(route_to_name_prefix("/"), "home");
    }

    #[test]
    fn route_to_name_prefix_nested() {
        assert_eq!(route_to_name_prefix("/blog/[slug]"), "blog_slug");
    }

    // -----------------------------------------------------------------------
    // Adapter::analyze — page detection
    // -----------------------------------------------------------------------

    #[test]
    fn analyzes_pages_router_page() {
        let a = adapter("/proj");
        let file = parse(
            "/proj",
            "pages/about.tsx",
            "export default function About() {}",
        );
        let events = a.analyze(&file);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::PageView);
        assert!(events[0].name.contains("page_viewed"));
        assert_eq!(events[0].confidence, 0.9_f32);
        assert!(events[0].properties.iter().any(|p| p.name == "route"));
    }

    #[test]
    fn analyzes_app_router_page() {
        let a = adapter("/proj");
        let file = parse(
            "/proj",
            "app/dashboard/page.tsx",
            "export default function Page() {}",
        );
        let events = a.analyze(&file);
        let pv: Vec<_> = events
            .iter()
            .filter(|e| e.kind == EventKind::PageView)
            .collect();
        assert!(!pv.is_empty());
        assert!(pv[0].name.contains("dashboard"));
    }

    #[test]
    fn analyzes_pages_router_api() {
        let a = adapter("/proj");
        let file = parse(
            "/proj",
            "pages/api/users.ts",
            "export default function handler(req, res) {}",
        );
        let events = a.analyze(&file);
        let api: Vec<_> = events
            .iter()
            .filter(|e| e.kind == EventKind::ApiCall)
            .collect();
        assert!(!api.is_empty());
        assert!(api[0].name.contains("api_called"));
    }

    #[test]
    fn analyzes_app_router_api_with_http_methods() {
        let a = adapter("/proj");
        let source = r#"
export async function GET(request: Request) { return Response.json({}); }
export async function POST(request: Request) { return Response.json({}); }
"#;
        let file = parse("/proj", "app/api/users/route.ts", source);
        let events = a.analyze(&file);
        let api: Vec<_> = events
            .iter()
            .filter(|e| e.kind == EventKind::ApiCall)
            .collect();
        assert_eq!(api.len(), 2);
        let names: Vec<&str> = api.iter().map(|e| e.name.as_str()).collect();
        assert!(names.iter().any(|n| n.contains("get")));
        assert!(names.iter().any(|n| n.contains("post")));
    }

    #[test]
    fn analyzes_app_router_api_fallback_no_methods() {
        let a = adapter("/proj");
        let file = parse("/proj", "app/api/items/route.ts", "// empty");
        let events = a.analyze(&file);
        let api: Vec<_> = events
            .iter()
            .filter(|e| e.kind == EventKind::ApiCall)
            .collect();
        assert_eq!(api.len(), 1);
        assert!(api[0].name.contains("api_called"));
    }

    // -----------------------------------------------------------------------
    // Adapter::analyze — auth detection
    // -----------------------------------------------------------------------

    #[test]
    fn detects_nextauth_sign_in() {
        let a = adapter("/proj");
        let source = "import { signIn } from 'next-auth/react';";
        let file = parse("/proj", "src/auth.ts", source);
        let events = a.analyze(&file);
        let auth: Vec<_> = events
            .iter()
            .filter(|e| e.kind == EventKind::AuthEvent)
            .collect();
        assert!(!auth.is_empty());
        assert_eq!(auth[0].name, "user_signed_in");
        assert!(auth[0].properties.iter().any(|p| p.name == "method"));
    }

    #[test]
    fn detects_nextauth_sign_out() {
        let a = adapter("/proj");
        let source = "import { signOut } from 'next-auth/react';";
        let file = parse("/proj", "src/auth.ts", source);
        let events = a.analyze(&file);
        let auth: Vec<_> = events
            .iter()
            .filter(|e| e.kind == EventKind::AuthEvent)
            .collect();
        assert!(!auth.is_empty());
        assert_eq!(auth[0].name, "user_signed_out");
    }

    #[test]
    fn no_auth_events_for_non_nextauth_imports() {
        let a = adapter("/proj");
        let source = "import { useState } from 'react';";
        let file = parse("/proj", "src/comp.tsx", source);
        let events = a.analyze(&file);
        assert!(events.iter().all(|e| e.kind != EventKind::AuthEvent));
    }

    // -----------------------------------------------------------------------
    // Adapter::analyze — form detection
    // -----------------------------------------------------------------------

    #[test]
    fn detects_handle_submit_function() {
        let a = adapter("/proj");
        let source = "function handleSubmit(e) { e.preventDefault(); }";
        let file = parse("/proj", "src/form.tsx", source);
        let events = a.analyze(&file);
        let forms: Vec<_> = events
            .iter()
            .filter(|e| e.kind == EventKind::FormSubmit)
            .collect();
        assert!(!forms.is_empty());
        assert_eq!(forms[0].name, "form_submitted");
    }

    #[test]
    fn detects_on_submit_arrow_function() {
        let a = adapter("/proj");
        let source = "const onSubmit = async (data) => { await fetch('/api', data); };";
        let file = parse("/proj", "src/form.tsx", source);
        let events = a.analyze(&file);
        let forms: Vec<_> = events
            .iter()
            .filter(|e| e.kind == EventKind::FormSubmit)
            .collect();
        assert!(!forms.is_empty());
    }

    #[test]
    fn no_form_event_for_regular_function() {
        let a = adapter("/proj");
        let source = "function processData(d) { return d; }";
        let file = parse("/proj", "src/util.ts", source);
        let events = a.analyze(&file);
        assert!(events.iter().all(|e| e.kind != EventKind::FormSubmit));
    }

    // -----------------------------------------------------------------------
    // Files outside Next.js conventions produce no route/api events
    // -----------------------------------------------------------------------

    #[test]
    fn no_route_events_for_non_nextjs_file() {
        let a = adapter("/proj");
        let file = parse(
            "/proj",
            "src/utils/helpers.ts",
            "export const add = (a, b) => a + b;",
        );
        let events = a.analyze(&file);
        assert!(events.iter().all(|e| e.kind != EventKind::PageView));
        assert!(events.iter().all(|e| e.kind != EventKind::ApiCall));
    }

    #[test]
    fn all_proposals_carry_nextjs_attribution() {
        let a = adapter("/proj");
        // Page
        let page = parse("/proj", "pages/about.tsx", "export default function About() {}");
        for e in a.analyze(&page) {
            assert_eq!(e.adapter, "nextjs", "page event missing adapter");
        }
        // Auth
        let auth = parse(
            "/proj",
            "src/auth.ts",
            "import { signIn } from 'next-auth/react';",
        );
        for e in a.analyze(&auth) {
            assert_eq!(e.adapter, "nextjs", "auth event missing adapter");
        }
        // Form
        let form = parse(
            "/proj",
            "src/form.tsx",
            "function handleSubmit(e) { e.preventDefault(); }",
        );
        for e in a.analyze(&form) {
            assert_eq!(e.adapter, "nextjs", "form event missing adapter");
        }
        // API
        let api = parse(
            "/proj",
            "pages/api/users.ts",
            "export default function handler(req, res) {}",
        );
        for e in a.analyze(&api) {
            assert_eq!(e.adapter, "nextjs", "api event missing adapter");
        }
    }
}
