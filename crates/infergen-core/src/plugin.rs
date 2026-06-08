//! Plugin SDK — scaffold template generators for community plugin authors (E7.3).
//!
//! Three extension points are available:
//!
//! | Type | Trait | Use |
//! |------|-------|-----|
//! | Provider | [`crate::ProviderPlugin`] | Add an analytics destination |
//! | Adapter  | [`crate::Adapter`]         | Detect moments in a framework |
//! | Parser   | [`crate::LanguageParser`]  | Support a new source language |
//!
//! Use the scaffold functions below to generate a ready-to-compile Rust
//! skeleton, or run `infergen plugin scaffold <kind> <name>` from the CLI.

/// Convert a kebab-case or snake_case plugin name to PascalCase.
///
/// `"debug-log"` → `"DebugLog"`, `"my_parser"` → `"MyParser"`.
/// An empty or blank string returns `"Plugin"` as a safe fallback.
fn to_pascal_case(s: &str) -> String {
    let segments: Vec<&str> = s.split(['-', '_']).collect();
    let result: String = segments
        .into_iter()
        .filter(|seg| !seg.trim().is_empty())
        .map(|seg| {
            let mut chars = seg.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    let upper: String = first.to_uppercase().collect();
                    upper + chars.as_str()
                }
            }
        })
        .collect();
    if result.is_empty() {
        "Plugin".to_owned()
    } else {
        result
    }
}

/// Generate a minimal `ProviderPlugin` skeleton for `name`.
///
/// The returned string is valid Rust source that can be placed in a new file
/// inside any crate that depends on `infergen-core`.
///
/// # Example
/// ```
/// let src = infergen_core::plugin::provider_scaffold("debug-log");
/// assert!(src.contains("DebugLogProvider"));
/// assert!(src.contains("impl ProviderPlugin"));
/// ```
pub fn provider_scaffold(name: &str) -> String {
    let name = name.trim();
    let name = if name.is_empty() { "plugin" } else { name };
    let pascal = to_pascal_case(name);
    format!(
        r#"use infergen_core::{{ProviderPlugin, TrackEvent, Result}};

pub struct {pascal}Provider;

impl ProviderPlugin for {pascal}Provider {{
    fn id(&self) -> &str {{
        "{name}"
    }}

    fn track(&self, event: &TrackEvent) -> Result<()> {{
        // TODO: send event.name and event.properties to your destination.
        let _ = event;
        Ok(())
    }}

    // fn flush(&self) -> Result<()> {{ Ok(()) }}     // optional: override to flush buffers
    // fn shutdown(&self) -> Result<()> {{ Ok(()) }}  // optional: override to close connections
}}
"#,
        pascal = pascal,
        name = name,
    )
}

/// Generate a minimal `Adapter` skeleton for `name` targeting `framework`.
///
/// The returned string is valid Rust source. The `framework()` method body
/// contains a `todo!()` because adding a new `Framework` variant requires
/// a PR to `infergen-core`; community authors are encouraged to submit one or
/// use the closest existing variant in the meantime.
///
/// # Example
/// ```
/// let src = infergen_core::plugin::adapter_scaffold("my-adapter", "htmx");
/// assert!(src.contains("MyAdapterAdapter"));
/// assert!(src.contains("impl Adapter"));
/// ```
pub fn adapter_scaffold(name: &str, framework: &str) -> String {
    let name = name.trim();
    let name = if name.is_empty() { "plugin" } else { name };
    let framework = framework.trim();
    let framework = if framework.is_empty() { "unknown" } else { framework };
    let pascal_name = to_pascal_case(name);
    let pascal_fw = to_pascal_case(framework);
    format!(
        r#"use infergen_core::{{Adapter, Framework, ParsedFile, ProposedEvent}};

pub struct {pascal_name}Adapter;

impl Adapter for {pascal_name}Adapter {{
    fn framework(&self) -> Framework {{
        // TODO: add Framework::{pascal_fw} to infergen-core, or use an existing variant.
        // Submit a PR at https://github.com/your-org/infergen to add your framework.
        todo!("map to a Framework variant")
    }}

    fn analyze(&self, file: &ParsedFile) -> Vec<ProposedEvent> {{
        let _ = file;
        // TODO: inspect file.path for path-based detection (confidence 0.9),
        //       use file.with_js_program / with_py_ast / with_go_source / with_ruby_stmts
        //       for AST-based detection (confidence 0.85), or fall back to heuristics (0.7).
        vec![]
    }}
}}
"#,
        pascal_name = pascal_name,
        pascal_fw = pascal_fw,
    )
}

