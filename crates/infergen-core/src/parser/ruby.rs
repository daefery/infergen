//! Line-scanner Ruby parser.
//!
//! [`RubyParser`] implements [`LanguageParser`] and produces a [`ParsedFile`]
//! with `lang = Ruby`. [`RubyParser::scan`] returns a [`Vec<RubyStmt>`] — a
//! lightweight structural view of Ruby source that adapters consume via
//! [`ParsedFile::with_ruby_stmts`].
//!
//! Design: a full Prism (C library) FFI adds heavy build complexity. A
//! line-scanner is error-tolerant, dependency-free, and sufficient for
//! route/controller/Devise detection that the Rails adapter needs.

use std::path::Path;

use crate::{Result, detect::Language};

use super::{LanguageParser, ParsedFile};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A lightweight structural statement extracted from Ruby source.
///
/// Produced by [`RubyParser::scan`] and consumed by framework adapters.
/// Order matches source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RubyStmt {
    /// An HTTP route declaration.
    ///
    /// Examples: `get "/users", to: "users#index"` or `get "/users" => "users#index"`.
    Route {
        /// HTTP method in lowercase, e.g. `"get"`, `"post"`.
        method: String,
        /// Route path, e.g. `"/users"` or `"/:id"`.
        path: String,
        /// Controller and action in `"controller#action"` form. Empty when not present.
        controller_action: String,
    },

    /// A `resources :name` plural resource macro.
    Resources {
        /// Resource name, e.g. `"users"`.
        name: String,
    },

    /// A `resource :name` singular resource macro.
    Resource {
        /// Resource name, e.g. `"profile"`.
        name: String,
    },

    /// A `devise_for :users` Devise auth route helper.
    DeviseFor {
        /// Resource name, e.g. `"users"`.
        resource: String,
    },

    /// A `class Foo < Bar` or `class Foo` declaration.
    ClassDef {
        /// Class name, e.g. `"UsersController"`.
        name: String,
        /// Parent class name, e.g. `"ApplicationController"`. Empty when absent.
        parent: String,
    },

    /// A `def method_name` declaration.
    MethodDef {
        /// Method name, e.g. `"index"`.
        name: String,
    },

    /// A `require "name"` or `require 'name'` statement.
    Require {
        /// Required name, e.g. `"devise"`.
        name: String,
    },

    /// A `namespace :api do` or `scope "/api" do` block opener.
    ScopeBegin {
        /// Kind: `"namespace"` or `"scope"`.
        kind: String,
        /// Label: the symbol or path string, e.g. `"api"` or `"/api"`.
        label: String,
    },
}

// ---------------------------------------------------------------------------
// RubyParser
// ---------------------------------------------------------------------------

/// Line-scanner Ruby parser.
///
/// Stateless; safe to share across threads. Produces structural statements
/// suitable for framework adapter use without requiring a full Prism AST.
pub struct RubyParser;

/// HTTP route methods supported in Rails routing DSL.
const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "head", "options", "match",
];

impl RubyParser {
    /// Scan `source` and return a list of structural statements.
    ///
    /// Error-tolerant: unrecognised lines are silently skipped. Output order
    /// matches source order.
    #[must_use]
    pub fn scan(source: &str) -> Vec<RubyStmt> {
        let mut stmts = Vec::new();

        for raw_line in source.lines() {
            let line = raw_line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // --- Route (get/post/put/…) ------------------------------------
            if let Some(stmt) = try_parse_route(line) {
                stmts.push(stmt);
                continue;
            }

            // --- resources / resource ---------------------------------------
            if let Some(name) = strip_symbol_after(line, "resources ") {
                stmts.push(RubyStmt::Resources { name });
                continue;
            }
            // `resource` must not accidentally match `resources`
            if !line.starts_with("resources") {
                if let Some(name) = strip_symbol_after(line, "resource ") {
                    stmts.push(RubyStmt::Resource { name });
                    continue;
                }
            }

            // --- devise_for -------------------------------------------------
            if let Some(name) = strip_symbol_after(line, "devise_for ") {
                stmts.push(RubyStmt::DeviseFor { resource: name });
                continue;
            }

            // --- namespace / scope ------------------------------------------
            if let Some(stmt) = try_parse_scope(line) {
                stmts.push(stmt);
                continue;
            }

            // --- class ------------------------------------------------------
            if let Some(stmt) = try_parse_class(line) {
                stmts.push(stmt);
                continue;
            }

            // --- def --------------------------------------------------------
            if let Some(stmt) = try_parse_def(line) {
                stmts.push(stmt);
                continue;
            }

            // --- require ----------------------------------------------------
            if let Some(stmt) = try_parse_require(line) {
                stmts.push(stmt);
            }
        }

        stmts
    }
}

