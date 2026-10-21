---
name: verify-work
description: Plan, write, run, and review tests through focused, production-boundary, built-artifact, and final runtime checks.
metadata:
  short-description: Verify user-visible behavior at its real consumer
---

# Verify Work

Use this skill whenever tests are written, changed, run, investigated, or used to claim behavior. Do not read `AGENTS.md`, `Input.md`, `.ai/changes/`, Obsidian notes, or `architecture.md` in full; extract and print only the needed sections, lines, or fields with `rg -n` or `grist task-show <id> --project <dir> --section <section>`, and check the current Table26 task with `grist gate` / `grist task`. The project rules own scope and the authoritative consumer.

## Work stages

Table26 `$H` is a Choice column that stores one Korean value: `초안`, `진단`, `작업`, `교정`, `검증`, `확인`, or `완료`. English names are aliases only and are never written to `$H`. `검토`(REVIEW) and per-item `RUNTIME` are not stages. Use `focused/unit → production-boundary integration → built artifact` for each item. Run one project-level runtime whole flow (`flow-wait` → `$H=확인`) after all requested items are `검증`.

## No legacy verification artifacts

Record requirements and results in `Input.md`, the dated `.ai/changes/YYMMDD.요약명.md` Markdown record, Table26 state, command output, and the real product consumer.

## Requirement-to-consumer testing

For every user-visible requirement, map exactly one path:

`start action → producer → transport/handler → authoritative consumer → final effect`

Use the same scenario and meaningful input at every layer. A helper return, ACK, mock, source string, screenshot, build, or intermediate UI state is not final evidence. Read the persisted consumer state directly. Keep adjacent workflows out of the path and add negative-scope checks when the request names a narrow workflow.

## Boundary rules

- Identify the first failing owner boundary before changing code. Record the actual producer value and type separately from the consumer expectation.
- Preserve typed values. A single Grist cell has one declared type; reject accidental unions, implicit numeric-string conversion, `String(value)`, tuple-last-element ID guesses, and uncontracted fallback parsers.
- For external APIs, test operation identity, owner, operation, boundary, attempt, result, and thread role while masking credentials, authorization data, raw payloads, and private paths.
- Test effect cardinality and ordering, retry before/after mutation, response loss, timeout, duplicate request IDs, fan-out/fan-in, bubbling, cleanup, reload, restart, and stale-instance removal when those boundaries exist.
- For path or file behavior, inspect the actual rendered/template output, decoded value, mapped path, file existence, permissions, and targeted line/field readback via `rg -n` (never dumping full files or objects).
- For browser extensions, load the deployed artifact in the real browser, execute the actual start action, trace content script/service worker delivery, and read the final DOM, editor, download, navigation, or persisted state from the declared consumer.

## Skills for runtime and records

- Browser runtime checks (web pages, Grist widgets, browser extensions, external sites) use the `run-cdp` skill: the visible (`headless: false`) shared profile `/home/tree/.config/codex-run-cdp` on `127.0.0.1:9222`. Load the deployed extension directory into that profile; do not launch another browser or profile.
- Drive the user-visible start action and read the page with the `access-stagehand` skill (`openStagehand()`, `act`/`observe`/`extract`). Playwright scripts and per-project browser profiles are legacy and are not used for new checks.
- Read and write Table26 (`$H`, `$I`, `$J`) and other Grist data with the `access-grist` skill.

## Runtime separation

The default project test command runs unit and production-boundary checks only. An explicit project runtime command is reserved for the final whole-flow check after all Table26 items are submitted (`검증`). Do not substitute a mock browser, blank page, endpoint health, target discovery, build output, or log existence for the final consumer readback. If user-only action remains, set `$H=확인` via `flow-wait` and request one exact action only after all agent-executable checks are exhausted.

## Verification bottleneck diagnosis and source repair

- Whenever a verification or runtime check command is sent to the background (`task-*`), **always set a 5-minute early-terminating wakeup timer (`schedule` with `DurationSeconds=300`, `TimerCondition="<task-id>"`) in the same turn before ending the turn**. Never end a turn waiting on a background verification task without a 300-second wakeup timer, because a hung background process will never wake the agent on its own.
- When the 5-minute timer fires (or when verification/API exploration takes 5 minutes or more), immediately inspect the task log and intermediate step logs (`[verify-step]` / `[stagehand-step]` / `[stagehand-bottleneck]`) across each boundary (`cdp-attach`, `extension-ready`, `start-action`, `transport`, `consumer-readback`, `session-close`) to identify the exact stage where the bottleneck occurs. Do not mask it by slapping a hard command timeout on it.
- A single API lookup, gate check, or verification path taking minutes means there is a defect or bottleneck in the source code itself (e.g., blocking `stdin`/IPC reads, unclosed keep-alive WebSocket/timer handles after `session.close()`, backgrounded processes holding parent stdio pipes without `</dev/null` and `disown`, repeated full-file scans or regex compilation inside loops, or unbounded polling).
- Immediately inspect the running process and intermediate step logs, pinpoint the exact bottleneck boundary, and fix the source code so the operation completes at normal speed. Record the bottleneck cause and fix in `.ai/changes/` (`### 메모`), `.ai/docs/performance.md`, and—whenever Stagehand init/attach/script execution was involved—in `~/ai/skills/access-stagehand/references/attach-notes.md` (and `~/ai/skills/access-stagehand/SKILL.md` if the procedure changed).
- Before running any Stagehand runtime check, check the relevant sections/lines of `~/ai/skills/access-stagehand/references/attach-notes.md` (`먼저 할 일`) and the project's `.ai/docs/performance.md` via `read-architecture` or `rg -n` (never reading full files): use `session.page` / `session.pages` directly (do not call non-existent `page.frames()` or `stagehand.context`), and use `node --experimental-transform-types --input-type=module -e` when importing `.ts` files alongside `openStagehand()`.
- `openStagehand()`'s `session.close()` unrefs remaining keep-alive handles automatically and emits `[stagehand-bottleneck]` every 60s while open; in custom scripts, ensure all timers/sockets are closed or call `process.exit(0)` after `await session.close()` in `finally`.

## Failure and completion

When a test fails, keep the item open, set `$H=진단`, preserve the combined reproducing state, and have the row's `t26_<id>` subagent repair the owning boundary and rerun the same sequence (the main session never modifies product code or tests directly). When the `t26_<id>` subagent submits (`$H=검증`), the main session extracts only the needed sections/lines/fields via `rg -n` or `grist task-show --section`, runs `task-hidden` and the whole-flow check (`flow-wait` → `$H=확인`), and runs `task-complete` (`$H=완료`) after user confirmation, or dispatches a bounded repair to `t26_<id>` (`$H=진단`).

The only maintained script is `scripts/bin/check-correct-path` (value, template, and path boundary self-test). Do not create contract or evidence files for any test.

