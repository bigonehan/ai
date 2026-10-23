---
name: verify-work
description: Plan, write, run, and review tests through focused, production-boundary, built-artifact, and final runtime checks.
metadata:
  short-description: Verify user-visible behavior at its real consumer
---

# Verify Work

Use this skill whenever tests are written, changed, run, investigated, or used to claim behavior. Do not read `AGENTS.md`, `.ai/changes/`, Obsidian notes, or `architecture.md` in full; extract and print only the needed sections, lines, or fields with `rg -n` or `grist task-show <id> --project <dir> --section <section>`. Before testing each Table26 row, the actor must directly run `grist task <id>` to read `H`/`I`/`J`/`K`; `grist gate` and `task-show` do not replace it. If an error's cause is written to `J` during testing, run `grist task <id>` again and repair that cause. The project rules own scope and the authoritative consumer.

## Work stages

Table26 `$H` is a Choice column that stores one Korean value: `초안`, `진단`, `작업`, `교정`, `검증`, `확인`, or `완료`. English names are aliases only and are never written to `$H`. `검토`(REVIEW) and per-item `RUNTIME` are not stages. Use `focused/unit → production-boundary integration → built artifact` when the changed behavior crosses those boundaries. If only the output format of an already working diagnostic logger changes, verify the formatter and its existing logger call with focused unit tests; do not require a browser session or whole-flow replay for that change.

## No legacy verification artifacts

Record requirements (its `# 요청` section) and results in the dated `.ai/changes/YYMMDD.요약명.md` Markdown record, Table26 state, command output, and the real product consumer.

## Requirement-to-consumer testing

For every user-visible requirement, map exactly one path:

`start action → producer → transport/handler → authoritative consumer → final effect`

Use the same scenario and meaningful input at every layer. A helper return, ACK, mock, source string, screenshot, build, or intermediate UI state is not final evidence. Read the persisted consumer state directly. Keep adjacent workflows out of the path and add negative-scope checks when the request names a narrow workflow.

## Boundary rules

- When an error is reported, record the user's observed failure in Table26 `$I`. Put the user's stated cause in `$J`; if no cause is given, reproduce the same failure to identify the first failing owner boundary and then write `$J`. Use the cause in that `task-update` output's `row` line as the repair target before changing code; do not re-read the row. Do not reproduce a cause the user already supplied. Record the actual producer value and type separately from the consumer expectation when a value or type mismatch is under investigation.
- Preserve typed values. A single Grist cell has one declared type; reject accidental unions, implicit numeric-string conversion, `String(value)`, tuple-last-element ID guesses, and uncontracted fallback parsers.
- For external APIs, test operation identity, owner, operation, boundary, attempt, result, and thread role while masking credentials, authorization data, raw payloads, and private paths.
- Test effect cardinality and ordering, retry before/after mutation, response loss, timeout, duplicate request IDs, fan-out/fan-in, bubbling, cleanup, reload, restart, and stale-instance removal when those boundaries exist.
- For path or file behavior, inspect the actual rendered/template output, decoded value, mapped path, file existence, permissions, and targeted line/field readback via `rg -n` (never dumping full files or objects).
- For a reported browser extension failure, use the user's observation and existing development-mode logs first. When those logs identify the failing request/handler boundary, do not launch CDP to reproduce the same failure. After repair, verify the changed boundary with focused tests and read the authoritative consumer's persisted result or correlated development-mode log. Use CDP when the changed behavior is a visible interaction, the user explicitly requests a real UI check, or those records cannot establish the final effect. An edit limited to an already working logger's output format uses the focused unit boundary above.
- For a user-requested drag interaction, require the `run-cdp` continuous browser-level mouse sequence and directly inspect the visible screen during the held drag and after release before passing the interaction. Record the visible selection range, which expected objects were included or missed, and any unintended text or image selection at both points. If either screen cannot be inspected or shows a wrong range, missed object, or unintended text/image selection, fail the check even when a function result says success. Unit tests, synthetic pointer/drag events, CDP command acknowledgments, DOM reads, and downloaded files can support diagnosis or final-effect checks, but cannot replace the observed on-screen gesture and selection.

## Skills for runtime and records

- When a browser runtime check is needed after the log and consumer readback decision above, use the `run-cdp` skill: the visible (`headless: false`) shared profile `/home/tree/.config/codex-run-cdp` on `127.0.0.1:9222`. Load the deployed extension directory into that profile; do not launch another browser or profile.
- Drive the user-visible start action and read the page with the `access-stagehand` skill (`openStagehand()`, `act`/`observe`/`extract`). Playwright scripts and per-project browser profiles are legacy and are not used for new checks.
- Read and write Table26 (`$H`, `$I`, `$J`) and other Grist data with the `access-grist` skill.

