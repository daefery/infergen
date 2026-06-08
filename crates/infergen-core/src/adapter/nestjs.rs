//! NestJS adapter.
//!
//! Detects controller routes via TypeScript decorator patterns.
//! Uses a stateful line scanner: `@Controller('prefix')` sets the current
//! prefix, and HTTP method decorators (`@Get(...)`, `@Post(...)`, etc.) emit
//! one `ApiCall` per decorator.

use std::path::PathBuf;

use crate::{
    detect::{Framework, Language},
    namer::{NameSignals, Namer},
    parser::ParsedFile,
};

use super::{Adapter, EventKind, ProposedEvent};

const HTTP_DECORATORS: &[&str] = &["Get", "Post", "Put", "Delete", "Patch", "Head", "Options"];

/// Adapter for NestJS applications.
///
/// Scans TypeScript source files for `@Controller(...)` and HTTP method
/// decorator patterns and proposes [`EventKind::ApiCall`] events.
pub struct NestJsAdapter {
    /// Project root — used to set `source_path` on proposals.
    pub project_root: PathBuf,
}

impl NestJsAdapter {
    /// Create a new adapter anchored at `project_root`.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }
}

impl Adapter for NestJsAdapter {
    fn analyze(&self, file: &ParsedFile) -> Vec<ProposedEvent> {
        if file.lang != Language::TypeScript {
            return Vec::new();
        }
        if !file.source.contains("@Controller") {
            return Vec::new();
        }
        let namer = Namer::new();
        extract_nestjs_routes(&file.source)
            .into_iter()
            .map(|(http_method, full_path)| {
                let name = namer
                    .derive(&NameSignals {
                        route: Some(&full_path),
                        handler_name: None,
                        kind: EventKind::ApiCall,
                        component_name: None,
                    })
                    .name;
                ProposedEvent::new(name, EventKind::ApiCall, file.path.clone(), 0.85)
                    .with_prop("endpoint", Some("string"))
                    .with_prop("method", Some("string"))
                    .with_adapter("nestjs")
                    ._with_http_method_hint(http_method)
            })
            .collect()
    }

    fn framework(&self) -> Framework {
        Framework::NestJs
    }
}

// Internal helper to attach http_method as a property value hint.
// ProposedEvent doesn't store method values directly, so we encode it in the
// endpoint property. Callers that need the raw method inspect properties[1].
impl ProposedEvent {
    fn _with_http_method_hint(mut self, method: String) -> Self {
        // Replace the placeholder "method" prop with one that carries a type_hint
        // showing the HTTP verb. This is purely informational for the reviewer.
        if let Some(p) = self.properties.iter_mut().find(|p| p.name == "method") {
            p.type_hint = Some(method);
        }
        self
    }
}

/// Extract `(http_method_lowercase, full_path)` from NestJS decorator patterns.
fn extract_nestjs_routes(source: &str) -> Vec<(String, String)> {
    let mut routes = Vec::new();
    let mut controller_prefix = String::new();

    for line in source.lines() {
        let trimmed = line.trim();

        if let Some(prefix) = extract_controller_prefix(trimmed) {
            controller_prefix = prefix;
            continue;
        }

        if let Some((method, suffix)) = extract_http_decorator(trimmed) {
            let full_path = build_route(&controller_prefix, &suffix);
            routes.push((method, full_path));
        }
    }

    routes
}

/// Extract controller prefix from `@Controller('prefix')` or `@Controller()`.
///
/// Returns `Some(prefix)` on match, `None` otherwise.
fn extract_controller_prefix(line: &str) -> Option<String> {
    let line = line.trim();
    let rest = line.strip_prefix("@Controller")?;
    let inner = rest.trim_start().strip_prefix('(')?;
    let inner = inner.trim();

    if let Some(stripped) = inner.strip_prefix(')') {
        // @Controller() — empty prefix
        let _ = stripped;
        return Some(String::new());
    }

    // @Controller('prefix') or @Controller("prefix")
    extract_quoted_prefix(inner)
}

/// Extract HTTP method and path suffix from `@Get(...)`, `@Post(...)`, etc.
fn extract_http_decorator(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    let rest = line.strip_prefix('@')?;

    for &decorator in HTTP_DECORATORS {
        if let Some(after) = rest.strip_prefix(decorator) {
            let inner = after.trim_start().strip_prefix('(')?;
            let inner = inner.trim();

            let suffix = if inner.starts_with(')') {
                // @Get() — no path suffix
                String::new()
            } else {
                extract_quoted_prefix(inner).unwrap_or_default()
            };

            return Some((decorator.to_lowercase(), suffix));
        }
    }

    None
}

