# Agents Override Rules

## Mandatory Completion Notification Rule
- 완료 후에는 `nf -m "<task-name> complete"`를 실행한다.
- 최종 응답 직전에는 현재 저장소 규칙과 override의 notification 항목을 다시 확인하고, 완료 보고보다 먼저 `nf -m` 실행 여부를 체크한다.

## Completion Notification Hard Gate
- 저장소 규칙이나 override에 completion notification 요구가 있으면, `nf -m` 성공 실행 전에는 완료 보고를 금지한다.
- 완료 직전 체크 순서는 고정한다: `notification rule re-read -> nf -m 실행 -> 종료 코드 확인 -> final 응답`.
- 이 순서를 건너뛴 completion 응답은 process violation으로 간주하고, 같은 유형 재발 방지 규칙을 즉시 override에 추가한다.

## Absolute Phrase Ban Rule
- 모든 채널에서 동의/확인 서두 없이 바로 결과부터 말하고, 금지 표현 `맞습니다`, `맞아요`, `인식했습니다`, `알겠습니다`, `네, 맞습니다`, `맞습니다.`, `네 맞습니다`, `그렇습니다`는 쓰지 않는다.
- 전송 직전 금지 표현을 다시 검사하고 하나라도 있으면 전체 문장을 다시 쓴다.

## Request Summary Output Rule
- For every user request, before starting work, output with label and description split across separate lines.
- Line 1: `[요약]`
- Line 2: `[${행동 설명:생성, 추가, 삭제, 변경}]`
- Line 3: `${대상}은 기능 한줄 요약`
- Line 4: `[결과]`
- Line 5: `일어날 결과`
- Keep this output concise and always place it immediately before implementation.

## File Path Display Rule (Output)
- 경로 표기는 `commentary`, `final`, `summary`에서 항상 `.../<parent>/<file>` 축약형만 사용한다.

## Screenshot Path Memory Rule
- `current.png`는 기본적으로 `/mnt/c/Users/tende/Pictures/Screenshots/current.png`로 바로 처리하고, 저장소 전체 검색은 사용자 후속 요청이 있을 때만 한다.
- 해당 파일이 없으면 그 정확한 경로가 비어 있다고만 말하고 대체 경로를 요청한다.
- 사용자가 `current.png`로 UI 문제를 지적한 턴에서는 test 산출 스크린샷만으로 완료 판정을 내리지 않는다. `current.png`에 보인 레이아웃 실패 조건을 직접 체크리스트로 적고, 수정 후 같은 조건이 사라졌는지 기준으로만 완료를 판단한다.
- mobile chat page는 첫 화면 진입 직후 메시지/typing/brief 중 최소 하나의 대화 본문 영역이 실제로 보여야 한다. 상단 title, topic, step chip, footer가 세로 공간을 다 먹어 chat body가 첫 화면에서 사라지면 실패다.
- mobile chat page에서는 전체 step chip 목록이 본문을 밀어내면 안 된다. mobile에서는 `현재/다음` 같은 압축 정보만 우선 노출하고, 전체 step 목록은 본문 가시성을 해치지 않는 보조 경로로만 보여야 한다.
- 사용자가 `current.png에 있는 것처럼 하라`고 지시하면, 같은 턴의 `current.png`는 문제 예시가 아니라 목표 배치 설계도로 취급한다.
- 이 경우 완료 기준은 `current.png`와의 레이아웃 유사성`이며, assistant가 스스로 더 낫다고 판단한 배치로 치환하면 안 된다.
- mobile chat에서 topic 선택 뒤 나오는 kickoff/확정 입력창은 작은 카드처럼 보이면 안 된다. mobile 폭 대부분을 쓰고, 카드의 시작 위치도 화면 상단 쪽에 붙여 `시작` 액션이 fold 위에 오도록 배치한다.
- yes/no 또는 confirm/cancel 성격의 모든 UI는 기본 순서를 `확인/시작/적용/삭제` 왼쪽, `취소/닫기` 오른쪽으로 통일한다. 기존 구현마다 순서가 섞여 있으면 같은 턴에 전수 점검 후 함께 바로잡는다.

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
  4) 실패 또는 새 문제 발견 시 `.project/feedback.md`의 `# 문제`에 항목을 추가/갱신하고 이를 바탕으로 `todo.md`를 재설계
  5) 해결된 항목은 `.project/feedback.md`의 `# 문제`에서 `# 해결`로 이동
  6) 재정비된 `todo.md` 문서를 바탕으로 처음부터 전체 재시작
- On failure, write/update `.project/feedback.md` and append retry reason to `todo.md` before restarting.
- Do not stop at intermediate logs only; continue until pass or max retry reached.

## Rule-First Enforcement (Highest Priority)
- On any new user behavioral instruction, update `/home/tree/ai/codex/AGENTS.override.md` first before running commands or editing source.
- If execution already started, stop running process first, write rule, then resume work.
- This rule has higher priority than implementation speed.

## Search Scope Lock Rule
- 검색 요청은 사용자가 지정한 파일/문구/경로 범위에서 가장 좁은 직접 검색만 먼저 실행하고, 첫 답변에는 존재 여부·정확한 hit 위치·검색 범위만 적는다.
- 정확한 문자열이 주어졌으면 exact match만 수행하고, 0건이면 0건으로 끝낸다. 유사 문구·의미 확장·원인 추적은 후속 요청이 있을 때만 한다.

## Temp Auto Loop Rule (Permanent)
- When user requests `rw cli` validation in `/home/tree/temp`, run iterative loop with this order:
  1) write/update `todo.md`
  2) remove and recreate `/home/tree/temp`
  3) run `rw auto` for requested app
  4) if failed, write `/home/tree/temp/.project/feedback.md` with `# 문제`, `# 해결`, `#개선필요`
  5) reflect feedback into next todo and restart from step 1
