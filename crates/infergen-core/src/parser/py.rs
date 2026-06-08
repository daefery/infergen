//! Line-scanner Python parser.
//!
//! [`PyParser`] implements [`LanguageParser`] and produces a [`ParsedFile`]
//! with `lang = Python`. [`PyParser::scan`] returns a [`Vec<PyStmt>`] — a
//! lightweight structural view of Python source that adapters consume via
//! [`ParsedFile::with_py_ast`].
//!
//! Design: a full AST crate adds uncertain API surface. A line-scanner is
//! error-tolerant, dependency-free, and sufficient for decorator/def/class/
//! import detection that framework adapters need.

use std::path::Path;

use crate::{Result, detect::Language};

use super::{LanguageParser, ParsedFile};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A lightweight structural statement extracted from Python source.
///
/// Produced by [`PyParser::scan`] and consumed by framework adapters.
/// Order matches source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PyStmt {
    /// A decorator line, e.g. `@app.get("/items")` or `@login_required`.
    /// `text` is the full decorator text after stripping the leading `@` and
    /// surrounding whitespace.
    Decorator {
        /// Decorator text, e.g. `"app.get(\"/items\")"` or `"login_required"`.
        text: String,
    },

    /// A `def` or `async def` statement.
    FunctionDef {
        /// Function name, e.g. `"get_items"`.
        name: String,
        /// Decorators that immediately precede this function (in order).
        decorators: Vec<String>,
        /// Raw parameter list text (everything between the outer parens).
        params: String,
    },

    /// A `class` statement.
    ClassDef {
        /// Class name, e.g. `"ItemView"`.
        name: String,
        /// Base class names parsed from the parenthesised list.
        bases: Vec<String>,
        /// Decorators that immediately precede this class.
        decorators: Vec<String>,
    },

    /// A `from module import name1, name2` statement.
    ImportFrom {
        /// Module path, e.g. `"fastapi.security"`.
        module: String,
        /// Imported names, e.g. `["OAuth2PasswordBearer", "Depends"]`.
        names: Vec<String>,
    },

    /// A bare `import name1, name2` statement.
    Import {
        /// Imported module names.
        names: Vec<String>,
    },

    /// A simple `target = ...` assignment (e.g. `urlpatterns = [...]`).
    Assign {
        /// The assignment target (left-hand side identifier).
        target: String,
        /// Raw right-hand side text (single line; multi-line not tracked).
        value: String,
    },
}

impl PyStmt {
    /// Returns the decorator text if this is a `Decorator` variant.
    pub fn decorator_text(&self) -> Option<&str> {
        if let Self::Decorator { text } = self { Some(text) } else { None }
    }
}

// ---------------------------------------------------------------------------
// PyParser
// ---------------------------------------------------------------------------

/// Line-scanner Python parser.
///
/// Stateless; safe to share across threads. Produces structural statements
/// suitable for framework adapter use without requiring a full AST library.
pub struct PyParser;