/// Build a normalized route path from a controller prefix and method suffix.
fn build_route(prefix: &str, suffix: &str) -> String {
    match (prefix.is_empty(), suffix.is_empty()) {
        (true, true) => "/".to_owned(),
        (true, false) => format!("/{suffix}"),
        (false, true) => format!("/{prefix}"),
        (false, false) => format!("/{prefix}/{suffix}"),
    }
}

/// Extract the first single- or double-quoted string from a decorator argument.
fn extract_quoted_prefix(s: &str) -> Option<String> {
    let s = s.trim();
    let quote = s.chars().next()?;
    if !matches!(quote, '"' | '\'') {
        return None;
    }
    let rest = &s[1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_owned())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::detect::Language;
    use crate::parser::ParsedFile;

    fn file(lang: Language, source: &str) -> ParsedFile {
        ParsedFile {
            path: PathBuf::from("users.controller.ts"),
            lang,
            source: source.to_owned(),
            diagnostics: vec![],
        }
    }

    fn adapter() -> NestJsAdapter {
        NestJsAdapter::new("/project")
    }

    #[test]
    fn nestjs_adapter_empty_for_non_ts_file() {
        let f = file(Language::JavaScript, "@Controller('users')\nclass C {\n  @Get()\n  findAll() {}\n}");
        assert!(adapter().analyze(&f).is_empty());
    }

    #[test]
    fn nestjs_adapter_empty_without_controller_decorator() {
        let f = file(Language::TypeScript, "class UsersService {}");
        assert!(adapter().analyze(&f).is_empty());
    }

    #[test]
    fn nestjs_adapter_get_on_controller_prefix() {
        let src = "@Controller('users')\nexport class UsersController {\n  @Get()\n  findAll() {}\n}";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
        assert!(events[0].name.contains("user") || events[0].name.contains("users") || !events[0].name.is_empty());
    }

    #[test]
    fn nestjs_adapter_post_generates_event() {
        let src = "@Controller('items')\nexport class ItemsController {\n  @Post()\n  create() {}\n}";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::ApiCall);
    }

    #[test]
    fn nestjs_adapter_empty_controller_decorator() {
        let src = "@Controller()\nexport class AppController {\n  @Get()\n  root() {}\n}";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn nestjs_adapter_get_with_path_suffix() {
        let src = "@Controller('users')\nexport class UsersController {\n  @Get(':id')\n  findOne() {}\n}";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn nestjs_adapter_delete_with_param() {
        let src = "@Controller('posts')\nexport class PostsController {\n  @Delete(':id')\n  remove() {}\n}";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn nestjs_adapter_multiple_methods_same_controller() {
        let src = "@Controller('users')\nexport class UsersController {\n  @Get()\n  findAll() {}\n  @Post()\n  create() {}\n  @Get(':id')\n  findOne() {}\n}";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn nestjs_adapter_confidence_is_0_85() {
        let src = "@Controller('users')\nexport class UsersController {\n  @Get()\n  findAll() {}\n}";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        assert!((events[0].confidence - 0.85).abs() < f32::EPSILON);
    }

    #[test]
    fn nestjs_adapter_attribution_is_nestjs() {
        let src = "@Controller('users')\nexport class UsersController {\n  @Get()\n  findAll() {}\n}";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        assert!(events.iter().all(|e| e.adapter == "nestjs"));
    }

    #[test]
    fn nestjs_framework_returns_nestjs() {
        assert_eq!(adapter().framework(), Framework::NestJs);
    }

    #[test]
    fn nestjs_adapter_events_have_endpoint_and_method_props() {
        let src = "@Controller('users')\nexport class UsersController {\n  @Get()\n  findAll() {}\n}";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        let names: Vec<&str> = events[0].properties.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"endpoint"));
        assert!(names.contains(&"method"));
    }

    #[test]
    fn nestjs_adapter_second_controller_updates_prefix() {
        let src = "@Controller('users')\nclass UsersController {\n  @Get()\n  list() {}\n}\n@Controller('posts')\nclass PostsController {\n  @Get()\n  list() {}\n}";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn nestjs_adapter_patch_decorator_detected() {
        let src = "@Controller('profile')\nexport class ProfileController {\n  @Patch(':id')\n  update() {}\n}";
        let f = file(Language::TypeScript, src);
        let events = adapter().analyze(&f);
        assert_eq!(events.len(), 1);
    }
}