- Keep looping until verification passes or hard technical blocker is confirmed.

## Feedback->Todo Merge Rule (Highest Priority)
- After any failure, write/update `.project/feedback.md` first with `# 문제`, `# 해결`, and `#개선필요`.
- New issues found during planning, implementation, or checking must be appended under `# 문제`.
- Resolved issues must be moved from `# 문제` to `# 해결`; do not duplicate the same item in both sections.
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

## Persona Diversity Correction Sync Rule
- 사용자가 detail chat persona 의견이 서로 비슷하다고 정정하면, 같은 턴에 대상 저장소 `AGENTS.md`에 persona별 새 관점 강제 규칙을 먼저 반영한다.
- 구현은 persona prompt 또는 worker 조합에서 각 persona가 이미 나온 의견과 겹치지 않는 새 관점 1개 이상을 내도록 강제하고, 유사 표현 반복을 성공으로 취급하지 않는다.

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

## 2026-03-13 - Feedback Zero Hard Gate
- If `.project/feedback.md` has any remaining item under `# 문제`, completion response is blocked.
- `# 해결` is the resolved-history section. Move items there when they are fixed instead of leaving them in `# 문제`.
- `#개선필요` is for process improvement notes observed during the current work and does not by itself block completion.
- Mandatory order: `feedback update -> todo merge with exact fix -> concrete implementation change -> verification rerun -> feedback re-check`.
- Never treat remaining `# 문제` entries as optional backlog unless the user explicitly says to defer them.
- Skill completion, test pass, or artifact generation does not override this gate.
- If this gate is violated once in a turn, stop and fix rules/skill docs first before any further implementation.

## 2026-03-13 - Checklist Guard Mandatory
- Python one-off guard commands are not allowed as completion gates.
- Completion gate must use checklist items written in settings files (`AGENTS.override.md` and skill docs).
- Before final response, the assistant must re-check this checklist and mark every item as satisfied in execution logs:
  - feedback file has no remaining bullet under `# 문제`
  - resolved items moved from `# 문제` to `# 해결`
  - required verification commands finished successfully
- `#개선필요` entries are retrospective process notes and are allowed to remain.
- If any checklist item is not satisfied, completion response is forbidden and retry loop must continue immediately.

## 2026-03-13 - Response Preflight Auto-Validation Rule (Highest Priority)
- Every response in `commentary` and `final` must run preflight validation first. No exceptions.
- Preflight checklist (auto-enforced):
  - banned phrase scan passed (`맞습니다`, `네, 맞습니다`, `그렇습니다`, `확인했습니다` 포함 금지어 전부)
  - completion gate check passed (`feedback` unresolved 0, required verify commands success)
  - requested skill/order constraints are still satisfied
- If preflight is not executed, the response must be blocked and replaced with rule-fix execution.
- If preflight fails, do not explain-only; immediately apply fix and rerun validation before sending text.
- Treat skipped preflight as process violation and patch this override file first in the same turn before any further work.

## 2026-03-13 - User Report Replay Hard Gate (Highest Priority)
- If the user reports that a just-implemented feature still fails, stop all unrelated work immediately.
- Do not run unrelated unit tests first. Reproduce the reported flow first using an execution path that matches user behavior.
- For tray/UI/event-loop issues, completion is blocked unless this path is verified:
  - `트리거 입력(예: tray 클릭)` -> `이벤트 수신` -> `핸들러 실행` -> `화면/상태 변화`
- Unit tests alone cannot close this class of issue. Runtime verification evidence is mandatory.
- If runtime verification fails, final response is forbidden; continue fix loop until pass.

## 2026-03-13 - rust-response Output Routing Rule (Highest Priority)
- Before sending any user-facing response, route output through `~/project/rust-response` CLI with a `msg` argument.

## 2026-03-14 - Detail Chat Pause Badge Rule
- detail chat page는 footer의 `반영하기` 버튼을 두지 않고, 취소와 별도의 `중지/재개` 상태를 상단 badge로 제어한다.
- `취소`는 현재 채팅 session 종료를 뜻하고, `중지`는 session을 유지한 채 입력/투표/전송만 멈춘다.
- 중지 상태는 chat page 오른쪽 위 badge로 항상 보여야 하며, 같은 badge를 다시 눌러 즉시 재개한다.

## 2026-03-14 - Detail Blur Unlock Rule
- `project.md` detail pane와 `episode outline` pane의 blur overlay는 모든 핵심 항목이 비어 있는 경우에만 적용한다.
- 항목이 하나라도 채워진 상태에서는 전체 blur를 유지하지 않는다.
- 사용자가 `직접 입력`으로 진입하면 남은 항목이 있어도 pane blur 없이 개별 항목을 직접 선택해 수정할 수 있어야 한다.

## 2026-03-14 - Script Detail List Object Rule
- script detail의 `speak`, `규칙`, `제약 조건`, `등장인물` 편집 UI는 단순 multiline textarea로 두지 않는다.
- 위 4개 항목은 각 item을 개별 추가/삭제/수정하는 list-object 편집 방식이어야 한다.
- 저장 시에도 object-list UI에서 수정한 항목 집합이 그대로 `project.md`의 리스트 구조로 반영되어야 한다.

## 2026-03-14 - Detail Pane Order Rule
- detail page에서 `video pane`은 `episode drafts` 뒤, 즉 detail column의 맨 아래에 둔다.
- script/video 프로젝트에서 draft_item 기반 작업 흐름이 video pane보다 먼저 보여야 한다.

