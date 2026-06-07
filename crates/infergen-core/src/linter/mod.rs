//! Naming convention engine and linter (E1.3).
//!
//! Validates event names in a [`Catalog`] against a configured
//! [`ConventionCase`] and emits structured [`LintViolation`]s with auto-fix
//! suggestions.

use std::fmt;

use infergen_types::{Catalog, EventStatus};

use crate::{config::NamingConfig, namer::split_identifier};

/// Supported event name case formats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConventionCase {
    /// All lowercase, words separated by underscores: `user_signed_in`.
    SnakeCase,
    /// First word lowercase, subsequent words capitalised: `userSignedIn`.
    CamelCase,
    /// Every word capitalised, no separator: `UserSignedIn`.
    PascalCase,
}

impl ConventionCase {
    /// Parse `naming.case` from a [`NamingConfig`].
    ///
    /// Recognised values: `"snake_case"`, `"camelCase"`, `"PascalCase"` /
    /// `"pascal_case"`. All other values default to [`ConventionCase::SnakeCase`].
    pub fn from_config(naming: &NamingConfig) -> Self {
        match naming.case.as_str() {
            "camelCase" => Self::CamelCase,
            "PascalCase" | "pascal_case" => Self::PascalCase,
            _ => Self::SnakeCase,
        }
    }
}

impl fmt::Display for ConventionCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SnakeCase => write!(f, "snake_case"),
            Self::CamelCase => write!(f, "camelCase"),
            Self::PascalCase => write!(f, "PascalCase"),
        }
    }
}

/// The specific rule that was violated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LintRule {
    /// Name characters do not match the configured case format.
    CaseViolation,
    /// Name is an empty string.
    EmptyName,
    /// Name contains two or more consecutive underscores.
    ConsecutiveUnderscores,
    /// Name starts or ends with an underscore.
    LeadingOrTrailingUnderscore,
}

/// A single naming convention violation for one catalog entry.
#[derive(Debug, Clone, PartialEq)]
pub struct LintViolation {
    /// Stable ID of the offending catalog entry (`evt_{016hex}`).
    pub event_id: String,
    /// Current name of the offending entry.
    pub event_name: String,
    /// Which rule was violated.
    pub rule: LintRule,
    /// Human-readable description of the violation.
    pub message: String,
    /// Auto-fix candidate name. `None` for `EmptyName` (can't auto-fix).
    pub suggestion: Option<String>,
}

/// Lint all non-[`EventStatus::Ignored`] catalog entries against `naming`.
///
/// Returns one [`LintViolation`] per rule per entry — multiple violations per
/// entry are possible. An empty catalog returns an empty `Vec`.
pub fn lint_catalog(catalog: &Catalog, naming: &NamingConfig) -> Vec<LintViolation> {
    let case = ConventionCase::from_config(naming);
    let mut violations = Vec::new();

    for entry in &catalog.events {
        if entry.status == EventStatus::Ignored {
            continue;
        }

        let name = &entry.name;
        let id = &entry.id;

        if name.is_empty() {
            violations.push(LintViolation {
                event_id: id.clone(),
                event_name: name.clone(),
                rule: LintRule::EmptyName,
                message: "event name is empty".to_string(),
                suggestion: None,
            });
            // Skip further checks — no meaningful validation on an empty name.
            continue;
        }

        if name.contains("__") {
            let fixed = name
                .split('_')
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join("_");
            violations.push(LintViolation {
                event_id: id.clone(),
                event_name: name.clone(),
                rule: LintRule::ConsecutiveUnderscores,
                message: format!("event name `{name}` contains consecutive underscores"),
                suggestion: Some(fixed),
            });
        }

        if name.starts_with('_') || name.ends_with('_') {
            let fixed = name.trim_matches('_').to_string();
            violations.push(LintViolation {
                event_id: id.clone(),
                event_name: name.clone(),
                rule: LintRule::LeadingOrTrailingUnderscore,
                message: format!("event name `{name}` has a leading or trailing underscore"),
                suggestion: Some(fixed),
            });
        }

        if !is_valid_case(name, &case) {
            let fixed = to_convention_name(name, case.clone());
            violations.push(LintViolation {
                event_id: id.clone(),
                event_name: name.clone(),
                rule: LintRule::CaseViolation,
                message: format!(
                    "event name `{name}` does not conform to {case} convention"
                ),
                suggestion: Some(fixed),
            });
        }
    }

    violations
}

/// Returns `true` when `name` already conforms to `case`.
///
/// For [`ConventionCase::SnakeCase`] this checks character set only
/// (`[a-z0-9_]`), not structural invariants (consecutive underscores,
/// leading/trailing underscores) — those are separate [`LintRule`] variants
/// so each violation has a single root cause.
pub fn is_valid_case(name: &str, case: &ConventionCase) -> bool {
    if name.is_empty() {
        return true;
    }
    match case {
        ConventionCase::SnakeCase => name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()),
        ConventionCase::CamelCase => {
            name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                && name.chars().all(|c| c.is_alphanumeric())
        }
        ConventionCase::PascalCase => {
            name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                && name.chars().all(|c| c.is_alphanumeric())
        }
    }
}

