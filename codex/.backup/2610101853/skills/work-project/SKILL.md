---
name: work-project
description: Manage initialized projects through project AGENTS.md, Input.md, Table26 state, and one bounded implementation agent per item, with main-session verification.
metadata:
  short-description: Run bounded work on an initialized project
---

# Work Project (`work_project` alias)

Read the nearest `AGENTS.md`, `Input.md`, applicable Table7 management document, and current Table26 task before changing product files. The nearest project `AGENTS.md` is the current work-state source.

## Grist access

Every Grist read and write (Table7 project rows, Table26 task rows, readback after writes) goes through the `grist-access` skill: `python3 ~/.codex/skills/grist-access/scripts/grist.py <command>`. It loads the API key from its own `.env`. Do not ask the user for an API key, do not type a key into a command, and do not call the Grist REST API with curl, fetch, or ad-hoc scripts. If the script reports a `config` or `connection` error, report that boundary instead of working around it.

Typical calls: `task <Raw ID>` to read the current row, `task-update <Raw ID> --fields '{"H": "진단"}'` for a stage change, `task-update <Raw ID> --fields '{"I": "...", "J": "..."}'` for the failure record, `task-add --fields '{"A": <Table7 Raw ID>, "G": "기능", "B": "...", "H": "초안"}'` for a new task row. The script rejects a `$H` value outside the six Korean stages.

## Table26 work state

`$H` is a Choice column and the only stage field; never write a stage to `$C`. Store exactly one Korean choice value; the English names are documentation aliases only and must never be written to `$H`.

| `$H` value | Alias | Meaning |
|---|---|---|
| `초안` | SCOPE | requirement bound, not started |
| `진단` | DIAGNOSE | reproducing or locating the first failing boundary |
| `교정` | REPAIR | changing the owner |
| `검증` | VERIFY | focused, production-boundary, and build checks |
| `대기` | HANDOFF | only the final user input remains |
| `완료` | DONE | bounded predicate passed |

Write `$H` only when the stage actually changes. `검토`(REVIEW) and per-item `RUNTIME` are not stages.

## Bounded implementation

1. Bind one requirement to one start action, one producer→transport/handler→authoritative-consumer path, one final effect, and the adjacent paths that must not change. Record the requirement in project `Input.md` and the existing dated `.ai/changes/YYMMDD.요약명.md` record.
2. Invoke one implementation agent for one atomic item. It diagnoses, repairs, and verifies in the same run. Its prompt contains only:
   - the project instruction: read this project's `AGENTS.md` first and work from its current state and completion condition;
   - one failure boundary;
   - the editable paths (forbidden paths default to sibling projects, global settings, and skills);
   - the completion condition as commands and expected results.
3. The agent updates `$H` as its stage changes, updates the dated change record, and returns plain text: cause, changed files, and a test-output summary.
4. The main session reads the diff and test output, compares them with the `Input.md` requirement, and sets `$H=완료` or dispatches exactly one next bounded repair without waiting for a new user message.
5. Call a separate completion-verification agent only for a contract change on a path that mutates external data (Grist writes, Eagle move/delete, Bridge commands), a shared Bridge contract extension, or an explicit user request. Its result is plain text; never review a review.

Markdown change records, project `AGENTS.md`, `Input.md`, Table26 state, command output, and the product's real consumer are the work records.

## Verification order

For each item use the same scenario and input through `focused/unit → production-boundary integration → built artifact`. Do not run the whole-flow runtime per item. After every requested item is `완료`, run one project-level runtime flow through the real artifact and authoritative consumer. A mock, source inspection, build, or intermediate acknowledgement is not completion evidence for a user-visible item.

If a user-only action remains, set `$H=대기`, prepare the exact single action and expected result, and request it only after all agent-executable checks are complete.

## Failure loop

This loop also covers reported errors, regressions, and features that did not work.

1. Record the observation and diagnosis only in the current task's Table26 row: `$I`(오류) and `$J`(원인). Each of `$D`, `$I` and `$J` holds one line: the latest item with its number in this task (for example `3. 인증 단계 확립`), replacing the previous line. The full numbered list 1..N with details goes only into the dated `.ai/changes` record under the same numbers. Do not create a separate error row or change `$G` for this. Create a task row only when the work has none, and point the project `AGENTS.md` task Raw ID at it.
2. Observe before editing code. Read the real runtime first (console exceptions, rendered screen, the actual state value on the failing path, loaded artifact hash) and write `$I`(오류, Text) as copied command output, not inference:
   `경로: <how it was read> / 실제값: <observed value> / 기대값: <declared contract> / artifact: <loaded hash vs built hash>`
3. Write `$J`(원인, Text) with the first failing boundary, owner code location, and the regression test that reproduces `$I`'s actual value. When one symptom has several causes, number them in the `.ai/changes` record and keep only the latest numbered cause in `$J`.
4. Stage gates: `진단→교정` only when `$I` and `$J` are filled. `교정→검증` only when the `$J` test fails on the pre-fix code and passes after the fix. A mock or fixture must use the input shape captured in `$I`; a shape never observed at runtime is not regression evidence.
5. Every early return and async exception on the user-action path logs or shows a message; a silent no-op is itself a failing boundary.
6. Before requesting a user run, confirm the browser-loaded artifact hash equals the built artifact.

Keep the item open, record the combined reproducing state in the change record, then dispatch a new bounded repair for that boundary. Preserve adjacent workflows and do not add fields, relationships, transports, or validation artifacts outside the request.

Keep credentials, authorization data, raw private payloads, and full private paths out of logs. External API diagnostics use operation ID, owner, operation, boundary, attempt, result, and thread role.