// ---------------------------------------------------------------------------
// Private parse helpers
// ---------------------------------------------------------------------------

/// Extract a symbol or string name from after a keyword prefix.
///
/// `"resources :users"` → `Some("users")`.
/// `"resources :users, only: [...]"` → `Some("users")`.
fn strip_symbol_after(line: &str, prefix: &str) -> Option<String> {
    let rest = line.strip_prefix(prefix)?.trim_start();
    // Symbol: `:name`
    if let Some(sym) = rest.strip_prefix(':') {
        let end = sym
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(sym.len());
        let name = &sym[..end];
        if name.is_empty() {
            return None;
        }
        return Some(name.to_owned());
    }
    None
}

/// Parse a quoted string value from `s` (single or double quotes).
fn extract_quoted(s: &str) -> Option<String> {
    for delim in ['"', '\''] {
        if let Some(open) = s.find(delim) {
            let inner = &s[open + 1..];
            if let Some(close) = inner.find(delim) {
                return Some(inner[..close].to_owned());
            }
        }
    }
    None
}

/// Try to parse a route line: `get "/path", to: "ctrl#action"` etc.
fn try_parse_route(line: &str) -> Option<RubyStmt> {
    let (method, rest) = {
        let mut found = None;
        for m in ROUTE_METHODS {
            let prefix = format!("{m} ");
            if let Some(r) = line.strip_prefix(prefix.as_str()) {
                found = Some((m.to_string(), r.trim_start()));
                break;
            }
        }
        found?
    };

    // Extract path (first quoted string or symbol)
    let path = extract_quoted(rest).or_else(|| {
        // Named route shorthand: `get :name`
        if let Some(sym) = rest.strip_prefix(':') {
            let end = sym
                .find(|c: char| !c.is_alphanumeric() && c != '_')
                .unwrap_or(sym.len());
            if !sym[..end].is_empty() {
                return Some(format!(":{}", &sym[..end]));
            }
        }
        None
    })?;

    // Extract controller_action from `to: "ctrl#action"` or `=> "ctrl#action"`
    let controller_action = extract_controller_action(rest).unwrap_or_default();

    Some(RubyStmt::Route { method, path, controller_action })
}

/// Extract the `to:` or `=>` target value from a route line's remaining text.
fn extract_controller_action(rest: &str) -> Option<String> {
    // Pattern 1: `to: "ctrl#action"`
    if let Some(pos) = rest.find("to:") {
        let after = &rest[pos + 3..].trim_start();
        if let Some(val) = extract_quoted(after) {
            return Some(val);
        }
    }
    // Pattern 2: `=> "ctrl#action"`
    if let Some(pos) = rest.find("=>") {
        let after = &rest[pos + 2..].trim_start();
        if let Some(val) = extract_quoted(after) {
            return Some(val);
        }
    }
    None
}

/// Try to parse `class Foo < Bar` or `class Foo`.
fn try_parse_class(line: &str) -> Option<RubyStmt> {
    let rest = line.strip_prefix("class ")?;
    let rest = rest.trim_start();
    // Extract class name (up to space, `<`, or end)
    let name_end = rest
        .find(|c: char| c == '<' || c == ' ' || c == '\n')
        .unwrap_or(rest.len());
    let name = rest[..name_end].trim().to_owned();
    if name.is_empty() {
        return None;
    }

    // Extract parent: after `<`
    let parent = if let Some(pos) = rest.find('<') {
        let after = rest[pos + 1..].trim();
        let end = after
            .find(|c: char| c == ' ' || c == '\n')
            .unwrap_or(after.len());
        after[..end].trim().to_owned()
    } else {
        String::new()
    };

    Some(RubyStmt::ClassDef { name, parent })
}

