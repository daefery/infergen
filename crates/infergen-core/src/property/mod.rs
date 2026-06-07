//! Property type inference and PII detection (E1.4).
//!
//! Provides heuristic functions to infer JS/TS types from property names,
//! flag likely PII, and extract property hints from OXC AST nodes (crate-internal).

use crate::{adapter::PropertyHint, namer::split_identifier};

// ─── PII detection ────────────────────────────────────────────────────────────

/// Tokens that, when found in a split-identifier property name, indicate PII.
const PII_TOKENS: &[&str] = &[
    "email",
    "mail",
    "phone",
    "mobile",
    "tel",
    "address",
    "addr",
    "street",
    "city",
    "zip",
    "postal",
    "postcode",
    "state",
    "country",
    "name",
    "firstname",
    "lastname",
    "surname",
    "givenname",
    "familyname",
    "dob",
    "birthdate",
    "birthday",
    "birth",
    "ssn",
    "social",
    "national",
    "passport",
    "license",
    "credit",
    "card",
    "cvv",
    "cvc",
    "token",
    "secret",
    "password",
    "passwd",
    "pin",
    "ip",
];

/// Returns `true` when `name` contains a PII token after splitting on
/// camelCase/PascalCase/snake_case/kebab-case boundaries.
///
/// Examples:
/// - `"email"` → `true`
/// - `"userEmail"` → `true` (split → `["user", "email"]`)
/// - `"method"` → `false`
pub fn is_pii_property(name: &str) -> bool {
    split_identifier(name)
        .iter()
        .any(|t| PII_TOKENS.contains(&t.as_str()))
}

// ─── Type inference from name ─────────────────────────────────────────────────

/// Heuristic JS/TS type inference from a property name.
///
/// Returns `None` when no confident pattern applies. Callers may display the
/// type as "unknown" or leave it unspecified. Conservative on purpose: only
/// boolean and number patterns are inferred; "string" is not inferred because
/// it is the default assumption and would add noise.
///
/// # Patterns
///
/// **Boolean** — first token is a boolean-prefix word: `is`, `has`, `can`,
/// `should`, `was`, `did`, `will`. Or last token is a boolean-suffix word:
/// `enabled`, `disabled`, `active`, `checked`, `visible`, `hidden`, `flag`.
///
/// **Number** — last token is an unambiguously numeric concept: `count`,
/// `total`, `amount`, `price`, `quantity`, `age`, `size`, `limit`, `offset`,
/// `page`, `index`, `rank`, `score`, `weight`, `height`, `width`, `length`,
/// `duration`. Also exact names: `timestamp`, `created_at`, `updated_at`,
/// `deleted_at`.
pub fn type_from_name(name: &str) -> Option<&'static str> {
    let tokens = split_identifier(name);
    let first = tokens.first().map(String::as_str).unwrap_or("");
    let last = tokens.last().map(String::as_str).unwrap_or("");

    if matches!(first, "is" | "has" | "can" | "should" | "was" | "did" | "will") {
        return Some("boolean");
    }
    if matches!(
        last,
        "enabled" | "disabled" | "active" | "checked" | "visible" | "hidden" | "flag"
    ) {
        return Some("boolean");
    }

    if matches!(
        last,
        "count" | "total" | "amount" | "price" | "quantity" | "age"
            | "size" | "limit" | "offset" | "page" | "index" | "rank"
            | "score" | "weight" | "height" | "width" | "length" | "duration"
    ) {
        return Some("number");
    }
    if matches!(
        name,
        "timestamp" | "created_at" | "updated_at" | "deleted_at"
    ) {
        return Some("number");
    }

    None
}

// ─── Hint enrichment ──────────────────────────────────────────────────────────

/// Enrich `hints` by filling missing `type_hint` values via [`type_from_name`]
/// and setting `pii_hint = true` where [`is_pii_property`] returns `true`.
///
/// Existing `type_hint` values are preserved. `pii_hint` can only go from
/// `false` → `true`, never the reverse. Safe to call multiple times (idempotent).
pub fn enrich_hints(mut hints: Vec<PropertyHint>) -> Vec<PropertyHint> {
    for hint in &mut hints {
        if hint.type_hint.is_none() && let Some(t) = type_from_name(&hint.name) {
            hint.type_hint = Some(t.to_owned());
        }
        if !hint.pii_hint {
            hint.pii_hint = is_pii_property(&hint.name);
        }
    }
    hints
}

// ─── AST-based extraction ─────────────────────────────────────────────────────

