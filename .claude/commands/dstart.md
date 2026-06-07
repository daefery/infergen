---
description: Full automated pipeline: plan → implement → QA + commit/push/PR for an Infergen epic in one call
argument-hint: <epic e.g. E0.1 | milestone e.g. M0>
model: sonnet
allowed-tools: Read, Write, Edit, Bash, Glob, Grep
---

# /dstart — Full Pipeline (Plan → Implement → QA → PR)

## Step 0 — Model check (REQUIRED before anything else)

This pipeline uses **different models per phase**:

| Phase | Required model | Switch command |
|-------|---------------|----------------|
| Phase 1 — Plan | **Opus** | `/model opus` |
| Phase 2 — Implement | **Sonnet** | `/model sonnet` |
| Phase 3 — QA + PR | **Sonnet** | `/model sonnet` |

Before starting Phase 1:
1. Run `/model` to see the active model.
2. If it is not `opus`, run `/model opus` and wait for confirmation.
3. Only begin planning once confirmed on **Opus**.

Before starting Phase 2 (after plan gate passes):
1. Run `/model sonnet` and wait for confirmation.
2. State "Switched to Sonnet — beginning implementation."

Phase 3 stays on Sonnet (no switch needed after Phase 2).

---

Runs the full Infergen development pipeline in one command:
**`/dplan` → `/dapprove` → `/dqa`** — gated at each transition.
Commit, push, and PR only happen if QA reaches GREEN.

## Input

Target: **$ARGUMENTS**

Epic (e.g. `E0.1`) or Milestone (e.g. `M0`). If empty, stop and ask.

---

## Phase 1 — PLAN (follows /dplan rules exactly)

### 1.1 — Scan the ROADMAP

1. Read `ROADMAP.md` in full.
2. Locate the target (epic row or full milestone section). Capture Name, Description, Status, Dependencies.
3. For every listed dependency epic, note whether it is `Done`. If a hard dependency is not `Done`, flag **⚠ Blocked-by** at the top of the plan — but continue.
4. If target not found, stop and list available epic/milestone IDs.

### 1.2 — Read PRD

Read `PRD.md`. Pull sections relevant to the target (§6 functional requirements, §7 non-functional, §8 architecture, and anything the epic touches). Ground every design choice in PRD requirement IDs.

### 1.3 — Survey existing code

Glob/Grep/Read any code the epic builds on or modifies. The plan reflects reality on disk, not assumptions.

### 1.4 — Author the plan

Output path: `docs/<epic-id-lowercased>-<kebab-name>-plan.md`

For each file in build/sequence order:
- **Path** — exact target path.
- **Purpose** — one line.
- **Detailed spec** — exported symbols, signatures, key logic, edge cases, error handling.
- **Dependencies** — other files/packages.
- **Tests** — concrete test cases + fixture notes.
- **Score** — `NN%` (see rubric below).

Plan header: epic ID, name, source milestone, ⚠ Blocked-by warnings, PRD/ROADMAP references, build/sequence order, open questions/risks.

### 1.5 — Score and auto-review (gate: ≥ 96%)

Rubric (weighted):

| Dimension | Weight |
|-----------|--------|
| Completeness | 30% |
| Correctness | 25% |
| Implementability | 20% |
| Testability | 15% |
| Clarity | 10% |

Auto-review loop (max 5 iterations): score every file → revise any `< 96%` → re-score → repeat until all ≥ 96%. Files still below gate after 5 passes: mark `⚠ BELOW GATE` with explanation.

Embed in the plan file:
- `## Scorecard` table (each file → score, overall plan score).
- Per-file **Score: NN%** line.
- Review summary (iterations run, gate status).

### 1.6 — Gate check before Phase 2

If **any file is `⚠ BELOW GATE`** or **Overall Plan Score < 96%**: stop, print the scorecard, and tell the user which files failed. Do not proceed to implementation.

If gate passes: state "Phase 1 complete — plan at `<path>`, score NN%. Proceeding to implementation."

---

## Phase 2 — IMPLEMENT (follows /dapprove rules exactly)

### 2.1 — Create the working branch (BEFORE any code changes)

Derive branch name from the plan filename: `epic/<epic-id-lowercased>-<kebab-name>`.

- **Not a git repo**: `git init` → `git checkout -b <branch>`. Add `.gitignore` as first file.
- **Is a git repo**: `git checkout main && git pull origin main` → `git checkout <branch>` (if exists) or `git checkout -b <branch>`.

Confirm branch with `git branch --show-current`. If branch can't be created, stop — do not implement on default branch.

### 2.2 — Implement

Work the plan's build/sequence order, file by file:
1. Create/edit each file exactly to spec.
2. Write the tests the plan specifies for that file.
3. Run build/lint/test after each meaningful unit; fix failures before moving on.
4. If reality diverges from plan, **stop and report the conflict** — do not improvise.

