# Infergen Plugin SDK

Infergen exposes three extension points as documented Rust traits. Community
authors can add new analytics destinations, framework adapters, or language
parsers without modifying the core engine.

| Extension point | Trait | What it does |
|-----------------|-------|--------------|
| Provider | `ProviderPlugin` | Send events to an analytics destination |
| Adapter  | `Adapter`        | Detect trackable moments in a framework |
| Parser   | `LanguageParser` | Parse source files in a new language |

---

## Prerequisites

- Rust 1.78+ (stable)
- `infergen-core` as a dependency in your plugin's `Cargo.toml`

```toml
[dependencies]
infergen-core = "0.1"
```

---

## Scaffolding

The fastest way to start is the scaffold command:

```bash
# Analytics provider
infergen plugin scaffold provider my-provider

# Framework adapter (--framework is required)
infergen plugin scaffold adapter my-adapter --framework htmx

# Language parser
infergen plugin scaffold parser lua

# Write to a file instead of stdout
infergen plugin scaffold provider my-provider --output src/my_provider.rs

# List all plugin types
infergen plugin list-types
```

Each command prints ready-to-compile Rust source. Paste it into your crate and
fill in the `TODO` comments.

---

## 1. Provider Plugin

### Trait contract

```rust
pub trait ProviderPlugin: Send + Sync {
    /// Unique lowercase hyphen-separated ID, e.g. `"posthog"`.
    fn id(&self) -> &str;

    /// Send one event to this destination.
    /// Return `Err(Error::ProviderError { id, message })` on failure.
    fn track(&self, event: &TrackEvent) -> Result<()>;

    /// Flush buffered events. Default: no-op.
    fn flush(&self) -> Result<()> { Ok(()) }

    /// Graceful shutdown. Default: no-op.
    fn shutdown(&self) -> Result<()> { Ok(()) }
}

pub struct TrackEvent {
    pub name: String,
    pub properties: HashMap<String, serde_json::Value>,
}
```

`ProviderPlugin` must be `Send + Sync` because the registry may be shared
across threads.

### Minimal example

```rust
use infergen_core::{ProviderPlugin, TrackEvent, Result};

pub struct DebugLogProvider;

impl ProviderPlugin for DebugLogProvider {
    fn id(&self) -> &str { "debug-log" }

    fn track(&self, event: &TrackEvent) -> Result<()> {
        eprintln!("[infergen] {} {:?}", event.name, event.properties);
        Ok(())
    }
}
```

### Returning errors

```rust
fn track(&self, event: &TrackEvent) -> Result<()> {
    if self.client.send(event).is_err() {
        return Err(infergen_core::Error::ProviderError {
            id: self.id().to_owned(),
            message: "connection refused".into(),
        });
    }
    Ok(())
}
```

The `ProviderRegistry` dispatches to all registered plugins sequentially. One
failure does not stop the remaining providers, but the first error is returned
to the caller.

### Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn debug_log_provider_tracks_without_error() {
        let p = DebugLogProvider;
        let event = TrackEvent {
            name: "test_event".into(),
            properties: HashMap::new(),
        };
        assert!(p.track(&event).is_ok());
    }
}
```

---

## 2. Framework Adapter

### Trait contract

```rust
pub trait Adapter {
    /// Which framework this adapter targets.
    fn framework(&self) -> Framework;

    /// Inspect `file` and return proposed tracking events.
    /// Must never return `Err` — return an empty `Vec` for files that don't match.
    fn analyze(&self, file: &ParsedFile) -> Vec<ProposedEvent>;
}

pub struct ProposedEvent {
    pub name: String,
    pub kind: EventKind,          // PageView, ApiCall, AuthEvent, FormSubmit, ButtonClick, Search, Error
    pub source_path: PathBuf,
    pub confidence: f32,          // 0.0–1.0
    pub properties: Vec<PropertyHint>,
    pub adapter: String,          // attribution, e.g. "nextjs"
}

pub struct PropertyHint {
    pub name: String,
    pub type_hint: Option<String>,
    pub pii_hint: bool,
}
```

### Confidence scoring guide

| Detection method | Confidence |
|------------------|-----------|
| Path-based (file lives in `pages/`, `app/`) | 0.9 |
| AST-based (import detected, function named) | 0.85 |
| Heuristic (identifier pattern match only)  | 0.7 |

### Accessing parsed source

`ParsedFile` provides language-specific re-entry points:

| Language | Method | Notes |
|----------|--------|-------|
| TypeScript / JavaScript | `file.with_js_program(\|prog\| …)` | Returns `Option<R>` |
| Python | `file.with_py_ast(\|stmts\| …)` | Returns `R: Default` |
| Go | `file.with_go_source(\|src\| …)` | Raw text, returns `Option<R>` |
| Ruby | `file.with_ruby_stmts(\|stmts\| …)` | Returns `R: Default` |
| Vue SFC | `file.with_vue_source(\|src\| …)` | Raw text, returns `Option<R>` |
| Svelte | `file.with_svelte_source(\|src\| …)` | Raw text, returns `Option<R>` |

### Minimal example

```rust
use infergen_core::{Adapter, EventKind, Framework, ParsedFile, ProposedEvent};

