---
name: Complete_task
description: Own the Table26 task completion evidence, project end check, and Stop/SubagentStop response decision for initialized extensions projects. Use before an agent or main session ends and when a user confirms a result.
---

# Complete_task

This Skill is the single owner of task completion judgment, response ending, and project ending for `/home/tree/extensions`. `manage-project` owns request intake and implementation; `verify-work` owns test methods. The Rust executable `~/ai/scripts/bin/task-complete` owns the completion state machine and is called directly by Stop and SubagentStop hooks. Do not invoke `jev` directly.

## State and ownership

Task steps `add`, `ready`/`red`, `submit` and `check` share one `TaskStore` trait in the grist library (`~/ai/scripts/grist/src/task.rs`). `TaskChain` writes each step to its stores in a fixed order: `GateStore` (`.ai/gate/<id>.json` test command and submitted output only), `ChangesStore` (`.ai/changes`) and `GristStore` (Table26 `$H`). `task-complete` links that library and calls `check` directly; there is no separate completion command or gate completion marker.

1. Skip: the user's Grist `$H=완료` outranks every other record. Such a row is never judged, needs no `# 완료` entry, and never blocks a Stop or J2, even when changes, gate evidence or J1 would call it unfinished; `grist sync` moves its changes item to `# 완료`. A row listed in `# 완료` whose `$H` is `확인` is not judged again either. Any other `$H` on a listed row (reopened by `task-ready`/`task-red`, or a previous `$H` write that failed) is judged again.
2. Evidence and J1: a row with a gate preparation record needs `$H=검증`/`확인`, the saved `task-submit` output, and the latest successful flow confirmation; the hook never reruns tests. When `submit.changed` is 1 or 2, the latest successful `task-confirm` supplies the flow evidence without `flow-wait`; other code rows need `flow-wait` evidence. A `[✒️]` row without `task-submit` uses its latest `task-confirm` conflict check. Jev J1 compares the per-goal `- 근거 N` lines from `task-confirm --evidence` plus the flow confirmation (not the test output) with `### 목표`; it judges coverage of each goal line, not whether the evidence is true, so it never replaces the hidden case or flow check the path requires. Missing evidence lines block the row; a failed or uncertain enforced judgment leaves the row open and adds nothing.
3. Check chain: after J1 passes, `ChangesStore` writes numbered `#### 완료 (#id)` entries (implemented steps, or the latest successful `#### 확인` summary for a text-only row) and moves the row from `# 작업` to `# 완료` once; `GristStore` then writes `$H=확인`. Every write is idempotent, so a repeated call heals a partial write and leaves a finished row unchanged.
4. `COMPLETE_WORK`: compare the current work's row IDs with the `# 완료` list and current Table26 states. The main session may end only when every row is user-set `완료`, or `확인` and represented in `# 완료`. A `t26_<id>` agent may end after its own submitted tests pass; its ending cannot set `$H=완료`.

For the main Stop hook, `grist task-add` records each created row under the current `CODEX_THREAD_ID` in `~/.codex/tmp/task_complete/session_rows/`. The hook applies J1, the completion list, and stage checks only to rows recorded for that session. A previous session's rows alone do not block this session's response. A session with no recorded rows (a question answered without `manage-project`) ends freely unless files changed: SessionStart runs `task-complete --session-start` to store working-tree snapshots of the session's repo and `~/ai`, and the main Stop blocks once when files other than `.ai/` changed since then, asking to start `manage-project`; the same change list is not blocked again, so another session's edits never keep it open. Explicit `task-complete --row <id>` and `--project <dir>` calls retain their requested row or project scope.

If a previously listed row reenters `진단`/`검증` after a repair, keep its list entry without duplication and run the normal verification again. The project flow must still find that row in `# 완료` and return it to `확인` before ending.
When a session crosses midnight, an empty new daily changes file does not replace the latest file that actually lists the current work. Reuse the known row and record rather than creating a new completion target.

The user owns final `$H=완료`. An agent, the main session, a hook, and `task-complete` never write it. A user's “완료” message confirms their review; it is not an instruction to run an automatic final-status transition. Use the already known row and record when answering it. If the user changes `$H` to `완료` in Grist, `grist sync` reflects that status without duplicating completion entries.

## Before ending a response

- Run the focused and final consumer checks required by `verify-work`; keep their actual outputs in the existing row records. The main performs the whole flow after agent submission; `flow-wait` records its evidence while the row stays at `검증`. Do not treat a build or an ACK alone as completion evidence.
- Stop/SubagentStop calls `task-complete` directly with the hook event JSON. It reads the owning project's `# 상태` and current `.ai/changes` rows, performs the applicable state checks, and emits the hook decision. The main Stop path uses Jev J2 to distinguish a finished response from a progress report or an unsupported request for user action. A pending user reply or subagent wait result alone does not finish the work.
- A main final response or user-facing question requires the work check to pass. Only the main sends `nf -m "<구체적 결과>"` once just before that response; subagents and the hook never send it. If the command returns a running session, wait on that same process.
- If a required check fails, record the actual failure in `$I`, the identified cause in `$J`, use that update's `row` line, and continue the bounded repair. A user's stated cause is the repair target; do not rediscover it through unrelated product checks.

## Interfaces

- `~/ai/scripts/bin/task-complete --hook` reads the Stop/SubagentStop event JSON from stdin and emits the hook JSON decision.
- `~/ai/scripts/bin/task-complete --row <id> --project <dir>` checks one existing row and updates its completion record and `$H=확인` after passing evidence.
- `~/ai/scripts/bin/task-complete --project <dir>` checks whether the current work's completion list is full. These commands may read Grist through the existing `grist` access path and Jev through the existing `access-jev` path; they must not use a second API key or transport.

Historical `.ai/changes` entries remain as facts. New active instructions and executable paths use this Skill exclusively.
