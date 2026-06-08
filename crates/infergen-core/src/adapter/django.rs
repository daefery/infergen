//! Django framework adapter.
//!
//! Detection strategy:
//!
//! 1. **Path-based** — always runs.  `urls.py` → URL patterns; `views.py` →
//!    view functions/CBVs; `forms.py` → form fields as property hints.
//! 2. **AST-based** — runs via [`ParsedFile::with_py_ast`].
//!
//! All proposals carry `adapter = "django"`.

use std::path::{Path, PathBuf};

use crate::{
    detect::Framework,
    namer::{NameSignals, Namer},
    parser::{ParsedFile, py::PyStmt},
    property,
};

use super::{Adapter, EventKind, PropertyHint, ProposedEvent};

// ---------------------------------------------------------------------------
// Django CBV base → event-kind mapping
// ---------------------------------------------------------------------------

/// Django class-based view bases that map to specific event kinds.
const CBV_PAGE_VIEW_BASES: &[&str] = &[
    "View", "TemplateView", "DetailView", "ListView", "RedirectView",
    "BaseDetailView", "BaseListView",
];
const CBV_FORM_SUBMIT_BASES: &[&str] = &[
    "CreateView", "UpdateView", "FormView", "BaseFormView",
];
const CBV_AUTH_BASES: &[&str] = &[
    "LoginView", "LogoutView", "PasswordChangeView", "PasswordResetView",
    "PasswordResetConfirmView", "RegistrationView",
];
const CBV_BUTTON_CLICK_BASES: &[&str] = &["DeleteView", "BaseDeleteView"];

/// Django auth-related import names.
const AUTH_IMPORTS: &[&str] = &[
    "LoginView", "LogoutView", "login_required", "login_user", "logout_user",
    "authenticate", "login", "logout", "UserCreationForm", "AuthenticationForm",
    "PasswordChangeForm",
];

/// Form field class names that imply a string type.
const STRING_FIELD_CLASSES: &[&str] = &[
    "CharField", "TextField", "URLField", "SlugField", "GenericIPAddressField",
    "FilePathField", "UUIDField", "JSONField",
];
const NUMBER_FIELD_CLASSES: &[&str] = &[
    "IntegerField", "FloatField", "DecimalField", "PositiveIntegerField",
    "SmallIntegerField", "BigIntegerField", "DurationField",
];
const BOOL_FIELD_CLASSES: &[&str] = &["BooleanField", "NullBooleanField"];
const EMAIL_FIELD_CLASSES: &[&str] = &["EmailField"];
const PASSWORD_FIELD_CLASSES: &[&str] = &["PasswordInput"];
const PII_FIELD_CLASSES: &[&str] = &[
    "EmailField", "PasswordInput", "GenericIPAddressField",
];

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Django framework adapter.
pub struct DjangoAdapter {
    /// Absolute path to the Python project root.
    pub project_root: PathBuf,
}

impl DjangoAdapter {
    /// Create a new adapter rooted at `project_root`.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self { project_root: project_root.into() }
    }
}

impl Adapter for DjangoAdapter {
    fn analyze(&self, file: &ParsedFile) -> Vec<ProposedEvent> {
        if file.lang != crate::detect::Language::Python {
            return Vec::new();
        }

        let mut events: Vec<ProposedEvent> = Vec::new();
        let namer = Namer::new();

        let rel = file.path.strip_prefix(&self.project_root).unwrap_or(&file.path);
        let file_stem = rel
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        let source_path = file.path.clone();

        let raw_source = &file.source;
        let ast_events: Vec<ProposedEvent> = file.with_py_ast(|stmts| {
            let mut result = Vec::new();

            match file_stem {
                "urls" => {
                    // Multiline urlpatterns: scan raw source (stmt value only has `[`).
                    detect_url_patterns_source(raw_source, &source_path, &namer, &mut result);
                    // Auth imports are common in urls.py too.
                    detect_auth_imports(stmts, &source_path, &namer, &mut result);
                }
                "views" => detect_views(stmts, &source_path, &namer, &mut result),
                "forms" => {
                    // Collect form fields but attach them to placeholder FormSubmit
                    // events representing each form class.
                    detect_form_classes(stmts, &source_path, &namer, &mut result);
                }
                "auth" | "authentication" | "login" | "register" | "signup" => {
                    detect_auth_file(stmts, &source_path, &namer, &mut result);
                }
                _ => {
                    // Generic Python file: check imports for auth patterns.
                    detect_auth_imports(stmts, &source_path, &namer, &mut result);
                    // Also check for CBVs in any file.
                    detect_views(stmts, &source_path, &namer, &mut result);
                }
            }

            result
        });
        events.extend(ast_events);

        for event in &mut events {
            let props = std::mem::take(&mut event.properties);
            event.properties = property::enrich_hints(props);
            event.adapter = "django".to_owned();
        }

        events
    }

