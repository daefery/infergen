# Infergen — Product Requirements Document

> **One-liner:** A framework-agnostic library that scans any codebase offline, infers a typed analytics event catalog from project context, and generates a type-safe, multi-provider telemetry SDK — eliminating manual event planning, naming drift, and provider lock-in.

---

## 1. Background & Problem

Every product team that wants analytics repeats the same expensive, error-prone setup:

1. **Event design** — Someone decides *what* to track, invents event names, and lists properties. This lives in a spreadsheet or a Notion doc that immediately rots.
2. **Naming chaos** — `signup`, `Sign Up`, `user_signed_up`, `SIGNUP_COMPLETE` all coexist. No enforced convention. Dashboards become unusable.
3. **Manual wiring** — Developers hand-write `track('...', {...})` calls scattered across the code, with no type safety. Typos and property mismatches ship to production silently.
4. **Provider lock-in** — Switching from Mixpanel to PostHog (or adding a second provider) means rewriting every call site.
5. **Drift** — The "tracking plan" and the actual code diverge over time. Nobody knows what's *really* being tracked.

Existing tools (Avo, Segment Typewriter, Segment Protocols, RudderStack) solve *parts* of this but all require **manual upfront event planning**, are mostly **cloud/SaaS-dependent**, and are usually **single-ecosystem** (JS-only or Segment-only).

**The gap:** No tool *reads the project you already wrote* and proposes the tracking plan for you, offline, across any language/framework.

---

## 2. Vision

> Run one command in any repo. Infergen reads your routes, forms, auth flows, API endpoints, and error boundaries, then hands you a reviewable, version-controlled event catalog and a generated type-safe SDK wired to the providers you choose. Edit the catalog, regenerate, ship. Your tracking plan can never drift from your code again — because it's *derived* from your code.

**Principles:**
- **Offline-first.** Core scanning, naming, and codegen run with zero network calls. No data leaves the machine. Optional local LLM (Ollama) for semantic naming, never a required cloud API.
- **Code is the source of truth.** The catalog is generated *from* the codebase, not maintained beside it.
- **Human-in-the-loop.** Auto-detection proposes; humans approve. Manual edits survive re-scans.
- **Polyglot & framework-agnostic.** FE, BE, or fullstack. JS/TS, Python, Go, Ruby, and beyond via a pluggable adapter system.
- **Provider-neutral.** One call site, many destinations. Swap providers via config.

---

## 3. Goals & Non-Goals

### Goals
- Auto-discover trackable moments from source code with useful (not perfect) accuracy.
- Produce a human-reviewable, diff-friendly, version-controlled event catalog.
- Generate type-safe tracking SDKs in the project's language(s).
- Abstract multiple analytics providers behind one runtime interface with batching, retry, and offline queueing.
- Support incremental re-scans that merge new discoveries without clobbering manual edits.

### Non-Goals (v1)
- Not a dashboard / analytics visualization product. We *feed* providers; we don't replace them.
- Not a server-side data warehouse or ETL pipeline (RudderStack territory).
- Not a session-replay or heatmap tool.
- Not attempting 100% auto-detection accuracy — the review loop is a feature, not a bug.
- No real-time runtime auto-instrumentation (monkey-patching). v1 is scan-time codegen, not runtime magic.

---

## 4. Target Users & Personas

| Persona | Pain | What Infergen gives them |
|---|---|---|
| **Solo dev / indie hacker** | No time to design a tracking plan; wants analytics "now" | One command → working, sensible events |
| **Frontend engineer** | Hates writing/maintaining `track()` calls by hand | Generated typed SDK, autocomplete, compile-time safety |
| **Data / Analytics engineer** | Fights naming drift and undocumented events | Single source-of-truth catalog, enforced conventions |
| **Eng lead / Platform team** | Wants consistency across many repos/services | Shared convention config, CI enforcement |
| **Privacy / Compliance officer** | Needs to know what's collected & where it goes | Catalog doubles as a data-collection manifest; PII flags |

---

## 5. User Journey (Happy Path)

1. `npx infergen init` — detects frameworks, languages, monorepo layout. Writes a `infergen.config.*`.
2. `infergen scan` — AST-parses the project, runs framework adapters + heuristic namer, proposes events.
3. Infergen writes `.infergen/catalog.yaml` — the reviewable tracking plan.
4. Dev reviews the diff, edits names/properties/providers, marks false positives as `ignored`.
5. `infergen generate` — emits a type-safe SDK + provider bindings into the project.
6. Dev replaces ad-hoc tracking with generated, autocompleted calls: `track.userSignupCompleted({ method: 'google' })`.
7. CI runs `infergen scan --check` — fails the build if code introduces untracked moments or drifts from the catalog.
8. Later: re-run `scan`. New events merge in as proposals; manual edits are preserved.

---

## 6. Functional Requirements

