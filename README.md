# Infergen

A framework-agnostic library that scans any codebase offline, infers a typed analytics
event catalog from project context, and generates a type-safe, multi-provider telemetry
SDK — eliminating manual event planning, naming drift, and provider lock-in.

![CI](https://github.com/infergen/infergen/actions/workflows/ci.yml/badge.svg)

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
| Rust | ≥1.85 | `rustup` auto-installs from `rust-toolchain.toml` — [rustup.rs](https://rustup.rs) |
| Node | ≥20 | [nodejs.org](https://nodejs.org) or `nvm install 20` |
| pnpm | 9 | `corepack enable && corepack prepare pnpm@9.15.0 --activate` |
| just | latest | `cargo install just` or `brew install just` |
| cargo-deny | latest | `cargo install cargo-deny` (needed for `just deny`) |

---

## Quickstart

```bash
just install      # pnpm install
just build        # cargo + pnpm build
just test         # all tests
just ci           # full local CI parity
cargo run -p infergen-cli -- --version
```

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

## License

Apache-2.0 — see [LICENSE](./LICENSE).
