//! Provider plugin interface for Infergen's runtime SDK (E3.1).
//!
//! Defines the [`ProviderPlugin`] trait, [`TrackEvent`] payload type, and
//! [`ProviderRegistry`] dispatcher. First-party implementations land in E3.2.

use std::collections::HashMap;

use serde_json::Value;

// ---------------------------------------------------------------------------
// TrackEvent
// ---------------------------------------------------------------------------

/// A single analytics event ready to dispatch to registered providers.
pub struct TrackEvent {
    /// Event name, e.g. `"page_viewed"`.
    pub name: String,
    /// Typed properties keyed by name.
    pub properties: HashMap<String, Value>,
}

// ---------------------------------------------------------------------------
// ProviderPlugin trait
// ---------------------------------------------------------------------------

/// Contract that every analytics provider adapter must satisfy.
///
/// Implement this trait to create a custom destination. See E3.2 for
/// first-party implementations (Segment, PostHog, Amplitude, …).
///
/// # Minimal implementation
/// ```rust,ignore
/// struct MyProvider;
/// impl infergen_core::ProviderPlugin for MyProvider {
///     fn id(&self) -> &str { "my-provider" }
///     fn track(&self, event: &infergen_core::TrackEvent) -> infergen_core::Result<()> {
///         println!("track: {} {:?}", event.name, event.properties);
///         Ok(())
///     }
/// }
/// ```
pub trait ProviderPlugin: Send + Sync {
    /// Unique, lowercase, hyphen-separated provider ID, e.g. `"posthog"`.
    fn id(&self) -> &str;

    /// Send a single event to this provider.
    ///
    /// # Errors
    /// Returns [`crate::Error::ProviderError`] if the provider cannot accept
    /// the event (not initialised, serialization failure, etc.).
    fn track(&self, event: &TrackEvent) -> crate::Result<()>;

    /// Flush any internally buffered events.
    ///
    /// Default: no-op. Override in providers that buffer internally.
    ///
    /// # Errors
    /// Returns [`crate::Error::ProviderError`] on failure.
    fn flush(&self) -> crate::Result<()> {
        Ok(())
    }

