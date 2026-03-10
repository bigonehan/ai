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
- This hard block check is mandatory for every channel and every draft stage (`analysis`, `commentary`, `final`, `summary`).
- When user is angry, keep response short and action-only; never use agreement-preface words.
- The exact token `맞습니다` is permanently banned with zero exceptions.
- Before sending any response, run a final text gate for the exact token `맞습니다`.
- If the token is detected at any stage, abort that draft immediately and regenerate from scratch.
- Do not emit partial output containing the banned token.
- Violation is treated as critical process failure.

## Request Summary Output Rule
- For every user request, before starting work, output with label and description split across separate lines.
- Line 1: `[요약]`
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
- For `current.png`, skip repository-wide filename search first and apply the fixed mapping immediately.
- If `current.png` handling does not use `/mnt/c/Users/tende/Pictures/Screenshots/current.png`, treat it as process violation and correct before continuing.
- Always open `/mnt/c/Users/tende/Pictures/Screenshots/current.png` first.
- Do not run repository-wide search (`rg --files ... current.png`) before this direct open.
- If the file does not exist at that fixed path, report that exact path is missing and ask only for replacement path/file.
- This protocol is mandatory in every session and has higher priority than convenience search.

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
  4) 실패 시  `.project/feedback.md` 생성후 이를 바탕으로 `todo.md`를 재설계
  5) 재 정비된 `todo.md` 문서를 바탕으로 처음부터 전체 재시작
- On failure, write/update `.project/feedback.md` and append retry reason to `todo.md` before restarting.
- Do not stop at intermediate logs only; continue until pass or max retry reached.

## Rule-First Enforcement (Highest Priority)
- On any new user behavioral instruction, update `/home/tree/ai/codex/AGENTS.override.md` first before running commands or editing source.
- If execution already started, stop running process first, write rule, then resume work.
- This rule has higher priority than implementation speed.

## Temp Auto Loop Rule (Permanent)
- When user requests `rw cli` validation in `/home/tree/temp`, run iterative loop with this order:
  1) write/update `todo.md`
  2) remove and recreate `/home/tree/temp`
  3) run `rw auto` for requested app
  4) if failed, write `/home/tree/temp/.project/feedback.md` with 문제/미해결점
  5) reflect feedback into next todo and restart from step 1
- Keep looping until verification passes or hard technical blocker is confirmed.

## Feedback->Todo Merge Rule (Highest Priority)
- After any failure, write/update `.project/feedback.md` first with `문제` and `미해결점`.
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

## "다음부터" Improvement Logging Rule (Highest Priority)
- If the assistant says phrases equivalent to `다음부터` (for example: `앞으로는`, `재발 방지로`) in any response, it must first identify at least one concrete process improvement.
- The identified improvement must be written to `/home/tree/ai/codex/AGENTS.override.md` in the same turn before finishing the response.
- Response-only promises without rule update are invalid and treated as process violation.

## Explicit ORC Workflow Execution Rule
- If the user explicitly names `orc-cli-workflow` or provides that skill block, execute the requested flow with real `orc` commands first.
- Do not substitute manual implementation for the workflow unless `orc` is unavailable or the user explicitly permits fallback.
- If manual implementation already started, stop and switch to `orc` stage execution in the same turn.

## Skill Invocation Strictness Rule
- A skill invocation command exists to constrain execution, not to inspire an approximate workflow.
- When a user invokes a skill explicitly, do not mix in self-directed implementation choices before the skill-defined command path is completed.
- If the user criticizes skill misuse, stop every in-flight process tied to that task, answer the cause in one line, then continue only with the exact skill path or wait for the next instruction.

## Skill Priority Override Rule
- Any explicitly invoked skill is higher priority than default autonomy, implementation-first behavior, or general coding heuristics.
- On skill invocation, the assistant must read the skill file first, extract the required execution order, and follow that order before considering any fallback.
- If the skill prescribes concrete commands, those commands must be attempted in that sequence unless a hard blocker is observed.
- Fallback implementation is allowed only after stating the blocker and only if the user permits deviation.
- Mixing a skill workflow with self-directed manual implementation in the same task is prohibited.

