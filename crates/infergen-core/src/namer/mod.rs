//! Heuristic event namer (E1.2).
//!
//! Derives canonical snake_case event names and confidence scores from raw
//! code identifiers: component names, route paths, handler function names.
//! The default convention is `{entity}_{action}` in snake_case.
//! Configurable conventions ship in E1.3.

use crate::adapter::EventKind;

// ---------------------------------------------------------------------------
// Vocabulary constants
// ---------------------------------------------------------------------------

/// Handler function name prefixes that carry no entity information.
const HANDLER_PREFIXES: &[&str] = &["handle", "on", "do"];

/// Words that indicate an action (verb) or generic UI-component type word,
/// rather than a domain entity (noun). Stripped from identifier tokens when
/// deriving the entity component of an event name.
const ACTION_WORDS: &[&str] = &[
    "submit",
    "submitted",
    "click",
    "clicked",
    "view",
    "viewed",
    "load",
    "loaded",
    "error",
    "errored",
    "page",
    "form",   // UI component type — stripping yields domain entity ("checkout")
    "sign",
    "signed",
    "login",
    "logout",
    "signup",
    "signin",
    "signout",
    "register",
    "registered",
    "auth",
    "authenticate",
    "in",
    "out",
    "up",
];

/// HTTP method names (lowercase) recognised for API call action derivation.
const HTTP_METHODS: &[&str] = &["get", "post", "put", "delete", "patch", "head", "options"];

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Input signals for event name derivation.
///
/// All string fields accept any identifier format — camelCase, PascalCase,
/// snake_case, kebab-case, or HTTP verb. [`split_identifier`] normalises them.
#[derive(Debug, Clone)]
pub struct NameSignals<'a> {
    /// PascalCase/camelCase React/Vue component name, e.g. `"UserProfilePage"`.
    pub component_name: Option<&'a str>,
    /// Route path, e.g. `"/blog/[slug]"` or `"/api/users"`.
    pub route: Option<&'a str>,
    /// Handler function name, e.g. `"handleSubmit"`, `"signIn"`, `"GET"`.
    pub handler_name: Option<&'a str>,
    /// Broad event category (used for action defaults and entity fallbacks).
    pub kind: EventKind,
}

/// Output of [`Namer::derive`].
#[derive(Debug, Clone, PartialEq)]
pub struct NameResult {
    /// Derived event name in snake_case, e.g. `"about_page_viewed"`.
    pub name: String,
    /// Naming confidence `0.0`–`1.0` reflecting signal richness.
    ///
    /// Route signals yield ~0.9, component signals ~0.85, handler signals
    /// ~0.75, kind-only fallback ~0.5.
    pub confidence: f32,
}

/// Stateless heuristic event namer.
///
/// Derives `{entity}_{action}` names in snake_case from identifier signals.
/// The instance carries no state; calling [`Namer::new`] is free (ZST).
#[derive(Debug, Default, Clone, Copy)]
pub struct Namer;

// ---------------------------------------------------------------------------
// Namer implementation
// ---------------------------------------------------------------------------

impl Namer {
    /// Create a new stateless namer instance. Free: `Namer` is a zero-sized type.
    pub fn new() -> Self {
        Self
    }

    /// Derive a canonical event name and confidence from `signals`.
    ///
    /// Signal priority for entity: route > component_name > handler_name > kind fallback.
    /// Signal priority for action: handler_name > kind default.
    pub fn derive(&self, signals: &NameSignals<'_>) -> NameResult {
        let (entity, entity_conf) = self.derive_entity(signals);
        let (action, action_conf) = self.derive_action(signals);

        let name = if entity.is_empty() {
            action.clone()
        } else {
            format!("{entity}_{action}")
        };

        let confidence = (entity_conf * 0.6 + action_conf * 0.4).min(1.0);

        NameResult { name, confidence }
    }

