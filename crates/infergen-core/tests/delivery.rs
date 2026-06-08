//! End-to-end delivery tests (E8.2).
//!
//! Verifies the full pipeline from adapter proposals through catalog formation
//! to provider registry dispatch.  Tests that the system's parts compose
//! correctly at runtime, not just that each part works in isolation.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use infergen_core::{
    Adapter, EventKind, EventStatus, JsParser, LanguageParser, NextjsAdapter, ProposedEvent,
    ProviderPlugin, ProviderRegistry, TrackEvent, approve_all_proposed, from_proposals,
};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

struct RecordingProvider {
    name: String,
    events: Mutex<Vec<TrackEvent>>,
    flush_calls: Mutex<u32>,
    shutdown_calls: Mutex<u32>,
}

impl RecordingProvider {
    fn new(name: &str) -> Arc<Self> {
        Arc::new(Self {
            name: name.to_string(),
            events: Mutex::new(Vec::new()),
            flush_calls: Mutex::new(0),
            shutdown_calls: Mutex::new(0),
        })
    }

    fn event_names(&self) -> Vec<String> {
        self.events.lock().unwrap().iter().map(|e| e.name.clone()).collect()
    }

    fn events_len(&self) -> usize {
        self.events.lock().unwrap().len()
    }

    fn flush_count(&self) -> u32 {
        *self.flush_calls.lock().unwrap()
    }

    fn shutdown_count(&self) -> u32 {
        *self.shutdown_calls.lock().unwrap()
    }

    fn last_event_properties(&self) -> HashMap<String, serde_json::Value> {
        self.events
            .lock()
            .unwrap()
            .last()
            .map(|e| e.properties.clone())
            .unwrap_or_default()
    }
}

impl ProviderPlugin for RecordingProvider {
    fn id(&self) -> &str {
        &self.name
    }

    fn track(&self, event: &TrackEvent) -> infergen_core::Result<()> {
        self.events.lock().unwrap().push(TrackEvent {
            name: event.name.clone(),
            properties: event.properties.clone(),
        });
        Ok(())
    }

    fn flush(&self) -> infergen_core::Result<()> {
        *self.flush_calls.lock().unwrap() += 1;
        Ok(())
    }

    fn shutdown(&self) -> infergen_core::Result<()> {
        *self.shutdown_calls.lock().unwrap() += 1;
        Ok(())
    }
}

struct FailingProvider {
    name: String,
}

impl ProviderPlugin for FailingProvider {
    fn id(&self) -> &str {
        &self.name
    }

    fn track(&self, _event: &TrackEvent) -> infergen_core::Result<()> {
        Err(infergen_core::Error::ProviderError {
            id: self.name.clone(),
            message: "forced failure".into(),
        })
    }
}

/// Wraps `Arc<RecordingProvider>` to satisfy `Box<dyn ProviderPlugin>`.
struct ArcProvider(Arc<RecordingProvider>);

impl ProviderPlugin for ArcProvider {
    fn id(&self) -> &str {
        self.0.id()
    }

    fn track(&self, event: &TrackEvent) -> infergen_core::Result<()> {
        self.0.track(event)
    }

    fn flush(&self) -> infergen_core::Result<()> {
        self.0.flush()
    }

    fn shutdown(&self) -> infergen_core::Result<()> {
        self.0.shutdown()
    }
}

fn mk_event(name: &str) -> TrackEvent {
    TrackEvent { name: name.to_string(), properties: HashMap::new() }
}

fn mk_event_with_props(name: &str, props: Vec<(&str, serde_json::Value)>) -> TrackEvent {
    let properties = props.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    TrackEvent { name: name.to_string(), properties }
}

fn registry_with(provider: &Arc<RecordingProvider>) -> ProviderRegistry {
    let mut reg = ProviderRegistry::new();
    reg.register(Box::new(ArcProvider(Arc::clone(provider))));
    reg
}

// ---------------------------------------------------------------------------
// End-to-end pipeline tests
// ---------------------------------------------------------------------------

#[test]
fn e2e_proposals_through_registry() {
    let proposals = vec![
        ProposedEvent::new("page_viewed", EventKind::PageView, "src/index.tsx", 0.9)
            .with_adapter("nextjs"),
        ProposedEvent::new("user_signed_in", EventKind::AuthEvent, "src/auth.ts", 0.85)
            .with_adapter("nextjs"),
        ProposedEvent::new("checkout_completed", EventKind::ApiCall, "src/checkout.ts", 0.9)
            .with_adapter("nextjs"),
    ];

    let mut catalog = from_proposals(&proposals, Path::new("/project"));
    let approved = approve_all_proposed(&mut catalog);
    assert_eq!(approved, 3, "all 3 proposals should be approvable");

    let rec = RecordingProvider::new("segment");
    let mut reg = registry_with(&rec);

    for entry in catalog.events.iter().filter(|e| e.status == EventStatus::Approved) {
        reg.track(mk_event(&entry.name)).unwrap();
    }

    let names = rec.event_names();
    assert_eq!(names.len(), 3, "all 3 approved events should be delivered");
    assert!(names.contains(&"page_viewed".to_string()));
    assert!(names.contains(&"user_signed_in".to_string()));
    assert!(names.contains(&"checkout_completed".to_string()));
}

