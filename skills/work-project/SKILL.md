---
name: work-project
description: Manage initialized projects through project AGENTS.md, Input.md and Table26 rows — main splits requests into rows and writes each row's goal (detail in Input.md, one line in $K), one t26_<id> agent per row works test-first (task-red → task-submit → 검증), main runs hidden cases and the whole flow (flow-wait → 대기) and completes after user confirmation (완료).
metadata:
  short-description: Run bounded work on an initialized project
---

# Work Project (`work_project` alias)

Read the nearest `AGENTS.md`, `Input.md`, applicable Table7 management document, and current Table26 task before changing product files. The nearest project `AGENTS.md` is the current work-state source.

## Grist access

Every Grist read and write (Table7 project rows, Table26 task rows, readback after writes) goes through the `grist-access` skill: `python3 ~/.codex/skills/grist-access/scripts/grist.py <command>`. It loads the API key from its own `.env`. Do not ask the user for an API key, do not type a key into a command, and do not call the Grist REST API with curl, fetch, or ad-hoc scripts. If the script reports a `config` or `connection` error, report that boundary instead of working around it.

Typical calls: `task <Raw ID>` to read a row, `task-update <Raw ID> --fields '{"H": "작업"}'` for a stage change, `task-update <Raw ID> --fields '{"I": "...", "J": "..."}'` for the failure record, `task-add --fields '{"A": <Table7 Raw ID>, "G": "기능", "B": "...", "H": "초안"}'` for a new row. The script rejects a `$H` value outside the Korean stages, rejects `검증`/`완료` (written only by `task-submit`/`task-complete`) and any `$K` write (only `task-goal`).

## Table26 work state

`$H` is a Choice column and the only stage field; never write a stage to `$C`. The row's goal is the `### 목표 #<id>` block of the project `Input.md` (the single source of completion); Table26 `$K` (목표) holds only its one-line summary. Only `task-goal` writes either; agents only read them.

| `$H` | Written by | Meaning | Turn may end |
|---|---|---|---|
| `초안` | main (`task-add`, then `task-goal`) | split from `Input.md`, goal being concretized | – |
| `작업` | `task-red` | a failing test pins the goal; implementing | no |
| `교정` | agent, or a gate on a failed rerun/judgment | repairing a failing boundary | no |
| `검증` | `task-submit` | test passes, scope/literal/jev checks passed | agent yes / main no |
| `대기` | main via `flow-wait`, or main before a question | whole flow passed; only user-input checks remain | main yes |
| `완료` | main via `task-complete` | every goal line judged with evidence after user confirmation | yes |

`진단` is a legacy value. Write `$H` only when the stage actually changes.

## Flow: Input.md → goals → agents (TDD) → 검증 → hidden cases → 대기 → 완료