## ORC Completion Evidence Rule
- For `rust-orc` tmux workflow handling, completion and failure must be judged from generated files such as `.project/plan.yaml` and `.project/drafts.yaml`, not from pane capture text.
- Pane capture may be used only as auxiliary debugging evidence and must not be the primary success condition.
- If any manager-worker flow still uses pane text tokens as the completion gate, replace it with file-state verification in the same task.

- 2026-03-09: 기본 AGENTS 선행 읽기 대상은 `~/ai/codex/AGENTS.override.md`로 고정한다. codex 실행 중 현재 디렉터리에 `AGENTS.override*` 심볼릭 링크를 생성하지 않는다.
- 2026-03-10: repo별 추가 규칙은 해당 repo의 `AGENTS.md`에만 병합한다. 현재 작업 디렉터리에 `AGENTS.override` 또는 `AGENTS.override.md`를 새 파일로 만들지 않는다.

## Completion Loop Hard Gate
- When the user requests full implementation and repository rules require retry-until-complete, the assistant must not stop at blocker reporting if a concrete next fix is available.
- Required order on failure: `.project/feedback.md update -> todo.md merge with exact fix -> concrete code/process change -> full workflow restart`.
- Partial status summaries are forbidden until the requested end condition is met or a hard technical blocker with no safe local fix remains.
- `impl/check` stage hangs or long waits are not completion conditions; they must be treated as retry targets.
- Before sending any response that claims progress, verify whether the current task still has an unfinished required loop item. If yes, continue execution instead of reporting.
- For skill-driven workflows, completion is defined by the skill's final artifact/check step, not by intermediate artifact creation.

## Check-Code Hardcoding Coverage Rule
- `check-code` 실행 시 하드코딩 검출 범위를 boolean 고정 반환만으로 제한하지 않는다.
- 문자열/패턴 기반 성공·실패 단정 로직도 하드코딩으로 검사한다.
  - 예: `contains("Logout")`, `contains("success")`, `starts_with("ok")`, `ends_with("done")`
- 위 패턴이 입력/상태/외부결과 검증 없이 최종 판정에 직접 사용되면 `report.md`의 `# 발견된 문제`에 반드시 기록한다.

## 2026-03-09 - Target Repo Correction Rule
- 사용자가 대상 저장소 경로를 정정하면 즉시 정정된 경로만 작업 대상으로 사용한다.
- 이번 세션 기준 `orc` 관련 수정 대상은 `~/project/rust-orc`를 우선한다.

## 2026-03-10 - Provider Scope Rule
- 사용자가 provider 주입 경로를 직접 지적하면, 해당 작업은 provider 분기와 직결된 최소 변경부터 우선 처리한다.
- project type, registry, asset 정리는 provider 연결 수정에 직접 필요한 범위만 함께 바꾸고, 비직결 확장은 금지한다.

## 2026-03-10 - Wrapper Removal Rule
- 사용자가 코드상 불필요한 중간 wrapper 함수 제거를 지시하면, 먼저 zero-behavior wrapper 후보를 찾고 직접 호출로 인라인 가능한 함수부터 제거한다.
- 경로 계산/환경 복사처럼 의미 전달 외 동작이 없는 단일 반환 wrapper는 우선 제거 대상으로 본다.
- 제거 범위는 호출부 직결 파일로 제한하고, 의미 있는 검증/정규화 함수는 유지한다.

## 2026-03-10 - CLI Naming Rule
- 사용자가 CLI 명령 이름을 정정하면, 이후 사용자 노출 기본 명령명은 정정된 이름으로 통일한다.
- 이번 세션 기준 기본 CLI 명령명은 `rw`다.
- help, README, 예시 명령, 응답 본문에서 기본 표기는 `rw`를 우선 사용한다.