    fn derive_entity(&self, signals: &NameSignals<'_>) -> (String, f32) {
        // Route: most reliable entity signal.
        if let Some(route) = signals.route {
            return (route_to_name_prefix(route), 0.9);
        }

        // Component name: strip known action words, keep noun tokens.
        if let Some(comp) = signals.component_name {
            let tokens = split_identifier(comp);
            let entity_tokens: Vec<&str> = tokens
                .iter()
                .map(String::as_str)
                .filter(|t| !ACTION_WORDS.contains(t))
                .collect();
            if !entity_tokens.is_empty() {
                return (entity_tokens.join("_"), 0.85);
            }
        }

        // Handler name: strip handler prefixes AND action words to isolate entity.
        if let Some(handler) = signals.handler_name {
            let tokens = split_identifier(handler);
            let lc: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();
            let entity_tokens: Vec<&str> = lc
                .iter()
                .map(String::as_str)
                .filter(|t| !HANDLER_PREFIXES.contains(t) && !ACTION_WORDS.contains(t))
                .collect();
            if !entity_tokens.is_empty() {
                return (entity_tokens.join("_"), 0.7);
            }
        }

        // Kind-based fallback.
        let fallback = match signals.kind {
            EventKind::PageView => "page",
            EventKind::FormSubmit => "form",
            EventKind::AuthEvent => "user",
            EventKind::ApiCall => "api",
            EventKind::Error => "error",
        };
        (fallback.to_owned(), 0.5)
    }

    fn derive_action(&self, signals: &NameSignals<'_>) -> (String, f32) {
        // Try to detect action from handler name.
        if let Some(handler) = signals.handler_name {
            let tokens = split_identifier(handler);
            let lc: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();
            let lc_refs: Vec<&str> = lc.iter().map(String::as_str).collect();

            if let Some(action) = detect_action_from_tokens(&lc_refs, signals.kind) {
                return (action, 0.9);
            }
        }

        // Kind-default action.
        let action = match signals.kind {
            EventKind::PageView => "page_viewed",
            EventKind::FormSubmit => "submitted",
            EventKind::AuthEvent => "triggered",
            EventKind::ApiCall => "api_called",
            EventKind::Error => "errored",
        };
        (action.to_owned(), 0.75)
    }
}

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

/// Split any code identifier into lowercase tokens.
///
/// Handles camelCase, PascalCase, snake_case, kebab-case, and all-uppercase
/// (e.g. HTTP methods). Empty input returns an empty vec.
///
/// # Examples
/// ```
/// use infergen_core::namer::split_identifier;
/// assert_eq!(split_identifier("handleSubmit"), vec!["handle", "submit"]);
/// assert_eq!(split_identifier("UserProfile"),  vec!["user", "profile"]);
/// assert_eq!(split_identifier("GET"),          vec!["get"]);
/// assert_eq!(split_identifier("user_signed_in"), vec!["user", "signed", "in"]);
/// ```
pub fn split_identifier(s: &str) -> Vec<String> {
    if s.is_empty() {
        return Vec::new();
    }
    s.split(['_', '-'])
        .filter(|p| !p.is_empty())
        .flat_map(split_camel_case)
        .collect()
}