/// Generate a minimal `LanguageParser` skeleton for `name` / `language`.
///
/// The returned string is valid Rust source. The `parse` implementation stores
/// the source in a [`ParsedFile`] with an empty diagnostics list. Community
/// authors replace the body with their actual parser.
///
/// # Example
/// ```
/// let src = infergen_core::plugin::parser_scaffold("lua", "lua");
/// assert!(src.contains("LuaParser"));
/// assert!(src.contains("impl LanguageParser"));
/// ```
pub fn parser_scaffold(name: &str, language: &str) -> String {
    let name = name.trim();
    let name = if name.is_empty() { "plugin" } else { name };
    let language = language.trim();
    let language = if language.is_empty() { name } else { language };
    let pascal_name = to_pascal_case(name);
    let pascal_lang = to_pascal_case(language);
    format!(
        r#"use std::path::{{Path, PathBuf}};
use infergen_core::{{Diagnostic, Language, LanguageParser, ParsedFile, Result}};

pub struct {pascal_name}Parser;

impl LanguageParser for {pascal_name}Parser {{
    fn parse(&self, path: &Path, source: &str) -> Result<ParsedFile> {{
        // TODO: parse `source` with your language's parser library.
        // Non-fatal syntax errors → push to `diagnostics`; do NOT return Err.
        // Only infrastructure failures (I/O, OOM) should return Err.
        let diagnostics: Vec<Diagnostic> = vec![];
        Ok(ParsedFile {{
            path: PathBuf::from(path),
            // TODO: add Language::{pascal_lang} to infergen-core, or use the closest
            // existing variant and file an issue / PR to add your language.
            lang: todo!("map to a Language variant"),
            source: source.to_owned(),
            diagnostics,
        }})
    }}
}}
"#,
        pascal_name = pascal_name,
        pascal_lang = pascal_lang,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_pascal_case_hyphen() {
        assert_eq!(to_pascal_case("debug-log"), "DebugLog");
    }

    #[test]
    fn to_pascal_case_underscore() {
        assert_eq!(to_pascal_case("my_parser"), "MyParser");
    }

    #[test]
    fn to_pascal_case_single_word() {
        assert_eq!(to_pascal_case("posthog"), "Posthog");
    }

    #[test]
    fn to_pascal_case_empty_returns_plugin() {
        assert_eq!(to_pascal_case(""), "Plugin");
        assert_eq!(to_pascal_case("  "), "Plugin"); // trimmed in callers
    }

    #[test]
    fn provider_scaffold_struct_and_impl() {
        let src = provider_scaffold("debug-log");
        assert!(src.contains("struct DebugLogProvider"), "missing struct: {src}");
        assert!(src.contains("impl ProviderPlugin for DebugLogProvider"), "missing impl: {src}");
        assert!(src.contains("fn id"), "missing id: {src}");
        assert!(src.contains("fn track"), "missing track: {src}");
    }

    #[test]
    fn provider_scaffold_id_matches_name() {
        let src = provider_scaffold("my-analytics");
        assert!(src.contains(r#""my-analytics""#), "id mismatch: {src}");
    }

    #[test]
    fn provider_scaffold_empty_name_does_not_panic() {
        let src = provider_scaffold("");
        assert!(!src.is_empty());
        assert!(src.contains("ProviderPlugin"));
    }

    #[test]
    fn adapter_scaffold_struct_and_impl() {
        let src = adapter_scaffold("my-fw", "htmx");
        assert!(src.contains("struct MyFwAdapter"), "missing struct: {src}");
        assert!(src.contains("impl Adapter for MyFwAdapter"), "missing impl: {src}");
        assert!(src.contains("fn framework"), "missing framework: {src}");
        assert!(src.contains("fn analyze"), "missing analyze: {src}");
    }

    #[test]
    fn adapter_scaffold_pascal_framework() {
        let src = adapter_scaffold("test", "my-framework");
        assert!(src.contains("MyFramework"), "expected pascal framework: {src}");
    }

    #[test]
    fn parser_scaffold_struct_and_impl() {
        let src = parser_scaffold("lua", "lua");
        assert!(src.contains("struct LuaParser"), "missing struct: {src}");
        assert!(src.contains("impl LanguageParser for LuaParser"), "missing impl: {src}");
        assert!(src.contains("fn parse"), "missing parse: {src}");
    }

    #[test]
    fn parser_scaffold_pascal_language() {
        let src = parser_scaffold("kotlin", "kotlin");
        assert!(src.contains("Kotlin"), "expected pascal language: {src}");
    }
}
