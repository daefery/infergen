//! SvelteKit adapter.
//!
//! Detects routes from SvelteKit's file-based routing convention:
//!
//! * `src/routes/.../+page.svelte` → [`EventKind::PageView`]
//! * `src/routes/.../+page.server.{ts,js}` → [`EventKind::PageView`]
//! * `src/routes/.../+server.{ts,js}` → [`EventKind::ApiCall`]
//!
//! Route groups `(group)` are stripped; dynamic segments `[param]` are kept.
//! All detection is path-based — file contents are not read.

use std::path::{Path, PathBuf};

use crate::{
    detect::Framework,
    namer::{NameSignals, Namer},
    parser::ParsedFile,
};

use super::{Adapter, EventKind, ProposedEvent};

/// Adapter for SvelteKit applications.
pub struct SvelteKitAdapter {
    /// Project root — paths are resolved relative to this.
    pub project_root: PathBuf,
}

impl SvelteKitAdapter {
    /// Create a new adapter anchored at `project_root`.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }
}

impl Adapter for SvelteKitAdapter {
    fn analyze(&self, file: &ParsedFile) -> Vec<ProposedEvent> {
        let ext = file
            .path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if !matches!(ext, "svelte" | "ts" | "js") {
            return Vec::new();
        }

        let rel = match file.path.strip_prefix(&self.project_root) {
            Ok(r) => r,
            Err(_) => file.path.as_path(),
        };

        let Some((route, kind)) = route_from_sveltekit_path(rel) else {
            return Vec::new();
        };

        let name = Namer::new()
            .derive(&NameSignals {
                route: Some(&route),
                handler_name: None,
                kind,
                component_name: None,
            })
            .name;

        let mut event =
            ProposedEvent::new(name, kind, file.path.clone(), 0.90).with_adapter("svelte-kit");

        if kind == EventKind::ApiCall {
            event = event
                .with_prop("endpoint", Some("string"))
                .with_prop("method", Some("string"));
        }

        vec![event]
    }

    fn framework(&self) -> Framework {
        Framework::SvelteKit
    }
}

/// Derive a `(route, EventKind)` pair from a path relative to the project root.
///
/// Returns `None` when the path is not a recognized SvelteKit route file.
pub fn route_from_sveltekit_path(rel: &Path) -> Option<(String, EventKind)> {
    let comps: Vec<&str> = rel
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    // Find the "routes" segment, optionally preceded by "src".
    let routes_idx = comps.iter().position(|&s| s == "routes")?;

    let filename = *comps.last()?;

    // Classify by filename.
    let kind = match filename {
        "+page.svelte" | "+page.server.ts" | "+page.server.js" => EventKind::PageView,
        "+server.ts" | "+server.js" => EventKind::ApiCall,
        _ => return None, // +layout.svelte, +error.svelte, etc.
    };

    // Directory segments between "routes" and the filename.
    let dir_segs = &comps[routes_idx + 1..comps.len().saturating_sub(1)];

    let route_segs: Vec<&str> = dir_segs
        .iter()
        .filter_map(|seg| {
            // Drop route groups: "(marketing)" → skip
            if seg.starts_with('(') && seg.ends_with(')') {
                return None;
            }
            Some(*seg)
        })
        .collect();

    let route = if route_segs.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", route_segs.join("/"))
    };

    Some((route, kind))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::detect::Language;
    use crate::parser::ParsedFile;

    fn svelte_file(path: &str) -> ParsedFile {
        ParsedFile {
            path: PathBuf::from(path),
            lang: Language::Svelte,
            source: String::new(),
            diagnostics: vec![],
        }
    }

    fn ts_file(path: &str) -> ParsedFile {
        ParsedFile {
            path: PathBuf::from(path),
            lang: Language::TypeScript,
            source: String::new(),
            diagnostics: vec![],
        }
    }

    fn adapter() -> SvelteKitAdapter {
        SvelteKitAdapter::new("/project")
    }

    fn adapter_rel() -> SvelteKitAdapter {
        // Adapter with empty project root so rel paths pass through unchanged.
        SvelteKitAdapter::new("")
    }

    #[test]
    fn sveltekit_adapter_empty_for_non_sveltekit_path() {
        let f = ts_file("src/lib/utils.ts");
        assert!(adapter_rel().analyze(&f).is_empty());
    }

    #[test]
    fn sveltekit_adapter_root_page() {
        let f = svelte_file("src/routes/+page.svelte");
        let events = adapter_rel().analyze(&f);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::PageView);
    }

    #[test]
    fn sveltekit_adapter_about_page() {
        let f = svelte_file("src/routes/about/+page.svelte");
        let events = adapter_rel().analyze(&f);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::PageView);
    }

    #[test]
    fn sveltekit_adapter_server_route_is_api_call() {
        let f = ts_file("src/routes/api/users/+server.ts");
        let events = adapter_rel().analyze(&f);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::ApiCall);
    }

    #[test]
    fn sveltekit_adapter_page_server_ts_is_page_view() {
        let f = ts_file("src/routes/blog/+page.server.ts");
        let events = adapter_rel().analyze(&f);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::PageView);
    }

    #[test]
    fn sveltekit_adapter_layout_skipped() {
        let f = svelte_file("src/routes/+layout.svelte");
        assert!(adapter_rel().analyze(&f).is_empty());
    }

    #[test]
    fn sveltekit_adapter_error_skipped() {
        let f = svelte_file("src/routes/+error.svelte");
        assert!(adapter_rel().analyze(&f).is_empty());
    }

    #[test]
    fn sveltekit_adapter_route_group_stripped() {
        let f = svelte_file("src/routes/(marketing)/home/+page.svelte");
        let events = adapter_rel().analyze(&f);
        assert_eq!(events.len(), 1);
        // Route should not contain "(marketing)"
        assert!(!events[0].name.contains("marketing"));
    }

    #[test]
    fn sveltekit_adapter_dynamic_segment_kept() {
        let f = svelte_file("src/routes/users/[id]/+page.svelte");
        let events = adapter_rel().analyze(&f);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn sveltekit_adapter_deep_nesting() {
        let f = svelte_file("src/routes/a/b/c/+page.svelte");
        let events = adapter_rel().analyze(&f);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::PageView);
    }

    #[test]
    fn sveltekit_adapter_confidence_is_0_90() {
        let f = svelte_file("src/routes/about/+page.svelte");
        let events = adapter_rel().analyze(&f);
        assert!((events[0].confidence - 0.90).abs() < f32::EPSILON);
    }

    #[test]
    fn sveltekit_adapter_attribution_is_svelte_kit() {
        let f = svelte_file("src/routes/about/+page.svelte");
        let events = adapter_rel().analyze(&f);
        assert_eq!(events[0].adapter, "svelte-kit");
    }

    #[test]
    fn sveltekit_framework_returns_sveltekit() {
        assert_eq!(adapter().framework(), Framework::SvelteKit);
    }

    #[test]
    fn sveltekit_adapter_api_route_has_props() {
        let f = ts_file("src/routes/api/users/+server.ts");
        let events = adapter_rel().analyze(&f);
        let prop_names: Vec<&str> = events[0].properties.iter().map(|p| p.name.as_str()).collect();
        assert!(prop_names.contains(&"endpoint"));
        assert!(prop_names.contains(&"method"));
    }
}
