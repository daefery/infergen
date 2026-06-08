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

/// Derive the canonical route from an App Router `page` file.
///
/// Only `page.{tsx,ts,jsx,js}` marks a navigable page. `layout` files are
/// excluded: a layout wraps its `page` and re-rendering it is not a distinct
/// page view, so counting it would duplicate the page's event. Returns `None`
/// for `route.ts` (API) and any non-`page` file.
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
    if stem != "page" {
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
// Feature-prefix derivation (event-name disambiguation)
// ---------------------------------------------------------------------------

/// Path segments that carry no feature meaning and are dropped when building
/// an event-name prefix.
const PREFIX_NOISE: &[&str] = &[
    "components",
    "component",
    "ui",
    "common",
    "shared",
    "lib",
    "libs",
    "hooks",
    "hook",
    "utils",
    "util",
    "helpers",
    "helper",
    "modules",
    "module",
    "features",
    "feature",
    "containers",
    "container",
    "views",
    "view",
    "screens",
    "screen",
    "widgets",
    "widget",
    "src",
];

/// Derive a feature/page prefix from a file path (relative to the project
/// root) so generic event names (`button_clicked`, `form_submitted`) become
/// unique across files.
///
/// Strategy: keep meaningful directory + filename segments — feature folders,
/// route segments, route-group names (`(marketing)` → `marketing`), dynamic
/// segments (`[slug]` → `slug`) — while dropping boilerplate folders
/// ([`PREFIX_NOISE`]), the router roots (`app`/`pages`), and generic filenames
/// (`index`, `page`, `layout`, `route`). The last three meaningful segments are
/// joined to bound length. Returns `None` when nothing meaningful remains.
fn feature_prefix(rel: &Path) -> Option<String> {
    let rel = rel.strip_prefix("src").unwrap_or(rel);
    let comps: Vec<&str> = rel
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    let n = comps.len();
    let mut segs: Vec<String> = Vec::new();

    for (i, raw) in comps.iter().enumerate() {
        let is_file = i + 1 == n;
        let seg = if is_file { strip_js_ext(raw) } else { *raw };

        // Route group `(marketing)` → feature name; never noise-filtered.
        if seg.starts_with('(') && seg.ends_with(')') {
            let inner = seg.trim_start_matches('(').trim_end_matches(')');
            if !inner.is_empty() {
                segs.push(normalize_segment(inner));
            }
            continue;
        }

        // Dynamic route segment `[slug]` / `[...slug]` → bare name.
        let seg = seg
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim_start_matches("...");
        let lower = seg.to_ascii_lowercase();

        // Router roots only count as noise at the path head.
        if i == 0 && matches!(lower.as_str(), "app" | "pages") {
            continue;
        }
        if PREFIX_NOISE.contains(&lower.as_str()) {
            continue;
        }
        if is_file && matches!(lower.as_str(), "index" | "page" | "layout" | "route") {
            continue;
        }
        let norm = normalize_segment(&lower);
        if !norm.is_empty() {
            segs.push(norm);
        }
    }

    if segs.is_empty() {
        return None;
    }
    let start = segs.len().saturating_sub(4);
    Some(segs[start..].join("_"))
}

/// Lowercase a path segment and convert kebab/space separators to `_`.
fn normalize_segment(seg: &str) -> String {
    seg.to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// Join `prefix` and `name` on a snake_case token boundary, collapsing any
/// overlap so shared tokens are not repeated (path-join style dedup).
///
/// The largest suffix of `prefix`'s tokens that equals a leading run of
/// `name`'s tokens is merged:
///
/// - `dashboard_habits` + `dashboard_habits_page_viewed` → `dashboard_habits_page_viewed`
/// - `main_dashboard_wallet` + `dashboard_wallet_page_viewed` → `main_dashboard_wallet_page_viewed`
/// - `marketing_about` + `about_page_viewed` → `marketing_about_page_viewed`
/// - `dashboard_goal_pill` + `button_clicked` → `dashboard_goal_pill_button_clicked`
fn apply_prefix(name: &str, prefix: &str) -> String {
    let p: Vec<&str> = prefix.split('_').filter(|t| !t.is_empty()).collect();
    let n: Vec<&str> = name.split('_').filter(|t| !t.is_empty()).collect();
    if p.is_empty() {
        return name.to_owned();
    }

    // Largest k where the last k tokens of `prefix` equal the first k of `name`.
    let max_k = p.len().min(n.len());
    let mut overlap = 0;
    for k in 1..=max_k {
        if p[p.len() - k..] == n[..k] {
            overlap = k;
        }
    }

    let mut tokens: Vec<&str> = p[..p.len() - overlap].to_vec();
    tokens.extend_from_slice(&n);
    tokens.join("_")
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
// JSX interaction detection (clicks, form submits, search)
// ---------------------------------------------------------------------------

/// JSX attribute names that signal a click interaction.
const CLICK_ATTRS: &[&str] = &["onClick"];
/// JSX attribute names that signal a search interaction.
const SEARCH_ATTRS: &[&str] = &["onSearch", "onSearchChange", "onSearchSubmit"];
/// JSX attribute names that signal a form submission.
const SUBMIT_ATTRS: &[&str] = &["onSubmit"];

/// Walk the full component tree of `prog` and emit interaction proposals:
///
/// - `<button onClick>` / clickable elements with `onClick` → [`EventKind::ButtonClick`]
/// - `<form onSubmit>` → [`EventKind::FormSubmit`]
/// - search inputs (`type="search"`, name/placeholder containing "search") or
///   `onSearch` handlers → [`EventKind::Search`]
///
/// Unlike [`detect_forms_from_ast`], this descends into nested JSX inside
/// component render trees (return statements, arrows, conditionals, `.map`),
/// where real React interactions live.
fn detect_interactions_from_ast(
    prog: &oxc_ast::ast::Program<'_>,
    source_path: &Path,
    namer: &Namer,
) -> Vec<ProposedEvent> {
    let mut events = Vec::new();
    for stmt in &prog.body {
        walk_stmt(stmt, source_path, namer, &mut events);
    }
    events
}

fn walk_stmt(
    stmt: &oxc_ast::ast::Statement<'_>,
    src: &Path,
    namer: &Namer,
    events: &mut Vec<ProposedEvent>,
) {
    use oxc_ast::ast::{Declaration, ExportDefaultDeclarationKind, Statement};

    match stmt {
        Statement::FunctionDeclaration(func) => {
            if let Some(body) = &func.body {
                for s in &body.statements {
                    walk_stmt(s, src, namer, events);
                }
            }
        }
        Statement::ReturnStatement(ret) => {
            if let Some(expr) = &ret.argument {
                walk_expr(expr, src, namer, events);
            }
        }
        Statement::ExpressionStatement(es) => walk_expr(&es.expression, src, namer, events),
        Statement::VariableDeclaration(decl) => {
            for d in &decl.declarations {
                if let Some(init) = &d.init {
                    walk_expr(init, src, namer, events);
                }
            }
        }
        Statement::IfStatement(if_stmt) => {
            walk_stmt(&if_stmt.consequent, src, namer, events);
            if let Some(alt) = &if_stmt.alternate {
                walk_stmt(alt, src, namer, events);
            }
        }
        Statement::BlockStatement(block) => {
            for s in &block.body {
                walk_stmt(s, src, namer, events);
            }
        }
        Statement::ExportDefaultDeclaration(export) => match &export.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                if let Some(body) = &func.body {
                    for s in &body.statements {
                        walk_stmt(s, src, namer, events);
                    }
                }
            }
            ExportDefaultDeclarationKind::ArrowFunctionExpression(arrow) => {
                for s in &arrow.body.statements {
                    walk_stmt(s, src, namer, events);
                }
            }
            ExportDefaultDeclarationKind::JSXElement(elem) => {
                walk_jsx(elem, src, namer, events);
            }
            ExportDefaultDeclarationKind::JSXFragment(frag) => {
                for c in &frag.children {
                    walk_jsx_child(c, src, namer, events);
                }
            }
            ExportDefaultDeclarationKind::ParenthesizedExpression(p) => {
                walk_expr(&p.expression, src, namer, events);
            }
            _ => {}
        },
        Statement::ExportNamedDeclaration(export) => {
            if let Some(decl) = &export.declaration {
                match decl {
                    Declaration::FunctionDeclaration(func) => {
                        if let Some(body) = &func.body {
                            for s in &body.statements {
                                walk_stmt(s, src, namer, events);
                            }
                        }
                    }
                    Declaration::VariableDeclaration(var) => {
                        for d in &var.declarations {
                            if let Some(init) = &d.init {
                                walk_expr(init, src, namer, events);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn walk_expr(
    expr: &oxc_ast::ast::Expression<'_>,
    src: &Path,
    namer: &Namer,
    events: &mut Vec<ProposedEvent>,
) {
    use oxc_ast::ast::Expression;

    match expr {
        Expression::JSXElement(elem) => walk_jsx(elem, src, namer, events),
        Expression::JSXFragment(frag) => {
            for c in &frag.children {
                walk_jsx_child(c, src, namer, events);
            }
        }
        Expression::ParenthesizedExpression(p) => walk_expr(&p.expression, src, namer, events),
        Expression::ArrowFunctionExpression(arrow) => {
            for s in &arrow.body.statements {
                walk_stmt(s, src, namer, events);
            }
        }
        Expression::ConditionalExpression(cond) => {
            walk_expr(&cond.consequent, src, namer, events);
            walk_expr(&cond.alternate, src, namer, events);
        }
        Expression::LogicalExpression(logical) => {
            walk_expr(&logical.left, src, namer, events);
            walk_expr(&logical.right, src, namer, events);
        }
        Expression::CallExpression(call) => {
            // Handles `items.map(() => <button .../>)` and similar.
            for arg in &call.arguments {
                if let Some(arg_expr) = arg.as_expression() {
                    walk_expr(arg_expr, src, namer, events);
                }
            }
        }
        _ => {}
    }
}

fn walk_jsx_child(
    child: &oxc_ast::ast::JSXChild<'_>,
    src: &Path,
    namer: &Namer,
    events: &mut Vec<ProposedEvent>,
) {
    use oxc_ast::ast::JSXChild;

    match child {
        JSXChild::Element(elem) => walk_jsx(elem, src, namer, events),
        JSXChild::Fragment(frag) => {
            for c in &frag.children {
                walk_jsx_child(c, src, namer, events);
            }
        }
        JSXChild::ExpressionContainer(container) => {
            if let Some(expr) = container.expression.as_expression() {
                walk_expr(expr, src, namer, events);
            }
        }
        _ => {}
    }
}

fn walk_jsx(
    elem: &oxc_ast::ast::JSXElement<'_>,
    src: &Path,
    namer: &Namer,
    events: &mut Vec<ProposedEvent>,
) {
    process_jsx_element(elem, src, namer, events);
    for child in &elem.children {
        walk_jsx_child(child, src, namer, events);
    }
}

/// Inspect a single JSX element and emit any interaction proposals it implies.
fn process_jsx_element(
    elem: &oxc_ast::ast::JSXElement<'_>,
    src: &Path,
    namer: &Namer,
    events: &mut Vec<ProposedEvent>,
) {
    use oxc_ast::ast::JSXElementName;

    let opening = &elem.opening_element;
    let tag = match &opening.name {
        JSXElementName::Identifier(id) => id.name.as_str(),
        JSXElementName::IdentifierReference(id) => id.name.as_str(),
        _ => "",
    };
    let tag_lower = tag.to_ascii_lowercase();
    let is_custom_component = tag.chars().next().is_some_and(char::is_uppercase);

    // --- Form submit: <form onSubmit={...}> --------------------------------
    if tag_lower == "form"
        && let Some(handler) = first_handler_attr(opening, SUBMIT_ATTRS)
    {
        let signals = NameSignals {
            handler_name: handler.as_deref(),
            kind: EventKind::FormSubmit,
            route: None,
            component_name: None,
        };
        let result = namer.derive(&signals);
        events.push(ProposedEvent::new(
            result.name,
            EventKind::FormSubmit,
            src,
            0.6,
        ));
    }

    // --- Search: search inputs or onSearch handlers ------------------------
    let input_type = attr_string(opening, "type");
    let name_attr = attr_string(opening, "name");
    let placeholder = attr_string(opening, "placeholder");
    let looks_like_search = |s: &Option<String>| {
        s.as_deref()
            .is_some_and(|v| v.to_ascii_lowercase().contains("search"))
    };
    let is_search_input = tag_lower == "input"
        && (input_type.as_deref() == Some("search")
            || looks_like_search(&name_attr)
            || looks_like_search(&placeholder));
    let search_handler = first_handler_attr(opening, SEARCH_ATTRS);
    if is_search_input || search_handler.is_some() {
        // Prefer a named handler, then the input's `name` attribute, as entity.
        let handler_sig = search_handler.as_ref().and_then(|h| h.as_deref());
        let comp_sig = if handler_sig.is_none() {
            name_attr.as_deref()
        } else {
            None
        };
        let signals = NameSignals {
            handler_name: handler_sig,
            component_name: comp_sig,
            kind: EventKind::Search,
            route: None,
        };
        let result = namer.derive(&signals);
        events.push(
            ProposedEvent::new(result.name, EventKind::Search, src, 0.6)
                .with_prop("query", Some("string")),
        );
    }

    // --- Button click: clickable elements with onClick ---------------------
    if let Some(handler) = first_handler_attr(opening, CLICK_ATTRS) {
        let label = element_text(elem);
        let role = attr_string(opening, "role");
        let clickable = tag_lower == "button"
            || tag_lower == "a"
            || is_custom_component
            || role.as_deref() == Some("button")
            || label.is_some()
            || handler.is_some();
        if clickable {
            // Entity priority: named handler → button label → custom component tag.
            let label_ident = label.as_deref().and_then(label_to_identifier);
            let (handler_sig, comp_sig) = match (&handler, &label_ident) {
                (Some(h), _) => (Some(h.as_str()), None),
                (None, Some(l)) => (None, Some(l.as_str())),
                (None, None) if is_custom_component => (None, Some(tag)),
                _ => (None, None),
            };
            let signals = NameSignals {
                handler_name: handler_sig,
                component_name: comp_sig,
                kind: EventKind::ButtonClick,
                route: None,
            };
            let result = namer.derive(&signals);
            let mut event = ProposedEvent::new(result.name, EventKind::ButtonClick, src, 0.55);
            if label.is_some() {
                event = event.with_prop("label", Some("string"));
            }
            events.push(event);
        }
    }
}

/// Return `Some(Some(ident))` when the element has one of `attr_names` bound to
/// `{identifier}`, `Some(None)` when bound to a non-identifier expression
/// (arrow, member call), and `None` when the attribute is absent.
fn first_handler_attr(
    opening: &oxc_ast::ast::JSXOpeningElement<'_>,
    attr_names: &[&str],
) -> Option<Option<String>> {
    use oxc_ast::ast::{Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue};

    for item in &opening.attributes {
        let JSXAttributeItem::Attribute(attr) = item else {
            continue;
        };
        let JSXAttributeName::Identifier(id) = &attr.name else {
            continue;
        };
        if !attr_names.contains(&id.name.as_str()) {
            continue;
        }
        // Attribute present. Try to extract a bare identifier handler name.
        let ident = match &attr.value {
            Some(JSXAttributeValue::ExpressionContainer(container)) => container
                .expression
                .as_expression()
                .and_then(|e| match e {
                    Expression::Identifier(id) => Some(id.name.to_string()),
                    _ => None,
                }),
            _ => None,
        };
        return Some(ident);
    }
    None
}

/// Extract the string-literal value of attribute `attr_name`, if present.
fn attr_string(opening: &oxc_ast::ast::JSXOpeningElement<'_>, attr_name: &str) -> Option<String> {
    use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue};

    for item in &opening.attributes {
        let JSXAttributeItem::Attribute(attr) = item else {
            continue;
        };
        let JSXAttributeName::Identifier(id) = &attr.name else {
            continue;
        };
        if id.name.as_str() != attr_name {
            continue;
        }
        if let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value {
            return Some(lit.value.to_string());
        }
        return None;
    }
    None
}

/// Concatenate the visible text of a JSX element's subtree (depth-limited),
/// returning the trimmed result or `None` if empty.
fn element_text(elem: &oxc_ast::ast::JSXElement<'_>) -> Option<String> {
    let mut buf = String::new();
    gather_text(&elem.children, &mut buf, 0);
    let trimmed = buf.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn gather_text(children: &[oxc_ast::ast::JSXChild<'_>], buf: &mut String, depth: usize) {
    use oxc_ast::ast::JSXChild;

    if depth > 4 {
        return;
    }
    for child in children {
        match child {
            JSXChild::Text(text) => {
                buf.push(' ');
                buf.push_str(text.value.as_str());
            }
            JSXChild::Element(el) => gather_text(&el.children, buf, depth + 1),
            JSXChild::Fragment(frag) => gather_text(&frag.children, buf, depth + 1),
            _ => {}
        }
    }
}

/// Normalise human button text into a snake_case identifier suitable for the
/// namer's `component_name` signal. Returns `None` for empty or overly long
/// labels (likely a sentence, not a button).
fn label_to_identifier(label: &str) -> Option<String> {
    let words: Vec<String> = label
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    if words.is_empty() || words.len() > 5 {
        return None;
    }
    Some(words.join("_"))
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
            // Next.js supports an optional `src/` directory: `src/app` and
            // `src/pages` are equivalent to top-level `app`/`pages`. Strip a
            // leading `src` component so route detection treats both alike.
            let rel = rel.strip_prefix("src").unwrap_or(rel);
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

            // JSX interaction detection: clicks, form submits, search inputs
            // nested anywhere in the component render tree.
            v.extend(detect_interactions_from_ast(prog, &source_path, namer_ref));
            v
        }) {
            events.extend(ast_events);
        }

        // Disambiguate generic event names with a feature/page prefix derived
        // from the file path, so e.g. `button_clicked` from two files become
        // `dashboard_goal_pill_button_clicked` vs `wallet_button_clicked`.
        if let Some(prefix) = file
            .path
            .strip_prefix(&self.project_root)
            .ok()
            .and_then(feature_prefix)
        {
            for event in &mut events {
                event.name = apply_prefix(&event.name, &prefix);
            }
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
    fn route_from_app_layout_is_none() {
        // Layouts wrap pages; they must not produce a duplicate page view.
        assert!(route_from_app_path(Path::new("app/dashboard/layout.tsx")).is_none());
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
    fn analyzes_app_router_page_in_src_dir() {
        let a = adapter("/proj");
        let file = parse(
            "/proj",
            "src/app/dashboard/page.tsx",
            "export default function Page() {}",
        );
        let events = a.analyze(&file);
        let pv: Vec<_> = events
            .iter()
            .filter(|e| e.kind == EventKind::PageView)
            .collect();
        assert!(!pv.is_empty(), "src/app page must be detected");
        assert!(pv[0].name.contains("dashboard"));
        assert!(pv[0].properties.iter().any(|p| p.name == "route"));
    }

    #[test]
    fn analyzes_pages_router_page_in_src_dir() {
        let a = adapter("/proj");
        let file = parse(
            "/proj",
            "src/pages/about.tsx",
            "export default function About() {}",
        );
        let events = a.analyze(&file);
        assert_eq!(events.len(), 1, "src/pages page must be detected");
        assert_eq!(events[0].kind, EventKind::PageView);
        assert!(events[0].name.contains("page_viewed"));
    }

    #[test]
    fn analyzes_app_router_api_in_src_dir() {
        let a = adapter("/proj");
        let source = r#"
export async function GET(request: Request) { return Response.json({}); }
"#;
        let file = parse("/proj", "src/app/api/users/route.ts", source);
        let events = a.analyze(&file);
        let api: Vec<_> = events
            .iter()
            .filter(|e| e.kind == EventKind::ApiCall)
            .collect();
        assert_eq!(api.len(), 1, "src/app route handler must be detected");
        assert!(api[0].name.contains("get"));
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
        // `src/auth.ts` contributes the `auth` feature prefix.
        assert_eq!(auth[0].name, "auth_user_signed_in");
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
        assert_eq!(auth[0].name, "auth_user_signed_out");
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

    // -----------------------------------------------------------------------
    // JSX interaction detection — clicks, form submits, search
    // -----------------------------------------------------------------------

    fn kinds(events: &[ProposedEvent], kind: EventKind) -> Vec<&ProposedEvent> {
        events.iter().filter(|e| e.kind == kind).collect()
    }

    #[test]
    fn detects_button_click_with_named_handler() {
        let a = adapter("/proj");
        let src = r#"
export default function Page() {
    return <button onClick={handleDelete}>Delete</button>;
}
"#;
        let file = parse("/proj", "src/app/page.tsx", src);
        let events = a.analyze(&file);
        let clicks = kinds(&events, EventKind::ButtonClick);
        assert_eq!(clicks.len(), 1, "expected one button click");
        assert!(clicks[0].name.contains("delete"));
        assert!(clicks[0].name.ends_with("clicked"));
    }

    #[test]
    fn detects_button_click_from_text_label() {
        let a = adapter("/proj");
        let src = r#"
export default function Page() {
    return <button onClick={() => doThing()}>Add to Cart</button>;
}
"#;
        let file = parse("/proj", "src/app/page.tsx", src);
        let events = a.analyze(&file);
        let clicks = kinds(&events, EventKind::ButtonClick);
        assert_eq!(clicks.len(), 1);
        assert_eq!(clicks[0].name, "add_to_cart_clicked");
        assert!(clicks[0].properties.iter().any(|p| p.name == "label"));
    }

    #[test]
    fn detects_button_click_nested_in_map_and_conditional() {
        let a = adapter("/proj");
        let src = r#"
export default function List({ items, show }) {
    return (
        <div>
            {show && <button onClick={handleOpen}>Open</button>}
            {items.map((i) => <button onClick={() => remove(i)}>Remove</button>)}
        </div>
    );
}
"#;
        let file = parse("/proj", "src/app/list.tsx", src);
        let events = a.analyze(&file);
        let clicks = kinds(&events, EventKind::ButtonClick);
        // One from the `&&` branch, one from the `.map` callback.
        assert_eq!(clicks.len(), 2, "expected clicks from && and .map");
    }

    #[test]
    fn detects_form_submit_from_jsx() {
        let a = adapter("/proj");
        let src = r#"
export default function Page() {
    return <form onSubmit={handleCheckout}><input name="card" /></form>;
}
"#;
        let file = parse("/proj", "src/app/checkout.tsx", src);
        let events = a.analyze(&file);
        let forms = kinds(&events, EventKind::FormSubmit);
        assert_eq!(forms.len(), 1, "expected one form submit");
        assert!(forms[0].name.contains("checkout"));
        assert!(forms[0].name.ends_with("submitted"));
    }

    #[test]
    fn detects_search_input_by_type() {
        let a = adapter("/proj");
        let src = r#"
export default function Bar() {
    return <input type="search" name="q" />;
}
"#;
        let file = parse("/proj", "src/app/bar.tsx", src);
        let events = a.analyze(&file);
        let searches = kinds(&events, EventKind::Search);
        assert_eq!(searches.len(), 1, "expected one search event");
        assert!(searches[0].name.ends_with("searched"));
        assert!(searches[0].properties.iter().any(|p| p.name == "query"));
    }

    #[test]
    fn detects_search_from_handler() {
        let a = adapter("/proj");
        let src = r#"
export default function Bar() {
    return <SearchBox onSearch={handleSearch} />;
}
"#;
        let file = parse("/proj", "src/app/bar.tsx", src);
        let events = a.analyze(&file);
        let searches = kinds(&events, EventKind::Search);
        assert_eq!(searches.len(), 1);
        assert!(searches[0].name.ends_with("searched"));
    }

    #[test]
    fn detects_search_input_by_name() {
        let a = adapter("/proj");
        let src = r#"
export default function Bar() {
    return <input type="text" name="searchQuery" />;
}
"#;
        let file = parse("/proj", "src/app/bar.tsx", src);
        let events = a.analyze(&file);
        assert_eq!(kinds(&events, EventKind::Search).len(), 1);
    }

    #[test]
    fn no_button_click_for_bare_div_without_label_or_handler() {
        let a = adapter("/proj");
        let src = r#"
export default function Page() {
    return <div onClick={() => {}} />;
}
"#;
        let file = parse("/proj", "src/app/page.tsx", src);
        let events = a.analyze(&file);
        assert!(
            kinds(&events, EventKind::ButtonClick).is_empty(),
            "bare div onClick with no label/handler must not propose a click"
        );
    }

    #[test]
    fn feature_prefix_from_module_component() {
        assert_eq!(
            feature_prefix(Path::new("src/modules/dashboard/components/goal-pill.tsx")),
            Some("dashboard_goal_pill".to_owned())
        );
    }

    #[test]
    fn feature_prefix_route_group_kept() {
        assert_eq!(
            feature_prefix(Path::new("app/(marketing)/page.tsx")),
            Some("marketing".to_owned())
        );
    }

    #[test]
    fn feature_prefix_router_root_and_generic_file_dropped() {
        // `app` root + `page` filename are both dropped; nothing meaningful left.
        assert_eq!(feature_prefix(Path::new("app/page.tsx")), None);
    }

    #[test]
    fn apply_prefix_skips_when_name_already_prefixed() {
        assert_eq!(
            apply_prefix("dashboard_habits_page_viewed", "dashboard_habits"),
            "dashboard_habits_page_viewed"
        );
    }

    #[test]
    fn apply_prefix_dedupes_overlapping_token() {
        assert_eq!(
            apply_prefix("about_page_viewed", "marketing_about"),
            "marketing_about_page_viewed"
        );
    }

    #[test]
    fn apply_prefix_prepends_generic_name() {
        assert_eq!(
            apply_prefix("button_clicked", "dashboard_goal_pill"),
            "dashboard_goal_pill_button_clicked"
        );
    }

    #[test]
    fn same_generic_click_in_different_files_gets_unique_names() {
        let a = adapter("/proj");
        let src = r#"
export default function Comp() {
    return <button onClick={() => {}}>Back</button>;
}
"#;
        let f1 = parse("/proj", "src/modules/wallet/components/header.tsx", src);
        let f2 = parse("/proj", "src/modules/dashboard/components/header.tsx", src);
        let n1 = &a.analyze(&f1)[0].name;
        let n2 = &a.analyze(&f2)[0].name;
        assert_ne!(n1, n2, "same click in different features must differ");
        assert!(n1.contains("wallet"));
        assert!(n2.contains("dashboard"));
    }

    #[test]
    fn page_view_route_name_not_double_prefixed() {
        let a = adapter("/proj");
        let file = parse(
            "/proj",
            "src/app/dashboard/habits/page.tsx",
            "export default function Page() {}",
        );
        let pv: Vec<_> = a
            .analyze(&file)
            .into_iter()
            .filter(|e| e.kind == EventKind::PageView)
            .collect();
        assert_eq!(pv.len(), 1);
        assert_eq!(pv[0].name, "dashboard_habits_page_viewed");
    }

    #[test]
    fn interaction_events_carry_attribution() {
        let a = adapter("/proj");
        let src = r#"
export default function Page() {
    return <button onClick={handleSave}>Save</button>;
}
"#;
        let file = parse("/proj", "src/app/page.tsx", src);
        let events = a.analyze(&file);
        assert!(!events.is_empty());
        for e in &events {
            assert_eq!(e.adapter, "nextjs");
        }
    }
}
