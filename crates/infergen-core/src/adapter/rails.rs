//! Rails framework adapter.
//!
//! Detection strategy — three file roles:
//!
//! 1. **Routes file** (`config/routes.rb` or any `routes.rb`):
//!    - Explicit HTTP routes → `ApiCall` events (confidence 0.85).
//!    - `resources :name` → CRUD events per standard action (confidence 0.75).
//!    - `resource :name` → singular resource events, no `index` (confidence 0.75).
//!    - `devise_for :users` → auth events: signed_in / signed_out / registered
//!      (confidence 0.85).
//!
//! 2. **Controller files** (`*_controller.rb` or in `app/controllers/`):
//!    - CRUD action methods → `ApiCall` (confidence 0.70).
//!    - Other public methods → `ApiCall` (confidence 0.50).
//!
//! All proposals carry `adapter = "rails"`.

use std::path::{Path, PathBuf};

use crate::{
    detect::{Framework, Language},
    namer::{NameSignals, Namer},
    parser::{ParsedFile, ruby::RubyStmt},
    property,
};

use super::{Adapter, EventKind, ProposedEvent};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Standard CRUD actions Rails generates for `resources`.
const CRUD_ACTIONS: &[&str] = &["index", "show", "new", "create", "edit", "update", "destroy"];

/// Standard CRUD actions for singular `resource` (no `index`).
const SINGULAR_CRUD_ACTIONS: &[&str] = &["show", "new", "create", "edit", "update", "destroy"];

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Rails framework adapter.
pub struct RailsAdapter {
    /// Absolute path to the Rails project root.
    pub project_root: PathBuf,
}

impl RailsAdapter {
    /// Create a new adapter rooted at `project_root`.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self { project_root: project_root.into() }
    }
}

impl Adapter for RailsAdapter {
    fn analyze(&self, file: &ParsedFile) -> Vec<ProposedEvent> {
        if file.lang != Language::Ruby {
            return Vec::new();
        }

        let rel = file.path.strip_prefix(&self.project_root).unwrap_or(&file.path);
        let is_routes = is_routes_file(rel);
        let is_controller = is_controller_file(rel);

        if !is_routes && !is_controller {
            return Vec::new();
        }

        let namer = Namer::new();
        let source_path = file.path.clone();

        file.with_ruby_stmts(|stmts| {
            let mut events = Vec::new();
            if is_routes {
                analyze_routes(stmts, &source_path, &namer, &mut events);
            }
            if is_controller {
                analyze_controller(stmts, &source_path, &namer, &mut events);
            }
            events
        })
    }

    fn framework(&self) -> Framework {
        Framework::Rails
    }
}

// ---------------------------------------------------------------------------
// Routes analysis
// ---------------------------------------------------------------------------

