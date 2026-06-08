# Catalog Schema Reference

Reference for `.infergen/catalog.yaml` — the version-controlled event catalog that is
the source of truth for your analytics tracking plan.

---

## Overview

The catalog lives at `.infergen/catalog.yaml` (configurable via `catalog` in
`infergen.config.*`). It is:

- Written/updated by `infergen scan`
- Reviewed and annotated by `infergen review`
- Read by `infergen generate`, `infergen check`, and `infergen view`
- Exported by `infergen manifest`
- Safe to commit — it is diff-friendly YAML, sorted deterministically

**Do not delete or rename** the `id` field of existing events. Stable IDs are how
Infergen tracks events across re-scans while preserving manual edits.

---

## Top-level fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `schemaVersion` | integer | yes | Must be `1` |
| `events` | object[] | yes | List of catalog events |

---

## Event fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | yes | Stable unique ID (`evt_` + 16 hex chars). Set by Infergen; do not change. |
| `name` | string | yes | Event name (validated against the convention engine) |
| `status` | enum | yes | `proposed` / `approved` / `ignored` |
| `confidence` | float (0–1) | no | Detection confidence score from the scanner |
| `description` | string | no | Human-readable event description |
| `adapter` | string | no | Source adapter that detected this event (e.g. `nextjs`, `django`) |
| `properties` | object[] | no | Event properties (see below) |
| `provenance` | object | no | Source file and line where the event was detected |
| `flows` | string[] | no | IDs of multi-step flows this event belongs to |

---

## Event status lifecycle

```
           infergen scan
                │
                ▼
           [proposed]  ─── review approve ──▶  [approved]  ─── included in SDK
                │
           review ignore
                │
                ▼
           [ignored]   ─── not included in SDK, not re-proposed on re-scan
```

- **`proposed`** — detected by the scanner, awaiting human review
- **`approved`** — human-confirmed; included in the generated SDK
- **`ignored`** — marked as false positive; excluded from SDK; not re-proposed

---

## Property fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Property name |
| `type` | enum | yes | `string` / `number` / `boolean` / `object` / `array` |
| `required` | bool | no | Whether the property is required at the call site |
| `pii` | bool | no | Whether the property may contain personally identifiable information |

---

## `provenance` object

| Field | Type | Description |
|-------|------|-------------|
| `file` | string | Relative path to the source file |
| `line` | integer | Line number where the trackable moment was detected |
| `kind` | string | Detection kind (e.g. `route`, `form`, `auth`, `button`, `api_call`) |

---

## Annotated full example

```yaml
schemaVersion: 1
events:
  # Approved event — will be included in the generated SDK.
  - id: evt_a1b2c3d4e5f67890
    name: user_signup_completed
    status: approved
    confidence: 0.85
    description: "User completes the signup flow."
    adapter: nextjs
    properties:
      - name: method
        type: string
        required: true
        pii: false
      - name: email
        type: string
        required: false
        pii: true            # <-- PII flag; consent gating applies
    provenance:
      file: app/signup/page.tsx
      line: 42
      kind: form

  # Proposed event — awaiting review.
  - id: evt_0011223344556677
    name: button_clicked
    status: proposed
    confidence: 0.6
    description: "User clicks an interactive button."
    adapter: nextjs
    properties:
      - name: label
        type: string
        required: false
        pii: false
    provenance:
      file: components/hero/CTA.tsx
      line: 18
      kind: button

  # Ignored event — false positive, excluded from SDK.
  - id: evt_aabbccddeeff0011
    name: test_button_clicked
    status: ignored
    confidence: 0.55
    adapter: nextjs
    properties: []
```

---

## Working with the catalog

```bash
# List all events
infergen review list

# Approve a specific event
infergen review approve evt_a1b2c3d4e5f67890

# Approve everything (good for first-run)
infergen review approve --all

# Mark a false positive
infergen review ignore evt_aabbccddeeff0011

# Rename an event
infergen review rename evt_0011223344556677 cta_button_clicked

# Add a description
infergen review describe evt_0011223344556677 "User clicks the hero CTA button."
```

---

## Stable IDs and re-scans

Infergen assigns each event a stable 16-character hex ID (`evt_xxxxxxxxxxxxxxxx`) on first
detection. On re-scan, events are matched by ID: new detections are appended as `proposed`,
existing events retain their status and manual edits. **Do not change `id` values.**

If you rename an event via `infergen review rename`, the ID stays constant — history and
manual edits are preserved.