### 6.1 Discovery / Scanning
- Parse source via language-native AST (TS/JS via SWC/OXC, Python via `ast`, Go via `go/ast`, Ruby via Prism).
- Framework adapters recognize idioms:
  - **Routes/pages** → page-view / navigation events (Next.js, React Router, Vue Router, Express, FastAPI, Django, Rails, Gin).
  - **Forms & handlers** → submit/validation events; field names → property candidates.
  - **Auth flows** → login/logout/signup/session events (NextAuth, Passport, Clerk, Auth0, Devise).
  - **API endpoints** → request/response/error events.
  - **Payment/checkout** → funnel events (Stripe, etc.).
  - **Error boundaries / try-catch** → error events.
  - **Feature flags** → experiment exposure events.
- Heuristic event namer derives names from component/route/function identifiers.
- Optional local-LLM pass refines names/descriptions/property inference when heuristics are low-confidence.
- Confidence score per proposed event.

### 6.2 Catalog
- Human-readable, diff-friendly format (YAML/JSON) committed to the repo.
- Per event: stable name, trigger location(s), properties (name+type+required+PII flag), target providers, description, status (`proposed` / `approved` / `ignored`), confidence, source provenance.
- Enforced naming convention (configurable: `entity.action.state`, `snake_case`, etc.) with a linter.
- Stable IDs so renames don't lose history.

### 6.3 Codegen
- Generate typed SDK in the project's language(s) from the approved catalog.
- Compile-time safety: wrong event name or property shape = type error.
- Idempotent, deterministic output; safe to commit or `.gitignore`.

### 6.4 Runtime SDK & Destinations
- The generated `track()` is a real function shipped in the user's project (imported module on the frontend; injected service/singleton on the backend). A single call fans out to one or more configured destinations with **zero call-site changes** to add, remove, or swap a destination.
- Destination types (pluggable adapters):
  - **Analytics providers** — Segment, Amplitude, Mixpanel, PostHog, GA4, RudderStack.
  - **Custom HTTP** — POST events to any endpoint/collector (local or remote).
  - **Database table** — write events directly to the project's own table (Postgres, MySQL, SQLite, etc.) via a DB adapter. Server-side only. Includes a declarative event-table schema and optional migration generation so the destination table matches the catalog's event/property shape.
  - **File / stdout** — local debugging, append-to-log.
- Same `track()` call writes to all configured destinations in parallel.
- Batching, retry with backoff, offline persistent queue, sampling, consent gating, PII redaction hooks — applied uniformly regardless of destination.
- Works in browser, Node, and other server runtimes. DB and file destinations are restricted to server runtimes.

### 6.5 Incremental / CI
- Re-scan merges new proposals; preserves manual edits (three-way merge on stable IDs).
- `--check` mode for CI: detect drift, untracked moments, convention violations.
- Watch mode for local dev.

---

## 7. Non-Functional Requirements
- **Offline:** Core path requires no network. LLM optional and local.
- **Performance:** Scan a medium repo (~100k LOC) in seconds, not minutes. Incremental scans sub-second where possible.
- **Privacy:** No telemetry-about-telemetry by default. Source never transmitted.
- **Extensibility:** Adding a framework adapter, language parser, or provider is a documented plugin contract.
- **Portability:** Single distributable binary for the CLI (Rust/Go core favored) + thin language-native runtime packages.

---

## 8. Proposed Architecture

```
┌─────────────────────────────────────────────────────────┐
│ CLI  (init · scan · generate · check · watch)            │
└───────────────┬─────────────────────────────────────────┘
                │
   ┌────────────┴────────────┐
   │  Scan Engine            │
   │  ├─ Language Parsers    │  TS/JS · Python · Go · Ruby
   │  ├─ Framework Adapters  │  Next · Express · Django · Rails …
   │  ├─ Heuristic Namer     │
   │  └─ Optional Local LLM  │  (Ollama)
   └────────────┬────────────┘
                │ proposes
        ┌───────▼────────┐
        │  catalog.yaml   │  human-reviewed · version-controlled
        └───────┬────────┘
                │
        ┌───────▼────────┐
        │  Codegen        │  catalog → typed SDK (per language)
        └───────┬────────┘
                │
        ┌───────▼──────────────────────────────┐
        │  Runtime SDK                          │
        │  ├─ Provider Plugins (Segment/Amp/…)  │
        │  └─ Queue · Batch · Retry · Consent   │
        └───────────────────────────────────────┘
```

---

## 9. Differentiation

| | Manual planning | Auto-discovery | Offline | Multi-framework | Multi-provider | Type-safe |
|---|---|---|---|---|---|---|
| Segment Typewriter | required | ✗ | partial | JS-centric | Segment only | ✓ |
| Avo | required | ✗ | ✗ | limited | via Segment | ✓ |
| RudderStack | required | ✗ | ✗ | ✓ | ✓ | partial |
| **Infergen** | **optional** | **✓** | **✓** | **✓** | **✓** | **✓** |

