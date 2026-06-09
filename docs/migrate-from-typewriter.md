# Migrate from Segment Typewriter

This guide walks you through moving from Segment Typewriter to Infergen. Both tools
generate type-safe tracking SDKs, but Infergen adds auto-discovery, offline operation,
and multi-provider support.

---

## Why migrate

| | Typewriter | Infergen |
|---|-----------|---------|
| Event discovery | Manual (define in Segment UI) | Auto-detected from code + human-approved |
| Tracking plan | Segment cloud (locked in) | `catalog.yaml` in your repo |
| Offline operation | Partial | Full — core never calls the network |
| Destinations | Segment only | Any (Segment, Amplitude, PostHog, GA4, custom HTTP, DB, …) |
| Languages | TypeScript / JS | TS/JS, Python, Go, Ruby |
| CI enforcement | `typewriter build --check` | `infergen scan --check` (richer: drift, naming, untracked moments) |

---

## Concept mapping

| Typewriter | Infergen |
|-----------|---------|
| Tracking plan in Segment UI | `.infergen/catalog.yaml` (version-controlled in your repo) |
| `typewriter.json` source | `infergen.config.json` |
| `typewriter build` | `infergen generate` |
| `typewriter build --check` | `infergen generate --check` |
| `track.myEvent({...})` | `track.myEvent({...})` (same call pattern) |
| Segment as sole destination | Any provider(s) configured in `infergen.config.json` |
| Manual event definitions | Auto-discovered by `infergen scan` + human review |

---

## Step-by-step migration

### Step 1 — Export your Typewriter tracking plan

In the Segment UI, export your tracking plan as JSON (or note your event names and
properties). This becomes the seed for your Infergen catalog.

### Step 2 — Run `infergen init`

```bash
cd my-project
infergen init
```

This detects your stack and writes `infergen.config.json`. It also scaffolds
`.infergen/catalog.yaml` with a few example events.

### Step 3 — Add your Typewriter events to the catalog

For each event you had in Typewriter, add an entry to `.infergen/catalog.yaml` with
`status: approved`. Example:

```yaml
# Before (Typewriter source definition):
# event: "Order Completed"
# properties: orderId, total, currency

# After (infergen catalog entry):
- id: evt_0000000000000001    # run `infergen review list` to get real IDs after scan
  name: order_completed
  status: approved
  description: "User completes a purchase."
  properties:
    - name: order_id
      type: string
      required: true
      pii: false
    - name: total
      type: number
      required: true
      pii: false
    - name: currency
      type: string
      required: true
      pii: false
```

### Step 4 — Configure the Segment provider

Add Segment to `infergen.config.json`:

```json
{
  "providers": [
    {
      "name": "segment",
      "config": {
        "writeKey": "your-segment-write-key"
      }
    }
  ]
}
```

### Step 5 — Generate the new SDK

```bash
infergen generate
```

This emits `infergen.generated.ts` (TypeScript) or the language equivalent.

### Step 6 — Replace Typewriter imports at call sites

**Before (Typewriter):**

```ts
import Typewriter from './typewriter';

Typewriter.orderCompleted({
  orderId: '123',
  total: 49.99,
  currency: 'USD',
});
```

**After (Infergen):**

```ts
import { track } from './infergen.generated';

track.orderCompleted({
  order_id: '123',
  total: 49.99,
  currency: 'USD',
});
```

Note: Infergen uses `snake_case` by default (configurable via `naming.case`). If you prefer
`camelCase` to match your Typewriter schema, set `"case": "camelCase"` in `infergen.config.json`.

### Step 7 — Run `infergen scan` to discover additional events

```bash
infergen scan
```

New events in your code that weren't in your Typewriter plan will appear as `proposed`
in the catalog. Review and approve or ignore them:

```bash
infergen review list --status proposed
infergen review approve evt_...   # for genuine new events
infergen review ignore evt_...    # for false positives
```

---

## Config before and after

**Before (`typewriter.json`):**
```json
{
  "client": {
    "sdk": "typescript"
  },
  "trackingPlans": [
    {
      "name": "My Tracking Plan",
      "id": "rs_...",
      "workspaceSlug": "my-workspace"
    }
  ]
}
```

**After (`infergen.config.json`):**
```json
{
  "schemaVersion": 1,
  "catalog": ".infergen/catalog.yaml",
  "output": "infergen/generated",
  "languages": ["TypeScript"],
  "frameworks": ["NextJs"],
  "naming": { "convention": "entity.action.state", "case": "snake_case" },
  "providers": [
    { "name": "segment", "config": { "writeKey": "your-write-key" } }
  ]
}
```

---

## CI before and after

**Before:**
```yaml
- name: Check Typewriter
  run: npx typewriter build --check
```

**After:**
```yaml
- name: Infergen check
  run: infergen scan --check
```

The `infergen scan --check` command additionally detects:
- Untracked code paths (new routes/forms not in the catalog)
- Convention violations (event names not matching your naming config)
- Catalog drift (generated SDK out of date with the catalog)

---

## Keeping Segment while adding other destinations

One of Infergen's key advantages: adding a second analytics destination requires
zero call-site changes. Add it to `providers`:

```json
"providers": [
  { "name": "segment", "config": { "writeKey": "..." } },
  { "name": "posthog", "config": { "apiKey": "ph-..." } }
]
```

Both destinations receive every `track.*()` call automatically.
