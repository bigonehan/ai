---
name: end-project
description: End-of-work verification for /home/tree/extensions projects. Use when the main session or a t26_<id> agent has finished its work, and before any final message or question to the user. It runs the grist gates, which rerun the tests, take the Jev judgment and update Grist, and it lists the end conditions that the Stop hook enforces.
metadata:
  short-description: Verify tests, Jev and Grist before ending a turn
---

# End Project

Run this Skill whenever the main session or a `t26_<id>` agent finishes its work, before the final message or a question to the user. The project `AGENTS.md` header (`## 작업 상태`) and Grist Table26 are the state. Every Grist call goes through `~/ai/scripts/bin/grist`.

## End conditions

- The main session may end its turn only when every row in the header is `확인` or `완료`. A `t26_<RawID>` agent may end only when its own row is `검증` or `완료`. `~/ai/scripts/bin/completion-gate` blocks any other end (Stop and SubagentStop).
- The goal (changes `### 목표` and `$K`) and `$H` are written only by the `grist` commands below. Scope, hardcoding and goal-match judgments run inside those commands and the hook (`access-jev`). Each command's failure output is the next task. Agents only read the goal; they never ask the user or write `확인`/`완료`.
- Before asking the user a question or for an action, write `$H=확인` and the reason in `$D`. Before asking for an action on a specific target (work, scene, row), confirm that the target and its inputs are on the current screen or filter.
- `확인` only means waiting for a user-only action or user confirmation. It is not a reason to end with a progress or failure report; hook J2 blocks that. A new user message or a `wait_agent` `aborted` result is not a stop instruction: reread Table26 and the remaining steps and continue.
- CDP, runner or Stagehand failures, `runtime-unverified` and stale artifacts are not reasons to end. Write the first failing boundary to `$I` and continue: fix the cause → rebuild/deploy → reconnect → rerun the same flow → consumer readback.

## End verification: tests, Jev, Grist

Do not run `jev` directly (see `access-jev`). The gates below rerun the tests, take the Jev judgment and write Grist:

| Who | When | Command | Result |
|---|---|---|---|
| `t26_<id>` agent | its implementation and unit testing (`교정`) are done | `grist task-submit <id> --project <dir>` | reruns the red test and checks the red record, scope and literals, then Jev J1 output-vs-goal → `$H=검증`. On failure, write `$H=진단` with `$I`/`$J`, fix, and rerun. |
| main | every header row is `검증`, hidden cases (`grist task-hidden`) and `#### 확인 (#<id>)` under `### 구현` are recorded | `grist flow-wait --project <dir>` | reruns the submitted and hidden tests, Jev on `[화면]` lines → `$H=확인` with the `[사용자]` lines in `$D`. Failed rows go to `진단`. |
| main | the user confirmed | write `#### 완료 (#<id>)` under `### 구현` as a numbered list of around 5 lines (`1. [📦\|📬\|⚙️] 한줄요약`, without `결과`/`상세`), then `grist task-complete <id> --project <dir>` | sets `$H=완료`. `unchecked` or `missing_evidence` output is the next task. |
| main | the final message | Stop hook | Jev J2 classifies the message as 완료 보고, 사용자 질문, 실패를 결론으로 종료, 진행 보고, 질문 답변 or 대기 보고. In any `JEV_MODE`, even when every row is `확인`/`완료`, it blocks 진행 보고 and 실패를 결론으로 종료. In `enforce` mode it also blocks a question unless it asks for one of the four allowed actions (login or authentication, payment or cost, an irreversible external send, an OS app action outside the browser), a `.py` consent naming the path and reason, or a choice between concrete interpretations before editing. Two J2 blocks without a `$H` change let the turn end, recorded in `$D` and announced with nf; blocks for rows not yet `확인`/`완료` are released after three. |

Test records in `.ai/changes` are `- 목적 : <label>`, `- 결과 : 성공|실패`, `- 상세 : <one line>` only (see `work-project` Management records).

Then read `grist gate --cwd <dir>` and end only when the end conditions above hold. After `flow-wait`, the final report asks the user to check the `[사용자]` lines (or the delivered behaviour when there are none) and says that the rows become `완료` once the user confirms.