/// Try to parse `def method_name` (skipping `def self.xxx` to avoid noise).
fn try_parse_def(line: &str) -> Option<RubyStmt> {
    let rest = line.strip_prefix("def ")?;
    let rest = rest.trim_start();
    // Skip class-level method definitions (e.g. `def self.create`)
    if rest.starts_with("self.") {
        return None;
    }
    let end = rest
        .find(|c: char| c == '(' || c == ' ' || c == '\n' || c == ';')
        .unwrap_or(rest.len());
    let name = rest[..end].trim().to_owned();
    if name.is_empty() || name.starts_with('_') {
        return None;
    }
    Some(RubyStmt::MethodDef { name })
}

/// Try to parse `require "name"` or `require 'name'`.
fn try_parse_require(line: &str) -> Option<RubyStmt> {
    let rest = line
        .strip_prefix("require ")?
        .trim_start();
    let name = extract_quoted(rest)?;
    Some(RubyStmt::Require { name })
}

/// Try to parse `namespace :api do` or `scope "/api" do`.
fn try_parse_scope(line: &str) -> Option<RubyStmt> {
    for kind in ["namespace", "scope"] {
        let prefix = format!("{kind} ");
        if let Some(rest) = line.strip_prefix(prefix.as_str()) {
            let rest = rest.trim_start();
            let label = extract_quoted(rest)
                .or_else(|| {
                    if let Some(sym) = rest.strip_prefix(':') {
                        let end = sym
                            .find(|c: char| !c.is_alphanumeric() && c != '_')
                            .unwrap_or(sym.len());
                        if !sym[..end].is_empty() {
                            return Some(sym[..end].to_owned());
                        }
                    }
                    None
                })?;
            return Some(RubyStmt::ScopeBegin { kind: kind.to_owned(), label });
        }
    }
    None
}

// ---------------------------------------------------------------------------
// LanguageParser impl
// ---------------------------------------------------------------------------

const RUBY_EXTS: &[&str] = &["rb", "rake", "gemspec"];

