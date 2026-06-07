//! Integration tests for the heuristic namer (E1.2).
//!
//! Tests cover: Namer end-to-end, adapter attribution flowing through to
//! catalog entries, and public namer API accessibility.

use std::path::PathBuf;

use infergen_core::{
    Adapter, EventKind, JsParser, NameSignals, Namer, NextjsAdapter,
    from_proposals,
    namer::split_identifier,
    parser::LanguageParser,
};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn parse_file(root: &str, rel: &str, source: &str) -> infergen_core::parser::ParsedFile {
    let path = PathBuf::from(root).join(rel);
    JsParser.parse(&path, source).unwrap()
}

fn nextjs_page(root: &str, rel: &str) -> infergen_core::parser::ParsedFile {
    parse_file(root, rel, "export default function Page() {}")
}

fn nextjs_auth(root: &str, rel: &str, specifier: &str) -> infergen_core::parser::ParsedFile {
    let src = format!("import {{ {specifier} }} from 'next-auth/react';");
    parse_file(root, rel, &src)
}

fn nextjs_form(root: &str, rel: &str) -> infergen_core::parser::ParsedFile {
    parse_file(root, rel, "function handleSubmit(e) { e.preventDefault(); }")
}

// ---------------------------------------------------------------------------
// Namer determinism & priority
// ---------------------------------------------------------------------------

#[test]
fn namer_is_deterministic() {
    let namer = Namer::new();
    let signals = NameSignals {
        route: Some("/dashboard"),
        kind: EventKind::PageView,
        component_name: None,
        handler_name: None,
    };
    let r1 = namer.derive(&signals);
    let r2 = namer.derive(&signals);
    assert_eq!(r1, r2);
}

#[test]
fn namer_route_wins_over_component() {
    let namer = Namer::new();
    let result = namer.derive(&NameSignals {
        route: Some("/about"),
        component_name: Some("HomePage"),
        kind: EventKind::PageView,
        handler_name: None,
    });
    assert_eq!(result.name, "about_page_viewed");
}

#[test]
fn namer_route_wins_over_handler_for_entity() {
    let namer = Namer::new();
    let result = namer.derive(&NameSignals {
        route: Some("/api/users"),
        handler_name: Some("GET"),
        kind: EventKind::ApiCall,
        component_name: None,
    });
    // entity comes from route, action from handler
    assert_eq!(result.name, "api_users_get_api_called");
}

#[test]
fn namer_component_wins_over_handler_for_entity() {
    let namer = Namer::new();
    let result = namer.derive(&NameSignals {
        component_name: Some("CheckoutForm"),
        handler_name: Some("handleSubmit"),
        kind: EventKind::FormSubmit,
        route: None,
    });
    // entity from component "checkout", action from handler "submitted"
    assert_eq!(result.name, "checkout_submitted");
}

#[test]
fn namer_handler_action_overrides_kind_default() {
    let namer = Namer::new();
    let result = namer.derive(&NameSignals {
        handler_name: Some("signIn"),
        kind: EventKind::AuthEvent,
        route: None,
        component_name: None,
    });
    // action is "signed_in" not "triggered" (kind default)
    assert_eq!(result.name, "user_signed_in");
}

// ---------------------------------------------------------------------------
// All auth import patterns
// ---------------------------------------------------------------------------

#[test]
fn namer_all_auth_patterns() {
    let namer = Namer::new();

    let cases = [
        ("signIn",   "user_signed_in"),
        ("signOut",  "user_signed_out"),
        ("signUp",   "user_signed_up"),
        ("login",    "user_signed_in"),
        ("logout",   "user_signed_out"),
        ("register", "user_signed_up"),
    ];

    for (handler, expected_name) in &cases {
        let result = namer.derive(&NameSignals {
            handler_name: Some(handler),
            kind: EventKind::AuthEvent,
            route: None,
            component_name: None,
        });
        assert_eq!(
            result.name, *expected_name,
            "handler={handler}: expected {expected_name}, got {}",
            result.name
        );
    }
}

// ---------------------------------------------------------------------------
// HTTP method handling
// ---------------------------------------------------------------------------

#[test]
fn namer_http_methods_all() {
    let namer = Namer::new();
    let methods = ["GET", "POST", "PUT", "DELETE", "PATCH"];

    for method in &methods {
        let result = namer.derive(&NameSignals {
            route: Some("/api/items"),
            handler_name: Some(method),
            kind: EventKind::ApiCall,
            component_name: None,
        });
        let method_lower = method.to_lowercase();
        assert_eq!(
            result.name,
            format!("api_items_{method_lower}_api_called"),
            "method={method}"
        );
    }
}

