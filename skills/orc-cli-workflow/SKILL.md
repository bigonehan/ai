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
- `orc auto`/`orc auto -f` 실패, 단계별 ORC 명령 실패, 검증 실패, 새 문제 감지 중 하나라도 발생하면 즉시 재귀 개선 루프를 시작한다(대기 금지).
- `.job.md`는 단일 운영 문서로 사용한다. 
- 계획/구현/점검 중 새로 발견한 이슈는 `.job.mb#requirement`에 추가한다.
- `.job.md#feedback`에는 이미 반영한 변경 설명을 적지 않고, 앞으로 더 손봐야 할 지점, 반복 병목, 재시도 비용을 줄일 개선 후보만 남긴다.
- `clit test`의 `-p`는 기본적으로 현재 작업 루트(`.`)를 사용한다. 별도 실행 대상 폴더가 분명할 때만 그 경로로 바꾼다.
- 화면 캡처 산출물은 작업 루트의 `./.project/captures/`에만 저장하고 Git에 포함하지 않는다.
- 완료 시에는 반드시 `nf -m "<task-name> complete"`를 실행해 완료 알림을 보낸다.
- 구현 시작 하드 게이트는 고정한다: `전역설정 읽기 -> orc create_job_md -> job.md#task 고정 -> git rev-parse --is-inside-work-tree 확인`.
- 하드 게이트 실패 시 구현을 금지하고, 원인 수정 후 0단계부터 재시도한다.
- ORC 체인 실행 전 반드시 현재 저장소 루트 `AGENTS.md`를 읽고 해당 규칙을 준수한다.
- UI 검증 절차의 상세 규칙은 스킬에 중복 정의하지 않고, 항상 현재 저장소 `AGENTS.md`를 단일 원천으로 따른다.
- 저장/삭제/생성/수정처럼 상태 변화가 있는 UI 기능은 `selector/screenshot`만으로 완료 판정하지 않는다.
- 위 기능은 최소 `입력 또는 클릭 -> 상태 변화 트리거 -> reload/reopen 또는 동등한 재조회 -> selector/assert 확인` 순서를 포함해야 한다.

## Stage Gate (Plan-First)
- `project`, `domain`, `plan`, `draft` 단계는 실제 `orc` 명령 실행 전에 `/plan` 모드로 먼저 사고하고 결정사항을 고정한다.
- 위 4개 단계는 `/plan`에서 목표, 입력/출력, 검증 기준을 확정한 뒤 normal mode로 전환해 실행한다.
- `impl`, `check` 단계는 `/plan` 선행을 강제하지 않고 normal mode에서 바로 실행한다.
- 같은 단계를 재시도할 때는 직전 `/plan` 결과가 유효하면 재사용하고, 요구사항 변경이 있을 때만 `/plan`을 다시 수행한다.
# 분기 설정 
- `추가해` , `개선해` 같은 명령어가 입력된 경우는 이미 있는 프로젝트에 기능을 추가하는 경우이므로 `#기능 추가`를 따라간다
- `요구사항` 과 함께 `생성해`, `만들어` 라고 명령하면 `# 프로젝트 초기화` 순서를 따라간다. 
## 기능 추가 순서
- 현재 폴더내에 `.job.md`가 있는지 확인후 있다면 `job.md`를 지우고 `references/job.md`문서 형식을 생성한다. 
- 사용자의 입력에 맞춰서 `job.md#requriement` 항목을 채운다 
- `domain/plan/draft` 진입 전에는 `## Stage Gate (Plan-First)`를 적용해 `/plan` 선행 결정을 먼저 확정한다.
- `orc add-function`기능을 수행한다.
## 프로젝트 초기화
- `project/plan/draft` 단계는 실행 전 `/plan` 선행으로 범위/검증 기준을 먼저 확정한다.
- 초기화 하드 규칙은 `project.md 생성 -> bootstrap -> job/draft` 순서로 고정한다.
- `orc init_orc_project` 또는 별칭 `orc init_code_project`를 사용한다.
- `-m <요구사항>` 또는 `-n/-d/-s`로 `./.project/project.md`를 먼저 생성한다.
- `-a` bootstrap은 반드시 방금 생성된 `./.project/project.md`의 `name/spec/path`를 다시 읽어 실행한다.
- `./.project/project.md#architecture`에 `name:` 값이 있으면 같은 이름의 architecture skill contract를 draft/check 단계에 함께 주입한다.
- `spec`가 비어 있으면 bootstrap 단계로 넘어가지 말고 먼저 `project.md` 입력 생성 경로를 점검한다.
- 대상 루트가 분리되어 있으면 `-p <path>`를 사용해 프로젝트 루트를 먼저 고정한 뒤 그 루트 안에서 `.project/project.md`를 생성한다.


