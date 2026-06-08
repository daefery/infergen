//! Integration tests for E5.2 — Go parser, adapters, and codegen.

use std::path::PathBuf;

use infergen_core::{
    EchoAdapter, GinAdapter, GoCodegenConfig, GoParser, NetHttpAdapter,
    adapter::Adapter,
    detect::Framework,
    generate_go,
    parser::LanguageParser,
    Catalog, CatalogEntry, CatalogEventKind, EventProvenance, EventStatus,
    CATALOG_SCHEMA_VERSION,
};
use infergen_types::EventProperty;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_go(rel: &str, src: &str) -> infergen_core::ParsedFile {
    GoParser.parse(&PathBuf::from(rel), src).unwrap()
}

fn make_catalog(events: Vec<CatalogEntry>) -> Catalog {
    Catalog { schema_version: CATALOG_SCHEMA_VERSION, events, flows: Vec::new() }
}

fn make_entry(name: &str) -> CatalogEntry {
    CatalogEntry {
        id: format!("evt_{name}"),
        name: name.to_owned(),
        description: String::new(),
        status: EventStatus::Approved,
        confidence: 0.9,
        kind: CatalogEventKind::ApiCall,
        provenance: vec![EventProvenance {
            source_path: "main.go".into(),
            line: None,
            adapter: "gin".into(),
        }],
        properties: Vec::new(),
        providers: Vec::new(),
        package: None,
        flow_ids: Vec::new(),
    }
}

fn make_prop(name: &str, t: Option<&str>) -> EventProperty {
    EventProperty { name: name.into(), prop_type: t.map(Into::into), required: false, pii: false }
}

// ---------------------------------------------------------------------------
// GoParser integration
// ---------------------------------------------------------------------------

#[test]
fn go_parser_roundtrips_source() {
    let src = "package main\n\nimport \"fmt\"\n\nfunc main() {\n\tfmt.Println(\"hello\")\n}\n";
    let f = parse_go("main.go", src);
    assert_eq!(f.source, src);
    assert!(f.diagnostics.is_empty());
}