## 2026-03-15 - Template Scope Correction Rule
- 사용자가 `template들`, `template/web`, `templates too`처럼 복수 템플릿 범위를 지적하면 단일 템플릿으로 임의 축소하지 않는다.
- 최소 실행 단위는 `template/web` 하위 전체 목록 점검 + 대상별 현재 상태 기록이며, 구현 우선순위를 줄여야 할 때도 먼저 전체 범위를 읽고 근거를 남긴 뒤 좁힌다.

## 2026-03-14 - Detail Page Root Background Rule
- `data-testid="detail-page"` 루트 래퍼에는 별도 배경색 클래스를 두지 않는다.
- detail page 배경은 상위 page shell 또는 개별 pane이 담당하고, 루트는 투명 상태를 유지한다.

## 2026-03-14 - Draft Flow Label Rule
- draft flow pane 단계명은 현재 작업 의미를 그대로 보여야 한다.
- `project.md` 단계 label은 `draft detail`, `draft_item` 단계 label은 `scene add`로 표시한다.

## 2026-03-14 - Initial Override Read Gate
- `/home/tree/ai`에서 시작한 turn은 어떤 `commentary`나 `final` 문구보다 먼저 `~/ai/codex/AGENTS.override.md`를 내부적으로 읽는다.
- 사용자에게 보이는 첫 문장은 override 선독 이후에만 보낼 수 있다.
- 요청 요약 출력 규칙도 override 선독이 끝난 뒤에만 적용한다.

## 2026-03-15 - No Skill On User Demand
- 사용자가 스킬을 쓰지 말라고 지시한 turn에서는 명시적 skill invocation, skill 파일 절차 준수, skill 기반 우회 설명을 모두 중단한다.
- 해당 turn의 작업은 일반 탐색/구현/검증 절차로만 수행한다.

## 2026-03-14 - Detail State Badge Rule
- detail page의 현재 project state badge는 읽기 전용이 아니라 상태 변경 진입점이어야 한다.
- detail page state 선택 UI에는 `complete`를 포함한 전체 상태 후보가 보여야 한다.

## 2026-03-14 - Character Object Input Rule
- script detail의 `등장인물` 입력은 단일 문자열 리스트가 아니라 `등장인물 이름 + 설명` 2필드 object 입력이어야 한다.
- 등장인물 form/editor는 각 row를 두 칸으로 나눠 저장하고, markdown 반영 시에도 이름과 설명이 함께 유지되어야 한다.
- Direct raw response output is forbidden when `rust-response` CLI is available.
- `rust-response` must filter banned terms (including `맞습니다`, `네, 맞습니다`, `그렇습니다`, `확인했습니다`) before printing.
- If CLI output filtering fails, stop response and fix `rust-response` first.
- Do not print CLI execution logs to the user (for example: `Ran ...`, `Finished ...`, `Running ...` lines).
- Only the final filtered message text may be shown in user-facing output.

## 2026-03-13 - No Visible rust-response Invocation Rule
- If calling `rust-response` through a tool would expose command transcript or execution log to the user, that invocation path is forbidden.
- In that case, do not call `rust-response` in a user-visible tool channel.
- Use `rust-response` for implementation and verification work, but user-facing output must contain only the final filtered message text and no invocation trace.
- Showing `Ran ...`, `cargo run ...`, `Finished ...`, `Running ...`, or equivalent wrapper lines is a hard failure of this rule.

## 2026-03-13 - rust-response Console Leak Hard Block
- If a response would require a visible `cargo run`, `rust-response`, or equivalent CLI transcript in `commentary` or `final`, the assistant must not invoke that CLI through a visible tool path.
- In this case, respond with plain filtered text directly and preserve the same banned-term constraints without emitting any tool transcript.
- After any user report of console leakage, the assistant must stop using visible `rust-response` command execution for user-facing output for the rest of the session.

## 2026-03-13 - Response Tool Avoidance Checkpoint
- At every `tool call -> commentary` and `tool call -> final` transition, explicitly check whether the next user-facing text depends on a developer-tool invocation.
- If that invocation would expose transcript lines such as `Ran`, `cargo run`, `Finished`, or `Running`, block the response path and emit direct plain text instead.

## 2026-03-14 - Desktop Chat Width Rule
- 사용자가 desktop chat 가로폭을 지시하면 `chat-page` 주 컨테이너는 가능한 폭을 쓰되 최대 너비를 `1920px`로 제한한다.
- viewport가 `1920px`보다 넓어도 chat 본문 래퍼는 가운데 정렬된 `max-width: 1920px`를 유지해야 한다.
- The assistant must not treat output-routing preferences as permission to violate transcript-visibility rules.
- If the user has already reported the same response-path failure once in the session, no further user-facing response may depend on developer-tool execution output.

## 2026-03-14 - Long-Running ORC Phase Trace Rule
- 사용자가 진행 로그 부족이나 모호한 대기 보고를 지적하면, 이후 `orc auto`, `bootstrap_code_project`, `init_code_project`, `impl/check` 장기 대기 구간에서는 단순 `응답 대기중` 표현만으로 보고하지 않는다.
- 30초 이상 걸리는 단계는 보고 전에 현재 부모 stage, 실제 하위 command 또는 LLM 단계, 마지막 완료 stage, 최근 갱신된 파일/로그 중 최소 1개를 직접 확인해야 한다.
- composite stage(`bootstrap_code_project` 등)가 길어지면 assistant는 내부 세부 단계가 무엇인지 확인한 뒤에만 원인 보고를 할 수 있다.
- 현재 추적 정보만으로 내부 단계를 식별할 수 없으면, 같은 턴에 먼저 저장소 logging을 보강하고 같은 흐름을 재실행해 추적 가능 상태를 만든다.