## Plan Mode Auto-Continuation (Mandatory)
- Codex가 `/plan` 모드 응답에서 `<proposed_plan>`을 확정하면 normal mode 전환 직후 아래 체인을 자동 실행한다.
- 자동 체인: `job.md 계획 섹션 갱신 -> orc init_orc_job|create_job_md -> orc add_orc_drafts -> planned draft 병렬 orc impl_orc_code -> orc check_orc_code -> orc clit test -p . -m "<task-name>"`.
- `drafts.yaml` 생성은 ORC 명령 전용이며, 수동 작성/수정으로 대체하지 않는다.
- 체인 실행 전 preflight: 현재 루트가 Git 저장소인지 확인하고 아니라면 `git init` 후 진행한다.
- 병렬 구현은 기본적으로 `planned` 상태 draft_item 전체를 동시에 처리한다.
- `add_orc_drafts`, `impl_orc_code`, `check_orc_code`, `orc clit test`는 각 `timeout 180s`로 실행하고 실패 시 동일 단계를 최대 2회 재시도한다.
- 단계 실패 시 다음 단계로 넘어가지 않고 실패 로그를 남긴 뒤 같은 단계부터 재시도한다.
- 구현 자체는 manager pane에서 직접 하지 않고 반드시 tmux worker pane에 위임한다.
- manager pane은 worker의 `done` 메시지를 받은 직후 `job.md`를 다시 읽고, 그 다음에만 e2e와 스크린샷 검증을 시작한다.
- worker 성공 보고만으로 완료를 확정하면 안 된다.



## Draft Parse Safety
- `orc add_orc_drafts` 단계는 파서 입력이므로 내부 LLM 응답을 구조 데이터 전용으로 제한한다.
- 금지: 서술형 문장, 완료 안내, 파일 목록, Markdown code fence.
- 허용: 파서가 읽는 draft item YAML/JSON 본문만.

# 작업 완료시 
- 현재  pane은 `manager pane`으로 고정한다.
- 워커 pane은 `orc worker-create`로 생성하고, 이후 명령은 `orc worker-send`, 대기는 `orc worker-wait`, 종료는 `orc worker-close`만 사용한다.
- 각 워커 실행은 `orc worker-send <worker_ref> "<명령>" enter`로 전달한다.
- 워커 종료 시 worker 출력은 `worker:<pane_id>:done|fail:<reason>` 형식을 유지하되 manager 대기는 `orc worker-wait <worker_ref> "<pattern>" ...`로 회수한다.
- worker가 `done`을 보낸 뒤 manager는 반드시 `job.md`를 다시 읽고 worker 보고와 현재 상태가 일치하는지 확인한다.
- manager 검증 순서는 고정한다: `job.md 재확인 -> e2e 실행 -> ./.project/captures/ 스크린샷 저장 -> 스크린샷 확인 -> 완료/실패 판정`.
- UI 작업은 e2e와 스크린샷 확인이 둘 다 끝나기 전까지 완료로 처리하면 안 된다.
- 상태 변화 기능은 manager가 `render verified`와 `state-change verified`를 구분해 기록해야 하며, 전자만으로는 완료 처리하지 않는다.
- manager 검증이 실패하면 manager가 `job.md`를 새 실패 상태 기준으로 갱신하고, 남은 문제를 적은 뒤 새 worker pane을 열어 같은 과정을 반복한다.
- 실패 시 갱신된 `job.md`에는 최소 `worker 완료 보고 요약`, `manager 실패 단계`, `남은 문제`, `다음 worker 작업`, `재검증 기준`이 있어야 한다.
- 아래 상황이 발생하면 `job.md`에 실패 원인과 다음 조치를 기록한 뒤 `/plan` 재진입 + 고정 ORC 체인 재실행으로 재귀 개선 루프를 즉시 실행한다.
- 트리거: 순서 위반, 단계 실패, 테스트 실패, 새 실패 원인 발견.
- 재귀 개선 루프 성공 기준: `job.md` 생성/유지 + `#task` 고정 + Git preflight 통과.
- 재귀 개선 루프의 실행 체인은 고정한다: `orc init_orc_job|create_job_md -> orc add_orc_drafts -> orc impl_orc_code(병렬) -> orc check_orc_code -> orc clit test -p . -m "<task>"`.
