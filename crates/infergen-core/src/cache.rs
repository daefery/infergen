//! Incremental scan cache (E8.1).
//!
//! Stores per-file `(mtime, content_hash, proposals)` in
//! `.infergen/scan-cache.json`.  On the next `infergen scan`, files whose
//! mtime matches the cache entry are skipped entirely; only files modified
//! since the last scan are re-parsed and re-analysed.  This makes incremental
//! rescans sub-second on typical repos.
//!
//! Cache misses are safe: the cache always falls back to a fresh parse on any
//! error (missing file, parse error, version mismatch).

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};

use crate::adapter::ProposedEvent;

/// Version of the on-disk cache schema.  Increment on any breaking format change.
pub const CACHE_VERSION: u32 = 1;

/// Per-file cache entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheEntry {
    /// File mtime as seconds since the Unix epoch.
    pub modified_secs: u64,
    /// FNV-1a 64-bit hash of the file's byte content.  Used as secondary
    /// validation when mtime changes but content is identical (e.g. `touch`).
    pub content_hash: u64,
    /// Proposals produced by the last parse of this file.
    pub proposals: Vec<ProposedEvent>,
}

/// The full scan cache, serialised to `.infergen/scan-cache.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanCache {
    /// Schema version — always [`CACHE_VERSION`].
    pub version: u32,
    /// Entries keyed by normalised relative path (forward slashes, no leading slash).
    pub entries: HashMap<String, CacheEntry>,
}

impl Default for ScanCache {
    fn default() -> Self {
        ScanCache { version: CACHE_VERSION, entries: HashMap::new() }
    }
}

impl ScanCache {
    /// Look up a cache entry by normalised relative path.
    pub fn get(&self, rel_path: &str) -> Option<&CacheEntry> {
        self.entries.get(rel_path)
    }

    /// Insert or update a cache entry.
    pub fn insert(&mut self, rel_path: String, entry: CacheEntry) {
        self.entries.insert(rel_path, entry);
    }

