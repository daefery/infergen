//! Per-stack onboarding templates for `infergen init`.
//!
//! Each [`StackTemplate`] bundles an example `catalog.yaml` string (embedded at
//! compile time), a language-matched tracking snippet, and ordered quickstart
//! steps so `infergen init` can scaffold a usable starting point without
//! requiring a prior `infergen scan` run.

use crate::detect::Framework;

/// A stack-specific onboarding template.
pub struct StackTemplate {
    /// Human-readable stack name (e.g. `"Next.js"`).
    pub stack_name: &'static str,
    /// Example `catalog.yaml` content, ready to write to `.infergen/catalog.yaml`.
    pub example_catalog_yaml: &'static str,
    /// Short language-appropriate tracking snippet shown in the quickstart.
    pub tracking_snippet: &'static str,
    /// Ordered list of quickstart steps printed after `infergen init`.
    pub quickstart_steps: &'static [&'static str],
}

static NEXTJS_TEMPLATE: StackTemplate = StackTemplate {
    stack_name: "Next.js",
    example_catalog_yaml: include_str!("../templates/nextjs/catalog.yaml"),
    tracking_snippet: include_str!("../templates/nextjs/snippet.ts"),
    quickstart_steps: &[
        "Run `infergen scan` to discover events in your Next.js codebase.",
        "Review `.infergen/catalog.yaml` — approve or ignore proposed events.",
        "Run `infergen generate` to emit a typed TypeScript SDK.",
        "Import and call: `import { track } from './infergen.generated'; track.pageViewed({ page_path: '/' });`",
        "Add `infergen scan --check` to CI to catch untracked moments before merge.",
    ],
};

static EXPRESS_TEMPLATE: StackTemplate = StackTemplate {
    stack_name: "Express",
    example_catalog_yaml: include_str!("../templates/express/catalog.yaml"),
    tracking_snippet: include_str!("../templates/express/snippet.ts"),
    quickstart_steps: &[
        "Run `infergen scan` to discover events in your Express app.",
        "Review `.infergen/catalog.yaml` — approve or ignore proposed events.",
        "Run `infergen generate` to emit a typed TypeScript SDK.",
        "Import and call: `import { track } from './infergen.generated'; track.apiRequestReceived({ method: 'POST', path: '/login' });`",
        "Add `infergen scan --check` to CI to catch untracked moments before merge.",
    ],
};

static NESTJS_TEMPLATE: StackTemplate = StackTemplate {
    stack_name: "NestJS",
    example_catalog_yaml: include_str!("../templates/nestjs/catalog.yaml"),
    tracking_snippet: include_str!("../templates/nestjs/snippet.ts"),
    quickstart_steps: &[
        "Run `infergen scan` to discover events in your NestJS app.",
        "Review `.infergen/catalog.yaml` — approve or ignore proposed events.",
        "Run `infergen generate` to emit a typed TypeScript SDK.",
        "Import and call: `import { track } from './infergen.generated'; track.userCreated({ role: 'admin' });`",
        "Add `infergen scan --check` to CI to catch untracked moments before merge.",
    ],
};

static VUE_TEMPLATE: StackTemplate = StackTemplate {
    stack_name: "Vue/Nuxt",
    example_catalog_yaml: include_str!("../templates/vue/catalog.yaml"),
    tracking_snippet: include_str!("../templates/vue/snippet.ts"),
    quickstart_steps: &[
        "Run `infergen scan` to discover events in your Vue/Nuxt app.",
        "Review `.infergen/catalog.yaml` — approve or ignore proposed events.",
        "Run `infergen generate` to emit a typed TypeScript SDK.",
        "Import and call: `import { track } from './infergen.generated'; track.pageViewed({ route_name: 'home' });`",
        "Add `infergen scan --check` to CI to catch untracked moments before merge.",
    ],
};

