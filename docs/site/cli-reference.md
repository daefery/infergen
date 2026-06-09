# CLI Reference

Complete reference for the `infergen` command-line tool.

---

## Global flags

```
infergen [--version] [--help] <command>
```

| Flag | Description |
|------|-------------|
| `--version` | Print version and exit |
| `--help` | Print help for any command |

---

## `infergen init`

Detect languages and frameworks in a project directory and write `infergen.config.json`
(or `.toml`). Also scaffolds `.infergen/catalog.yaml` with stack-appropriate example events.

```bash
infergen init [OPTIONS]
```

**Flags:**

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--dir` | path | `.` | Project directory to initialize |
| `--format` | `json` \| `toml` | `json` | Config file format to write |
| `--force` | bool | false | Overwrite an existing config file |
| `--no-example` | bool | false | Skip writing the example `.infergen/catalog.yaml` |

**Exit codes:** 0 success · 1 error (e.g. no write permission, dir not found)

**Examples:**

```bash
# Initialize current directory (auto-detect stack)
infergen init

# Initialize a specific directory
infergen init --dir ./my-project

# Write TOML config instead of JSON
infergen init --format toml

# Force overwrite if config already exists
infergen init --force

# Skip example catalog scaffold
infergen init --no-example
```

---

## `infergen scan`

Parse the project's source files and propose new events in `.infergen/catalog.yaml`.
Re-scanning merges new proposals without overwriting manual edits.

```bash
infergen scan
```

No flags in current release. Reads `infergen.config.*` from the current directory.

**Exit codes:** 0 success · 1 error

**Examples:**

```bash
# Scan and update the catalog
infergen scan

# After adding new routes, re-scan to discover them
infergen scan
infergen review list --status proposed
```

---

## `infergen generate`

Generate a type-safe SDK from the approved catalog. Output language is determined by
`languages` in the config.

```bash
infergen generate [OPTIONS]
```

**Flags:**

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--catalog` | path | `.infergen/catalog.yaml` | Path to the catalog file |
| `--output` | path | `infergen.generated.ts` | Output file path |
| `--include-proposed` | bool | false | Also generate code for `proposed` events (in addition to `approved`) |
| `--check` | bool | false | Check if output is up to date; exit non-zero if stale. Does not write. |

**Exit codes:** 0 success · 1 error · 2 (`--check` mode: output is stale)

**Examples:**

```bash
# Generate typed SDK
infergen generate

# Generate to a custom path
infergen generate --output src/analytics/generated.ts

# CI: fail if generated file is stale
infergen generate --check

# Include proposed events (useful during development)
infergen generate --include-proposed
```

---

## `infergen check`

CI mode: scan the project and fail if any untracked moments, unreviewed events, convention
violations, or catalog drift are detected.

```bash
infergen check [OPTIONS]
```

**Flags:**

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--catalog` | path | from config | Path to the catalog file |
| `--json` | bool | false | Output check result as JSON instead of human-readable text |

**Exit codes:** 0 clean · 1 error · 2 violations detected

**Examples:**

```bash
# Run CI check
infergen check

# Output as JSON (for programmatic parsing)
infergen check --json

# Use a specific catalog
infergen check --catalog path/to/catalog.yaml
```

---

## `infergen watch`

Watch source files for changes; re-scan and regenerate the SDK automatically.

```bash
infergen watch [OPTIONS]
```

**Flags:**

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--once` | bool | false | Run one scan+generate cycle then exit (no file watching) |
| `--output` | path | `infergen.generated.ts` | Output file path for the generated SDK |

**Exit codes:** 0 (after `--once`) · 1 error

**Examples:**

```bash
# Start file watcher
infergen watch

# Run one cycle and exit (useful for scripting)
infergen watch --once
```

---

## `infergen review`

Review and annotate the event catalog. All subcommands operate on the catalog at
`--catalog` (default: `.infergen/catalog.yaml`).

```bash
infergen review [--catalog <path>] <subcommand>
```

### `infergen review list`

List catalog events, optionally filtered by status.

```bash
infergen review list [--status <status>]
```

| Flag | Values | Default | Description |
|------|--------|---------|-------------|
| `--status` | `all` \| `proposed` \| `approved` \| `ignored` | `all` | Filter by event status |

