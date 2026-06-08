//! Monorepo detection and multi-package catalog management.
//!
//! Detects npm/pnpm workspaces, Cargo workspaces, and multi-manifest layouts,
//! then provides helpers to namespace and merge per-package catalogs and
//! to surface cross-service event-name inconsistencies.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use infergen_types::{Catalog, CatalogEntry, EventStatus, CATALOG_SCHEMA_VERSION};

use crate::detect::{Framework, Language, detect};
use crate::Error;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single package discovered inside a monorepo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonorepoPackage {
    /// Human-readable package name (from manifest `name` field or directory name).
    pub name: String,
    /// Absolute path to this package's root directory.
    pub root: PathBuf,
    /// Default catalog path: `{root}/.infergen/catalog.yaml`.
    pub catalog_path: PathBuf,
    /// Languages detected in this package (via [`detect`]).
    pub languages: Vec<Language>,
    /// Frameworks detected in this package (via [`detect`]).
    pub frameworks: Vec<Framework>,
}

/// Layout of a monorepo: root path and all discovered packages.
///
/// If `packages` is empty the root is a single-package project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonorepoLayout {
    /// Absolute path to the monorepo root.
    pub root: PathBuf,
    /// Sub-packages discovered in the monorepo. Empty for single-package projects.
    pub packages: Vec<MonorepoPackage>,
}

impl MonorepoLayout {
    /// `true` when more than one package is present.
    #[must_use]
    pub fn is_monorepo(&self) -> bool {
        self.packages.len() > 1
    }

    /// Number of distinct languages across all packages (union).
    #[must_use]
    pub fn language_count(&self) -> usize {
        let mut langs: Vec<u8> = self
            .packages
            .iter()
            .flat_map(|p| p.languages.iter().map(|l| *l as u8))
            .collect();
        langs.sort_unstable();
        langs.dedup();
        langs.len()
    }
}

// ---------------------------------------------------------------------------
// Nature of a cross-service inconsistency
// ---------------------------------------------------------------------------

/// Nature of a cross-service event-name inconsistency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsistencyKind {
    /// Same event name appears in multiple packages — may be intentional or accidental.
    DuplicateName,
    /// Same event name appears in multiple packages with different property sets.
    PropertyConflict,
}

/// A cross-service event consistency issue found by [`cross_service_check`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsistencyIssue {
    /// Event name that appears in more than one package.
    pub event_name: String,
    /// Packages where this event name is defined.
    pub packages: Vec<String>,
    /// Nature of the inconsistency.
    pub kind: ConsistencyKind,
}

// ---------------------------------------------------------------------------
// detect_monorepo
// ---------------------------------------------------------------------------