/// Extract [`PropertyHint`]s from OXC [`FormalParameters`].
///
/// Produces one hint per simple identifier param. Destructured params
/// (`{x, y}`, `[a, b]`) are skipped. TS primitive type annotations
/// (`string`, `number`, `boolean`) are extracted when present; all other
/// types yield `type_hint = None`. `pii_hint` defaults to `false`;
/// callers should apply [`enrich_hints`] afterwards.
///
/// In OXC 0.134 the type annotation lives on `FormalParameter.type_annotation`,
/// NOT on the nested `BindingPattern`.
pub(crate) fn hints_from_params<'a>(
    params: &oxc_ast::ast::FormalParameters<'a>,
) -> Vec<PropertyHint> {
    use oxc_ast::ast::{BindingPattern, TSType};

    params
        .items
        .iter()
        .filter_map(|p| {
            let BindingPattern::BindingIdentifier(id) = &p.pattern else {
                return None;
            };
            let name = id.name.to_string();
            let type_hint = p.type_annotation.as_ref().and_then(|ta| {
                match &ta.type_annotation {
                    TSType::TSStringKeyword(_) => Some("string".to_owned()),
                    TSType::TSNumberKeyword(_) => Some("number".to_owned()),
                    TSType::TSBooleanKeyword(_) => Some("boolean".to_owned()),
                    _ => None,
                }
            });
            Some(PropertyHint {
                name,
                type_hint,
                pii_hint: false,
            })
        })
        .collect()
}

/// Scan `prog` for JSX `<input>`, `<select>`, and `<textarea>` elements and
/// extract their `name` attribute as a [`PropertyHint`].
///
/// Walks the top-level statement list, recurses into function bodies, return
/// statements, arrow function block bodies, and JSX trees. `name` attribute
/// values must be string literals; dynamic expressions (`name={var}`) are
/// skipped. Results are deduplicated by name (first occurrence wins).
///
/// `type_hint` and `pii_hint` are unset on returned hints; apply
/// [`enrich_hints`] afterwards.
pub(crate) fn hints_from_jsx_inputs<'a>(
    prog: &oxc_ast::ast::Program<'a>,
) -> Vec<PropertyHint> {
    let mut hints = Vec::new();
    for stmt in &prog.body {
        collect_from_stmt(stmt, &mut hints);
    }
    // Deduplicate by name, preserving first occurrence.
    let mut seen = std::collections::HashSet::new();
    hints.retain(|h| seen.insert(h.name.clone()));
    hints
}

// ── Internal JSX walkers ──────────────────────────────────────────────────────