impl PyParser {
    /// Scan `source` and return a list of structural statements.
    ///
    /// Error-tolerant: unrecognised lines are silently skipped. Output order
    /// matches source order.
    ///
    /// This is the primary entry point for framework adapters
    /// (via [`ParsedFile::with_py_ast`]).
    #[must_use]
    pub fn scan(source: &str) -> Vec<PyStmt> {
        let mut stmts = Vec::new();
        let mut pending_decorators: Vec<String> = Vec::new();

        for raw_line in source.lines() {
            let line = raw_line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // --- Decorator ---------------------------------------------------
            if let Some(rest) = line.strip_prefix('@') {
                let text = rest.trim().to_owned();
                pending_decorators.push(text.clone());
                stmts.push(PyStmt::Decorator { text });
                continue;
            }

            // --- async def / def ---------------------------------------------
            let def_rest = if let Some(r) = line.strip_prefix("async def ") {
                Some(r)
            } else {
                line.strip_prefix("def ")
            };

            if let Some(rest) = def_rest {
                if let Some((name, params)) = parse_def(rest) {
                    stmts.push(PyStmt::FunctionDef {
                        name,
                        decorators: std::mem::take(&mut pending_decorators),
                        params,
                    });
                    continue;
                }
                pending_decorators.clear();
                continue;
            }

            // --- class -------------------------------------------------------
            if let Some(rest) = line.strip_prefix("class ") {
                if let Some((name, bases)) = parse_class(rest) {
                    stmts.push(PyStmt::ClassDef {
                        name,
                        bases,
                        decorators: std::mem::take(&mut pending_decorators),
                    });
                } else {
                    pending_decorators.clear();
                }
                continue;
            }

            // A non-decorator, non-def, non-class line clears pending decorators.
            // Exception: blank lines and comments already handled above.
            pending_decorators.clear();

            // --- from … import -----------------------------------------------
            if let Some(rest) = line.strip_prefix("from ") {
                if let Some(stmt) = parse_import_from(rest) {
                    stmts.push(stmt);
                }
                continue;
            }

            // --- import -------------------------------------------------------
            if let Some(rest) = line.strip_prefix("import ") {
                let names = split_names(rest);
                if !names.is_empty() {
                    stmts.push(PyStmt::Import { names });
                }
                continue;
            }

            // --- simple assignment (e.g. urlpatterns = [...]) -----------------
            if let Some((target, value)) = parse_assign(line) {
                stmts.push(PyStmt::Assign { target, value });
            }
        }

        stmts
    }
}

// ---------------------------------------------------------------------------
// LanguageParser impl
// ---------------------------------------------------------------------------

