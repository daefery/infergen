# Infergen — Roadmap

> Full-vision roadmap. Describes the **completed** product across all epics, from prototype to mature, extensible platform. Status reflects the journey: nothing built yet — everything `Planned` — but the table maps the entire arc so the end-state is visible.

**Status legend:** `Planned` · `In Progress` · `Done` · `Blocked`

---

## Milestone 0 — Foundation & Proof of Concept

Goal: validate the core bet (scan quality) on a single stack before generalizing.

| Epic No | Name | Status | Description | Note/Blocker/Dependency |
|---|---|---|---|---|
| E0.1 | Project Scaffold & Monorepo | Done | Set up the repo: CLI package, core engine, runtime SDK package, shared types, build/test/release tooling. Choose core language (Rust or Go for the CLI/scan engine for speed + single-binary distribution). | None — first work item. |
| E0.2 | CLI Skeleton & Config | Done | `infergen` CLI with `init`, `scan`, `generate`, `check`, `watch` command stubs. Config schema (`infergen.config.*`) + loader. Framework/language auto-detection on `init`. | Depends on E0.1. |
| E0.3 | TS/JS AST Parser Integration | Done | Integrate SWC/OXC to parse TypeScript/JavaScript into a normalized AST the scan engine consumes. Define the internal AST abstraction so other languages plug in later. | Depends on E0.1. Pick SWC vs OXC early. |
| E0.4 | Next.js Adapter (vertical slice) | Done | First framework adapter end-to-end: detect routes/pages, forms, auth (NextAuth), API routes → propose events. Proves the full pipeline on one stack. | Depends on E0.3. Core validation epic. |

---

## Milestone 1 — Catalog & Naming

Goal: turn raw detections into a reviewable, durable, convention-enforced tracking plan.

| Epic No | Name | Status | Description | Note/Blocker/Dependency |
|---|---|---|---|---|
| E1.1 | Event Catalog Schema & Store | Done | Define `catalog.yaml` schema: event name, trigger provenance, properties (name/type/required/PII), providers, status, confidence, stable ID. Read/write/serialize with stable, diff-friendly ordering. | Depends on E0.4. |
| E1.2 | Heuristic Event Namer | Done | Derive event names + property candidates from identifiers (components, routes, handlers). Apply naming convention. Assign confidence scores. | Depends on E1.1. |
| E1.3 | Naming Convention Engine & Linter | Done | Configurable conventions (`entity.action.state`, snake_case, etc.). Lint catalog for violations; auto-suggest fixes. Ship sensible default. | Depends on E1.2. |
| E1.4 | Property Type Inference | Done | Infer property types from AST context (function params, form fields, TS types). Flag likely PII (email, name, phone, address). | Depends on E0.3, E1.1. |
| E1.5 | Review Workflow | Done | Mark events `approved` / `ignored`; edit names/props inline. Diff view between scan result and existing catalog. | Depends on E1.1. |

---

## Milestone 2 — Codegen & Type-Safe SDK

Goal: turn the approved catalog into code developers call with full type safety.

| Epic No | Name | Status | Description | Note/Blocker/Dependency |
|---|---|---|---|---|
| E2.1 | TypeScript Codegen | Done | Generate a typed SDK from the catalog: one strongly-typed function/key per event, property shapes enforced at compile time. Deterministic, idempotent output. | Depends on E1.1. |
| E2.2 | Generated SDK Ergonomics | Done | Autocomplete-friendly API, JSDoc from catalog descriptions, tree-shakeable, framework-friendly imports. | Depends on E2.1. |
| E2.3 | Codegen Determinism & Safety | Done    | Stable output ordering, no spurious diffs, `--check` to detect stale generated code. Safe to commit or gitignore. | Depends on E2.1. |

---

## Milestone 3 — Runtime SDK & Providers

Goal: one call site, many destinations, production-grade delivery.