## 2026-03-14 - Instruction Priority Hard Gate
- 사용자 지침, 저장소 `AGENTS.md`, 전역 `AGENTS.override.md`는 assistant의 임의 판단보다 항상 우선한다.
- assistant는 부분 성공, 산출물 존재, 임시 우회 성공만으로 종료 기준을 재정의할 수 없다.
- 종료 직전에는 반드시 `feedback.md #문제 0개`, 필수 검증 성공, 규칙 위반 0건을 순서대로 다시 확인한다.
- 위 조건 중 하나라도 남아 있으면 설명성 종료 응답은 금지되고, 즉시 수정-검증 루프를 계속한다.

## 2026-03-14 - Feedback Followup Agent Rule
- 작업 완료 시점마다 현재 작업 루트의 `.project/feedback.md`를 먼저 읽는다.
- `# 문제` 아래에 bullet이 1개라도 남아 있으면 완료 응답은 금지한다.
- 이 경우 assistant는 반드시 새 tmux pane을 열고, 남은 문제만 전담하는 followup agent를 생성해 그 pane에서 해결 루프를 이어가게 한다.
- followup agent는 남은 `# 문제`를 입력으로 받아 수정 -> 검증 -> `# 해결` 이동까지 수행해야 하며, `# 문제`가 0개가 될 때까지 종료할 수 없다.
- followup agent 프롬프트는 반드시 `check-code` skill과 `orc-cli-workflow` skill을 사용하도록 지시해야 한다.
- assistant는 future turns에서도 completion 직전에 이 followup agent 실행 여부를 기본 점검 항목으로 취급한다.
- `orc-cli-workflow` 관점에서 check 단계는 메인 pane이 직접 끝내는 단계가 아니라 followup agent를 호출하는 단계다.
- followup agent는 `orc-cli-workflow` 내부의 `check_code_draft`/feedback 정리/재검증 구간을 전담 수행하는 agent로 취급한다.
- followup agent의 핵심 책임은 코드 체크 수행, `.project/feedback.md` 갱신, 남은 문제 해결이다.
- followup agent에 전달하는 pane 명령은 반드시 `feedback을 읽고 새 plan.md를 만든 뒤 문제를 해결하라`는 뜻을 직접 포함해야 한다.
- followup agent가 동작하는 동안에는 followup용 `plan.md`를 작업 문서로 사용할 수 있다.

## 2026-03-13 - Rule Conflict Resolution Order
- When two response rules conflict, prefer the rule that prevents user-visible leakage over the rule that requests CLI-based routing.
- A hidden or non-user-visible validation path may still be used for internal checks, but any visible tool transcript is forbidden.
- If conflict resolution is not checked before response, the response is blocked until the check is performed.

## 2026-03-13 - Rule Generalization Mandatory
- If the same failure class appears more than once, do not keep adding narrow per-case rules for each wording, tool, or surface symptom.
- Replace or merge those patches into one generalized decision rule that covers the whole failure class, with a clear trigger, block condition, and priority.
- Before appending any new rule, check whether the issue is actually a missing enforcement step, priority mistake, or checkpoint omission. If so, fix that higher-level rule instead of stacking another symptom-level rule.
- Repeated failure followed by another symptom-specific rule is process failure.

## 2026-03-13 - Global Rule Efficacy Gate
- A global rule is only valid if it changes behavior across repeated cases without requiring case-by-case amendments.
- If a user reports that a global rule is becoming meaningless because of repeated micro-rules, stop adding new micro-rules and rewrite the enforcement as a broader invariant or checkpoint.
- The assistant must explain recurring failure in terms of the broken enforcement mechanism, not just the latest surface example.

## 2026-03-13 - Doc-Only Change Verification Gate
- If the current turn changes only documentation, rule files, or other non-executable text artifacts such as `*.md`, `AGENTS*`, `SKILL.md`, or prose-only records, do not run code build/test commands or artifact copy steps by default.
- Repository-wide build/test completion rules apply only when executable code, configuration affecting runtime behavior, or generated deliverables are changed.
- For doc-only turns, verification must be limited to text-structure checks, reference consistency, and requested file updates unless the user explicitly asks for code verification.
- Running `cargo test`, `cargo build`, or executable copy steps after a doc-only change is process failure.

## 2026-03-13 - User Instruction Hard Gate Lock (Highest Priority)
- Any explicit user instruction about priority, order, validation method, output method, or completion criteria is a hard gate, not guidance.
- Hard gates override default autonomy, implementation habits, unit-test-first habits, convenience shortcuts, and any assistant-chosen workflow.
- The assistant must not reinterpret, downgrade, defer, partially apply, or replace a hard gate with an equivalent-by-opinion alternative.
- If multiple rules exist, priority order is fixed:
  - current turn explicit user instruction
  - `AGENTS.override.md`
  - repo `AGENTS.md`
  - explicitly invoked skill
  - developer defaults / assistant heuristics
- If behavior conflicts with this order, stop current work immediately and realign before any further command, edit, or response.

## 2026-03-13 - Rust Build Artifact Exclusion Rule
- For any Rust project build, packaging, staging, commit, or push workflow, exclude generated artifact directories and files by default.
- Minimum excluded paths are `target/`, `build/`, and equivalent compiler/package output directories created by Cargo or build scripts unless the user explicitly requests their inclusion.
- If a Rust workflow would stage or push generated artifacts, stop and remove them from the candidate set before continuing.
- Build verification may use generated artifacts locally, but repository operations must treat them as non-source outputs by default.

