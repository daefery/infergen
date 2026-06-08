//! Flask framework adapter.
//!
//! Detection strategy:
//!
//! 1. **Path-based** — always runs.  Files in `routes/`, `views/`,
//!    `blueprints/` get a confidence boost.
//! 2. **AST-based** — runs via [`ParsedFile::with_py_ast`].  Detects
//!    `@app.route()` / `@blueprint.route()` decorators, Flask-Login imports,
//!    error handlers, and WTForms field hints.
//!
//! All proposals carry `adapter = "flask"`.

use std::path::{Path, PathBuf};

use crate::{
    detect::Framework,
    namer::{NameSignals, Namer},
    parser::{ParsedFile, py::PyStmt},
    property,
};

use super::{Adapter, EventKind, PropertyHint, ProposedEvent};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// WTForms field classes.
const STRING_FIELDS: &[&str] = &[
    "StringField", "TextAreaField", "URLField", "TelField", "SearchField",
    "HiddenField", "FileField",
];
const NUMBER_FIELDS: &[&str] = &["IntegerField", "FloatField", "DecimalField"];
const BOOL_FIELDS: &[&str] = &["BooleanField", "RadioField"];
const EMAIL_FIELDS: &[&str] = &["EmailField"];
const PASSWORD_FIELDS: &[&str] = &["PasswordField"];

/// Form base class names (WTForms / Flask-WTF).
const FORM_BASES: &[&str] = &["FlaskForm", "Form", "BaseForm"];

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Flask framework adapter.
pub struct FlaskAdapter {
    /// Absolute path to the Python project root.
    pub project_root: PathBuf,
}

impl FlaskAdapter {
    /// Create a new adapter rooted at `project_root`.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self { project_root: project_root.into() }
    }
}

impl Adapter for FlaskAdapter {
    fn analyze(&self, file: &ParsedFile) -> Vec<ProposedEvent> {
        if file.lang != crate::detect::Language::Python {
            return Vec::new();
        }

        let mut events: Vec<ProposedEvent> = Vec::new();
        let namer = Namer::new();

        let rel = file.path.strip_prefix(&self.project_root).unwrap_or(&file.path);
        let in_route_dir = in_route_dir(rel);
        let source_path = file.path.clone();

        let ast_events: Vec<ProposedEvent> = file.with_py_ast(|stmts| {
            let mut result = Vec::new();
            detect_routes(stmts, &source_path, &namer, in_route_dir, &mut result);
            detect_auth(stmts, &source_path, &namer, &mut result);
            detect_form_classes(stmts, &source_path, &namer, &mut result);
            detect_error_handlers(stmts, &source_path, &namer, &mut result);
            result
        });
        events.extend(ast_events);

        for event in &mut events {
            let props = std::mem::take(&mut event.properties);
            event.properties = property::enrich_hints(props);
            event.adapter = "flask".to_owned();
        }

        events
    }

    fn framework(&self) -> Framework {
        Framework::Flask
    }
}

// ---------------------------------------------------------------------------
// Route detection
// ---------------------------------------------------------------------------

fn in_route_dir(rel: &Path) -> bool {
    rel.components().any(|c| {
        matches!(
            c.as_os_str().to_str().unwrap_or(""),
            "routes" | "route" | "views" | "view" | "blueprints" | "blueprint" | "api"
        )
    })
}

/// Parsed Flask route descriptor.
struct RouteDecorator {
    path: String,
    methods: Vec<String>,
}

/// Try to parse `some_obj.route("/path", methods=["GET", "POST"])` from decorator text.
fn parse_flask_route(text: &str) -> Option<RouteDecorator> {
    let dot = text.find('.')?;
    let after_dot = &text[dot + 1..];
    if !after_dot.starts_with("route(") && !after_dot.starts_with("route (") {
        return None;
    }
    let paren_pos = after_dot.find('(')?;
    let args_text = &after_dot[paren_pos + 1..];

    let path = extract_first_string_arg(args_text)?;
    let methods = extract_methods_kwarg(args_text);

    Some(RouteDecorator { path, methods })
}

/// Extract the first string literal from Flask decorator arguments.
fn extract_first_string_arg(args: &str) -> Option<String> {
    for quote in ['"', '\''] {
        if let Some(start) = args.find(quote) {
            let after = &args[start + 1..];
            if let Some(end) = after.find(quote) {
                return Some(after[..end].to_owned());
            }
        }
    }
    None
}

