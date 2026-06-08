//! Minimal passthrough parser for Svelte components (`.svelte`).
//!
//! [`SvelteParser`] stores the raw source verbatim; no template parsing is
//! performed. Enables path-based detection in
//! [`crate::adapter::svelte_kit::SvelteKitAdapter`] without requiring a Svelte
//! compiler dependency.

use std::path::Path;

use crate::{Result, detect::Language};

use super::{LanguageParser, ParsedFile};

/// Passthrough parser for `.svelte` component files.
///
/// Accepts only the `.svelte` extension. Source is stored as-is; adapters
/// that need content use `file.source` directly or operate purely on the path.
pub struct SvelteParser;

impl LanguageParser for SvelteParser {
    fn parse(&self, path: &Path, source: &str) -> Result<ParsedFile> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("svelte") => Ok(ParsedFile {
                path: path.to_owned(),
                lang: Language::Svelte,
                source: source.to_owned(),
                diagnostics: vec![],
            }),
            _ => Err(crate::Error::NotImplemented(
                "SvelteParser only accepts .svelte files",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn parse(path: &str, src: &str) -> Result<ParsedFile> {
        SvelteParser.parse(&PathBuf::from(path), src)
    }

    #[test]
    fn svelte_parser_accepts_svelte_extension() {
        let f = parse("src/routes/+page.svelte", "<h1>Home</h1>").unwrap();
        assert_eq!(f.lang, Language::Svelte);
    }

    #[test]
    fn svelte_parser_rejects_ts_extension() {
        assert!(parse("router.ts", "").is_err());
    }

    #[test]
    fn svelte_parser_rejects_vue_extension() {
        assert!(parse("app.vue", "").is_err());
    }

    #[test]
    fn svelte_parser_rejects_js_extension() {
        assert!(parse("main.js", "").is_err());
    }

    #[test]
    fn svelte_parser_source_roundtrip() {
        let src = "<script>\n  let count = 0;\n</script>\n<p>{count}</p>";
        let f = parse("comp.svelte", src).unwrap();
        assert_eq!(f.source, src);
    }

    #[test]
    fn svelte_parser_diagnostics_empty() {
        let f = parse("comp.svelte", "unclosed {{").unwrap();
        assert!(f.diagnostics.is_empty());
    }

    #[test]
    fn svelte_parser_path_roundtrip() {
        let p = PathBuf::from("src/routes/about/+page.svelte");
        let f = SvelteParser.parse(&p, "").unwrap();
        assert_eq!(f.path, p);
    }

    #[test]
    fn svelte_parser_empty_source() {
        let f = parse("empty.svelte", "").unwrap();
        assert_eq!(f.source, "");
        assert_eq!(f.lang, Language::Svelte);
    }

    #[test]
    fn svelte_parser_template_source_stored_verbatim() {
        let src = "<script>\n  export let name;\n</script>\n<h1>Hello {name}!</h1>";
        let f = parse("page.svelte", src).unwrap();
        assert_eq!(f.source, src);
    }

    #[test]
    fn svelte_parser_is_language_parser() {
        fn _accepts(_: &dyn LanguageParser) {}
        _accepts(&SvelteParser);
    }

    #[test]
    fn svelte_parser_no_extension_errors() {
        assert!(parse("Makefile", "").is_err());
    }
}