## 2026-03-13 - Stage Transition Re-Read Gate
- Before each stage transition (`explore -> edit`, `edit -> test`, `test -> report`, `tool call -> final response`), re-read the currently active user hard gates from the current turn and verify they are still being followed.
- Stage transition is blocked until this re-check is complete.
- "Read once at start" is invalid. Re-validation is mandatory at every transition.

## 2026-03-13 - Reported Failure Priority Gate
- If the user reports a specific failure in a just-built feature, that reported path becomes the top-priority blocker.
- No unrelated tests, refactors, builds, or secondary fixes may run before the reported path is reproduced and fixed.
- For UI/tray/runtime failures, passing unit tests or static checks must not be used as evidence of completion ahead of direct runtime-path verification.

## 2026-03-13 - No Assistant Priority Substitution Rule
- The assistant must never replace user-specified validation or execution order with its own preferred order.
- Examples of forbidden substitutions:
  - running unit tests before reproducing a user-reported UI failure
  - treating build success as completion when the user required screenshot/runtime validation
  - answering with analysis when the user required rule patching first
- If substitution occurs, patch rules first in the same turn, then resume from the user-specified order.

## 2026-03-13 - Ambiguity Follows User Instruction Rule
- If any point is ambiguous, prefer the user's explicit instruction over assistant inference.
- When two reasonable interpretations exist, choose the one that stays closest to the user's wording and stated order.
- The assistant must not use ambiguity as permission to substitute its own workflow, validation order, or completion standard.
- If ambiguity remains after local context review, narrow work to the safest interpretation of the user's instruction instead of broadening scope.

## 2026-03-13 - Future Commitment Hard Gate Rule
- If the assistant is about to say any future-commitment phrase such as `앞으로`, `이제부터`, `다음부터`, it must first add a concrete hard-gate rule to `AGENTS.override.md` in the same turn.
- Response-only promises are forbidden.
- The hard-gate rule must describe the exact behavior to enforce and the block condition if it is skipped.
- If no rule is added first, the future-commitment wording must not be sent.

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

## 2026-03-11 - Script Episode Draft Canonical Rule
- script episode 설계도 canonical 파일은 `./episode/S{season}E{episode}_draft.yaml`이다.
- `drafts.yaml`은 개별 episode 시놉시스/설계도 역할을 가지며 `state`, `highlight`, `speak`, `step`, `rules`를 포함한다.
- `plan.yaml`은 작업중인 episode draft들의 상태/대상 목록을 관리하는 문서로 사용한다.
- detail page에서는 legacy `episode pane`을 제거하고, `drafts pane`이 episode 목록 선택/생성/수정/impl/check/view/delete 흐름의 단일 진입점이 된다.
- script 관련 template/schema 변경은 반드시 `templates 파일 갱신 -> 호출 함수 참조 갱신 -> UI/API 수정` 순서로 진행한다.
- 2026-03-11 추가: 위 episode blueprint 규칙은 `script` 전용이 아니라 episode blueprint를 사용하는 `ad`, `video`, `script` 전체에 동일 적용한다.
- episode blueprint 관련 형식 판단이 필요하면 코드의 현재 직렬화보다 `assets/presets/*/templates` 아래 최신 템플릿을 source of truth로 우선 사용한다.

## 2026-03-11 - Mobile Delete Button Position Rule
- mobile project page의 `삭제하기` 고정 버튼은 하단 브라우저 UI와 겹치지 않게 기본 하단 여백보다 더 위에 배치한다.
- 데스크톱 이상 레이아웃의 버튼 위치는 유지하고, small viewport에서만 위치를 올리는 최소 변경을 우선 적용한다.
- 내용은 대기 구간, owner 전달 누락, timeout/heartbeat 개선처럼 실제 관찰된 프로세스 개선점만 기록한다.
- 이미 이번 턴에서 반영한 수정 사항 설명은 쓰지 않는다. 아직 남아 있는 병목, 구조적 혼선, 다음에 손봐야 할 개선 후보만 기록한다.
- 사용자가 구현 완료 후 현재 `orc` 과정의 병목이나 개선점을 함께 남기라고 지시하면, 최종 검증이 끝난 뒤 `.project/feedback.md`에 `#개선필요` 섹션을 포함해 기록한다.
- 이 섹션에는 이번 작업 중 실제로 관찰한 병목, 불안정 지점, 또는 다음 재시도 없이 바로 적용 가능한 개선 후보만 적는다.
- 기능 구현 보고를 끝내기 전에 `.project/feedback.md` 반영 여부를 확인한다.
- 사용자가 `.project/feedback.md` 작성 기준을 정정하면, 관련 skill 문서와 `.project/feedback.md` 규칙을 같은 턴에 함께 갱신한다.
- `.project/feedback.md` 개선 섹션의 표준 헤더는 `#개선필요`로 고정한다.

## 2026-03-12 - Detail Chat Topic Reset Rule
- detail chat에서 current topic이 다음 topic으로 넘어가면, 직전 topic의 결정 내용은 즉시 project 파일 반영 경로에 저장한다.
- topic 전환 직후에는 기존 message history를 이어쓰지 않고 새 session/message list로 다시 시작한다.
- 구현 검증은 `topic 결정 -> project 파일 반영 -> session 메시지 초기화 -> 다음 topic 새 대화 시작` 순서의 실제 실행 경로로 확인한다.

## 2026-03-12 - Temp Project Cleanup Rule
- 테스트나 재현용으로 `/tmp` 또는 임시 registry에 project를 생성한 경우, 성공/실패와 무관하게 검증 종료 단계에서 해당 임시 project 디렉터리와 registry entry를 모두 삭제한다.
- Playwright/helper가 임시 project를 자동 로드한 경우에도 `cleanupTempProject` 또는 동등한 정리 경로가 실제로 실행됐는지 확인하고, 누락 시 같은 턴에 테스트/helper를 수정한다.
- 최종 응답 전에는 `남은 임시 project 0개`를 직접 확인한 뒤에만 완료로 간주한다.