/// Extract `methods=["GET", "POST"]` kwarg values.
fn extract_methods_kwarg(args: &str) -> Vec<String> {
    let needle = "methods=";
    let Some(pos) = args.find(needle) else { return Vec::new() };
    let after = &args[pos + needle.len()..];
    // Collect string literals within the following [...].
    let bracket_start = after.find('[');
    let bracket_end = after.find(']');
    let (start, end) = match (bracket_start, bracket_end) {
        (Some(s), Some(e)) if s < e => (s, e),
        _ => return Vec::new(),
    };
    let list_text = &after[start + 1..end];
    let mut methods = Vec::new();
    let mut cursor = list_text;
    while let Some(q_pos) = cursor.find(|c| c == '"' || c == '\'') {
        let quote = &cursor[q_pos..q_pos + 1];
        let quote_char = quote.chars().next().unwrap();
        let after_open = &cursor[q_pos + 1..];
        if let Some(close) = after_open.find(quote_char) {
            methods.push(after_open[..close].to_ascii_uppercase());
            cursor = &after_open[close + 1..];
        } else {
            break;
        }
    }
    methods
}

/// Convert Flask path template `"/users/<int:user_id>"` → `"/users/[user_id]"`.
fn normalise_flask_path(path: &str) -> String {
    let mut result = String::new();
    for segment in path.split('/') {
        if segment.is_empty() {
            result.push('/');
            continue;
        }
        let norm = if segment.starts_with('<') && segment.ends_with('>') {
            let inner = &segment[1..segment.len() - 1];
            let name = inner.split(':').last().unwrap_or(inner);
            format!("[{name}]")
        } else {
            segment.to_owned()
        };
        result.push('/');
        result.push_str(&norm);
    }
    if result.is_empty() {
        result.push('/');
    }
    // Normalise double slashes.
    while result.contains("//") {
        result = result.replace("//", "/");
    }
    result
}

fn detect_routes(
    stmts: &[PyStmt],
    source_path: &Path,
    namer: &Namer,
    in_route_dir: bool,
    out: &mut Vec<ProposedEvent>,
) {
    for stmt in stmts {
        let PyStmt::FunctionDef { decorators, .. } = stmt else { continue };

        for dec_text in decorators {
            let Some(route_dec) = parse_flask_route(dec_text) else { continue };
            let normalised = normalise_flask_path(&route_dec.path);

            // Expand one event per HTTP method, or default to GET (PageView).
            let effective_methods: Vec<String> = if route_dec.methods.is_empty() {
                vec!["GET".to_owned()]
            } else {
                route_dec.methods.clone()
            };

            for method in &effective_methods {
                let (kind, confidence) = method_to_kind(method, in_route_dir);
                let result = namer.derive(&NameSignals {
                    route: Some(&normalised),
                    handler_name: Some(&method.to_ascii_lowercase()),
                    kind,
                    component_name: None,
                });
                let event = match kind {
                    EventKind::ApiCall => {
                        ProposedEvent::new(result.name, kind, source_path, confidence)
                            .with_prop("endpoint", Some("string"))
                            .with_prop("method", Some("string"))
                    }
                    EventKind::PageView => {
                        ProposedEvent::new(result.name, kind, source_path, confidence)
                            .with_prop("route", Some("string"))
                    }
                    _ => ProposedEvent::new(result.name, kind, source_path, confidence),
                };
                out.push(event);
            }
        }
    }
}

fn method_to_kind(method: &str, in_route_dir: bool) -> (EventKind, f32) {
    let base: f32 = if in_route_dir { 0.9 } else { 0.85 };
    match method.to_ascii_uppercase().as_str() {
        "GET" => (EventKind::PageView, base),
        _ => (EventKind::ApiCall, base),
    }
}

// ---------------------------------------------------------------------------
// Auth detection
// ---------------------------------------------------------------------------

fn detect_auth(
    stmts: &[PyStmt],
    source_path: &Path,
    namer: &Namer,
    out: &mut Vec<ProposedEvent>,
) {
    let mut has_login_user = false;
    let mut has_logout_user = false;

    for stmt in stmts {
        match stmt {
            PyStmt::ImportFrom { names, .. } => {
                if names.iter().any(|n| n == "login_user") {
                    has_login_user = true;
                }
                if names.iter().any(|n| n == "logout_user") {
                    has_logout_user = true;
                }
            }
            _ => {}
        }
    }

    if has_login_user {
        let result = namer.derive(&NameSignals {
            handler_name: Some("login"),
            kind: EventKind::AuthEvent,
            route: None,
            component_name: None,
        });
        out.push(
            ProposedEvent::new(result.name, EventKind::AuthEvent, source_path, 0.85)
                .with_prop("method", Some("string")),
        );
    }
    if has_logout_user {
        let result = namer.derive(&NameSignals {
            handler_name: Some("logout"),
            kind: EventKind::AuthEvent,
            route: None,
            component_name: None,
        });
        out.push(ProposedEvent::new(result.name, EventKind::AuthEvent, source_path, 0.85));
    }
}

