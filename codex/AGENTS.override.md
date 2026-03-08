# Agents Override Rules

## Absolute Phrase Ban Rule
- The assistant must never use agreement-preface phrases in any response.
- Absolute forbidden phrases:
  - `맞습니다`
  - `맞아요`
  - `인식했습니다`
  - `알겠습니다`
  - `네, 맞습니다`
  - `맞습니다.`
  - `네 맞습니다`
  - `그렇습니다`
- This ban applies to all channels, all contexts, and all response lengths.
- Pre-send guard: before every response, scan final text for banned phrases.
- If any banned phrase appears, rewrite the sentence and re-check before sending.
- Responses must start directly with outcome/action, without acknowledgement-preface wording.
- If the final draft response contains any banned phrase, do not send it.
- Regenerate the full sentence/paragraph until banned phrase count is 0.
- This hard block check is mandatory for `commentary`, `final`, and `summary` channels.
- When user is angry, keep response short and action-only; never use agreement-preface words.

## Request Summary Output Rule
- For every user request, before starting work, output with label and description split across separate lines.
- Line 1: `요구사항 요약 >`
- Line 2: `[${행동 설명:생성, 추가, 삭제, 변경}]`
- Line 3: `${대상}은 기능 한줄 요약`
- Line 4: `[결과]`
- Line 5: `일어날 결과`
- Keep this output concise and always place it immediately before implementation.

## File Path Display Rule (Output)
- In change reports and file references, display paths in abbreviated form using only parent folder + filename.
- Format: `.../<parent>/<file>`
- Example: `/home/tree/home/template/web/blog/main.tsx` -> `.../blog/main.tsx`
- Apply this formatting rule to `commentary`, `final`, and `summary` outputs.

## Screenshot Path Memory Rule
- When the user says `current.png`, resolve it to this fixed directory by default:
  - `/mnt/c/Users/tende/Pictures/Screenshots/current.png`
- If only folder context is needed, use:
- Treat this mapping as persistent unless the user explicitly changes it.

## Desktop Path Memory Rule
- Treat the desktop path associated with the `current.png` user context as fixed:
  - `C:\Users\tende\Desktop` (`/mnt/c/Users/tende/Desktop`)
- When asked to copy/move files to desktop for that user context, use this path by default unless explicitly changed.

## Startup Override Load Rule (Highest Priority)
- At the first step of every task, load `~/ai/codex/AGENTS.override.md` before any file search or path inference.
- For `current.png`, skip repository-wide filename search first and apply the fixed mapping immediately.
- If `current.png` handling does not use `/mnt/c/Users/tende/Pictures/Screenshots/current.png`, treat it as process violation and correct before continuing.

## Mandatory Startup Checklist Rule (Highest Priority)
- Before any implementation command, print and satisfy this checklist in order:
  1) `override_loaded`
  2) `current_png_mapped`
  3) `todo_checked`
  4) `execution_scope_confirmed`
- If any item is not satisfied, block implementation/edit/test commands and resolve the missing item first.
- If checklist output is omitted, treat it as critical process failure and restart from checklist step.

## Todo First Rule (Permanent)
- Before any source code edit, create or update `todo.md` first.
- `plan.md` file creation/update is prohibited for normal work.
- If any flow/tool generates `plan.md`, do not use it as source of truth; migrate content into `todo.md`.
- Minimum `todo.md` structure is mandatory and must use these headings exactly:
  - `# problem`
  - `# tasks`
  - `# check`
- Section meaning is fixed:
  - `# problem`: 해결해야 하는 문제를 명시한다.
  - `# tasks`: 문제를 해결하기 위해 수행할 작업 리스트를 작성한다.
  - `# check`: 어떻게 검증할 것인지(명령/기준)를 작성한다.
- If `todo.md` is missing, stop editing source and write `todo.md` first.

## Retry Loop Rule (Permanent)
- Required execution loop:
  1) 문제 제시 + 작업 단계 + 검증 기준 설정후 `todo.md` 생성
  2) 해결책 시도
  3) 검증 실행
  4) 실패 시  `feedback.md` 생성후 이를 바탕으로 `todo.md`를 재설계
  5) 재 정비된 `todo.md` 문서를 바탕으로 처음부터 전체 재시작
- On failure, write/update `feedback.md` and append retry reason to `todo.md` before restarting.
- Do not stop at intermediate logs only; continue until pass or max retry reached.

## Rule-First Enforcement (Highest Priority)
- On any new user behavioral instruction, update `AGENTS.override.md` first before running commands or editing source.
- If execution already started, stop running process first, write rule, then resume work.
- This rule has higher priority than implementation speed.

## Temp Auto Loop Rule (Permanent)
- When user requests `orc cli` validation in `/home/tree/temp`, run iterative loop with this order:
  1) write/update `todo.md`
  2) remove and recreate `/home/tree/temp`
  3) run `orc auto` for requested app
  4) if failed, write `/home/tree/temp/feedback.md` with 문제/미해결점
  5) reflect feedback into next todo and restart from step 1
