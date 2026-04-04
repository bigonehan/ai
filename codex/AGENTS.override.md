# Agents Override Rules

### New Session `/plan` ORC-First Rule (Global)
- 새 Codex 세션에서 사용자가 `/plan` 또는 `plan 모드` 지시를 내리면 먼저 `/plan` 계획 응답을 완료한다.
- 계획 응답 완료 직후 ORC를 실행해 `job.md`를 생성/고정한다.
- 강제 순서: `/plan 완료` -> `codexo "/plan <task-name>: <plan prompt>"` -> ORC 검증 체인 실행 -> 결과 보고.
- `/plan` 완료 후 ORC 실행을 생략하는 응답을 금지한다.
- 사용자가 `$plan` 지시와 함께 “중단 없이 진행” 또는 유사한 “즉시 이어서 계속” 요청을 할 때는 후보 제시 후 중단하지 않고, 구현-검증-기록 루프를 동일 세션에서 계속 이어간다.
- 후보 목록만 남기고 종료하는 응답은 금지하고, 최소 1개 다음 액션을 즉시 실행한다.

# basic case 
## common Case
- 완료 후에는 `nf -m "<task-name> complete"`를 실행한다.
- 최종 응답 직전에는 완료 보고보다 먼저 `nf -m` 실행 여부를 체크하고, 안되있으면 `nf-m`을 실행한다.
- 완료 직전 체크 순서는 고정한다: `nf -m 실행 -> 종료 코드 확인 -> final 응답`.
- fail-closed: `nf -m` 종료코드가 0이 아니거나 실행 기록이 없으면 `final` 응답을 전송하지 않는다.
- 예외 없음: 실패 보고/단문 응답/재시도 안내를 포함한 모든 `final` 응답은 동일하게 `nf -m` 성공 확인을 선행한다.
- 모든 채널에서 동의/확인 서두 없이 바로 결과부터 말하고, 금지 표현 `맞습니다`, `맞아요`, `인식했습니다`, `알겠습니다`, `네, 맞습니다`, `맞습니다.`, `네 맞습니다`, `그렇습니다`는 쓰지 않는다.
- 전송 직전 금지 표현을 다시 검사하고 하나라도 있으면 전체 문장을 다시 쓴다.

### Recursive Improvement Loop Rule (Global)
- 반복 위반 시 `codexo "/plan <원문 명령>"`으로 재계획 + preflight 재검증을 수행한다.
- 실행 체인은 `orc create_job_md -> orc add_orc_drafts -> orc impl_orc_code -> orc check_orc_code`로 고정한다.
- 성공 기준은 `job.md` 존재, `#task` 고정, ORC pipeline preflight 통과다.

### Request Summary Output Rule
- For every user request, before starting work, output with label and description split across separate lines.
- Line 1: `[요약]`
- Line 2: `[${행동 설명:생성, 추가, 삭제, 변경}]`
- Line 3: `${대상}은 기능 한줄 요약`
- Line 4: `[결과]`
- Line 5: `일어날 결과`
- Keep this output concise and always place it immediately before implementation.

### File Path Display Rule (Output)
- 경로 표기는 `commentary`, `final`, `summary`에서 항상 `.../<parent>/<file>` 축약형만 사용한다.

## 의도 파악 

### User Instruction Priority Gate
- 매 턴 시작 시 현재 사용자 원문을 그 턴의 최상위 제약 조건으로 먼저 고정한다.
- 기존 규칙을 적용하기 전에, 해당 규칙이 사용자 원문의 범위/검증/예외 조건을 축소하거나 변형하는지 먼저 검사한다.
- 기존 규칙이 사용자 원문보다 더 좁은 범위, 더 약한 검증, 임의 예외 추가를 강제하면 그 규칙은 해당 턴에서 무효다.
- 내부 우선순위는 항상 `현재 사용자 원문 > 삭제/수정 같은 작업 전용 강제 규칙 > 일반 검색/출력/편의 규칙` 순서다.
- 삭제/제거 작업에서는 `이 규칙이 사용자 원문을 덮어쓰는가`, `이 규칙 때문에 범위가 좁아지는가`, `이 규칙 때문에 검증 강도가 약해지는가`를 먼저 점검하고, 하나라도 해당하면 사용자 원문 기준으로 재결정한다.

