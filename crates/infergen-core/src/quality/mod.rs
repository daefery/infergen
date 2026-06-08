//! Suggestion quality loop (E6.3).
//!
//! Persists review decisions (approve/ignore) to `.infergen/quality.yaml` and
//! derives two tuning signals for the next scan:
//!
//! 1. **Confidence multipliers** — aggregates per `(adapter, kind)` approval
//!    rate and returns a float to multiply future proposals' confidence by.
//! 2. **Name hints** — returns the most-recently approved event name for a
//!    given `(adapter, kind, source_path)` so recurring patterns keep their
//!    user-validated names instead of re-running heuristics.
//!
//! All I/O errors are propagated via [`crate::Error`]; callers in the CLI
//! treat them as soft failures (log + continue).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Error;

// ---------------------------------------------------------------------------
// Schema version
// ---------------------------------------------------------------------------

/// Version of the on-disk quality store (`quality.yaml`) schema.
pub const QUALITY_SCHEMA_VERSION: u32 = 1;

fn default_quality_version() -> u32 {
    QUALITY_SCHEMA_VERSION
}

// ---------------------------------------------------------------------------
// Review action
// ---------------------------------------------------------------------------

/// Whether a review decision was an approval or an explicit ignore.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FeedbackAction {
    /// Human approved this event — it will be tracked.
    Approved,
    /// Human marked this event as a false positive — it will not be tracked.
    Ignored,
}

// ---------------------------------------------------------------------------
// Feedback entry
// ---------------------------------------------------------------------------

/// A single review decision, persisted in `quality.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackEntry {
    /// Stable catalog ID (`evt_{016hex}`).
    pub event_id: String,
    /// Event name at review time (after any renames the user applied).
    pub event_name: String,
    /// Whether the user approved or ignored the event.
    pub action: FeedbackAction,
    /// Adapter that proposed the event, e.g. `"nextjs"`.
    pub adapter: String,
    /// Event kind in camelCase, e.g. `"pageView"`.
    pub kind: String,
    /// Source file path relative to the project root.
    pub source_path: String,
    /// Adapter confidence at review time (`0.0`–`1.0`).
    pub confidence_at_review: f64,
}

// ---------------------------------------------------------------------------
// Feedback store
// ---------------------------------------------------------------------------

/// Minimum number of `(adapter, kind)` entries before confidence adjustment
/// is applied. Below this threshold `confidence_multiplier` returns `1.0`.
const MIN_SAMPLE: usize = 3;

/// The `.infergen/quality.yaml` document.
///
/// Stores all review decisions. `Default` gives an empty, ready-to-use store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackStore {
    /// Schema version (always [`QUALITY_SCHEMA_VERSION`]).
    #[serde(default = "default_quality_version")]
    pub schema_version: u32,
    /// All recorded review decisions, in insertion order.
    #[serde(default)]
    pub entries: Vec<FeedbackEntry>,
}

impl Default for FeedbackStore {
    fn default() -> Self {
        FeedbackStore {
            schema_version: QUALITY_SCHEMA_VERSION,
            entries: Vec::new(),
        }
    }
}

impl FeedbackStore {
    /// Load from `path`.
    ///
    /// If `path` does not exist, returns an empty default store — this is
    /// normal for new projects that have not yet reviewed any events.
    ///
    /// # Errors
    /// [`Error::CatalogParse`] if the file exists but cannot be parsed.
    /// [`Error::Io`] for other I/O failures (e.g. permission denied).
    pub fn load(path: &Path) -> Result<Self, Error> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_yaml::from_str::<Self>(&text).map_err(|e| Error::CatalogParse {
                path: path.to_path_buf(),
                message: e.to_string(),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(Error::Io(e)),
        }
    }