// ---------------------------------------------------------------------------
// split_identifier public accessibility
// ---------------------------------------------------------------------------

#[test]
fn split_identifier_accessible_from_integration() {
    assert_eq!(split_identifier("UserProfile"), vec!["user", "profile"]);
    assert_eq!(split_identifier("handleSubmit"), vec!["handle", "submit"]);
    assert_eq!(split_identifier("GET"), vec!["get"]);
}

// ---------------------------------------------------------------------------
// NextjsAdapter — adapter attribution
// ---------------------------------------------------------------------------

#[test]
fn nextjs_adapter_page_proposals_carry_attribution() {
    let adapter = NextjsAdapter::new("/proj");
    let file = nextjs_page("/proj", "pages/about.tsx");
    let events = adapter.analyze(&file);
    assert!(!events.is_empty(), "expected at least one page event");
    for e in &events {
        assert_eq!(e.adapter, "nextjs", "event {:?} missing nextjs adapter", e.name);
    }
}

#[test]
fn nextjs_adapter_auth_proposals_carry_attribution() {
    let adapter = NextjsAdapter::new("/proj");
    let file = nextjs_auth("/proj", "src/auth.ts", "signIn");
    let events = adapter.analyze(&file);
    assert!(!events.is_empty(), "expected at least one auth event");
    for e in &events {
        assert_eq!(e.adapter, "nextjs");
    }
}

#[test]
fn nextjs_adapter_form_proposals_carry_attribution() {
    let adapter = NextjsAdapter::new("/proj");
    let file = nextjs_form("/proj", "src/form.tsx");
    let events = adapter.analyze(&file);
    assert!(!events.is_empty(), "expected at least one form event");
    for e in &events {
        assert_eq!(e.adapter, "nextjs");
    }
}

// ---------------------------------------------------------------------------
// Catalog provenance — adapter flows through from proposal to entry
// ---------------------------------------------------------------------------

#[test]
fn catalog_entry_provenance_adapter_from_nextjs() {
    let adapter = NextjsAdapter::new("/proj");
    let file = nextjs_page("/proj", "pages/about.tsx");
    let proposals = adapter.analyze(&file);
    assert!(!proposals.is_empty());

    let root = PathBuf::from("/proj");
    let catalog = from_proposals(&proposals, &root);
    assert!(!catalog.events.is_empty());

    for entry in &catalog.events {
        assert_eq!(
            entry.provenance[0].adapter, "nextjs",
            "catalog entry {:?} missing nextjs provenance",
            entry.name
        );
    }
}

#[test]
fn catalog_entry_provenance_empty_for_unattributed_proposal() {
    use infergen_core::ProposedEvent;

    let proposal = ProposedEvent::new(
        "page_viewed",
        EventKind::PageView,
        "/proj/pages/index.tsx",
        0.9,
    );
    // adapter field defaults to ""

    let root = PathBuf::from("/proj");
    let catalog = from_proposals(&[proposal], &root);
    assert_eq!(catalog.events[0].provenance[0].adapter, "");
}

// ---------------------------------------------------------------------------
// Output format
// ---------------------------------------------------------------------------

#[test]
fn namer_output_is_valid_snake_case() {
    let namer = Namer::new();
    let cases = [
        NameSignals { route: Some("/about"), kind: EventKind::PageView, component_name: None, handler_name: None },
        NameSignals { handler_name: Some("handleSubmit"), kind: EventKind::FormSubmit, route: None, component_name: None },
        NameSignals { handler_name: Some("signIn"), kind: EventKind::AuthEvent, route: None, component_name: None },
        NameSignals { route: Some("/api/users"), handler_name: Some("GET"), kind: EventKind::ApiCall, component_name: None },
        NameSignals { kind: EventKind::Error, route: None, component_name: None, handler_name: None },
    ];

    for signals in &cases {
        let result = namer.derive(signals);
        assert!(
            result.name.chars().all(|c| c.is_lowercase() || c == '_' || c.is_numeric()),
            "name {:?} is not snake_case",
            result.name
        );
        assert!(!result.name.is_empty(), "name must not be empty");
    }
}

// ---------------------------------------------------------------------------
// Confidence ordering
// ---------------------------------------------------------------------------

#[test]
fn confidence_route_higher_than_fallback() {
    let namer = Namer::new();
    let route = namer.derive(&NameSignals {
        route: Some("/about"),
        kind: EventKind::PageView,
        component_name: None,
        handler_name: None,
    });
    let fallback = namer.derive(&NameSignals {
        kind: EventKind::PageView,
        route: None,
        component_name: None,
        handler_name: None,
    });
    assert!(route.confidence > fallback.confidence);
}