### 스크린샷 언급 
- `current.png`는 기본적으로 `/mnt/c/Users/tende/Pictures/Screenshots/current.png`로 바로 처리하고, 저장소 전체 검색은 사용자 후속 요청이 있을 때만 한다.
- 사용자가 `current.png`로 UI 문제를 지적한 턴에서는 test 산출 스크린샷만으로 완료 판정을 내리지 않는다. `current.png`에 보인 레이아웃 실패 조건을 직접 체크리스트로 적고, 수정 후 같은 조건이 사라졌는지 기준으로만 완료를 판단한다.
- 사용자가 `current.png에 있는 것처럼 하라`고 지시하면, 같은 턴의 `current.png`는 문제 예시가 아니라 목표 배치 설계도로 취급한다. 이 경우 완료 기준은 `current.png`와의 레이아웃 유사성`이며, assistant가 스스로 더 낫다고 판단한 배치로 치환하면 안 된다.
- 사용자가 `current.png`로 UI를 지적하면, 이미지에 보인 에셋/라벨/버튼/그룹/툴바의 상대 위치를 우선 보존한다. 상하/좌우/포함/동일 컨테이너/헤더-리스트-item 계층 관계를 임의로 바꾸면 사용자 원문 위반으로 처리한다.
- 사용자가 `A 아래에 B가 보이게`, `A 안에 B를 넣어`, `A 밑에 B를 추가`처럼 말하면 기본 해석은 `A 유지 + B 추가`다. A를 숨기거나 치환하거나 이름만 남기지 않는 해석은 금지한다.
- 사용자가 제거를 명시하지 않았는데 기존 라벨/헤더/그룹명을 지우면 사용자 원문 위반으로 처리한다.
### 검색 요청
- 검색 요청은 사용자가 지정한 파일/문구/경로 범위에서 가장 좁은 직접 검색만 먼저 실행하고, 첫 답변에는 존재 여부·정확한 hit 위치·검색 범위만 적는다.
- 정확한 문자열이 주어졌으면 exact match만 수행하고, 0건이면 0건으로 끝낸다. 유사 문구·의미 확장·원인 추적은 후속 요청이 있을 때만 한다.
### 호출, 실행 
- If the user says phrases like `호출해서 실행`, `실행해봐`, `돌려봐`, interpret the request as run existing CLI command first, not implementation.
- In this case, do not edit code/docs unless the user explicitly asks to implement/change.
- Output must prioritize executed command and result summary.
- If command execution hangs, report hang reason first and ask whether to stop/retry with timeout.

### Full-Scope Modify Rule
- 사용자가 `수정`, `바꿔`, `고쳐`를 지시하면 단일 파일 패치로 끝내지 않고 관련 규칙/검증/연동 경로까지 같은 턴에 함께 반영한다.
- 사용자가 `수정`만 단독으로 말해도 동일 규칙을 자동 적용하며, 추가 확인 질문 없이 전수 반영을 시작한다.
- 최소 포함 범위: 구현 코드, 연결된 설정/규칙 문서, 검증 스크립트/테스트.
- 관련 항목 중 하나라도 이전 포맷/구 규칙을 강제하면 완료 보고를 금지한다.
### 에러 메시지 표시 
- When the user input consists only of error messages/log output, automatically execute the full cycle without extra confirmation:
  1) identify root cause
  2) implement fix
  3) run verification
  4) report final result
- Do not stop at diagnosis-only responses for error-only inputs unless execution is technically blocked.
### 사용자 지적시
- 사용자가 문제점/오해를 지적하면, 해당 지적 사항은 추가 확인 질문 없이 즉시 실행 지시로 해석한다.
- "이건 지적이다" 유형 입력은 분석-only 응답을 금지하고 즉시 수정/실행/검증 순서로 진행한다.
- 동일 턴에서 규칙 반영과 구현을 모두 수행한다.
## 작업 완료시 
### Promise/Critique/Fault Auto Recovery Rule (Highest Priority)
- 트리거 문구: assistant 응답에 `다음부터`, `앞으로는`, `맞습니다`, `재발 방지`, `다시는`, `하겠다/하겠습니다`, `제 실수입니다`, `잘못했습니다`가 포함되면 즉시 실패 반응으로 판정한다.
- 감지 즉시 최소 1개 이상의 구체적 프로세스 개선안을 식별하고 같은 턴에 `AGENTS.md`에 기록한다. 규칙 갱신 없는 약속성 응답은 위반으로 처리한다.
- 즉시 `/plan` 모드로 재진입해 현재 문제를 재정의하고, 루트 `job.md`의 `#task/#problems/#check`에 보완책을 먼저 기록한다.
- `/plan` 요청에는 `원인` + `재발 조건` + `예방 검증 예제(정상 1개, 차단 1개 이상)` 기록 지시를 반드시 포함한다.
- 보완책 기록 뒤 tmux 새 pane을 생성하고, 문제가 처음 발생한 사용자 원문 명령을 `codexo "/plan <원문 명령>"`으로 재실행한다.
- 재실행 검증 체인은 `create_job_md -> add_orc_drafts -> impl_orc_code -> check_orc_code`로 고정한다.
- 실패 시 `job.md`에 실패 원인/보완책을 추가하고 같은 루프를 반복한다.
- 종료 조건은 원문 명령 재실행이 동일 검증 체인을 통과하는 경우로만 제한한다.