## 2026-03-11 - Create Project Path Sync Rule
- create project form에서는 `Project name` 입력이 바뀔 때마다 현재 선택된 부모 경로 기준으로 `project path` 마지막 segment를 새 이름 기반 폴더명으로 다시 만든다.
- 경로 브라우저나 수동 입력으로 부모 경로를 바꾼 뒤에도 다음 이름 변경 시 마지막 segment는 새 이름으로 다시 붙는다.
- create 검증은 `name 입력 -> path last segment 자동 갱신 -> create click -> 실제 폴더/.project/project.md 생성` 순서로 확인한다.

## 2026-03-11 - YAML Change Order Rule
- 사용자가 YAML 구조 변경 순서를 지정하면, 같은 턴 구현 순서는 반드시 `templates 폴더 수정 -> 호출 함수의 속성 참조 갱신 -> 추가 수정(UI/동작/검증)` 순서를 따른다.
- `drafts.yaml`, `draft_first.yaml`, `draft_item.yaml`, `project.md`처럼 템플릿이 기준인 구조 변경은 먼저 템플릿 계약을 바꾼 뒤 런타임 참조와 UI를 맞춘다.
- 중간 단계에서 구버전 필드명을 유지하는 임시 우회 분기는 추가하지 않는다.
- `.project/feedback.md`에 `#개선필요`를 추가할 때는 이미 반영이 끝난 변경 설명을 적지 않는다.
- 개선 섹션에는 앞으로 더 손봐야 할 지점, 반복 병목, 재시도 시 줄여야 할 낭비만 남긴다.

## 2026-03-11 - Repo Rule Placement Rule
- 저장소 전용 동작 규칙은 global override가 아니라 해당 저장소의 `AGENTS.md`에만 기록한다.
- `.../codex/AGENTS.override.md`에는 여러 저장소에 공통으로 적용되는 규칙만 남긴다.

## 2026-03-12 - Explicit Persistence Instruction Sync Rule
- 사용자가 chat 확정값의 저장/재진입 이어받기, 수정 후 자동 재생성처럼 명시 동작을 정정하면, 구현 전에 이 요구를 대상 저장소 `AGENTS.md`에 같은 턴 즉시 반영한다.
- 확정값은 자동생성과 별도 경로의 수동 입력처럼 취급해야 하며, 재진입 시 이미 저장된 값은 완료 항목으로 간주하는 구현을 우선한다.
- item 단위 수정 후 후속 LLM 재생성 요구가 있으면 해당 item 범위만 다시 맞추고 전체 초기화는 금지한다.

## 2026-03-12 - Episode Draft Split Action Rule
- episode drafts pane에서 `project.md -> blueprint 메타(item 제외)` 채우기, `draft_item` 생성/수정, `draft_item 기반 write`는 하나의 버튼/모달로 합치지 않는다.
- 각 단계는 별도 버튼과 별도 modal state를 가져야 하며, modal 안에서 해당 단계 값 수정이 가능해야 한다.
- `draft_item` 생성 단계가 `write`를 자동 시작하면 안 되고, `write` 단계는 현재 저장된 `draft_item`만 기준으로 실행해야 한다.

## 2026-03-12 - Term Definition Sync Rule
- 사용자가 서사 용어 의미를 직접 정정하면, 구현 전에 그 정의를 대상 저장소 `AGENTS.md`에 같은 턴 즉시 반영한다.
- prompt/rule/template 자산이 그 용어를 쓰고 있으면 최소 변경으로 같은 정의를 함께 갱신한다.

## 2026-03-10 - Template Generation Rule
- 사용자가 template 기반 생성을 지시하면 `assets/presets/<type>/templates/*` 원본 파일을 읽고, 해당 주석/placeholder를 해석해 값을 채우라는 prompt 경로로만 생성한다.
- md/yaml 생성 로직에서 문자열 배열/인라인 텍스트 조합으로 최종 파일 내용을 하드코딩하는 방식을 금지한다.
- type별 차이는 공통 service interface 뒤의 template loader/parser/prompt selector로만 표현한다.
- 재시도 시 기존 하드코딩 생성 코드는 제거 또는 template 기반 service 호출로 치환해야 한다.

## 2026-03-10 - Legacy Reference Path Removal Rule
- 사용자가 특정 legacy reference 경로 제거를 지시하면 해당 경로 계열 흔적을 전부 제거 대상으로 본다.
- 제거 범위에는 repo 코드, repo 문서, repo 로그, skill reference 자산, help/응답용 문구가 포함된다.
- 새로운 구현이나 규칙에서 제거된 legacy reference 경로를 다시 만들거나 참조하지 않는다.
- 기능 설계 산출물은 현재 기준 문서 경로만 사용하고, 제거된 legacy reference 경로로의 호환 유지 코드는 두지 않는다.

## 2026-03-10 - Home Log No-Touch On Replace Rule
- 사용자가 치환, 일괄 치환, 문자열 교체를 지시할 때 `~/.agents/log.md`는 수정 대상에서 항상 제외한다.
- 치환 범위 산정 중 `~/.agents/log.md`가 검색 결과에 보여도 건드리지 않는다.
- 사용자가 별도로 `~/.agents/log.md` 자체 수정까지 명시한 경우에만 예외로 본다.