    /// Persist to `path`, creating parent directories as needed.
    ///
    /// # Errors
    /// [`Error::CatalogParse`] on serialization failure.
    /// [`Error::Io`] on write failure.
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_yaml::to_string(self).map_err(|e| Error::CatalogParse {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        std::fs::write(path, text)?;
        Ok(())
    }

    /// Append `entry` to the store.
    pub fn record(&mut self, entry: FeedbackEntry) {
        self.entries.push(entry);
    }

    /// Confidence multiplier for proposals from `(adapter, kind)`.
    ///
    /// Returns `1.0` if fewer than [`MIN_SAMPLE`] entries exist.
    /// Otherwise derives a multiplier from the historical approval rate:
    /// ```text
    /// multiplier = 0.75 + 0.35 * approval_rate
    /// ```
    /// Range: `[0.75, 1.10]`. The upper bound exceeds 1.0 — the caller is
    /// expected to clamp `proposal.confidence` to `1.0` after multiplying.
    #[must_use]
    pub fn confidence_multiplier(&self, adapter: &str, kind: &str) -> f64 {
        let relevant: Vec<&FeedbackEntry> = self
            .entries
            .iter()
            .filter(|e| e.adapter == adapter && e.kind == kind)
            .collect();

        if relevant.len() < MIN_SAMPLE {
            return 1.0;
        }

        let n_approved = relevant.iter().filter(|e| e.action == FeedbackAction::Approved).count();
        let rate = n_approved as f64 / relevant.len() as f64;
        (0.75_f64 + 0.35_f64 * rate).clamp(0.5, 1.1)
    }

    /// Most-recently approved event name for `(adapter, kind, source_path)`.
    ///
    /// Scans entries in reverse insertion order and returns the first match.
    /// Only `Approved` entries are considered — `Ignored` decisions do not
    /// contribute name hints.
    ///
    /// Returns `None` if no matching approved entry exists.
    #[must_use]
    pub fn name_hint(&self, adapter: &str, kind: &str, source_path: &str) -> Option<String> {
        self.entries
            .iter()
            .rev()
            .find(|e| {
                e.action == FeedbackAction::Approved
                    && e.adapter == adapter
                    && e.kind == kind
                    && e.source_path == source_path
            })
            .map(|e| e.event_name.clone())
    }
}

// ---------------------------------------------------------------------------
// Path helper
// ---------------------------------------------------------------------------

/// Derive `quality.yaml` path from the catalog path.
///
/// Places the quality store in the same directory as the catalog:
/// `.infergen/catalog.yaml` → `.infergen/quality.yaml`.
#[must_use]
pub fn quality_path(catalog_path: &Path) -> PathBuf {
    catalog_path.with_file_name("quality.yaml")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn approved(adapter: &str, kind: &str, source: &str, name: &str) -> FeedbackEntry {
        FeedbackEntry {
            event_id: "evt_abc".into(),
            event_name: name.into(),
            action: FeedbackAction::Approved,
            adapter: adapter.into(),
            kind: kind.into(),
            source_path: source.into(),
            confidence_at_review: 0.8,
        }
    }

    fn ignored(adapter: &str, kind: &str) -> FeedbackEntry {
        FeedbackEntry {
            event_id: "evt_def".into(),
            event_name: "ignored_event".into(),
            action: FeedbackAction::Ignored,
            adapter: adapter.into(),
            kind: kind.into(),
            source_path: "src/foo.ts".into(),
            confidence_at_review: 0.5,
        }
    }

    // --- FeedbackStore basics -----------------------------------------------

    #[test]
    fn default_store_has_no_entries() {
        let s = FeedbackStore::default();
        assert!(s.entries.is_empty());
        assert_eq!(s.schema_version, QUALITY_SCHEMA_VERSION);
    }

    #[test]
    fn record_adds_entry() {
        let mut s = FeedbackStore::default();
        s.record(approved("nextjs", "pageView", "src/pages/index.tsx", "home_page_viewed"));
        assert_eq!(s.entries.len(), 1);
    }

    // --- confidence_multiplier ----------------------------------------------

    #[test]
    fn confidence_multiplier_below_min_sample_returns_one() {
        let mut s = FeedbackStore::default();
        // Only 2 entries — below MIN_SAMPLE (3).
        s.record(approved("nextjs", "pageView", "src/a.tsx", "a_viewed"));
        s.record(approved("nextjs", "pageView", "src/b.tsx", "b_viewed"));
        assert!((s.confidence_multiplier("nextjs", "pageView") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn confidence_multiplier_all_approved_boosts() {
        let mut s = FeedbackStore::default();
        for i in 0..5 {
            s.record(approved("nextjs", "pageView", &format!("src/{i}.tsx"), "x_viewed"));
        }
        let m = s.confidence_multiplier("nextjs", "pageView");
        assert!(m > 1.0, "all approved should boost: got {m}");
    }

    #[test]
    fn confidence_multiplier_all_ignored_reduces() {
        let mut s = FeedbackStore::default();
        for _ in 0..5 {
            s.record(ignored("nextjs", "buttonClick"));
        }
        let m = s.confidence_multiplier("nextjs", "buttonClick");
        assert!(m < 1.0, "all ignored should reduce: got {m}");
        assert!((m - 0.75).abs() < 1e-9, "expected 0.75, got {m}");
    }

    #[test]
    fn confidence_multiplier_mixed_rates_interpolates() {
        let mut s = FeedbackStore::default();
        // 3 approved, 1 ignored → rate = 0.75
        for i in 0..3 {
            s.record(approved("nextjs", "formSubmit", &format!("src/{i}.tsx"), "form_submitted"));
        }
        s.record(ignored("nextjs", "formSubmit"));
        let m = s.confidence_multiplier("nextjs", "formSubmit");
        // expected: 0.75 + 0.35 * 0.75 = 0.75 + 0.2625 = 1.0125
        let expected = 0.75 + 0.35 * 0.75;
        assert!((m - expected).abs() < 1e-9, "expected {expected}, got {m}");
    }

    #[test]
    fn confidence_multiplier_ignores_other_adapters() {
        let mut s = FeedbackStore::default();
        for _ in 0..5 {
            s.record(ignored("fastapi", "apiCall"));
        }
        // nextjs/apiCall has no entries → should return 1.0
        assert!((s.confidence_multiplier("nextjs", "apiCall") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn confidence_multiplier_ignores_other_kinds() {
        let mut s = FeedbackStore::default();
        for _ in 0..5 {
            s.record(ignored("nextjs", "authEvent"));
        }
        assert!((s.confidence_multiplier("nextjs", "pageView") - 1.0).abs() < 1e-9);
    }

    // --- name_hint ----------------------------------------------------------

    #[test]
    fn name_hint_returns_most_recent_approved() {
        let mut s = FeedbackStore::default();
        s.record(approved("nextjs", "pageView", "src/pages/index.tsx", "old_name"));
        s.record(approved("nextjs", "pageView", "src/pages/index.tsx", "new_name"));
        assert_eq!(
            s.name_hint("nextjs", "pageView", "src/pages/index.tsx"),
            Some("new_name".into())
        );
    }

    #[test]
    fn name_hint_ignored_action_not_returned() {
        let mut s = FeedbackStore::default();
        s.record(ignored("nextjs", "pageView"));
        // The ignored entry's source_path is "src/foo.ts"
        assert_eq!(s.name_hint("nextjs", "pageView", "src/foo.ts"), None);
    }

    #[test]
    fn name_hint_no_match_returns_none() {
        let s = FeedbackStore::default();
        assert!(s.name_hint("nextjs", "pageView", "src/pages/index.tsx").is_none());
    }

    #[test]
    fn name_hint_cross_adapter_not_matched() {
        let mut s = FeedbackStore::default();
        s.record(approved("nextjs", "pageView", "src/pages/index.tsx", "home_viewed"));
        assert!(s.name_hint("fastapi", "pageView", "src/pages/index.tsx").is_none());
    }

    // --- quality_path -------------------------------------------------------

    #[test]
    fn quality_path_same_dir_as_catalog() {
        let p = quality_path(Path::new(".infergen/catalog.yaml"));
        assert_eq!(p, PathBuf::from(".infergen/quality.yaml"));
    }

    #[test]
    fn quality_path_preserves_parent() {
        let p = quality_path(Path::new("/project/.infergen/catalog.yaml"));
        assert_eq!(p, PathBuf::from("/project/.infergen/quality.yaml"));
    }

    // --- save / load roundtrip ----------------------------------------------

    #[test]
    fn save_load_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("quality.yaml");
        let mut s = FeedbackStore::default();
        s.record(approved("nextjs", "pageView", "src/pages/index.tsx", "home_page_viewed"));
        s.record(ignored("nextjs", "buttonClick"));
        s.save(&path).unwrap();
        let loaded = FeedbackStore::load(&path).unwrap();
        assert_eq!(loaded, s);
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("quality.yaml");
        let s = FeedbackStore::load(&path).unwrap();
        assert_eq!(s, FeedbackStore::default());
    }
}