/// Detect packages inside a monorepo root.
///
/// Scans for:
/// 1. npm `package.json` `workspaces` field
/// 2. pnpm `pnpm-workspace.yaml` `packages` field
/// 3. Cargo `Cargo.toml` `[workspace] members`
/// 4. Multiple manifest files in immediate subdirectories (fallback)
///
/// # Errors
/// Returns [`Error::Io`] if `root` does not exist or is not a directory.
/// Individual sub-directory failures are silently skipped.
pub fn detect_monorepo(root: &Path) -> Result<MonorepoLayout, Error> {
    if !root.is_dir() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("not a directory: {}", root.display()),
        )));
    }

    let npm_dirs = detect_npm_workspace_dirs(root);
    let cargo_dirs = detect_cargo_workspace_dirs(root);

    // Multi-manifest fallback only when no explicit workspace config found.
    let manifest_dirs = if npm_dirs.is_empty() && cargo_dirs.is_empty() {
        detect_multi_manifest_dirs(root)
    } else {
        vec![]
    };

    // Union all dirs — order: npm/pnpm → Cargo → multi-manifest; dedup by canonical path.
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut package_dirs: Vec<PathBuf> = Vec::new();
    for dir in npm_dirs.into_iter().chain(cargo_dirs).chain(manifest_dirs) {
        if dir.is_dir() && seen.insert(dir.clone()) {
            package_dirs.push(dir);
        }
    }

    let mut used_names: HashMap<String, u32> = HashMap::new();
    let mut packages: Vec<MonorepoPackage> = Vec::new();

    for dir in package_dirs {
        let raw_name = package_name_from_dir(root, &dir);
        let count = used_names.entry(raw_name.clone()).or_insert(0);
        *count += 1;
        let name = if *count == 1 {
            raw_name
        } else {
            format!("{raw_name}-{count}")
        };

        let (languages, frameworks) = detect(&dir)
            .map(|r| (r.languages, r.frameworks))
            .unwrap_or_default();

        let catalog_path = dir.join(".infergen").join("catalog.yaml");
        packages.push(MonorepoPackage { name, root: dir, catalog_path, languages, frameworks });
    }

    Ok(MonorepoLayout { root: root.to_owned(), packages })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn read_opt(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn detect_npm_workspace_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    // npm package.json workspaces
    if let Some(text) = read_opt(&root.join("package.json")) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(patterns) = value.get("workspaces").and_then(|v| v.as_array()) {
                for p in patterns {
                    if let Some(pat) = p.as_str() {
                        dirs.extend(resolve_workspace_pattern(root, pat));
                    }
                }
            }
        }
    }

    // pnpm-workspace.yaml
    if let Some(text) = read_opt(&root.join("pnpm-workspace.yaml")) {
        #[derive(serde::Deserialize)]
        struct PnpmWorkspace {
            #[serde(default)]
            packages: Vec<String>,
        }
        if let Ok(ws) = serde_yaml::from_str::<PnpmWorkspace>(&text) {
            for pat in ws.packages {
                dirs.extend(resolve_workspace_pattern(root, &pat));
            }
        }
    }

    dirs
}

fn detect_cargo_workspace_dirs(root: &Path) -> Vec<PathBuf> {
    let text = match read_opt(&root.join("Cargo.toml")) {
        Some(t) => t,
        None => return vec![],
    };

    let value = match toml::from_str::<toml::Value>(&text) {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    let members = match value
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
    {
        Some(m) => m.clone(),
        None => return vec![],
    };

    members
        .iter()
        .filter_map(|m| m.as_str())
        .filter_map(|m| {
            // Only literal paths in v1; wildcard members not supported.
            let path = root.join(m);
            if path.is_dir() { Some(path) } else { None }
        })
        .collect()
}

fn detect_multi_manifest_dirs(root: &Path) -> Vec<PathBuf> {
    const MANIFESTS: &[&str] =
        &["package.json", "go.mod", "pyproject.toml", "Gemfile", "Cargo.toml"];

    let entries = match fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    let mut candidates: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter(|dir| MANIFESTS.iter().any(|m| dir.join(m).is_file()))
        .collect();

    // Only meaningful when ≥2 subdirs each have manifests.
    if candidates.len() < 2 {
        return vec![];
    }

    candidates.sort();
    candidates
}

fn resolve_workspace_pattern(root: &Path, pattern: &str) -> Vec<PathBuf> {
    let pattern = pattern.trim_start_matches("./");

    if let Some(prefix) = pattern.strip_suffix("/*") {
        let dir = root.join(prefix);
        if !dir.is_dir() {
            return vec![];
        }
        let mut result: Vec<PathBuf> = fs::read_dir(&dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        result.sort();
        result
    } else if !pattern.contains('*') {
        let path = root.join(pattern);
        if path.is_dir() { vec![path] } else { vec![] }
    } else {
        // Other glob patterns not supported in v1.
        vec![]
    }
}

fn package_name_from_dir(_root: &Path, dir: &Path) -> String {
    // Try package.json name.
    if let Some(text) = read_opt(&dir.join("package.json")) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
                if !name.is_empty() {
                    return name.to_owned();
                }
            }
        }
    }

    // Try Cargo.toml [package] name.
    if let Some(text) = read_opt(&dir.join("Cargo.toml")) {
        if let Ok(value) = toml::from_str::<toml::Value>(&text) {
            if let Some(name) = value
                .get("package")
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
            {
                if !name.is_empty() {
                    return name.to_owned();
                }
            }
        }
    }

    // Fallback: directory name relative to root (or bare name if not nested).
    dir.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_owned()
}