fn collect_from_stmt<'a>(
    stmt: &oxc_ast::ast::Statement<'a>,
    hints: &mut Vec<PropertyHint>,
) {
    use oxc_ast::ast::{Declaration, ExportDefaultDeclarationKind, Statement};

    match stmt {
        Statement::FunctionDeclaration(func) => {
            if let Some(body) = &func.body {
                for s in &body.statements {
                    collect_from_stmt(s, hints);
                }
            }
        }
        Statement::ReturnStatement(ret) => {
            if let Some(expr) = &ret.argument {
                collect_from_expr(expr, hints);
            }
        }
        Statement::ExpressionStatement(expr_stmt) => {
            collect_from_expr(&expr_stmt.expression, hints);
        }
        Statement::VariableDeclaration(decl) => {
            for declarator in &decl.declarations {
                if let Some(init) = &declarator.init {
                    collect_from_expr(init, hints);
                }
            }
        }
        Statement::ExportDefaultDeclaration(export) => {
            match &export.declaration {
                ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                    if let Some(body) = &func.body {
                        for s in &body.statements {
                            collect_from_stmt(s, hints);
                        }
                    }
                }
                ExportDefaultDeclarationKind::ArrowFunctionExpression(arrow) => {
                    for s in &arrow.body.statements {
                        collect_from_stmt(s, hints);
                    }
                }
                ExportDefaultDeclarationKind::JSXElement(elem) => {
                    collect_from_jsx(elem, hints);
                }
                ExportDefaultDeclarationKind::JSXFragment(frag) => {
                    for child in &frag.children {
                        collect_from_jsx_child(child, hints);
                    }
                }
                ExportDefaultDeclarationKind::ParenthesizedExpression(paren) => {
                    collect_from_expr(&paren.expression, hints);
                }
                _ => {}
            }
        }
        Statement::ExportNamedDeclaration(export) => {
            if let Some(decl) = &export.declaration {
                match decl {
                    Declaration::FunctionDeclaration(func) => {
                        if let Some(body) = &func.body {
                            for s in &body.statements {
                                collect_from_stmt(s, hints);
                            }
                        }
                    }
                    Declaration::VariableDeclaration(var) => {
                        for declarator in &var.declarations {
                            if let Some(init) = &declarator.init {
                                collect_from_expr(init, hints);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn collect_from_expr<'a>(
    expr: &oxc_ast::ast::Expression<'a>,
    hints: &mut Vec<PropertyHint>,
) {
    use oxc_ast::ast::Expression;

    match expr {
        Expression::JSXElement(elem) => {
            collect_from_jsx(elem, hints);
        }
        Expression::JSXFragment(frag) => {
            for child in &frag.children {
                collect_from_jsx_child(child, hints);
            }
        }
        Expression::ParenthesizedExpression(paren) => {
            collect_from_expr(&paren.expression, hints);
        }
        Expression::ArrowFunctionExpression(arrow) => {
            // Block-body arrows only; expression-body arrows (expression=true)
            // wrap their expression in statements[0] as a return — still handled.
            for s in &arrow.body.statements {
                collect_from_stmt(s, hints);
            }
        }
        _ => {}
    }
}

fn collect_from_jsx<'a>(
    elem: &oxc_ast::ast::JSXElement<'a>,
    hints: &mut Vec<PropertyHint>,
) {
    use oxc_ast::ast::JSXElementName;

    let tag = match &elem.opening_element.name {
        JSXElementName::Identifier(id) => id.name.as_str(),
        _ => "",
    };

    if matches!(tag, "input" | "select" | "textarea")
        && let Some(name_val) = extract_name_attr(&elem.opening_element)
    {
        hints.push(PropertyHint {
            name: name_val,
            type_hint: None,
            pii_hint: false,
        });
    }

    for child in &elem.children {
        collect_from_jsx_child(child, hints);
    }
}

fn collect_from_jsx_child<'a>(
    child: &oxc_ast::ast::JSXChild<'a>,
    hints: &mut Vec<PropertyHint>,
) {
    use oxc_ast::ast::JSXChild;

    match child {
        JSXChild::Element(elem) => collect_from_jsx(elem, hints),
        JSXChild::Fragment(frag) => {
            for c in &frag.children {
                collect_from_jsx_child(c, hints);
            }
        }
        _ => {}
    }
}

fn extract_name_attr<'a>(elem: &oxc_ast::ast::JSXOpeningElement<'a>) -> Option<String> {
    use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue};

    for attr_item in &elem.attributes {
        let JSXAttributeItem::Attribute(attr) = attr_item else {
            continue;
        };
        let attr_name = match &attr.name {
            JSXAttributeName::Identifier(id) => id.name.as_str(),
            _ => continue,
        };
        if attr_name != "name" {
            continue;
        }
        if let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value {
            return Some(lit.value.to_string());
        }
    }
    None
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{JsParser, adapter::PropertyHint, parser::LanguageParser};
    use std::path::PathBuf;

    fn parse_prog_and_run<F: Fn(&oxc_ast::ast::Program<'_>) -> Vec<PropertyHint>>(
        src: &str,
        f: F,
    ) -> Vec<PropertyHint> {
        let path = PathBuf::from("test.tsx");
        let parsed = JsParser.parse(&path, src).unwrap();
        parsed.with_js_program(f).unwrap_or_default()
    }

    // ── is_pii_property ───────────────────────────────────────────────────────

    #[test]
    fn is_pii_snake_email() {
        assert!(is_pii_property("email"));
    }

    #[test]
    fn is_pii_snake_phone() {
        assert!(is_pii_property("phone_number"));
    }

    #[test]
    fn is_pii_camel_user_email() {
        assert!(is_pii_property("userEmail"));
    }

    #[test]
    fn is_pii_pascal_email_address() {
        assert!(is_pii_property("EmailAddress"));
    }

    #[test]
    fn is_pii_name_token() {
        assert!(is_pii_property("full_name"));
    }

    #[test]
    fn is_pii_password() {
        assert!(is_pii_property("password"));
    }

    #[test]
    fn is_pii_negative_method() {
        assert!(!is_pii_property("method"));
    }

    #[test]
    fn is_pii_negative_endpoint() {
        assert!(!is_pii_property("endpoint"));
    }

    #[test]
    fn is_pii_negative_count() {
        assert!(!is_pii_property("count"));
    }

    // ── type_from_name ────────────────────────────────────────────────────────

    #[test]
    fn type_from_name_is_active_boolean() {
        assert_eq!(type_from_name("is_active"), Some("boolean"));
    }

    #[test]
    fn type_from_name_has_permission_boolean() {
        assert_eq!(type_from_name("has_permission"), Some("boolean"));
    }

    #[test]
    fn type_from_name_enabled_boolean() {
        assert_eq!(type_from_name("feature_enabled"), Some("boolean"));
    }

    #[test]
    fn type_from_name_count_number() {
        assert_eq!(type_from_name("count"), Some("number"));
    }

    #[test]
    fn type_from_name_total_amount_number() {
        assert_eq!(type_from_name("total_amount"), Some("number"));
    }

    #[test]
    fn type_from_name_page_index_number() {
        assert_eq!(type_from_name("page_index"), Some("number"));
    }

    #[test]
    fn type_from_name_timestamp_number() {
        assert_eq!(type_from_name("timestamp"), Some("number"));
    }

    #[test]
    fn type_from_name_email_unknown() {
        assert_eq!(type_from_name("email"), None);
    }

    #[test]
    fn type_from_name_method_unknown() {
        assert_eq!(type_from_name("method"), None);
    }

    #[test]
    fn type_from_name_empty_unknown() {
        assert_eq!(type_from_name(""), None);
    }

    // ── enrich_hints ──────────────────────────────────────────────────────────

    #[test]
    fn enrich_hints_adds_pii_flag() {
        let hints = vec![PropertyHint {
            name: "email".into(),
            type_hint: None,
            pii_hint: false,
        }];
        let result = enrich_hints(hints);
        assert!(result[0].pii_hint);
    }

    #[test]
    fn enrich_hints_fills_missing_type() {
        let hints = vec![PropertyHint {
            name: "count".into(),
            type_hint: None,
            pii_hint: false,
        }];
        let result = enrich_hints(hints);
        assert_eq!(result[0].type_hint.as_deref(), Some("number"));
    }

    #[test]
    fn enrich_hints_preserves_existing_type() {
        let hints = vec![PropertyHint {
            name: "email".into(),
            type_hint: Some("string".into()),
            pii_hint: false,
        }];
        let result = enrich_hints(hints);
        assert_eq!(result[0].type_hint.as_deref(), Some("string"));
        assert!(result[0].pii_hint);
    }

    #[test]
    fn enrich_hints_idempotent() {
        let hints = vec![
            PropertyHint { name: "email".into(), type_hint: None, pii_hint: false },
            PropertyHint { name: "count".into(), type_hint: None, pii_hint: false },
        ];
        let once = enrich_hints(hints.clone());
        let twice = enrich_hints(once.clone());
        assert_eq!(once, twice);
    }

    #[test]
    fn enrich_hints_multiple_hints() {
        let hints = vec![
            PropertyHint { name: "email".into(),  type_hint: None, pii_hint: false },
            PropertyHint { name: "method".into(), type_hint: None, pii_hint: false },
            PropertyHint { name: "count".into(),  type_hint: None, pii_hint: false },
        ];
        let result = enrich_hints(hints);
        assert!(result[0].pii_hint);
        assert!(!result[1].pii_hint);
        assert_eq!(result[2].type_hint.as_deref(), Some("number"));
    }

    // ── hints_from_params ─────────────────────────────────────────────────────

    #[test]
    fn hints_from_params_ts_typed() {
        let src = "function handleSubmit(email: string, count: number) {}";
        let hints = parse_prog_and_run(src, |prog| {
            use oxc_ast::ast::Statement;
            for stmt in &prog.body {
                if let Statement::FunctionDeclaration(func) = stmt {
                    return hints_from_params(&func.params);
                }
            }
            vec![]
        });
        assert_eq!(hints.len(), 2);
        assert_eq!(hints[0].name, "email");
        assert_eq!(hints[0].type_hint.as_deref(), Some("string"));
        assert_eq!(hints[1].name, "count");
        assert_eq!(hints[1].type_hint.as_deref(), Some("number"));
    }

    #[test]
    fn hints_from_params_boolean_type() {
        let src = "function f(isActive: boolean) {}";
        let hints = parse_prog_and_run(src, |prog| {
            use oxc_ast::ast::Statement;
            for stmt in &prog.body {
                if let Statement::FunctionDeclaration(func) = stmt {
                    return hints_from_params(&func.params);
                }
            }
            vec![]
        });
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].type_hint.as_deref(), Some("boolean"));
    }

    #[test]
    fn hints_from_params_untyped() {
        let src = "function f(email, data) {}";
        let hints = parse_prog_and_run(src, |prog| {
            use oxc_ast::ast::Statement;
            for stmt in &prog.body {
                if let Statement::FunctionDeclaration(func) = stmt {
                    return hints_from_params(&func.params);
                }
            }
            vec![]
        });
        assert_eq!(hints.len(), 2);
        assert!(hints[0].type_hint.is_none());
        assert!(hints[1].type_hint.is_none());
    }

    #[test]
    fn hints_from_params_skips_destructured() {
        let src = "function f({x, y}, name: string) {}";
        let hints = parse_prog_and_run(src, |prog| {
            use oxc_ast::ast::Statement;
            for stmt in &prog.body {
                if let Statement::FunctionDeclaration(func) = stmt {
                    return hints_from_params(&func.params);
                }
            }
            vec![]
        });
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].name, "name");
    }

    #[test]
    fn hints_from_params_empty() {
        let src = "function f() {}";
        let hints = parse_prog_and_run(src, |prog| {
            use oxc_ast::ast::Statement;
            for stmt in &prog.body {
                if let Statement::FunctionDeclaration(func) = stmt {
                    return hints_from_params(&func.params);
                }
            }
            vec![]
        });
        assert!(hints.is_empty());
    }

    // ── hints_from_jsx_inputs ─────────────────────────────────────────────────

    #[test]
    fn hints_from_jsx_inputs_single_input() {
        let src = r#"
            function F() {
                return (<form><input name="email" /></form>);
            }
        "#;
        let hints = parse_prog_and_run(src, hints_from_jsx_inputs);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].name, "email");
    }

    #[test]
    fn hints_from_jsx_inputs_multiple_inputs() {
        let src = r#"
            function F() {
                return (
                    <form>
                        <input name="email" />
                        <input name="age" />
                    </form>
                );
            }
        "#;
        let hints = parse_prog_and_run(src, hints_from_jsx_inputs);
        assert_eq!(hints.len(), 2);
        let names: Vec<&str> = hints.iter().map(|h| h.name.as_str()).collect();
        assert!(names.contains(&"email"));
        assert!(names.contains(&"age"));
    }

    #[test]
    fn hints_from_jsx_inputs_nested() {
        let src = r#"
            function F() {
                return (
                    <div>
                        <form>
                            <input name="foo" />
                        </form>
                    </div>
                );
            }
        "#;
        let hints = parse_prog_and_run(src, hints_from_jsx_inputs);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].name, "foo");
    }

    #[test]
    fn hints_from_jsx_inputs_select() {
        let src = r#"
            function F() {
                return (<form><select name="country" /></form>);
            }
        "#;
        let hints = parse_prog_and_run(src, hints_from_jsx_inputs);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].name, "country");
    }

    #[test]
    fn hints_from_jsx_inputs_textarea() {
        let src = r#"
            function F() {
                return (<form><textarea name="bio" /></form>);
            }
        "#;
        let hints = parse_prog_and_run(src, hints_from_jsx_inputs);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].name, "bio");
    }

    #[test]
    fn hints_from_jsx_inputs_skips_dynamic_name() {
        let src = r#"
            function F() {
                return (<form><input name={someVar} /></form>);
            }
        "#;
        let hints = parse_prog_and_run(src, hints_from_jsx_inputs);
        assert!(hints.is_empty());
    }

    #[test]
    fn hints_from_jsx_inputs_deduplicates() {
        let src = r#"
            function F() {
                return (
                    <form>
                        <input name="email" />
                        <input name="email" />
                    </form>
                );
            }
        "#;
        let hints = parse_prog_and_run(src, hints_from_jsx_inputs);
        assert_eq!(hints.len(), 1);
    }

    #[test]
    fn hints_from_jsx_inputs_no_jsx() {
        let src = "const x = 1 + 2;";
        let hints = parse_prog_and_run(src, hints_from_jsx_inputs);
        assert!(hints.is_empty());
    }

    #[test]
    fn hints_from_jsx_inputs_in_function_body() {
        let src = r#"
            function LoginForm() {
                return (
                    <form>
                        <input name="username" />
                        <input name="password" />
                    </form>
                );
            }
        "#;
        let hints = parse_prog_and_run(src, hints_from_jsx_inputs);
        let names: Vec<&str> = hints.iter().map(|h| h.name.as_str()).collect();
        assert!(names.contains(&"username"));
        assert!(names.contains(&"password"));
    }

    #[test]
    fn hints_from_jsx_inputs_default_export() {
        let src = r#"
            export default function LoginForm() {
                return (<form><input name="email" /></form>);
            }
        "#;
        let hints = parse_prog_and_run(src, hints_from_jsx_inputs);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].name, "email");
    }
}