impl LanguageParser for RubyParser {
    fn parse(&self, path: &Path, source: &str) -> Result<ParsedFile> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !RUBY_EXTS.contains(&ext) {
            return Err(crate::Error::NotImplemented(
                "RubyParser: unsupported extension (only .rb, .rake, .gemspec accepted)",
            ));
        }
        Ok(ParsedFile {
            path: path.to_owned(),
            lang: Language::Ruby,
            source: source.to_owned(),
            diagnostics: Vec::new(),
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn parse(name: &str, src: &str) -> crate::Result<ParsedFile> {
        RubyParser.parse(&PathBuf::from(name), src)
    }

    #[test]
    fn parse_rb_file_returns_language_ruby() {
        let f = parse("app.rb", "").unwrap();
        assert_eq!(f.lang, Language::Ruby);
    }

    #[test]
    fn parse_non_rb_extension_errors() {
        assert!(parse("main.go", "").is_err());
    }

    #[test]
    fn parse_rb_source_roundtrips() {
        let src = "class Foo\nend\n";
        let f = parse("foo.rb", src).unwrap();
        assert_eq!(f.source, src);
    }

    #[test]
    fn scan_route_get_with_to() {
        let stmts = RubyParser::scan(r#"get "/users", to: "users#index""#);
        assert_eq!(
            stmts,
            vec![RubyStmt::Route {
                method: "get".into(),
                path: "/users".into(),
                controller_action: "users#index".into(),
            }]
        );
    }

    #[test]
    fn scan_route_get_arrow_syntax() {
        let stmts = RubyParser::scan(r#"get "/users" => "users#index""#);
        assert_eq!(
            stmts,
            vec![RubyStmt::Route {
                method: "get".into(),
                path: "/users".into(),
                controller_action: "users#index".into(),
            }]
        );
    }

    #[test]
    fn scan_route_post() {
        let stmts = RubyParser::scan(r#"post "/users", to: "users#create""#);
        assert!(matches!(&stmts[0], RubyStmt::Route { method, .. } if method == "post"));
    }

    #[test]
    fn scan_route_no_action() {
        let stmts = RubyParser::scan(r#"get "/health""#);
        assert_eq!(
            stmts,
            vec![RubyStmt::Route {
                method: "get".into(),
                path: "/health".into(),
                controller_action: "".into(),
            }]
        );
    }

    #[test]
    fn scan_resources() {
        let stmts = RubyParser::scan("resources :users");
        assert_eq!(stmts, vec![RubyStmt::Resources { name: "users".into() }]);
    }

    #[test]
    fn scan_resource_singular() {
        let stmts = RubyParser::scan("resource :profile");
        assert_eq!(stmts, vec![RubyStmt::Resource { name: "profile".into() }]);
    }

    #[test]
    fn scan_devise_for() {
        let stmts = RubyParser::scan("devise_for :users");
        assert_eq!(stmts, vec![RubyStmt::DeviseFor { resource: "users".into() }]);
    }

    #[test]
    fn scan_class_def() {
        let stmts = RubyParser::scan("class UsersController < ApplicationController");
        assert_eq!(
            stmts,
            vec![RubyStmt::ClassDef {
                name: "UsersController".into(),
                parent: "ApplicationController".into(),
            }]
        );
    }

    #[test]
    fn scan_class_def_no_parent() {
        let stmts = RubyParser::scan("class MyClass");
        assert_eq!(
            stmts,
            vec![RubyStmt::ClassDef { name: "MyClass".into(), parent: "".into() }]
        );
    }

    #[test]
    fn scan_method_def() {
        let stmts = RubyParser::scan("  def index");
        assert_eq!(stmts, vec![RubyStmt::MethodDef { name: "index".into() }]);
    }

    #[test]
    fn scan_require() {
        let stmts = RubyParser::scan("require 'devise'");
        assert_eq!(stmts, vec![RubyStmt::Require { name: "devise".into() }]);
    }

    #[test]
    fn scan_require_double_quotes() {
        let stmts = RubyParser::scan(r#"require "rails/engine""#);
        assert_eq!(stmts, vec![RubyStmt::Require { name: "rails/engine".into() }]);
    }

    #[test]
    fn scan_comment_skipped() {
        let stmts = RubyParser::scan("# get \"/secret\"");
        assert!(stmts.is_empty());
    }

    #[test]
    fn scan_multiple_routes() {
        let src = "get \"/users\", to: \"users#index\"\npost \"/users\", to: \"users#create\"\ndelete \"/users/:id\", to: \"users#destroy\"\n";
        let stmts = RubyParser::scan(src);
        assert_eq!(stmts.len(), 3);
    }

    #[test]
    fn scan_namespace_begin() {
        let stmts = RubyParser::scan("namespace :api do");
        assert_eq!(
            stmts,
            vec![RubyStmt::ScopeBegin { kind: "namespace".into(), label: "api".into() }]
        );
    }

    #[test]
    fn ruby_parser_is_language_parser() {
        fn _accepts(_: &dyn LanguageParser) {}
        _accepts(&RubyParser);
    }

    #[test]
    fn scan_indented_method() {
        let stmts = RubyParser::scan("    def show");
        assert_eq!(stmts, vec![RubyStmt::MethodDef { name: "show".into() }]);
    }

    #[test]
    fn scan_empty_source_returns_empty() {
        assert!(RubyParser::scan("").is_empty());
    }

    #[test]
    fn scan_scope_string_label() {
        let stmts = RubyParser::scan("scope \"/api\" do");
        assert_eq!(
            stmts,
            vec![RubyStmt::ScopeBegin { kind: "scope".into(), label: "/api".into() }]
        );
    }

    #[test]
    fn scan_delete_route() {
        let stmts = RubyParser::scan("delete \"/users/:id\", to: \"users#destroy\"");
        assert!(matches!(&stmts[0], RubyStmt::Route { method, .. } if method == "delete"));
    }

    #[test]
    fn parse_rake_extension_accepted() {
        let f = RubyParser.parse(&PathBuf::from("Rakefile.rake"), "").unwrap();
        assert_eq!(f.lang, Language::Ruby);
    }
}