1. **Split (main).** Read the new `Input.md` request. Split it into independent requirements; requirements that touch the same files stay in one row. `task-add` one row per requirement with `$H=초안`, and set the project `AGENTS.md` header to `- Table26 task Raw ID: <id>, <id>, …`.
2. **Concretize the goal (main).** For each row write the detailed goal: real input → expected output examples, output conditions and boundaries, what must not change, and a `변형:` line saying how the output follows a different input. Tag every line by how it is observed: `[값]` function output, `[상태]` readback after an action, `[수치]` measured number, `[불변]` unchanged behaviour, `[화면]` screen, `[사용자]` only the user can confirm. Use only what the request states; ask the user only when two readings lead to different results. Write it with `grist.py task-goal <id> --project <dir> --goal-file <file> --summary "<one line>"`: the lines go to `Input.md` `### 목표 #<id>`, the summary to `$K`; its format/scope failure is the next edit.
3. **Assign (main).** `spawn_agent` per row with `task_name` exactly `t26_<id>` and `fork_turns="none"`. The prompt holds only: project dir and "read its `AGENTS.md` first"; `Table26 #<id>` and "read the goal in `Input.md` `### 목표 #<id>`"; the editable paths (sibling projects, global settings, skills and other rows' files are forbidden); the agent procedure below. Rows run in parallel only when their editable paths do not overlap.
4. **Agent procedure (TDD).**
   1. Read the goal (`Input.md` `### 목표 #<id>`; legacy rows: `$K`); never edit it.
   2. Create or adjust the function skeleton, then write a test that asserts the `[값]`/`[상태]`/`[수치]` lines on real inputs and prints the actual results.
   3. `grist.py task-red <id> --project <dir> --test "<cmd>" --tests <test files> --paths <editable paths>`; it accepts only an assertion failure and sets `$H=작업`.
   4. Implement until the test passes; on a failing boundary set `$H=교정` with `$I`/`$J` (Failure loop below). Compute results from inputs; never return goal expected values as literals.
   5. `grist.py task-submit <id> --project <dir>` writes `$H=검증` only when the red record, tests and goal are unchanged, the test passes, nothing outside the editable paths changed, and the output meets the goal. Its failure output is the next repair.
   - The SubagentStop hook blocks the agent until its row is `검증`. Agents change only sources and tests: no version bump, build, version history or deploy copy. They do not ask the user, spawn agents, write the goal/`대기`/`완료`, or touch other rows; they return cause, changed files and test summary as text.
5. **Hidden cases (main).** After a row is `검증`, append to today's record a case the agent never saw (different input, boundary, empty value) and run it:
   ```
   ## 숨긴 사례 #<id>
   - 테스트: `<command with a new input>`
   ```
   A failure is reassigned to the same agent row as one bounded repair.
6. **Flow test (main).** When every row is `검증`, first release once as the project `AGENTS.md` and `/home/tree/extensions/AGENTS.md` say (version bump, build, version history, deploy copy, reload into a running desktop app). Then run the whole flow once through the real artifact and authoritative consumer (see Verification order), covering `[화면]` lines, and append after the latest submission:
   ```
   ## 흐름 판정
   - [x] <flow check>
     근거: <command or readback → actual value>
   ```
   Then `grist.py flow-wait --project <dir>`. It reruns submitted and hidden tests (failures go back to `교정`), checks the flow record, and writes `$H=대기` with the `[사용자]` lines in `$D`. Ask the user only now, for those lines.
7. **Complete (main).** After the user confirms, append per row:
   ```
   ## 완료 판정 #<id>
   - [x] [<유형>] <exact goal line text>
     근거: <command or readback → actual value>
   ```
   and run `grist.py task-complete <id> --project <dir>`. Its `unchecked`/`missing_evidence` output is the next bounded task.

A single small request may be one row handled by the main session itself with the same commands. Judgments inside the gates come from `jev-access`; they are not a separate step.

Markdown change records, project `AGENTS.md`, `Input.md`, Table26 state, command output, and the product's real consumer are the work records.

## Verification order

For each item use the same scenario and input through `focused/unit → production-boundary integration → built artifact`. Do not run the whole-flow runtime per item. After every row is `검증`, run one project-level runtime flow through the real artifact and authoritative consumer. A mock, source inspection, build, or intermediate acknowledgement is not completion evidence for a user-visible item.

If a user-only action remains (login, payment/cost, irreversible external send, OS app action), set `$H=대기` and write the reason in `$D` before asking; prepare the exact single action and expected result, and request it only after all agent-executable checks are complete. Any question to the user also requires `$H=대기` first.

## Completion gate

The Codex hook `~/.codex/hooks/completion_gate.py` enforces the table above: on Stop, the main session may end only when every header row is `대기` or `완료`; on SubagentStop, a `t26_<id>` agent may end only when its row is `검증` or `완료`. With jev in `enforce` mode it also blocks a final message that asks the user for anything but the four allowed actions, and sharpens the reason when a message ends on a failure. Three blocks with no `$H` change let the turn end and record it in `$D`. A CDP/runner/Stagehand failure or `runtime-unverified` is not a reason to stop: record it in `$I` and continue (fix owner → rebuild/deploy → reconnect → rerun the same flow → consumer readback).

## Failure loop

This loop also covers reported errors, regressions, and features that did not work.

1. Record the observation and diagnosis only in the current task's Table26 row: `$I`(오류) and `$J`(원인). Each of `$D`, `$I` and `$J` holds one line: the latest item with its number in this task (for example `3. 인증 단계 확립`), replacing the previous line. The full numbered list 1..N with details goes only into the dated `.ai/changes` record under the same numbers. Do not create a separate error row or change `$G` for this. Create a task row only when the work has none, and point the project `AGENTS.md` task Raw ID at it.
2. Observe before editing code. Read the real runtime first (console exceptions, rendered screen, the actual state value on the failing path, loaded artifact hash) and write `$I`(오류, Text) as copied command output, not inference:
   `경로: <how it was read> / 실제값: <observed value> / 기대값: <declared contract> / artifact: <loaded hash vs built hash>`
3. Write `$J`(원인, Text) with the first failing boundary, owner code location, and the regression test that reproduces `$I`'s actual value. When one symptom has several causes, number them in the `.ai/changes` record and keep only the latest numbered cause in `$J`.
4. Stage gates: `작업→교정` records `$I` and `$J`. `교정→검증` (`task-submit`) only when the `$J` test fails on the pre-fix code and passes after the fix. A mock or fixture must use the input shape captured in `$I`; a shape never observed at runtime is not regression evidence.
5. Every early return and async exception on the user-action path logs or shows a message; a silent no-op is itself a failing boundary.
6. Before requesting a user run, confirm the browser-loaded artifact hash equals the built artifact.

Keep the item open, record the combined reproducing state in the change record, then dispatch a new bounded repair for that boundary. Preserve adjacent workflows and do not add fields, relationships, transports, or validation artifacts outside the request.

Keep credentials, authorization data, raw private payloads, and full private paths out of logs. External API diagnostics use operation ID, owner, operation, boundary, attempt, result, and thread role.
