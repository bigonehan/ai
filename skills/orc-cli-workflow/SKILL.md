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
- 구현 시작 하드 게이트는 고정한다: `전역설정 읽기 -> orc create_job_md -> job.md#task 고정 -> /home/tree/ai/codex/script/orc_gate_preflight.sh`.
- 하드 게이트 실패 시 구현을 금지하고, 원인 수정 후 0단계부터 재시도한다.
- ORC 체인 실행 전 반드시 현재 저장소 루트 `AGENTS.md`를 읽고 해당 규칙을 준수한다.
- UI 검증 절차의 상세 규칙은 스킬에 중복 정의하지 않고, 항상 현재 저장소 `AGENTS.md`를 단일 원천으로 따른다.

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
- `orc init_code_project -a "<요구사항>"` 을 수행한다.


## Plan Mode Auto-Continuation (Mandatory)
- Codex가 `/plan` 모드 응답에서 `<proposed_plan>`을 확정하면 normal mode 전환 직후 아래 체인을 자동 실행한다.
- 자동 체인: `job.md 계획 섹션 갱신 -> orc create_job_md -> orc add_code_draft_item/add_code_draft -> planned draft 병렬 impl_code_draft -> orc check_code_draft -> orc clit test -p . -m "<task-name>"`.
- `drafts.yaml` 생성은 ORC 명령 전용이며, 수동 작성/수정으로 대체하지 않는다.
- 체인 실행 전 preflight: 현재 루트가 Git 저장소인지 확인하고 아니라면 `git init` 후 진행한다.
- 병렬 구현은 기본적으로 `planned` 상태 draft_item 전체를 동시에 처리한다.
- `add_code_draft_item/add_code_draft`, `impl_code_draft`, `check_code_draft`, `clit test`는 각 `timeout 180s`로 실행하고 실패 시 동일 단계를 최대 2회 재시도한다.
- 단계 실패 시 다음 단계로 넘어가지 않고 실패 로그를 남긴 뒤 같은 단계부터 재시도한다.



## Draft Parse Safety
- `orc add_code_draft_item/add_code_draft` 단계는 파서 입력이므로 내부 LLM 응답을 구조 데이터 전용으로 제한한다.
- 금지: 서술형 문장, 완료 안내, 파일 목록, Markdown code fence.
- 허용: 파서가 읽는 draft item YAML/JSON 본문만.

# 작업 완료시 
- 현재  pane은 `manager pane`으로 고정한다.
- 워커 pane을 생성한다. 이때 생성은 `tmux split-window -h -P -F '#{pane_id}'`로 pane id를 받아 처리한다. (`rust-orc/src/tmux/mod.rs`와 동일하게 좌/우 분할 고정)
- 각 워커 실행은 `orc send-tmux <worker_pane_id> "<명령>" enter`로 전달한다.
- 워커 종료 시 `orc send-tmux <manager_pane_id> "<stage>:done|fail:<reason>" enter` 형식으로 회수한다.
- 반복적인 순서 위반이 있으면 `/home/tree/ai/codex/script/orc_recursive_improve.sh <root> <task-name> "<plan 요청>"`으로 재귀 개선 루프를 실행한다.
- 재귀 개선 루프 성공 기준: `job.md` 생성/유지 + `#task` 고정 + `/home/tree/ai/codex/script/orc_gate_preflight.sh` 통과.
- 재귀 개선 루프의 실행 체인은 고정한다: `orc create_job_md -> orc add_code_draft_item/add_code_draft -> orc impl_code_draft(병렬) -> orc check_code_draft -> orc clit test -p . -m "<task>"`.


