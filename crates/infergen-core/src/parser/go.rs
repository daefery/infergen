//! Go language parser.
//!
//! [`GoParser`] implements [`LanguageParser`] for `.go` source files using
//! a text-based approach — the Rust host cannot link `go/ast`, so parsing
//! is line-level.  Adapters receive the raw source via
//! [`ParsedFile::with_go_source`] and do their own pattern matching.

use std::path::Path;

use crate::{Error, Result, detect::Language};

use super::{LanguageParser, ParsedFile};

/// Go source file extensions accepted by this parser.
const GO_EXTS: &[&str] = &["go"];

/// Text-based parser for Go source files (`.go`).
///
/// Returns a [`ParsedFile`] with `lang = Language::Go` and the raw source
/// text.  Syntax errors cannot be detected without a real Go parser; the
/// `diagnostics` field is always empty.  Adapters analyse `file.source`
/// directly.
pub struct GoParser;

impl LanguageParser for GoParser {
    fn parse(&self, path: &Path, source: &str) -> Result<ParsedFile> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !GO_EXTS.contains(&ext) {
            return Err(Error::NotImplemented(
                "GoParser: unsupported extension (only .go files accepted)",
            ));
        }
        Ok(ParsedFile {
            path: path.to_owned(),
            lang: Language::Go,
            source: source.to_owned(),
            diagnostics: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn parse(rel: &str, src: &str) -> Result<ParsedFile> {
        GoParser.parse(&PathBuf::from(rel), src)
    }

    #[test]
    fn parse_go_file_returns_language_go() {
        let f = parse("main.go", "package main").unwrap();
        assert_eq!(f.lang, Language::Go);
    }

    #[test]
    fn parse_go_file_source_roundtrips() {
        let src = "package main\n\nfunc main() {}\n";
        let f = parse("main.go", src).unwrap();
        assert_eq!(f.source, src);
    }

    #[test]
    fn parse_go_file_path_preserved() {
        let f = parse("cmd/server/main.go", "package main").unwrap();
        assert_eq!(f.path, PathBuf::from("cmd/server/main.go"));
    }

    #[test]
    fn parse_non_go_extension_errors() {
        let err = parse("main.ts", "").unwrap_err();
        assert!(matches!(err, Error::NotImplemented(_)));
    }

    #[test]
    fn parse_rs_extension_errors() {
        let err = parse("lib.rs", "fn main() {}").unwrap_err();
        assert!(matches!(err, Error::NotImplemented(_)));
    }

    #[test]
    fn parse_empty_source_ok() {
        let f = parse("empty.go", "").unwrap();
        assert!(f.diagnostics.is_empty());
        assert_eq!(f.source, "");
    }

    #[test]
    fn parse_test_file_accepted() {
        let f = parse("handlers_test.go", "package main\n").unwrap();
        assert_eq!(f.lang, Language::Go);
    }

    #[test]
    fn go_parser_is_language_parser() {
        fn _accepts(_: &dyn LanguageParser) {}
        _accepts(&GoParser);
    }

    #[test]
    fn diagnostics_always_empty() {
        // GoParser is text-only; it never emits diagnostics.
        let f = parse("broken.go", "this is not valid go syntax {{{").unwrap();
        assert!(f.diagnostics.is_empty());
    }
}
