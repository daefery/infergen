# Infergen Quickstart

Get from zero to a tracked event in under 5 minutes.

## Prerequisites

- Rust toolchain (`rustup.rs`) — or download a pre-built binary once available
- A project directory (Next.js, Express, Django, Go, Ruby on Rails, Vue, SvelteKit, and more)

## Step 1 — Install

```bash
cargo install infergen
```

## Step 2 — Init

```bash
cd my-project
infergen init
```

Infergen detects your stack, writes `infergen.config.json`, and scaffolds
`.infergen/catalog.yaml` with 2–3 example events appropriate for your framework.
Pass `--no-example` to skip the example catalog scaffold.

**Sample output (Next.js project):**

```
infergen: wrote infergen.config.json
languages: TypeScript
frameworks: NextJs
infergen: wrote example catalog to .infergen/catalog.yaml

Quickstart (Next.js)

  1. Run `infergen scan` to discover events in your Next.js codebase.
  2. Review `.infergen/catalog.yaml` — approve or ignore proposed events.
  3. Run `infergen generate` to emit a typed TypeScript SDK.
  4. Import and call: import { track } from './infergen.generated'; track.pageViewed({ page_path: '/' });
  5. Add `infergen scan --check` to CI to catch untracked moments before merge.
```

## Step 3 — Scan

```bash
infergen scan
```

Parses your source files and adds detected events to `.infergen/catalog.yaml` as `proposed`.

## Step 4 — Review

```bash
# List all proposed events
infergen review list

# Approve all at once
infergen review approve --all

# Or approve individually
infergen review approve evt_0123456789abcdef

# Mark a false positive as ignored
infergen review ignore evt_abc123
```

## Step 5 — Generate

```bash
infergen generate
```

Emits a typed SDK at `infergen.generated.ts` (TypeScript projects) or the language
equivalent for Python, Go, and Ruby projects.

## Step 6 — Track

Replace ad-hoc `track()` calls with the generated, type-safe SDK:

**TypeScript / Next.js / Express:**
```ts
import { track } from "./infergen.generated";

track.pageViewed({ page_path: window.location.pathname });
track.userSignupCompleted({ method: "google" });
```

**Python / Django / FastAPI / Flask:**
```python
from infergen_generated import track

track.page_viewed(page_path=request.path)
track.user_signup_completed(method="google")
```

**Go / Gin / Echo:**
```go
import infergen "github.com/your-org/your-project/infergen/generated"

infergen.Track.PageViewed("/dashboard")
infergen.Track.UserSignupCompleted("google")
```

**Ruby / Rails:**
```ruby
Infergen::Track.page_viewed(page_path: request.path)
Infergen::Track.user_signup_completed(method: "google")
```

## Step 7 — CI

Add `infergen scan --check` to your CI pipeline to catch untracked moments and catalog
drift before they reach main:

```yaml
# GitHub Actions example
- name: Infergen check
  run: infergen scan --check
```

## Re-scan after changes

Infergen merges new proposals without clobbering manual edits:

```bash
infergen scan        # re-run after adding new routes/forms
infergen review list # review only the new proposals
```

## Stack examples

- **Next.js** — [`examples/nextjs-app/`](../examples/nextjs-app/)
- **Express** — [`examples/express-app/`](../examples/express-app/)

## All supported stacks

| Language | Frameworks |
|---|---|
| TypeScript / JavaScript | Next.js, React Router, Express, NestJS |
| TypeScript / JavaScript | Vue/Nuxt, SvelteKit |
| Python | Django, FastAPI, Flask |
| Go | Gin, Echo, net/http |
| Ruby | Rails |

## Config reference

`infergen.config.json` (or `.toml`) is written by `infergen init` and can be edited manually:

```json
{
  "schemaVersion": 1,
  "catalog": ".infergen/catalog.yaml",
  "output": "infergen/generated",
  "languages": ["TypeScript"],
  "frameworks": ["NextJs"],
  "naming": {
    "convention": "entity.action.state",
    "case": "snake_case"
  },
  "providers": [
    { "name": "posthog", "config": { "apiKey": "ph-..." } }
  ]
}
```
