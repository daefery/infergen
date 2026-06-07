---
description: End-to-end QA an implemented Infergen epic — test every feature, report per-feature success rate
argument-hint: [epic e.g. E0.1 or milestone e.g. M0 — defaults to most recent Done epic]
model: sonnet
allowed-tools: Read, Write, Edit, Bash, Glob, Grep
---

# /dqa — End-to-End QA (Sonnet)

## Step 0 — Model check (REQUIRED before anything else)

This command requires **Sonnet**. Before reading any file or doing any work:

1. Run `/model` to see the active model.
2. If it is not `sonnet`, run `/model sonnet` and wait for confirmation.
3. Only proceed once the model is confirmed as **Sonnet**.

---

You are operating in **QA mode**. You **test** an implemented epic end-to-end and write a QA report. You do **not** fix code, refactor, or implement features here — the only file you may create/edit is the QA report under `docs/`. If you find bugs, you report them; fixing is a separate `/dapprove` (or `/dplan`) pass.

## Input

Target: **$ARGUMENTS**

- **Epic** (e.g. `E0.1`): QA that one epic.
- **Milestone** (e.g. `M0`): QA every `Done` (or `In Progress`) epic in it — each epic gets its own section in one combined report.
- **Empty**: pick the most recently completed epic — the one whose `docs/*-plan.md` was most recently modified AND is marked `Done`/`In Progress` in `ROADMAP.md`. State which epic you selected before proceeding. If nothing is implemented yet, stop and say so.

## Step 1 — Establish scope & context

1. Read `ROADMAP.md` — find the target epic row: Name, Description, Status, Dependencies. If Status is still `Planned` (nothing built), stop and tell the user to run `/dapprove <epic>` first.
2. Read the epic's plan: `docs/<epic-id>-*-plan.md`. Pull its deliverables, the **build/sequence order**, the per-file **Tests** specs, and any **gate checks** / acceptance criteria.
3. Read `PRD.md` — especially **§5 User Journey** and **§6 Functional Requirements** — to ground the e2e scenario in real user-facing behavior, not just unit tests.
4. Note the git branch under test (`git branch --show-current` if a repo exists) and record it in the report. Test the current working tree as-is; do **not** switch branches or modify source.

## Step 2 — Derive the e2e test scenario

Build a **feature checklist** for the epic: enumerate every distinct capability the epic claims to deliver (from the plan's deliverables + the PRD requirements it satisfies). For each feature, define a concrete **test case**:

- **Feature** — the capability name (e.g. "CLI `--version` flag", "Cargo workspace builds", "runtime package emits ESM+CJS+types").
- **What it proves** — the requirement/behavior under test (cite PRD §/ROADMAP where relevant).
- **Steps** — exact commands or actions to run (real, reproducible).
- **Expected result** — the observable success condition.

Cover the full happy-path scenario end-to-end, plus the obvious failure/edge cases the plan called out. A feature that depends on a not-yet-built downstream epic is **N/A** — list it with the blocking epic, don't fail it.

## Step 3 — Execute

Run each test case **for real** via Bash (build, lint, test suites, CLI invocations, generated-output checks, file-existence/format assertions, etc.). For each case record:

- **Result:** `PASS` / `FAIL` / `PARTIAL` / `N/A`.
- **Evidence:** the command run and the relevant output. Quote failure output **verbatim**.
- **Assertions:** how many sub-checks the case has and how many passed (drives the success rate).

Do not stop on the first failure — run the whole suite so the report is complete. Never edit source to make a test pass; if a test can't run because of a real defect, mark it `FAIL` and capture why.

## Step 4 — Score

Compute success rates:

- **Per-feature success rate** = `passed assertions / total assertions` for that feature, as `NN%`. (A single-assertion case is 100% or 0%.)
- **Overall success rate** = `passed cases / total testable cases` (exclude `N/A` from the denominator), as `NN%`. Also report a weighted-by-assertions overall if it differs meaningfully.
- **Verdict:** `GREEN` (100% testable pass), `YELLOW` (≥1 PARTIAL or non-critical FAIL), `RED` (any critical-path feature FAILs). Critical-path = features on the PRD critical path / the epic's acceptance gate.

## Step 5 — Write the QA report

Path: `docs/<epic-id-lowercased>-qa-report.md` (milestone: `docs/m0-qa-report.md`).

The report MUST contain:

1. **Header** — epic id, name, source milestone, date, branch/commit under test, environment (OS, toolchain versions actually used), overall success rate, and verdict.
2. **Summary line** — `X/Y cases passed (NN%) · verdict · Z N/A (blocked by …)`.
3. **Feature results table** — one row per feature: `Feature | What it proves | Result | Success rate | Notes`. This is the core deliverable: which feature, what was tested, and the success rate of each.
4. **Per-failure detail** — for every `FAIL`/`PARTIAL`: the steps, expected vs actual, **verbatim output**, and a suspected cause (one line — diagnosis only, not a fix).
5. **N/A list** — features deferred and the downstream epic that unblocks them.
6. **Recommendations** — concrete next actions (e.g. "re-run `/dapprove <epic>` to fix case 4", "spec gap — re-`/dplan`"). Diagnosis only; no code changes.

## Step 6 — Report

Print to the user:
1. The QA report path.
2. The summary line (overall success rate + verdict + N/A count).
3. The feature results table (or, for a milestone, per-epic overall rates).
4. If `RED`/`YELLOW`: the failing features and the recommended fix command. If `GREEN`: proceed to Step 7.

Do not modify source code. `/dqa` tests and reports only.

## Step 7 — Commit, push, and open PR (GREEN only)

**Only execute this step if the verdict is `GREEN` (100% testable cases pass).** If `YELLOW` or `RED`, stop after Step 6 and tell the user to fix failures then re-run `/dqa`.

1. **Stage and commit** — commit everything on the current branch. Use a conventional-commits subject derived from the epic:
   ```
   feat(<epic-id-lowercased>): <epic name in lowercase>
   ```
   Body: one-line summary of what was implemented, referencing the plan file path.
   ```bash
   git add -A
   git commit -m "feat(<epic-id>): <epic-name>

   Implements plan at docs/<plan-file>.md
   QA: <X>/<Y> cases passed (100%) — GREEN"
   ```

2. **Push branch** to origin:
   ```bash
   git push -u origin <branch>
   ```

3. **Open PR** via `gh pr create`:
   - Title: `feat(<epic-id>): <epic-name>`
   - Body must include: epic ID, plan path, QA report path, overall success rate, and a checklist of features tested.
   - Base branch: `main`
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
   <paste feature table rows>

   🤖 Generated with [Claude Code](https://claude.com/claude-code)
   EOF
   )"
   ```

4. **Print the PR URL** returned by `gh pr create` so the user can open it directly.

If `gh` is not installed or the remote is not GitHub, skip step 3-4, push the branch, and tell the user to open the PR manually — print the branch name and base branch clearly.