fn analyze_routes(
    stmts: &[RubyStmt],
    source_path: &Path,
    namer: &Namer,
    events: &mut Vec<ProposedEvent>,
) {
    for stmt in stmts {
        match stmt {
            RubyStmt::Route { method, path, controller_action } => {
                let action_part = controller_action
                    .split('#')
                    .nth(1)
                    .unwrap_or("")
                    .to_owned();
                let handler: Option<&str> = if action_part.is_empty() {
                    None
                } else {
                    Some(&action_part)
                };
                let result = namer.derive(&NameSignals {
                    route: Some(path.as_str()),
                    handler_name: handler,
                    kind: EventKind::ApiCall,
                    component_name: None,
                });
                let mut event = ProposedEvent::new(
                    result.name,
                    EventKind::ApiCall,
                    source_path,
                    0.85,
                )
                .with_prop("endpoint", Some("string"))
                .with_prop("method", Some("string"))
                .with_adapter("rails");
                event.properties = property::enrich_hints(event.properties);
                let _ = method;
                events.push(event);
            }

            RubyStmt::Resources { name } => {
                for action in CRUD_ACTIONS {
                    let path = if matches!(*action, "show" | "edit" | "update" | "destroy") {
                        format!("/{}/:id", name)
                    } else {
                        format!("/{}", name)
                    };
                    let result = namer.derive(&NameSignals {
                        route: Some(&path),
                        handler_name: Some(action),
                        kind: EventKind::ApiCall,
                        component_name: None,
                    });
                    let mut event = ProposedEvent::new(
                        result.name,
                        EventKind::ApiCall,
                        source_path,
                        0.75,
                    )
                    .with_prop("endpoint", Some("string"))
                    .with_prop("method", Some("string"))
                    .with_adapter("rails");
                    event.properties = property::enrich_hints(event.properties);
                    events.push(event);
                }
            }

            RubyStmt::Resource { name } => {
                for action in SINGULAR_CRUD_ACTIONS {
                    let path = if matches!(*action, "show" | "edit" | "update" | "destroy") {
                        format!("/{}", name)
                    } else {
                        format!("/{}/new", name)
                    };
                    let result = namer.derive(&NameSignals {
                        route: Some(&path),
                        handler_name: Some(action),
                        kind: EventKind::ApiCall,
                        component_name: None,
                    });
                    let mut event = ProposedEvent::new(
                        result.name,
                        EventKind::ApiCall,
                        source_path,
                        0.75,
                    )
                    .with_prop("endpoint", Some("string"))
                    .with_prop("method", Some("string"))
                    .with_adapter("rails");
                    event.properties = property::enrich_hints(event.properties);
                    events.push(event);
                }
            }

            RubyStmt::DeviseFor { resource } => {
                // Emit three canonical Devise auth events.
                for (action, kind) in [
                    ("signed_in", EventKind::AuthEvent),
                    ("signed_out", EventKind::AuthEvent),
                    ("registered", EventKind::AuthEvent),
                ] {
                    // Strip trailing `s` from resource name for singular form:
                    // "users" → "user". Naive but sufficient for common cases.
                    let singular = resource.trim_end_matches('s');
                    let name = format!("{}_{}", singular, action);
                    let mut event = ProposedEvent::new(name, kind, source_path, 0.85)
                        .with_adapter("rails");
                    if matches!(action, "signed_in" | "registered") {
                        event = event.with_prop("method", Some("string"));
                        event.properties = property::enrich_hints(event.properties);
                    }
                    events.push(event);
                }
            }

            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Controller analysis
// ---------------------------------------------------------------------------

fn analyze_controller(
    stmts: &[RubyStmt],
    source_path: &Path,
    namer: &Namer,
    events: &mut Vec<ProposedEvent>,
) {
    let mut current_resource: Option<String> = None;

    for stmt in stmts {
        match stmt {
            RubyStmt::ClassDef { name, .. } => {
                // Extract resource name from controller class: "UsersController" → "users"
                if name.ends_with("Controller") {
                    let resource = name
                        .trim_end_matches("Controller")
                        .to_lowercase();
                    current_resource = Some(resource);
                }
            }

            RubyStmt::MethodDef { name } => {
                let resource = current_resource.as_deref().unwrap_or("");
                let confidence = if CRUD_ACTIONS.contains(&name.as_str()) { 0.70 } else { 0.50 };
                let result = namer.derive(&NameSignals {
                    route: None,
                    handler_name: Some(name.as_str()),
                    kind: EventKind::ApiCall,
                    component_name: if resource.is_empty() { None } else { Some(resource) },
                });
                let mut event = ProposedEvent::new(
                    result.name,
                    EventKind::ApiCall,
                    source_path,
                    confidence,
                )
                .with_adapter("rails");
                event.properties = property::enrich_hints(event.properties);
                events.push(event);
            }

            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Path classification helpers
// ---------------------------------------------------------------------------

fn is_routes_file(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("routes") && path.extension().and_then(|e| e.to_str()) == Some("rb")
}

fn is_controller_file(path: &Path) -> bool {
    let s = path.to_string_lossy();
    (s.contains("controller") || s.contains("controllers/"))
        && path.extension().and_then(|e| e.to_str()) == Some("rb")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{parser::LanguageParser, parser::ruby::RubyParser};

    fn adapter() -> RailsAdapter {
        RailsAdapter::new(PathBuf::from("/proj"))
    }

    fn parse_ruby(rel: &str, src: &str) -> ParsedFile {
        RubyParser
            .parse(&PathBuf::from("/proj").join(rel), src)
            .unwrap()
    }

    fn non_ruby_file() -> ParsedFile {
        use crate::{JsParser, parser::LanguageParser as _};
        JsParser.parse(&PathBuf::from("/proj/app.ts"), "const x = 1;").unwrap()
    }

    #[test]
    fn rails_adapter_empty_for_non_ruby_file() {
        let a = adapter();
        assert!(a.analyze(&non_ruby_file()).is_empty());
    }

    #[test]
    fn rails_adapter_empty_for_non_routes_non_controller_rb() {
        let a = adapter();
        let f = parse_ruby("models/user.rb", "class User < ApplicationRecord\nend\n");
        assert!(a.analyze(&f).is_empty());
    }

    #[test]
    fn rails_adapter_detects_get_route_in_routes_file() {
        let a = adapter();
        let f = parse_ruby(
            "config/routes.rb",
            "get \"/users\", to: \"users#index\"\n",
        );
        let events = a.analyze(&f);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::ApiCall);
    }

    #[test]
    fn rails_adapter_route_event_has_endpoint_method_props() {
        let a = adapter();
        let f = parse_ruby(
            "config/routes.rb",
            "get \"/users\", to: \"users#index\"\n",
        );
        let events = a.analyze(&f);
        assert_eq!(events.len(), 1);
        let prop_names: Vec<&str> =
            events[0].properties.iter().map(|p| p.name.as_str()).collect();
        assert!(prop_names.contains(&"endpoint"), "props: {:?}", prop_names);
        assert!(prop_names.contains(&"method"), "props: {:?}", prop_names);
    }

    #[test]
    fn rails_adapter_resources_generates_crud_events() {
        let a = adapter();
        let f = parse_ruby("config/routes.rb", "resources :posts\n");
        let events = a.analyze(&f);
        assert_eq!(events.len(), CRUD_ACTIONS.len());
    }

    #[test]
    fn rails_adapter_resource_singular_generates_4_events() {
        let a = adapter();
        let f = parse_ruby("config/routes.rb", "resource :profile\n");
        let events = a.analyze(&f);
        assert_eq!(events.len(), SINGULAR_CRUD_ACTIONS.len());
    }

    #[test]
    fn rails_adapter_devise_for_generates_auth_events() {
        let a = adapter();
        let f = parse_ruby("config/routes.rb", "devise_for :users\n");
        let events = a.analyze(&f);
        assert_eq!(events.len(), 3);
        for e in &events {
            assert_eq!(e.kind, EventKind::AuthEvent);
        }
    }

    #[test]
    fn rails_adapter_devise_signed_in_has_method_prop() {
        let a = adapter();
        let f = parse_ruby("config/routes.rb", "devise_for :users\n");
        let events = a.analyze(&f);
        // signed_in event should have a method prop
        let signed_in = events.iter().find(|e| e.name.contains("signed_in")).unwrap();
        let has_method = signed_in.properties.iter().any(|p| p.name == "method");
        assert!(has_method, "signed_in event missing method prop");
    }

    #[test]
    fn rails_adapter_controller_crud_actions() {
        let a = adapter();
        let src = "class UsersController < ApplicationController\n  def index\n  end\n  def show\n  end\n  def create\n  end\nend\n";
        let f = parse_ruby("app/controllers/users_controller.rb", src);
        let events = a.analyze(&f);
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn rails_adapter_controller_custom_action_lower_confidence() {
        let a = adapter();
        let src =
            "class UsersController < ApplicationController\n  def dashboard\n  end\nend\n";
        let f = parse_ruby("app/controllers/users_controller.rb", src);
        let events = a.analyze(&f);
        assert_eq!(events.len(), 1);
        assert!((events[0].confidence - 0.50_f32).abs() < f32::EPSILON);
    }

    #[test]
    fn rails_adapter_controller_crud_action_confidence() {
        let a = adapter();
        let src =
            "class UsersController < ApplicationController\n  def index\n  end\nend\n";
        let f = parse_ruby("app/controllers/users_controller.rb", src);
        let events = a.analyze(&f);
        assert_eq!(events.len(), 1);
        assert!((events[0].confidence - 0.70_f32).abs() < f32::EPSILON);
    }

    #[test]
    fn rails_adapter_all_events_carry_rails_attribution() {
        let a = adapter();
        let f = parse_ruby(
            "config/routes.rb",
            "get \"/ping\", to: \"health#ping\"\nresources :posts\n",
        );
        for event in a.analyze(&f) {
            assert_eq!(event.adapter, "rails", "event {} missing rails attribution", event.name);
        }
    }

    #[test]
    fn rails_framework_returns_rails() {
        assert_eq!(adapter().framework(), Framework::Rails);
    }

    #[test]
    fn rails_adapter_route_confidence_0_85() {
        let a = adapter();
        let f = parse_ruby("config/routes.rb", "get \"/users\", to: \"users#index\"\n");
        let events = a.analyze(&f);
        assert!((events[0].confidence - 0.85_f32).abs() < f32::EPSILON);
    }

    #[test]
    fn rails_adapter_resources_confidence_0_75() {
        let a = adapter();
        let f = parse_ruby("config/routes.rb", "resources :posts\n");
        for e in a.analyze(&f) {
            assert!((e.confidence - 0.75_f32).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn rails_adapter_post_route_detected() {
        let a = adapter();
        let f = parse_ruby("config/routes.rb", "post \"/users\", to: \"users#create\"\n");
        let events = a.analyze(&f);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn rails_adapter_delete_route_detected() {
        let a = adapter();
        let f = parse_ruby(
            "config/routes.rb",
            "delete \"/users/:id\", to: \"users#destroy\"\n",
        );
        let events = a.analyze(&f);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn rails_adapter_arrow_syntax_route() {
        let a = adapter();
        let f = parse_ruby("config/routes.rb", "get \"/about\" => \"pages#about\"\n");
        let events = a.analyze(&f);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn rails_adapter_private_methods_skipped() {
        let a = adapter();
        let src =
            "class UsersController < ApplicationController\n  def _helper\n  end\nend\n";
        let f = parse_ruby("app/controllers/users_controller.rb", src);
        let events = a.analyze(&f);
        assert!(events.is_empty());
    }

    #[test]
    fn rails_adapter_mixed_routes_and_devise() {
        let a = adapter();
        let src = "get \"/home\", to: \"pages#home\"\ndevise_for :users\n";
        let f = parse_ruby("config/routes.rb", src);
        let events = a.analyze(&f);
        // 1 route + 3 devise
        assert_eq!(events.len(), 4);
    }
}
