//! Integration tests for E5.4: Additional JS Framework Adapters.
//!
//! Tests ExpressAdapter, NestJsAdapter, ReactRouterAdapter, SvelteKitAdapter,
//! VueAdapter, and the VueParser/SvelteParser passthrough parsers.

use std::path::PathBuf;

use infergen_core::{
    Adapter, EventKind, ExpressAdapter, LanguageParser, NestJsAdapter, ParsedFile,
    ReactRouterAdapter, SvelteKitAdapter, SvelteParser, VueAdapter, VueParser,
    detect::Language,
};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn ts(path: &str, source: &str) -> ParsedFile {
    ParsedFile {
        path: PathBuf::from(path),
        lang: Language::TypeScript,
        source: source.to_owned(),
        diagnostics: vec![],
    }
}

fn js(path: &str, source: &str) -> ParsedFile {
    ParsedFile {
        path: PathBuf::from(path),
        lang: Language::JavaScript,
        source: source.to_owned(),
        diagnostics: vec![],
    }
}

fn svelte(path: &str) -> ParsedFile {
    ParsedFile {
        path: PathBuf::from(path),
        lang: Language::Svelte,
        source: String::new(),
        diagnostics: vec![],
    }
}

fn vue(path: &str) -> ParsedFile {
    ParsedFile {
        path: PathBuf::from(path),
        lang: Language::Vue,
        source: String::new(),
        diagnostics: vec![],
    }
}

// ---------------------------------------------------------------------------
// ExpressAdapter — integration
// ---------------------------------------------------------------------------

#[test]
fn express_adapter_full_route_file() {
    let src = r#"
import express from 'express';
const app = express();

app.get('/users', listUsers);
app.post('/users', createUser);
app.delete('/users/:id', deleteUser);
"#;
    let f = ts("src/server.ts", src);
    let events = ExpressAdapter::new("/project").analyze(&f);
    assert_eq!(events.len(), 3);
    assert!(events.iter().all(|e| e.kind == EventKind::ApiCall));
}

#[test]
fn express_adapter_attribution_integration() {
    let src = "import express from 'express';\napp.get('/health', healthCheck);\n";
    let f = ts("server.ts", src);
    let events = ExpressAdapter::new("/project").analyze(&f);
    assert!(events.iter().all(|e| e.adapter == "express"));
}

// ---------------------------------------------------------------------------
// NestJsAdapter — integration
// ---------------------------------------------------------------------------

#[test]
fn nestjs_adapter_controller_with_crud_routes() {
    let src = r#"
import { Controller, Get, Post } from '@nestjs/common';

@Controller('users')
export class UsersController {
  @Get()
  findAll() { return []; }

  @Post()
  create() { return {}; }

  @Get(':id')
  findOne() { return {}; }
}
"#;
    let f = ts("src/users/users.controller.ts", src);
    let events = NestJsAdapter::new("/project").analyze(&f);
    assert_eq!(events.len(), 3);
    assert!(events.iter().all(|e| e.kind == EventKind::ApiCall));
    assert!(events.iter().all(|e| e.adapter == "nestjs"));
}

#[test]
fn nestjs_adapter_two_controllers_same_file() {
    let src = r#"
@Controller('users')
export class UsersController {
  @Get()
  list() {}
}

@Controller('posts')
export class PostsController {
  @Get()
  list() {}
  @Post()
  create() {}
}
"#;
    let f = ts("src/app.controller.ts", src);
    let events = NestJsAdapter::new("/project").analyze(&f);
    assert_eq!(events.len(), 3);
}

// ---------------------------------------------------------------------------
// ReactRouterAdapter — integration
// ---------------------------------------------------------------------------

#[test]
fn react_router_adapter_full_app_routes() {
    let src = r#"
import { BrowserRouter, Routes, Route } from 'react-router-dom';

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Home />} />
        <Route path="/about" element={<About />} />
        <Route path="/users/:id" element={<UserDetail />} />
      </Routes>
    </BrowserRouter>
  );
}
"#;
    let f = ts("src/App.tsx", src);
    let events = ReactRouterAdapter::new("/project").analyze(&f);
    assert_eq!(events.len(), 3);
    assert!(events.iter().all(|e| e.kind == EventKind::PageView));
}

#[test]
fn react_router_adapter_config_object_routes() {
    let src = r#"
import { createBrowserRouter } from 'react-router-dom';

const router = createBrowserRouter([
  { path: '/', element: <Root /> },
  { path: '/dashboard', element: <Dashboard /> },
]);
"#;
    let f = ts("src/router.ts", src);
    let events = ReactRouterAdapter::new("/project").analyze(&f);
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|e| e.kind == EventKind::PageView));
}