/// Format a slice of lowercase string tokens into the target `case`.
///
/// An empty slice returns `""` for all cases. Tokens are expected to already
/// be lowercase; mixed-case tokens are passed through unchanged.
pub fn apply_case(tokens: &[&str], case: ConventionCase) -> String {
    if tokens.is_empty() {
        return String::new();
    }
    match case {
        ConventionCase::SnakeCase => tokens.join("_"),
        ConventionCase::CamelCase => {
            let mut out = String::new();
            for (i, token) in tokens.iter().enumerate() {
                if i == 0 {
                    out.push_str(token);
                } else {
                    let mut chars = token.chars();
                    if let Some(first) = chars.next() {
                        for c in first.to_uppercase() {
                            out.push(c);
                        }
                        out.push_str(chars.as_str());
                    }
                }
            }
            out
        }
        ConventionCase::PascalCase => tokens
            .iter()
            .map(|token| {
                let mut chars = token.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => {
                        let mut s = String::new();
                        for c in first.to_uppercase() {
                            s.push(c);
                        }
                        s.push_str(chars.as_str());
                        s
                    }
                }
            })
            .collect(),
    }
}

/// Convert `name` to the target `case` by tokenising with [`split_identifier`]
/// then reformatting with [`apply_case`].
pub fn to_convention_name(name: &str, case: ConventionCase) -> String {
    let tokens = split_identifier(name);
    let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
    apply_case(&refs, case)
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NamingConfig;

    // ── apply_case ────────────────────────────────────────────────────────────

    #[test]
    fn apply_case_snake_multi_word() {
        assert_eq!(
            apply_case(&["user", "signed", "in"], ConventionCase::SnakeCase),
            "user_signed_in"
        );
    }

    #[test]
    fn apply_case_snake_single_word() {
        assert_eq!(apply_case(&["submit"], ConventionCase::SnakeCase), "submit");
    }

    #[test]
    fn apply_case_snake_empty() {
        assert_eq!(apply_case(&[], ConventionCase::SnakeCase), "");
    }

    #[test]
    fn apply_case_camel_multi_word() {
        assert_eq!(
            apply_case(&["user", "signed", "in"], ConventionCase::CamelCase),
            "userSignedIn"
        );
    }

    #[test]
    fn apply_case_camel_single_word() {
        assert_eq!(apply_case(&["submit"], ConventionCase::CamelCase), "submit");
    }

    #[test]
    fn apply_case_pascal_multi_word() {
        assert_eq!(
            apply_case(&["user", "signed", "in"], ConventionCase::PascalCase),
            "UserSignedIn"
        );
    }

    #[test]
    fn apply_case_pascal_single_word() {
        assert_eq!(
            apply_case(&["submit"], ConventionCase::PascalCase),
            "Submit"
        );
    }

    // ── to_convention_name ────────────────────────────────────────────────────

    #[test]
    fn to_convention_name_pascal_to_snake() {
        assert_eq!(
            to_convention_name("UserSignedIn", ConventionCase::SnakeCase),
            "user_signed_in"
        );
    }

    #[test]
    fn to_convention_name_snake_to_camel() {
        assert_eq!(
            to_convention_name("user_signed_in", ConventionCase::CamelCase),
            "userSignedIn"
        );
    }

    #[test]
    fn to_convention_name_snake_to_pascal() {
        assert_eq!(
            to_convention_name("user_signed_in", ConventionCase::PascalCase),
            "UserSignedIn"
        );
    }

    #[test]
    fn to_convention_name_empty() {
        assert_eq!(to_convention_name("", ConventionCase::SnakeCase), "");
    }

    // ── is_valid_case ─────────────────────────────────────────────────────────

    #[test]
    fn is_valid_case_snake_valid() {
        assert!(is_valid_case("about_page_viewed", &ConventionCase::SnakeCase));
    }

    #[test]
    fn is_valid_case_snake_invalid_upper() {
        assert!(!is_valid_case("AboutPageViewed", &ConventionCase::SnakeCase));
    }

    #[test]
    fn is_valid_case_snake_underscore_only() {
        // Structural rules (leading/trailing/consecutive underscores) handle this.
        assert!(is_valid_case("_", &ConventionCase::SnakeCase));
    }

    #[test]
    fn is_valid_case_snake_digit() {
        assert!(is_valid_case("user123_event", &ConventionCase::SnakeCase));
    }

    #[test]
    fn is_valid_case_camel_valid() {
        assert!(is_valid_case("userSignedIn", &ConventionCase::CamelCase));
    }

    #[test]
    fn is_valid_case_camel_invalid_underscore() {
        assert!(!is_valid_case("user_signed_in", &ConventionCase::CamelCase));
    }

    #[test]
    fn is_valid_case_camel_invalid_starts_upper() {
        assert!(!is_valid_case("UserSignedIn", &ConventionCase::CamelCase));
    }

    #[test]
    fn is_valid_case_pascal_valid() {
        assert!(is_valid_case("UserSignedIn", &ConventionCase::PascalCase));
    }

    #[test]
    fn is_valid_case_pascal_invalid_lower() {
        assert!(!is_valid_case("userSignedIn", &ConventionCase::PascalCase));
    }

    // ── ConventionCase::from_config ───────────────────────────────────────────

    #[test]
    fn convention_case_from_config_snake() {
        let naming = NamingConfig {
            case: "snake_case".into(),
            ..Default::default()
        };
        assert_eq!(ConventionCase::from_config(&naming), ConventionCase::SnakeCase);
    }

    #[test]
    fn convention_case_from_config_camel() {
        let naming = NamingConfig {
            case: "camelCase".into(),
            ..Default::default()
        };
        assert_eq!(ConventionCase::from_config(&naming), ConventionCase::CamelCase);
    }

    #[test]
    fn convention_case_from_config_pascal() {
        let naming = NamingConfig {
            case: "PascalCase".into(),
            ..Default::default()
        };
        assert_eq!(ConventionCase::from_config(&naming), ConventionCase::PascalCase);
    }

    #[test]
    fn convention_case_from_config_unknown_defaults_snake() {
        let naming = NamingConfig {
            case: "kebab-case".into(),
            ..Default::default()
        };
        assert_eq!(ConventionCase::from_config(&naming), ConventionCase::SnakeCase);
    }

    // ── lint_catalog (unit-level, uses in-memory Catalog) ────────────────────

    fn make_entry(
        id: &str,
        name: &str,
        status: EventStatus,
    ) -> infergen_types::CatalogEntry {
        use infergen_types::{CatalogEntry, CatalogEventKind, EventProvenance};
        CatalogEntry {
            id: id.to_string(),
            name: name.to_string(),
            description: String::new(),
            status,
            kind: CatalogEventKind::PageView,
            confidence: 0.9,
            properties: vec![],
            providers: vec![],
            provenance: vec![EventProvenance {
                source_path: "src/page.tsx".into(),
                line: None,
                adapter: "nextjs".into(),
            }],
        }
    }

    fn make_catalog(entries: Vec<infergen_types::CatalogEntry>) -> Catalog {
        Catalog {
            schema_version: 1,
            events: entries,
        }
    }

    #[test]
    fn lint_empty_catalog() {
        let catalog = make_catalog(vec![]);
        let violations = lint_catalog(&catalog, &NamingConfig::default());
        assert!(violations.is_empty());
    }

    #[test]
    fn lint_all_valid_snake_names() {
        let catalog = make_catalog(vec![
            make_entry("evt_001", "page_viewed", EventStatus::Proposed),
            make_entry("evt_002", "user_signed_in", EventStatus::Approved),
            make_entry("evt_003", "checkout_submitted", EventStatus::Proposed),
        ]);
        let violations = lint_catalog(&catalog, &NamingConfig::default());
        assert!(violations.is_empty());
    }

    #[test]
    fn lint_case_violation_uppercase() {
        let catalog =
            make_catalog(vec![make_entry("evt_001", "UserSignedIn", EventStatus::Proposed)]);
        let violations = lint_catalog(&catalog, &NamingConfig::default());
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, LintRule::CaseViolation);
    }

    #[test]
    fn lint_case_violation_suggestion_correct() {
        let catalog =
            make_catalog(vec![make_entry("evt_001", "UserSignedIn", EventStatus::Proposed)]);
        let violations = lint_catalog(&catalog, &NamingConfig::default());
        assert_eq!(violations[0].suggestion.as_deref(), Some("user_signed_in"));
    }

    #[test]
    fn lint_consecutive_underscores() {
        let catalog =
            make_catalog(vec![make_entry("evt_001", "user__signed_in", EventStatus::Proposed)]);
        let violations = lint_catalog(&catalog, &NamingConfig::default());
        let rules: Vec<_> = violations.iter().map(|v| &v.rule).collect();
        assert!(rules.contains(&&LintRule::ConsecutiveUnderscores));
    }

    #[test]
    fn lint_consecutive_underscores_suggestion() {
        let catalog =
            make_catalog(vec![make_entry("evt_001", "user__signed_in", EventStatus::Proposed)]);
        let violations = lint_catalog(&catalog, &NamingConfig::default());
        let v = violations
            .iter()
            .find(|v| v.rule == LintRule::ConsecutiveUnderscores)
            .unwrap();
        assert_eq!(v.suggestion.as_deref(), Some("user_signed_in"));
    }

    #[test]
    fn lint_leading_underscore() {
        let catalog =
            make_catalog(vec![make_entry("evt_001", "_user_signed_in", EventStatus::Proposed)]);
        let violations = lint_catalog(&catalog, &NamingConfig::default());
        let rules: Vec<_> = violations.iter().map(|v| &v.rule).collect();
        assert!(rules.contains(&&LintRule::LeadingOrTrailingUnderscore));
    }

    #[test]
    fn lint_trailing_underscore() {
        let catalog =
            make_catalog(vec![make_entry("evt_001", "user_signed_in_", EventStatus::Proposed)]);
        let violations = lint_catalog(&catalog, &NamingConfig::default());
        let rules: Vec<_> = violations.iter().map(|v| &v.rule).collect();
        assert!(rules.contains(&&LintRule::LeadingOrTrailingUnderscore));
    }

    #[test]
    fn lint_leading_suggestion() {
        let catalog =
            make_catalog(vec![make_entry("evt_001", "_user_signed_in", EventStatus::Proposed)]);
        let violations = lint_catalog(&catalog, &NamingConfig::default());
        let v = violations
            .iter()
            .find(|v| v.rule == LintRule::LeadingOrTrailingUnderscore)
            .unwrap();
        assert_eq!(v.suggestion.as_deref(), Some("user_signed_in"));
    }

    #[test]
    fn lint_empty_name() {
        let catalog =
            make_catalog(vec![make_entry("evt_001", "", EventStatus::Proposed)]);
        let violations = lint_catalog(&catalog, &NamingConfig::default());
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, LintRule::EmptyName);
        assert_eq!(violations[0].suggestion, None);
    }

    #[test]
    fn lint_empty_name_no_further_violations() {
        // Empty name must not also trigger CaseViolation or structural rules.
        let catalog =
            make_catalog(vec![make_entry("evt_001", "", EventStatus::Proposed)]);
        let violations = lint_catalog(&catalog, &NamingConfig::default());
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn lint_ignored_skipped() {
        let catalog =
            make_catalog(vec![make_entry("evt_001", "BADNAME", EventStatus::Ignored)]);
        let violations = lint_catalog(&catalog, &NamingConfig::default());
        assert!(violations.is_empty());
    }

    #[test]
    fn lint_proposed_linted() {
        let catalog =
            make_catalog(vec![make_entry("evt_001", "BADNAME", EventStatus::Proposed)]);
        let violations = lint_catalog(&catalog, &NamingConfig::default());
        assert!(!violations.is_empty());
    }

    #[test]
    fn lint_multiple_violations_same_entry() {
        // "__BadName_" has: ConsecutiveUnderscores + LeadingOrTrailingUnderscore + CaseViolation
        let catalog =
            make_catalog(vec![make_entry("evt_001", "__BadName_", EventStatus::Proposed)]);
        let violations = lint_catalog(&catalog, &NamingConfig::default());
        let rules: Vec<_> = violations.iter().map(|v| &v.rule).collect();
        assert!(rules.contains(&&LintRule::ConsecutiveUnderscores));
        assert!(rules.contains(&&LintRule::LeadingOrTrailingUnderscore));
        assert!(rules.contains(&&LintRule::CaseViolation));
        assert_eq!(violations.len(), 3);
    }

    #[test]
    fn lint_camelcase_config_snake_name_violates() {
        let naming = NamingConfig {
            case: "camelCase".into(),
            ..Default::default()
        };
        let catalog =
            make_catalog(vec![make_entry("evt_001", "user_signed_in", EventStatus::Proposed)]);
        let violations = lint_catalog(&catalog, &naming);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, LintRule::CaseViolation);
        assert_eq!(violations[0].suggestion.as_deref(), Some("userSignedIn"));
    }

    #[test]
    fn lint_convention_display_snake() {
        assert_eq!(format!("{}", ConventionCase::SnakeCase), "snake_case");
    }

    #[test]
    fn lint_result_event_id_matches() {
        let catalog =
            make_catalog(vec![make_entry("evt_abc123", "BadName", EventStatus::Proposed)]);
        let violations = lint_catalog(&catalog, &NamingConfig::default());
        assert!(!violations.is_empty());
        assert_eq!(violations[0].event_id, "evt_abc123");
    }

    #[test]
    fn lint_multiple_entries_violations_independent() {
        let catalog = make_catalog(vec![
            make_entry("evt_001", "valid_name", EventStatus::Proposed),
            make_entry("evt_002", "BadName", EventStatus::Proposed),
        ]);
        let violations = lint_catalog(&catalog, &NamingConfig::default());
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].event_id, "evt_002");
    }
}
