---
name: orc-cli-workflow
description: rust-orc 프로젝트를 orc 명령으로 단계별 실행하고, tmux pane 위임/완료 회수 방식으로 project/plan/draft/impl를 운영할 때 사용한다.
---

# ORC CLI Workflow

## 목적
- `orc` 명령만 사용해 `project -> plan -> draft -> impl -> check`를 순서대로 처리한다.
- tmux 환경에서는 각 단계를 워커 pane으로 위임하고, 완료 메시지를 메인 pane으로 회수한 뒤 워커 pane을 닫는다.

## 기본 규칙
- 자동 연쇄 호출에 의존하지 않는다. 각 단계는 명령을 명시적으로 한 번씩 실행한다.
- `orc auto`/`orc auto -f`는 내부 재시도 루프로 unresolved 상태를 반복 처리한다. (기본 무제한, `ORC_AUTO_RETRY_MAX=0`)
- 산출물은 항상 `./.project` 기준으로 확인한다.
  - `./.project/project.md`
  - `./.project/plan.yaml`
  - `./.project/drafts.yaml`
- `plan.md`가 아니라 `plan.yaml`을 사용한다.
- 폴더 상태를 먼저 확인한다. auto 모드에서는 `input.md` 유무와 관계없이 `orc create_input_md`를 통해 최신 input을 생성한 뒤 진행한다.
- 병렬 build가 완료되면 즉시 `orc clit test -p . -m "<build 완료 기능 요약>"`를 실행해 현재 작업 디렉터리를 점검하고 `./.project/feedback.md`를 남긴다.
- `./.project/feedback.md`에는 최소 `# 문제`, `# 미해결점`, `#개선필요` 섹션을 사용한다.
- `#개선필요`에는 이미 반영한 변경 설명을 적지 않고, 앞으로 더 손봐야 할 지점, 반복 병목, 재시도 비용을 줄일 개선 후보만 남긴다.
- `clit test`의 `-p`는 기본적으로 현재 작업 루트(`.`)를 사용한다. 별도 실행 대상 폴더가 분명할 때만 그 경로로 바꾼다.
- 완료 시에는 반드시 `nf -m "<task-name> complete"`를 실행해 완료 알림을 보낸다.

## 표준 실행 순서
0. 초기 폴더 빠른 경로 확인
- 빠른 자동 경로: `orc auto -f`
- 이 경로에서는 `project.md + plan.yaml` 기준으로 `create_input_md`를 먼저 실행해 `input.md`를 생성/갱신한 뒤 구현 단계까지 진행한다.

1. 프로젝트 초기화
- `orc init_code_project -a "<요구사항>"`
- 확인: `./.project/project.md`

2. 계획 생성
- `orc init_code_plan -a`
- 필요 시 기능 보강: `orc add_code_plan -a` 또는 `orc add_code_plan -m "<feature>"`
- 확인: `./.project/plan.yaml`

3. input 생성
- `orc create_input_md` (`project.md + plan.yaml` 기반으로 `./input.md` 생성/갱신)
- 확인: `./input.md`

4. draft 생성
- 파일 기반: `orc add_code_draft -f`
- auto 입력 기반: `orc add_code_draft -a` (`project.md + plan.yaml` 기준으로 `build_input_md_auto()`를 통해 `./input.md`를 생성한 후 draft를 생성)
- 확인: `./.project/drafts.yaml` (`planned/worked/complete/failed` 상태)

5. 구현 실행
- `orc impl_code_draft`
- 실패 항목은 `./.project/drafts.yaml.failed`로 이동되는지 확인

6. 병렬 build 완료 점검
- `orc clit test -p . -m "<build 완료 기능 요약>"`
- 확인: `./.project/feedback.md` (`#개선필요`에는 미래 개선 항목만 기록)

7. 점검/리포트
- `orc check_code_draft -a`
- 확인: `report.md`, `./.project/feedback.md`

## tmux 위임 운영
- 메인 pane에서 위 명령을 실행하면 하위 단계는 워커 pane으로 분기될 수 있다.
- 워커 pane은 종료 시 메인 pane에 완료/실패 메시지를 전달한다.
- 완료/실패 메시지 전달 후 워커 pane은 자동으로 닫힌다.

## Manager-Worker 강제 흐름
- 트리거: 스킬 로드 상태에서 사용자 입력이 아래 형태면, 최초 입력 pane은 `manager pane`으로 고정한다.
  - `"~~~을 만들어줘"`
  - `"~~~을 추가해줘"`
  - `"~을 읽고 처리해줘"`