static SVELTEKIT_TEMPLATE: StackTemplate = StackTemplate {
    stack_name: "SvelteKit",
    example_catalog_yaml: include_str!("../templates/sveltekit/catalog.yaml"),
    tracking_snippet: include_str!("../templates/sveltekit/snippet.ts"),
    quickstart_steps: &[
        "Run `infergen scan` to discover events in your SvelteKit app.",
        "Review `.infergen/catalog.yaml` — approve or ignore proposed events.",
        "Run `infergen generate` to emit a typed TypeScript SDK.",
        "Import and call: `import { track } from '$lib/infergen.generated'; track.pageViewed({ page_path: '/' });`",
        "Add `infergen scan --check` to CI to catch untracked moments before merge.",
    ],
};

static DJANGO_TEMPLATE: StackTemplate = StackTemplate {
    stack_name: "Django",
    example_catalog_yaml: include_str!("../templates/django/catalog.yaml"),
    tracking_snippet: include_str!("../templates/django/snippet.py"),
    quickstart_steps: &[
        "Run `infergen scan` to discover events in your Django project.",
        "Review `.infergen/catalog.yaml` — approve or ignore proposed events.",
        "Run `infergen generate` to emit a typed Python SDK.",
        "Import and call: `from infergen_generated import track; track.view_requested(view_name='index', method='GET')`",
        "Add `infergen scan --check` to CI to catch untracked moments before merge.",
    ],
};

static FASTAPI_TEMPLATE: StackTemplate = StackTemplate {
    stack_name: "FastAPI",
    example_catalog_yaml: include_str!("../templates/fastapi/catalog.yaml"),
    tracking_snippet: include_str!("../templates/fastapi/snippet.py"),
    quickstart_steps: &[
        "Run `infergen scan` to discover events in your FastAPI app.",
        "Review `.infergen/catalog.yaml` — approve or ignore proposed events.",
        "Run `infergen generate` to emit a typed Python SDK.",
        "Import and call: `from infergen_generated import track; track.endpoint_called(path='/users', method='POST')`",
        "Add `infergen scan --check` to CI to catch untracked moments before merge.",
    ],
};

static FLASK_TEMPLATE: StackTemplate = StackTemplate {
    stack_name: "Flask",
    example_catalog_yaml: include_str!("../templates/flask/catalog.yaml"),
    tracking_snippet: include_str!("../templates/flask/snippet.py"),
    quickstart_steps: &[
        "Run `infergen scan` to discover events in your Flask app.",
        "Review `.infergen/catalog.yaml` — approve or ignore proposed events.",
        "Run `infergen generate` to emit a typed Python SDK.",
        "Import and call: `from infergen_generated import track; track.route_accessed(endpoint='index', method='GET')`",
        "Add `infergen scan --check` to CI to catch untracked moments before merge.",
    ],
};

static GIN_TEMPLATE: StackTemplate = StackTemplate {
    stack_name: "Gin",
    example_catalog_yaml: include_str!("../templates/gin/catalog.yaml"),
    tracking_snippet: include_str!("../templates/gin/snippet.go"),
    quickstart_steps: &[
        "Run `infergen scan` to discover events in your Gin app.",
        "Review `.infergen/catalog.yaml` — approve or ignore proposed events.",
        "Run `infergen generate` to emit a typed Go SDK.",
        "Import and call: `infergen.Track.HttpRequestHandled(\"GET\", \"/users\")`",
        "Add `infergen scan --check` to CI to catch untracked moments before merge.",
    ],
};

static ECHO_TEMPLATE: StackTemplate = StackTemplate {
    stack_name: "Echo",
    example_catalog_yaml: include_str!("../templates/echo/catalog.yaml"),
    tracking_snippet: include_str!("../templates/echo/snippet.go"),
    quickstart_steps: &[
        "Run `infergen scan` to discover events in your Echo app.",
        "Review `.infergen/catalog.yaml` — approve or ignore proposed events.",
        "Run `infergen generate` to emit a typed Go SDK.",
        "Import and call: `infergen.Track.HttpRequestHandled(\"GET\", \"/users\")`",
        "Add `infergen scan --check` to CI to catch untracked moments before merge.",
    ],
};

