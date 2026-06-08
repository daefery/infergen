//! Integration tests for E5.1: Python parser + adapters + Python codegen.

use std::path::PathBuf;

use infergen_core::{
    DjangoAdapter, FastApiAdapter, FlaskAdapter, PyParser,
    adapter::Adapter,
    adapter::EventKind,
    codegen::{CodegenConfig, generate_python},
    parser::LanguageParser,
};
use infergen_types::{
    CatalogEntry, CatalogEventKind, EventProperty, EventProvenance, EventStatus,
    CATALOG_SCHEMA_VERSION,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_py(project_root: &str, rel_path: &str, source: &str) -> infergen_core::ParsedFile {
    let path = PathBuf::from(project_root).join(rel_path);
    PyParser.parse(&path, source).unwrap()
}

fn make_catalog(entries: Vec<CatalogEntry>) -> infergen_core::Catalog {
    infergen_core::Catalog { schema_version: CATALOG_SCHEMA_VERSION, events: entries, flows: Vec::new() }
}

fn make_entry(name: &str) -> CatalogEntry {
    CatalogEntry {
        id: format!("evt_{name}"),
        name: name.to_owned(),
        description: String::new(),
        status: EventStatus::Approved,
        confidence: 0.9,
        kind: CatalogEventKind::PageView,
        provenance: vec![EventProvenance {
            source_path: "views.py".into(),
            line: None,
            adapter: String::new(),
        }],
        properties: Vec::new(),
        providers: Vec::new(),
        package: None,
        flow_ids: Vec::new(),
    }
}

fn make_prop(name: &str, t: Option<&str>, pii: bool) -> EventProperty {
    EventProperty { name: name.into(), prop_type: t.map(Into::into), required: false, pii }
}

// ---------------------------------------------------------------------------
// PyParser — integration
// ---------------------------------------------------------------------------

#[test]
fn py_parser_valid_function_no_errors() {
    let f = parse_py("/proj", "greet.py", "def greet(name: str) -> str:\n    return name\n");
    assert!(f.diagnostics.is_empty());
    assert_eq!(f.lang, infergen_core::detect::Language::Python);
}

#[test]
fn py_parser_class_no_errors() {
    let f = parse_py("/proj", "views.py", "class MyView(View):\n    pass\n");
    assert!(f.diagnostics.is_empty());
}

#[test]
fn py_parser_syntax_tolerant() {
    // Even malformed source must not panic or return Err.
    let f = PyParser.parse(PathBuf::from("broken.py").as_path(), "def broken(").unwrap();
    assert_eq!(f.lang, infergen_core::detect::Language::Python);
}

#[test]
fn py_parser_empty_source() {
    let f = parse_py("/proj", "empty.py", "");
    assert!(f.diagnostics.is_empty());
    let count: usize = f.with_py_ast(|s| s.len());
    assert_eq!(count, 0);
}

#[test]
fn py_parser_lang_is_python() {
    let f = parse_py("/proj", "main.py", "x = 1\n");
    assert_eq!(f.lang, infergen_core::detect::Language::Python);
}

// ---------------------------------------------------------------------------
// FastAPI adapter — integration
// ---------------------------------------------------------------------------

#[test]
fn fastapi_detects_app_get_route() {
    let a = FastApiAdapter::new(PathBuf::from("/proj"));
    let src = "@app.get(\"/items\")\nasync def list_items():\n    pass\n";
    let file = parse_py("/proj", "main.py", src);
    let events = a.analyze(&file);
    let api: Vec<_> = events.iter().filter(|e| e.kind == EventKind::ApiCall).collect();
    assert!(!api.is_empty(), "expected ApiCall from @app.get");
    assert!(api[0].name.contains("items"));
}

#[test]
fn fastapi_detects_router_post_route() {
    let a = FastApiAdapter::new(PathBuf::from("/proj"));
    let src = "@router.post(\"/users\")\nasync def create_user():\n    pass\n";
    let file = parse_py("/proj", "routers/users.py", src);
    let events = a.analyze(&file);
    assert!(events.iter().any(|e| e.kind == EventKind::ApiCall));
}

#[test]
fn fastapi_events_carry_fastapi_attribution() {
    let a = FastApiAdapter::new(PathBuf::from("/proj"));
    let src = "@app.get(\"/ping\")\nasync def ping():\n    pass\n";
    let file = parse_py("/proj", "health.py", src);
    for event in a.analyze(&file) {
        assert_eq!(event.adapter, "fastapi");
    }
}

#[test]
fn fastapi_detects_oauth2_import() {
    let a = FastApiAdapter::new(PathBuf::from("/proj"));
    let src = "from fastapi.security import OAuth2PasswordBearer\n";
    let file = parse_py("/proj", "security.py", src);
    let events = a.analyze(&file);
    assert!(events.iter().any(|e| e.kind == EventKind::AuthEvent));
}

#[test]
fn fastapi_non_python_returns_empty() {
    let a = FastApiAdapter::new(PathBuf::from("/proj"));
    let file = infergen_core::JsParser
        .parse(&PathBuf::from("/proj/app.ts"), "const x = 1;")
        .unwrap();
    assert!(a.analyze(&file).is_empty());
}

#[test]
fn fastapi_empty_file_returns_empty() {
    let a = FastApiAdapter::new(PathBuf::from("/proj"));
    let file = parse_py("/proj", "empty.py", "");
    assert!(a.analyze(&file).is_empty());
}

#[test]
fn fastapi_confidence_on_route_is_high() {
    let a = FastApiAdapter::new(PathBuf::from("/proj"));
    let src = "@app.get(\"/users\")\nasync def users():\n    pass\n";
    let file = parse_py("/proj", "api/users.py", src);
    let events = a.analyze(&file);
    let api = events.iter().find(|e| e.kind == EventKind::ApiCall).unwrap();
    assert!(api.confidence >= 0.85);
}

#[test]
fn fastapi_detects_delete_route() {
    let a = FastApiAdapter::new(PathBuf::from("/proj"));
    let src = "@app.delete(\"/items/{id}\")\nasync def delete_item(id: int):\n    pass\n";
    let file = parse_py("/proj", "main.py", src);
    assert!(a.analyze(&file).iter().any(|e| e.kind == EventKind::ApiCall));
}

// ---------------------------------------------------------------------------
// Django adapter — integration
// ---------------------------------------------------------------------------

#[test]
fn django_detects_urlpatterns_path() {
    let a = DjangoAdapter::new(PathBuf::from("/proj"));
    let src = "urlpatterns = [path(\"users/\", views.UserList, name=\"user-list\"),]\n";
    let file = parse_py("/proj", "urls.py", src);
    assert!(!a.analyze(&file).is_empty(), "urlpatterns should produce events");
}

#[test]
fn django_detects_login_view_cbv() {
    let a = DjangoAdapter::new(PathBuf::from("/proj"));
    let src = "class MyLoginView(LoginView):\n    template_name = \"login.html\"\n";
    let file = parse_py("/proj", "views.py", src);
    assert!(a.analyze(&file).iter().any(|e| e.kind == EventKind::AuthEvent));
}

#[test]
fn django_detects_list_view_cbv() {
    let a = DjangoAdapter::new(PathBuf::from("/proj"));
    let src = "class ArticleList(ListView):\n    model = Article\n";
    let file = parse_py("/proj", "views.py", src);
    assert!(a.analyze(&file).iter().any(|e| e.kind == EventKind::PageView));
}

#[test]
fn django_email_form_field_pii_flagged() {
    let a = DjangoAdapter::new(PathBuf::from("/proj"));
    let src = "class ContactForm(forms.Form):\n    email = forms.EmailField()\n";
    let file = parse_py("/proj", "forms.py", src);
    let events = a.analyze(&file);
    let form = events.iter().find(|e| e.kind == EventKind::FormSubmit).unwrap();
    let email_prop = form.properties.iter().find(|p| p.name == "email");
    assert!(email_prop.is_some());
    assert!(email_prop.unwrap().pii_hint);
}

#[test]
fn django_all_proposals_carry_attribution() {
    let a = DjangoAdapter::new(PathBuf::from("/proj"));
    let src = "urlpatterns = [path(\"home/\", views.Home),]\n";
    let file = parse_py("/proj", "urls.py", src);
    for event in a.analyze(&file) {
        assert_eq!(event.adapter, "django");
    }
}

#[test]
fn django_non_python_returns_empty() {
    let a = DjangoAdapter::new(PathBuf::from("/proj"));
    let file = infergen_core::JsParser
        .parse(&PathBuf::from("/proj/app.ts"), "const x = 1;")
        .unwrap();
    assert!(a.analyze(&file).is_empty());
}

#[test]
fn django_detects_auth_import() {
    let a = DjangoAdapter::new(PathBuf::from("/proj"));
    let src = "from django.contrib.auth.views import LoginView\n";
    let file = parse_py("/proj", "urls.py", src);
    assert!(a.analyze(&file).iter().any(|e| e.kind == EventKind::AuthEvent));
}

// ---------------------------------------------------------------------------
// Flask adapter — integration
// ---------------------------------------------------------------------------

#[test]
fn flask_detects_app_route_get() {
    let a = FlaskAdapter::new(PathBuf::from("/proj"));
    let src = "@app.route(\"/users\")\ndef user_list():\n    pass\n";
    let file = parse_py("/proj", "views.py", src);
    assert!(a.analyze(&file).iter().any(|e| e.kind == EventKind::PageView));
}

#[test]
fn flask_detects_post_route() {
    let a = FlaskAdapter::new(PathBuf::from("/proj"));
    let src = "@app.route(\"/login\", methods=[\"POST\"])\ndef login_post():\n    pass\n";
    let file = parse_py("/proj", "views.py", src);
    assert!(a.analyze(&file).iter().any(|e| e.kind == EventKind::ApiCall));
}

#[test]
fn flask_detects_multiple_methods_expand() {
    let a = FlaskAdapter::new(PathBuf::from("/proj"));
    let src = "@app.route(\"/orders\", methods=[\"GET\", \"POST\"])\ndef orders():\n    pass\n";
    let file = parse_py("/proj", "views.py", src);
    assert_eq!(a.analyze(&file).len(), 2, "GET+POST → 2 events");
}

#[test]
fn flask_detects_login_user_import() {
    let a = FlaskAdapter::new(PathBuf::from("/proj"));
    let src = "from flask_login import login_user\n";
    let file = parse_py("/proj", "auth.py", src);
    assert!(a.analyze(&file).iter().any(|e| e.kind == EventKind::AuthEvent));
}

#[test]
fn flask_detects_error_handler() {
    let a = FlaskAdapter::new(PathBuf::from("/proj"));
    let src = "@app.errorhandler(404)\ndef not_found(error):\n    pass\n";
    let file = parse_py("/proj", "errors.py", src);
    assert!(a.analyze(&file).iter().any(|e| e.kind == EventKind::Error));
}

#[test]
fn flask_all_proposals_carry_attribution() {
    let a = FlaskAdapter::new(PathBuf::from("/proj"));
    let src = "@app.route(\"/home\")\ndef home():\n    pass\n";
    let file = parse_py("/proj", "views.py", src);
    for event in a.analyze(&file) {
        assert_eq!(event.adapter, "flask");
    }
}

#[test]
fn flask_non_python_returns_empty() {
    let a = FlaskAdapter::new(PathBuf::from("/proj"));
    let file = infergen_core::JsParser
        .parse(&PathBuf::from("/proj/app.ts"), "const x = 1;")
        .unwrap();
    assert!(a.analyze(&file).is_empty());
}

// ---------------------------------------------------------------------------
// Python codegen — integration
// ---------------------------------------------------------------------------

#[test]
fn py_codegen_empty_catalog_no_events() {
    let py = generate_python(&make_catalog(vec![]), &CodegenConfig::default());
    assert!(py.contains("EventName = None"), "empty catalog: {py}");
}

#[test]
fn py_codegen_approved_event_generates_typed_dict() {
    let cat = make_catalog(vec![make_entry("page_viewed")]);
    let py = generate_python(&cat, &CodegenConfig::default());
    assert!(py.contains("PageViewedProperties"), "typed dict missing: {py}");
}

#[test]
fn py_codegen_track_function_present() {
    let cat = make_catalog(vec![make_entry("page_viewed")]);
    let py = generate_python(&cat, &CodegenConfig::default());
    assert!(py.contains("def track_page_viewed("), "track fn missing: {py}");
}

#[test]
fn py_codegen_track_namespace_present() {
    let cat = make_catalog(vec![make_entry("page_viewed")]);
    let py = generate_python(&cat, &CodegenConfig::default());
    assert!(py.contains("track = _TrackNamespace()"), "namespace missing: {py}");
}

#[test]
fn py_codegen_pii_prop_has_comment() {
    let mut entry = make_entry("user_signed_in");
    entry.properties.push(make_prop("email", Some("string"), true));
    let py = generate_python(&make_catalog(vec![entry]), &CodegenConfig::default());
    assert!(py.contains("PII: handle with care"), "PII comment missing: {py}");
}

#[test]
fn py_codegen_sorted_alphabetically() {
    let cat = make_catalog(vec![make_entry("z_event"), make_entry("a_event")]);
    let py = generate_python(&cat, &CodegenConfig::default());
    assert!(py.find("a_event").unwrap() < py.find("z_event").unwrap());
}

#[test]
fn py_codegen_deterministic() {
    let cat = make_catalog(vec![make_entry("page_viewed")]);
    let config = CodegenConfig::default();
    assert_eq!(
        generate_python(&cat, &config),
        generate_python(&cat, &config)
    );
}

#[test]
fn py_codegen_dispatch_called() {
    let cat = make_catalog(vec![make_entry("page_viewed")]);
    let py = generate_python(&cat, &CodegenConfig::default());
    assert!(py.contains("_dispatch(\"page_viewed\""), "dispatch not called: {py}");
}
