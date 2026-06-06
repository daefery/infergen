//! OXC-backed TypeScript / JavaScript parser.
//!
//! [`JsParser`] implements [`LanguageParser`] and exposes
//! [`JsParser::with_program`] for direct OXC AST traversal.  The arena
//! lifetime never escapes: callers must return owned data from the closure.

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::Program;
use oxc_parser::Parser as OxcParser;
use oxc_span::SourceType;

use crate::{Result, detect::Language};

use super::{Diagnostic, LanguageParser, ParsedFile};

/// OXC-backed TypeScript/JavaScript parser.
///
/// Stateless; safe to share across threads.
pub struct JsParser;

impl JsParser {
    /// Infer [`Language`] and OXC [`SourceType`] from a file path's extension.
    ///
    /// Falls back to `(JavaScript, SourceType::default())` for unknown extensions
    /// so callers never get an error purely from extension mismatch.
    fn source_type_for_path(path: &Path) -> (Language, SourceType) {
        match path.extension().and_then(|e| e.to_str()) {
            Some("ts") => (Language::TypeScript, SourceType::ts()),
            Some("tsx") => (Language::TypeScript, SourceType::tsx()),
            Some("js") => (Language::JavaScript, SourceType::mjs()),
            Some("jsx") => (Language::JavaScript, SourceType::jsx()),
            Some("mjs") => (Language::JavaScript, SourceType::mjs()),
            Some("cjs") => (Language::JavaScript, SourceType::cjs()),
            _ => (Language::JavaScript, SourceType::default()),
        }
    }

    /// Allocate an OXC arena, parse `source`, and call `f` with the resulting
    /// [`Program`].  The result `R` must be owned (it cannot borrow from the
    /// arena — the HRTB `for<'a>` enforces this).
    ///
    /// This is the primary AST entry point for framework adapters (E0.4+).
    pub fn with_program<F, R>(path: &Path, source: &str, f: F) -> R
    where
        F: for<'a> FnOnce(&'a Program<'a>) -> R,
    {
        let allocator = Allocator::default();
        let (_, source_type) = Self::source_type_for_path(path);
        let ret = OxcParser::new(&allocator, source, source_type).parse();
        f(&ret.program)
    }
}

impl LanguageParser for JsParser {
    /// Parse `source` from `path`.  Always returns `Ok`; syntax errors go
    /// into [`ParsedFile::diagnostics`] rather than `Err(...)`.
    fn parse(&self, path: &Path, source: &str) -> Result<ParsedFile> {
        let allocator = Allocator::default();
        let (lang, source_type) = Self::source_type_for_path(path);
        let ret = OxcParser::new(&allocator, source, source_type).parse();

        let diagnostics = ret
            .errors
            .iter()
            .map(|e| Diagnostic {
                // OxcDiagnostic.message is Cow<'static, str>.
                message: e.message.to_string(),
                // Span extraction requires miette::LabeledSpan; deferred to E0.4.
                start: 0,
                end: 0,
            })
            .collect();

        Ok(ParsedFile {
            path: path.to_path_buf(),
            lang,
            source: source.to_owned(),
            diagnostics,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    const VALID_TS: &str = r#"
interface User { id: number; name: string; }
function greet(user: User): string {
    return `Hello, ${user.name}`;
}
export { greet };
"#;

    const VALID_JS: &str = r#"
function add(a, b) { return a + b; }
export default add;
"#;

    // OXC is error-tolerant; an unclosed brace still returns a partial program.
    const BROKEN_JS: &str = "const x = {";

    #[test]
    fn source_type_ts_extension() {
        let (lang, st) = JsParser::source_type_for_path(Path::new("app.ts"));
        assert_eq!(lang, Language::TypeScript);
        assert!(st.is_typescript());
        assert!(!st.is_jsx());
    }

    #[test]
    fn source_type_tsx_extension() {
        let (lang, st) = JsParser::source_type_for_path(Path::new("App.tsx"));
        assert_eq!(lang, Language::TypeScript);
        assert!(st.is_typescript());
        assert!(st.is_jsx());
    }

    #[test]
    fn source_type_js_extension() {
        let (lang, st) = JsParser::source_type_for_path(Path::new("index.js"));
        assert_eq!(lang, Language::JavaScript);
        assert!(!st.is_typescript());
    }

    #[test]
    fn source_type_jsx_extension() {
        let (lang, st) = JsParser::source_type_for_path(Path::new("App.jsx"));
        assert_eq!(lang, Language::JavaScript);
        assert!(st.is_jsx());
    }

    #[test]
    fn source_type_unknown_extension_fallback() {
        let (lang, _) = JsParser::source_type_for_path(Path::new("file.unknown"));
        assert_eq!(lang, Language::JavaScript);
    }

    #[test]
    fn parse_valid_typescript_no_errors() {
        let parser = JsParser;
        let file = parser.parse(Path::new("app.ts"), VALID_TS).unwrap();
        assert!(!file.has_errors());
        assert_eq!(file.lang, Language::TypeScript);
        assert_eq!(file.path, PathBuf::from("app.ts"));
        assert_eq!(file.source, VALID_TS);
    }

    #[test]
    fn parse_valid_javascript_no_errors() {
        let parser = JsParser;
        let file = parser.parse(Path::new("index.js"), VALID_JS).unwrap();
        assert!(!file.has_errors());
        assert_eq!(file.lang, Language::JavaScript);
    }

    #[test]
    fn parse_broken_js_returns_ok_with_diagnostics() {
        let parser = JsParser;
        // parse() must never return Err for syntax problems.
        let file = parser.parse(Path::new("broken.js"), BROKEN_JS).unwrap();
        assert!(file.has_errors());
        assert!(!file.diagnostics.is_empty());
        assert!(!file.diagnostics[0].message.is_empty());
    }

    #[test]
    fn parse_empty_source_no_errors() {
        let parser = JsParser;
        let file = parser.parse(Path::new("empty.ts"), "").unwrap();
        assert!(!file.has_errors());
    }

    #[test]
    fn with_program_counts_statements() {
        let count =
            JsParser::with_program(Path::new("app.ts"), VALID_TS, |prog| prog.body.len());
        // interface + function + export = 3 statements
        assert_eq!(count, 3);
    }

    #[test]
    fn with_program_can_extract_owned_strings() {
        // Verifies callers can extract owned data (Vec<String>) from the arena.
        let names: Vec<String> = JsParser::with_program(
            Path::new("app.ts"),
            "function foo() {} function bar() {}",
            |prog| {
                prog.body
                    .iter()
                    .filter_map(|s| {
                        if let oxc_ast::ast::Statement::FunctionDeclaration(f) = s {
                            f.id.as_ref().map(|id| id.name.to_string())
                        } else {
                            None
                        }
                    })
                    .collect()
            },
        );
        assert_eq!(names, vec!["foo", "bar"]);
    }
}