| Epic No | Name | Status | Description | Note/Blocker/Dependency |
|---|---|---|---|---|
| E3.1 | Provider Plugin Interface | Done    | Define the provider contract (identify, track, flush, shutdown). Plugin registration + config. | Depends on E2.1. |
| E3.2 | First-Party Provider Adapters | Done    | Implement Segment, Amplitude, Mixpanel, PostHog, GA4, RudderStack, and a generic HTTP webhook provider. | Depends on E3.1. |
| E3.2b | Database Destination Adapter | Done    | Write events directly to the project's own table (Postgres, MySQL, SQLite). Declarative event-table schema derived from the catalog + optional migration generation so the table matches event/property shapes. Server-side only. | Depends on E3.1, E1.1 (catalog drives schema). Bundle/file destination included here. |
| E3.3 | Delivery Engine | Done    | Batching, retry with backoff, persistent offline queue, sampling, flush-on-exit. Browser + Node runtimes. | Depends on E3.1. |
| E3.4 | Consent & PII Controls | Done    | Consent gating, per-property redaction hooks, opt-out, region routing. Uses PII flags from E1.4. | Depends on E3.3, E1.4. |

---

## Milestone 4 — Incremental Scans & CI

Goal: keep catalog and code in sync forever; enforce in CI.

| Epic No | Name | Status | Description | Note/Blocker/Dependency |
|---|---|---|---|---|
| E4.1 | Incremental Re-scan & Merge | Planned | Re-scan merges new proposals without clobbering manual edits. Three-way merge keyed on stable IDs. Hardest correctness problem. | Depends on E1.1, E1.5. |
| E4.2 | Drift Detection (`scan --check`) | Planned | CI mode: fail build on untracked moments, convention violations, stale generated code, or catalog drift. | Depends on E4.1, E2.3. |
| E4.3 | Watch Mode | Planned | Local file-watch → live re-scan + regenerate during development. | Depends on E4.1. |
| E4.4 | CI/CD Integrations | Planned | GitHub Action + GitLab/CircleCI recipes. PR comment summarizing new/changed events. | Depends on E4.2. |

---

## Milestone 5 — Polyglot & Framework Breadth

Goal: deliver on the "any project, any stack" promise.

| Epic No | Name | Status | Description | Note/Blocker/Dependency |
|---|---|---|---|---|
| E5.1 | Python Parser & Adapters | Planned | `ast`-based parser; adapters for Django, FastAPI, Flask. Python codegen + runtime SDK. | Depends on E0.3 (AST abstraction), E2.1. |
| E5.2 | Go Parser & Adapters | Planned | `go/ast` parser; adapters for Gin, Echo, net/http. Go codegen + runtime SDK. | Depends on E0.3, E2.1. |
| E5.3 | Ruby Parser & Adapters | Planned | Prism parser; Rails adapter (routes, controllers, Devise). Ruby codegen + runtime SDK. | Depends on E0.3, E2.1. |
| E5.4 | Additional JS Frameworks | Planned | Adapters for React Router, Vue/Nuxt, SvelteKit, Express, NestJS beyond the Next.js slice. | Depends on E0.4. |
| E5.5 | Monorepo / Polyglot Catalog | Planned | One catalog namespaced per package across mixed-language repos. Cross-service event consistency. | Depends on E5.1–E5.4. |

---

## Milestone 6 — Intelligence & Semantic Naming

Goal: lift detection quality beyond heuristics.

| Epic No | Name | Status | Description | Note/Blocker/Dependency |
|---|---|---|---|---|
| E6.1 | Local LLM Integration (Ollama) | Planned | Optional local-LLM pass to refine event names, descriptions, and property inference on low-confidence detections. Fully offline. | Depends on E1.2. Optional path — never required. |
| E6.2 | Semantic Flow Detection | Planned | Detect multi-step funnels (checkout, onboarding) by linking related events across files. | Depends on E6.1, E0.4. |
| E6.3 | Suggestion Quality Loop | Planned | Learn from user accept/reject of proposals to tune confidence and naming locally (no cloud). | Depends on E1.5, E6.1. |

---

## Milestone 7 — Developer Experience & Ecosystem

Goal: make Infergen pleasant, visible, and extensible.

