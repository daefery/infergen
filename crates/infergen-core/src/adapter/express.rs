//! Express.js adapter.
//!
//! Detects route registrations (`app.get('/path', handler)`) via a line-based
//! text scan. No OXC AST traversal required — Express route calls are
//! consistently formatted one-liners that a pattern scan handles reliably.

use std::path::PathBuf;

use crate::{
    detect::{Framework, Language},
    namer::{NameSignals, Namer},
    parser::ParsedFile,
};

use super::{Adapter, EventKind, ProposedEvent};

const EXPRESS_METHODS: &[&str] = &[
    "get", "post", "put", "delete", "patch", "head", "options", "all", "use",
];

/// Adapter for Express.js applications.
///
/// Scans TypeScript and JavaScript source files for `app.METHOD('/path', ...)`
/// and `router.METHOD('/path', ...)` route registration calls and proposes
/// [`EventKind::ApiCall`] events.
pub struct ExpressAdapter {
    /// Project root — used to set `source_path` on proposals.
    pub project_root: PathBuf,
}

impl ExpressAdapter {
    /// Create a new adapter anchored at `project_root`.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }
}

impl Adapter for ExpressAdapter {
    fn analyze(&self, file: &ParsedFile) -> Vec<ProposedEvent> {
        if !matches!(file.lang, Language::TypeScript | Language::JavaScript) {
            return Vec::new();
        }
        // Pre-filter: file must reference 'express' to avoid deep-scanning unrelated JS.
        if !file.source.contains("express") {
            return Vec::new();
        }
        let namer = Namer::new();
        extract_express_routes(&file.source)
            .into_iter()
            .map(|(method, path)| {
                let name = namer
                    .derive(&NameSignals {
                        route: Some(&path),
                        handler_name: None,
                        kind: EventKind::ApiCall,
                        component_name: None,
                    })
                    .name;
                let _ = method; // method carried via prop type_hint
                ProposedEvent::new(name, EventKind::ApiCall, file.path.clone(), 0.80)
                    .with_prop("endpoint", Some("string"))
                    .with_prop("method", Some("string"))
                    .with_adapter("express")
            })
            .collect()
    }

    fn framework(&self) -> Framework {
        Framework::Express
    }
}

/// Extract `(method, path)` pairs from Express route registrations in `source`.
///
/// Handles `app.METHOD('/path', ...)` and `router.METHOD('/path', ...)`.
/// Paths without a leading `/` are skipped.
fn extract_express_routes(source: &str) -> Vec<(String, String)> {
    let mut routes = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('*') {
            continue;
        }
        for &method in EXPRESS_METHODS {
            let needle = format!(".{method}(");
            let Some(pos) = trimmed.find(needle.as_str()) else {
                continue;
            };
            let after = &trimmed[pos + needle.len()..];
            if let Some(path) = extract_first_quoted(after) {
                if path.starts_with('/') {
                    routes.push((method.to_owned(), path));
                }
            }
        }
    }
    routes
}

/// Extract the first quoted string (single, double, or backtick) from `s`.
fn extract_first_quoted(s: &str) -> Option<String> {
    let s = s.trim_start();
    let quote = s.chars().next()?;
    if !matches!(quote, '"' | '\'' | '`') {
        return None;
    }
    let rest = &s[1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_owned())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::detect::Language;
    use crate::parser::ParsedFile;

    fn file(lang: Language, source: &str) -> ParsedFile {
        ParsedFile {
            path: PathBuf::from("server.ts"),
            lang,
            source: source.to_owned(),
            diagnostics: vec![],
        }
    }

    fn adapter() -> ExpressAdapter {
        ExpressAdapter::new("/project")
    }

    #[test]
    fn express_adapter_empty_for_non_js_file() {
        let f = file(Language::Ruby, "app.get('/users', handler)");
        assert!(adapter().analyze(&f).is_empty());
    }

    #[test]
    fn express_adapter_empty_without_express_keyword() {
        let f = file(Language::TypeScript, "router.get('/users', handler)");
        assert!(adapter().analyze(&f).is_empty());
    }

    #[test]
    fn express_adapter_detects_get_route() {
        let f = file(
            Language::TypeScript,
            "import express from 'express';\napp.get('/users', getUsers);",
        );
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::ApiCall);
    }

    #[test]
    fn express_adapter_detects_post_route() {
        let f = file(
            Language::JavaScript,
            "const express = require('express');\napp.post('/items', createItem);",
        );
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::ApiCall);
    }

    #[test]
    fn express_adapter_detects_delete_route() {
        let f = file(
            Language::TypeScript,
            "import express from 'express';\napp.delete('/users/:id', del);",
        );
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn express_adapter_detects_dynamic_route() {
        let f = file(
            Language::TypeScript,
            "import 'express';\napp.get('/users/:id', getUser);",
        );
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn express_adapter_multiple_routes_multiple_events() {
        let f = file(
            Language::TypeScript,
            "import express from 'express';\napp.get('/a', h1);\napp.post('/b', h2);\napp.put('/c', h3);",
        );
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn express_adapter_all_events_carry_express_attribution() {
        let f = file(
            Language::TypeScript,
            "import express from 'express';\napp.get('/x', h);",
        );
        let events = adapter().analyze(&f);
        assert!(events.iter().all(|e| e.adapter == "express"));
    }

    #[test]
    fn express_adapter_confidence_is_0_80() {
        let f = file(
            Language::TypeScript,
            "import express from 'express';\napp.get('/users', h);",
        );
        let events = adapter().analyze(&f);
        assert!((events[0].confidence - 0.80).abs() < f32::EPSILON);
    }

    #[test]
    fn express_adapter_events_have_endpoint_and_method_props() {
        let f = file(
            Language::TypeScript,
            "import express from 'express';\napp.get('/users', h);",
        );
        let events = adapter().analyze(&f);
        let names: Vec<&str> = events[0].properties.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"endpoint"));
        assert!(names.contains(&"method"));
    }

    #[test]
    fn express_framework_returns_express() {
        assert_eq!(adapter().framework(), Framework::Express);
    }

    #[test]
    fn express_adapter_comment_line_skipped() {
        let f = file(
            Language::TypeScript,
            "import express from 'express';\n// app.get('/hidden', handler)\napp.get('/real', h);",
        );
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn express_adapter_router_prefix() {
        let f = file(
            Language::JavaScript,
            "const express = require('express');\nrouter.get('/items', listItems);",
        );
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn express_adapter_no_leading_slash_path_skipped() {
        let f = file(
            Language::TypeScript,
            "import express from 'express';\napp.get('users', handler);",
        );
        assert!(adapter().analyze(&f).is_empty());
    }
}
