---
name: manage-intervention
description: 작업 도중 사용자가 작업 과정·Skill·규칙·hook·작업 도구의 결함이나 과한 작업을 지적하거나 개선을 요구한 "개입" 상태를 맡는다 — 원래 행 중단 표시, 개입 행 기록, 수정 또는 개선자 호출, 실제 hook 확인, 원래 행 복귀까지. `main-edit-gate`가 "작업 과정 수정"으로 판정했거나 `task-complete`가 "과정 수정 미완료"로 막을 때 메인 세션이 쓴다. 제품 기능 요청·순수 설명 질문, 마감 때의 개선 기록(`improve_process`), 개선자 세션(`role-improver`)에는 쓰지 않는다.
---

# Manage Intervention

작업 중 지적은 원래 작업을 버리는 사유가 아니다. 지적을 고친 뒤 멈춘 행으로 돌아가는 것이 이 Skill의 목적이다.

## 1. 진입

- 사용자 메시지가 작업 과정·Skill·작업관리기 절차·hook·작업 도구의 결함, 과한 작업, 규칙 위반을 지적하거나 개선을 요구하면 개입 상태다. 사례로 든 제품이 아니라 작업관리기 루트(`/home/tree/extensions`, Table7 1198)가 소유자다.
- 순수한 이유·의견 질문("왜 ~?", "어때?", "추천은?")은 개입이 아니다. 답부터 한다.
- 진행 중인 백그라운드 위임·검증 태스크(`manage_task kill`·`tmux kill-pane`)를 멈추고, 끝난 태스크에 `dispatch-agent --attach`를 다시 걸지 않는다.

## 2. 기록

- 중단한 원래 행마다 `$D`에 `[⏸] 개입 #<개입 행> <지적 한 줄>`을 쓴다(`grist task-update <id> --project <dir> --fields '{"D": "…"}'`). `$H`는 바꾸지 않는다. 개입용 `$H` 값은 없다.
- 지적은 작업관리기 새 행으로 기록한다(`grist request-record` → `task-add`, `manage-project` `stage-start.md`). 이미 `확인`·`완료`된 행의 증거로 대신하지 않는다(`task-complete`가 거부).

## 3. 수정

- 규칙·Skill·hook·`~/ai/scripts` 작업 도구 결함은 메인이 직접 고친다. 사용자가 위임을 지시했으면 `dispatch-agent`로 맡긴다.
- 문구보다 기계 강제를 먼저 고른다(`manage-global-rules` §1).
- 같은 행에서 2회 이상 실패했거나 착각이 있었으면 개선자를 부른다(`improve_process` §2, 작업 규칙은 `role-improver`).
- 제품 파일 탐색·제품 행 재개는 하지 않는다(`main-edit-gate`가 거부). 오분류로 시작한 제품 작업, 사용자가 "차후에/나중에"로 뺀 작업은 원복·취소하고 다시 시작하지 않는다.

## 4. 확인

- 고친 hook·명령을 hook과 같은 최소 환경(`env -i HOME=… PATH=/usr/bin:/bin`)에서 차단 사례와 허용 사례로 한 번씩 실행해 출력으로 확인한다.
- 개입 행은 일반 행처럼 `task-confirm`·`improve_process`·`task-complete --row`로 마감한다. 확인 전에는 완료를 보고하지 않는다.

## 5. 복귀

- 개입 행이 `확인`되면 2절에서 `[⏸]`를 붙인 원래 행으로 돌아가 그 행의 단계 카드(`manage-project` 하위 진입 표)를 이어 간다.
- 복귀하면 `$D`의 `[⏸]` 줄을 그 행의 최신 진행 한 줄로 덮어쓴다. 사용자가 원래 작업을 보류하라고 했으면 `[🏷] 사용자 대기 — <사유>`로 남긴다.
- 최종 보고에는 `개입: #<개입 행> → 복귀 #<원래 행>` 또는 `보류 #<원래 행>` 한 줄을 넣는다.
