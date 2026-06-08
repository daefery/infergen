//! Semantic refinement trait and no-op default (E6.2).
//!
//! [`SemanticRefiner`] is the plug-in point for E6.1 (Local LLM / Ollama).
//! When E6.1 ships, it implements this trait to run an LLM pass that can
//! merge semantically equivalent flows, re-order steps, and add descriptions.

use super::DetectedFlow;
use crate::ProposedEvent;

/// Optional semantic refinement pass over detected flows.
///
/// The default implementation ([`NoOpRefiner`]) is a no-op. E6.1 implements
/// this trait to invoke Ollama for higher-quality flow grouping.
pub trait SemanticRefiner: Send + Sync {
    /// Refine `flows` in-place. May add, remove, merge, or reorder flows and
    /// their steps. Must not panic.
    fn refine(&self, flows: &mut Vec<DetectedFlow>, proposals: &[ProposedEvent]);
}

/// No-op refiner — leaves flows unchanged. Used when E6.1 is not available.
pub struct NoOpRefiner;

impl SemanticRefiner for NoOpRefiner {
    fn refine(&self, _flows: &mut Vec<DetectedFlow>, _proposals: &[ProposedEvent]) {}
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use infergen_types::FlowKind;

    use crate::adapter::EventKind;
    use crate::ProposedEvent;
    use crate::flow::DetectedFlow;
    use crate::flow::DetectedStep;

    use super::*;

    fn make_flow() -> DetectedFlow {
        DetectedFlow {
            name: "checkout".into(),
            kind: FlowKind::Checkout,
            confidence: 0.85,
            steps: vec![
                DetectedStep { proposal_idx: 0, step_index: 0 },
                DetectedStep { proposal_idx: 1, step_index: 1 },
            ],
        }
    }

    fn make_proposal(name: &str) -> ProposedEvent {
        ProposedEvent::new(name, EventKind::PageView, PathBuf::from("a.ts"), 0.9)
    }

    #[test]
    fn noop_refiner_leaves_flows_unchanged() {
        let mut flows = vec![make_flow()];
        let proposals = vec![make_proposal("cart_viewed"), make_proposal("order_confirmed")];
        let refiner = NoOpRefiner;
        refiner.refine(&mut flows, &proposals);
        assert_eq!(flows.len(), 1);
        assert_eq!(flows[0].steps.len(), 2);
    }

    #[test]
    fn noop_refiner_does_not_panic_on_empty() {
        let mut flows: Vec<DetectedFlow> = vec![];
        let refiner = NoOpRefiner;
        refiner.refine(&mut flows, &[]);
    }

    #[test]
    fn semantic_refiner_is_object_safe() {
        fn _check(_: &dyn SemanticRefiner) {}
    }
}