#[test]
fn delivery_properties_passed_through() {
    let rec = RecordingProvider::new("test");
    let mut reg = registry_with(&rec);

    let event = mk_event_with_props("api_called", vec![
        ("user_id", serde_json::Value::String("u123".into())),
        ("status_code", serde_json::Value::Number(200.into())),
        ("is_retry", serde_json::Value::Bool(false)),
    ]);
    reg.track(event).unwrap();

    assert_eq!(rec.events_len(), 1);
    let props = rec.last_event_properties();
    assert_eq!(props.get("user_id"), Some(&serde_json::Value::String("u123".into())));
    assert_eq!(props.get("status_code"), Some(&serde_json::Value::Number(200.into())));
    assert_eq!(props.get("is_retry"), Some(&serde_json::Value::Bool(false)));
}

#[test]
fn delivery_ordering_preserved() {
    let rec = RecordingProvider::new("test");
    let mut reg = registry_with(&rec);

    let names = ["a", "b", "c", "d", "e"];
    for name in &names {
        reg.track(mk_event(name)).unwrap();
    }

    let recorded = rec.event_names();
    let expected: Vec<String> = names.iter().map(|s| s.to_string()).collect();
    assert_eq!(recorded, expected, "events must be received in dispatch order");
}

#[test]
fn delivery_multi_provider_each_receives_all() {
    let p0 = RecordingProvider::new("p0");
    let p1 = RecordingProvider::new("p1");
    let p2 = RecordingProvider::new("p2");

    let mut reg = ProviderRegistry::new();
    reg.register(Box::new(ArcProvider(Arc::clone(&p0))));
    reg.register(Box::new(ArcProvider(Arc::clone(&p1))));
    reg.register(Box::new(ArcProvider(Arc::clone(&p2))));

    for i in 0..5 {
        reg.track(mk_event(&format!("e{i}"))).unwrap();
    }

    assert_eq!(p0.events_len(), 5, "p0 should receive all 5 events");
    assert_eq!(p1.events_len(), 5, "p1 should receive all 5 events");
    assert_eq!(p2.events_len(), 5, "p2 should receive all 5 events");
}

#[test]
fn delivery_partial_failure_isolation() {
    let good1 = RecordingProvider::new("good1");
    let good2 = RecordingProvider::new("good2");

    let mut reg = ProviderRegistry::new();
    reg.register(Box::new(ArcProvider(Arc::clone(&good1))));
    reg.register(Box::new(FailingProvider { name: "failing".into() }));
    reg.register(Box::new(ArcProvider(Arc::clone(&good2))));

    let err = reg.track(mk_event("test_event")).unwrap_err();
    assert!(
        matches!(&err, infergen_core::Error::ProviderError { id, .. } if id == "failing"),
        "error should identify the failing provider"
    );
    assert_eq!(good1.events_len(), 1, "good1 should receive event despite failing provider");
    assert_eq!(good2.events_len(), 1, "good2 should receive event despite failing provider");
}

#[test]
fn delivery_flush_after_batch() {
    let rec = RecordingProvider::new("test");
    let mut reg = registry_with(&rec);

    for i in 0..5 {
        reg.track(mk_event(&format!("event_{i}"))).unwrap();
    }
    reg.flush().unwrap();

    assert_eq!(rec.events_len(), 5, "all 5 events should be received before flush");
    assert_eq!(rec.flush_count(), 1, "flush should be called exactly once");
}

#[test]
fn delivery_shutdown_lifecycle() {
    let rec = RecordingProvider::new("test");
    let mut reg = registry_with(&rec);

    reg.track(mk_event("event_1")).unwrap();
    reg.track(mk_event("event_2")).unwrap();
    reg.track(mk_event("event_3")).unwrap();
    reg.flush().unwrap();
    reg.shutdown().unwrap();

    assert_eq!(rec.events_len(), 3, "all 3 events received");
    assert_eq!(rec.flush_count(), 1, "flush called once");
    assert_eq!(rec.shutdown_count(), 1, "shutdown called once");
}

#[test]
fn e2e_nextjs_adapter_to_delivery() {
    // Full pipeline: parse real source → adapter → proposals → catalog → delivery
    let source = r#"
import { GetServerSideProps } from 'next';
export default function DashboardPage() { return null; }
export const getServerSideProps: GetServerSideProps = async () => ({ props: {} });
"#;

    let file = JsParser
        .parse(Path::new("/project/pages/dashboard.tsx"), source)
        .expect("parse dashboard fixture");
    let events = NextjsAdapter::new("/project").analyze(&file);

    assert!(!events.is_empty(), "adapter should detect events in the dashboard page");

    let mut catalog = from_proposals(&events, Path::new("/project"));
    approve_all_proposed(&mut catalog);

    let rec = RecordingProvider::new("analytics");
    let mut reg = registry_with(&rec);

    let approved_count =
        catalog.events.iter().filter(|e| e.status == EventStatus::Approved).count();
    for entry in catalog.events.iter().filter(|e| e.status == EventStatus::Approved) {
        reg.track(mk_event(&entry.name)).unwrap();
    }

    assert_eq!(
        rec.events_len(),
        approved_count,
        "all approved events from the adapter pipeline should be delivered"
    );
}
