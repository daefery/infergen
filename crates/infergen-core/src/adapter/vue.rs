//! Vue / Nuxt adapter.
//!
//! Two detection modes:
//!
//! * **Mode 1 — Nuxt file-based routing**: `.vue` files under a `pages/`
//!   directory are mapped to routes the same way Next.js maps its `pages/`
//!   directory.  Confidence 0.85.
//!
//! * **Mode 2 — Vue Router config**: TypeScript/JavaScript files that reference
//!   `vue-router` / `createRouter` / `VueRouter` are scanned for
//!   `path: '...'` object-property patterns.  Confidence 0.75.

use std::path::{Path, PathBuf};

use crate::{
    detect::{Framework, Language},
    namer::{NameSignals, Namer},
    parser::ParsedFile,
};

use super::{Adapter, EventKind, ProposedEvent};

/// Adapter for Vue.js and Nuxt applications.
pub struct VueAdapter {
    /// Project root — paths are resolved relative to this.
    pub project_root: PathBuf,
}

impl VueAdapter {
    /// Create a new adapter anchored at `project_root`.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }
}

impl Adapter for VueAdapter {
    fn analyze(&self, file: &ParsedFile) -> Vec<ProposedEvent> {
        let ext = file
            .path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let rel = match file.path.strip_prefix(&self.project_root) {
            Ok(r) => r,
            Err(_) => file.path.as_path(),
        };

        if ext == "vue" {
            return self.analyze_vue_file(rel, file);
        }

        if matches!(file.lang, Language::TypeScript | Language::JavaScript) {
            return self.analyze_router_config(file);
        }

        Vec::new()
    }

    fn framework(&self) -> Framework {
        Framework::Vue
    }
}

impl VueAdapter {
    /// Mode 1: derive a PageView event from a Nuxt `pages/xxx.vue` path.
    fn analyze_vue_file(&self, rel: &Path, file: &ParsedFile) -> Vec<ProposedEvent> {
        let Some(route) = route_from_nuxt_path(rel) else {
            return Vec::new();
        };
        let name = Namer::new()
            .derive(&NameSignals {
                route: Some(&route),
                handler_name: None,
                kind: EventKind::PageView,
                component_name: None,
            })
            .name;
        vec![
            ProposedEvent::new(name, EventKind::PageView, file.path.clone(), 0.85)
                .with_adapter("vue"),
        ]
    }

    /// Mode 2: scan Vue Router config file for `path: '...'` patterns.
    fn analyze_router_config(&self, file: &ParsedFile) -> Vec<ProposedEvent> {
        if !is_vue_router_file(&file.source) {
            return Vec::new();
        }
        extract_vue_router_paths(&file.source)
            .into_iter()
            .map(|path| {
                let kind = if is_api_path(&path) {
                    EventKind::ApiCall
                } else {
                    EventKind::PageView
                };
                let name = Namer::new()
                    .derive(&NameSignals {
                        route: Some(&path),
                        handler_name: None,
                        kind,
                        component_name: None,
                    })
                    .name;
                ProposedEvent::new(name, kind, file.path.clone(), 0.75).with_adapter("vue")
            })
            .collect()
    }
}

/// Derive a route string from a Nuxt `pages/xxx.vue` file path.
///
/// Returns `None` when the path is not under a `pages/` directory.
pub fn route_from_nuxt_path(rel: &Path) -> Option<String> {
    let comps: Vec<&str> = rel
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    let first = *comps.first()?;
    if first != "pages" {
        return None;
    }

    let rest = &comps[1..];
    if rest.is_empty() {
        return None;
    }

    let last = *rest.last()?;
    if !last.ends_with(".vue") {
        return None;
    }

    let stem = last.strip_suffix(".vue")?;

    // Build segment list: all dirs + stem (omit "index" stem).
    let dir_segs = &rest[..rest.len() - 1];
    let mut segs: Vec<&str> = dir_segs.to_vec();
    if stem != "index" {
        segs.push(stem);
    }

    if segs.is_empty() {
        Some("/".to_owned())
    } else {
        Some(format!("/{}", segs.join("/")))
    }
}

/// Return `true` if the source looks like a Vue Router config file.
fn is_vue_router_file(source: &str) -> bool {
    source.contains("vue-router")
        || source.contains("createRouter")
        || source.contains("VueRouter")
}

/// Extract route paths from `path: '...'` or `path: "..."` object properties.
///
/// Only returns paths that start with `/`.
fn extract_vue_router_paths(source: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('*') {
            continue;
        }
        if let Some(path) = extract_path_value(trimmed) {
            if !path.is_empty() && seen.insert(path.clone()) {
                paths.push(path);
            }
        }
    }

    paths
}

