# Infergen Documentation

**Infergen** scans any codebase offline, infers a typed analytics event catalog from project
context, and generates a type-safe, multi-provider telemetry SDK — eliminating manual event
planning, naming drift, and provider lock-in.

---

## Core principles

- **Offline-first.** Core scanning, naming, and codegen run with zero network calls. Source never leaves your machine.
- **Code is the source of truth.** The catalog is generated *from* the codebase, not maintained beside it.
- **Human-in-the-loop.** Auto-detection proposes; humans approve. Manual edits survive re-scans.
- **Provider-neutral.** One call site, many destinations. Swap providers via config.

---

## Navigation

| Doc | What it covers |
|-----|---------------|
| [Quickstart](../quickstart.md) | From zero to a tracked event in under 5 minutes |
| [CLI Reference](cli-reference.md) | All commands and flags |
| [Config Reference](config-reference.md) | `infergen.config.json` / `.toml` schema |
| [Catalog Schema](catalog-schema.md) | `.infergen/catalog.yaml` format and fields |
| [Adapter Gallery](adapter-gallery.md) | All 13 supported stacks and what they detect |
| [Plugin SDK](../plugin-sdk.md) | Add custom adapters, parsers, and providers |
| [CI Recipes](../ci-recipes/) | GitHub Actions, GitLab CI, CircleCI |
| [Migrate from Typewriter](../migrate-from-typewriter.md) | Segment Typewriter → Infergen |
| [Migrate from Avo](../migrate-from-avo.md) | Avo → Infergen |

---

## Example projects

| Stack | Path |
|-------|------|
| Next.js (TypeScript) | [`examples/nextjs-app/`](../../examples/nextjs-app/) |
| Express (TypeScript) | [`examples/express-app/`](../../examples/express-app/) |
| Django (Python) | [`examples/django-app/`](../../examples/django-app/) |
| Rails (Ruby) | [`examples/rails-app/`](../../examples/rails-app/) |

---

## Supported stacks

| Language | Framework | Adapter |
|----------|-----------|---------|
| TypeScript / JavaScript | Next.js 13+ | `nextjs` |
| TypeScript / JavaScript | React Router v6+ | `react_router` |
| TypeScript / JavaScript | Express 4.x | `express` |
| TypeScript / JavaScript | NestJS 10+ | `nestjs` |
| TypeScript / JavaScript | Vue 3 / Nuxt 3 | `vue` |
| TypeScript / JavaScript | SvelteKit 2+ | `sveltekit` |
| Python | Django 4.x | `django` |
| Python | FastAPI 0.100+ | `fastapi` |
| Python | Flask 3.x | `flask` |
| Go | Gin 1.9+ | `gin` |
| Go | Echo 4.x | `echo` |
| Go | `net/http` (stdlib) | `nethttp` |
| Ruby | Rails 7+ | `rails` |

---

## Architecture overview

```
┌─────────────────────────────────────────────────────────┐
│ CLI  (init · scan · generate · check · watch · view · …) │
└───────────────┬─────────────────────────────────────────┘
                │
   ┌────────────┴────────────┐
   │  Scan Engine            │
   │  ├─ Language Parsers    │  TS/JS · Python · Go · Ruby
   │  ├─ Framework Adapters  │  Next · Express · Django · Rails …
   │  ├─ Heuristic Namer     │
   │  └─ Optional Local LLM  │  (Ollama / Claude / OpenAI)
   └────────────┬────────────┘
                │ proposes
        ┌───────▼────────┐
        │  catalog.yaml  │  human-reviewed · version-controlled
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
