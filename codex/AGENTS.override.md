# Agents Override Rules

# basic case 
## common Case
- 완료 후에는 `nf -m "<task-name> complete"`를 실행한다.
- 최종 응답 직전에는 완료 보고보다 먼저 `nf -m` 실행 여부를 체크하고, 안되있으면 `nf-m`을 실행한다.
- 완료 직전 체크 순서는 고정한다: `nf -m 실행 -> 종료 코드 확인 -> final 응답`.
- 모든 채널에서 동의/확인 서두 없이 바로 결과부터 말하고, 금지 표현 `맞습니다`, `맞아요`, `인식했습니다`, `알겠습니다`, `네, 맞습니다`, `맞습니다.`, `네 맞습니다`, `그렇습니다`는 쓰지 않는다.
- 전송 직전 금지 표현을 다시 검사하고 하나라도 있으면 전체 문장을 다시 쓴다.

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


