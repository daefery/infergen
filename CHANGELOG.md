# Changelog

All notable changes to Infergen are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Infergen adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- E8.3: Catalog schema version validation + migration framework — `load_catalog` now rejects catalogs written by a newer Infergen binary and provides a structured upgrade path for future schema versions
- E8.3: `cargo-release` config (`release.toml`) for workspace-wide version bumping with automatic CHANGELOG promotion and npm version sync
- E8.3: npm publish CI workflow (`.github/workflows/publish-npm.yml`) for `@infergen/runtime` on release tag
- E8.3: `scripts/bump-version.sh` helper to sync Cargo workspace version with npm packages

---

## [0.1.0] — Upcoming

First versioned release. Full feature set across Milestones 0–8.3:

### Added

**Milestone 0 — Foundation & Proof of Concept**
- E0.1: Rust + pnpm monorepo scaffold with build/test/release tooling; cargo-dist v0.28.0 for multi-platform binary distribution
- E0.2: `infergen` CLI with `init`, `scan`, `generate`, `check`, `watch`, `review` commands and TOML/JSON config schema
- E0.3: TypeScript/JavaScript AST parsing via OXC
- E0.4: Next.js adapter — pages router, app router, NextAuth, API routes (vertical slice proof-of-concept)

**Milestone 1 — Catalog & Naming**
- E1.1: `catalog.yaml` schema with stable IDs (FNV-1a), provenance, typed properties, PII flags, and status lifecycle
- E1.2: Heuristic event namer deriving names from component/route/function identifiers with confidence scoring
- E1.3: Configurable naming convention engine (`entity.action.state`, snake_case) + linter with auto-suggestions
- E1.4: Property type inference from TypeScript AST and JSX input elements; 51-token PII detection
- E1.5: Review workflow — approve/ignore/rename/edit events; diff view between scan result and existing catalog

**Milestone 2 — Codegen & Type-Safe SDK**
- E2.1: TypeScript SDK codegen from approved catalog — one typed function per event, compile-time property safety
- E2.2: Autocomplete-friendly API, JSDoc from catalog descriptions, tree-shakeable, framework-friendly imports
- E2.3: Deterministic, idempotent output; `infergen generate --check` for CI stale-code detection

**Milestone 3 — Runtime SDK & Providers**
- E3.1: Provider plugin interface with `identify`, `track`, `flush`, `shutdown` contract + registry
- E3.2: Segment, Amplitude, Mixpanel, PostHog, GA4, RudderStack, and generic HTTP webhook adapters
- E3.2b: Database destination adapter (Postgres/MySQL/SQLite) with declarative event-table schema and migration generation
- E3.3: Batching, retry with exponential backoff, persistent offline queue, sampling, flush-on-exit; browser + Node runtimes
- E3.4: Consent gating, per-property PII redaction hooks, opt-out, region-based routing

**Milestone 4 — Incremental Scans & CI**
- E4.1: Three-way merge on re-scan — preserves all manual edits via stable IDs, drops only unreviewed disappeared events
- E4.2: `infergen scan --check` CI mode detecting drift, untracked moments, and naming convention violations
- E4.3: `infergen watch` mode for live re-scan + regenerate during development
- E4.4: GitHub Action + GitLab/CircleCI recipe; PR comment summarising new/changed/removed events

**Milestone 5 — Polyglot & Framework Breadth**
- E5.1: Python parser (`ast` module) + Django/FastAPI/Flask adapters + Python SDK codegen
- E5.2: Go parser (`go/ast`) + Gin/Echo/net/http adapters + Go SDK codegen
- E5.3: Ruby parser (Prism) + Rails adapter (routes, controllers, Devise) + Ruby SDK codegen
- E5.4: React Router, Vue/Nuxt, SvelteKit, Express, NestJS framework adapters
- E5.5: Monorepo/polyglot catalog — per-package namespace, cross-service event-name consistency check

**Milestone 6 — Intelligence & Semantic Naming**
- E6.1: Optional LLM pass for event name/description/property refinement — Ollama (local/offline), Anthropic Claude API, OpenAI API, and any OpenAI-compatible endpoint
- E6.2: Semantic flow detection — groups related events into multi-step funnels (checkout, onboarding, auth) across files using route-prefix, name-prefix, and known-pattern heuristics
- E6.3: Suggestion quality loop — per-adapter confidence multipliers and name hints learned from review accept/reject history (fully local, no cloud)

**Milestone 7 — Developer Experience & Ecosystem**
- E7.1: Offline catalog web viewer — self-contained HTML with zero runtime deps; `infergen view` opens in browser
- E7.2: VS Code extension — inline untracked-moment diagnostics, jump-to-trigger, `track.X` autocomplete, event hover docs, code lens with Approve/Ignore actions
- E7.3: Plugin SDK + docs — documented contracts and scaffold for community framework adapters, language parsers, and provider plugins
- E7.4: Data-collection manifest export — privacy/compliance YAML listing all collected events, properties, PII flags, and destinations; `infergen manifest` command
- E7.5: Per-stack `init` templates (Next.js, Express, Django, FastAPI, Rails, Go) + quickstart scaffolding; time-to-first-event < 5 minutes

**Milestone 8 (partial)**
- E8.3: Distribution & versioning infrastructure (this release)

[Unreleased]: https://github.com/infergen/infergen/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/infergen/infergen/releases/tag/v0.1.0