// ---------------------------------------------------------------------------
// SvelteKitAdapter — integration
// ---------------------------------------------------------------------------

#[test]
fn sveltekit_adapter_mixed_pages_and_api() {
    let adapter = SvelteKitAdapter::new("");

    let root = svelte("src/routes/+page.svelte");
    let about = svelte("src/routes/about/+page.svelte");
    let api = ts("src/routes/api/users/+server.ts", "");

    let mut events = Vec::new();
    events.extend(adapter.analyze(&root));
    events.extend(adapter.analyze(&about));
    events.extend(adapter.analyze(&api));

    let page_views: Vec<_> = events.iter().filter(|e| e.kind == EventKind::PageView).collect();
    let api_calls: Vec<_> = events.iter().filter(|e| e.kind == EventKind::ApiCall).collect();
    assert_eq!(page_views.len(), 2);
    assert_eq!(api_calls.len(), 1);
}

#[test]
fn sveltekit_adapter_route_from_path_helper() {
    use infergen_core::adapter::svelte_kit::route_from_sveltekit_path;
    use std::path::Path;

    let (route, kind) = route_from_sveltekit_path(Path::new("src/routes/about/+page.svelte")).unwrap();
    assert_eq!(route, "/about");
    assert_eq!(kind, EventKind::PageView);

    let (route, kind) = route_from_sveltekit_path(Path::new("src/routes/api/items/+server.ts")).unwrap();
    assert_eq!(route, "/api/items");
    assert_eq!(kind, EventKind::ApiCall);

    let result = route_from_sveltekit_path(Path::new("src/routes/+layout.svelte"));
    assert!(result.is_none());
}

// ---------------------------------------------------------------------------
// VueAdapter — integration
// ---------------------------------------------------------------------------

#[test]
fn vue_adapter_nuxt_pages_directory() {
    let adapter = VueAdapter::new("");

    let index = vue("pages/index.vue");
    let about = vue("pages/about.vue");
    let user = vue("pages/users/[id].vue");

    let mut events = Vec::new();
    events.extend(adapter.analyze(&index));
    events.extend(adapter.analyze(&about));
    events.extend(adapter.analyze(&user));

    assert_eq!(events.len(), 3);
    assert!(events.iter().all(|e| e.kind == EventKind::PageView));
    assert!(events.iter().all(|e| e.adapter == "vue"));
}

#[test]
fn vue_adapter_router_config_integration() {
    let src = r#"
import { createRouter, createWebHistory } from 'vue-router';

const routes = [
  { path: '/', component: Home },
  { path: '/about', component: About },
  { path: '/contact', component: Contact },
];

const router = createRouter({ history: createWebHistory(), routes });
"#;
    let f = ts("src/router/index.ts", src);
    let events = VueAdapter::new("/project").analyze(&f);
    assert_eq!(events.len(), 3);
}

// ---------------------------------------------------------------------------
// detect.rs — Language::Vue / Language::Svelte integration
// ---------------------------------------------------------------------------

#[test]
fn detect_returns_vue_language_for_vue_project() {
    use infergen_core::detect::{Language, detect};
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"dependencies":{"vue":"3"}}"#,
    )
    .unwrap();
    let result = detect(dir.path()).unwrap();
    assert!(result.languages.contains(&Language::Vue));
}

#[test]
fn detect_returns_svelte_language_for_sveltekit_project() {
    use infergen_core::detect::{Language, detect};
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"devDependencies":{"@sveltejs/kit":"1"}}"#,
    )
    .unwrap();
    let result = detect(dir.path()).unwrap();
    assert!(result.languages.contains(&Language::Svelte));
}

// ---------------------------------------------------------------------------
// VueParser / SvelteParser — passthrough roundtrip
// ---------------------------------------------------------------------------

#[test]
fn vue_parser_roundtrips_source() {
    let src = "<template><h1>Hello</h1></template>";
    let f = VueParser.parse(&PathBuf::from("page.vue"), src).unwrap();
    assert_eq!(f.source, src);
    assert_eq!(f.lang, Language::Vue);
}

#[test]
fn svelte_parser_roundtrips_source() {
    let src = "<script>\n  let x = 0;\n</script>\n<p>{x}</p>";
    let f = SvelteParser
        .parse(&PathBuf::from("+page.svelte"), src)
        .unwrap();
    assert_eq!(f.source, src);
    assert_eq!(f.lang, Language::Svelte);
}
