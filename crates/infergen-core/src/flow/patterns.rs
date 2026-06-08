//! Known flow pattern definitions and proposal-level matching (E6.2).

use infergen_types::FlowKind;

use crate::ProposedEvent;

/// A template for a well-known multi-step funnel.
pub struct FlowPattern {
    /// Human-readable funnel name, e.g. `"checkout"`.
    pub name: &'static str,
    /// Category assigned to flows that match this pattern.
    pub kind: FlowKind,
    /// Lowercase keywords; a proposal matches if its name contains any of them.
    pub triggers: &'static [&'static str],
    /// Minimum number of proposals that must match for the pattern to fire.
    pub min_matches: usize,
}

/// All built-in funnel templates, ordered by specificity (checkout first so
/// it beats the more generic payment pattern on overlapping proposals).
pub const KNOWN_PATTERNS: &[FlowPattern] = &[
    FlowPattern {
        name: "checkout",
        kind: FlowKind::Checkout,
        triggers: &[
            "cart",
            "checkout",
            "shipping",
            "order",
            "confirm",
            "purchase",
            "place_order",
        ],
        min_matches: 2,
    },
    FlowPattern {
        name: "onboarding",
        kind: FlowKind::Onboarding,
        triggers: &[
            "onboard",
            "setup",
            "welcome",
            "tour",
            "tutorial",
            "getting_started",
            "first_run",
            "profile_create",
            "invite",
        ],
        min_matches: 2,
    },
    FlowPattern {
        name: "auth",
        kind: FlowKind::Auth,
        triggers: &[
            "login",
            "signin",
            "signup",
            "register",
            "logout",
            "signout",
            "verify_email",
            "confirm_email",
            "otp",
            "two_factor",
            "magic_link",
        ],
        min_matches: 2,
    },
    FlowPattern {
        name: "payment",
        kind: FlowKind::Payment,
        triggers: &[
            "pay",
            "subscription",
            "plan",
            "invoice",
            "billing",
            "charge",
            "refund",
            "stripe",
            "upgrade",
            "downgrade",
        ],
        min_matches: 2,
    },
    FlowPattern {
        name: "search",
        kind: FlowKind::Search,
        triggers: &[
            "search",
            "query",
            "filter",
            "result",
            "suggest",
            "autocomplete",
        ],
        min_matches: 2,
    },
];

/// A proposal matched to a known flow pattern.
pub struct PatternMatch {
    /// Index into the proposals slice.
    pub proposal_idx: usize,
    /// Which pattern matched.
    pub pattern: &'static FlowPattern,
}

/// Match each proposal's name against [`KNOWN_PATTERNS`].
///
/// A proposal matches a pattern if its lowercase name contains at least one
/// trigger keyword. A proposal can match multiple patterns. Only patterns with
/// at least `min_matches` matching proposals are included in the output.
pub fn match_known_patterns(proposals: &[ProposedEvent]) -> Vec<PatternMatch> {
    let mut pattern_hits: Vec<Vec<usize>> = vec![vec![]; KNOWN_PATTERNS.len()];

    for (idx, proposal) in proposals.iter().enumerate() {
        let lower = proposal.name.to_lowercase();
        for (pat_idx, pattern) in KNOWN_PATTERNS.iter().enumerate() {
            if pattern.triggers.iter().any(|t| lower.contains(t)) {
                pattern_hits[pat_idx].push(idx);
            }
        }
    }

    let mut out = Vec::new();
    for (pat_idx, hits) in pattern_hits.iter().enumerate() {
        if hits.len() >= KNOWN_PATTERNS[pat_idx].min_matches {
            for &proposal_idx in hits {
                out.push(PatternMatch {
                    proposal_idx,
                    pattern: &KNOWN_PATTERNS[pat_idx],
                });
            }
        }
    }
    out
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
    fn checkout_pattern_matches_cart_and_confirm() {
        let proposals = vec![prop("cart_viewed"), prop("order_confirmed")];
        let matches = match_known_patterns(&proposals);
        let checkout_hits: Vec<_> = matches.iter().filter(|m| m.pattern.name == "checkout").collect();
        assert_eq!(checkout_hits.len(), 2);
    }

    #[test]
    fn onboarding_below_min_matches_returns_empty() {
        let proposals = vec![prop("welcome_viewed")];
        let matches = match_known_patterns(&proposals);
        assert!(matches.is_empty());
    }

    #[test]
    fn auth_pattern_matches_login_and_signup() {
        let proposals = vec![prop("user_signed_in"), prop("user_signup_completed")];
        let matches = match_known_patterns(&proposals);
        // "signed_in" → contains "signin"? No. "login"? No. Actually "signed_in" doesn't match.
        // Let me re-check: triggers for auth include "login", "signin", "signup", "register" etc.
        // "user_signed_in" — doesn't contain any exact trigger (signed_in ≠ signin)
        // "user_signup_completed" — contains "signup" ✓
        // So only 1 match → below min_matches=2 → empty
        // Let's use proper names instead
        let _ = matches; // reset
        let proposals2 = vec![prop("user_login_completed"), prop("user_signup_completed")];
        let matches2 = match_known_patterns(&proposals2);
        let auth_hits: Vec<_> = matches2.iter().filter(|m| m.pattern.name == "auth").collect();
        assert_eq!(auth_hits.len(), 2);
    }

    #[test]
    fn proposal_can_match_multiple_patterns() {
        // checkout: "cart_viewed" (cart) + "checkout_confirmed" (checkout) → 2 matches ✓
        // payment:  "stripe_charge_created" (stripe) + "invoice_billing_viewed" (invoice,billing) → 2 matches ✓
        // Note: single proposal can match multiple patterns (proposal 0 or 1 may also hit payment).
        let proposals = vec![
            prop("cart_viewed"),              // checkout (cart)
            prop("checkout_confirmed"),        // checkout (checkout)
            prop("stripe_charge_created"),     // payment (stripe, charge)
            prop("invoice_billing_viewed"),    // payment (invoice, billing)
        ];
        let matches = match_known_patterns(&proposals);
        let matched_patterns: std::collections::HashSet<&str> =
            matches.iter().map(|m| m.pattern.name).collect();
        assert!(matched_patterns.contains("checkout"), "checkout must fire");
        assert!(matched_patterns.contains("payment"), "payment must fire");
    }

    #[test]
    fn no_proposals_returns_empty() {
        let matches = match_known_patterns(&[]);
        assert!(matches.is_empty());
    }

    #[test]
    fn search_pattern_fires_on_two_search_events() {
        let proposals = vec![prop("search_query_submitted"), prop("search_results_viewed")];
        let matches = match_known_patterns(&proposals);
        let search_hits: Vec<_> = matches.iter().filter(|m| m.pattern.name == "search").collect();
        assert_eq!(search_hits.len(), 2);
    }
}