## 2026-03-10 - Script Type Canonical Rule
- 사용자가 project type 이름을 정정하면 이후 canonical project type은 정정된 이름만 사용한다.
- 이번 세션 기준 `write` project type의 canonical 이름은 `script`다.
- preset 경로, project_type 값, UI/API 라벨, 테스트 fixture, prompt/template 참조는 `script`로 통일하고 `write` 전용 canonical 경로는 제거한다.
- 호환 alias나 이중 분기를 남기지 말고 현재 기준 이름으로 직접 치환한다.

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

## 2026-03-11 - Temp Test Project Cleanup Rule
- 사용자가 테스트 중 생성한 임시 project 정리를 지시하면, 이후 해당 저장소의 검증 경로는 성공 종료 시 임시 project 등록과 임시 디렉터리를 함께 삭제해야 한다.
- web e2e/통합 테스트에서 `project-load`로 등록한 임시 project는 `project-delete` API 또는 동등한 정리 경로로 제거한다.
- 임시 디렉터리는 프로젝트 삭제 뒤 `fs.rmSync(..., { recursive: true, force: true })` 등으로 정리한다.
- 검증 보고에는 임시 project cleanup 실행 여부를 함께 적는다.
- 테스트 파일 내부 `finally` 정리만으로 종료하지 않는다. 최종 검증 직후 추가로 registry(`configs/project.yaml` 등)와 임시 루트(`/tmp` 등)를 sweep해 `pw-`, `playwright-`, 테스트 prefix project 잔존 여부를 직접 확인하고 남아 있으면 즉시 삭제한다.
- cleanup 누락 지적이 한 번이라도 나온 저장소에서는 이후 모든 검증 종료 단계에 `잔존 항목 검색 -> 강제 삭제 -> 재검색 0건 확인`을 반드시 포함한다.

## 2026-03-11 - Script Output Format Rule
- 사용자가 reference script를 바탕으로 script 출력 형식 규칙 문서를 만들라고 지시하면 `assets/presets/script/rules` 아래에 형식 지시사항과 짧은 예시가 함께 있는 파일을 추가한다.
- script draft 구현 prompt는 장면 문서를 쓰기 전에 그 규칙 파일을 반드시 읽고 prompt 입력에 함께 포함한다.
- 형식 규칙 파일은 장면 헤더, 지문, 대사, 인서트/전환 표기 같은 문서 형식을 담당하고, 코드에는 예시 문장을 하드코딩하지 않는다.

## 2026-03-10 - Script Episode Draft Structure Rule
- 사용자가 script episode 구조를 정정하면 script 타입 draft 생성은 `first`, `middle`, `final` episode 역할을 구분하는 canonical 구조만 사용한다.
- `plan.yaml`에는 최소한 `planned/work/complete` 상태와 이야기의 큰 위기, 목적, 주인공 결핍 속성이 포함되어야 한다.
- `drafts.yaml` item에는 episode 역할, 등장 캐릭터, 등장 배경, 갈등, ending 계열 메타가 공통으로 들어가야 한다.
- 1화용 script draft는 일반 episode draft와 분리된 `draft_first.yaml` 템플릿을 사용하고, `강한 오프닝 이미지`, `세계관과 주인공 결핍 소개`, `핵심 인물 배치`, `발화 사건`, `주인공 연결`, `룰과 갈등 확장`, `엔딩 훅`을 강제 필드로 포함한다.
- script 타입의 draft/draft_item/plan 생성·정규화 함수는 위 구조를 주입하고, 중간 episode와 마지막 episode는 공통 episode 필드 위에 각 역할별 필수 항목만 추가한다.
- script episode 설계도 파일의 canonical 경로는 `.project/drafts.yaml`이 아니라 `./episode/S{season}E{episode}_draft.yaml`이다.
- script episode 설계도에서 `trigger_event`, `conflict_expansion`, `ending_hook`, `conflict`는 배열이 아니라 단일 문자열 필드로 유지한다.
- 검증은 최소 1개 이상의 실제 UI 경로에서 `버튼 클릭 -> 녹음 시작 -> transcript 반영 -> input value 갱신`을 확인하고, `rg`로 적용 범위를 점검한다.
- generic text locator로 버튼을 찾지 말고 각 입력 제어 옆 음성 버튼은 고유 `data-testid` 또는 안정적인 aria-label 규칙을 가진다.
- 2026-03-11: project page 초기 로딩 UI 변경 작업은 skeleton 표시만 추가하지 말고 `초기 렌더 -> /api/projects fetch -> 상태 갱신 -> 실제 프로젝트 카드 렌더` 경로까지 함께 검증한다.
- 2026-03-12: detail chat은 진입 시 `project.md`, `drafts.yaml`, 관련 blueprint/task 파일의 현재 값을 기준으로 결정 완료 항목을 다시 계산하고, 파일 기준 결정 상태가 하나라도 바뀌었으면 이전 대화 이력을 재사용하지 않고 새 session으로 시작한다.
- 2026-03-12: script detail chat의 skip 판정은 `project.md` 원문 섹션(`후킹/주인공/등장인물/목표/복선/전환/결말/scene/speak/규칙/제약 조건/에피소드`)과 blueprint/template 파일의 실제 채워진 값을 직접 기준으로 해야 하며, UI detail pane에 값이 보이는 항목은 chat badge에서도 decided로 건너뛰어야 한다.
- 2026-03-12: 사용자가 별도 해제 지시를 하지 않는 한, 작업 완료 시마다 completion notification을 반드시 `nf -m "<task> complete"` 형식으로 실행한다.
## 2026-03-12 - UI Completion Verdict Rule
- UI 요청은 내부 함수, API action, modal 컴포넌트, 테스트 이름이 존재해도 완료로 판정하지 않는다.
- 완료 판정 최소 기준은 화면 렌더 결과 기준이다: 메인 버튼/액션의 순서, 노출 여부, disabled 조건, 클릭 후 modal 종류, 저장 후 파일/상태 반영이 사용자 요구와 일치해야 한다.
- `구현했다`, `이미 있다`, `완료됐다`라고 답하기 전에는 실제 렌더 화면 또는 UI 자동화에서 위 항목을 직접 확인해야 한다.
- UI 관련 검증은 기존 테스트 통과만으로 충분하지 않다. 사용자 요구를 직접 검증하는 assertion이 없으면 테스트를 먼저 추가/수정한 뒤 구현 완료를 선언한다.
- 스크린샷이 주어진 경우 코드 구조보다 스크린샷의 실제 배치와 노출 상태를 우선 기준으로 삼는다.
- 스크린샷이 주어진 UI 작업은 자동화 테스트 외에 실제 스크린샷 캡처를 다시 수행하고 `이미지 확인`이 끝나기 전에는 완료 판정을 금지한다.
- 스크린샷 기반 UI 검증에서는 `스크린샷 재현 경로`, `캡처 파일`, `post-change screenshot`, `이미지 확인`, `배치 차이`를 작업 중 문서에 남겨야 한다.

