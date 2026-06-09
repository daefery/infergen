# Config Reference

Reference for `infergen.config.json` (or `infergen.config.toml`), written by `infergen init`
and read by `scan`, `generate`, `check`, and `watch`.

---

## Discovery

Infergen searches for config files in the project root in this precedence order:

1. `infergen.config.json`
2. `infergen.config.toml`

The first file found is used. If neither exists, commands that require config (e.g. `scan`)
will error; `init` creates it.

---

## Top-level fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `schemaVersion` | integer | `1` | Config schema version. Do not change manually. |
| `catalog` | string (path) | `".infergen/catalog.yaml"` | Path to the event catalog, relative to the project root |
| `output` | string (path) | `"infergen/generated"` | Output directory for the generated SDK, relative to project root |
| `languages` | string[] | `[]` | Source languages (auto-populated by `init`) |
| `frameworks` | string[] | `[]` | Frameworks (auto-populated by `init`) |
| `naming` | object | see below | Naming convention configuration |
| `providers` | object[] | `[]` | Analytics destination configurations |
| `llm` | object \| null | `null` | Optional LLM refinement configuration |

---

## Valid `languages` values

| Value | Description |
|-------|-------------|
| `"TypeScript"` | TypeScript source files |
| `"JavaScript"` | JavaScript source files |
| `"Python"` | Python source files |
| `"Go"` | Go source files |
| `"Ruby"` | Ruby source files |
| `"Vue"` | Vue single-file components |
| `"Svelte"` | Svelte/SvelteKit components |

---

## Valid `frameworks` values

| Value | Description |
|-------|-------------|
| `"NextJs"` | Next.js (App Router or Pages Router) |
| `"React"` | React (without Next.js) |
| `"Express"` | Express.js |
| `"NestJs"` | NestJS |
| `"Vue"` | Vue 3 / Nuxt 3 |
| `"SvelteKit"` | SvelteKit |
| `"Django"` | Django |
| `"FastApi"` | FastAPI |
| `"Flask"` | Flask |
| `"Gin"` | Gin (Go) |
| `"Echo"` | Echo (Go) |
| `"NetHttp"` | Go standard library `net/http` |
| `"Rails"` | Ruby on Rails |

---

## `naming` object

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `convention` | string | `"entity.action.state"` | Template for event name structure |
| `case` | string | `"snake_case"` | Case style applied to event names |

**Valid `naming.case` values:**

| Value | Example |
|-------|---------|
| `"snake_case"` | `user_signup_completed` |
| `"camelCase"` | `userSignupCompleted` |
| `"PascalCase"` | `UserSignupCompleted` |
| `"kebab-case"` | `user-signup-completed` |

---

## `providers` array

Each provider entry:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Provider identifier (see table below) |
| `config` | object | no | Provider-specific key-value configuration |

**Built-in providers:**

| `name` | Service | Config keys |
|--------|---------|-------------|
| `"segment"` | Segment.io | `writeKey` |
| `"amplitude"` | Amplitude | `apiKey` |
| `"mixpanel"` | Mixpanel | `token` |
| `"posthog"` | PostHog | `apiKey`, `host` (optional) |
| `"ga4"` | Google Analytics 4 | `measurementId`, `apiSecret` |
| `"rudderstack"` | RudderStack | `writeKey`, `dataPlaneUrl` |
| `"http"` | Custom HTTP endpoint | `url`, `headers` (optional) |
| `"database"` | SQL database | `url`, `table` |
| `"file"` | Local file / stdout | `path` (optional; defaults to stdout) |

---

## `llm` object (optional)

Enable LLM-assisted event name and description refinement. All fields are optional;
`provider` is the only required field when `llm` is non-null.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `provider` | string | — | `"ollama"`, `"anthropic"`, `"openai"`, or `"openai_compatible"` |
| `model` | string | provider default | Model name/identifier |
| `apiKey` | string | env var | API key (prefer env var `ANTHROPIC_API_KEY` / `OPENAI_API_KEY`) |
| `baseUrl` | string | provider default | API base URL (for `openai_compatible` endpoints) |
| `maxTokens` | integer | 1024 | Maximum tokens per refinement call |

---

## Full example (JSON)

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
    {
      "name": "posthog",
      "config": {
        "apiKey": "ph-proj-...",
        "host": "https://app.posthog.com"
      }
    },
    {
      "name": "file",
      "config": {}
    }
  ],
  "llm": {
    "provider": "ollama",
    "model": "llama3.2"
  }
}
```

## Full example (TOML)

```toml
schemaVersion = 1
catalog = ".infergen/catalog.yaml"
output = "infergen/generated"
languages = ["TypeScript"]
frameworks = ["NextJs"]

[naming]
convention = "entity.action.state"
case = "snake_case"

[[providers]]
name = "posthog"
[providers.config]
apiKey = "ph-proj-..."
host = "https://app.posthog.com"

[llm]
provider = "ollama"
model = "llama3.2"
```
