//! Integration tests for E8.1 — incremental scan cache.

use std::path::PathBuf;

use infergen_core::{
    CacheEntry, ScanCache, CACHE_VERSION,
    adapter::{EventKind, ProposedEvent},
    cache::{cache_path, fnv1a_hash, load_cache, normalize_path, save_cache},
};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn make_proposal(name: &str) -> ProposedEvent {
    ProposedEvent::new(name, EventKind::PageView, "src/app.ts", 0.9).with_adapter("nextjs")
}

fn entry_with_proposals(mtime: u64, hash: u64, names: &[&str]) -> CacheEntry {
    CacheEntry {
        modified_secs: mtime,
        content_hash: hash,
        proposals: names.iter().map(|n| make_proposal(n)).collect(),
    }
}

fn empty_entry(mtime: u64, hash: u64) -> CacheEntry {
    CacheEntry { modified_secs: mtime, content_hash: hash, proposals: Vec::new() }
}

// ---------------------------------------------------------------------------
// Version
// ---------------------------------------------------------------------------

#[test]
fn cache_version_constant_is_one() {
    assert_eq!(CACHE_VERSION, 1);
}

// ---------------------------------------------------------------------------
// cache_path
// ---------------------------------------------------------------------------

#[test]
fn cache_path_alongside_catalog() {
    assert_eq!(
        cache_path(&PathBuf::from(".infergen/catalog.yaml")),
        PathBuf::from(".infergen/scan-cache.json")
    );
}

#[test]
fn cache_path_root_level_catalog() {
    assert_eq!(
        cache_path(&PathBuf::from("catalog.yaml")),
        PathBuf::from("scan-cache.json")
    );
}

// ---------------------------------------------------------------------------
// ScanCache operations
// ---------------------------------------------------------------------------

#[test]
fn insert_and_get_roundtrip() {
    let mut c = ScanCache::default();
    c.insert("src/app.ts".into(), empty_entry(1000, 42));
    let e = c.get("src/app.ts").unwrap();
    assert_eq!(e.modified_secs, 1000);
    assert_eq!(e.content_hash, 42);
}

#[test]
fn insert_overwrites_existing() {
    let mut c = ScanCache::default();
    c.insert("src/app.ts".into(), entry_with_proposals(100, 1, &["old_event"]));
    c.insert("src/app.ts".into(), entry_with_proposals(200, 2, &["new_event"]));
    let e = c.get("src/app.ts").unwrap();
    assert_eq!(e.modified_secs, 200);
    assert_eq!(e.proposals[0].name, "new_event");
}

#[test]
fn get_missing_key_returns_none() {
    assert!(ScanCache::default().get("nonexistent.ts").is_none());
}

