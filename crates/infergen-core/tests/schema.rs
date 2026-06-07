//! Integration tests for E3.2b SQL schema generation.

use infergen_core::{
    Catalog, CatalogEntry, CatalogEventKind, EventProperty, EventProvenance, EventStatus,
    SqlDialect, generate_sql_schema,
};
use infergen_types::CATALOG_SCHEMA_VERSION;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
        kind: CatalogEventKind::PageView,
        provenance: vec![EventProvenance {
            source_path: "src/index.tsx".into(),
            line: None,
            adapter: "nextjs".into(),
        }],
        properties: Vec::new(),
        providers: Vec::new(),
    }
}

fn make_prop(name: &str, t: Option<&str>, pii: bool) -> EventProperty {
    EventProperty { name: name.into(), prop_type: t.map(Into::into), required: false, pii }
}

// ---------------------------------------------------------------------------
// Integration tests
// ---------------------------------------------------------------------------

#[test]
fn full_pipeline_postgres_schema_has_events_table() {
    let sql = generate_sql_schema(&Catalog::default(), SqlDialect::Postgres);
    assert!(
        sql.contains(r#"CREATE TABLE IF NOT EXISTS "infergen_events""#),
        "Postgres events table missing\noutput:\n{sql}"
    );
    assert!(sql.contains("JSONB"), "Postgres JSONB column missing\noutput:\n{sql}");
}

#[test]
fn full_pipeline_mysql_schema_has_events_table() {
    let sql = generate_sql_schema(&Catalog::default(), SqlDialect::Mysql);
    assert!(
        sql.contains("CREATE TABLE IF NOT EXISTS `infergen_events`"),
        "MySQL events table missing\noutput:\n{sql}"
    );
    assert!(sql.contains("JSON"), "MySQL JSON column missing\noutput:\n{sql}");
    assert!(!sql.contains("JSONB"), "MySQL should not use JSONB\noutput:\n{sql}");
}

#[test]
fn full_pipeline_sqlite_schema_has_events_table() {
    let sql = generate_sql_schema(&Catalog::default(), SqlDialect::Sqlite);
    assert!(
        sql.contains(r#"CREATE TABLE IF NOT EXISTS "infergen_events""#),
        "SQLite events table missing\noutput:\n{sql}"
    );
    assert!(sql.contains("AUTOINCREMENT"), "AUTOINCREMENT missing\noutput:\n{sql}");
    assert!(sql.contains("properties TEXT"), "SQLite TEXT column missing\noutput:\n{sql}");
}

#[test]
fn full_pipeline_empty_catalog_no_views() {
    for dialect in [SqlDialect::Postgres, SqlDialect::Mysql, SqlDialect::Sqlite] {
        let sql = generate_sql_schema(&Catalog::default(), dialect);
        assert!(
            !sql.contains("CREATE VIEW") && !sql.contains("CREATE OR REPLACE VIEW"),
            "unexpected view for empty catalog ({:?})\noutput:\n{sql}", dialect
        );
    }
}

#[test]
fn full_pipeline_approved_event_no_props_no_view() {
    let cat = make_catalog(vec![make_entry("page_viewed", EventStatus::Approved)]);
    let sql = generate_sql_schema(&cat, SqlDialect::Postgres);
    assert!(
        !sql.contains("CREATE OR REPLACE VIEW"),
        "no-props event should not have a view\noutput:\n{sql}"
    );
}

#[test]
fn full_pipeline_approved_event_with_props_has_view() {
    let mut entry = make_entry("page_viewed", EventStatus::Approved);
    entry.properties.push(make_prop("route", Some("string"), false));
    let cat = make_catalog(vec![entry]);
    let sql = generate_sql_schema(&cat, SqlDialect::Postgres);
    assert!(
        sql.contains(r#"CREATE OR REPLACE VIEW "page_viewed""#),
        "view missing\noutput:\n{sql}"
    );
    assert!(
        sql.contains("properties ->> 'route'"),
        "JSON extraction missing\noutput:\n{sql}"
    );
}

#[test]
fn full_pipeline_postgres_view_uses_double_arrow() {
    let mut entry = make_entry("api_called", EventStatus::Approved);
    entry.properties.push(make_prop("method", Some("string"), false));
    let cat = make_catalog(vec![entry]);
    let sql = generate_sql_schema(&cat, SqlDialect::Postgres);
    assert!(
        sql.contains("properties ->> 'method'"),
        "Postgres ->> extraction missing\noutput:\n{sql}"
    );
}

#[test]
fn full_pipeline_mysql_view_uses_json_unquote_extract() {
    let mut entry = make_entry("api_called", EventStatus::Approved);
    entry.properties.push(make_prop("method", Some("string"), false));
    let cat = make_catalog(vec![entry]);
    let sql = generate_sql_schema(&cat, SqlDialect::Mysql);
    assert!(
        sql.contains("JSON_UNQUOTE(JSON_EXTRACT"),
        "MySQL JSON_UNQUOTE/JSON_EXTRACT missing\noutput:\n{sql}"
    );
}

#[test]
fn full_pipeline_sqlite_view_uses_json_extract_fn() {
    let mut entry = make_entry("api_called", EventStatus::Approved);
    entry.properties.push(make_prop("method", Some("string"), false));
    let cat = make_catalog(vec![entry]);
    let sql = generate_sql_schema(&cat, SqlDialect::Sqlite);
    assert!(
        sql.contains("CREATE VIEW IF NOT EXISTS"),
        "SQLite CREATE VIEW IF NOT EXISTS missing\noutput:\n{sql}"
    );
    assert!(
        sql.contains("json_extract(properties, '$.method')"),
        "SQLite json_extract missing\noutput:\n{sql}"
    );
}

#[test]
fn full_pipeline_pii_prop_has_pii_comment_in_view() {
    let mut entry = make_entry("user_signed_in", EventStatus::Approved);
    entry.properties.push(make_prop("email", Some("string"), true));
    let cat = make_catalog(vec![entry]);
    let sql = generate_sql_schema(&cat, SqlDialect::Postgres);
    assert!(sql.contains("-- @pii"), "PII comment missing\noutput:\n{sql}");
}

#[test]
fn full_pipeline_ignored_event_not_in_schema() {
    let mut entry = make_entry("noise_event", EventStatus::Ignored);
    entry.properties.push(make_prop("route", None, false));
    let cat = make_catalog(vec![entry]);
    let sql = generate_sql_schema(&cat, SqlDialect::Postgres);
    assert!(!sql.contains("noise_event"), "ignored event in schema\noutput:\n{sql}");
}

#[test]
fn full_pipeline_views_alphabetically_ordered() {
    let mut z = make_entry("zebra_event", EventStatus::Approved);
    z.properties.push(make_prop("x", None, false));
    let mut a = make_entry("alpha_event", EventStatus::Approved);
    a.properties.push(make_prop("x", None, false));
    let cat = make_catalog(vec![z, a]);
    let sql = generate_sql_schema(&cat, SqlDialect::Postgres);
    let alpha_pos = sql.find("alpha_event").unwrap();
    let zebra_pos = sql.find("zebra_event").unwrap();
    assert!(
        alpha_pos < zebra_pos,
        "views not in alphabetical order\noutput:\n{sql}"
    );
}