- Keep looping until verification passes or hard technical blocker is confirmed.

## Feedback->Todo Merge Rule (Highest Priority)
- After any failure, write/update `feedback.md` first with `문제` and `미해결점`.
- Then update `todo.md` by merging prior todo + new feedback deltas.
- The updated `todo.md` must include:
  - new/changed problem statements
  - concrete solution steps
  - forced execution item (must-apply action)
- Do not run the next attempt unless merged `todo.md` has been written.

## Forced Resolution Rule
- Retry is not a blind rerun.
- Every retry must apply at least one concrete change from updated `todo.md` before execution.
- If no new change is applied, stop and mark as process violation.

## Forbidden Phrase Re-Assert (Hard Block)
- The token `맞습니다` is absolutely forbidden in all channels.
- If response draft contains `맞습니다`, block send and regenerate text.

## Zero-Tolerance Ban (Ultimate)
- The exact token `맞습니다` is permanently banned with zero exceptions.
- Apply ban to every channel and every draft stage (`analysis/commentary/final/summary`).
- If token is detected at any stage, abort that draft immediately and regenerate from scratch.
- Do not emit partial output containing the banned token.
- Violation is treated as critical process failure.

## No-`맞습니다` Output Gate (Hard Block)
- Before sending any response, run a final text gate for the exact token `맞습니다`.
- If found once, sending is blocked; rewrite the full response and re-check until count is 0.
- This gate is mandatory even when user explicitly requests that token.

## Failure-Solution Mandatory Rule (Highest Priority)
- If any failure cause is detected, `todo.md` must be updated with a concrete fix for that exact cause before next run.
- `todo.md` update is invalid if it only repeats the problem without actionable solution steps.
- Retry execution is blocked until the failure->solution mapping is explicitly written in `todo.md`.

## CLI Execute-First Interpretation Rule
- If the user says phrases like `호출해서 실행`, `실행해봐`, `돌려봐`, interpret the request as run existing CLI command first, not implementation.
- In this case, do not edit code/docs unless the user explicitly asks to implement/change.
- Output must prioritize executed command and result summary.
- If command execution hangs, report hang reason first and ask whether to stop/retry with timeout.

## Port Ownership Override
- `packages/ports/*` 레이어는 사용하지 않는다.
- 포트 인터페이스는 각 도메인 패키지에서 직접 관리한다.
- 파일 규칙: `packages/domains/<domain>/src/<domain>_port.ts`
- 소비자는 `@domain/<domain>`에서 포트 타입을 import 한다.

## No-Hardcoding Default Rule
- 사용자가 하드코딩을 명시적으로 요청하지 않으면 하드코딩 분기 구현을 금지한다.
- 생성/판단/추론은 `assets/code/prompts` 기반 LLM 경로를 우선 사용한다.
- 예외적으로 fallback이 필요하면 최소/범용 형태만 허용하고, 도메인 특화 하드코딩은 금지한다.

## Regret Skill Trigger Rule (Highest Priority)
- If the assistant output includes the token `잘못` in any channel, run the `regret` skill immediately in the same turn.
- Required action order:
  1) Append one item to `/home/tree/ai/skills/regret/references/report.md` under `# 잘못한점`.
  2) Append one item to `/home/tree/ai/skills/regret/references/report.md` under `# 개선할점`.
  3) State that the regret skill execution record was written.
- This rule is mandatory for `commentary`, `final`, and `summary` channels.

## Error-Only Input Auto-Handle Rule (Highest Priority)
- When the user input consists only of error messages/log output, automatically execute the full cycle without extra confirmation:
  1) identify root cause
  2) implement fix
  3) run verification
  4) report final result
- Do not stop at diagnosis-only responses for error-only inputs unless execution is technically blocked.

## User Critique Immediate-Execute Rule (Highest Priority)
- 사용자가 문제점/오해를 지적하면, 해당 지적 사항은 추가 확인 질문 없이 즉시 실행 지시로 해석한다.
- "이건 지적이다" 유형 입력은 분석-only 응답을 금지하고 즉시 수정/실행/검증 순서로 진행한다.
- 동일 턴에서 규칙 반영과 구현을 모두 수행한다.

## Current PNG Zero-Miss Protocol (Hard Block)
- When user says `current.png`, always open `/mnt/c/Users/tende/Pictures/Screenshots/current.png` first.
- Do not run repository-wide search (`rg --files ... current.png`) before this direct open.
- If the file does not exist at that fixed path, report that exact path is missing and ask only for replacement path/file.
- This protocol is mandatory in every session and has higher priority than convenience search.

## "다음부터" Improvement Logging Rule (Highest Priority)
- If the assistant says phrases equivalent to `다음부터` (for example: `앞으로는`, `재발 방지로`) in any response, it must first identify at least one concrete process improvement.
- The identified improvement must be written to `/home/tree/ai/codex/AGENTS.override.md` in the same turn before finishing the response.
- Response-only promises without rule update are invalid and treated as process violation.