/// Convert a route path to a snake_case name prefix.
///
/// - `"/"` → `"home"`
/// - `"/about"` → `"about"`
/// - `"/blog/[slug]"` → `"blog_slug"`
/// - `"/api/users"` → `"api_users"`
pub fn route_to_name_prefix(route: &str) -> String {
    let stripped = route.trim_start_matches('/');
    if stripped.is_empty() {
        return "home".to_owned();
    }
    stripped
        .replace('/', "_")
        .replace(['[', ']'], "")
        .replace('-', "_")
        .to_lowercase()
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Split a single camelCase/PascalCase/ALLCAPS word into lowercase tokens.
fn split_camel_case(s: &str) -> Vec<String> {
    if s.is_empty() {
        return Vec::new();
    }
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let mut tokens = Vec::new();
    let mut start = 0;

    for i in 1..n {
        let c = chars[i];
        if c.is_uppercase() {
            let prev_lower = chars[i - 1].is_lowercase() || chars[i - 1].is_numeric();
            // Start of a new word after an acronym: e.g. "APIRoute" → 'R' has prev='I'
            // (uppercase) but next='o' (lowercase) and there are preceding chars.
            let next_lower = i + 1 < n && (chars[i + 1].is_lowercase() || chars[i + 1].is_numeric());
            if prev_lower || (next_lower && i > start + 1) {
                let token: String = chars[start..i].iter().collect::<String>().to_lowercase();
                if !token.is_empty() {
                    tokens.push(token);
                }
                start = i;
            }
        }
    }
    let last: String = chars[start..].iter().collect::<String>().to_lowercase();
    if !last.is_empty() {
        tokens.push(last);
    }
    tokens
}

/// Map token lists to canonical action strings.
///
/// Returns `None` when no recognised action pattern is found; the caller
/// falls back to the kind-default action.
fn detect_action_from_tokens(tokens: &[&str], kind: EventKind) -> Option<String> {
    // HTTP methods (ApiCall only): single token that is an HTTP verb.
    if kind == EventKind::ApiCall
        && tokens.len() == 1
        && HTTP_METHODS.contains(&tokens[0])
    {
        return Some(format!("{}_api_called", tokens[0]));
    }

    // Auth: sign-in patterns.
    let has_sign = tokens.contains(&"sign") || tokens.contains(&"signin");
    let has_log = tokens.contains(&"login") || tokens.contains(&"log");
    let has_in = tokens.contains(&"in");
    let has_out = tokens.contains(&"out");
    let has_up = tokens.contains(&"up");

    if (has_sign && has_out) || tokens.contains(&"signout") || tokens.contains(&"logout") {
        return Some("signed_out".to_owned());
    }
    if (has_sign && has_up) || tokens.contains(&"signup") || tokens.contains(&"register") {
        return Some("signed_up".to_owned());
    }
    if (has_sign || has_log) && has_in || tokens.contains(&"login") {
        return Some("signed_in".to_owned());
    }

    // Generic actions.
    if tokens.contains(&"submit") {
        return Some("submitted".to_owned());
    }
    if tokens.contains(&"click") {
        return Some("clicked".to_owned());
    }
    if tokens.contains(&"load") {
        return Some("loaded".to_owned());
    }
    if tokens.contains(&"error") {
        return Some("errored".to_owned());
    }
    if tokens.contains(&"view") {
        return Some("viewed".to_owned());
    }

    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- split_identifier ---------------------------------------------------

    #[test]
    fn split_identifier_camel_case() {
        assert_eq!(split_identifier("handleSubmit"), vec!["handle", "submit"]);
    }

    #[test]
    fn split_identifier_pascal_case() {
        assert_eq!(split_identifier("UserProfile"), vec!["user", "profile"]);
    }

    #[test]
    fn split_identifier_snake_case() {
        assert_eq!(
            split_identifier("user_signed_in"),
            vec!["user", "signed", "in"]
        );
    }

    #[test]
    fn split_identifier_kebab_case() {
        assert_eq!(split_identifier("user-profile"), vec!["user", "profile"]);
    }

    #[test]
    fn split_identifier_http_method() {
        assert_eq!(split_identifier("GET"), vec!["get"]);
    }

    #[test]
    fn split_identifier_on_sign_in() {
        assert_eq!(split_identifier("onSignIn"), vec!["on", "sign", "in"]);
    }

    #[test]
    fn split_identifier_sign_out() {
        assert_eq!(split_identifier("signOut"), vec!["sign", "out"]);
    }

    #[test]
    fn split_identifier_sign_up() {
        assert_eq!(split_identifier("signUp"), vec!["sign", "up"]);
    }

    #[test]
    fn split_identifier_single_word() {
        assert_eq!(split_identifier("submit"), vec!["submit"]);
    }

    #[test]
    fn split_identifier_empty() {
        let v: Vec<String> = split_identifier("");
        assert!(v.is_empty());
    }

    #[test]
    fn split_identifier_api_route() {
        assert_eq!(split_identifier("APIRoute"), vec!["api", "route"]);
    }

    #[test]
    fn split_identifier_all_post() {
        assert_eq!(split_identifier("POST"), vec!["post"]);
    }

    // --- route_to_name_prefix -----------------------------------------------

    #[test]
    fn route_to_name_prefix_home() {
        assert_eq!(route_to_name_prefix("/"), "home");
    }

    #[test]
    fn route_to_name_prefix_about() {
        assert_eq!(route_to_name_prefix("/about"), "about");
    }

    #[test]
    fn route_to_name_prefix_nested() {
        assert_eq!(route_to_name_prefix("/blog/[slug]"), "blog_slug");
    }

    #[test]
    fn route_to_name_prefix_api() {
        assert_eq!(route_to_name_prefix("/api/users"), "api_users");
    }

    #[test]
    fn route_to_name_prefix_kebab_segment() {
        assert_eq!(route_to_name_prefix("/user-profile"), "user_profile");
    }

    // --- Namer::derive — page views -----------------------------------------

    #[test]
    fn derive_page_view_from_route() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            route: Some("/about"),
            kind: EventKind::PageView,
            component_name: None,
            handler_name: None,
        });
        assert_eq!(result.name, "about_page_viewed");
        assert!((result.confidence - 0.84).abs() < 0.01);
    }

    #[test]
    fn derive_page_view_home() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            route: Some("/"),
            kind: EventKind::PageView,
            component_name: None,
            handler_name: None,
        });
        assert_eq!(result.name, "home_page_viewed");
    }

    #[test]
    fn derive_page_view_nested_route() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            route: Some("/blog/[slug]"),
            kind: EventKind::PageView,
            component_name: None,
            handler_name: None,
        });
        assert_eq!(result.name, "blog_slug_page_viewed");
    }

    // --- Namer::derive — API calls ------------------------------------------

    #[test]
    fn derive_api_call_from_route() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            route: Some("/api/users"),
            kind: EventKind::ApiCall,
            component_name: None,
            handler_name: None,
        });
        assert_eq!(result.name, "api_users_api_called");
    }

    #[test]
    fn derive_http_get() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            route: Some("/api/users"),
            handler_name: Some("GET"),
            kind: EventKind::ApiCall,
            component_name: None,
        });
        assert_eq!(result.name, "api_users_get_api_called");
    }

    #[test]
    fn derive_http_post() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            route: Some("/api/users"),
            handler_name: Some("POST"),
            kind: EventKind::ApiCall,
            component_name: None,
        });
        assert_eq!(result.name, "api_users_post_api_called");
    }

    #[test]
    fn derive_http_delete() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            route: Some("/api/items"),
            handler_name: Some("DELETE"),
            kind: EventKind::ApiCall,
            component_name: None,
        });
        assert_eq!(result.name, "api_items_delete_api_called");
    }

    // --- Namer::derive — auth events ----------------------------------------

    #[test]
    fn derive_auth_sign_in() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            handler_name: Some("signIn"),
            kind: EventKind::AuthEvent,
            route: None,
            component_name: None,
        });
        assert_eq!(result.name, "user_signed_in");
    }

    #[test]
    fn derive_auth_sign_out() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            handler_name: Some("signOut"),
            kind: EventKind::AuthEvent,
            route: None,
            component_name: None,
        });
        assert_eq!(result.name, "user_signed_out");
    }

    #[test]
    fn derive_auth_sign_up() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            handler_name: Some("signUp"),
            kind: EventKind::AuthEvent,
            route: None,
            component_name: None,
        });
        assert_eq!(result.name, "user_signed_up");
    }

    #[test]
    fn derive_auth_login() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            handler_name: Some("login"),
            kind: EventKind::AuthEvent,
            route: None,
            component_name: None,
        });
        assert_eq!(result.name, "user_signed_in");
    }

    #[test]
    fn derive_auth_logout() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            handler_name: Some("logout"),
            kind: EventKind::AuthEvent,
            route: None,
            component_name: None,
        });
        assert_eq!(result.name, "user_signed_out");
    }

    #[test]
    fn derive_auth_register() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            handler_name: Some("register"),
            kind: EventKind::AuthEvent,
            route: None,
            component_name: None,
        });
        assert_eq!(result.name, "user_signed_up");
    }

    #[test]
    fn derive_auth_on_sign_in() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            handler_name: Some("onSignIn"),
            kind: EventKind::AuthEvent,
            route: None,
            component_name: None,
        });
        assert_eq!(result.name, "user_signed_in");
    }

    // --- Namer::derive — form submit ----------------------------------------

    #[test]
    fn derive_form_handle_submit() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            handler_name: Some("handleSubmit"),
            kind: EventKind::FormSubmit,
            route: None,
            component_name: None,
        });
        assert_eq!(result.name, "form_submitted");
    }

    #[test]
    fn derive_form_on_submit() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            handler_name: Some("onSubmit"),
            kind: EventKind::FormSubmit,
            route: None,
            component_name: None,
        });
        assert_eq!(result.name, "form_submitted");
    }

    #[test]
    fn derive_form_submit_form() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            handler_name: Some("submitForm"),
            kind: EventKind::FormSubmit,
            route: None,
            component_name: None,
        });
        assert_eq!(result.name, "form_submitted");
    }

    // --- Namer::derive — component names ------------------------------------

    #[test]
    fn derive_from_component_name() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            component_name: Some("CheckoutForm"),
            kind: EventKind::FormSubmit,
            route: None,
            handler_name: None,
        });
        assert_eq!(result.name, "checkout_submitted");
    }

    #[test]
    fn derive_route_wins_over_component() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            route: Some("/about"),
            component_name: Some("HomePage"),
            kind: EventKind::PageView,
            handler_name: None,
        });
        assert_eq!(result.name, "about_page_viewed");
    }

    // --- Namer::derive — fallback -------------------------------------------

    #[test]
    fn derive_fallback_no_signals_page_view() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            kind: EventKind::PageView,
            route: None,
            component_name: None,
            handler_name: None,
        });
        assert_eq!(result.name, "page_page_viewed");
        assert!((result.confidence - 0.6).abs() < 0.01); // 0.5*0.6 + 0.75*0.4 = 0.6
    }

    #[test]
    fn derive_fallback_auth_no_signals() {
        let namer = Namer::new();
        let result = namer.derive(&NameSignals {
            kind: EventKind::AuthEvent,
            route: None,
            component_name: None,
            handler_name: None,
        });
        assert_eq!(result.name, "user_triggered");
    }

    // --- Namer confidence ordering ------------------------------------------

    #[test]
    fn confidence_route_higher_than_handler() {
        let namer = Namer::new();
        let route_result = namer.derive(&NameSignals {
            route: Some("/about"),
            kind: EventKind::PageView,
            component_name: None,
            handler_name: None,
        });
        let handler_result = namer.derive(&NameSignals {
            handler_name: Some("handleView"),
            kind: EventKind::PageView,
            route: None,
            component_name: None,
        });
        assert!(route_result.confidence > handler_result.confidence);
    }

    #[test]
    fn confidence_component_between_route_and_handler() {
        let namer = Namer::new();
        let route_result = namer.derive(&NameSignals {
            route: Some("/about"),
            kind: EventKind::PageView,
            component_name: None,
            handler_name: None,
        });
        let comp_result = namer.derive(&NameSignals {
            component_name: Some("AboutPage"),
            kind: EventKind::PageView,
            route: None,
            handler_name: None,
        });
        let handler_result = namer.derive(&NameSignals {
            handler_name: Some("handleView"),
            kind: EventKind::PageView,
            route: None,
            component_name: None,
        });
        assert!(route_result.confidence > comp_result.confidence);
        assert!(comp_result.confidence >= handler_result.confidence);
    }

    // --- Determinism --------------------------------------------------------

    #[test]
    fn derive_is_deterministic() {
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

    // --- Output format ------------------------------------------------------

    #[test]
    fn derive_name_is_snake_case() {
        let namer = Namer::new();
        let inputs = [
            NameSignals {
                route: Some("/about"),
                kind: EventKind::PageView,
                component_name: None,
                handler_name: None,
            },
            NameSignals {
                handler_name: Some("handleSubmit"),
                kind: EventKind::FormSubmit,
                route: None,
                component_name: None,
            },
            NameSignals {
                handler_name: Some("signIn"),
                kind: EventKind::AuthEvent,
                route: None,
                component_name: None,
            },
        ];
        for signals in &inputs {
            let result = namer.derive(signals);
            assert!(
                result.name.chars().all(|c| c.is_lowercase() || c == '_' || c.is_numeric()),
                "name {:?} is not snake_case",
                result.name
            );
        }
    }
}
