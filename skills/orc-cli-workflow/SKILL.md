---
name: orc-cli-workflow
description: rust-orc 프로젝트를 orc 명령으로 단계별 실행하고, tmux pane 위임/완료 회수 방식으로 project/plan/draft/impl를 운영할 때 사용한다.
---

# ORC CLI Workflow

## 목적
- `orc` 명령만 사용해 `project -> plan -> draft -> impl -> check`를 순서대로 처리한다.
- tmux 환경에서는 각 단계를 워커 pane으로 위임하고, 완료 메시지를 메인 pane으로 회수한 뒤 워커 pane을 닫는다.

## 기본 규칙
- `자동으로` 라는 명령이 없다면 각 단계는 명령을 명시적으로 한 번씩 실행한다.
- `orc auto`/`orc auto -f`는 에서 실패했다면 실패 이유를 출력하고 대기한다
- `.job.md`는 단일 운영 문서로 사용한다. 
- 계획/구현/점검 중 새로 발견한 이슈는 `.job.mb#requirement`에 추가한다.
- `.job.md#feedback`에는 이미 반영한 변경 설명을 적지 않고, 앞으로 더 손봐야 할 지점, 반복 병목, 재시도 비용을 줄일 개선 후보만 남긴다.
- `clit test`의 `-p`는 기본적으로 현재 작업 루트(`.`)를 사용한다. 별도 실행 대상 폴더가 분명할 때만 그 경로로 바꾼다.
- 화면 캡처 산출물은 작업 루트의 `./.project/captures/`에만 저장하고 Git에 포함하지 않는다.
- 완료 시에는 반드시 `nf -m "<task-name> complete"`를 실행해 완료 알림을 보낸다.
# 분기 설정 
- `추가해` , `개선해` 같은 명령어가 입력된 경우는 이미 있는 프로젝트에 기능을 추가하는 경우이므로 `#기능 추가`를 따라간다
- `요구사항` 과 함께 `생성해`, `만들어` 라고 명령하면 `# 프로젝트 초기화` 순서를 따라간다. 
## 기능 추가 순서
- 현재 폴더내에 `.job.md`가 있는지 확인후 있다면 `job.md`를 지우고 `references/job.md`문서 형식을 생성한다. 
- 사용자의 입력에 맞춰서 `job.md#requriement` 항목을 채운다 
- `orc add-function`기능을 수행한다.
## 프로젝트 초기화
- `orc init_code_project -a "<요구사항>"` 을 수행한다.

# 작업 완료시 
- 현재  pane은 `manager pane`으로 고정한다.
- 워커 pane을 생성한다. 이때 생성은 `tmux split-window -h -P -F '#{pane_id}'`로 pane id를 받아 처리한다. (`rust-orc/src/tmux/mod.rs`와 동일하게 좌/우 분할 고정)
- 각 워커 실행은 `orc send-tmux <worker_pane_id> "<명령>" enter`로 전달한다.
- 워커 종료 시 `orc send-tmux <manager_pane_id> "<stage>:done|fail:<reason>" enter` 형식으로 회수한다.





