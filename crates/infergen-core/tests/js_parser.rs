//! Integration tests for E0.3: TS/JS AST parser public API.

use std::path::Path;

use infergen_core::{
    JsParser,
    detect::Language,
    parser::{LanguageParser, ParsedFile},
};

const VALID_TS: &str = r#"
interface Config { debug: boolean; }
function build(cfg: Config): void { console.log(cfg.debug); }
export default build;
"#;

const BROKEN_JS: &str = "const obj = { key: ";

// ---------------------------------------------------------------------------
// JsParser::parse (via LanguageParser trait)
// ---------------------------------------------------------------------------

#[test]
fn parse_ts_file_succeeds() {
    let parser = JsParser;
    let file = parser.parse(Path::new("config.ts"), VALID_TS).unwrap();
    assert_eq!(file.lang, Language::TypeScript);
    assert!(!file.has_errors());
    assert!(!file.source.is_empty());
}

#[test]
fn parse_broken_source_is_ok_not_err() {
    let parser = JsParser;
    // Must never panic or return Err for syntax problems.
    let result = parser.parse(Path::new("bad.js"), BROKEN_JS);
    assert!(result.is_ok());
    let file = result.unwrap();
    assert!(file.has_errors());
}

// ---------------------------------------------------------------------------
// JsParser::with_program (static, direct AST access)
// ---------------------------------------------------------------------------

#[test]
fn with_program_sees_export_default() {
    let has_default_export = JsParser::with_program(Path::new("config.ts"), VALID_TS, |prog| {
        prog.body
            .iter()
            .any(|stmt| matches!(stmt, oxc_ast::ast::Statement::ExportDefaultDeclaration(_)))
    });
    assert!(has_default_export);
}

#[test]
fn with_program_returns_owned_data() {
    // Vec<String> is fully owned — no lifetime issues.
    let stmts: Vec<String> = JsParser::with_program(
        Path::new("config.ts"),
        "import React from 'react'; export const x = 1;",
        |prog| {
            prog.body
                .iter()
                .map(|s| format!("{:?}", std::mem::discriminant(s)))
                .collect()
        },
    );
    assert_eq!(stmts.len(), 2);
}

// ---------------------------------------------------------------------------
// ParsedFile::with_js_program (on ParsedFile instances)
// ---------------------------------------------------------------------------

#[test]
fn parsed_file_with_js_program_some_for_ts() {
    let parser = JsParser;
    let file = parser.parse(Path::new("config.ts"), VALID_TS).unwrap();
    let count = file.with_js_program(|prog| prog.body.len());
    assert!(count.is_some());
    assert!(count.unwrap() > 0);
}

#[test]
fn parsed_file_with_js_program_none_for_python() {
    let file = ParsedFile {
        path: Path::new("script.py").to_path_buf(),
        lang: Language::Python,
        source: "print('hi')".to_owned(),
        diagnostics: vec![],
    };
    assert!(file.with_js_program(|_| ()).is_none());
}

// ---------------------------------------------------------------------------
// LanguageParser object safety
// ---------------------------------------------------------------------------

#[test]
fn language_parser_is_dyn_compatible() {
    let parser: Box<dyn LanguageParser> = Box::new(JsParser);
    let file = parser.parse(Path::new("x.ts"), "const x = 1;").unwrap();
    assert_eq!(file.lang, Language::TypeScript);
}
