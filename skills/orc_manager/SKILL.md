---
name: orc_manager
description: 사용자의 요구를 먼저 /plan으로 정리한 뒤, ORC tmux pane 루프를 열어 job.md -> drafts.yaml -> impl -> check -> 개선 탐색까지 manager pane에서 반복 운영할 때 사용하는 skill.
---

# ORC Manager

## 목적
- 사용자 요구를 바로 구현하지 않고 먼저 `/plan`으로 작업 계획을 고정한다.
- 구현과 점검은 모두 tmux worker pane에서 ORC 명령으로 실행한다.
- manager pane은 worker 완료 메시지를 받은 뒤 `job.md`, ORC check, 개선 여지를 다시 확인하고 필요하면 같은 루프를 반복한다.

## 시작 규칙
- 사용자 요구를 받으면 먼저 `/plan` 모드로 계획을 작성한다.
- `<proposed_plan>`이 확정되기 전에는 ORC worker pane을 열지 않는다.
- plan이 확정되면 현재 pane을 manager pane으로 고정한다.
- manager pane에서는 직접 `orc impl_*`, `orc check_*`, `orc clit`를 실행하지 않는다.

## 구현 워커 루프
1. manager pane이 `tmux split-window -h -P -F '#{pane_id}'`로 worker pane을 연다.
2. manager pane은 `orc send-tmux <worker_pane_id> "<command>" enter` 형태로만 명령을 보낸다.
3. 첫 worker에는 확정된 계획을 함께 보내서 아래 순서로 ORC를 실행하게 한다.
4. 실행 순서:
   - `job.md` 생성 또는 갱신
   - `orc add_orc_drafts`
   - `drafts.yaml`의 draft_item 생성 확인
   - `orc impl_orc_code`
   - 구현 완료 또는 실패 판단
5. worker는 완료 시 원래 pane으로 응답 메시지를 보낸다.
6. 완료 메시지 형식은 고정한다:
   - 성공: `worker:<pane_id>:done:<report>`
   - 실패: `worker:<pane_id>:fail:<reason>`

## manager 검증 루프
- worker의 `done` 메시지를 받아도 바로 성공으로 판정하지 않는다.
- manager pane은 항상 다음 순서로 검증한다:
  - `job.md` 읽기
  - 남아 있는 문제 확인
  - 새 tmux pane 열기
  - `orc clit` 기반 코드 점검 실행
  - 점검 완료 메시지 회수
  - 현재 sub agent pane 상태와 `job.md`를 함께 읽고 개선 필요 여부 판단
- 점검 worker도 `orc send-tmux` 방식으로만 실행한다.

## 개선 판단 규칙
- code check가 끝나면 manager pane은 아래 둘을 같이 본다:
  - 최신 `job.md`
  - 방금 끝난 sub agent pane 결과
- 둘 중 하나라도 남은 문제, 누락된 검증, 느린 루프, 반복 실패를 보여주면 개선 대상으로 간주한다.
- 개선해야 하는 대상이 코드 자체가 아니라 과정 자체인 경우, 그 과정 문제를 설정/규칙 문서에 기록한다.

## 추가 개선 탐색 루프
- 기본 구현/점검 루프가 끝난 뒤 manager pane은 새 ORC worker pane을 다시 연다.
- 그 worker에 반드시 추가 메시지를 보낸다:
  - `현재 job.md 이외에 개선할 사항을 개선해`
- 이 worker는 `job.md` 밖의 개선점까지 찾아 수정하거나 보고해야 한다.
- 개선점이 발견되면 manager pane은 다시 `job.md`, worker 결과, check 결과를 읽고 루프를 반복한다.

## 종료 조건
- `job.md`에 남은 blocking issue가 없다.
- code check worker가 성공했다.
- 추가 개선 탐색 worker도 더 이상 개선점을 찾지 못했다.
- 그 다음에만 완료를 보고한다.

## 하드게이트
- plan 없이 ORC worker pane을 열면 실패다.
- manager pane에서 직접 ORC 구현/점검 명령을 실행하면 실패다.
- worker `done` 메시지만 보고 `job.md` 재확인 없이 종료하면 실패다.
- code check를 생략하면 실패다.
- `현재 job.md 이외에 개선할 사항을 개선해` 추가 탐색을 생략하면 실패다.

## 권장 조합
- ORC 세부 실행은 `orc-cli-workflow` skill 규칙을 함께 따른다.
- 코드 점검은 `check-code` skill과 함께 사용한다.
