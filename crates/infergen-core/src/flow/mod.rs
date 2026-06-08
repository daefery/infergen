//! Semantic flow detection — multi-step funnel discovery (E6.2).
//!
//! [`FlowDetector`] runs a post-scan pass over all proposed events and groups
//! related events into named funnels (checkout, onboarding, auth, …) using:
//!
//! 1. **Route grouping** — proposals whose source files share a path prefix
//!    (e.g. `pages/checkout/`) are grouped.
//! 2. **Name grouping** — proposals whose event names share a leading prefix
//!    and have temporal-order suffixes are grouped and ordered.
//! 3. **Known patterns** — well-known funnel templates (checkout, auth, …)
//!    overlay the heuristic groups with higher confidence and richer metadata.
//!
//! An optional [`SemanticRefiner`] (E6.1 plug-in point) can post-process the
//! detected flows with an LLM pass when available.

use std::collections::HashMap;

use infergen_types::FlowKind;

use crate::ProposedEvent;
use self::name_grouper::group_by_name_prefix;
use self::patterns::{KNOWN_PATTERNS, match_known_patterns};
use self::refiner::{NoOpRefiner, SemanticRefiner};
use self::route_grouper::group_by_route_prefix;

pub mod name_grouper;
pub mod patterns;
pub mod refiner;
pub mod route_grouper;

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

/// A step in a detected flow, referencing a proposal by slice index.
#[derive(Debug, Clone)]
pub struct DetectedStep {
    /// Index into the `proposals` slice passed to [`FlowDetector::detect`].
    pub proposal_idx: usize,
    /// Zero-based step index within the flow.
    pub step_index: u32,
}

/// A funnel detected across proposals, before catalog assignment.
#[derive(Debug, Clone)]
pub struct DetectedFlow {
    /// Human-readable name derived from prefix or pattern.
    pub name: String,
    /// Category of funnel.
    pub kind: FlowKind,
    /// Detection confidence: pattern match ≥ 0.85, route 0.75, name-only 0.60.
    pub confidence: f32,
    /// Ordered steps.
    pub steps: Vec<DetectedStep>,
}

// ---------------------------------------------------------------------------
// FlowDetector
// ---------------------------------------------------------------------------

/// Detects multi-step funnels from a flat list of proposed events.
pub struct FlowDetector {
    refiner: Box<dyn SemanticRefiner>,
}

impl Default for FlowDetector {
    fn default() -> Self {
        Self { refiner: Box::new(NoOpRefiner) }
    }
}

impl FlowDetector {
    /// Create a detector with the default no-op refiner.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a detector with a custom semantic refiner (e.g. Ollama from E6.1).
    pub fn with_refiner(refiner: Box<dyn SemanticRefiner>) -> Self {
        Self { refiner }
    }

    /// Detect flows from `proposals`.
    ///
    /// Returns flows with ≥ 2 steps and confidence ≥ 0.5, sorted by name for
    /// deterministic output.
    pub fn detect(&self, proposals: &[ProposedEvent]) -> Vec<DetectedFlow> {
        // Candidate flows keyed by a sorted set of proposal indices so we can
        // dedup overlapping groups later.
        let mut candidates: HashMap<Vec<usize>, DetectedFlow> = HashMap::new();

        // 1. Route-based candidates (confidence 0.75).
        for group in group_by_route_prefix(proposals) {
            let key = group.indices.clone();
            let flow = DetectedFlow {
                name: group.prefix.clone(),
                kind: FlowKind::Custom,
                confidence: 0.75,
                steps: key
                    .iter()
                    .enumerate()
                    .map(|(i, &idx)| DetectedStep { proposal_idx: idx, step_index: i as u32 })
                    .collect(),
            };
            candidates.entry(key).or_insert(flow);
        }

        // 2. Name-based candidates (confidence 0.60).
        for group in group_by_name_prefix(proposals) {
            let mut key = group.ordered_indices.clone();
            key.sort_unstable();
            let steps: Vec<DetectedStep> = group
                .ordered_indices
                .iter()
                .enumerate()
                .map(|(i, &idx)| DetectedStep { proposal_idx: idx, step_index: i as u32 })
                .collect();
            let flow = DetectedFlow {
                name: group.prefix.clone(),
                kind: FlowKind::Custom,
                confidence: 0.60,
                steps,
            };
            // Only insert if not already present (route candidate has higher confidence).
            candidates.entry(key).or_insert(flow);
        }

        // 3. Pattern-based overlay (confidence 0.85, overwrites lower-confidence candidates).
        let pattern_matches = match_known_patterns(proposals);

        // Group matches by pattern name (stable key regardless of pointer identity).
        let mut by_pattern: HashMap<&'static str, (&'static patterns::FlowPattern, Vec<usize>)> =
            HashMap::new();
        for pm in &pattern_matches {
            by_pattern
                .entry(pm.pattern.name)
                .or_insert((pm.pattern, Vec::new()))
                .1
                .push(pm.proposal_idx);
        }

        for (_name, (pattern, mut indices)) in by_pattern {
            indices.sort_unstable();
            indices.dedup();
            if indices.len() < 2 {
                continue;
            }

            // Check if this pattern overlaps ≥ 80% with an existing candidate.
            // If so, replace it. Otherwise, insert as a new candidate.
            let overlap_key = find_overlapping_key(&candidates, &indices);
            let key = overlap_key.unwrap_or_else(|| indices.clone());

            let steps: Vec<DetectedStep> = indices
                .iter()
                .enumerate()
                .map(|(i, &idx)| DetectedStep { proposal_idx: idx, step_index: i as u32 })
                .collect();

            let flow = DetectedFlow {
                name: pattern.name.to_string(),
                kind: pattern.kind.clone(),
                confidence: 0.85,
                steps,
            };

            candidates.insert(key, flow);
        }