#[test]
fn go_parser_rejects_non_go_extension() {
    let result = GoParser.parse(&PathBuf::from("main.rs"), "fn main() {}");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// GinAdapter integration
// ---------------------------------------------------------------------------

#[test]
fn gin_adapter_full_gin_router_source() {
    let a = GinAdapter::new("/proj");
    let src = r#"
package main

import "github.com/gin-gonic/gin"

func main() {
    r := gin.Default()
    r.GET("/health", HealthCheck)
    r.POST("/users", CreateUser)
}
"#;
    let file = parse_go("/proj/main.go", src);
    let events = a.analyze(&file);
    assert_eq!(events.len(), 2, "expected 2 events, got {}: {:?}", events.len(), events.iter().map(|e| &e.name).collect::<Vec<_>>());
}

#[test]
fn gin_adapter_dynamic_path_param() {
    let a = GinAdapter::new("/proj");
    let src = "r.GET(\"/users/:id\", GetUser)\n";
    let file = parse_go("/proj/main.go", src);
    let events = a.analyze(&file);
    assert_eq!(events.len(), 1);
    assert!(events[0].name.contains("users"), "name: {}", events[0].name);
}

#[test]
fn gin_adapter_package_qualified_handler() {
    let a = GinAdapter::new("/proj");
    let src = "r.GET(\"/users\", controllers.CreateUser)\n";
    let file = parse_go("/proj/main.go", src);
    let events = a.analyze(&file);
    assert_eq!(events.len(), 1);
    assert!(events[0].name.contains("users"), "name: {}", events[0].name);
}

#[test]
fn gin_adapter_attribution() {
    let a = GinAdapter::new("/proj");
    let src = "r.GET(\"/ping\", Ping)\nr.POST(\"/items\", Create)\n";
    let file = parse_go("/proj/main.go", src);
    for event in a.analyze(&file) {
        assert_eq!(event.adapter, "gin");
    }
}

#[test]
fn gin_adapter_all_methods_detected() {
    let a = GinAdapter::new("/proj");
    let src = "r.GET(\"/a\", A)\nr.POST(\"/b\", B)\nr.PUT(\"/c\", C)\nr.DELETE(\"/d\", D)\nr.PATCH(\"/e\", E)\n";
    let file = parse_go("/proj/main.go", src);
    assert_eq!(a.analyze(&file).len(), 5);
}

// ---------------------------------------------------------------------------
// EchoAdapter integration
// ---------------------------------------------------------------------------

#[test]
fn echo_adapter_full_echo_source() {
    let a = EchoAdapter::new("/proj");
    let src = r#"
package main

import "github.com/labstack/echo/v4"

func main() {
    e := echo.New()
    e.GET("/ping", Ping)
    e.POST("/login", Login)
}
"#;
    let file = parse_go("/proj/main.go", src);
    let events = a.analyze(&file);
    assert_eq!(events.len(), 2);
}

#[test]
fn echo_adapter_attribution() {
    let a = EchoAdapter::new("/proj");
    let src = "e.GET(\"/ping\", Ping)\ne.POST(\"/login\", Login)\n";
    let file = parse_go("/proj/main.go", src);
    for event in a.analyze(&file) {
        assert_eq!(event.adapter, "echo");
    }
}

#[test]
fn echo_framework_is_echo() {
    assert_eq!(EchoAdapter::new("/proj").framework(), Framework::Echo);
}

// ---------------------------------------------------------------------------
// NetHttpAdapter integration
// ---------------------------------------------------------------------------

#[test]
fn nethttp_adapter_handlefunc_source() {
    let a = NetHttpAdapter::new("/proj");
    let src = "http.HandleFunc(\"/\", RootHandler)\nhttp.HandleFunc(\"/api/health\", HealthHandler)\n";
    let file = parse_go("/proj/main.go", src);
    let events = a.analyze(&file);
    assert_eq!(events.len(), 2);
}

#[test]
fn nethttp_adapter_handler_func_detection() {
    let a = NetHttpAdapter::new("/proj");
    let src = "func UsersHandler(w http.ResponseWriter, r *http.Request) {}\n";
    let file = parse_go("/proj/handlers.go", src);
    let events = a.analyze(&file);
    assert_eq!(events.len(), 1);
    assert!((events[0].confidence - 0.55_f32).abs() < f32::EPSILON);
}

#[test]
fn nethttp_adapter_combined_registration_and_declaration() {
    let a = NetHttpAdapter::new("/proj");
    let src = [
        "http.HandleFunc(\"/health\", HealthHandler)\n",
        "func HealthHandler(w http.ResponseWriter, r *http.Request) {}\n",
    ]
    .concat();
    let file = parse_go("/proj/main.go", &src);
    let events = a.analyze(&file);
    // Registered AND declared → dedup to one event (from registration pass).
    assert_eq!(events.len(), 1, "expected dedup to 1 event");
}

#[test]
fn nethttp_framework_is_net_http() {
    assert_eq!(NetHttpAdapter::new("/proj").framework(), Framework::NetHttp);
}

// ---------------------------------------------------------------------------
// Go Codegen integration
// ---------------------------------------------------------------------------

#[test]
fn go_codegen_two_events_sorted() {
    let cat = make_catalog(vec![
        make_entry("z_event"),
        make_entry("a_event"),
    ]);
    let go = generate_go(&cat, &GoCodegenConfig::default());
    let a_pos = go.find("AEvent").expect("AEvent not found");
    let z_pos = go.find("ZEvent").expect("ZEvent not found");
    assert!(a_pos < z_pos, "a_event must appear before z_event");
}

#[test]
fn go_codegen_with_properties_roundtrip() {
    let mut entry = make_entry("user_signed_in");
    entry.properties.push(make_prop("method", Some("string")));
    entry.properties.push(make_prop("user_id", Some("string")));
    let cat = make_catalog(vec![entry]);
    let go = generate_go(&cat, &GoCodegenConfig::default());
    assert!(go.contains("Method string"), "output:\n{go}");
    assert!(go.contains("UserId string"), "output:\n{go}");
    assert!(go.contains("func TrackUserSignedIn"), "output:\n{go}");
}

#[test]
fn go_codegen_compiles_via_gofmt_if_available() {
    use std::process::{Command, Stdio};
    use std::io::Write;

    // Skip if gofmt not available.
    if Command::new("gofmt").arg("-h").output().is_err() {
        return;
    }

    let mut entry = make_entry("page_viewed");
    entry.properties.push(make_prop("route", Some("string")));
    let cat = make_catalog(vec![entry]);
    let go = generate_go(&cat, &GoCodegenConfig::default());

    let mut child = Command::new("gofmt")
        .arg("-e")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("gofmt spawn failed");

    child.stdin.as_mut().unwrap().write_all(go.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(
        output.status.success(),
        "gofmt rejected generated Go code:\nSTDERR: {}\nSOURCE:\n{}",
        String::from_utf8_lossy(&output.stderr),
        go
    );
}
