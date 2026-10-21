---
name: improve-performance
description: Diagnose, improve, and verify product performance regressions through delegated, evidence-led work. Use for performance, latency, rendering, responsiveness, throughput, memory, or resource-use improvements; not for ordinary feature work without a performance concern.
---

# Performance Improvement

The main session owns scope, evidence, and completion. It does not modify product code, configuration, tests, build artifacts, or deployment files for a performance task: delegate every product and test mutation to a `t26_<id>` subagent (the main session never modifies product code or tests directly). The main session may inspect evidence, coordinate agents, and reject or re-delegate incomplete work.

Work records follow `work-project`: the nearest project `AGENTS.md`, `Input.md`, the dated `.ai/changes/YYMMDD.요약명.md` Markdown record, Table26 `$H` (Korean choice values `초안`/`진단`/`작업`/`교정`/`검증`/`확인`/`완료`), command output, product logs, and the authoritative consumer.

## Establish the diagnostic boundary before a patch

1. Do not read `AGENTS.md`, `Input.md`, `.ai/changes/`, Obsidian notes, or `architecture.md` in full; extract and print only the needed sections, lines, or fields (such as `# 상태` / `runtime :`) with `read-architecture`, `rg -n`, or `grist task-show --section`. Identify the exact user-visible action, the authoritative consumer, the target package/component, and adjacent workflows that must remain unchanged.
2. Delegate a bounded diagnostic task before any functional patch. It must trace the actual production loader and map `producer → transport → consumer`, including the real owner of each hot path.
3. Inventory the full target package rather than only the reported node or component. Include registered modules and every relevant draw/render, pointer, keyboard/input, DOM-widget/rebuild, serialization, layout read/write, canvas-dirty, lifecycle, and cleanup path. Keep canvas pan, node drag, link drag, and text/input interaction as separate observation scenarios.
4. If durable diagnostics are absent or insufficient, delegate instrumentation first. A product with settings must expose a persistently stored `개발 모드`/development-mode toggle. At the logger boundary, disabled mode must produce zero diagnostic event creation, transport, and file writes.

## Diagnostic logging

- Send structured diagnostic records to an existing product-owned, Host-readable persistent log consumer; browser-console output and screenshots are not authoritative evidence.
- Every record includes a stable operation ID, owner, operation, boundary, attempt, result, and thread role. Record safe counters and durations for the mapped hot paths; exclude tokens, credentials, raw private payloads, and full private paths.
- Collect a baseline from the actual runtime before selecting a functional optimization. A patch must target the first measured failing owner/boundary, not a plausible-but-unmeasured helper.
- Tests for instrumentation must prove persisted toggle/reload behavior, zero disabled-mode logging work, required record fields, distinct owners for distinct failures, and an isolated mutant that removes or mislabels a required boundary.

## Delegated repair loop

- Once Host diagnostics identify the first failing owner/boundary, immediately delegate one bounded product-repair task for that owner. Measurement, diagnostics, or an explanation alone are never an outcome for an active performance task.
- Each `t26_<id>` subagent prompt contains only: the instruction to run `read-architecture` and extract only needed sections/lines via `rg -n` or `grist task-show --section` (never reading full files), one owner boundary, the editable paths, and the completion condition (tests, replay, and consumer readback; tests print only target fields/lines). The agent returns plain text: cause, changed files, test output, and measured counters/durations.
- After each agent result, the main session inspects only the needed lines/fields of the diff, tests, and logs via `rg -n` or `grist task-show --section`. A green test, build, or agent claim is not completion evidence.
- If evidence is missing, the owner is wrong, the package inventory is incomplete, or runtime readback fails, dispatch the next bounded task at the first unresolved boundary. Escalate repeated failures to the loader, shared runtime, or owning configuration rather than retrying the same patch blindly.
- Call a separate reviewer agent only when the repair changes a path that mutates external data, extends a shared Bridge contract, or the user asks for review. Never review a review.

## Verification and handoff

- For every changed owner, verify focused/unit and production-boundary integration behavior, then load the deployed artifact into the real runtime and replay the same production start action. Read the Host-visible logs and authoritative consumer with the same operation ID.
- Perform this sequence without user wait while an agent-executable step remains: repair → focused/integration test → deploy artifact → reload or reconnect the same runtime → replay the same start action → Host-log and authoritative-consumer readback.
- Compare the four interaction scenarios separately: canvas pan, node drag, link drag, and text/input interaction. Record the applicable counters/durations and the resulting consumer state in the change record; do not substitute a lower-level helper or another interaction.
- Treat a user-run execution as a scarce final observation. Exhaust agent-executable diagnostics, tests, artifact loading, and readback first; when exactly one user-exclusive action remains, set `$H=확인` via `flow-wait` and request that single complete production flow, then read its logs and consumer.
- Treat all prior user-run Host records and consumer readbacks for the current artifact as one accumulated evidence set. Do not request the same manual interaction again because an agent result or measurement is incomplete. A second request is allowed only after a newly deployed artifact has replaced the one used for the prior run; bundle every remaining observation into exactly one replay.
- If the user rejects or calls out a repeated manual-run request for the current artifact, do not reissue it or reword it. Continue from accumulated Host evidence and agent-executable work.
- A performance task stays active until the authoritative consumer confirms the completion condition in the project `AGENTS.md`. Missing runtime evidence is a new bounded work item while an agent-executable repair, reload, replay, or readback remains.
- Host-log delivery, receiver acknowledgement, a build, or a status summary is not performance completion. Before the final response, the main session compares every active `Input.md` requirement with the latest diff, tests, logs, and consumer readback, and transitions to `$H=확인` (or `$H=완료` upon user confirmation) only when all pass. Otherwise it dispatches the next bounded repair.