    fn framework(&self) -> Framework {
        Framework::Django
    }
}

// ---------------------------------------------------------------------------
// URL pattern detection
// ---------------------------------------------------------------------------

/// Detect Django URL patterns from `urlpatterns = [...]`.
fn detect_url_patterns(
    stmts: &[PyStmt],
    source_path: &Path,
    namer: &Namer,
    out: &mut Vec<ProposedEvent>,
) {
    for stmt in stmts {
        let PyStmt::Assign { target, value } = stmt else { continue };
        if target != "urlpatterns" {
            continue;
        }
        // Parse `path("url/", view, name="slug")` entries in the value text.
        for path_call in extract_path_calls(value) {
            let route = normalise_django_path(&path_call.url);
            // Determine kind: if route starts with "api/" or "api", treat as ApiCall.
            let kind = if route.starts_with("/api/") || route.starts_with("api/") {
                EventKind::ApiCall
            } else {
                EventKind::PageView
            };
            let result = namer.derive(&NameSignals {
                route: Some(&route),
                handler_name: path_call.name.as_deref(),
                kind,
                component_name: None,
            });
            let confidence: f32 = if path_call.name.is_some() { 0.85 } else { 0.7 };
            let event = if kind == EventKind::ApiCall {
                ProposedEvent::new(result.name, kind, source_path, confidence)
                    .with_prop("endpoint", Some("string"))
            } else {
                ProposedEvent::new(result.name, kind, source_path, confidence)
                    .with_prop("route", Some("string"))
            };
            out.push(event);
        }
    }
}

/// Raw-source URL detection: handles multiline `urlpatterns = [\n    path(...)\n]`.
fn detect_url_patterns_source(
    source: &str,
    source_path: &Path,
    namer: &Namer,
    out: &mut Vec<ProposedEvent>,
) {
    if !source.contains("urlpatterns") {
        return;
    }
    for path_call in extract_path_calls(source) {
        let route = normalise_django_path(&path_call.url);
        let kind = if route.starts_with("/api/") || route.starts_with("api/") {
            EventKind::ApiCall
        } else {
            EventKind::PageView
        };
        let result = namer.derive(&NameSignals {
            route: Some(&route),
            handler_name: path_call.name.as_deref(),
            kind,
            component_name: None,
        });
        let confidence: f32 = if path_call.name.is_some() { 0.85 } else { 0.7 };
        let event = if kind == EventKind::ApiCall {
            ProposedEvent::new(result.name, kind, source_path, confidence)
                .with_prop("endpoint", Some("string"))
        } else {
            ProposedEvent::new(result.name, kind, source_path, confidence)
                .with_prop("route", Some("string"))
        };
        out.push(event);
    }
}

struct PathCall {
    url: String,
    name: Option<String>,
}

/// Extract `path("url", ...)` and `re_path(r"url", ...)` calls from raw text.
fn extract_path_calls(text: &str) -> Vec<PathCall> {
    let mut result = Vec::new();
    // Look for `path(` or `re_path(` occurrences.
    let search_fns: &[&str] = &["path(", "re_path(", "url("];
    for call_marker in search_fns {
        let mut cursor = 0;
        while let Some(pos) = text[cursor..].find(call_marker) {
            let abs = cursor + pos + call_marker.len();
            // Extract argument text up to matching ')'.
            if let Some(args) = extract_balanced(text, abs) {
                let url = extract_first_string_arg_from_text(&args);
                let name = extract_kwarg_name(&args);
                if let Some(u) = url {
                    result.push(PathCall { url: u, name });
                }
            }
            cursor = abs;
        }
    }
    result
}