impl LanguageParser for PyParser {
    fn parse(&self, path: &Path, source: &str) -> Result<ParsedFile> {
        Ok(ParsedFile {
            path: path.to_path_buf(),
            lang: Language::Python,
            source: source.to_owned(),
            diagnostics: Vec::new(),
        })
    }
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// Parse the rest of a `def`/`async def` line after the `def ` prefix:
/// `get_items(item_id: int, db: Session = Depends()):` → `("get_items", "item_id: int, db: Session = Depends()")`
fn parse_def(rest: &str) -> Option<(String, String)> {
    let paren = rest.find('(')?;
    let name = rest[..paren].trim().to_owned();
    if name.is_empty() || !is_valid_identifier(&name) {
        return None;
    }
    // Extract parameter text between the outermost parens (may not close on
    // this line for multi-line defs — return empty string in that case).
    let after_open = &rest[paren + 1..];
    let params = if let Some(close) = after_open.rfind(')') {
        after_open[..close].trim().to_owned()
    } else {
        String::new()
    };
    Some((name, params))
}

/// Parse the rest of a `class ` line after the `class ` prefix:
/// `ItemView(DetailView):` → `("ItemView", ["DetailView"])`
fn parse_class(rest: &str) -> Option<(String, Vec<String>)> {
    let colon = rest.find(':')?;
    let head = &rest[..colon];
    let (name, bases) = if let Some(paren) = head.find('(') {
        let n = head[..paren].trim().to_owned();
        let inner = head[paren + 1..].trim_end_matches(')').trim();
        (n, split_names(inner))
    } else {
        (head.trim().to_owned(), Vec::new())
    };
    if name.is_empty() || !is_valid_identifier(&name) {
        return None;
    }
    Some((name, bases))
}

/// Parse a `from X import Y, Z` statement (rest = text after `from `).
fn parse_import_from(rest: &str) -> Option<PyStmt> {
    let import_pos = rest.find(" import ")?;
    let module = rest[..import_pos].trim().to_owned();
    let names_str = rest[import_pos + " import ".len()..].trim();
    let names = parse_import_names(names_str);
    if module.is_empty() {
        return None;
    }
    Some(PyStmt::ImportFrom { module, names })
}

/// Parse a simple `target = value` assignment (single-line only).
fn parse_assign(line: &str) -> Option<(String, String)> {
    // Avoid matching augmented assignments (+=, -=, etc.) and comparisons (==).
    if line.contains("==") {
        return None;
    }
    let eq = line.find(" = ")?;
    let target = line[..eq].trim().to_owned();
    let value = line[eq + 3..].trim().to_owned();
    if target.is_empty() || !is_valid_identifier(&target) {
        return None;
    }
    Some((target, value))
}

/// Split a comma-separated identifier list, stripping whitespace and aliases
/// (`name as alias` → `name`), and parentheses (for grouped imports).
fn parse_import_names(s: &str) -> Vec<String> {
    let cleaned = s.trim_start_matches('(').trim_end_matches(')');
    split_names(cleaned)
        .into_iter()
        .map(|n| {
            // Handle `Name as alias` → take the first word (the real name).
            n.split_whitespace().next().unwrap_or("").to_owned()
        })
        .filter(|n| !n.is_empty())
        .collect()
}

/// Split a comma-separated list of names, stripping individual whitespace and
/// trailing/leading punctuation.
fn split_names(s: &str) -> Vec<String> {
    s.split(',')
        .map(|p| p.trim().to_owned())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Returns `true` if `s` looks like a valid Python identifier (ASCII subset).
fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- LanguageParser impl -------------------------------------------------

    #[test]
    fn parse_sets_language_python() {
        let f = PyParser.parse(Path::new("views.py"), "def hello(): pass").unwrap();
        assert_eq!(f.lang, Language::Python);
    }

    #[test]
    fn parse_always_returns_ok() {
        // Even totally invalid source must not return Err.
        let f = PyParser.parse(Path::new("broken.py"), "def )(broken").unwrap();
        assert_eq!(f.lang, Language::Python);
    }

    #[test]
    fn parse_empty_source_no_errors() {
        let f = PyParser.parse(Path::new("empty.py"), "").unwrap();
        assert!(f.diagnostics.is_empty());
    }

    #[test]
    fn parse_source_stored_verbatim() {
        let src = "x = 1\n";
        let f = PyParser.parse(Path::new("x.py"), src).unwrap();
        assert_eq!(f.source, src);
    }

    // --- PyParser::scan — functions ------------------------------------------

    #[test]
    fn scan_simple_function() {
        let stmts = PyParser::scan("def greet(name: str) -> str:\n    pass\n");
        let func: Vec<_> = stmts
            .iter()
            .filter(|s| matches!(s, PyStmt::FunctionDef { .. }))
            .collect();
        assert_eq!(func.len(), 1);
        if let PyStmt::FunctionDef { name, .. } = &func[0] {
            assert_eq!(name, "greet");
        }
    }

    #[test]
    fn scan_async_function() {
        let stmts = PyParser::scan("async def get_items():\n    pass\n");
        let funcs: Vec<_> = stmts
            .iter()
            .filter(|s| matches!(s, PyStmt::FunctionDef { .. }))
            .collect();
        assert_eq!(funcs.len(), 1);
        if let PyStmt::FunctionDef { name, .. } = &funcs[0] {
            assert_eq!(name, "get_items");
        }
    }

    #[test]
    fn scan_function_with_decorator() {
        let src = "@app.get(\"/items\")\nasync def list_items():\n    pass\n";
        let stmts = PyParser::scan(src);
        let func = stmts
            .iter()
            .find(|s| matches!(s, PyStmt::FunctionDef { .. }))
            .unwrap();
        if let PyStmt::FunctionDef { decorators, .. } = func {
            assert_eq!(decorators.len(), 1);
            assert!(decorators[0].contains("app.get"));
        }
    }

    #[test]
    fn scan_multiple_decorators() {
        let src =
            "@require_auth\n@app.route(\"/admin\")\ndef admin_view():\n    pass\n";
        let stmts = PyParser::scan(src);
        let func = stmts
            .iter()
            .find(|s| matches!(s, PyStmt::FunctionDef { .. }))
            .unwrap();
        if let PyStmt::FunctionDef { decorators, .. } = func {
            assert_eq!(decorators.len(), 2);
        }
    }

    // --- PyParser::scan — classes --------------------------------------------

    #[test]
    fn scan_class_with_base() {
        let stmts = PyParser::scan("class MyView(DetailView):\n    pass\n");
        let cls = stmts
            .iter()
            .find(|s| matches!(s, PyStmt::ClassDef { .. }))
            .unwrap();
        if let PyStmt::ClassDef { name, bases, .. } = cls {
            assert_eq!(name, "MyView");
            assert!(bases.contains(&"DetailView".to_owned()));
        }
    }

    #[test]
    fn scan_class_multiple_bases() {
        let stmts = PyParser::scan("class Mixin(LoginRequiredMixin, DetailView):\n    pass\n");
        let cls = stmts
            .iter()
            .find(|s| matches!(s, PyStmt::ClassDef { .. }))
            .unwrap();
        if let PyStmt::ClassDef { bases, .. } = cls {
            assert_eq!(bases.len(), 2);
        }
    }

    #[test]
    fn scan_class_no_bases() {
        let stmts = PyParser::scan("class MyModel:\n    pass\n");
        let cls = stmts
            .iter()
            .find(|s| matches!(s, PyStmt::ClassDef { .. }))
            .unwrap();
        if let PyStmt::ClassDef { bases, .. } = cls {
            assert!(bases.is_empty());
        }
    }

    // --- PyParser::scan — imports --------------------------------------------

    #[test]
    fn scan_import_from() {
        let stmts = PyParser::scan("from fastapi.security import OAuth2PasswordBearer\n");
        let imp = stmts
            .iter()
            .find(|s| matches!(s, PyStmt::ImportFrom { .. }))
            .unwrap();
        if let PyStmt::ImportFrom { module, names } = imp {
            assert_eq!(module, "fastapi.security");
            assert!(names.contains(&"OAuth2PasswordBearer".to_owned()));
        }
    }

    #[test]
    fn scan_import_multiple_names() {
        let stmts = PyParser::scan("from flask_login import login_user, logout_user, current_user\n");
        let imp = stmts
            .iter()
            .find(|s| matches!(s, PyStmt::ImportFrom { .. }))
            .unwrap();
        if let PyStmt::ImportFrom { names, .. } = imp {
            assert_eq!(names.len(), 3);
        }
    }

    #[test]
    fn scan_bare_import() {
        let stmts = PyParser::scan("import django\n");
        let imp = stmts
            .iter()
            .find(|s| matches!(s, PyStmt::Import { .. }))
            .unwrap();
        if let PyStmt::Import { names } = imp {
            assert!(names.contains(&"django".to_owned()));
        }
    }

    // --- PyParser::scan — assignment -----------------------------------------

    #[test]
    fn scan_assignment() {
        let stmts = PyParser::scan("urlpatterns = [\n    path(\"/users/\", views.UserList),\n]\n");
        let assign = stmts
            .iter()
            .find(|s| matches!(s, PyStmt::Assign { .. }))
            .unwrap();
        if let PyStmt::Assign { target, .. } = assign {
            assert_eq!(target, "urlpatterns");
        }
    }

    // --- PyParser::scan — edge cases -----------------------------------------

    #[test]
    fn scan_empty_source_returns_empty() {
        assert!(PyParser::scan("").is_empty());
    }

    #[test]
    fn scan_comments_skipped() {
        let stmts = PyParser::scan("# This is a comment\n# another\n");
        assert!(stmts.is_empty());
    }

    #[test]
    fn scan_mixed_source() {
        let src = r#"
from fastapi import FastAPI
app = FastAPI()

@app.get("/items")
async def list_items():
    pass

class ItemView(DetailView):
    pass
"#;
        let stmts = PyParser::scan(src);
        let func_count = stmts
            .iter()
            .filter(|s| matches!(s, PyStmt::FunctionDef { .. }))
            .count();
        let class_count = stmts
            .iter()
            .filter(|s| matches!(s, PyStmt::ClassDef { .. }))
            .count();
        assert!(func_count >= 1);
        assert!(class_count >= 1);
    }
}
