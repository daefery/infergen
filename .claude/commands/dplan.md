---
description: Deep-plan a Infergen epic/milestone from PRD + ROADMAP into a scored implementation plan
argument-hint: <epic e.g. E0.1 | milestone e.g. M0>
model: opus
allowed-tools: Read, Write, Edit, Glob, Grep, Bash
---

# /dplan — Deep Planning (Opus)

## Step 0 — Model check (REQUIRED before anything else)

This command requires **Opus**. Before reading any file or doing any work:

1. Run `/model` to see the active model.
2. If it is not `opus`, run `/model opus` and wait for confirmation.
3. Only proceed once the model is confirmed as **Opus**.

---

You are operating in **PLANNING mode**. This command runs on **Opus** because planning and documentation demand the strongest model. Do **not** write or modify any production/source code here — your only output artifact is a plan document under `docs/`.

## Input

Target requested: **$ARGUMENTS**

This is either an Epic number (e.g. `E0.1`, `E1.3`) or a Milestone number (e.g. `M0`, `Milestone 0`). If `$ARGUMENTS` is empty, stop and ask the user which epic/milestone to plan.

## Step 1 — Scan the ROADMAP

1. Read `ROADMAP.md` in full.
2. Locate the requested target:
   - **Epic** (e.g. `E0.1`): find that exact row. Capture its Name, Description, Status, and Note/Blocker/Dependency.
   - **Milestone** (e.g. `M0`): find the milestone section, then plan **every epic** in it (each epic gets its own scored section in one combined plan file).
3. Resolve dependencies: for every listed dependency epic, note whether it is `Done`. If a hard dependency is not `Done`, flag it prominently at the top of the plan as a **⚠ Blocked-by** warning — but still produce the plan.
4. If the target cannot be found in the ROADMAP, stop and report the available epic/milestone IDs.

## Step 2 — Read the PRD for context

Read `PRD.md`. Pull the sections relevant to the target (functional requirements §6, non-functional §7, architecture §8, and any others the epic touches). The plan must be grounded in the PRD — quote/reference specific requirement IDs and section numbers where they justify a design choice.

## Step 3 — Survey existing code

Use Glob/Grep/Read to inspect any code already present that the epic builds on or modifies. The plan must reflect reality on disk, not assumptions. If the repo is still empty for this area, say so explicitly.

## Step 4 — Author the detailed plan

Determine the output path: slugify the epic name.
`docs/<epic-id-lowercased>-<kebab-name>-plan.md`
Example: `E0.1` "Project Scaffold & Monorepo" → `docs/e0.1-project-scaffold-monorepo-plan.md`
For a milestone, use `docs/m0-foundation-plan.md`.

The plan must be **file-driven**: enumerate every file to be created or changed, in dependency order. For each file provide:

- **Path** — exact target path.
- **Purpose** — one line.
- **Detailed spec** — exported symbols/types/functions, signatures, key logic, edge cases, error handling. Concrete enough that the implementer writes it without further design decisions.
- **Dependencies** — other files/packages it relies on.
- **Tests** — what proves it correct (unit/golden/integration), with fixture notes.
- **Score** — a planning-quality score `NN%` for THIS file's spec (see Step 5 rubric).

Also include, at the top of the plan:
- Epic ID, name, source milestone, and any ⚠ Blocked-by warnings.
- PRD/ROADMAP references.
- Build/sequence order (the order files should be implemented).
- Open questions / risks (if any are unresolved, they cap the score — resolve them in-plan where possible).

## Step 5 — Score and auto-review (gate: ≥ 96%)

Score **each file's spec** on this rubric (weighted), 0–100%:

| Dimension | Weight | Asks |
|---|---|---|
| Completeness | 30% | Are all symbols, signatures, edge cases, and errors specified? Nothing left as "TBD"? |
| Correctness | 25% | Does the spec satisfy the PRD requirement and respect dependencies/architecture? |
| Implementability | 20% | Can a Sonnet implementer build it with zero further design decisions? |
| Testability | 15% | Are concrete tests + fixtures defined that prove correctness? |
| Clarity | 10% | Unambiguous paths, names, ordering? |

**The minimum score for every file is 96%.**

Run an **auto-review loop**:
1. Score every file.
2. For any file `< 96%`, identify the exact gap (which dimension lost points and why) and **revise that file's spec in place** to close it.
3. Re-score. Repeat until **every file ≥ 96%**.
4. Cap the loop at 5 iterations. If a file still can't reach 96% after 5 passes, leave it at its best score, mark it `⚠ BELOW GATE`, and write a short note on exactly what blocks it (usually an unresolved decision that needs the user).

Compute the **Overall Plan Score** = weighted-by-effort average of file scores (simple mean is fine if effort is uniform).

## Step 6 — Write scores into the plan file

The plan file MUST embed the scores:
- A `## Scorecard` table near the top: each file → its score, plus the Overall Plan Score.
- Each per-file section repeats its own **Score: NN%** line.
- A one-line **review summary** stating how many auto-review iterations ran and that the gate (≥96%) is met (or which files are below gate and why).

## Step 7 — Hand off

After writing the file, print:
1. The plan path.
2. The Scorecard (overall + any below-gate files).
3. This exact prompt to the user:

> ✅ Plan ready at `<path>` — overall score **NN%**.
> Review it, then type **`/dapprove`** (optionally `/dapprove <epic-id>`) to execute. Implementation runs on **Sonnet**.

Do not begin implementation. `/dplan` plans only.
