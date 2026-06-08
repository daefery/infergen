//! Language parser abstraction.
//!
//! [`LanguageParser`] is the contract every language parser must satisfy.
//! [`ParsedFile`] is the common output: path + language + owned source +
//! non-fatal diagnostics.  JS/TS callers then call [`ParsedFile::with_js_program`]
//! to walk the OXC AST; future language parsers will add analogous methods.

use std::path::{Path, PathBuf};

use crate::{Result, detect::Language};

pub mod go;
pub mod js;
pub mod py;

/// A non-fatal diagnostic emitted during parsing (syntax error or warning).
///
/// Parsers are error-tolerant: a file with syntax errors still produces a
/// [`ParsedFile`]; the errors are collected here rather than propagated as
/// `Err(...)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Human-readable error or warning message.
    pub message: String,
    /// Byte offset of the diagnostic start in the source. `0` when unknown.
    pub start: u32,
    /// Byte offset of the diagnostic end in the source. `0` when unknown.
    pub end: u32,
}

/// A successfully parsed source file.
///
/// Holds the owned source text so callers can re-enter the language-specific
/// AST on demand (e.g. [`ParsedFile::with_js_program`]) without re-reading disk.
#[derive(Debug)]
pub struct ParsedFile {
    /// Canonical path on disk.
    pub path: PathBuf,
    /// Detected or declared language.
    pub lang: Language,
    /// Full source text (owned).
    pub source: String,
    /// Non-fatal diagnostics from the parser.  May be non-empty even when
    /// the parse succeeded (partial recovery).
    pub diagnostics: Vec<Diagnostic>,
}

impl ParsedFile {
    /// `true` if the parser emitted any diagnostics (errors or warnings).
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    /// Re-parse this file's source and call `f` with the OXC [`Program`].
    ///
    /// Returns `None` when [`self.lang`][ParsedFile::lang] is not
    /// [`Language::TypeScript`] or [`Language::JavaScript`].  The result `R`
    /// must be owned: it cannot borrow from the arena (enforced by the HRTB).
    ///
    /// [`Program`]: oxc_ast::ast::Program
    pub fn with_js_program<F, R>(&self, f: F) -> Option<R>
    where
        F: for<'a> FnOnce(&'a oxc_ast::ast::Program<'a>) -> R,
    {
        if matches!(self.lang, Language::TypeScript | Language::JavaScript) {
            Some(js::JsParser::with_program(&self.path, &self.source, f))
        } else {
            None
        }
    }

    /// Scan this file's source as Python and call `f` with the structural
    /// statements produced by [`py::PyParser::scan`].
    ///
    /// Returns `R::default()` when [`self.lang`][ParsedFile::lang] is not
    /// [`Language::Python`].  The Python "AST" is fully owned — no arena
    /// lifetime issues.
    pub fn with_py_ast<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[py::PyStmt]) -> R,
        R: Default,
    {
        if self.lang == Language::Python {
            let stmts = py::PyParser::scan(&self.source);
            f(&stmts)
        } else {
            R::default()
        }
    }

    /// Call `f` with the raw Go source text.
    ///
    /// Returns `None` when [`self.lang`][ParsedFile::lang] is not
    /// [`Language::Go`].  Go adapters do text-based pattern matching directly
    /// on the source string rather than walking a structured AST.
    #[must_use]
    pub fn with_go_source<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&str) -> R,
    {
        if self.lang == Language::Go {
            Some(f(&self.source))
        } else {
            None
        }
    }
}

/// Contract for language parsers.
///
/// Implementations must be **error-tolerant**: source with syntax errors
/// returns `Ok(ParsedFile { diagnostics: … })`, not `Err(…)`.
/// Only I/O failures or unsupported paths should return `Err`.
pub trait LanguageParser {
    /// Parse `source` read from `path`.
    ///
    /// # Errors
    /// Returns `Err` only for infrastructure failures (unsupported extension,
    /// internal allocator panic).  Syntax errors go into `ParsedFile.diagnostics`.
    fn parse(&self, path: &Path, source: &str) -> Result<ParsedFile>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_errors_false_when_empty() {
        let f = ParsedFile {
            path: PathBuf::from("x.ts"),
            lang: Language::TypeScript,
            source: String::new(),
            diagnostics: vec![],
        };
        assert!(!f.has_errors());
    }

    #[test]
    fn has_errors_true_when_populated() {
        let f = ParsedFile {
            path: PathBuf::from("x.ts"),
            lang: Language::TypeScript,
            source: String::new(),
            diagnostics: vec![Diagnostic {
                message: "unexpected token".into(),
                start: 0,
                end: 5,
            }],
        };
        assert!(f.has_errors());
    }

    #[test]
    fn with_js_program_none_for_python() {
        let f = ParsedFile {
            path: PathBuf::from("script.py"),
            lang: Language::Python,
            source: "print('hello')".to_owned(),
            diagnostics: vec![],
        };
        assert!(f.with_js_program(|_| ()).is_none());
    }

    #[test]
    fn with_py_ast_default_for_typescript() {
        let f = ParsedFile {
            path: PathBuf::from("app.ts"),
            lang: Language::TypeScript,
            source: "const x = 1;".to_owned(),
            diagnostics: vec![],
        };
        let count: usize = f.with_py_ast(|stmts| stmts.len());
        assert_eq!(count, 0); // returns R::default() = 0
    }

    #[test]
    fn with_py_ast_scans_python() {
        let f = ParsedFile {
            path: PathBuf::from("views.py"),
            lang: Language::Python,
            source: "def hello():\n    pass\n".to_owned(),
            diagnostics: vec![],
        };
        let count: usize = f.with_py_ast(|stmts| {
            stmts
                .iter()
                .filter(|s| matches!(s, py::PyStmt::FunctionDef { .. }))
                .count()
        });
        assert_eq!(count, 1);
    }

    #[test]
    fn with_js_program_some_for_typescript() {
        let f = ParsedFile {
            path: PathBuf::from("app.ts"),
            lang: Language::TypeScript,
            source: "const x = 1;".to_owned(),
            diagnostics: vec![],
        };
        let len = f.with_js_program(|prog| prog.body.len());
        assert_eq!(len, Some(1));
    }

    #[test]
    fn with_go_source_some_for_go() {
        let f = ParsedFile {
            path: PathBuf::from("main.go"),
            lang: Language::Go,
            source: "package main\n".to_owned(),
            diagnostics: vec![],
        };
        let len = f.with_go_source(|s| s.len());
        assert_eq!(len, Some("package main\n".len()));
    }

    #[test]
    fn with_go_source_none_for_typescript() {
        let f = ParsedFile {
            path: PathBuf::from("app.ts"),
            lang: Language::TypeScript,
            source: "const x = 1;".to_owned(),
            diagnostics: vec![],
        };
        assert!(f.with_go_source(|_| ()).is_none());
    }

    #[test]
    fn with_go_source_none_for_python() {
        let f = ParsedFile {
            path: PathBuf::from("views.py"),
            lang: Language::Python,
            source: "pass".to_owned(),
            diagnostics: vec![],
        };
        assert!(f.with_go_source(|_| ()).is_none());
    }

    /// Verify the trait is object-safe so it can be used as `dyn LanguageParser`.
    #[test]
    fn language_parser_is_object_safe() {
        fn _accepts(_: &dyn LanguageParser) {}
    }
}
