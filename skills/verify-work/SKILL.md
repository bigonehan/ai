---
name: verify-work
description: Plan, write, run, and review tests through focused, production-boundary, built-artifact, and final runtime checks.
metadata:
  short-description: Verify user-visible behavior at its real consumer
---

# Verify Work

Use this skill whenever tests are written, changed, run, investigated, or used to claim behavior. Read the nearest project `AGENTS.md`, `Input.md`, product test commands, and the current Table26 task first. The project rules own scope and the authoritative consumer.

## Work stages

Table26 `$H` is a Choice column that stores one Korean value: `초안` (SCOPE), `진단` (DIAGNOSE), `교정` (REPAIR), `검증` (VERIFY), `대기` (HANDOFF), or `완료` (DONE). English names are aliases only and are never written to `$H`. `검토`(REVIEW) and per-item `RUNTIME` are not stages. Use `focused/unit → production-boundary integration → built artifact` for each item. Run one project-level runtime whole flow only after all requested items are `완료` and Table26 `$C` is `완료`.

## No legacy verification artifacts

Record requirements and results in `Input.md`, the dated `.ai/changes/YYMMDD.요약명.md` Markdown record, Table26 state, command output, and the real product consumer.

## Requirement-to-consumer testing

For every user-visible requirement, map exactly one path:

`start action → producer → transport/handler → authoritative consumer → final effect`

Use the same scenario and meaningful input at every layer. A helper return, ACK, mock, source string, screenshot, build, checksum, or intermediate UI state is not final evidence. Read the persisted consumer state directly. Keep adjacent workflows out of the path and add negative-scope checks when the request names a narrow workflow.

## Boundary rules

- Identify the first failing owner boundary before changing code. Record the actual producer value and type separately from the consumer expectation.
- Preserve typed values. A single Grist cell has one declared type; reject accidental unions, implicit numeric-string conversion, `String(value)`, tuple-last-element ID guesses, and uncontracted fallback parsers.
- For external APIs, test operation identity, owner, operation, boundary, attempt, result, and thread role while masking credentials, authorization data, raw payloads, and private paths.
- Test effect cardinality and ordering, retry before/after mutation, response loss, timeout, duplicate request IDs, fan-out/fan-in, bubbling, cleanup, reload, restart, and stale-instance removal when those boundaries exist.
- For path or file behavior, inspect the actual rendered/template output, decoded value, mapped path, file existence, permissions, and byte readback.
- For browser extensions, load the deployed artifact in the real browser, execute the actual start action, trace content script/service worker delivery, and read the final DOM, editor, download, navigation, or persisted state from the declared consumer.

## Skills for runtime and records

- Browser runtime checks (web pages, Grist widgets, browser extensions, external sites) use the `run-cdp` skill: the visible (`headless: false`) shared profile `/home/tree/.config/codex-run-cdp` on `127.0.0.1:9222`. Load the deployed extension directory into that profile; do not launch another browser or profile.
- Drive the user-visible start action and read the page with the `access-stagehand` skill (`openStagehand()`, `act`/`observe`/`extract`). Playwright scripts and per-project browser profiles are legacy and are not used for new checks.
- Read and write Table26 (`$H`, `$I`, `$J`) and other Grist data with the `access-grist` skill.

## Runtime separation

The default project test command runs unit and production-boundary checks only. An explicit project runtime command is reserved for the final whole-flow check after all Table26 items are complete. Do not substitute a mock browser, blank page, endpoint health, target discovery, build output, or log existence for the final consumer readback. If user-only action remains, set `$H=대기` and request one exact action only after all agent-executable checks are exhausted.

## Verification bottleneck diagnosis and source repair

- When verification or API/runtime exploration takes 5 minutes or more, immediately add intermediate step logging (`[verify-step] <boundary> <elapsed_ms>`) across each boundary (`cdp-attach`, `extension-ready`, `start-action`, `transport`, `consumer-readback`, `session-close`) to identify the exact stage where the bottleneck occurs. Do not mask it by slapping a hard command timeout on it.
- A single API lookup, gate check, or verification path taking minutes means there is a defect or bottleneck in the source code itself (e.g., blocking `stdin`/IPC reads, unclosed keep-alive WebSocket/timer handles after `session.close()`, backgrounded processes holding parent stdio pipes without `</dev/null` and `disown`, repeated full-file scans or regex compilation inside loops, or unbounded polling).
- Immediately inspect the running process and intermediate step logs, pinpoint the exact bottleneck boundary, and fix the source code so the operation completes at normal speed. Record the bottleneck cause and fix in `.ai/changes/` (`### 메모`) or `.ai/docs/performance.md`.
- `openStagehand()`'s `session.close()` unrefs remaining keep-alive handles automatically; in custom scripts, ensure all timers/sockets are closed or call `process.exit(0)` after `await session.close()` in `finally`.

## Failure and completion

When a test fails, keep the item open, set `$H=진단`, preserve the combined reproducing state, repair the owning boundary, and rerun the same sequence. When the implementation agent finishes, the main session reads the diff and test output and compares them with the `Input.md` requirement. It sets `$H=완료` on a passing predicate, or dispatches one bounded repair and sets `$H` back to `진단`. A separate verifier agent runs only under the conditions in `work-project`.

The only maintained script is `scripts/bin/check-correct-path` (value, template, and path boundary self-test). Do not create contract or evidence files for any test.