### Action Log Trace Rule
- 사용자가 오류 해결 중 동작 기록을 남기라고 지시하면, 구현과 검증 동안 저장소의 `log.md`에 단계별 실행 기록을 append 한다.
- 기록 최소 단위는 `시각`, `동작`, `대상`, `결과` 4항목이다.
- 같은 오류가 다시 발생하면 새 항목에 이전 항목 참조 또는 `반복` 표시를 남겨 재발 여부를 식별한다.
- 최종 보고 전에 이번 턴에서 남긴 `log.md` 항목으로 반복 실패 여부를 한 번 요약 점검한다.
## 2026-03-21 - Global Settings First
- 작업 시작 전 전역설정 파일을 먼저 읽고 준수한 다음 저장소 작업을 진행한다.
- fail-closed: 전역설정 파일 읽기 확인이 실패하면 구현/실행/보고를 즉시 중단한다.
- 강제 게이트: 전역설정 읽기 확인이 선행되지 않은 작업 실행은 규칙 위반으로 처리한다.

## 2026-03-21 - Full Scope Trigger Rule
- 사용자가 `전부`, `모두`, `전체`를 명시하면 부분 수정/부분 보고를 금지하고 관련 범위를 전수 반영한다.
- 전수 반영 범위에는 구현 코드, 호출 경로, 규칙/문서, 검증 경로가 포함된다.
- 미완료 항목이 1개라도 남아 있으면 완료 보고를 금지한다.

## 2026-03-21 - Skill Global Legacy Cleanup Rule
- 사용자가 Skill 전역설정 정리를 지시하면 `/home/tree/ai/skills`, `/home/tree/.codex/skills`의 설정 문서를 전수 점검한다.
- legacy/구식 경로/구식 호환 문구가 발견되면 남김없이 현재 표준 규칙으로 치환한다.
- 전수 점검 후에는 잔존 문자열 검색 결과를 함께 확인하고 보고한다.

## 2026-03-22 - Cargo Install Always Rule
- 사용자가 `설정에 매번 실행`을 지시한 저장소 작업에서는 완료 직전에 `cargo install --path /home/tree/project/mono_Manager --bin orc --force`를 매번 실행한다.
- 완료 전 체크 순서는 `cargo install -> nf -m -> final 응답`으로 고정한다.

## 2026-03-27 - Full Delete Memory Rule
- 사용자가 특정 문자열/파일/기능을 `지워`, `삭제`, `전부`, `다`, `모두`, `하나도 안 나오게`라고 지시하면 부분 삭제를 금지한다.
- 이 경우 검색 요청으로 해석하지 않고 `Delete Verification Mode`로 즉시 전환한다.
- `Delete Verification Mode`에서는 `검색 요청은 가장 좁은 범위부터`, `tracked files만 검사`, `단일 파일 최소 수정` 같은 일반 규칙보다 삭제 전수 검증 규칙이 항상 우선한다.
- 이 경우 구현 전에 해당 삭제 기준을 규칙으로 간주하고, 관련 코드/문서/프롬프트/테스트/산출물까지 전수 삭제 범위에 포함한다.
- 첫 검증은 저장소 루트 기준 `rg` 전수 검색으로 시작하며, 완료 기준도 사용자가 지정한 primary pattern과 alias pattern을 `rg`로 검색했을 때 모두 0건인 상태뿐이다.
- 사용자가 보존을 명시한 경로만 `rg` 예외로 제외할 수 있고, 그 외 임의 범위 축소는 금지한다.
- 1건이라도 남아 있으면 완료 보고를 금지하고 `검색 -> 수정 -> 재검색` 루프를 계속 반복한다.

## 2026-04-02 - ORC Worker API First
- tmux worker pane orchestration은 `orc worker-create`, `orc worker-send`, `orc worker-wait`, `orc worker-close`만 표준으로 사용한다.
- `tmux split-window ...` 직접 호출, `.project` 아래 worker 실행 스크립트 생성, `orc send-tmux` 기반 worker 조합은 worker 표준 경로에서 금지한다.
- skill/설정/문서/코드 예시는 모두 위 worker API를 사용하도록 같은 턴에 동기화한다.

## 2026-04-02 - ORC Manager Trace API First
- `orc_manager` 흐름에서 trace 기록/검증은 shell script 호출이 아니라 `orc manager-trace`, `orc check-manager-trace` 명령을 표준으로 사용한다.
- 위 shell script 경로는 남기지 않는다. 호출, 문서 언급, 파일 자체를 같은 턴에 제거하고 ORC 내부 명령만 남긴다.

## 2026-04-04 - ORC Manager Blocking Loop Guard
- `orc_manager` 작업에서 사용자가 `성공할때까지`, `될때까지`, `끝까지`, `중간 승인 없이 계속`을 명시한 턴에는 explanation-only 종료를 금지한다.
- 이 경우 `job.md#problems` 또는 `job.md##problems`에 blocking bullet이 남아 있거나 `job.md##verify`에 unchecked 항목이 남아 있으면 final 응답을 금지한다.
- 위 종료 금지는 `scripts/check_orc_manager_completion_guard.sh`를 canonical gate로 사용한다.
- `scripts/response_send_guard.sh`는 requirement_trace와 trace final 뿐 아니라 ORC manager completion guard도 통과해야 PASS다.
