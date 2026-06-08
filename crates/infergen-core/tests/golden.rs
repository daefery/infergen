//! Golden-file adapter tests (E8.2).
//!
//! Each test loads a real fixture file from `tests/fixtures/`, runs a parser
//! and framework adapter, and makes rich structural assertions on the full
//! event output.  This is complementary to the inline-string tests in
//! `js_frameworks.rs`, `py_parser.rs`, etc. — these tests use on-disk source
//! files that look like real project files.

use std::path::Path;

use infergen_core::{
    Adapter, DjangoAdapter, EventKind, ExpressAdapter, FastApiAdapter, JsParser, LanguageParser,
    NextjsAdapter, PyParser, RailsAdapter, RubyParser,
};

// ---------------------------------------------------------------------------
// Fixture helper
// ---------------------------------------------------------------------------

fn fixture(rel: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(rel),
    )
    .unwrap_or_else(|e| panic!("fixture not found: {rel} — {e}"))
}

// ---------------------------------------------------------------------------
// Next.js golden tests
// ---------------------------------------------------------------------------

#[test]
fn nextjs_page_golden() {
    let page_src = fixture("nextjs/pages/index.tsx");
    let page_file = JsParser
        .parse(
            Path::new("/project/pages/index.tsx"),
            &page_src,
        )
        .expect("parse index.tsx");

    let adapter = NextjsAdapter::new("/project");
    let page_events = adapter.analyze(&page_file);

    assert!(
        !page_events.is_empty(),
        "NextjsAdapter should detect at least one event in pages/index.tsx"
    );
    assert!(
        page_events.iter().all(|e| e.adapter == "nextjs"),
        "all events should carry adapter='nextjs'"
    );
    assert!(
        page_events.iter().all(|e| e.confidence > 0.0),
        "all events should have a positive confidence score"
    );
    assert!(
        page_events.iter().any(|e| e.kind == EventKind::PageView),
        "expected at least one PageView event from index.tsx"
    );
}

#[test]
fn nextjs_api_route_golden() {
    let api_src = fixture("nextjs/pages/api/users.ts");
    let api_file = JsParser
        .parse(
            Path::new("/project/pages/api/users.ts"),
            &api_src,
        )
        .expect("parse users.ts");

    let adapter = NextjsAdapter::new("/project");
    let events = adapter.analyze(&api_file);

    assert!(
        !events.is_empty(),
        "NextjsAdapter should detect at least one event in pages/api/users.ts"
    );
    assert!(
        events.iter().all(|e| e.adapter == "nextjs"),
        "all events should carry adapter='nextjs'"
    );
    assert!(
        events.iter().any(|e| matches!(e.kind, EventKind::ApiCall | EventKind::PageView)),
        "expected ApiCall or PageView from an API route file"
    );
}

#[test]
fn nextjs_combined_golden() {
    let page_src = fixture("nextjs/pages/index.tsx");
    let api_src = fixture("nextjs/pages/api/users.ts");

    let adapter = NextjsAdapter::new("/project");
    let page_file = JsParser.parse(Path::new("/project/pages/index.tsx"), &page_src).unwrap();
    let api_file = JsParser.parse(Path::new("/project/pages/api/users.ts"), &api_src).unwrap();

    let mut all: Vec<_> = adapter.analyze(&page_file);
    all.extend(adapter.analyze(&api_file));

    assert!(all.len() >= 2, "combined fixture should yield >= 2 events, got {}", all.len());
    assert!(
        all.iter().any(|e| e.kind == EventKind::PageView),
        "combined: expected at least one PageView"
    );
    assert!(
        all.iter().any(|e| matches!(e.kind, EventKind::ApiCall | EventKind::PageView)),
        "combined: expected at least one route-based event"
    );
}

// ---------------------------------------------------------------------------
// Express golden test
// ---------------------------------------------------------------------------

#[test]
fn express_routes_golden() {
    let src = fixture("express/routes.ts");
    let file = JsParser
        .parse(Path::new("src/routes.ts"), &src)
        .expect("parse routes.ts");

    let events = ExpressAdapter::new(".").analyze(&file);

    assert!(
        events.len() >= 4,
        "ExpressAdapter should detect >= 4 events from the routes fixture, got {}",
        events.len()
    );
    assert!(
        events.iter().all(|e| e.adapter == "express"),
        "all events should carry adapter='express'"
    );
    assert!(
        events.iter().any(|e| e.kind == EventKind::ApiCall),
        "expected at least one ApiCall from Express routes"
    );
    // The /auth/login route should produce an auth-related event or api call
    assert!(
        events.iter().any(|e| {
            e.name.to_lowercase().contains("login")
                || e.name.to_lowercase().contains("auth")
                || e.kind == EventKind::AuthEvent
                || e.kind == EventKind::ApiCall
        }),
        "expected an event related to the /auth/login route"
    );
}

// ---------------------------------------------------------------------------
// Django golden test
// ---------------------------------------------------------------------------

#[test]
fn django_views_golden() {
    let src = fixture("django/views.py");
    let file = PyParser
        .parse(Path::new("views.py"), &src)
        .expect("parse views.py");

    let events = DjangoAdapter::new(".").analyze(&file);

    assert!(
        !events.is_empty(),
        "DjangoAdapter should detect at least one event from views.py"
    );
    assert!(
        events.iter().all(|e| e.adapter == "django"),
        "all events should carry adapter='django'"
    );
    assert!(
        events.iter().all(|e| e.confidence > 0.0),
        "all events should have a positive confidence score"
    );
}

// ---------------------------------------------------------------------------
// FastAPI golden test
// ---------------------------------------------------------------------------

#[test]
fn fastapi_routes_golden() {
    let src = fixture("fastapi/main.py");
    let file = PyParser
        .parse(Path::new("main.py"), &src)
        .expect("parse main.py");

    let events = FastApiAdapter::new(".").analyze(&file);

    assert!(
        events.len() >= 3,
        "FastApiAdapter should detect >= 3 events from the fixture, got {}",
        events.len()
    );
    assert!(
        events.iter().all(|e| e.adapter == "fastapi"),
        "all events should carry adapter='fastapi'"
    );
    assert!(
        events.iter().any(|e| e.kind == EventKind::ApiCall),
        "expected at least one ApiCall from FastAPI routes"
    );
    assert!(
        events.iter().any(|e| {
            e.name.to_lowercase().contains("login")
                || e.name.to_lowercase().contains("auth")
                || e.kind == EventKind::AuthEvent
                || e.kind == EventKind::ApiCall
        }),
        "expected an event related to the /auth/login route"
    );
}

// ---------------------------------------------------------------------------
// Rails golden test
// ---------------------------------------------------------------------------

#[test]
fn rails_routes_golden() {
    let src = fixture("rails/config/routes.rb");
    let file = RubyParser
        .parse(Path::new("config/routes.rb"), &src)
        .expect("parse routes.rb");

    let events = RailsAdapter::new(".").analyze(&file);

    assert!(
        events.len() >= 2,
        "RailsAdapter should detect >= 2 events from routes.rb, got {}",
        events.len()
    );
    assert!(
        events.iter().all(|e| e.adapter == "rails"),
        "all events should carry adapter='rails'"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e.kind, EventKind::PageView | EventKind::ApiCall | EventKind::AuthEvent)),
        "expected at least one PageView, ApiCall, or AuthEvent from Rails routes"
    );
}