pub struct HtmxAdapter;

impl Adapter for HtmxAdapter {
    fn framework(&self) -> Framework {
        // Submit a PR to add Framework::Htmx to infergen-core.
        // Until then, use the closest existing variant or file an issue.
        Framework::Express // placeholder
    }

    fn analyze(&self, file: &ParsedFile) -> Vec<ProposedEvent> {
        // Detect hx-post / hx-get attributes in HTML files (text scan, no AST).
        if file.path.extension().map_or(true, |e| e != "html") {
            return vec![];
        }
        let mut events = vec![];
        if file.source.contains("hx-post") {
            events.push(
                ProposedEvent::new(
                    "form_submitted",
                    EventKind::FormSubmit,
                    file.path.clone(),
                    0.7,
                )
                .with_adapter("htmx"),
            );
        }
        events
    }
}
```

### Adding a new `Framework` variant

The `Framework` enum lives in `infergen-core/src/detect.rs`. Open a PR to add
your framework variant. Until the PR merges, use the closest existing variant
as a placeholder.

---

## 3. Language Parser

### Trait contract

```rust
pub trait LanguageParser {
    /// Parse `source` read from `path`.
    /// Syntax errors → push to `ParsedFile.diagnostics`; do NOT return `Err`.
    /// Only infrastructure failures (unsupported extension, OOM) return `Err`.
    fn parse(&self, path: &Path, source: &str) -> Result<ParsedFile>;
}

pub struct ParsedFile {
    pub path: PathBuf,
    pub lang: Language,
    pub source: String,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct Diagnostic {
    pub message: String,
    pub start: u32,   // byte offset
    pub end: u32,     // byte offset
}
```

**The error-tolerant contract is critical.** The scan engine passes every file
through the parser; returning `Err` on a syntax error would abort the entire
scan for that file's language. Use `diagnostics` instead.

### Minimal example

```rust
use std::path::{Path, PathBuf};
use infergen_core::{Diagnostic, Language, LanguageParser, ParsedFile, Result};

pub struct LuaParser;

impl LanguageParser for LuaParser {
    fn parse(&self, path: &Path, source: &str) -> Result<ParsedFile> {
        let mut diagnostics = vec![];
        // TODO: use a Lua parser crate to produce an AST.
        // On syntax error: push a Diagnostic, do not return Err.
        // Example: diagnostics.push(Diagnostic { message: err.to_string(), start: 0, end: 0 });
        Ok(ParsedFile {
            path: PathBuf::from(path),
            lang: Language::TypeScript, // placeholder — see note below
            source: source.to_owned(),
            diagnostics,
        })
    }
}
```

### Adding a new `Language` variant

The `Language` enum lives in `infergen-core/src/detect.rs`. Submit a PR to add
`Language::Lua` (or your language). Framework adapters for your language will
call `file.with_*` methods that you add to `ParsedFile` in `parser/mod.rs`.

---

## 4. Publishing

Suggested crate naming convention:

- `infergen-provider-<name>` — e.g. `infergen-provider-slack`
- `infergen-adapter-<name>` — e.g. `infergen-adapter-htmx`
- `infergen-parser-<name>` — e.g. `infergen-parser-lua`

Publish to [crates.io](https://crates.io). Add the `infergen-plugin` keyword so
authors can find your crate.

```toml
[package]
name = "infergen-provider-slack"
keywords = ["infergen", "infergen-plugin", "analytics"]
```

---

## 5. Stability guarantees

| API surface | Policy |
|-------------|--------|
| `ProviderPlugin` trait methods | Stable from v0.1 |
| `Adapter` trait methods | Stable from v0.1 |
| `LanguageParser` trait method | Stable from v0.1 |
| `TrackEvent` fields | Additive-only in minor releases |
| `ProposedEvent` fields | Additive-only in minor releases |
| `Framework` enum variants | Additive-only in minor releases |
| `Language` enum variants | Additive-only in minor releases |
| Internal engine modules | No stability guarantee |

New optional trait methods will always provide a default implementation so
existing plugins continue to compile without changes.