// ---------------------------------------------------------------------------
// namespace_catalog
// ---------------------------------------------------------------------------

/// Return a copy of `catalog` with every entry's `package` field set to `package_name`.
///
/// Does not modify the original. Any existing `package` value is overwritten.
#[must_use]
pub fn namespace_catalog(catalog: &Catalog, package_name: &str) -> Catalog {
    Catalog {
        schema_version: catalog.schema_version,
        events: catalog
            .events
            .iter()
            .map(|e| CatalogEntry {
                package: Some(package_name.to_owned()),
                ..e.clone()
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// merge_package_catalogs
// ---------------------------------------------------------------------------

/// Merge per-package catalogs into a single root-level catalog.
///
/// Each package catalog is namespaced with [`namespace_catalog`], then events
/// are concatenated, deduplicated by `(package, id)`, and sorted by
/// `(package_name, id)` for stable grouped diffs.
#[must_use]
pub fn merge_package_catalogs(packages: &[(&str, &Catalog)]) -> Catalog {
    let mut events: Vec<CatalogEntry> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();

    for (name, catalog) in packages {
        let namespaced = namespace_catalog(catalog, name);
        for entry in namespaced.events {
            let key = (name.to_string(), entry.id.clone());
            if seen.insert(key) {
                events.push(entry);
            }
        }
    }

    events.sort_by(|a, b| {
        let pa = a.package.as_deref().unwrap_or("");
        let pb = b.package.as_deref().unwrap_or("");
        pa.cmp(pb).then(a.id.cmp(&b.id))
    });

    Catalog { schema_version: CATALOG_SCHEMA_VERSION, events }
}

// ---------------------------------------------------------------------------
// cross_service_check
// ---------------------------------------------------------------------------

/// Check cross-service event consistency across multiple package catalogs.
///
/// Finds event names in two or more packages. For each duplicate:
/// - Identical property name sets → [`ConsistencyKind::DuplicateName`].
/// - Differing property sets → [`ConsistencyKind::PropertyConflict`].
///
/// `Ignored` events are excluded. Issues sorted by `event_name`.
#[must_use]
pub fn cross_service_check(packages: &[(&str, &Catalog)]) -> Vec<ConsistencyIssue> {
    // event_name → vec<(package_name, sorted_prop_names)>
    let mut map: HashMap<String, Vec<(String, Vec<String>)>> = HashMap::new();

    for (pkg_name, catalog) in packages {
        for entry in &catalog.events {
            if entry.status == EventStatus::Ignored {
                continue;
            }
            let mut prop_names: Vec<String> =
                entry.properties.iter().map(|p| p.name.clone()).collect();
            prop_names.sort_unstable();
            map.entry(entry.name.clone())
                .or_default()
                .push((pkg_name.to_string(), prop_names));
        }
    }

    let mut issues: Vec<ConsistencyIssue> = Vec::new();
    for (event_name, occurrences) in &map {
        if occurrences.len() < 2 {
            continue;
        }
        let pkg_names: Vec<String> = occurrences.iter().map(|(n, _)| n.clone()).collect();
        let all_props: Vec<&Vec<String>> = occurrences.iter().map(|(_, p)| p).collect();
        let all_same = all_props.windows(2).all(|w| w[0] == w[1]);
        let kind = if all_same {
            ConsistencyKind::DuplicateName
        } else {
            ConsistencyKind::PropertyConflict
        };
        issues.push(ConsistencyIssue { event_name: event_name.clone(), packages: pkg_names, kind });
    }
    issues.sort_by(|a, b| a.event_name.cmp(&b.event_name));
    issues
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use infergen_types::{
        CatalogEventKind, EventProperty, EventProvenance, EventStatus,
    };
    use tempfile::tempdir;

    fn write(dir: &Path, name: &str, contents: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, contents).unwrap();
    }

    fn make_entry(id: &str, name: &str, props: &[&str]) -> CatalogEntry {
        CatalogEntry {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            status: EventStatus::Proposed,
            confidence: 0.9,
            kind: CatalogEventKind::PageView,
            provenance: vec![EventProvenance {
                source_path: "src/app.ts".into(),
                line: None,
                adapter: "nextjs".into(),
            }],
            properties: props
                .iter()
                .map(|n| EventProperty {
                    name: n.to_string(),
                    prop_type: Some("string".into()),
                    required: true,
                    pii: false,
                })
                .collect(),
            providers: vec![],
            package: None,
        }
    }

    fn make_catalog(entries: Vec<CatalogEntry>) -> Catalog {
        Catalog { schema_version: CATALOG_SCHEMA_VERSION, events: entries }
    }

    // --- detect_monorepo ---

    #[test]
    fn detect_monorepo_non_dir_errors() {
        let err = detect_monorepo(Path::new("/no/such/path/xyz_infergen")).unwrap_err();
        assert!(matches!(err, Error::Io(_)));
    }

    #[test]
    fn detect_monorepo_single_package_json_no_workspaces() {
        let dir = tempdir().unwrap();
        write(dir.path(), "package.json", r#"{"name":"app","version":"1.0.0"}"#);
        let layout = detect_monorepo(dir.path()).unwrap();
        assert!(layout.packages.is_empty());
    }

    #[test]
    fn detect_monorepo_npm_workspaces_packages_star() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "package.json",
            r#"{"workspaces":["packages/*"]}"#,
        );
        write(dir.path(), "packages/frontend/package.json", r#"{"name":"frontend"}"#);
        write(dir.path(), "packages/backend/package.json", r#"{"name":"backend"}"#);
        let layout = detect_monorepo(dir.path()).unwrap();
        assert_eq!(layout.packages.len(), 2);
        let names: Vec<&str> = layout.packages.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"frontend"));
        assert!(names.contains(&"backend"));
    }

    #[test]
    fn detect_monorepo_pnpm_workspace_yaml() {
        let dir = tempdir().unwrap();
        write(dir.path(), "pnpm-workspace.yaml", "packages:\n  - apps/*\n");
        write(dir.path(), "apps/web/package.json", r#"{"name":"web"}"#);
        write(dir.path(), "apps/api/package.json", r#"{"name":"api"}"#);
        let layout = detect_monorepo(dir.path()).unwrap();
        assert_eq!(layout.packages.len(), 2);
    }

    #[test]
    fn detect_monorepo_cargo_workspace_members() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/a\", \"crates/b\"]\n",
        );
        write(dir.path(), "crates/a/Cargo.toml", "[package]\nname = \"crate-a\"\nversion = \"0.1.0\"\n");
        write(dir.path(), "crates/b/Cargo.toml", "[package]\nname = \"crate-b\"\nversion = \"0.1.0\"\n");
        let layout = detect_monorepo(dir.path()).unwrap();
        assert_eq!(layout.packages.len(), 2);
        let names: Vec<&str> = layout.packages.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"crate-a"));
        assert!(names.contains(&"crate-b"));
    }

    #[test]
    fn detect_monorepo_multi_manifest_fallback() {
        let dir = tempdir().unwrap();
        // No root workspace config — manifests in immediate subdirs.
        write(dir.path(), "gateway/go.mod", "module gateway\n");
        write(dir.path(), "worker/go.mod", "module worker\n");
        let layout = detect_monorepo(dir.path()).unwrap();
        assert_eq!(layout.packages.len(), 2);
    }

    #[test]
    fn detect_monorepo_empty_workspace_array() {
        let dir = tempdir().unwrap();
        write(dir.path(), "package.json", r#"{"workspaces":[]}"#);
        let layout = detect_monorepo(dir.path()).unwrap();
        assert!(layout.packages.is_empty());
    }

    #[test]
    fn is_monorepo_false_for_zero_packages() {
        let layout = MonorepoLayout { root: PathBuf::from("/tmp"), packages: vec![] };
        assert!(!layout.is_monorepo());
    }

    #[test]
    fn is_monorepo_true_for_two_packages() {
        let pkg = MonorepoPackage {
            name: "a".into(),
            root: PathBuf::from("/tmp/a"),
            catalog_path: PathBuf::from("/tmp/a/.infergen/catalog.yaml"),
            languages: vec![],
            frameworks: vec![],
        };
        let layout = MonorepoLayout {
            root: PathBuf::from("/tmp"),
            packages: vec![pkg.clone(), MonorepoPackage { name: "b".into(), ..pkg }],
        };
        assert!(layout.is_monorepo());
    }

    #[test]
    fn language_count_distinct() {
        let make_pkg = |name: &str, langs: Vec<Language>| MonorepoPackage {
            name: name.into(),
            root: PathBuf::from(format!("/tmp/{name}")),
            catalog_path: PathBuf::from(format!("/tmp/{name}/.infergen/catalog.yaml")),
            languages: langs,
            frameworks: vec![],
        };
        let layout = MonorepoLayout {
            root: PathBuf::from("/tmp"),
            packages: vec![
                make_pkg("a", vec![Language::TypeScript]),
                make_pkg("b", vec![Language::Python]),
            ],
        };
        assert_eq!(layout.language_count(), 2);
    }

    #[test]
    fn language_count_overlap() {
        let make_pkg = |name: &str| MonorepoPackage {
            name: name.into(),
            root: PathBuf::from(format!("/tmp/{name}")),
            catalog_path: PathBuf::from(format!("/tmp/{name}/.infergen/catalog.yaml")),
            languages: vec![Language::TypeScript],
            frameworks: vec![],
        };
        let layout = MonorepoLayout {
            root: PathBuf::from("/tmp"),
            packages: vec![make_pkg("a"), make_pkg("b")],
        };
        assert_eq!(layout.language_count(), 1);
    }

    // --- namespace_catalog ---

    #[test]
    fn namespace_catalog_sets_package_all_entries() {
        let catalog = make_catalog(vec![
            make_entry("evt_001", "page_viewed", &[]),
            make_entry("evt_002", "user_signed_in", &["email"]),
        ]);
        let namespaced = namespace_catalog(&catalog, "frontend");
        assert!(namespaced.events.iter().all(|e| e.package == Some("frontend".into())));
    }

    #[test]
    fn namespace_catalog_empty_catalog() {
        let catalog = make_catalog(vec![]);
        let namespaced = namespace_catalog(&catalog, "pkg");
        assert!(namespaced.events.is_empty());
    }

    #[test]
    fn namespace_catalog_original_unchanged() {
        let catalog = make_catalog(vec![make_entry("evt_001", "page_viewed", &[])]);
        let _namespaced = namespace_catalog(&catalog, "frontend");
        assert!(catalog.events[0].package.is_none());
    }

    // --- merge_package_catalogs ---

    #[test]
    fn merge_package_catalogs_two_packages() {
        let cat_a = make_catalog(vec![
            make_entry("evt_001", "page_viewed", &[]),
            make_entry("evt_002", "user_signed_in", &[]),
        ]);
        let cat_b = make_catalog(vec![
            make_entry("evt_003", "api_called", &[]),
        ]);
        let merged = merge_package_catalogs(&[("frontend", &cat_a), ("backend", &cat_b)]);
        assert_eq!(merged.events.len(), 3);
        assert!(merged.events.iter().all(|e| e.package.is_some()));
    }

    #[test]
    fn merge_package_catalogs_sorted_by_package_then_id() {
        let cat_b = make_catalog(vec![make_entry("evt_999", "b_event", &[])]);
        let cat_a = make_catalog(vec![make_entry("evt_001", "a_event", &[])]);
        let merged = merge_package_catalogs(&[("backend", &cat_b), ("alpha", &cat_a)]);
        // "alpha" < "backend" alphabetically
        assert_eq!(merged.events[0].package, Some("alpha".into()));
        assert_eq!(merged.events[1].package, Some("backend".into()));
    }

    #[test]
    fn merge_package_catalogs_deduplicates_same_id_within_package() {
        let entry = make_entry("evt_001", "page_viewed", &[]);
        let cat = make_catalog(vec![entry.clone(), entry]);
        let merged = merge_package_catalogs(&[("pkg", &cat)]);
        assert_eq!(merged.events.len(), 1);
    }

    #[test]
    fn merge_package_catalogs_empty_input() {
        let merged = merge_package_catalogs(&[]);
        assert!(merged.events.is_empty());
        assert_eq!(merged.schema_version, CATALOG_SCHEMA_VERSION);
    }

    // --- cross_service_check ---

    #[test]
    fn cross_service_check_no_conflicts() {
        let cat_a = make_catalog(vec![make_entry("evt_001", "page_viewed", &[])]);
        let cat_b = make_catalog(vec![make_entry("evt_002", "user_signed_in", &[])]);
        let issues = cross_service_check(&[("frontend", &cat_a), ("backend", &cat_b)]);
        assert!(issues.is_empty());
    }

    #[test]
    fn cross_service_check_duplicate_name_same_props() {
        let cat_a = make_catalog(vec![make_entry("evt_001", "user_signed_in", &["email"])]);
        let cat_b = make_catalog(vec![make_entry("evt_002", "user_signed_in", &["email"])]);
        let issues = cross_service_check(&[("svc_a", &cat_a), ("svc_b", &cat_b)]);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, ConsistencyKind::DuplicateName);
        assert_eq!(issues[0].event_name, "user_signed_in");
    }

    #[test]
    fn cross_service_check_property_conflict() {
        let cat_a = make_catalog(vec![make_entry("evt_001", "user_signed_in", &["email"])]);
        let cat_b = make_catalog(vec![make_entry("evt_002", "user_signed_in", &["user_id", "email"])]);
        let issues = cross_service_check(&[("svc_a", &cat_a), ("svc_b", &cat_b)]);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, ConsistencyKind::PropertyConflict);
    }

    #[test]
    fn cross_service_check_ignored_events_skipped() {
        let mut entry = make_entry("evt_001", "user_signed_in", &["email"]);
        entry.status = EventStatus::Ignored;
        let cat_a = make_catalog(vec![entry]);
        let cat_b = make_catalog(vec![make_entry("evt_002", "user_signed_in", &["email"])]);
        let issues = cross_service_check(&[("svc_a", &cat_a), ("svc_b", &cat_b)]);
        // Only one non-ignored occurrence — no conflict.
        assert!(issues.is_empty());
    }

    #[test]
    fn cross_service_check_sorted_by_event_name() {
        let cat_a = make_catalog(vec![
            make_entry("evt_001", "zebra_event", &["x"]),
            make_entry("evt_002", "alpha_event", &["y"]),
        ]);
        let cat_b = make_catalog(vec![
            make_entry("evt_003", "zebra_event", &["z"]),
            make_entry("evt_004", "alpha_event", &["w"]),
        ]);
        let issues = cross_service_check(&[("a", &cat_a), ("b", &cat_b)]);
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].event_name, "alpha_event");
        assert_eq!(issues[1].event_name, "zebra_event");
    }
}