**Core bet:** scan quality. If auto-detection produces a catalog people actually keep, everything else follows.

---

## 10. Risks & Open Questions
- **Detection accuracy** — heuristics over/under-generate. Mitigation: confidence scores + mandatory review + LLM assist.
- **Naming convention** — which default? Make configurable; ship a sensible default (`entity.action.state`).
- **Polyglot monorepos** — one catalog or per-package? Lean: one catalog, namespaced by package.
- **Merge correctness** — preserving manual edits across re-scans is the hardest engineering problem. Stable IDs + three-way merge.
- **Adapter sprawl** — maintaining many frameworks. Mitigation: strong plugin contract, community adapters.
- **Runtime vs scan-time** — v1 is scan-time codegen. Runtime auto-capture is a later, riskier bet.

---

## 11. Monetization & Licensing

Infergen is **open-source first**. Adoption is the moat: every solo developer and single-repo use case must be free, forever, with no friction. Revenue comes from **multi-team coordination, compliance, and hosting** — the surfaces where individuals feel no pain but organizations pay willingly.

### 11.1 Model: Open-Core + Hosted Control Plane

The full single-developer / single-repo value is free and permissively licensed. We monetize the layers that only matter at team and enterprise scale.

| Layer | License | Monetization |
|---|---|---|
| Scan engine, language parsers, heuristic namer | Apache 2.0 | Free — drives adoption |
| Catalog format, codegen, runtime SDK, provider adapters | Apache 2.0 | Free — must be free or nobody adopts |
| Local LLM namer (Ollama) | Free | Free — preserves offline promise |
| **Team catalog registry + naming governance** | Proprietary | SaaS subscription |
| **Drift dashboards, PR bots, org-wide convention enforcement** | Proprietary | SaaS subscription |
| **Compliance / data-collection manifest service** | Proprietary | SaaS / enterprise tier |
| **Cloud LLM namer (higher quality than local)** | Proprietary | Usage-based |
| **Premium / certified provider & framework adapters** | Proprietary or revenue-share | Marketplace |
| **Support, SLA, custom adapter development** | Commercial contract | Services |

**Guiding line:** if a feature helps one developer ship analytics, it is free. If it coordinates many developers, satisfies auditors, or removes ops burden, it is paid.

### 11.2 Revenue Streams (ranked by fit)

1. **Hosted control plane (SaaS)** — most reliable, recurring. Catalog registry, versioning, audit log, cross-team governance dashboard, hosted multi-tenant web viewer. Self-host stays free; hosting = convenience + collaboration.
2. **Compliance tier (enterprise)** — the sleeper. The catalog doubles as an auditable manifest: *what PII is collected, where it is sent*. Living, code-derived compliance documentation is something enterprises pay real money for.
3. **Cloud LLM namer (usage-based)** — higher-quality semantic naming than local Ollama, billed per scan or per seat. Local path always remains free.
4. **Marketplace (revenue-share)** — community adapters free; certified/premium adapters (e.g. Salesforce, Adobe Analytics) revenue-shared with authors.
5. **Support & services (commercial)** — SLAs, priority response, and bespoke adapter/integration work for enterprise.

### 11.3 Licensing Strategy

- **Core:** Apache 2.0 (permissive — maximizes adoption and enterprise comfort).
- **Proprietary layers:** closed, sold via subscription/enterprise contracts.
- **Alternative considered:** BSL / Fair-Source for parts of the control plane (source-available, free below a revenue/seat threshold, converts to Apache after N years) — defers the open-core boundary fight while still protecting commercial surfaces. Decision deferred until the commercial layers exist.
- **Avoid** AGPL on the core: scares the enterprise adopters we most want.

### 11.4 Risks

- **Open-core boundary** — drawing the free/paid line wrong drives community forks or yields no revenue. Mitigation: keep the boundary at *team coordination*, never at core developer value.
- **Hosted-vs-self-host parity** — self-host of paid features would gut SaaS revenue. Mitigation: paid features are inherently multi-tenant / managed.
- **Offline promise vs cloud LLM** — must never make the cloud namer feel mandatory. Local Ollama path stays first-class and free.

---

## 12. Success Metrics
- **Time-to-first-event:** < 5 minutes from `init` to a tracked event firing.
- **Catalog retention:** % of auto-proposed events kept (target > 60% after review).
- **Drift caught:** untracked moments flagged in CI before merge.
- **Provider swaps:** zero call-site changes to add/swap a provider.
- **Adoption breadth:** supported frameworks × languages × providers.
- **Conversion:** free → paid team/enterprise conversion rate; net revenue retention.

---

## 13. Phased Strategy (summary)
Prove scan quality on **one stack first (Next.js + TypeScript)** end-to-end before generalizing. Validate that the generated catalog is something a real team keeps and edits — then expand languages, frameworks, providers, and the CI/merge story. Full epic breakdown in [ROADMAP.md](./ROADMAP.md).
