//! Integration tests for E3.1 provider plugin interface.
//!
//! Verifies `ProviderRegistry` dispatch, error propagation, and that the
//! `ProviderPlugin` default method implementations work correctly.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use infergen_core::{ProviderPlugin, ProviderRegistry, TrackEvent};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

struct RecordingProvider {
    name: String,
    events: Mutex<Vec<String>>,
    flush_calls: Mutex<u32>,
    shutdown_calls: Mutex<u32>,
}

impl RecordingProvider {
    fn new(name: &str) -> Arc<Self> {
        Arc::new(RecordingProvider {
            name: name.to_string(),
            events: Mutex::new(Vec::new()),
            flush_calls: Mutex::new(0),
            shutdown_calls: Mutex::new(0),
        })
    }

    fn recorded(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }

    fn flush_count(&self) -> u32 {
        *self.flush_calls.lock().unwrap()
    }

    fn shutdown_count(&self) -> u32 {
        *self.shutdown_calls.lock().unwrap()
    }
}

impl ProviderPlugin for RecordingProvider {
    fn id(&self) -> &str {
        &self.name
    }

    fn track(&self, event: &TrackEvent) -> infergen_core::Result<()> {
        self.events.lock().unwrap().push(event.name.clone());
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
    also_calls: Option<Arc<RecordingProvider>>,
}

impl ProviderPlugin for FailingProvider {
    fn id(&self) -> &str {
        &self.name
    }

    fn track(&self, event: &TrackEvent) -> infergen_core::Result<()> {
        if let Some(r) = &self.also_calls {
            let _ = r.track(event);
        }
        Err(infergen_core::Error::ProviderError {
            id: self.name.clone(),
            message: "forced failure".into(),
        })
    }
}

/// Wraps `Arc<RecordingProvider>` so it can be boxed as `dyn ProviderPlugin`.
struct ArcPlugin(Arc<RecordingProvider>);

impl ProviderPlugin for ArcPlugin {
    fn id(&self) -> &str { self.0.id() }
    fn track(&self, event: &TrackEvent) -> infergen_core::Result<()> { self.0.track(event) }
    fn flush(&self) -> infergen_core::Result<()> { self.0.flush() }
    fn shutdown(&self) -> infergen_core::Result<()> { self.0.shutdown() }
}

fn event(name: &str) -> TrackEvent {
    TrackEvent { name: name.to_string(), properties: HashMap::new() }
}

fn event_with_props(name: &str, props: &[(&str, &str)]) -> TrackEvent {
    let mut properties = HashMap::new();
    for (k, v) in props {
        properties.insert(k.to_string(), serde_json::Value::String(v.to_string()));
    }
    TrackEvent { name: name.to_string(), properties }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn registry_dispatches_to_single_provider() {
    let mut r = ProviderRegistry::new();
    let p = RecordingProvider::new("posthog");
    r.register(Box::new(ArcPlugin(Arc::clone(&p))));
    r.track(event("page_viewed")).unwrap();
    assert_eq!(p.recorded(), vec!["page_viewed"]);
}

#[test]
fn registry_dispatches_to_multiple_providers() {
    let mut r = ProviderRegistry::new();
    let a = RecordingProvider::new("segment");
    let b = RecordingProvider::new("posthog");
    r.register(Box::new(ArcPlugin(Arc::clone(&a))));
    r.register(Box::new(ArcPlugin(Arc::clone(&b))));
    r.track(event("user_signed_in")).unwrap();
    r.track(event("page_viewed")).unwrap();
    assert_eq!(a.recorded(), vec!["user_signed_in", "page_viewed"]);
    assert_eq!(b.recorded(), vec!["user_signed_in", "page_viewed"]);
}

#[test]
fn registry_first_error_returned_others_still_called() {
    let mut r = ProviderRegistry::new();
    let good = RecordingProvider::new("good");
    r.register(Box::new(FailingProvider { name: "bad".into(), also_calls: None }));
    r.register(Box::new(ArcPlugin(Arc::clone(&good))));
    let err = r.track(event("test")).unwrap_err();
    assert!(matches!(err, infergen_core::Error::ProviderError { ref id, .. } if id == "bad"));
    // good provider was still called despite bad failing first
    assert_eq!(good.recorded(), vec!["test"]);
}

#[test]
fn registry_flush_reaches_all() {
    let mut r = ProviderRegistry::new();
    let a = RecordingProvider::new("a");
    let b = RecordingProvider::new("b");
    r.register(Box::new(ArcPlugin(Arc::clone(&a))));
    r.register(Box::new(ArcPlugin(Arc::clone(&b))));
    r.flush().unwrap();
    assert_eq!(a.flush_count(), 1);
    assert_eq!(b.flush_count(), 1);
}

#[test]
fn registry_shutdown_reaches_all() {
    let mut r = ProviderRegistry::new();
    let a = RecordingProvider::new("a");
    let b = RecordingProvider::new("b");
    r.register(Box::new(ArcPlugin(Arc::clone(&a))));
    r.register(Box::new(ArcPlugin(Arc::clone(&b))));
    r.shutdown().unwrap();
    assert_eq!(a.shutdown_count(), 1);
    assert_eq!(b.shutdown_count(), 1);
}

#[test]
fn registry_empty_is_no_op() {
    let r = ProviderRegistry::new();
    assert!(r.track(event("page_viewed")).is_ok());
    assert!(r.flush().is_ok());
    assert!(r.shutdown().is_ok());
}

#[test]
fn provider_ids_reflect_registration_order() {
    let mut r = ProviderRegistry::new();
    r.register(Box::new(ArcPlugin(RecordingProvider::new("alpha"))));
    r.register(Box::new(ArcPlugin(RecordingProvider::new("beta"))));
    r.register(Box::new(ArcPlugin(RecordingProvider::new("gamma"))));
    assert_eq!(r.provider_ids(), vec!["alpha", "beta", "gamma"]);
}

#[test]
fn track_event_carries_properties() {
    let mut r = ProviderRegistry::new();
    let p = RecordingProvider::new("p");
    r.register(Box::new(ArcPlugin(Arc::clone(&p))));
    let ev = event_with_props("api_called", &[("method", "GET"), ("status", "200")]);
    r.track(ev).unwrap();
    // Just verify the event name was dispatched (property inspection is in unit tests)
    assert_eq!(p.recorded(), vec!["api_called"]);
}