## Runtime separation

The default project test command runs unit and production-boundary checks only. An explicit project runtime command is reserved for the final whole-flow check after all Table26 items are submitted (`검증`). If the user has already identified a cause and harmful effect, take them as the diagnostic input and repair the effect's owner boundary first. Do not reopen UI, reread logs, or check installation merely to prove the same cause before editing. For a versioned runtime, after repair compare the installed artifact version with the version of each running actor that actually participates in this request's start action and final effect. Read UI version only when UI changes or UI interaction is part of that flow; do not open a plugin window to validate a service-only import or deletion fix. A mismatch fails the whole-flow check: do not count the consumer result as completion evidence or run `flow-wait` until the running versions match. Do not require this comparison for a runtime without versions. Do not substitute a mock browser, blank page, endpoint health, target discovery, build output, or log existence for the final consumer readback. If user-only action remains, use `Complete_task` to write `$H=확인` after the available evidence passes and request one exact action only after all agent-executable checks are exhausted.

## Verification bottleneck diagnosis and source repair

- Whenever a verification or runtime check command is sent to the background (`task-*`), **always set a 5-minute early-terminating wakeup timer (`schedule` with `DurationSeconds=300`, `TimerCondition="<task-id>"`) in the same turn before ending the turn**. Never end a turn waiting on a background verification task without a 300-second wakeup timer, because a hung background process will never wake the agent on its own.
- When the 5-minute timer fires (or when verification/API exploration takes 5 minutes or more), immediately inspect the task log and intermediate step logs (`[verify-step]` / `[stagehand-step]` / `[stagehand-bottleneck]`) across each boundary (`cdp-attach`, `extension-ready`, `start-action`, `transport`, `consumer-readback`, `session-close`) to identify the exact stage where the bottleneck occurs. Do not mask it by slapping a hard command timeout on it.
- A single API lookup, gate check, or verification path taking minutes means there is a defect or bottleneck in the source code itself (e.g., blocking `stdin`/IPC reads, unclosed keep-alive WebSocket/timer handles after `session.close()`, backgrounded processes holding parent stdio pipes without `</dev/null` and `disown`, repeated full-file scans or regex compilation inside loops, or unbounded polling).
- Immediately inspect the running process and intermediate step logs, pinpoint the exact bottleneck boundary, and fix the source code so the operation completes at normal speed. Record the bottleneck cause and fix in `.ai/changes/` (`### 메모`), `.ai/docs/performance.md`, and—whenever Stagehand init/attach/script execution was involved—in `~/ai/skills/access-stagehand/references/attach-notes.md` (and `~/ai/skills/access-stagehand/SKILL.md` if the procedure changed).
- Before running any Stagehand runtime check, check the relevant sections/lines of `~/ai/skills/access-stagehand/references/attach-notes.md` (`먼저 할 일`) and the project's `.ai/docs/performance.md` via `read-architecture` or `rg -n` (never reading full files): use `session.page` / `session.pages` directly (do not call non-existent `page.frames()` or `stagehand.context`), and use `node --experimental-transform-types --input-type=module -e` when importing `.ts` files alongside `openStagehand()`.
- `openStagehand()`'s `session.close()` unrefs remaining keep-alive handles automatically and emits `[stagehand-bottleneck]` every 60s while open; in custom scripts, ensure all timers/sockets are closed or call `process.exit(0)` after `await session.close()` in `finally`.

## Failure and completion

When a test fails, keep the item open, set `$H=진단`, record the failure in `$I`, and record the user's stated cause or the reproduced cause in `$J`. The `$J` write's `row` line is the repair target; a row with 1-2 changed files is repaired by the main session, a larger bundle by its `t26_<id>` subagent, which reruns the same sequence. When the `t26_<id>` subagent submits (`$H=검증`), the main session extracts only the needed sections/lines/fields via `rg -n` or `grist task-show --section` and verifies the changed boundary. For an existing logger's output-format-only edit, use focused unit and logger-call results rather than opening CDP; for runtime behavior, use `task-hidden` and the whole-flow check while `$H=검증`. `Complete_task` compares the actual output with the goal, records the completion list and writes `$H=확인`; the user alone sets `$H=완료`. On failure dispatch a bounded repair to `t26_<id>` (`$H=진단`).

The only maintained script is `scripts/bin/check-correct-path` (value, template, and path boundary self-test). Do not create contract or evidence files for any test.