- manager pane은 직접 구현 명령을 실행하지 않고, 단계마다 새 워커 pane을 열어 위임한다.
- 워커 pane 생성은 `tmux split-window -h -P -F '#{pane_id}'`로 pane id를 받아 처리한다. (`rust-orc/src/tmux/mod.rs`와 동일하게 좌/우 분할 고정)
- 각 워커 실행은 `orc send-tmux <worker_pane_id> "<명령>" enter`로 전달한다.
- 워커 종료 시 `orc send-tmux <manager_pane_id> "<stage>:done|fail:<reason>" enter` 형식으로 회수한다.
- manager pane은 회수 메시지를 보고 다음 단계 진행/재시도 여부를 결정한다.
- 실패 또는 산출물 미완성(`.project/*.yaml` 누락/비정상) 상태면 동일 단계를 새 워커 pane에서 재실행한다.
- 완료 조건: 병렬 build 완료 후 `orc clit test -p . -m "<build 완료 기능 요약>"`가 실행되어 `./.project/feedback.md`가 생성되고, 이후 `check_code_draft -a`가 통과한 상태.
- `읽고 처리해줘` 트리거에서는 기존 `input.md`를 읽는 경로를 사용하고 `create_input_md`를 호출하지 않는다.

## 단계별 위임 순서 (트리거별)
1. `"~~~을 만들어줘"` 또는 `"~~~을 추가해줘"`
- 워커 pane 열기 -> `orc send-tmux`로 `orc auto "<요구사항>"` 실행
- manager pane이 추가 질문을 수집
- 워커 pane별로 `orc init_code_plan -a`(또는 `orc add_code_plan -m "<추가요구>"`) -> `orc add_code_draft -a` -> `orc impl_code_draft` -> `orc clit test -p . -m "<build 완료 기능 요약>"` -> `orc check_code_draft -a` 순차 위임
2. `"~을 읽고 처리해줘"`
- 사전 조건: `./input.md`가 존재해야 한다.
- 워커 pane별로 `orc add_code_plan -f` -> `orc add_code_draft -f` -> `orc impl_code_draft` -> `orc clit test -p . -m "<build 완료 기능 요약>"` -> `orc check_code_draft -a` 순차 위임
- 이 경로에서는 `orc create_input_md`를 실행하지 않는다.
- 공통: 단계별 완료/실패 회수 후 manager가 다음 단계/재시도 판단

## 운영 예시 (manager pane 관점)
- `tmux split-window -h -P -F '#{pane_id}'` -> `<worker_auto>`
- `orc send-tmux <worker_auto> "cd /home/tree/project/rust-orc && orc auto \"react todo를 만들어줘\"" enter`
- (회수 대기) `<auto>:done` 수신 시 다음 단계 진행
- `tmux split-window -h -P -F '#{pane_id}'` -> `<worker_plan>`
- `orc send-tmux <worker_plan> "cd /home/tree/project/rust-orc && orc init_code_plan -a" enter`
- `tmux split-window -h -P -F '#{pane_id}'` -> `<worker_draft>`
- `orc send-tmux <worker_draft> "cd /home/tree/project/rust-orc && orc add_code_draft -a" enter`
- `tmux split-window -h -P -F '#{pane_id}'` -> `<worker_impl>`
- `orc send-tmux <worker_impl> "cd /home/tree/project/rust-orc && orc impl_code_draft" enter`
- `tmux split-window -h -P -F '#{pane_id}'` -> `<worker_clit>`
- `orc send-tmux <worker_clit> "cd /home/tree/project/rust-orc && orc clit test -p . -m \"parallel build verification\"" enter`
- `tmux split-window -h -P -F '#{pane_id}'` -> `<worker_check>`
- `orc send-tmux <worker_check> "cd /home/tree/project/rust-orc && orc check_code_draft -a" enter`
- `tmux split-window -h -P -F '#{pane_id}'` -> `<worker_read_plan>`
- `orc send-tmux <worker_read_plan> "cd /home/tree/project/rust-orc && orc add_code_plan -f" enter`
- `tmux split-window -h -P -F '#{pane_id}'` -> `<worker_read_draft>`
- `orc send-tmux <worker_read_draft> "cd /home/tree/project/rust-orc && orc add_code_draft -f" enter`

## 실패 처리
- 실패 시 우선 `./.project/feedback.md`, `./.project/check-process.md`, `./.project/drafts.yaml.failed`를 확인한다.
- 재시도는 자동 반복보다 단계별 수동 재실행을 우선한다.
  - 예: `orc add_code_draft -a` -> `orc impl_code_draft` -> `orc clit test -p . -m "parallel build verification"`
- 동일 실패가 반복되면 실패 항목만 분리해 `add_code_draft -m`으로 축소 재진입한다.
- 내부 자동 재시도 제어:
  - `ORC_AUTO_RETRY_MAX` (기본 `0`, 0이면 해결될 때까지 반복)
  - `ORC_AUTO_RETRY_SLEEP_SEC` (기본 `2`)