## 2026-03-12 - Mobile Detail Chat Fullscreen Rule
- 사용자가 mobile에서 채팅창 전체화면을 요구한 경우, detail chat modal은 viewport 상단 일부에 걸친 sheet 형태가 아니라 화면 전체를 덮는 full-screen overlay로 떠야 한다.
- mobile detail chat은 브라우저 safe-area를 포함한 전체 높이를 사용해야 하며, 배경 page가 상단에 보이거나 modal 바깥 여백이 남으면 안 된다.

## 2026-03-12 - Detail Chat Separate Page Rule
- 사용자가 detail chat을 별도 page로 분리하라고 지시한 저장소에서는 chat을 modal로 이어붙이지 않고, detail 옆 전용 chat 버튼을 통해 별도 page/pane으로 이동시키는 구조를 우선 적용한다.
- chat page는 detail page 일부 위에 뜨는 overlay가 아니라, 한 화면 전체를 chat 전용 레이아웃이 차지해야 한다.
- 현재 선택된 project 상태를 기준으로 chat page 내용을 결정해야 하며, 선택 project가 바뀌면 같은 chat page에서도 새 project 기준 세션/화면으로 즉시 전환되어야 한다.
- chat page가 분리된 뒤에는 detail page 안에 별도 chat 진입 버튼이나 chooser 내부 chat 액션을 남기지 않는다. detail page는 편집/auto만 담당하고 chat 관련 진입은 chat page에서만 처리한다.
- mobile `detail` page도 `project` page와 동일하게 page 본문 전체를 정상 렌더해야 하며, section이 잘리거나 desktop 전용 레이아웃처럼 깨지면 안 된다.
- `chat` page에서는 뒤쪽 `project` page가 비치면 안 된다. 전용 page shell이 현재 화면 한 장을 완전히 대체해야 한다.
- 사용자가 project를 고르지 않은 상태로 `detail` 또는 `chat` page에 들어가면, 목록의 첫 project를 현재 선택 project로 간주하고 같은 기준으로 detail/chat 내용을 연다.
- `chat` page 상단 pane은 detail page처럼 텍스트 중심으로 보여야 하며, mobile에서 큰 카드/chrome를 중첩해 채팅 본문 시작 위치를 밀어내면 안 된다.
- mobile `chat` page는 제목/설명 다음 바로 topic 또는 메시지 본문이 보이도록 vertical space를 아껴야 하며, page 안에 다시 page 카드처럼 보이는 이중 래퍼를 두지 않는다.
- detail page의 각 pane에서 아직 채워지지 않은 속성이 남아 있으면 pane 내부 본문은 blur overlay로 가리고, 그 위에 `chat으로 가기`, `auto`, `직접 입력` 세 액션만 보여 남은 항목을 채우게 해야 한다.
- 위 overlay의 `chat으로 가기`는 현재 pane target에 맞는 chat page topic으로 이동해야 하고, `auto`/`직접 입력`도 같은 pane target을 기준으로 동작해야 한다.
- mobile detail/page overlay와 pane 내부 액션줄은 viewport 폭을 넘기면 안 된다. 좌우 잘림이나 horizontal scroll이 생기면 레이아웃 불합격으로 본다.
- mobile chat/page text도 viewport 폭을 넘기면 안 된다. 제목, 설명, badge, 메시지 bubble, 버튼 라벨은 truncate가 아니라 줄바꿈 또는 축약 규칙으로 화면 안에 남아야 한다.
- 사용자가 `current.png`를 기준으로 mobile kickoff 입력창 배치를 다시 맞추라고 하면, 시작창 내부 제목/설명보다 입력 영역 크기와 첫 액션 가시성을 우선 기준으로 삼는다.
- mobile chat의 음성 녹음 버튼은 textarea/input의 옆 칸을 차지하는 inline 배치로 두지 않는다. 입력 wrapper 위에 떠 있는 floating anchor로 분리해 pane 크기 조정과 독립적으로 같은 자리를 유지해야 한다.
- 사용자가 mobile kickoff voice 버튼 위치를 다시 지시하면, 버튼은 input wrapper의 오른쪽 아래 absolute anchor로 둔다.
- mobile kickoff의 `시작/취소`는 카드 중간이 아니라 화면 기준 맨 아래 action bar에 둔다.
- mobile kickoff에는 textarea wrapper까지 합쳐 이중 카드/이중 배경을 두지 않는다. 노란 바탕 보조 카드 없이 단일 흰색 창 하나만 보여야 한다.