        // 4. Collect, filter, call refiner, sort.
        let mut flows: Vec<DetectedFlow> = candidates
            .into_values()
            .filter(|f| f.confidence >= 0.5 && f.steps.len() >= 2)
            .collect();

        self.refiner.refine(&mut flows, proposals);

        flows.sort_by(|a, b| a.name.cmp(&b.name));
        flows
    }
}

/// Find a key in `candidates` whose proposal-index set overlaps ≥ 80% with
/// `indices`. Returns `None` when no sufficiently-overlapping key exists.
fn find_overlapping_key(
    candidates: &HashMap<Vec<usize>, DetectedFlow>,
    indices: &[usize],
) -> Option<Vec<usize>> {
    let target_set: std::collections::HashSet<usize> = indices.iter().copied().collect();
    for key in candidates.keys() {
        let key_set: std::collections::HashSet<usize> = key.iter().copied().collect();
        let intersection = target_set.intersection(&key_set).count();
        let union = target_set.union(&key_set).count();
        if union > 0 && (intersection as f64 / union as f64) >= 0.8 {
            return Some(key.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use infergen_types::FlowKind;

    use crate::adapter::EventKind;
    use crate::ProposedEvent;

    use super::*;

    fn prop(name: &str, path: &str) -> ProposedEvent {
        ProposedEvent::new(name, EventKind::PageView, PathBuf::from(path), 0.9)
    }

    fn prop_name(name: &str) -> ProposedEvent {
        prop(name, "src/app.ts")
    }

    #[test]
    fn detect_checkout_flow_from_route() {
        let proposals = vec![
            prop("cart_viewed", "pages/checkout/cart.tsx"),
            prop("order_confirmed", "pages/checkout/confirm.tsx"),
        ];
        let flows = FlowDetector::new().detect(&proposals);
        assert!(!flows.is_empty(), "expected at least one flow");
        let checkout = flows.iter().find(|f| f.name == "checkout" || f.kind == FlowKind::Checkout);
        assert!(checkout.is_some(), "expected a checkout flow");
    }

    #[test]
    fn detect_onboarding_flow_from_names() {
        let proposals = vec![
            prop_name("onboarding_started"),
            prop_name("onboarding_completed"),
        ];
        let flows = FlowDetector::new().detect(&proposals);
        assert!(!flows.is_empty());
        // Pattern "onboarding" requires "onboard" substring — "onboarding_started" contains it.
        let ob = flows.iter().find(|f| f.name == "onboarding" || f.kind == FlowKind::Onboarding);
        assert!(ob.is_some(), "expected onboarding flow, got: {:?}", flows.iter().map(|f|&f.name).collect::<Vec<_>>());
    }

    #[test]
    fn pattern_beats_generic_for_overlapping_group() {
        // checkout route group AND checkout pattern both fire — pattern should win (confidence 0.85).
        let proposals = vec![
            prop("cart_viewed", "pages/checkout/cart.tsx"),
            prop("checkout_submitted", "pages/checkout/confirm.tsx"),
        ];
        let flows = FlowDetector::new().detect(&proposals);
        assert!(!flows.is_empty());
        // Find the flow covering both events; it should have Checkout kind.
        let checkout = flows.iter().find(|f| f.kind == FlowKind::Checkout);
        assert!(checkout.is_some());
        assert!(checkout.unwrap().confidence >= 0.85);
    }

    #[test]
    fn single_event_per_prefix_no_flow() {
        let proposals = vec![
            prop("payment_initiated", "app/payment/start.tsx"),
            prop("auth_login_viewed", "src/auth/login.tsx"),
        ];
        // Each prefix has only one proposal → no route groups; auth has 1 pattern match → no pattern group.
        let flows = FlowDetector::new().detect(&proposals);
        // Neither route nor name groupers should fire (1 proposal per prefix).
        // Only patterns could fire, but min_matches=2.
        assert!(flows.is_empty(), "expected no flows, got: {:?}", flows.iter().map(|f| &f.name).collect::<Vec<_>>());
    }

    #[test]
    fn detect_empty_returns_empty() {
        let flows = FlowDetector::new().detect(&[]);
        assert!(flows.is_empty());
    }

    #[test]
    fn flows_sorted_by_name() {
        let proposals = vec![
            prop_name("signup_started"),
            prop_name("signup_completed"),
            prop_name("onboarding_started"),
            prop_name("onboarding_completed"),
        ];
        let flows = FlowDetector::new().detect(&proposals);
        for w in flows.windows(2) {
            assert!(w[0].name <= w[1].name, "flows not sorted: {} > {}", w[0].name, w[1].name);
        }
    }

    #[test]
    fn noop_refiner_does_not_alter_detected_flows() {
        let proposals = vec![
            prop("cart_viewed", "pages/checkout/cart.tsx"),
            prop("order_confirmed", "pages/checkout/confirm.tsx"),
        ];
        let without_refiner = FlowDetector::new().detect(&proposals);
        let with_refiner = FlowDetector::with_refiner(Box::new(refiner::NoOpRefiner))
            .detect(&proposals);
        assert_eq!(without_refiner.len(), with_refiner.len());
    }
}
