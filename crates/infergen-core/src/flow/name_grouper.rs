//! Name-prefix grouping for flow detection (E6.2).
//!
//! Groups proposals by shared event-name prefix and infers step order from
//! temporal suffix markers (started → viewed → submitted → completed).

use std::collections::HashMap;

use crate::ProposedEvent;

/// Temporal ordering ranks for name suffix substrings (lower = earlier step).
///
/// Each entry is `(substring_to_match, rank)`. The last `_`-separated segment
/// of the event name is checked against these substrings.
const TEMPORAL_RANKS: &[(&str, u32)] = &[
    ("start",   0),
    ("initiat", 0),
    ("begin",   0),
    ("open",    1),
    ("view",    2),
    ("load",    2),
    ("enter",   3),
    ("submit",  4),
    ("complet", 5),
    ("confirm", 5),
    ("success", 6),
    ("done",    6),
    ("fail",    99),
    ("cancel",  99),
    ("abandon", 99),
    ("error",   99),
];

/// A group of proposals sharing a common name prefix.
pub struct NameGroup {
    /// The shared prefix, e.g. `"checkout"`.
    pub prefix: String,
    /// Proposal indices in temporal order. Lexicographic when no temporal
    /// signal is present.
    pub ordered_indices: Vec<usize>,
    /// True when at least one proposal had a recognisable temporal suffix.
    pub has_order_signal: bool,
}

/// Extract the leading prefix from an event name by stripping the last
/// `_`-separated segment when it is a temporal marker.
///
/// ```
/// // "checkout_submitted"        → Some("checkout")
/// // "checkout_step_submitted"   → Some("checkout_step")
/// // "clicked"                   → None (no prefix remains)
/// // "page_viewed"               → Some("page")
/// ```
pub fn name_prefix(name: &str) -> Option<String> {
    let parts: Vec<&str> = name.split('_').collect();
    if parts.len() < 2 {
        return None;
    }
    let last = parts[parts.len() - 1].to_lowercase();
    let is_temporal = TEMPORAL_RANKS
        .iter()
        .any(|(marker, _)| last.contains(marker));

    let prefix_parts = if is_temporal {
        &parts[..parts.len() - 1]
    } else {
        // No temporal marker: use all-but-last as prefix to find groups like
        // "signup_email" + "signup_phone".
        &parts[..parts.len() - 1]
    };

    if prefix_parts.is_empty() {
        return None;
    }
    Some(prefix_parts.join("_"))
}

/// Assign a temporal rank to an event name (lower = earlier step).
///
/// Checks the last `_`-separated segment. Defaults to `50` (middle rank) when
/// no temporal marker is found.
pub fn temporal_rank(name: &str) -> u32 {
    let last = name.split('_').last().unwrap_or(name).to_lowercase();
    for (marker, rank) in TEMPORAL_RANKS {
        if last.contains(marker) {
            return *rank;
        }
    }
    50
}

/// Group proposals by common name prefix, ordered by temporal rank.
///
/// Only groups with ≥ 2 proposals are returned.
pub fn group_by_name_prefix(proposals: &[ProposedEvent]) -> Vec<NameGroup> {
    // Map prefix → Vec<(proposal_idx, rank)>
    let mut map: HashMap<String, Vec<(usize, u32)>> = HashMap::new();

    for (idx, proposal) in proposals.iter().enumerate() {
        if let Some(prefix) = name_prefix(&proposal.name) {
            let rank = temporal_rank(&proposal.name);
            map.entry(prefix).or_default().push((idx, rank));
        }
    }

    let mut groups: Vec<NameGroup> = map
        .into_iter()
        .filter(|(_, entries)| entries.len() >= 2)
        .map(|(prefix, mut entries)| {
            let has_order_signal = entries.iter().any(|(idx, _)| {
                let last = proposals[*idx]
                    .name
                    .split('_')
                    .last()
                    .unwrap_or("")
                    .to_lowercase();
                TEMPORAL_RANKS.iter().any(|(m, _)| last.contains(m))
            });

            if has_order_signal {
                entries.sort_by_key(|&(proposal_idx, rank)| {
                    (rank, proposals[proposal_idx].name.clone())
                });
            } else {
                entries.sort_by_key(|(proposal_idx, _)| proposals[*proposal_idx].name.clone());
            }

            let ordered_indices = entries.into_iter().map(|(idx, _)| idx).collect();
            NameGroup { prefix, ordered_indices, has_order_signal }
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

    fn prop(name: &str) -> ProposedEvent {
        ProposedEvent::new(name, EventKind::PageView, PathBuf::from("a.ts"), 0.9)
    }

    #[test]
    fn checkout_steps_ordered() {
        let proposals = vec![
            prop("checkout_completed"),
            prop("checkout_started"),
            prop("checkout_submitted"),
        ];
        let groups = group_by_name_prefix(&proposals);
        assert_eq!(groups.len(), 1);
        let g = &groups[0];
        assert_eq!(g.prefix, "checkout");
        assert!(g.has_order_signal);
        // Indices should be ordered: started(0) → submitted(4) → completed(5)
        let names: Vec<&str> = g
            .ordered_indices
            .iter()
            .map(|&i| proposals[i].name.as_str())
            .collect();
        assert_eq!(names, vec!["checkout_started", "checkout_submitted", "checkout_completed"]);
    }

    #[test]
    fn single_name_no_group() {
        let proposals = vec![prop("checkout_started"), prop("login_viewed")];
        let groups = group_by_name_prefix(&proposals);
        assert!(groups.is_empty());
    }

    #[test]
    fn names_without_temporal_sorted_alpha() {
        let proposals = vec![prop("signup_phone"), prop("signup_email")];
        let groups = group_by_name_prefix(&proposals);
        assert_eq!(groups.len(), 1);
        assert!(!groups[0].has_order_signal);
        let names: Vec<&str> = groups[0]
            .ordered_indices
            .iter()
            .map(|&i| proposals[i].name.as_str())
            .collect();
        // alphabetical: signup_email < signup_phone
        assert_eq!(names, vec!["signup_email", "signup_phone"]);
    }

    #[test]
    fn name_prefix_returns_none_for_single_segment() {
        assert!(name_prefix("clicked").is_none());
    }

    #[test]
    fn temporal_rank_fail_is_99() {
        assert_eq!(temporal_rank("checkout_failed"), 99);
    }

    #[test]
    fn temporal_rank_start_is_zero() {
        assert_eq!(temporal_rank("onboarding_started"), 0);
    }

    #[test]
    fn name_prefix_strips_temporal_suffix() {
        assert_eq!(name_prefix("checkout_submitted"), Some("checkout".into()));
        assert_eq!(name_prefix("checkout_step_submitted"), Some("checkout_step".into()));
    }

    #[test]
    fn name_prefix_returns_prefix_even_without_temporal() {
        assert_eq!(name_prefix("signup_email"), Some("signup".into()));
    }

    #[test]
    fn group_by_name_prefix_sorted() {
        let proposals = vec![
            prop("checkout_started"),
            prop("checkout_completed"),
            prop("onboarding_started"),
            prop("onboarding_completed"),
        ];
        let groups = group_by_name_prefix(&proposals);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].prefix, "checkout");
        assert_eq!(groups[1].prefix, "onboarding");
    }
}