#[test]
fn len_and_is_empty() {
    let mut c = ScanCache::default();
    assert!(c.is_empty());
    assert_eq!(c.len(), 0);
    c.insert("a.ts".into(), empty_entry(1, 1));
    assert!(!c.is_empty());
    assert_eq!(c.len(), 1);
    c.insert("b.ts".into(), empty_entry(2, 2));
    assert_eq!(c.len(), 2);
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

#[test]
fn save_and_reload_empty_cache() {
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    let path = dir.path().join("scan-cache.json");
    save_cache(&ScanCache::default(), &path).unwrap();
    let loaded = load_cache(&path);
    assert!(loaded.is_empty());
    assert_eq!(loaded.version, CACHE_VERSION);
}

#[test]
fn save_and_reload_with_proposals() {
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    let path = dir.path().join(".infergen/scan-cache.json");
    let mut c = ScanCache::default();
    c.insert(
        "src/auth.ts".into(),
        entry_with_proposals(1_000, 42, &["user_signed_in", "user_signed_out"]),
    );
    save_cache(&c, &path).unwrap();
    let loaded = load_cache(&path);
    let entry = loaded.get("src/auth.ts").unwrap();
    assert_eq!(entry.proposals.len(), 2);
    assert_eq!(entry.proposals[0].name, "user_signed_in");
    assert_eq!(entry.proposals[1].name, "user_signed_out");
}

#[test]
fn save_creates_parent_directories() {
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    let path = dir.path().join("a/b/c/.infergen/scan-cache.json");
    save_cache(&ScanCache::default(), &path).unwrap();
    assert!(path.exists());
}

#[test]
fn load_missing_file_returns_empty() {
    let c = load_cache(std::path::Path::new("/no/such/path/scan-cache.json"));
    assert!(c.is_empty());
}

#[test]
fn load_malformed_json_returns_empty() {
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    let path = dir.path().join("scan-cache.json");
    std::fs::write(&path, "{ this is not valid JSON }").unwrap();
    assert!(load_cache(&path).is_empty());
}

#[test]
fn load_version_mismatch_resets_cache() {
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    let path = dir.path().join("scan-cache.json");
    std::fs::write(
        &path,
        r#"{"version":99,"entries":{"a.ts":{"modified_secs":1,"content_hash":1,"proposals":[]}}}"#,
    ).unwrap();
    let c = load_cache(&path);
    assert!(c.is_empty(), "version mismatch must produce empty cache");
}

// ---------------------------------------------------------------------------
// JSON roundtrip (proposals survive serialization)
// ---------------------------------------------------------------------------

#[test]
fn proposals_survive_json_roundtrip() {
    let original = entry_with_proposals(500, 9876, &["page_viewed", "button_clicked"]);
    let json = serde_json::to_string(&original).unwrap();
    let back: CacheEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(back.modified_secs, 500);
    assert_eq!(back.content_hash, 9876);
    assert_eq!(back.proposals.len(), 2);
    assert_eq!(back.proposals[1].name, "button_clicked");
}

// ---------------------------------------------------------------------------
// fnv1a_hash
// ---------------------------------------------------------------------------

#[test]
fn fnv1a_hash_empty_input_is_offset_basis() {
    assert_eq!(fnv1a_hash(b""), 14_695_981_039_346_656_037_u64);
}

#[test]
fn fnv1a_hash_is_deterministic() {
    assert_eq!(fnv1a_hash(b"src/app.ts"), fnv1a_hash(b"src/app.ts"));
}

#[test]
fn fnv1a_hash_differs_for_different_content() {
    assert_ne!(fnv1a_hash(b"hello"), fnv1a_hash(b"world"));
}

#[test]
fn fnv1a_hash_collision_resistance_on_filenames() {
    let filenames = [
        "src/app.ts",
        "src/auth.ts",
        "pages/index.tsx",
        "components/Nav.tsx",
        "lib/utils.ts",
        "hooks/useUser.ts",
    ];
    let hashes: Vec<u64> = filenames.iter().map(|s| fnv1a_hash(s.as_bytes())).collect();
    let unique: std::collections::HashSet<u64> = hashes.iter().cloned().collect();
    assert_eq!(unique.len(), hashes.len(), "all filename hashes must be unique");
}

// ---------------------------------------------------------------------------
// normalize_path
// ---------------------------------------------------------------------------

#[test]
fn normalize_path_strips_root_prefix() {
    let root = std::path::Path::new("/home/user/project");
    let file = std::path::Path::new("/home/user/project/src/auth/login.ts");
    assert_eq!(normalize_path(file, root), "src/auth/login.ts");
}

#[test]
fn normalize_path_no_backslashes_in_result() {
    let result = normalize_path(
        std::path::Path::new("src/auth/login.ts"),
        std::path::Path::new("."),
    );
    assert!(!result.contains('\\'));
}

#[test]
fn normalize_path_fallback_when_no_prefix_match() {
    let root = std::path::Path::new("/other/root");
    let file = std::path::Path::new("/home/user/project/src/app.ts");
    let rel = normalize_path(file, root);
    assert!(rel.contains("app.ts"), "fallback should still include the filename");
}
