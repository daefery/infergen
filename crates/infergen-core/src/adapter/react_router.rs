//! React Router adapter.
//!
//! Detects route definitions from JSX `<Route path="..." />` attributes and
//! route config object `{ path: '...' }` patterns via a line-based text scan.
//! Only processes files that import from `react-router` or `react-router-dom`.

use std::path::PathBuf;

use crate::{
    detect::{Framework, Language},
    namer::{NameSignals, Namer},
    parser::ParsedFile,
};

use super::{Adapter, EventKind, ProposedEvent};

/// Adapter for React Router applications.
///
/// Scans TypeScript and JavaScript source files for `<Route path="..." />`
/// JSX elements and `{ path: '...' }` route config objects and proposes
/// [`EventKind::PageView`] events.
pub struct ReactRouterAdapter {
    /// Project root — used to set `source_path` on proposals.
    pub project_root: PathBuf,
}

impl ReactRouterAdapter {
    /// Create a new adapter anchored at `project_root`.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }
}

impl Adapter for ReactRouterAdapter {
    fn analyze(&self, file: &ParsedFile) -> Vec<ProposedEvent> {
        if !matches!(file.lang, Language::TypeScript | Language::JavaScript) {
            return Vec::new();
        }
        // Pre-filter: file must reference react-router to avoid scanning unrelated JSX.
        if !file.source.contains("react-router") {
            return Vec::new();
        }
        let namer = Namer::new();
        extract_route_paths(&file.source)
            .into_iter()
            .map(|path| {
                let name = namer
                    .derive(&NameSignals {
                        route: Some(&path),
                        handler_name: None,
                        kind: EventKind::PageView,
                        component_name: None,
                    })
                    .name;
                ProposedEvent::new(name, EventKind::PageView, file.path.clone(), 0.80)
                    .with_adapter("react-router")
            })
            .collect()
    }

    fn framework(&self) -> Framework {
        Framework::React
    }
}

/// Extract deduplicated route path strings from React Router source.
///
/// Detects both JSX `path="..."` props and route config `path: '...'` objects.
fn extract_route_paths(source: &str) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('*') {
            continue;
        }

        // Pattern A: JSX prop  path="..." or path='...'
        if let Some(path) = extract_path_prop(trimmed) {
            if !path.is_empty() && path != "*" && seen.insert(path.clone()) {
                paths.push(path);
            }
        }

        // Pattern B: route config object  path: '...' or path: "..."
        if let Some(path) = extract_path_key(trimmed) {
            if !path.is_empty() && path != "*" && seen.insert(path.clone()) {
                paths.push(path);
            }
        }
    }

    paths
}

/// Extract `path` from JSX attribute `path="value"` or `path='value'`.
fn extract_path_prop(line: &str) -> Option<String> {
    // Look for `path=` followed by a quote or `{"`
    let pos = line.find("path=")?;
    let after = line[pos + 5..].trim_start();

    // Unwrap optional JSX expression braces: path={"/users"} → "/users"
    let after = after.trim_start_matches('{');

    extract_quoted_string(after).filter(|p| p.starts_with('/'))
}

/// Extract `path` from JS object key `path: 'value'` or `path: "value"`.
fn extract_path_key(line: &str) -> Option<String> {
    // Look for `path:` (object property, not JSX prop)
    let pos = line.find("path:")?;
    let after = line[pos + 5..].trim_start();

    extract_quoted_string(after).filter(|p| p.starts_with('/'))
}

/// Extract the content of the first quoted string (single or double quotes) in `s`.
fn extract_quoted_string(s: &str) -> Option<String> {
    let s = s.trim_start();
    let quote = s.chars().next()?;
    if !matches!(quote, '"' | '\'') {
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
            path: PathBuf::from("App.tsx"),
            lang,
            source: source.to_owned(),
            diagnostics: vec![],
        }
    }

    fn adapter() -> ReactRouterAdapter {
        ReactRouterAdapter::new("/project")
    }

    #[test]
    fn react_router_adapter_empty_for_non_js_file() {
        let f = file(Language::Ruby, r#"import { Route } from 'react-router-dom';"#);
        assert!(adapter().analyze(&f).is_empty());
    }

    #[test]
    fn react_router_adapter_empty_without_react_router_import() {
        let f = file(Language::TypeScript, r#"<Route path="/users" element={<Users />} />"#);
        assert!(adapter().analyze(&f).is_empty());
    }

    #[test]
    fn react_router_adapter_detects_jsx_route_double_quoted() {
        let src = "import { Route } from 'react-router-dom';\n<Route path=\"/users\" element={<Users />} />";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::PageView);
    }

    #[test]
    fn react_router_adapter_detects_jsx_route_single_quoted() {
        let src = "import { Route } from 'react-router-dom';\n<Route path='/about' component={About} />";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn react_router_adapter_detects_route_config_object() {
        let src = "import { createBrowserRouter } from 'react-router-dom';\nconst routes = [{ path: '/about', element: <About /> }];";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn react_router_adapter_multiple_routes() {
        let src = "import { Routes, Route } from 'react-router-dom';\n<Route path='/home' />\n<Route path='/about' />\n<Route path='/users' />";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn react_router_adapter_skips_comment_line() {
        let src = "import { Route } from 'react-router-dom';\n// <Route path='/hidden' />\n<Route path='/real' />";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn react_router_adapter_skips_empty_path() {
        let src = "import { Route } from 'react-router-dom';\n<Route path='' />";
        let f = file(Language::TypeScript, src);
        assert!(adapter().analyze(&f).is_empty());
    }

    #[test]
    fn react_router_adapter_skips_wildcard_path() {
        let src = "import { Route } from 'react-router-dom';\n<Route path='*' />";
        let f = file(Language::TypeScript, src);
        assert!(adapter().analyze(&f).is_empty());
    }

    #[test]
    fn react_router_adapter_param_route_accepted() {
        let src = "import { Route } from 'react-router-dom';\n<Route path='/:id' />";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn react_router_adapter_confidence_is_0_80() {
        let src = "import { Route } from 'react-router-dom';\n<Route path='/users' />";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        assert!((events[0].confidence - 0.80).abs() < f32::EPSILON);
    }

    #[test]
    fn react_router_adapter_attribution_is_react_router() {
        let src = "import { Route } from 'react-router-dom';\n<Route path='/users' />";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        assert_eq!(events[0].adapter, "react-router");
    }

    #[test]
    fn react_router_framework_returns_react() {
        assert_eq!(adapter().framework(), Framework::React);
    }

    #[test]
    fn react_router_adapter_events_are_page_view() {
        let src = "import { Route } from 'react-router-dom';\n<Route path='/dashboard' />";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        assert!(events.iter().all(|e| e.kind == EventKind::PageView));
    }
}
