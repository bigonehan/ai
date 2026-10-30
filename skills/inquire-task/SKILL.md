---
name: inquire-task
description: Split one `# 요청` detail line of an /home/tree/extensions request into goal lines that each name one observable result (screen value, return value, message or file), before `grist task-add --goal-file`/`task-goal`. Used by the manage-project orchestrator at 초안 only; not for implementers, verification or explanation-only questions.
---

# Inquire Task

메인(오케스트레이터)이 `초안`에서 행을 만들기 직전에 한 번 쓴다. 입력은 `~/ai/scripts/bin/grist request --project <dir>`의 세부 사항 한 줄이고, 출력은 그 행의 `### 목표` 번호 목록이다. 기록은 `grist task-add --goal-file`(새 행)·`grist task-goal`(초안 정정)로만 하고 changes를 손으로 쓰지 않는다.

## 측정 단위로 나누기

- 목표 한 줄 = 관측값 하나다. 형식: `N. [연결|기능|수정] <대상·입력> ⇒ <관측값>` (반환값은 `▷`).
  - 관측값은 화면에 뜨는 값, 함수·명령의 반환값·출력, 표시되는 메시지, 생기거나 바뀌는 파일·레코드 중 하나다.
  - 예: `1. [수정] 같은 파일 2개 다운로드 ⇒ Eagle 항목 2개`, `2. [수정] 저장 실패 ⇒ 에러 대신 "저장됨" 표시`, `3. [기능] grist lists ▷ # 작업·# 완료 줄만`
- 한 줄에 서로 다른 결과 둘을 `하고`·`·`·`그리고`로 묶지 않는다. 동작과 검사(예: "바꾸고 잔존 0 확인")도 두 줄로 나눈다.
- 모든 제품 작업의 목표는 수정한 문구·UI 요소 표시나 저장 상태 확인만으로 끝내지 않고, **그 수정이 속한 실제 기능을 실행했을 때 시작 동작부터 최종 소비 결과까지 연동 경로 오류 없이 정상 작동하는 관측값**을 포함한다.
- 같은 관측값을 여러 줄로 쪼개지 않는다. 대상만 다른 같은 결과(여러 문서의 같은 문구 교체)는 한 줄에 대상을 나열한다.
- 요청에 없는 조건·수치·화면을 만들지 않는다. J3가 최신 `# 요청` 블록과 대조해 넘치는 줄을 막는다. 요청이 짧으면 목표도 짧게 둔다.
- 해석이 둘 이상이라 결과가 달라질 때만 `task-add` 전에 사용자에게 묻는다.

## 이후 단계와의 연결

- 줄 번호 N이 그대로 측정 단위 번호다. 구현자는 `task-step --goal N`으로 진행 중인 단위를 기록하고, Grist `$K`에 `[연결|기능|수정]` 태그를 뺀 목표 N줄 본문만 표시된다.
- 검증 뒤 `task-confirm --evidence`는 목표 줄마다 `N. <실제 관측값>` 한 줄을 받는다. 목표 한 줄에 관측값이 하나여야 근거 한 줄과 1:1로 맞고 J1 판정이 의미를 가진다.
- 작업 중 오류가 나도 목표 줄을 늘리지 않는다. 오류는 `$I`·`$J`(`[오류 기호] 한 줄`, `manage-project` 3절 `### 원인`)에만 기록한다. 사용자가 새 요구를 말하면 새 `request-record` 블록과 새 목표를 만든다.