Match existing code style. Do not add files the plan didn't call for.

### 2.3 — Update ROADMAP status

Set the implemented epic's Status from `Planned` → `Done` (or `In Progress` if partial).

Append `## Implementation Log` to the plan file: date, files touched, test results, deviations.

### 2.4 — Phase 2 report

State: branch, files created/changed, test/build results (verbatim failures if any), ROADMAP change. Then: "Proceeding to QA."

**Do NOT commit, push, or open a PR here** — that happens in Phase 3 only if QA is GREEN.

---

## Phase 3 — QA + PR (follows /dqa rules, extended with commit/push/PR)

### 3.1 — Establish scope & context

1. Read `ROADMAP.md` — target epic row: Name, Description, Status, Dependencies. If still `Planned`, stop.
2. Read `docs/<epic-id>-*-plan.md` — deliverables, build/sequence order, per-file Tests specs, gate checks.
3. Read `PRD.md` §5 (User Journey) and §6 (Functional Requirements).
4. Note current branch (`git branch --show-current`). Test working tree as-is; do not switch branches or modify source.

### 3.2 — Feature checklist

For each capability the epic delivers (from plan deliverables + PRD requirements), define:
- **Feature** — capability name.
- **What it proves** — requirement/behavior (cite PRD §/ROADMAP).
- **Steps** — exact reproducible commands.
- **Expected result** — observable success condition.

Features blocked by not-yet-built downstream epics: mark `N/A` with blocking epic, don't fail them.

### 3.3 — Execute

Run every test case via Bash. Record per case:
- **Result:** `PASS` / `FAIL` / `PARTIAL` / `N/A`.
- **Evidence:** command + relevant output. Verbatim failure output.
- **Assertions:** sub-checks passed / total.

Run the full suite — don't stop on first failure. Never edit source to make a test pass.

### 3.4 — Score

- **Per-feature success rate** = `passed assertions / total assertions` as `NN%`.
- **Overall success rate** = `passed cases / total testable cases` (exclude N/A), as `NN%`.
- **Verdict:** `GREEN` (100% testable pass) / `YELLOW` (≥1 PARTIAL or non-critical FAIL) / `RED` (any critical-path FAIL).

### 3.5 — Write QA report

Path: `docs/<epic-id-lowercased>-qa-report.md`

Must contain:
1. Header — epic id, name, milestone, date, branch/commit, environment (OS, toolchain versions), overall success rate, verdict.
2. Summary line — `X/Y cases passed (NN%) · verdict · Z N/A (blocked by …)`.
3. Feature results table — `Feature | What it proves | Result | Success rate | Notes`.
4. Per-failure detail — steps, expected vs actual, verbatim output, suspected cause.
5. N/A list — features deferred + blocking epic.
6. Recommendations — concrete next actions (diagnosis only, no code changes).

### 3.6 — Gate: GREEN or stop

**If `YELLOW` or `RED`:** print QA report summary, list failing features, recommend fix command (e.g. `/dapprove <epic>` to fix, or `/dplan <epic>` for spec gap). **Stop — no commit, no push, no PR.**

**If `GREEN`:** state "QA GREEN — proceeding to commit, push, and PR."

### 3.7 — Commit, push, and open PR (GREEN only)

1. **Commit** all changes:
   ```bash
   git add -A
   git commit -m "feat(<epic-id-lowercased>): <epic-name-lowercase>

   Implements plan at docs/<plan-file>.md
   QA: <X>/<Y> cases passed (100%) — GREEN"
   ```

2. **Push** branch:
   ```bash
   git push -u origin <branch>
   ```

3. **Open PR** via `gh pr create`:
   ```bash
   gh pr create \
     --base main \
     --title "feat(<epic-id>): <epic-name>" \
     --body "$(cat <<'EOF'
   ## Epic
   **<Epic ID>** — <Epic Name>

   Plan: `docs/<plan-file>.md`
   QA report: `docs/<qa-report-file>.md`

   ## QA Summary
   **<X>/<Y> cases passed (100%) · GREEN**

   | Feature | Result | Success Rate |
   |---------|--------|-------------|
   <feature table rows>

   🤖 Generated with [Claude Code](https://claude.com/claude-code)
   EOF
   )"
   ```

4. **Print the PR URL** so the user can open it directly.

If `gh` is not installed or remote is not GitHub: push the branch, tell user to open PR manually — print branch name and base branch (`main`).

---

## Final output

Print in order:
1. Plan path + overall plan score.
2. Implementation summary (files created/changed, test results).
3. QA report path + verdict + summary line.
4. PR URL (or manual PR instructions if `gh` unavailable).
