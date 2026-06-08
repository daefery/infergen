//! Minimal passthrough parser for Vue single-file components (`.vue`).
//!
//! [`VueParser`] stores the raw source verbatim; no SFC template parsing is
//! performed. Enables path-based detection in [`crate::adapter::vue::VueAdapter`]
//! without requiring a full template compiler dependency.

use std::path::Path;

use crate::{Result, detect::Language};

use super::{LanguageParser, ParsedFile};

/// Passthrough parser for `.vue` single-file component files.
///
/// Accepts only the `.vue` extension. Source is stored as-is; adapters that
/// need content use `file.source` directly or operate purely on the file path.
pub struct VueParser;

impl LanguageParser for VueParser {
    fn parse(&self, path: &Path, source: &str) -> Result<ParsedFile> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("vue") => Ok(ParsedFile {
                path: path.to_owned(),
                lang: Language::Vue,
                source: source.to_owned(),
                diagnostics: vec![],
            }),
            _ => Err(crate::Error::NotImplemented(
                "VueParser only accepts .vue files",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn parse(path: &str, src: &str) -> Result<ParsedFile> {
        VueParser.parse(&PathBuf::from(path), src)
    }

    #[test]
    fn vue_parser_accepts_vue_extension() {
        let f = parse("pages/about.vue", "<template></template>").unwrap();
        assert_eq!(f.lang, Language::Vue);
    }

    #[test]
    fn vue_parser_rejects_ts_extension() {
        assert!(parse("router.ts", "").is_err());
    }

    #[test]
    fn vue_parser_rejects_svelte_extension() {
        assert!(parse("app.svelte", "").is_err());
    }

    #[test]
    fn vue_parser_rejects_js_extension() {
        assert!(parse("main.js", "").is_err());
    }

    #[test]
    fn vue_parser_source_roundtrip() {
        let src = "<template><div>Hello</div></template>";
        let f = parse("comp.vue", src).unwrap();
        assert_eq!(f.source, src);
    }

    #[test]
    fn vue_parser_diagnostics_empty() {
        let f = parse("comp.vue", "<bad>unclosed").unwrap();
        assert!(f.diagnostics.is_empty());
    }

    #[test]
    fn vue_parser_path_roundtrip() {
        let p = PathBuf::from("src/pages/users.vue");
        let f = VueParser.parse(&p, "").unwrap();
        assert_eq!(f.path, p);
    }

    #[test]
    fn vue_parser_empty_source() {
        let f = parse("empty.vue", "").unwrap();
        assert_eq!(f.source, "");
        assert_eq!(f.lang, Language::Vue);
    }

    #[test]
    fn vue_parser_template_source_stored_verbatim() {
        let src = "<template>\n  <h1>Title</h1>\n</template>\n<script setup>\nconst x = 1\n</script>";
        let f = parse("page.vue", src).unwrap();
        assert_eq!(f.source, src);
    }

    #[test]
    fn vue_parser_is_language_parser() {
        fn _accepts(_: &dyn LanguageParser) {}
        _accepts(&VueParser);
    }

    #[test]
    fn vue_parser_no_extension_errors() {
        assert!(parse("Makefile", "").is_err());
    }
}