**Examples:**

```bash
infergen review list
infergen review list --status proposed
infergen review list --status approved
```

---

### `infergen review approve`

Approve an event by stable ID, or approve all proposed events at once.

```bash
infergen review approve [<id>] [--all]
```

| Argument | Description |
|----------|-------------|
| `<id>` | Stable event ID (e.g. `evt_0123456789abcdef`) |
| `--all` | Approve every event in `proposed` status |

**Examples:**

```bash
# Approve a single event
infergen review approve evt_0123456789abcdef

# Approve everything
infergen review approve --all
```

---

### `infergen review ignore`

Mark an event as `ignored` (false positive; won't appear in the generated SDK).

```bash
infergen review ignore <id>
```

**Example:**

```bash
infergen review ignore evt_0123456789abcdef
```

---

### `infergen review rename`

Rename an event. Convention validation is applied to the new name.

```bash
infergen review rename <id> <new_name>
```

**Example:**

```bash
infergen review rename evt_0123456789abcdef user_signup_completed
```

---

### `infergen review describe`

Set the human-readable description for an event.

```bash
infergen review describe <id> <description>
```

**Example:**

```bash
infergen review describe evt_0123456789abcdef "User completes the onboarding signup flow."
```

---

### `infergen review diff`

Show the diff between a newly-proposed catalog and the existing catalog. Useful to
preview what `infergen scan` would add.

```bash
infergen review diff <proposed>
```

**Example:**

```bash
# Diff a proposed catalog against the current one
infergen review diff .infergen/catalog.proposed.yaml
```

---

## `infergen view`

Generate an offline HTML catalog viewer and (optionally) open it in the default browser.

```bash
infergen view [OPTIONS]
```

**Flags:**

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--catalog` | path | `.infergen/catalog.yaml` | Path to the catalog file |
| `--output` | path | `catalog-viewer.html` (same dir as catalog) | Output HTML file path |
| `--no-open` | bool | false | Do not auto-open in the browser |

**Exit codes:** 0 success · 1 error

**Examples:**

```bash
# Generate and open the viewer
infergen view

# Generate without opening
infergen view --no-open

# Custom output path
infergen view --output /tmp/my-catalog.html
```

---

## `infergen plugin`

Scaffold and describe plugin extension points for custom adapters, parsers, and providers.

```bash
infergen plugin <subcommand>
```

### `infergen plugin scaffold`

Generate a ready-to-compile Rust skeleton for a new plugin. Prints to stdout or writes
to a file.

```bash
infergen plugin scaffold <kind> <name> [OPTIONS]
```

| Argument | Values | Description |
|----------|--------|-------------|
| `<kind>` | `provider` \| `adapter` \| `parser` | Plugin type |
| `<name>` | string | Kebab-case plugin name (e.g. `my-provider`) |

| Flag | Description |
|------|-------------|
| `--framework <name>` | Framework name (required for `adapter` kind) |
| `--output/-o <path>` | Write to file instead of stdout |

**Examples:**

```bash
# Analytics provider scaffold
infergen plugin scaffold provider my-provider

# Framework adapter scaffold
infergen plugin scaffold adapter my-adapter --framework htmx

# Language parser scaffold
infergen plugin scaffold parser lua

# Write to file
infergen plugin scaffold provider my-provider --output src/my_provider.rs
```

---

### `infergen plugin list-types`

List all available plugin types and their trait contracts.

```bash
infergen plugin list-types
```

---

## `infergen manifest`

Export the event catalog as a privacy/compliance manifest — showing what data is collected,
where it's sent, and which properties are PII.

```bash
infergen manifest [OPTIONS]
```

**Flags:**

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--catalog` | path | from config | Path to the catalog file |
| `--output/-o` | path | stdout | Write output to a file |
| `--format` | `json` \| `yaml` \| `markdown` | `json` | Output format |

**Exit codes:** 0 success · 1 error

**Examples:**

```bash
# JSON manifest to stdout
infergen manifest

# Markdown audit report to file
infergen manifest --format markdown --output data-manifest.md

# YAML manifest
infergen manifest --format yaml --output manifest.yaml
```