/// Extract balanced text from `text[start..]` (everything before the matching
/// closing paren). Returns `None` if paren depth is never resolved.
fn extract_balanced(text: &str, start: usize) -> Option<String> {
    if start >= text.len() {
        return None;
    }
    let mut depth = 1i32;
    let mut end = start;
    for (i, ch) in text[start..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = start + i;
                    break;
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    Some(text[start..end].to_owned())
}

/// Extract the first string literal from Django path() argument text.
fn extract_first_string_arg_from_text(text: &str) -> Option<String> {
    for quote in ['"', '\''] {
        if let Some(start) = text.find(quote) {
            let after = &text[start + 1..];
            if let Some(end) = after.find(quote) {
                let s = after[..end].trim_start_matches('r').to_owned();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    }
    None
}

/// Extract the `name="..."` kwarg from Django path() argument text.
fn extract_kwarg_name(text: &str) -> Option<String> {
    let pattern = "name=";
    let pos = text.find(pattern)?;
    let after = &text[pos + pattern.len()..];
    for quote in ['"', '\''] {
        if let Some(s) = after.strip_prefix(quote) {
            if let Some(end) = s.find(quote) {
                return Some(s[..end].to_owned());
            }
        }
    }
    None
}

/// Convert a Django path template to a route string usable by the namer.
///
/// `"users/<int:pk>/"` → `"/users/[pk]"`.
fn normalise_django_path(path: &str) -> String {
    let path = path.trim_end_matches('/');
    let path = path.trim_start_matches('^').trim_end_matches('$');
    // Replace `<type:name>` and `<name>` with `[name]`, and `(?P<name>...)` with `[name]`.
    let mut result = String::from("/");
    for segment in path.split('/') {
        if segment.is_empty() {
            continue;
        }
        let norm = if segment.starts_with('<') && segment.ends_with('>') {
            let inner = &segment[1..segment.len() - 1];
            let name = inner.split(':').last().unwrap_or(inner);
            format!("[{name}]")
        } else if segment.starts_with("(?P<") {
            // Regex group: `(?P<name>...)` → `[name]`
            let after = &segment[4..];
            if let Some(end) = after.find('>') {
                format!("[{}]", &after[..end])
            } else {
                segment.to_owned()
            }
        } else {
            segment.to_owned()
        };
        result.push_str(&norm);
        result.push('/');
    }
    result.trim_end_matches('/').to_owned()
}

// ---------------------------------------------------------------------------
// View detection (views.py / any file with CBVs)
// ---------------------------------------------------------------------------

fn detect_views(
    stmts: &[PyStmt],
    source_path: &Path,
    namer: &Namer,
    out: &mut Vec<ProposedEvent>,
) {
    for stmt in stmts {
        match stmt {
            PyStmt::ClassDef { name, bases, .. } => {
                if let Some(kind) = cbv_kind(bases) {
                    let result = namer.derive(&NameSignals {
                        component_name: Some(name),
                        kind,
                        route: None,
                        handler_name: None,
                    });
                    let confidence: f32 = if matches!(kind, EventKind::AuthEvent) { 0.9 } else { 0.8 };
                    let mut event = ProposedEvent::new(result.name, kind, source_path, confidence);
                    if kind == EventKind::AuthEvent {
                        event = event.with_prop("method", Some("string"));
                    } else if kind == EventKind::PageView {
                        event = event.with_prop("route", Some("string"));
                    }
                    out.push(event);
                }
            }
            PyStmt::FunctionDef { name, params, decorators, .. } => {
                // FBV: first param named `request`.
                let first_param = params.split(',').next().unwrap_or("").trim().to_owned();
                if first_param == "request" || first_param.starts_with("request:") {
                    // Check for auth decorators.
                    let has_login_required = decorators.iter().any(|d| d == "login_required");
                    let kind = if has_login_required || is_auth_func_name(name) {
                        EventKind::AuthEvent
                    } else {
                        EventKind::PageView
                    };
                    let result = namer.derive(&NameSignals {
                        handler_name: Some(name),
                        kind,
                        route: None,
                        component_name: None,
                    });
                    let mut event = ProposedEvent::new(result.name, kind, source_path, 0.65);
                    if kind == EventKind::PageView {
                        event = event.with_prop("route", Some("string"));
                    } else {
                        event = event.with_prop("method", Some("string"));
                    }
                    out.push(event);
                }
            }
            _ => {}
        }
    }
}

/// Map CBV base class names to an `EventKind`.
fn cbv_kind(bases: &[String]) -> Option<EventKind> {
    for base in bases {
        if CBV_AUTH_BASES.contains(&base.as_str()) {
            return Some(EventKind::AuthEvent);
        }
        if CBV_FORM_SUBMIT_BASES.contains(&base.as_str()) {
            return Some(EventKind::FormSubmit);
        }
        if CBV_BUTTON_CLICK_BASES.contains(&base.as_str()) {
            return Some(EventKind::ButtonClick);
        }
        if CBV_PAGE_VIEW_BASES.contains(&base.as_str()) {
            return Some(EventKind::PageView);
        }
    }
    None
}

fn is_auth_func_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(lower.as_str(), "login" | "logout" | "signin" | "signout" | "register" | "signup")
}

// ---------------------------------------------------------------------------
// Form class detection
// ---------------------------------------------------------------------------

fn detect_form_classes(
    stmts: &[PyStmt],
    source_path: &Path,
    namer: &Namer,
    out: &mut Vec<ProposedEvent>,
) {
    // Each form class becomes a FormSubmit event; its field assignments become
    // property hints.  We do a two-pass scan: first collect all class names
    // and their bases, then look for field assignments in the source text.
    // Since our scanner doesn't track class bodies, we use a heuristic:
    // any class inheriting a form base that is followed by `=` assignments.

    let mut form_hints: Vec<PropertyHint> = Vec::new();
    let mut form_class_name: Option<&str> = None;

    for stmt in stmts {
        match stmt {
            PyStmt::ClassDef { name, bases, .. } => {
                if is_form_base(bases) {
                    // Flush previous form if any.
                    if let Some(prev) = form_class_name.take() {
                        if !form_hints.is_empty() {
                            let result = namer.derive(&NameSignals {
                                component_name: Some(prev),
                                kind: EventKind::FormSubmit,
                                route: None,
                                handler_name: None,
                            });
                            let mut event = ProposedEvent::new(
                                result.name,
                                EventKind::FormSubmit,
                                source_path,
                                0.75,
                            );
                            event.properties = property::enrich_hints(
                                std::mem::take(&mut form_hints)
                            );
                            out.push(event);
                        }
                    }
                    form_class_name = Some(name.as_str());
                } else {
                    form_class_name = None;
                    form_hints.clear();
                }
            }
            PyStmt::Assign { target, value } if form_class_name.is_some() => {
                // `email = forms.EmailField(...)` — target is the field name.
                if let Some(hint) = form_field_hint(target, value) {
                    form_hints.push(hint);
                }
            }
            _ => {}
        }
    }

    // Flush last form.
    if let Some(name) = form_class_name {
        if !form_hints.is_empty() {
            let result = namer.derive(&NameSignals {
                component_name: Some(name),
                kind: EventKind::FormSubmit,
                route: None,
                handler_name: None,
            });
            let mut event = ProposedEvent::new(
                result.name,
                EventKind::FormSubmit,
                source_path,
                0.75,
            );
            event.properties = property::enrich_hints(form_hints);
            out.push(event);
        }
    }
}

fn is_form_base(bases: &[String]) -> bool {
    bases.iter().any(|b| {
        matches!(
            b.as_str(),
            "Form" | "ModelForm" | "forms.Form" | "forms.ModelForm"
                | "AuthenticationForm" | "UserCreationForm" | "PasswordChangeForm"
        )
    })
}

/// Derive a property hint from a form field assignment.
///
/// `target = "email"`, `value = "forms.EmailField(...)"` → `PropertyHint { name: "email", ... }`.
fn form_field_hint(target: &str, value: &str) -> Option<PropertyHint> {
    if target.starts_with('_') || !target.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    // Extract the field class name from the RHS, e.g. `forms.EmailField(...)`.
    let field_class = extract_field_class(value);
    let type_hint = field_type_hint(field_class.as_deref());
    let pii_hint = field_class.as_deref().map(|c| PII_FIELD_CLASSES.contains(&c)).unwrap_or(false)
        || property::is_pii_property(target);
    Some(PropertyHint {
        name: target.to_owned(),
        type_hint: type_hint.map(str::to_owned),
        pii_hint,
    })
}

fn extract_field_class(value: &str) -> Option<String> {
    let call_pos = value.find('(')?;
    let before_call = &value[..call_pos];
    // May be `forms.EmailField` or just `EmailField`.
    let class = before_call.split('.').last()?.trim();
    if class.is_empty() {
        None
    } else {
        Some(class.to_owned())
    }
}

fn field_type_hint(class: Option<&str>) -> Option<&'static str> {
    match class? {
        c if NUMBER_FIELD_CLASSES.contains(&c) => Some("number"),
        c if BOOL_FIELD_CLASSES.contains(&c) => Some("boolean"),
        c if EMAIL_FIELD_CLASSES.contains(&c) => Some("string"),
        c if PASSWORD_FIELD_CLASSES.contains(&c) => Some("string"),
        c if STRING_FIELD_CLASSES.contains(&c) => Some("string"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Auth file / import detection
// ---------------------------------------------------------------------------

fn detect_auth_file(
    stmts: &[PyStmt],
    source_path: &Path,
    namer: &Namer,
    out: &mut Vec<ProposedEvent>,
) {
    detect_views(stmts, source_path, namer, out);
    detect_auth_imports(stmts, source_path, namer, out);
}

fn detect_auth_imports(
    stmts: &[PyStmt],
    source_path: &Path,
    namer: &Namer,
    out: &mut Vec<ProposedEvent>,
) {
    let found = stmts.iter().any(|stmt| match stmt {
        PyStmt::ImportFrom { names, .. } => {
            names.iter().any(|n| AUTH_IMPORTS.contains(&n.as_str()))
        }
        _ => false,
    });
    if found {
        let result = namer.derive(&NameSignals {
            handler_name: Some("login"),
            kind: EventKind::AuthEvent,
            route: None,
            component_name: None,
        });
        out.push(
            ProposedEvent::new(result.name, EventKind::AuthEvent, source_path, 0.8)
                .with_prop("method", Some("string")),
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{parser::LanguageParser, parser::py::PyParser};

    fn adapter(root: &str) -> DjangoAdapter {
        DjangoAdapter::new(PathBuf::from(root))
    }

    fn parse(root: &str, rel_path: &str, source: &str) -> ParsedFile {
        let path = PathBuf::from(root).join(rel_path);
        PyParser.parse(&path, source).unwrap()
    }

    #[test]
    fn non_python_file_returns_empty() {
        let a = adapter("/proj");
        let file = crate::JsParser
            .parse(&PathBuf::from("/proj/app.ts"), "const x = 1;")
            .unwrap();
        assert!(a.analyze(&file).is_empty());
    }

    #[test]
    fn detects_url_path_pattern() {
        let a = adapter("/proj");
        let src = r#"
urlpatterns = [
    path("users/", views.UserListView.as_view(), name="user-list"),
]
"#;
        let file = parse("/proj", "urls.py", src);
        let events = a.analyze(&file);
        assert!(!events.is_empty(), "url pattern should produce event");
    }

    #[test]
    fn detects_named_route_uses_name() {
        let a = adapter("/proj");
        let src = "urlpatterns = [path(\"dashboard/\", views.DashboardView, name=\"dashboard\"),]\n";
        let file = parse("/proj", "urls.py", src);
        let events = a.analyze(&file);
        assert!(!events.is_empty());
        // Named route → higher confidence.
        assert!(events[0].confidence >= 0.84);
    }

    #[test]
    fn detects_login_view_cbv() {
        let a = adapter("/proj");
        let src = "class MyLoginView(LoginView):\n    template_name = \"login.html\"\n";
        let file = parse("/proj", "views.py", src);
        let events = a.analyze(&file);
        let auth: Vec<_> = events.iter().filter(|e| e.kind == EventKind::AuthEvent).collect();
        assert!(!auth.is_empty(), "LoginView should propose AuthEvent");
    }

    #[test]
    fn detects_list_view_cbv() {
        let a = adapter("/proj");
        let src = "class ArticleListView(ListView):\n    model = Article\n";
        let file = parse("/proj", "views.py", src);
        let events = a.analyze(&file);
        let pv: Vec<_> = events.iter().filter(|e| e.kind == EventKind::PageView).collect();
        assert!(!pv.is_empty(), "ListView should propose PageView");
    }

    #[test]
    fn detects_create_view_cbv() {
        let a = adapter("/proj");
        let src = "class ArticleCreateView(CreateView):\n    model = Article\n";
        let file = parse("/proj", "views.py", src);
        let events = a.analyze(&file);
        let fs: Vec<_> = events.iter().filter(|e| e.kind == EventKind::FormSubmit).collect();
        assert!(!fs.is_empty(), "CreateView should propose FormSubmit");
    }

    #[test]
    fn form_fields_become_property_hints() {
        let a = adapter("/proj");
        let src = r#"
class SignupForm(forms.Form):
    username = forms.CharField()
    email = forms.EmailField()
"#;
        let file = parse("/proj", "forms.py", src);
        let events = a.analyze(&file);
        let form = events.iter().find(|e| e.kind == EventKind::FormSubmit);
        assert!(form.is_some(), "form class should produce FormSubmit event");
        let props = &form.unwrap().properties;
        assert!(props.iter().any(|p| p.name == "username"), "username prop missing");
        assert!(props.iter().any(|p| p.name == "email"), "email prop missing");
    }

    #[test]
    fn email_field_flagged_pii() {
        let a = adapter("/proj");
        let src = "class ContactForm(forms.Form):\n    email = forms.EmailField()\n";
        let file = parse("/proj", "forms.py", src);
        let events = a.analyze(&file);
        let form = events.iter().find(|e| e.kind == EventKind::FormSubmit).unwrap();
        let email_prop = form.properties.iter().find(|p| p.name == "email");
        assert!(email_prop.is_some(), "email prop missing");
        assert!(email_prop.unwrap().pii_hint, "email should be PII-flagged");
    }

    #[test]
    fn detects_auth_import() {
        let a = adapter("/proj");
        let src = "from django.contrib.auth.views import LoginView\n";
        let file = parse("/proj", "urls.py", src);
        // Auth import in non-views.py file → auth event via detect_auth_imports.
        // (urls.py falls into the `_` arm which calls detect_auth_imports)
        let events = a.analyze(&file);
        assert!(events.iter().any(|e| e.kind == EventKind::AuthEvent));
    }

    #[test]
    fn all_proposals_carry_attribution() {
        let a = adapter("/proj");
        let src = "urlpatterns = [path(\"home/\", views.Home),]\n";
        let file = parse("/proj", "urls.py", src);
        for event in a.analyze(&file) {
            assert_eq!(event.adapter, "django");
        }
    }

    #[test]
    fn normalise_django_path_simple() {
        assert_eq!(normalise_django_path("users/"), "/users");
    }

    #[test]
    fn normalise_django_path_typed_param() {
        assert_eq!(normalise_django_path("users/<int:pk>/"), "/users/[pk]");
    }

    #[test]
    fn normalise_django_path_nested() {
        assert_eq!(normalise_django_path("blog/<slug:slug>/"), "/blog/[slug]");
    }
}