    /// Number of cached file entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` when the cache has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Derive the cache file path from the catalog path.
///
/// Places the cache alongside the catalog; e.g.
/// `.infergen/catalog.yaml` → `.infergen/scan-cache.json`.
pub fn cache_path(catalog_path: &Path) -> PathBuf {
    catalog_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("scan-cache.json")
}

/// Load the cache from disk.
///
/// Returns an empty [`ScanCache`] on any I/O or parse error, and also when
/// the on-disk `version` does not match [`CACHE_VERSION`].  Cache misses are
/// always safe — the scanner falls back to a full re-parse.
pub fn load_cache(path: &Path) -> ScanCache {
    let Ok(text) = std::fs::read_to_string(path) else {
        return ScanCache::default();
    };
    match serde_json::from_str::<ScanCache>(&text) {
        Ok(c) if c.version == CACHE_VERSION => c,
        _ => ScanCache::default(),
    }
}

/// Persist the cache to disk.
///
/// Creates parent directories if they do not exist.
///
/// # Errors
/// - [`crate::Error::Io`] on file-system failure.
/// - [`crate::Error::CatalogParse`] (re-used) on JSON serialisation failure
///   (should be unreachable given the known type).
pub fn save_cache(cache: &ScanCache, path: &Path) -> crate::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let text = serde_json::to_string(cache).map_err(|e| crate::Error::CatalogParse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    std::fs::write(path, text)?;
    Ok(())
}

/// Return the mtime of `path` as seconds since the Unix epoch.
///
/// Returns `None` when the metadata cannot be read or the system clock
/// predates the epoch.
pub fn file_mtime(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// FNV-1a 64-bit hash of `data`.
///
/// Non-cryptographic; chosen for speed.  Used for secondary cache validation
/// (content-hash check after an mtime mismatch).  The same algorithm is used
/// for stable event IDs in `infergen-types`.
pub fn fnv1a_hash(data: &[u8]) -> u64 {
    const OFFSET: u64 = 14_695_981_039_346_656_037;
    const PRIME: u64 = 1_099_511_628_211;
    let mut h = OFFSET;
    for &byte in data {
        h ^= byte as u64;
        h = h.wrapping_mul(PRIME);
    }
    h
}

/// Compute the relative path of `path` under `root`, normalised with forward
/// slashes.
///
/// Falls back to the full path string when `path` is not under `root` (a
/// defensive fallback — normally all scanned files are under the project root).
pub fn normalize_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(mtime: u64, hash: u64) -> CacheEntry {
        CacheEntry { modified_secs: mtime, content_hash: hash, proposals: Vec::new() }
    }

    #[test]
    fn cache_version_is_one() {
        assert_eq!(CACHE_VERSION, 1);
    }

    #[test]
    fn default_cache_is_empty_and_versioned() {
        let c = ScanCache::default();
        assert!(c.is_empty());
        assert_eq!(c.version, CACHE_VERSION);
    }

    #[test]
    fn insert_and_get() {
        let mut c = ScanCache::default();
        c.insert("src/app.ts".into(), entry(1000, 42));
        let e = c.get("src/app.ts").unwrap();
        assert_eq!(e.modified_secs, 1000);
        assert_eq!(e.content_hash, 42);
    }

    #[test]
    fn get_missing_is_none() {
        let c = ScanCache::default();
        assert!(c.get("never.ts").is_none());
    }

    #[test]
    fn len_tracks_insertions() {
        let mut c = ScanCache::default();
        assert_eq!(c.len(), 0);
        c.insert("a.ts".into(), entry(1, 1));
        c.insert("b.ts".into(), entry(2, 2));
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn json_roundtrip() {
        let mut c = ScanCache::default();
        c.insert("src/auth.ts".into(), entry(999, 12345));
        let json = serde_json::to_string(&c).unwrap();
        let back: ScanCache = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn load_missing_file_returns_default() {
        let c = load_cache(Path::new("/no/such/path/scan-cache.json"));
        assert!(c.is_empty());
    }

    #[test]
    fn load_malformed_returns_default() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let path = dir.path().join("scan-cache.json");
        std::fs::write(&path, "not json at all").unwrap();
        assert!(load_cache(&path).is_empty());
    }

    #[test]
    fn load_version_mismatch_returns_default() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let path = dir.path().join("scan-cache.json");
        std::fs::write(
            &path,
            r#"{"version":99,"entries":{"a.ts":{"modified_secs":1,"content_hash":1,"proposals":[]}}}"#,
        ).unwrap();
        assert!(load_cache(&path).is_empty(), "version mismatch must reset cache");
    }

    #[test]
    fn save_and_load_roundtrip() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let path = dir.path().join(".infergen/scan-cache.json");
        let mut c = ScanCache::default();
        c.insert("src/index.ts".into(), entry(500, 9999));
        save_cache(&c, &path).unwrap();
        let loaded = load_cache(&path);
        assert_eq!(c, loaded);
    }

    #[test]
    fn save_creates_parent_dirs() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let path = dir.path().join("deep/nested/.infergen/scan-cache.json");
        save_cache(&ScanCache::default(), &path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn cache_path_alongside_catalog() {
        assert_eq!(
            cache_path(Path::new(".infergen/catalog.yaml")),
            PathBuf::from(".infergen/scan-cache.json")
        );
    }

    #[test]
    fn cache_path_root_catalog() {
        assert_eq!(cache_path(Path::new("catalog.yaml")), PathBuf::from("scan-cache.json"));
    }

    #[test]
    fn fnv1a_hash_empty_is_offset_basis() {
        assert_eq!(fnv1a_hash(b""), 14_695_981_039_346_656_037_u64);
    }

    #[test]
    fn fnv1a_hash_same_input_deterministic() {
        assert_eq!(fnv1a_hash(b"hello"), fnv1a_hash(b"hello"));
    }

    #[test]
    fn fnv1a_hash_different_inputs_differ() {
        assert_ne!(fnv1a_hash(b"hello"), fnv1a_hash(b"world"));
    }

    #[test]
    fn normalize_path_strips_root() {
        let root = Path::new("/home/user/project");
        let path = Path::new("/home/user/project/src/auth.ts");
        assert_eq!(normalize_path(path, root), "src/auth.ts");
    }

    #[test]
    fn normalize_path_no_backslashes() {
        let result = normalize_path(Path::new("src/auth/login.ts"), Path::new("."));
        assert!(!result.contains('\\'));
    }
}