## 2026-03-10 - ORC Process Improvement Log Rule
- 구현 작업이 모두 끝난 뒤 현재 ORC 과정의 병목 또는 개선점이 식별되면 `.project/feedback.md`의 `#개선필요` 섹션에 항목을 추가한다.
- 최종 보고 전 `.project/feedback.md`에 `#개선필요` 헤더 존재 여부를 확인하고, 없으면 생성 후 내용을 append 한다.
- 내용은 대기 구간, owner 전달 누락, timeout/heartbeat 개선처럼 실제 관찰된 프로세스 개선점만 기록한다.
- 이미 이번 턴에서 반영한 수정 사항 설명은 쓰지 않는다. 아직 남아 있는 병목, 구조적 혼선, 다음에 손봐야 할 개선 후보만 기록한다.
- 사용자가 구현 완료 후 현재 `orc` 과정의 병목이나 개선점을 함께 남기라고 지시하면, 최종 검증이 끝난 뒤 `.project/feedback.md`에 `#개선필요` 섹션을 포함해 기록한다.
- 이 섹션에는 이번 작업 중 실제로 관찰한 병목, 불안정 지점, 또는 다음 재시도 없이 바로 적용 가능한 개선 후보만 적는다.
- 기능 구현 보고를 끝내기 전에 `.project/feedback.md` 반영 여부를 확인한다.
- 사용자가 `.project/feedback.md` 작성 기준을 정정하면, 관련 skill 문서와 `.project/feedback.md` 규칙을 같은 턴에 함께 갱신한다.
- `.project/feedback.md` 개선 섹션의 표준 헤더는 `#개선필요`로 고정한다.
- `.project/feedback.md`에 `#개선필요`를 추가할 때는 이미 반영이 끝난 변경 설명을 적지 않는다.
- 개선 섹션에는 앞으로 더 손봐야 할 지점, 반복 병목, 재시도 시 줄여야 할 낭비만 남긴다.

## 2026-03-10 - Template Generation Rule
- 사용자가 template 기반 생성을 지시하면 `assets/presets/<type>/templates/*` 원본 파일을 읽고, 해당 주석/placeholder를 해석해 값을 채우라는 prompt 경로로만 생성한다.
- md/yaml 생성 로직에서 문자열 배열/인라인 텍스트 조합으로 최종 파일 내용을 하드코딩하는 방식을 금지한다.
- type별 차이는 공통 service interface 뒤의 template loader/parser/prompt selector로만 표현한다.
- 재시도 시 기존 하드코딩 생성 코드는 제거 또는 template 기반 service 호출로 치환해야 한다.

## 2026-03-10 - Compatibility Cleanup Rule
- 사용자가 변경을 지시하면 호환 전용 분기나 유지 경로를 우선 고려하지 않는다.
- 기존 동작을 바꾸는 요청은 현재 기준으로 직접 수정하고, compatibility-only 코드는 같은 변경에서 제거한다.
- alias, fallback, 이전 타입 변환, 이중 경로 지원처럼 호환만을 위한 코드는 기본적으로 삭제 대상으로 본다.

## 2026-03-10 - Action Log Trace Rule
- 사용자가 오류 해결 중 동작 기록을 남기라고 지시하면, 구현과 검증 동안 저장소의 `log.md`에 단계별 실행 기록을 append 한다.
- 기록 최소 단위는 `시각`, `동작`, `대상`, `결과` 4항목이다.
- 같은 오류가 다시 발생하면 새 항목에 이전 항목 참조 또는 `반복` 표시를 남겨 재발 여부를 식별한다.
- 최종 보고 전에 이번 턴에서 남긴 `log.md` 항목으로 반복 실패 여부를 한 번 요약 점검한다.

## 2026-03-10 - Global Voice Input Rule
- 사용자가 text input 전반에 음성 입력을 붙이라고 지시하면, 저장소의 모든 단일행 input과 multiline textarea 사용처를 전수 조사해 공통 음성 입력 경로로 연결한다.
- 구현은 개별 화면마다 중복 로직을 복사하지 않고 공용 voice input component 또는 hook을 우선 사용한다.
- 검증은 최소 1개 이상의 실제 UI 경로에서 `버튼 클릭 -> 녹음 시작 -> transcript 반영 -> input value 갱신`을 확인하고, `rg`로 적용 범위를 점검한다.
- generic text locator로 버튼을 찾지 말고 각 입력 제어 옆 음성 버튼은 고유 `data-testid` 또는 안정적인 aria-label 규칙을 가진다.
