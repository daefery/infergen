---
description: Execute an approved Infergen plan (coding & implementation)
argument-hint: [epic e.g. E0.1 — defaults to most recent plan]
model: sonnet
allowed-tools: Read, Write, Edit, Bash, Glob, Grep
---

# /dapprove — Execute Plan (Sonnet)

## Step 0 — Model check (REQUIRED before anything else)

This command requires **Sonnet**. Before reading any file or doing any work:

1. Run `/model` to see the active model.
2. If it is not `sonnet`, run `/model sonnet` and wait for confirmation.
3. Only proceed once the model is confirmed as **Sonnet**.

---

You are operating in **IMPLEMENTATION mode**. This command runs on **Sonnet** because coding and implementation should run on the faster model. The plan was already designed on Opus via `/dplan` — **follow it; do not re-design it.**

## Step 1 — Locate the plan

Target: **$ARGUMENTS**

- If `$ARGUMENTS` names an epic (e.g. `E0.1`), open the matching `docs/<epic-id>-*-plan.md`.
- If `$ARGUMENTS` is empty, list `docs/*-plan.md` and pick the **most recently modified** one. State which plan you selected before proceeding.
- If no plan files exist, stop and tell the user to run `/dplan <epic>` first.

## Step 2 — Verify the gate

Read the plan's `## Scorecard`. 
- If the **Overall Plan Score ≥ 96%** and no file is marked `⚠ BELOW GATE`, proceed.
- If any file is **below gate** or the overall is `< 96%`, **stop**. Report which files fail and tell the user to re-run `/dplan <epic>` to raise the score before implementing. Do not implement a sub-gate plan.

## Step 3 — Create the working branch (BEFORE any code changes)

**No source edits, file writes, or commits happen on the default branch.** Before touching a single file:

1. Derive the branch name from the epic being executed: `epic/<epic-id-lowercased>-<kebab-name>`, taken from the plan filename (e.g. `docs/e0.1-project-scaffold-monorepo-plan.md` → `epic/e0.1-project-scaffold-monorepo`). If the epic id can't be derived from the filename, use `epic/<epic-id-lowercased>` (e.g. `epic/e0.1`).
2. Check git state with `git rev-parse --is-inside-work-tree`:
   - **Not a git repo** (e.g. the E0.1 scaffold runs before git exists): run `git init`, then create the branch with `git checkout -b <branch>`. (Add `.gitignore` as the very first implemented file so nothing untracked leaks in.)
   - **Is a git repo:** always branch from `main` — run `git checkout main && git pull origin main` first. Then if `<branch>` already exists, `git checkout <branch>`; otherwise `git checkout -b <branch>`.
3. Confirm you are on the branch (`git branch --show-current`) and state it explicitly before proceeding. If for any reason the branch cannot be created, **stop and report** — do not implement on the default branch.

## Step 4 — Implement

Work the plan's **build/sequence order**, file by file:
1. Create or edit each file exactly to its spec (symbols, signatures, edge cases, error handling).
2. Write the tests the plan specifies for that file.
3. Run the relevant build/lint/test command after each meaningful unit; fix failures before moving on.
4. If reality diverges from the plan (a spec is wrong or impossible), **stop and report the conflict** rather than silently improvising — the plan is the contract.

Match existing code style and conventions in the repo. Do not add files the plan didn't call for.

## Step 5 — Update status

After all files pass:
- Update `ROADMAP.md`: set the implemented epic's Status from `Planned` → `Done` (or `In Progress` if only partially completed).
- Append a short `## Implementation Log` to the plan file: date, files touched, test results, and any deviations from the plan (with reason).

## Step 6 — Report

Summarize: the branch you worked on, files created/changed, test/build results (quote failures verbatim if any), ROADMAP status change, and any follow-ups or unblocked downstream epics.

**Do NOT commit, push, or open a PR/MR here.** All git operations (commit → push → PR) are deferred to `/dqa` and only happen after all QA tests pass (GREEN verdict). The working tree stays uncommitted so QA tests the actual implementation state.
