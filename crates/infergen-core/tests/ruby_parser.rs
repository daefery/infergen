//! Integration tests for E5.3 Ruby Parser & Adapters.
//!
//! Tests exercise `RubyParser`, `RailsAdapter`, and `generate_ruby`
//! end-to-end using public crate API only.

use std::path::PathBuf;

use infergen_core::{
    RailsAdapter, RubyCodegenConfig, RubyParser,
    adapter::{Adapter, EventKind},
    codegen::generate_ruby,
    detect::Framework,
    parser::LanguageParser,
};
use infergen_types::{
    Catalog, CatalogEntry, CatalogEventKind, EventProperty, EventProvenance, EventStatus,
    CATALOG_SCHEMA_VERSION,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_ruby(rel: &str, src: &str) -> infergen_core::ParsedFile {
    RubyParser.parse(&PathBuf::from("/proj").join(rel), src).unwrap()
}

fn adapter() -> RailsAdapter {
    RailsAdapter::new(PathBuf::from("/proj"))
}

fn make_catalog(entries: Vec<CatalogEntry>) -> Catalog {
    Catalog { schema_version: CATALOG_SCHEMA_VERSION, events: entries }
}

fn make_entry(name: &str, status: EventStatus) -> CatalogEntry {
    CatalogEntry {
        id: format!("evt_{name}"),
        name: name.to_owned(),
        description: String::new(),
        status,
        confidence: 0.9,
        kind: CatalogEventKind::ApiCall,
        provenance: vec![EventProvenance {
            source_path: "config/routes.rb".into(),
            line: None,
            adapter: "rails".into(),
        }],
        properties: Vec::new(),
        providers: Vec::new(),
    }
}

fn make_prop(name: &str, t: Option<&str>, pii: bool) -> EventProperty {
    EventProperty { name: name.into(), prop_type: t.map(Into::into), required: false, pii }
}

// ---------------------------------------------------------------------------
// RubyParser integration
// ---------------------------------------------------------------------------

#[test]
fn ruby_parser_roundtrips_source() {
    let src = "get \"/users\", to: \"users#index\"\nresources :posts\ndevise_for :users\n";
    let f = parse_ruby("config/routes.rb", src);
    assert_eq!(f.source, src);
}

#[test]
fn ruby_parser_rejects_non_rb_extension() {
    let result = RubyParser.parse(&PathBuf::from("/proj/main.rs"), "");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// RailsAdapter — routes.rb
// ---------------------------------------------------------------------------

#[test]
fn rails_adapter_full_routes_file() {
    let src = "get \"/users\", to: \"users#index\"\npost \"/users\", to: \"users#create\"\n";
    let f = parse_ruby("config/routes.rb", src);
    let events = adapter().analyze(&f);
    assert_eq!(events.len(), 2);
    for e in &events {
        assert_eq!(e.kind, EventKind::ApiCall);
    }
}

#[test]
fn rails_adapter_resources_full_crud() {
    let f = parse_ruby("config/routes.rb", "resources :posts\n");
    let events = adapter().analyze(&f);
    // CRUD actions: index, show, new, create, edit, update, destroy = 7
    assert_eq!(events.len(), 7);
}

#[test]
fn rails_adapter_devise_auth_events() {
    let f = parse_ruby("config/routes.rb", "devise_for :users\n");
    let events = adapter().analyze(&f);
    assert_eq!(events.len(), 3);
    for e in &events {
        assert_eq!(e.kind, EventKind::AuthEvent);
    }
}

#[test]
fn rails_adapter_mixed_routes_and_devise() {
    let src = "get \"/home\", to: \"pages#home\"\nresources :posts\ndevise_for :users\n";
    let f = parse_ruby("config/routes.rb", src);
    let events = adapter().analyze(&f);
    // 1 get route + 7 resources + 3 devise = 11
    assert_eq!(events.len(), 11);
}

#[test]
fn rails_adapter_arrow_syntax_detected() {
    let f = parse_ruby("config/routes.rb", "get \"/about\" => \"pages#about\"\n");
    let events = adapter().analyze(&f);
    assert_eq!(events.len(), 1);
}

#[test]
fn rails_adapter_attribution() {
    let src = "get \"/ping\", to: \"health#ping\"\nresources :items\n";
    let f = parse_ruby("config/routes.rb", src);
    for e in adapter().analyze(&f) {
        assert_eq!(e.adapter, "rails");
    }
}

// ---------------------------------------------------------------------------
// RailsAdapter — controller files
// ---------------------------------------------------------------------------

#[test]
fn rails_adapter_controller_crud_detection() {
    let src = "class UsersController < ApplicationController\n  def index\n  end\n  def show\n  end\n  def create\n  end\nend\n";
    let f = parse_ruby("app/controllers/users_controller.rb", src);
    let events = adapter().analyze(&f);
    assert_eq!(events.len(), 3);
}

#[test]
fn rails_adapter_controller_custom_action() {
    let src = "class UsersController < ApplicationController\n  def dashboard\n  end\nend\n";
    let f = parse_ruby("app/controllers/users_controller.rb", src);
    let events = adapter().analyze(&f);
    assert_eq!(events.len(), 1);
    assert!((events[0].confidence - 0.50_f32).abs() < f32::EPSILON);
}

#[test]
fn rails_adapter_non_controller_rb_skipped() {
    let src = "class User < ApplicationRecord\n  def full_name\n  end\nend\n";
    let f = parse_ruby("models/user.rb", src);
    let events = adapter().analyze(&f);
    assert!(events.is_empty());
}

// ---------------------------------------------------------------------------
// generate_ruby integration
// ---------------------------------------------------------------------------

#[test]
fn ruby_codegen_two_events_sorted() {
    let cat = make_catalog(vec![
        make_entry("z_viewed", EventStatus::Approved),
        make_entry("a_clicked", EventStatus::Approved),
    ]);
    let rb = generate_ruby(&cat, &RubyCodegenConfig::default());
    let a_pos = rb.find("a_clicked").unwrap();
    let z_pos = rb.find("z_viewed").unwrap();
    assert!(a_pos < z_pos);
}

#[test]
fn ruby_codegen_with_properties_roundtrip() {
    let mut entry = make_entry("user_signed_in", EventStatus::Approved);
    entry.properties.push(make_prop("auth_method", Some("string"), false));
    entry.properties.push(make_prop("provider", Some("string"), false));
    let cat = make_catalog(vec![entry]);
    let rb = generate_ruby(&cat, &RubyCodegenConfig::default());
    assert!(rb.contains("UserSignedInProperties"), "output:\n{rb}");
    assert!(rb.contains(":auth_method"), "output:\n{rb}");
    assert!(rb.contains(":provider"), "output:\n{rb}");
}

#[test]
fn ruby_codegen_syntax_check_via_ruby_if_available() {
    // Only runs if `ruby` binary is in PATH — skips gracefully otherwise.
    let rb_path = which_ruby();
    let Some(ruby_bin) = rb_path else {
        eprintln!("ruby not in PATH — skipping syntax check");
        return;
    };

    let cat = make_catalog(vec![{
        let mut e = make_entry("user_signed_in", EventStatus::Approved);
        e.properties.push(make_prop("auth_method", Some("string"), false));
        e
    }]);
    let src = generate_ruby(&cat, &RubyCodegenConfig::default());

    let output = std::process::Command::new(&ruby_bin)
        .args(["-e", &src])
        .output()
        .expect("failed to spawn ruby");

    assert!(
        output.status.success(),
        "ruby syntax check failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn which_ruby() -> Option<String> {
    std::process::Command::new("which")
        .arg("ruby")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}