| Epic No | Name | Status | Description | Note/Blocker/Dependency |
|---|---|---|---|---|
| E7.1 | Catalog Web Viewer | Planned | Local, offline web UI to browse/search the catalog, see trigger locations, providers, and PII flags. | Depends on E1.1. |
| E7.2 | Editor Integration | Planned | VS Code extension: inline "untracked moment" hints, jump-to-trigger, catalog autocomplete. | Depends on E2.1, E4.2. |
| E7.3 | Plugin SDK & Docs | Planned | Documented contracts + scaffolding for community framework adapters, language parsers, and providers. | Depends on E3.1, E0.3. |
| E7.4 | Data-Collection Manifest Export | Planned | Export catalog as a privacy/compliance manifest (what's collected, where it's sent, PII inventory). | Depends on E1.4, E3.2. |
| E7.5 | Onboarding & Templates | Planned | `init` templates per stack, example projects, getting-started docs, quickstart < 5 min. | Depends on E0.2, E0.4. |

---

## Milestone 8 — Hardening & Release

Goal: production-ready 1.0.

| Epic No | Name | Status | Description | Note/Blocker/Dependency |
|---|---|---|---|---|
| E8.1 | Performance & Caching | Planned | Parallel scanning, incremental AST cache; medium repo scan in seconds, incremental sub-second. | Depends on E4.1. |
| E8.2 | Test Suite & Fixtures | Planned | Golden-file tests per adapter, real-world fixture repos, codegen snapshot tests, runtime delivery tests. | Cross-cutting; grows with each adapter. |
| E8.3 | Distribution & Versioning | Planned | Single-binary CLI releases per platform, npm/pip/go-module runtime packages, semver + catalog schema migration. | Depends on E0.1. |
| E8.4 | Docs Site & Examples | Planned | Full documentation site, API reference, adapter gallery, migration guides from Typewriter/Avo. | Depends on E7.5. |
| E8.5 | 1.0 Launch | Planned | Stabilize APIs, finalize plugin contracts, public release. | Depends on all prior milestones. |

---

## Milestone 9 — Monetization & Commercial Layer

Goal: build the paid surfaces — team coordination, compliance, hosting — on top of the free open-core. Core developer value stays free forever; revenue comes from scale. Can run in parallel with later technical milestones, not strictly after 1.0.

| Epic No | Name | Status | Description | Note/Blocker/Dependency |
|---|---|---|---|---|
| E9.1 | Licensing & Repo Split | Planned | Apply Apache 2.0 to core (engine, catalog, codegen, runtime, adapters). Separate proprietary packages/repo for commercial layers. CLA + contribution policy. Decide BSL/Fair-Source for control plane. | Cross-cutting. Decide before commercial code exists to avoid relicensing pain. |
| E9.2 | Hosted Control Plane (SaaS) | Planned | Multi-tenant cloud service: catalog registry, versioning, audit log, cross-team governance dashboard, hosted web viewer. Auth, orgs, billing. | Depends on E1.1 (catalog), E7.1 (viewer). Primary revenue stream. |
| E9.3 | Team Catalog Sync & Governance | Planned | Push/pull catalogs across repos/services to the registry. Org-wide naming convention enforcement, shared property dictionaries, role-based approval. | Depends on E9.2, E1.3, E4.1. |
| E9.4 | Compliance / Manifest Service | Planned | Hosted, living data-collection manifest: PII inventory, destinations per event, exportable audit reports. Enterprise compliance tier. | Depends on E9.2, E7.4, E1.4. Sleeper enterprise value. |
| E9.5 | Cloud LLM Namer (usage-based) | Planned | Hosted, higher-quality semantic naming vs local Ollama. Metered billing per scan/seat. Local path stays free and first-class. | Depends on E6.1. Must never feel mandatory. |
| E9.6 | Adapter Marketplace | Planned | Listing + distribution for community and certified/premium adapters. Revenue-share for premium authors (e.g. Salesforce, Adobe Analytics). | Depends on E7.3 (plugin SDK). |
| E9.7 | Billing, Plans & Entitlements | Planned | Subscription tiers (free / team / enterprise), seat + usage metering, entitlement gating between OSS and paid features, self-serve checkout. | Depends on E9.2. |
| E9.8 | Support, SLA & Services | Planned | Enterprise support contracts, SLA tooling, priority response, bespoke adapter/integration engagements. | Depends on E9.1. Non-product revenue. |

---

### Critical Path (shortest line to a believable demo)
`E0.1 → E0.2 → E0.3 → E0.4 → E1.1 → E1.2 → E2.1 → E3.1 → E3.2`
→ scan a Next.js app, get a catalog, generate a typed SDK, fire a real event to PostHog. That slice validates the whole thesis; everything else is breadth and hardening.