// ---------------------------------------------------------------------------
// Error handler detection
// ---------------------------------------------------------------------------

fn detect_error_handlers(
    stmts: &[PyStmt],
    source_path: &Path,
    namer: &Namer,
    out: &mut Vec<ProposedEvent>,
) {
    for stmt in stmts {
        let PyStmt::FunctionDef { decorators, .. } = stmt else { continue };
        for dec_text in decorators {
            if !dec_text.contains("errorhandler(") {
                continue;
            }
            let result = namer.derive(&NameSignals {
                handler_name: Some("error"),
                kind: EventKind::Error,
                route: None,
                component_name: None,
            });
            out.push(ProposedEvent::new(result.name, EventKind::Error, source_path, 0.7));
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// WTForms detection
// ---------------------------------------------------------------------------

fn detect_form_classes(
    stmts: &[PyStmt],
    source_path: &Path,
    namer: &Namer,
    out: &mut Vec<ProposedEvent>,
) {
    let mut current_form: Option<&str> = None;
    let mut current_hints: Vec<PropertyHint> = Vec::new();

    for stmt in stmts {
        match stmt {
            PyStmt::ClassDef { name, bases, .. } => {
                // Flush previous.
                if let Some(prev) = current_form.take() {
                    if !current_hints.is_empty() {
                        emit_form(prev, source_path, namer, std::mem::take(&mut current_hints), out);
                    }
                }
                if is_form_base(bases) {
                    current_form = Some(name.as_str());
                }
            }
            PyStmt::Assign { target, value } if current_form.is_some() => {
                if let Some(hint) = wtf_field_hint(target, value) {
                    current_hints.push(hint);
                }
            }
            _ => {}
        }
    }

    // Flush last.
    if let Some(name) = current_form {
        if !current_hints.is_empty() {
            emit_form(name, source_path, namer, current_hints, out);
        }
    }
}

fn is_form_base(bases: &[String]) -> bool {
    bases.iter().any(|b| FORM_BASES.contains(&b.as_str()))
}

fn emit_form(
    class_name: &str,
    source_path: &Path,
    namer: &Namer,
    hints: Vec<PropertyHint>,
    out: &mut Vec<ProposedEvent>,
) {
    let result = namer.derive(&NameSignals {
        component_name: Some(class_name),
        kind: EventKind::FormSubmit,
        route: None,
        handler_name: None,
    });
    let mut event = ProposedEvent::new(result.name, EventKind::FormSubmit, source_path, 0.75);
    event.properties = property::enrich_hints(hints);
    out.push(event);
}

fn wtf_field_hint(target: &str, value: &str) -> Option<PropertyHint> {
    if target.starts_with('_') || target == "Meta" {
        return None;
    }
    let field_class = extract_field_class(value)?;
    let type_hint = wtf_type_hint(&field_class);
    let pii_hint = matches!(field_class.as_str(), "EmailField" | "PasswordField")
        || property::is_pii_property(target);
    Some(PropertyHint {
        name: target.to_owned(),
        type_hint: type_hint.map(str::to_owned),
        pii_hint,
    })
}

fn extract_field_class(value: &str) -> Option<String> {
    let call_pos = value.find('(')?;
    let before_call = value[..call_pos].trim();
    let class = before_call.split('.').last()?.trim();
    if class.is_empty() { None } else { Some(class.to_owned()) }
}

fn wtf_type_hint(class: &str) -> Option<&'static str> {
    if NUMBER_FIELDS.contains(&class) {
        return Some("number");
    }
    if BOOL_FIELDS.contains(&class) {
        return Some("boolean");
    }
    if EMAIL_FIELDS.contains(&class) || PASSWORD_FIELDS.contains(&class) || STRING_FIELDS.contains(&class) {
        return Some("string");
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{parser::LanguageParser, parser::py::PyParser};

    fn adapter(root: &str) -> FlaskAdapter {
        FlaskAdapter::new(PathBuf::from(root))
    }

    fn parse(root: &str, rel_path: &str, source: &str) -> ParsedFile {
        let path = PathBuf::from(root).join(rel_path);
        PyParser.parse(&path, source).unwrap()
    }

    #[test]
    fn non_python_returns_empty() {
        let a = adapter("/proj");
        let file = crate::JsParser
            .parse(&PathBuf::from("/proj/app.ts"), "const x = 1;")
            .unwrap();
        assert!(a.analyze(&file).is_empty());
    }

    #[test]
    fn detects_app_route_get() {
        let a = adapter("/proj");
        let src = "@app.route(\"/users\")\ndef user_list():\n    return render_template(\"users.html\")\n";
        let file = parse("/proj", "views.py", src);
        let events = a.analyze(&file);
        let pv: Vec<_> = events.iter().filter(|e| e.kind == EventKind::PageView).collect();
        assert_eq!(pv.len(), 1, "expected one PageView for GET route");
        assert!(pv[0].name.contains("users"));
    }

    #[test]
    fn detects_post_route() {
        let a = adapter("/proj");
        let src = r#"
@app.route("/login", methods=["POST"])
def login_post():
    pass
"#;
        let file = parse("/proj", "views.py", src);
        let events = a.analyze(&file);
        let api: Vec<_> = events.iter().filter(|e| e.kind == EventKind::ApiCall).collect();
        assert_eq!(api.len(), 1, "POST route should emit ApiCall");
    }

    #[test]
    fn detects_multiple_methods() {
        let a = adapter("/proj");
        let src = r#"
@app.route("/login", methods=["GET", "POST"])
def login():
    pass
"#;
        let file = parse("/proj", "views.py", src);
        let events = a.analyze(&file);
        // GET → PageView, POST → ApiCall
        assert_eq!(events.len(), 2, "GET+POST should produce 2 events");
    }

    #[test]
    fn detects_blueprint_route() {
        let a = adapter("/proj");
        let src = "@auth_bp.route(\"/register\", methods=[\"POST\"])\ndef register():\n    pass\n";
        let file = parse("/proj", "auth.py", src);
        let events = a.analyze(&file);
        assert!(!events.is_empty(), "blueprint route should be detected");
    }

    #[test]
    fn detects_login_user_import() {
        let a = adapter("/proj");
        let src = "from flask_login import login_user\n";
        let file = parse("/proj", "auth.py", src);
        let events = a.analyze(&file);
        let auth: Vec<_> = events.iter().filter(|e| e.kind == EventKind::AuthEvent).collect();
        assert!(!auth.is_empty(), "login_user import should propose AuthEvent");
    }

    #[test]
    fn detects_logout_user_import() {
        let a = adapter("/proj");
        let src = "from flask_login import logout_user\n";
        let file = parse("/proj", "auth.py", src);
        let events = a.analyze(&file);
        let auth: Vec<_> = events.iter().filter(|e| e.kind == EventKind::AuthEvent).collect();
        assert!(!auth.is_empty(), "logout_user import should propose AuthEvent");
    }

    #[test]
    fn detects_error_handler() {
        let a = adapter("/proj");
        let src = "@app.errorhandler(404)\ndef not_found(error):\n    pass\n";
        let file = parse("/proj", "errors.py", src);
        let events = a.analyze(&file);
        let err: Vec<_> = events.iter().filter(|e| e.kind == EventKind::Error).collect();
        assert_eq!(err.len(), 1, "errorhandler should produce Error kind event");
    }

    #[test]
    fn wtforms_fields_become_hints() {
        let a = adapter("/proj");
        let src = r#"
class LoginForm(FlaskForm):
    username = StringField("Username")
    password = PasswordField("Password")
"#;
        let file = parse("/proj", "forms.py", src);
        let events = a.analyze(&file);
        let form = events.iter().find(|e| e.kind == EventKind::FormSubmit);
        assert!(form.is_some(), "form class should emit FormSubmit");
        let props = &form.unwrap().properties;
        assert!(props.iter().any(|p| p.name == "username"));
        assert!(props.iter().any(|p| p.name == "password"));
    }

    #[test]
    fn email_field_pii() {
        let a = adapter("/proj");
        let src = "class RegisterForm(FlaskForm):\n    email = EmailField(\"Email\")\n";
        let file = parse("/proj", "forms.py", src);
        let events = a.analyze(&file);
        let form = events.iter().find(|e| e.kind == EventKind::FormSubmit).unwrap();
        let email = form.properties.iter().find(|p| p.name == "email");
        assert!(email.is_some(), "email prop missing");
        assert!(email.unwrap().pii_hint, "email should be PII");
    }

    #[test]
    fn all_proposals_carry_flask_attribution() {
        let a = adapter("/proj");
        let src = "@app.route(\"/home\")\ndef home():\n    pass\n";
        let file = parse("/proj", "views.py", src);
        for event in a.analyze(&file) {
            assert_eq!(event.adapter, "flask");
        }
    }

    #[test]
    fn route_without_methods_defaults_get() {
        let a = adapter("/proj");
        let src = "@app.route(\"/dashboard\")\ndef dashboard():\n    pass\n";
        let file = parse("/proj", "views.py", src);
        let events = a.analyze(&file);
        // No methods kwarg → default GET → PageView
        assert!(events.iter().any(|e| e.kind == EventKind::PageView), "default GET should emit PageView");
    }
}