static RAILS_TEMPLATE: StackTemplate = StackTemplate {
    stack_name: "Rails",
    example_catalog_yaml: include_str!("../templates/rails/catalog.yaml"),
    tracking_snippet: include_str!("../templates/rails/snippet.rb"),
    quickstart_steps: &[
        "Run `infergen scan` to discover events in your Rails app.",
        "Review `.infergen/catalog.yaml` — approve or ignore proposed events.",
        "Run `infergen generate` to emit a typed Ruby SDK.",
        "Require and call: `Infergen::Track.controller_action_called(controller: 'users', action: 'create')`",
        "Add `infergen scan --check` to CI to catch untracked moments before merge.",
    ],
};

static GENERIC_TEMPLATE: StackTemplate = StackTemplate {
    stack_name: "Generic",
    example_catalog_yaml: include_str!("../templates/generic/catalog.yaml"),
    tracking_snippet: include_str!("../templates/generic/snippet.txt"),
    quickstart_steps: &[
        "Run `infergen scan` to discover events in your codebase.",
        "Review `.infergen/catalog.yaml` — approve or ignore proposed events.",
        "Run `infergen generate` to emit a typed SDK.",
        "Import and call the generated `track` functions from your code.",
        "Add `infergen scan --check` to CI to catch untracked moments before merge.",
    ],
};