    /// Gracefully shut down the provider (flush + close connections).
    ///
    /// Default: no-op. Override in providers with open resources.
    ///
    /// # Errors
    /// Returns [`crate::Error::ProviderError`] on failure.
    fn shutdown(&self) -> crate::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ProviderRegistry
// ---------------------------------------------------------------------------

/// Holds registered [`ProviderPlugin`]s and fans out tracking calls.
///
/// Dispatch is synchronous and sequential for E3.1. Async batching +
/// retry land in E3.3. On partial failure, `track`/`flush`/`shutdown`
/// continue calling all remaining plugins and return the **first** error.
pub struct ProviderRegistry {
    plugins: Vec<Box<dyn ProviderPlugin>>,
}

impl ProviderRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        ProviderRegistry { plugins: Vec::new() }
    }

    /// Register a provider. Later registrations are dispatched after earlier ones.
    pub fn register(&mut self, plugin: Box<dyn ProviderPlugin>) {
        self.plugins.push(plugin);
    }

    /// Send `event` to every registered provider in registration order.
    ///
    /// Returns the first error encountered; remaining plugins are still called.
    /// Returns `Ok(())` when all succeed or the registry is empty.
    ///
    /// # Errors
    /// Returns the first [`crate::Error::ProviderError`] encountered.
    pub fn track(&self, event: TrackEvent) -> crate::Result<()> {
        let mut first_err: Option<crate::Error> = None;
        for plugin in &self.plugins {
            if let Err(e) = plugin.track(&event)
                && first_err.is_none()
            {
                first_err = Some(e);
            }
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Flush all registered providers.
    ///
    /// Calls `flush()` on every plugin, returns the first error if any.
    ///
    /// # Errors
    /// Returns the first [`crate::Error::ProviderError`] encountered.
    pub fn flush(&self) -> crate::Result<()> {
        let mut first_err: Option<crate::Error> = None;
        for plugin in &self.plugins {
            if let Err(e) = plugin.flush()
                && first_err.is_none()
            {
                first_err = Some(e);
            }
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Shut down all registered providers.
    ///
    /// Calls `shutdown()` on every plugin, returns the first error if any.
    ///
    /// # Errors
    /// Returns the first [`crate::Error::ProviderError`] encountered.
    pub fn shutdown(&self) -> crate::Result<()> {
        let mut first_err: Option<crate::Error> = None;
        for plugin in &self.plugins {
            if let Err(e) = plugin.shutdown()
                && first_err.is_none()
            {
                first_err = Some(e);
            }
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Returns `true` if no providers are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// Number of registered providers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// IDs of all registered providers, in registration order.
    #[must_use]
    pub fn provider_ids(&self) -> Vec<&str> {
        self.plugins.iter().map(|p| p.id()).collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        ProviderRegistry::new()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Test fixtures ---

    struct RecordingProvider {
        name: String,
        events: std::sync::Mutex<Vec<String>>,
        flush_count: std::sync::Mutex<u32>,
        shutdown_count: std::sync::Mutex<u32>,
    }

    impl RecordingProvider {
        fn new(name: &str) -> Self {
            RecordingProvider {
                name: name.to_string(),
                events: std::sync::Mutex::new(Vec::new()),
                flush_count: std::sync::Mutex::new(0),
                shutdown_count: std::sync::Mutex::new(0),
            }
        }

        fn recorded(&self) -> Vec<String> {
            self.events.lock().unwrap().clone()
        }

        fn flush_calls(&self) -> u32 {
            *self.flush_count.lock().unwrap()
        }

        fn shutdown_calls(&self) -> u32 {
            *self.shutdown_count.lock().unwrap()
        }
    }

    impl ProviderPlugin for RecordingProvider {
        fn id(&self) -> &str {
            &self.name
        }

        fn track(&self, event: &TrackEvent) -> crate::Result<()> {
            self.events.lock().unwrap().push(event.name.clone());
            Ok(())
        }

        fn flush(&self) -> crate::Result<()> {
            *self.flush_count.lock().unwrap() += 1;
            Ok(())
        }

        fn shutdown(&self) -> crate::Result<()> {
            *self.shutdown_count.lock().unwrap() += 1;
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

        fn track(&self, _event: &TrackEvent) -> crate::Result<()> {
            Err(crate::Error::ProviderError {
                id: self.name.clone(),
                message: "forced failure".into(),
            })
        }
    }

    fn simple_event(name: &str) -> TrackEvent {
        TrackEvent { name: name.to_string(), properties: HashMap::new() }
    }

    // --- Tests ---

    #[test]
    fn registry_new_is_empty() {
        let r = ProviderRegistry::new();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn registry_register_increments_len() {
        let mut r = ProviderRegistry::new();
        r.register(Box::new(RecordingProvider::new("a")));
        r.register(Box::new(RecordingProvider::new("b")));
        assert_eq!(r.len(), 2);
        assert!(!r.is_empty());
    }

    #[test]
    fn registry_provider_ids_in_order() {
        let mut r = ProviderRegistry::new();
        r.register(Box::new(RecordingProvider::new("alpha")));
        r.register(Box::new(RecordingProvider::new("beta")));
        assert_eq!(r.provider_ids(), vec!["alpha", "beta"]);
    }

    #[test]
    fn registry_empty_track_ok() {
        let r = ProviderRegistry::new();
        assert!(r.track(simple_event("page_viewed")).is_ok());
    }

    #[test]
    fn registry_default_flush_ok() {
        let provider = RecordingProvider::new("p");
        assert!(provider.flush().is_ok());
    }

    #[test]
    fn registry_default_shutdown_ok() {
        let provider = RecordingProvider::new("p");
        assert!(provider.shutdown().is_ok());
    }

    #[test]
    fn registry_track_calls_all_plugins() {
        // Use raw pointers to inspect after moving into registry.
        let mut r = ProviderRegistry::new();
        let a = std::sync::Arc::new(RecordingProvider::new("a"));
        let b = std::sync::Arc::new(RecordingProvider::new("b"));
        let a_ref = std::sync::Arc::clone(&a);
        let b_ref = std::sync::Arc::clone(&b);

        r.register(Box::new(ArcWrapper(a)));
        r.register(Box::new(ArcWrapper(b)));
        r.track(simple_event("page_viewed")).unwrap();

        assert_eq!(a_ref.recorded(), vec!["page_viewed"]);
        assert_eq!(b_ref.recorded(), vec!["page_viewed"]);
    }

    #[test]
    fn registry_track_returns_first_error_continues_others() {
        let mut r = ProviderRegistry::new();
        let good = std::sync::Arc::new(RecordingProvider::new("good"));
        let good_ref = std::sync::Arc::clone(&good);

        r.register(Box::new(FailingProvider { name: "bad".into() }));
        r.register(Box::new(ArcWrapper(good)));

        let err = r.track(simple_event("test")).unwrap_err();
        assert!(matches!(err, crate::Error::ProviderError { .. }));
        // good provider was still called despite bad provider failing first
        assert_eq!(good_ref.recorded(), vec!["test"]);
    }

    #[test]
    fn registry_flush_calls_all() {
        let mut r = ProviderRegistry::new();
        let a = std::sync::Arc::new(RecordingProvider::new("a"));
        let b = std::sync::Arc::new(RecordingProvider::new("b"));
        let a_ref = std::sync::Arc::clone(&a);
        let b_ref = std::sync::Arc::clone(&b);
        r.register(Box::new(ArcWrapper(a)));
        r.register(Box::new(ArcWrapper(b)));
        r.flush().unwrap();
        assert_eq!(a_ref.flush_calls(), 1);
        assert_eq!(b_ref.flush_calls(), 1);
    }

    #[test]
    fn registry_shutdown_calls_all() {
        let mut r = ProviderRegistry::new();
        let a = std::sync::Arc::new(RecordingProvider::new("a"));
        let b = std::sync::Arc::new(RecordingProvider::new("b"));
        let a_ref = std::sync::Arc::clone(&a);
        let b_ref = std::sync::Arc::clone(&b);
        r.register(Box::new(ArcWrapper(a)));
        r.register(Box::new(ArcWrapper(b)));
        r.shutdown().unwrap();
        assert_eq!(a_ref.shutdown_calls(), 1);
        assert_eq!(b_ref.shutdown_calls(), 1);
    }

    /// Wraps `Arc<RecordingProvider>` so it can be boxed as `dyn ProviderPlugin`.
    struct ArcWrapper(std::sync::Arc<RecordingProvider>);
    impl ProviderPlugin for ArcWrapper {
        fn id(&self) -> &str { self.0.id() }
        fn track(&self, event: &TrackEvent) -> crate::Result<()> { self.0.track(event) }
        fn flush(&self) -> crate::Result<()> { self.0.flush() }
        fn shutdown(&self) -> crate::Result<()> { self.0.shutdown() }
    }
}