/// Extract the value from `path: '...'` or `path: "..."` on a single line.
fn extract_path_value(line: &str) -> Option<String> {
    let pos = line.find("path:")?;
    let after = line[pos + 5..].trim_start();
    extract_quoted_string(after).filter(|p| p.starts_with('/'))
}

/// Extract the content of the first quoted string (single or double quotes).
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

/// Return `true` when path likely targets an API endpoint (starts with `/api/`).
fn is_api_path(path: &str) -> bool {
    path.starts_with("/api/")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::detect::Language;
    use crate::parser::ParsedFile;

    fn vue_file(path: &str) -> ParsedFile {
        ParsedFile {
            path: PathBuf::from(path),
            lang: Language::Vue,
            source: String::new(),
            diagnostics: vec![],
        }
    }

    fn ts_file(path: &str, source: &str) -> ParsedFile {
        ParsedFile {
            path: PathBuf::from(path),
            lang: Language::TypeScript,
            source: source.to_owned(),
            diagnostics: vec![],
        }
    }

    fn adapter() -> VueAdapter {
        VueAdapter::new("")
    }

    #[test]
    fn vue_adapter_empty_for_non_vue_file_non_router_js() {
        let f = ParsedFile {
            path: PathBuf::from("src/components/Button.ts"),
            lang: Language::TypeScript,
            source: "export default {}".to_owned(),
            diagnostics: vec![],
        };
        assert!(adapter().analyze(&f).is_empty());
    }

    #[test]
    fn vue_adapter_nuxt_index_vue_is_root_page_view() {
        let f = vue_file("pages/index.vue");
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::PageView);
    }

    #[test]
    fn vue_adapter_nuxt_about_vue() {
        let f = vue_file("pages/about.vue");
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::PageView);
    }

    #[test]
    fn vue_adapter_nuxt_users_index_vue() {
        let f = vue_file("pages/users/index.vue");
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
        // users/index.vue → /users (index stripped)
    }

    #[test]
    fn vue_adapter_nuxt_dynamic_segment() {
        let f = vue_file("pages/users/[id].vue");
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn vue_adapter_vue_router_config_path_detected() {
        let src = "import { createRouter } from 'vue-router';\nconst routes = [\n  { path: '/about', component: About },\n];\n";
        let f = ts_file("router/index.ts", src);
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::PageView);
    }

    #[test]
    fn vue_adapter_vue_router_api_path_is_api_call() {
        let src = "import { createRouter } from 'vue-router';\nconst routes = [{ path: '/api/users', component: Users }];\n";
        let f = ts_file("router/index.ts", src);
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::ApiCall);
    }

    #[test]
    fn vue_adapter_vue_router_without_keyword_skipped() {
        let src = "const routes = [{ path: '/about', component: About }];\n";
        let f = ts_file("router/index.ts", src);
        assert!(adapter().analyze(&f).is_empty());
    }

    #[test]
    fn vue_adapter_nuxt_confidence_is_0_85() {
        let f = vue_file("pages/about.vue");
        let events = adapter().analyze(&f);
        assert!((events[0].confidence - 0.85).abs() < f32::EPSILON);
    }

    #[test]
    fn vue_adapter_router_config_confidence_is_0_75() {
        let src = "import { createRouter } from 'vue-router';\nconst routes = [{ path: '/about', component: About }];\n";
        let f = ts_file("router/index.ts", src);
        let events = adapter().analyze(&f);
        assert!((events[0].confidence - 0.75).abs() < f32::EPSILON);
    }

    #[test]
    fn vue_adapter_attribution_is_vue() {
        let f = vue_file("pages/about.vue");
        let events = adapter().analyze(&f);
        assert_eq!(events[0].adapter, "vue");
    }

    #[test]
    fn vue_framework_returns_vue() {
        assert_eq!(adapter().framework(), Framework::Vue);
    }

    #[test]
    fn vue_adapter_vue_file_not_in_pages_skipped() {
        let f = vue_file("components/Button.vue");
        assert!(adapter().analyze(&f).is_empty());
    }

    #[test]
    fn vue_adapter_multiple_router_paths_detected() {
        let src = "import { createRouter } from 'vue-router';\nconst routes = [\n  { path: '/home', component: Home },\n  { path: '/about', component: About },\n  { path: '/contact', component: Contact },\n];\n";
        let f = ts_file("router/index.ts", src);
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 3);
    }
}