/// Return the best-fit [`StackTemplate`] for a detected set of frameworks.
///
/// Priority order: Next.js > NestJS > Express > Vue > SvelteKit > Django >
/// FastAPI > Flask > Gin > Echo > Rails > Generic (fallback).
pub fn template_for_frameworks(frameworks: &[Framework]) -> &'static StackTemplate {
    let has = |f: Framework| frameworks.contains(&f);

    if has(Framework::NextJs) {
        &NEXTJS_TEMPLATE
    } else if has(Framework::NestJs) {
        &NESTJS_TEMPLATE
    } else if has(Framework::Express) {
        &EXPRESS_TEMPLATE
    } else if has(Framework::Vue) {
        &VUE_TEMPLATE
    } else if has(Framework::SvelteKit) {
        &SVELTEKIT_TEMPLATE
    } else if has(Framework::Django) {
        &DJANGO_TEMPLATE
    } else if has(Framework::FastApi) {
        &FASTAPI_TEMPLATE
    } else if has(Framework::Flask) {
        &FLASK_TEMPLATE
    } else if has(Framework::Gin) {
        &GIN_TEMPLATE
    } else if has(Framework::Echo) {
        &ECHO_TEMPLATE
    } else if has(Framework::Rails) {
        &RAILS_TEMPLATE
    } else {
        &GENERIC_TEMPLATE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_nextjs_matched() {
        let t = template_for_frameworks(&[Framework::NextJs]);
        assert_eq!(t.stack_name, "Next.js");
    }

    #[test]
    fn template_nestjs_matched() {
        let t = template_for_frameworks(&[Framework::NestJs]);
        assert_eq!(t.stack_name, "NestJS");
    }

    #[test]
    fn template_express_matched() {
        let t = template_for_frameworks(&[Framework::Express]);
        assert_eq!(t.stack_name, "Express");
    }

    #[test]
    fn template_vue_matched() {
        let t = template_for_frameworks(&[Framework::Vue]);
        assert_eq!(t.stack_name, "Vue/Nuxt");
    }

    #[test]
    fn template_sveltekit_matched() {
        let t = template_for_frameworks(&[Framework::SvelteKit]);
        assert_eq!(t.stack_name, "SvelteKit");
    }

    #[test]
    fn template_django_matched() {
        let t = template_for_frameworks(&[Framework::Django]);
        assert_eq!(t.stack_name, "Django");
    }

    #[test]
    fn template_fastapi_matched() {
        let t = template_for_frameworks(&[Framework::FastApi]);
        assert_eq!(t.stack_name, "FastAPI");
    }

    #[test]
    fn template_flask_matched() {
        let t = template_for_frameworks(&[Framework::Flask]);
        assert_eq!(t.stack_name, "Flask");
    }

    #[test]
    fn template_gin_matched() {
        let t = template_for_frameworks(&[Framework::Gin]);
        assert_eq!(t.stack_name, "Gin");
    }

    #[test]
    fn template_echo_matched() {
        let t = template_for_frameworks(&[Framework::Echo]);
        assert_eq!(t.stack_name, "Echo");
    }

    #[test]
    fn template_rails_matched() {
        let t = template_for_frameworks(&[Framework::Rails]);
        assert_eq!(t.stack_name, "Rails");
    }

    #[test]
    fn template_generic_fallback() {
        let t = template_for_frameworks(&[]);
        assert_eq!(t.stack_name, "Generic");
    }

    #[test]
    fn template_nextjs_priority_over_express() {
        let t = template_for_frameworks(&[Framework::NextJs, Framework::Express]);
        assert_eq!(t.stack_name, "Next.js");
    }

    #[test]
    fn catalog_yaml_nonempty() {
        let templates: &[&StackTemplate] = &[
            &NEXTJS_TEMPLATE,
            &EXPRESS_TEMPLATE,
            &NESTJS_TEMPLATE,
            &VUE_TEMPLATE,
            &SVELTEKIT_TEMPLATE,
            &DJANGO_TEMPLATE,
            &FASTAPI_TEMPLATE,
            &FLASK_TEMPLATE,
            &GIN_TEMPLATE,
            &ECHO_TEMPLATE,
            &RAILS_TEMPLATE,
            &GENERIC_TEMPLATE,
        ];
        for t in templates {
            assert!(
                !t.example_catalog_yaml.is_empty(),
                "{} catalog yaml is empty",
                t.stack_name
            );
            assert!(
                t.example_catalog_yaml.contains("schemaVersion"),
                "{} catalog yaml missing schemaVersion",
                t.stack_name
            );
        }
    }

    #[test]
    fn snippet_nonempty() {
        let templates: &[&StackTemplate] = &[
            &NEXTJS_TEMPLATE,
            &EXPRESS_TEMPLATE,
            &NESTJS_TEMPLATE,
            &VUE_TEMPLATE,
            &SVELTEKIT_TEMPLATE,
            &DJANGO_TEMPLATE,
            &FASTAPI_TEMPLATE,
            &FLASK_TEMPLATE,
            &GIN_TEMPLATE,
            &ECHO_TEMPLATE,
            &RAILS_TEMPLATE,
            &GENERIC_TEMPLATE,
        ];
        for t in templates {
            assert!(!t.tracking_snippet.is_empty(), "{} snippet is empty", t.stack_name);
        }
    }

    #[test]
    fn quickstart_steps_nonempty() {
        let templates: &[&StackTemplate] = &[
            &NEXTJS_TEMPLATE,
            &EXPRESS_TEMPLATE,
            &NESTJS_TEMPLATE,
            &VUE_TEMPLATE,
            &SVELTEKIT_TEMPLATE,
            &DJANGO_TEMPLATE,
            &FASTAPI_TEMPLATE,
            &FLASK_TEMPLATE,
            &GIN_TEMPLATE,
            &ECHO_TEMPLATE,
            &RAILS_TEMPLATE,
            &GENERIC_TEMPLATE,
        ];
        for t in templates {
            assert!(
                t.quickstart_steps.len() >= 3,
                "{} has fewer than 3 quickstart steps",
                t.stack_name
            );
        }
    }
}
