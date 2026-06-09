# Migrate from Avo

This guide walks you through moving from Avo to Infergen. Both tools generate
type-safe tracking code, but Infergen operates entirely offline with a code-derived,
version-controlled catalog.

---

## Why migrate

| | Avo | Infergen |
|---|-----|---------|
| Event definitions | Avo workspace (cloud UI, manual) | `catalog.yaml` in your repo (auto-discovered + approved) |
| Offline operation | No (requires Avo cloud) | Full — core never calls the network |
| Version control | Avo branches (proprietary) | Git branches (standard) |
| Destinations | Via Avo destination interfaces | Any provider via config (Segment, Amplitude, PostHog, …) |
| Languages | TypeScript, other codegen targets | TS/JS, Python, Go, Ruby |
| Drift detection | Avo Inspector (runtime) | `infergen scan --check` (static, pre-merge) |
| Cost | Paid above free tier | Open-source core |

---

## Concept mapping

| Avo | Infergen |
|-----|---------|
| Avo workspace (cloud) | `.infergen/catalog.yaml` (in-repo) |
| Avo branch | Git branch |
| Avo codegen / `avo pull` | `infergen generate` |
| `Avo.init({...})` boilerplate | Handled by provider config in `infergen.config.json` |
| `Avo.EventName({...})` | `track.eventName({...})` |
| Avo destination interface | Provider plugin (built-in or custom) |
| Avo Inspector (runtime monitoring) | `infergen scan --check` (static pre-merge check) |
| Avo branches for collaboration | Standard Git PRs; catalog is a diff-friendly YAML file |

---

## Step-by-step migration

### Step 1 — Export events from Avo

In the Avo workspace, use **Export** → **JSON** to get a list of your event names,
properties, and types. This is your seed data for the Infergen catalog.

### Step 2 — Run `infergen init`

```bash
cd my-project
infergen init
```

This detects your stack, writes `infergen.config.json`, and scaffolds `.infergen/catalog.yaml`
with a few example events for your framework.

### Step 3 — Map Avo events into the catalog

For each event from the Avo export, add an entry to `.infergen/catalog.yaml` with
`status: approved`:

```yaml
# Avo event (from export):
# name: "Item Added To Cart"
# properties: itemId (string), price (float), currency (string)

# Infergen catalog entry:
- id: evt_0000000000000001
  name: item_added_to_cart
  status: approved
  description: "User adds an item to their shopping cart."
  properties:
    - name: item_id
      type: string
      required: true
      pii: false
    - name: price
      type: number
      required: true
      pii: false
    - name: currency
      type: string
      required: true
      pii: false
```

### Step 4 — Configure your analytics destinations

In Avo, destinations were wired up in the Avo UI and the generated `Avo.init(...)` call.
In Infergen, destinations are configured in `infergen.config.json`:

```json
{
  "providers": [
    {
      "name": "segment",
      "config": { "writeKey": "your-write-key" }
    }
  ]
}
```

See the [Config Reference](site/config-reference.md) for all built-in providers.

### Step 5 — Generate the typed SDK

```bash
infergen generate
```

Emits `infergen.generated.ts` (TypeScript) or the language equivalent.

### Step 6 — Replace Avo call sites

**Before (Avo):**

```ts
import Avo from './Avo';

// Avo.init call (remove entirely)
Avo.initAvo(
  { env: Avo.AvoEnv.Prod },
  {},           // System properties
  segmentDestination,
);

// Event calls
Avo.itemAddedToCart({
  itemId: 'prod-123',
  price: 19.99,
  currency: 'USD',
});
```

**After (Infergen):**

```ts
import { track } from './infergen.generated';

// No init call needed — provider config is in infergen.config.json

track.itemAddedToCart({
  item_id: 'prod-123',
  price: 19.99,
  currency: 'USD',
});
```

### Step 7 — Remove Avo initialization boilerplate

Remove:
- `Avo.initAvo(...)` calls
- Avo destination interface implementations (replaced by Infergen provider configs)
- `avo.json` / `avo.config.json` source files
- `@avo/codegen` dev dependency

---

## What you gain

- **Offline-first.** No Avo cloud dependency. Works in air-gapped environments and CI.
- **Version-controlled catalog.** `catalog.yaml` is a plain YAML file — diff it in PRs, edit it in any editor, track changes in Git.
- **Auto-discovery.** `infergen scan` finds new trackable moments in your code — you review proposals rather than manually defining everything upfront.
- **Multi-provider.** Add a second analytics destination with one config line, zero call-site changes.
- **Open-source core.** No per-seat pricing for developer tooling.

---

## What you give up (for now)

- **Avo Inspector runtime monitoring.** Avo Inspector catches issues at runtime in production. Infergen's `scan --check` is a static pre-merge check; runtime validation is on the roadmap (E8.x+).
- **Avo team governance UI.** Avo provides a cloud UI for naming reviews and team collaboration. Infergen's equivalent is Git PRs over `catalog.yaml`; a hosted control plane is planned for E9.2.

---

## CI before and after

**Before (Avo + GitHub Actions):**
```yaml
- name: Avo pull and check
  run: |
    npx avo pull
    git diff --exit-code Avo.ts
```

**After (Infergen):**
```yaml
- name: Infergen check
  run: infergen scan --check
```

`infergen scan --check` catches untracked moments, convention violations, and SDK drift —
all in one command, no network call required.
