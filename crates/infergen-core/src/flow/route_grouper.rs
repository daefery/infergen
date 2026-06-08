//! Route-prefix grouping for flow detection (E6.2).
//!
//! Groups proposals by the first meaningful path segment of their source file,
//! e.g. `pages/checkout/cart.tsx` and `pages/checkout/confirm.tsx` both yield
//! the segment `"checkout"` and are grouped together.

use std::collections::HashMap;
use std::path::Path;

use crate::ProposedEvent;

/// A group of proposals sharing a common route segment.
pub struct RouteGroup {
    /// Lowercase route segment, e.g. `"checkout"`.
    pub prefix: String,
    /// Indices into the proposals slice.
    pub indices: Vec<usize>,
}

/// Layout directories stripped before extracting the meaningful segment.
const LAYOUT_DIRS: &[&str] = &[
    "src", "app", "pages", "routes", "views", "components", "features",
    "lib", "modules", "screens", "containers",
];

/// Extract the first meaningful path segment from a source path.
///
/// Strips well-known layout directory prefixes, then returns the first
/// remaining component (lowercased, extension removed). Returns `None` when
/// the path yields no useful segment (root-level file, single-char component,
/// segment is a generic layout dir).
pub fn extract_route_segment(source_path: &Path) -> Option<String> {
    let mut components: Vec<String> = source_path
        .components()
        .filter_map(|c| {
            let s = c.as_os_str().to_string_lossy().to_lowercase();
            // Strip file extension from the last component.
            Some(s)
        })
        .collect();

    // Remove extension from the last component.
    if let Some(last) = components.last_mut() {
        if let Some(dot) = last.rfind('.') {
            *last = last[..dot].to_string();
        }
    }

    // Skip over leading layout dirs.
    let mut iter = components.iter().peekable();
    while let Some(seg) = iter.peek() {
        if LAYOUT_DIRS.contains(&seg.as_str()) {
            iter.next();
        } else {
            break;
        }
    }

    // Take the first remaining component that is meaningful.
    if let Some(seg) = iter.next() {
        // Reject single-char segments, empty, or those that are themselves layout dirs.
        if seg.len() >= 3 && !LAYOUT_DIRS.contains(&seg.as_str()) {
            return Some(seg.clone());
        }
    }
    None
}

/// Group proposals by their route segment.
///
/// Only groups with ≥ 2 proposals are returned, sorted by prefix.
pub fn group_by_route_prefix(proposals: &[ProposedEvent]) -> Vec<RouteGroup> {
    let mut map: HashMap<String, Vec<usize>> = HashMap::new();

    for (idx, proposal) in proposals.iter().enumerate() {
        if let Some(seg) = extract_route_segment(&proposal.source_path) {
            map.entry(seg).or_default().push(idx);
        }
    }

    let mut groups: Vec<RouteGroup> = map
        .into_iter()
        .filter(|(_, indices)| indices.len() >= 2)
        .map(|(prefix, mut indices)| {
            indices.sort_unstable();
            RouteGroup { prefix, indices }
        })
        .collect();

    groups.sort_by(|a, b| a.prefix.cmp(&b.prefix));
    groups
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::adapter::EventKind;
    use crate::ProposedEvent;

    use super::*;

    fn prop(name: &str, path: &str) -> ProposedEvent {
        ProposedEvent::new(name, EventKind::PageView, PathBuf::from(path), 0.9)
    }

    #[test]
    fn checkout_files_grouped() {
        let proposals = vec![
            prop("cart_viewed", "pages/checkout/cart.tsx"),
            prop("order_confirmed", "pages/checkout/confirm.tsx"),
        ];
        let groups = group_by_route_prefix(&proposals);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].prefix, "checkout");
        assert_eq!(groups[0].indices.len(), 2);
    }

    #[test]
    fn root_level_files_not_grouped() {
        let proposals = vec![
            prop("app_loaded", "App.tsx"),
            prop("index_viewed", "index.tsx"),
        ];
        let groups = group_by_route_prefix(&proposals);
        assert!(groups.is_empty());
    }

    #[test]
    fn single_file_prefix_excluded() {
        let proposals = vec![prop("onboarding_started", "app/onboarding/welcome.tsx")];
        let groups = group_by_route_prefix(&proposals);
        assert!(groups.is_empty());
    }

    #[test]
    fn strips_app_prefix() {
        let proposals = vec![
            prop("settings_profile_viewed", "app/settings/profile.ts"),
            prop("settings_account_viewed", "app/settings/account.ts"),
        ];
        let groups = group_by_route_prefix(&proposals);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].prefix, "settings");
    }

    #[test]
    fn strips_src_prefix() {
        let proposals = vec![
            prop("auth_login_viewed", "src/auth/login.ts"),
            prop("auth_signup_viewed", "src/auth/signup.ts"),
        ];
        let groups = group_by_route_prefix(&proposals);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].prefix, "auth");
    }

    #[test]
    fn short_segment_excluded() {
        // "ui" is 2 chars — too short
        let proposals = vec![
            prop("ev_a", "ui/button.tsx"),
            prop("ev_b", "ui/input.tsx"),
        ];
        let groups = group_by_route_prefix(&proposals);
        assert!(groups.is_empty());
    }

    #[test]
    fn multiple_prefixes_sorted() {
        let proposals = vec![
            prop("checkout_cart", "pages/checkout/cart.tsx"),
            prop("checkout_confirm", "pages/checkout/confirm.tsx"),
            prop("onboarding_start", "app/onboarding/start.tsx"),
            prop("onboarding_done", "app/onboarding/done.tsx"),
        ];
        let groups = group_by_route_prefix(&proposals);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].prefix, "checkout");
        assert_eq!(groups[1].prefix, "onboarding");
    }
}
