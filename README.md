# Infergen

A framework-agnostic library that scans any codebase offline, infers a typed analytics
event catalog from project context, and generates a type-safe, multi-provider telemetry
SDK — eliminating manual event planning, naming drift, and provider lock-in.

[![CI](https://github.com/infergen/infergen/actions/workflows/ci.yml/badge.svg)](https://github.com/infergen/infergen/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](./LICENSE)
[![Version](https://img.shields.io/badge/version-1.0.0-green.svg)](./CHANGELOG.md)

---

## What it does

1. **Scan** — parses your routes, forms, auth flows, API endpoints, and error boundaries across any language/framework and proposes an event catalog.
2. **Review** — you approve, ignore, and rename proposals. Manual edits survive re-scans.
3. **Generate** — emits a fully typed SDK in your project's language(s); one call site fans out to all configured analytics providers.

No spreadsheets. No manual tracking plans. Your analytics catalog is derived from — and kept in sync with — your actual code.

---

## Features

| Category | What's included |
|---|---|
| **Scan** | JS/TS (OXC), Python, Go, Ruby parsers; Next.js, React Router, Vue/Nuxt, SvelteKit, Express, NestJS, FastAPI, Django, Flask, Gin, Echo, Rails adapters |
| **Catalog** | `catalog.yaml` — stable IDs, PII flags, confidence scores, providers, diff-friendly ordering |
| **Codegen** | TypeScript, Python, Go, Ruby — typed per-event functions, JSDoc, tree-shakeable |
| **Providers** | Segment, Amplitude, Mixpanel, PostHog, GA4, RudderStack, generic HTTP, database (Postgres/MySQL/SQLite) |
| **CI** | `scan --check` detects drift; GitHub Action + GitLab/CircleCI recipes included |
| **Intelligence** | Optional LLM pass (Ollama, Claude, OpenAI) for low-confidence events; semantic flow/funnel detection; suggestion quality loop from review history |
| **DX** | Offline HTML catalog viewer; VS Code extension; plugin scaffold; PII/compliance manifest export; per-stack `init` templates |

---

## Quickstart

**Install:**
```bash
cargo install infergen
# or build from source:
git clone https://github.com/infergen/infergen && cd infergen
cargo install --path crates/infergen-cli
```

**5-minute workflow:**
```bash
cd your-project

# 1. Detect stack + write config
infergen init

# 2. Scan source → propose event catalog
infergen scan
# → .infergen/catalog.yaml (proposed events)

# 3. Review proposals
infergen review list
infergen review approve --all          # or approve individually
infergen review rename evt_xxx user_signed_up

# 4. Generate typed SDK
infergen generate
# → infergen.generated.ts (or .py / .go / .rb)

# 5. Use the SDK
import { track } from './infergen.generated'
track.userSignedUp({ method: 'google' })
```

**Browse your catalog:**
```bash
infergen view      # opens catalog-viewer.html in your browser
```

---

## Commands

| Command | Description |
|---|---|
| `infergen init` | Detect languages/frameworks, write `infergen.config.{json,toml}` |
| `infergen scan` | Propose event catalog from source code |
| `infergen generate` | Emit typed SDK from approved catalog |
| `infergen check` | CI mode — fail on drift, untracked moments, or convention violations |
| `infergen watch` | Live re-scan + regenerate on file change |
| `infergen review <action>` | Approve / ignore / rename / describe events; show diff |
| `infergen view` | Generate offline HTML catalog viewer |
| `infergen manifest` | Export PII/compliance manifest (JSON or Markdown) |
| `infergen plugin scaffold` | Scaffold a new provider, adapter, or parser plugin |

Run `infergen <command> --help` for full argument docs.

---

## Configuration

Infergen reads `infergen.config.json` (or `.toml`) from the project root.

```json
{
  "catalog": ".infergen/catalog.yaml",
  "output": "infergen/generated",
  "naming": { "convention": "entity.action.state", "case": "snake_case" },
  "providers": [
    { "name": "posthog", "config": { "apiKey": "ph-..." } }
  ],
  "llm": {
    "enabled": false,
    "provider": "ollama",
    "confidenceThreshold": 0.75
  }
}
```

`infergen init` writes a config file tuned for your detected stack.

---

## Documentation

| Doc | What it covers |
|-----|---------------|
| [Quickstart](docs/quickstart.md) | From zero to a tracked event in under 5 minutes |
| [CLI Reference](docs/site/cli-reference.md) | All commands and flags |
| [Config Reference](docs/site/config-reference.md) | `infergen.config.*` schema |
| [Catalog Schema](docs/site/catalog-schema.md) | `.infergen/catalog.yaml` format |
| [Adapter Gallery](docs/site/adapter-gallery.md) | All 13 supported stacks |
| [Plugin SDK](docs/plugin-sdk.md) | Custom adapters, parsers, providers |
| [Migrate from Typewriter](docs/migrate-from-typewriter.md) | Segment Typewriter → Infergen |
| [Migrate from Avo](docs/migrate-from-avo.md) | Avo → Infergen |

---

## Repository layout

```
infergen/
├── Cargo.toml                      # Rust workspace manifest
├── rust-toolchain.toml             # pinned toolchain + components
├── rustfmt.toml                    # Rust formatting config
├── deny.toml                       # cargo-deny: license + advisory gate
├── CONTRIBUTING.md                 # dev setup, conventions, release process
├── Justfile                        # cross-language task runner
├── package.json                    # root JS workspace orchestration (private)
├── pnpm-workspace.yaml
├── .github/workflows/ci.yml        # Rust + JS CI
├── crates/
│   ├── infergen-types/             # shared domain types
│   ├── infergen-core/              # scan engine (parsers, adapters, namer, codegen)
│   └── infergen-cli/               # `infergen` binary
├── packages/
│   ├── runtime/                    # @infergen/runtime — TypeScript runtime SDK
│   └── vscode-infergen/            # VS Code extension
├── docs/
│   ├── site/                       # documentation site (index, CLI ref, adapter gallery, …)
│   ├── quickstart.md
│   ├── plugin-sdk.md
│   ├── ci-recipes/                 # GitHub Actions, GitLab, CircleCI recipes
│   ├── migrate-from-typewriter.md
│   └── migrate-from-avo.md
└── examples/
    ├── nextjs-app/                 # Next.js + TypeScript example
    ├── express-app/                # Express + TypeScript example
    ├── django-app/                 # Django + Python example
    └── rails-app/                  # Rails + Ruby example
```

---

## Prerequisites

| Tool | Version | Install |
|---|---|---|
| Rust | ≥1.94 | `rustup` auto-installs from `rust-toolchain.toml` — [rustup.rs](https://rustup.rs) |
| Node | ≥20 | [nodejs.org](https://nodejs.org) or `nvm install 20` |
| pnpm | 9 | `corepack enable && corepack prepare pnpm@9.15.0 --activate` |
| just | latest | `cargo install just` or `brew install just` |
| cargo-deny | latest | `cargo install cargo-deny` (needed for `just deny`) |

---

## CLI usage

```bash
# Initialize a project (auto-detects stack, scaffolds example catalog)
infergen init
infergen init --format toml
infergen init --force        # overwrite existing config
infergen init --no-example   # skip catalog scaffold

# Scan source and propose events
infergen scan

# Review and annotate the catalog
infergen review list
infergen review list --status proposed
infergen review approve --all
infergen review approve evt_0123456789abcdef
infergen review ignore evt_abc123
infergen review rename evt_abc123 user_signup_completed

# Generate a typed SDK from the approved catalog
infergen generate
infergen generate --check    # CI: fail if SDK is stale

# CI check: fail on drift, untracked moments, convention violations
infergen check
infergen check --json        # machine-readable output

# File watcher: re-scan + regenerate on change
infergen watch

# Offline catalog viewer (opens in browser)
infergen view
infergen view --no-open

# Plugin scaffolding
infergen plugin scaffold provider my-provider
infergen plugin scaffold adapter my-adapter --framework htmx
infergen plugin list-types

# Privacy/compliance manifest export
infergen manifest
infergen manifest --format markdown --output data-manifest.md
```

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│ CLI  (init · scan · generate · check · watch · view · …) │
└───────────────┬─────────────────────────────────────────┘
                │
   ┌────────────┴────────────┐
   │  Scan Engine            │
   │  ├─ Language Parsers    │  TS/JS · Python · Go · Ruby
   │  ├─ Framework Adapters  │  13 adapters across 5 languages
   │  ├─ Heuristic Namer     │
   │  └─ Optional Local LLM  │  (Ollama / Claude / OpenAI)
   └────────────┬────────────┘
                │ proposes
        ┌───────▼────────┐
        │  catalog.yaml  │  human-reviewed · version-controlled
        └───────┬────────┘
                │
        ┌───────▼────────┐
        │  Codegen        │  catalog → typed SDK (TS/Python/Go/Ruby)
        └───────┬────────┘
                │
        ┌───────▼──────────────────────────────┐
        │  Runtime SDK                          │
        │  ├─ Provider Plugins (Segment/Amp/…)  │
        │  └─ Queue · Batch · Retry · Consent   │
        └───────────────────────────────────────┘
```

See [PRD.md](./PRD.md) §8 and [ROADMAP.md](./ROADMAP.md) for the full architecture.

---

## Plugin development

Extend Infergen with custom framework adapters, language parsers, and analytics providers:

```bash
infergen plugin scaffold provider --name my-provider
infergen plugin scaffold adapter  --name my-adapter --framework my-framework
infergen plugin scaffold parser   --name my-language
```

See [`docs/plugin-sdk.md`](./docs/plugin-sdk.md) for the full plugin contract.

---

## Contributing

See [`CONTRIBUTING.md`](./CONTRIBUTING.md) for dev setup, conventions, and the release process.

```bash
just install   # pnpm install
just build     # cargo + pnpm build
just test      # all tests
just ci        # full local CI parity
```

---

## License

Apache-2.0 — see [LICENSE](./LICENSE).
