## Mandatory Pre-Read
- If requirements are ambiguous or multiple interpretations exist: list options + ask 1 question OR pick safest option and state assumption.
- Keep changes minimal: no refactors, no formatting sweeps, no unrelated cleanup.
- Prefer simplest working code: no speculative abstractions or extra features.
- Before editing: identify exact file+location (search/read first). Don’t edit blind.
- Definition of done: run the project’s tests/lint (or stated check) and report results.
- Trivial tasks (<= ~10 LOC, 1 file, clear): skip long planning; still follow “minimal change + verify”.

## Meta Operation Override
### Priority Branch (Highest First)
1. If the request is a meta-operation file change, do not trigger `plan-project-code` or `add-function`.
2. If it is not a meta-operation file change and the request is code implementation/modification/creation, apply normal `plan-project-code`/`add-function` trigger rules.

### Meta Operation Scope
- `AGENTS*` files (`AGENTS.md`, `AGENTS.override`, `AGENTS.override.md`, etc.)
- All skill definition files (`**/SKILL.md`)
- Policy/text updates for config files (`*config*`, `settings*`, `*.yaml`, `*.yml`, `*.toml`, `*.json`)
- Documentation files (`*.md`, `*.txt`)
- Simple wording fixes without code behavior change

### Enforcement
- For meta-operation file edits, apply minimal changes only to the requested docs/settings.
- Do not force `references/problem-analysis.md` or `./project/project.md` gating for meta-operation edits.

## Skill Trigger Gate (Mandatory)
- Before any exploration/editing command, scan the latest user prompt for skill triggers (explicit skill names, architecture/design wording, external docs/URLs referenced for implementation).
- If a trigger matches a mandatory design-first skill (e.g. `coding-design-pipeline` / `plan-code`), freeze implementation and run the required design workflow first (`plan.md` creation/confirmation) before code changes.
- If work was interrupted, resume from the earliest unmet mandatory step; do not continue from a later implementation step.
- In the first progress update, state which skill is being applied (or why none applies).
- If the user provides an English file/folder path that does not exist, treat it as a typo: find the closest existing path, state the correction briefly, and use the corrected path for subsequent steps.


## AI Feedback Loop (Mandatory)
- If you recognize a mistake (instruction violation, wrong skill application, wrong sequence), run `ai-feedback` flow immediately before any further implementation.
- `ai-feedback` flow order is fixed:
  0) Create/update `problem-ai.md` with root cause and concrete corrective actions.
  1) Read `AGENTS.override.md` and related `SKILL.md` files, then add/strengthen guardrails to prevent recurrence.
- For `plan-code` tasks, order is absolute and cannot be skipped:
  1) `problem-analysis.md` first
  2) `plan.md` next
  3) then implementation


## Virtual Scenario Gate (Mandatory)
- If the request is a structure-improvement implementation (e.g. architecture/process/flow refactor or redesign), run `virtual-scenario` skill before implementation.
- Creating/updating `.project/scenario.md` is always mandatory.
- `virtual-scenario` summary format is:
  - `command | files touched | derived result`
- If the user’s first instruction says `알아서 처리`, do not print the virtual-scenario summary separately to the user; keep it in `.project/scenario.md` only.
- Without `.project/scenario.md`, do not start implementation.

## Mandatory Final Notification
- When final work is fully completed, run `/home/tree/Config/data/fish/functions/notify.fish -m "${작업명} complete"` exactly once.

## Completion Log Rule (Feature Addition)
- Whenever a feature addition is fully completed, append a completion record to `./.agents/log.md`.
- Use this heading format for each record: `## 날짜 - 작업한일`.
- Add one record per completed feature task and keep entries in chronological order.

## Shell Default Rule
- Default shell is `fish`.
- If a task can be solved directly with `rg`/`rg --files`/`sd` (and optional `xargs`), run it without wrapping in `fish -ic` or `bash -c`.
- For directory/file listing, prefer direct `eza` execution (fallback: `ls`) without shell wrapping.
- Use `fish -ic` only when direct commands (`rg`/`sd`/`eza`/`ls`) are insufficient and shell-dependent behavior is required.
- Mandatory final notification command should be executed directly (example: `/home/tree/Config/data/fish/functions/notify.fish -m "<task-name> complete"`).

## Scripting Language Rule
- For search/filter/simple substitution, prefer direct Rust CLI usage: `rg`, `rg --files`, `sd`.
- Use `bash -c` or `fish -ic` only for multi-step shell logic, shell syntax, or complex quoting that cannot be handled by direct `rg`/`sd` commands.
- Default scripting language is **Python** for file/text processing tasks.
