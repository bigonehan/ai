# Agents Override Rules

# basic case 
## common Case
- 완료 후에는 `nf -m "<task-name> complete"`를 실행한다.
- 최종 응답 직전에는 완료 보고보다 먼저 `nf -m` 실행 여부를 체크하고, 안되있으면 `nf-m`을 실행한다.
- 완료 직전 체크 순서는 고정한다: `nf -m 실행 -> 종료 코드 확인 -> final 응답`.
- fail-closed: `nf -m` 종료코드가 0이 아니거나 실행 기록이 없으면 `final` 응답을 전송하지 않는다.
- 예외 없음: 실패 보고/단문 응답/재시도 안내를 포함한 모든 `final` 응답은 동일하게 `nf -m` 성공 확인을 선행한다.
- 모든 채널에서 동의/확인 서두 없이 바로 결과부터 말하고, 금지 표현 `맞습니다`, `맞아요`, `인식했습니다`, `알겠습니다`, `네, 맞습니다`, `맞습니다.`, `네 맞습니다`, `그렇습니다`는 쓰지 않는다.
- 전송 직전 금지 표현을 다시 검사하고 하나라도 있으면 전체 문장을 다시 쓴다.

### Plan->ORC->job.md Gate Rule (Global)
- 사용자가 Plan 모드로 계획을 요청한 턴에서는 구현/파일수정 없이 계획만 작성한다.
- Plan 모드가 끝난 첫 실행 턴에서는 반드시 `orc-cli-workflow`를 먼저 적용한다.
- 실행 시작 0단계로 `job.md`를 생성/갱신하고 `#task` 상태(`planned/work/check/completed/fail`)를 먼저 고정한다.
- `job.md#task` 상태 고정 전에는 구현 시작/완료 보고를 금지한다.
- ORC 작업 완료 순서를 고정한다: `/home/tree/ai/codex/script/verify_job_evidence.sh` 통과 -> 기능 검증 통과 -> `nf -m "<task-name> complete"` -> 최종 보고.

### ORC Gate Hard-Block Rule (Global)
- 구현 전 강제 순서: `전역설정 읽기 -> orc init_orc_job -> job.md#task 고정 -> /home/tree/ai/codex/script/orc_gate_preflight.sh`.
- `/home/tree/ai/codex/script/orc_gate_preflight.sh`가 실패하면 구현/완료 보고를 금지하고 원인 수정 후 0단계부터 재시작한다.
- `turn_aborted`가 발생하면 항상 0단계부터 재개한다.
- preflight 실행 전 현재 경로에 `job.md`, `AGENTS.md(or AGENTS.override.md)`, `.project/orc_gate_trace.log`가 있는지 먼저 확인한다.
- 위 마커가 충족되지 않으면 repo root가 아니므로 preflight를 실행하지 않고 root부터 재설정한다.

### Preflight Script Path Canonical Rule (Global)
- preflight 실행 명령은 항상 절대경로 `/home/tree/ai/codex/script/orc_gate_preflight.sh`만 사용한다.
- `scripts/orc_gate_preflight.sh` 같은 상대경로/별칭/탐색 실행을 금지한다.
- 경로 실패 시 다른 경로를 찾지 말고, 절대경로 문자열과 실행 위치(repo root)만 재점검한다.

### Recursive Improvement Loop Rule (Global)
- 반복 위반 시 `/home/tree/ai/codex/script/orc_recursive_improve.sh`로 tmux worker + `orc send-tmux` codex `/plan` 요청 + preflight 재검증을 수행한다.
- 실행 체인은 `orc add_orc_drafts -> orc impl_orc_code(병렬) -> orc check_orc_code -> orc clit test -p . -m "<task>"`로 고정한다.
- 성공 기준은 `job.md` 존재, `#task` 고정, `/home/tree/ai/codex/script/orc_gate_preflight.sh pipeline` 통과다.

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

### 스크린샷 언급 
- `current.png`는 기본적으로 `/mnt/c/Users/tende/Pictures/Screenshots/current.png`로 바로 처리하고, 저장소 전체 검색은 사용자 후속 요청이 있을 때만 한다.
- 사용자가 `current.png`로 UI 문제를 지적한 턴에서는 test 산출 스크린샷만으로 완료 판정을 내리지 않는다. `current.png`에 보인 레이아웃 실패 조건을 직접 체크리스트로 적고, 수정 후 같은 조건이 사라졌는지 기준으로만 완료를 판단한다.
- 사용자가 `current.png에 있는 것처럼 하라`고 지시하면, 같은 턴의 `current.png`는 문제 예시가 아니라 목표 배치 설계도로 취급한다. 이 경우 완료 기준은 `current.png`와의 레이아웃 유사성`이며, assistant가 스스로 더 낫다고 판단한 배치로 치환하면 안 된다.
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
### "다음부터" Improvement Logging Rule (Highest Priority)
- If the assistant says phrases equivalent to `다음부터` (for example: `앞으로는`, `재발 방지로`) in any response, it must first identify at least one concrete process improvement.
- The identified improvement must be written to `AGENTS.md` in the same turn before finishing the response.
- Response-only promises without rule update are invalid and treated as process violation.

### Action Log Trace Rule
- 사용자가 오류 해결 중 동작 기록을 남기라고 지시하면, 구현과 검증 동안 저장소의 `log.md`에 단계별 실행 기록을 append 한다.
- 기록 최소 단위는 `시각`, `동작`, `대상`, `결과` 4항목이다.
- 같은 오류가 다시 발생하면 새 항목에 이전 항목 참조 또는 `반복` 표시를 남겨 재발 여부를 식별한다.
- 최종 보고 전에 이번 턴에서 남긴 `log.md` 항목으로 반복 실패 여부를 한 번 요약 점검한다.
## 2026-03-21 - Global Settings First
- 작업 시작 전 전역설정 파일을 먼저 읽고 준수한 다음 저장소 작업을 진행한다.

## 2026-03-21 - Full Scope Trigger Rule
- 사용자가 `전부`, `모두`, `전체`를 명시하면 부분 수정/부분 보고를 금지하고 관련 범위를 전수 반영한다.
- 전수 반영 범위에는 구현 코드, 호출 경로, 규칙/문서, 검증 경로가 포함된다.
- 미완료 항목이 1개라도 남아 있으면 완료 보고를 금지한다.

## 2026-03-21 - Skill Global Legacy Cleanup Rule
- 사용자가 Skill 전역설정 정리를 지시하면 `/home/tree/ai/skills`, `/home/tree/.codex/skills`의 설정 문서를 전수 점검한다.
- legacy/구식 경로/구식 호환 문구가 발견되면 남김없이 현재 표준 규칙으로 치환한다.
- 전수 점검 후에는 잔존 문자열 검색 결과를 함께 확인하고 보고한다.
