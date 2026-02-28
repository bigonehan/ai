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

## Hard Block Rule
- If the final draft response contains any banned phrase, do not send it.
- Regenerate the full sentence/paragraph until banned phrase count is 0.
- This hard block check is mandatory for `commentary`, `final`, and `summary` channels.
- When user is angry, keep response short and action-only; never use agreement-preface words.

## Request Summary Output Rule
- For every user request, before starting work, output using this exact 2-line format:
- Line 1: `요구사항 요약 > [${행동 설명:생성, 추가, 삭제, 변경}] ${대상}은 기능 한줄 요약`
- Line 2: `[결과] : 일어날 결과`
- Keep this output concise and always place it immediately before implementation.

## Screenshot Path Memory Rule
- When the user says `current.png`, resolve it to this fixed directory by default:
  - `/mnt/c/Users/tende/Pictures/Screenshots/current.png`
- If only folder context is needed, use:
  - `/mnt/c/Users/tende/Pictures/Screenshots`
- Treat this mapping as persistent unless the user explicitly changes it.

## Plan First Rule (Permanent)
- Before any source code edit, create or update `plan.md` first.
- Minimum `plan.md` structure is mandatory: `문제`, `해결책`, `검증`.
- If `plan.md` is missing, stop editing source and write `plan.md` first.

## Retry Loop Rule (Permanent)
- Required execution loop:
  1) 문제 제시 + 해결책 + 검증 기준 설정후 `plan.md` 생성 
  2) 해결책 시도
  3) 검증 실행
  4) 실패 시  `feedback.md` 생성후 이를 바탕으로 `plan.md`문제를 재설계 
  5) 재 정비된 plan.md 문서를 바탕으로 처음부터 전체 재시작
- On failure, write/update `feedback.md` and append retry reason to `plan.md` before restarting.
- Do not stop at intermediate logs only; continue until pass or max retry reached.

## Rule-First Enforcement (Highest Priority)
- On any new user behavioral instruction, update `AGENTS.override.md` first before running commands or editing source.
- If execution already started, stop running process first, write rule, then resume work.
- This rule has higher priority than implementation speed.

## Temp Auto Loop Rule (Permanent)
- When user requests `orc cli` validation in `/home/tree/temp`, run iterative loop with this order:
  1) write/update `plan.md`
  2) remove and recreate `/home/tree/temp`
  3) run `orc auto` for requested app
  4) if failed, write `/home/tree/temp/feedback.md` with 문제/미해결점
  5) reflect feedback into next plan and restart from step 1
- Keep looping until verification passes or hard technical blocker is confirmed.

## Feedback->Plan Merge Rule (Highest Priority)
- After any failure, write/update `feedback.md` first with `문제` and `미해결점`.
- Then update `plan.md` by merging prior plan + new feedback deltas.
- The updated `plan.md` must include:
  - new/changed problem statements
  - concrete solution steps
  - forced execution item (must-apply action)
- Do not run the next attempt unless merged `plan.md` has been written.

## Forced Resolution Rule
- Retry is not a blind rerun.
- Every retry must apply at least one concrete change from updated `plan.md` before execution.
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

## Failure-Solution Mandatory Rule (Highest Priority)
- If any failure cause is detected, `plan.md` must be updated with a concrete fix for that exact cause before next run.
- `plan.md` update is invalid if it only repeats the problem without actionable solution steps.
- Retry execution is blocked until the failure->solution mapping is explicitly written in `plan.md`.
