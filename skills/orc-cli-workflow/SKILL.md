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
- 사용자 기준 산출물 확인은 항상 `./.project/drafts.yaml`, `./.project/feedback.md`만 사용한다.
- 내부 준비 명령이 더 있더라도 진행 판단과 완료 판정은 위 두 문서로만 한다.
- 병렬 build가 완료되면 즉시 `orc clit test -p . -m "<build 완료 기능 요약>"`를 실행해 현재 작업 디렉터리를 점검하고 `./.project/feedback.md`를 남긴다.
- `./.project/feedback.md`는 단일 운영 문서로 사용하며 최소 `# 문제`, `# 해결`, `#개선필요` 섹션을 사용한다.
- 계획/구현/점검 중 새로 발견한 이슈는 `# 문제`에 추가한다.
- 해결된 이슈는 `# 문제`에서 `# 해결`로 이동한다.
- `#개선필요`에는 이미 반영한 변경 설명을 적지 않고, 앞으로 더 손봐야 할 지점, 반복 병목, 재시도 비용을 줄일 개선 후보만 남긴다.
- `clit test`의 `-p`는 기본적으로 현재 작업 루트(`.`)를 사용한다. 별도 실행 대상 폴더가 분명할 때만 그 경로로 바꾼다.
- 화면 캡처 산출물은 작업 루트의 `./.captures/`에만 저장하고 Git에 포함하지 않는다. `screen-capture.png`, `rect-capture.png` 같은 파일을 프로젝트 루트나 소스 디렉터리에 남기지 않는다.
- 완료 시에는 반드시 `nf -m "<task-name> complete"`를 실행해 완료 알림을 보낸다.

## 표준 실행 순서
0. 초기 폴더 빠른 경로 확인
- 빠른 자동 경로: `orc auto -f`
- 이 경로에서는 필요한 내부 준비 명령을 거친 뒤 `./.project/drafts.yaml` 반영 단계부터 구현까지 진행한다.

1. 프로젝트 초기화
- `orc init_code_project -a "<요구사항>"`
- 확인: 이후 draft 단계가 `./.project/drafts.yaml` 갱신으로 이어질 준비가 되었는지 본다.

2. 계획 생성
- `orc init_code_plan -a`
- 필요 시 기능 보강: `orc add_code_plan -a` 또는 `orc add_code_plan -m "<feature>"`
- 확인: 계획 보강 결과가 다음 `drafts.yaml` 갱신에 반영될 수 있는 상태인지 본다.

3. 구현 입력 준비
- `orc create_input_md`
- 확인: 준비 결과가 다음 `drafts.yaml` 갱신으로 이어지는지 본다.

4. draft 생성
- 파일 기반: `orc add_code_draft -f`
- auto 입력 기반: `orc add_code_draft -a`
- 확인: `./.project/drafts.yaml` (`planned/worked/complete/failed` 상태)

5. 구현 실행
- `orc impl_code_draft`
- 성공/실패 상태는 `./.project/drafts.yaml`와 `./.project/feedback.md`에서 확인한다

6. 병렬 build 완료 점검
- `orc clit test -p . -m "<build 완료 기능 요약>"`
- 확인: `./.project/feedback.md` (`# 문제`는 현재 남아 있는 이슈, `# 해결`은 해결 완료 이력, `#개선필요`는 프로세스 개선 항목)

6-1. 병렬 build 이후 Unit Test 작성/실행 (의무)
- 병렬 build가 끝나면 각 기능(feature)별 unit test를 반드시 작성한다.
- 테스트 코드 작성 전에는 `check` 단계로 진행할 수 없다.
- 최소 기준:
  - 기능별 테스트 1개 이상
  - 정상 흐름 + 핵심 실패/경계 조건 포함
- Rust 기준 필수 명령:
  - `cargo test`
- 기능별 unit test 작성/실행 중 발견한 문제는 `./.project/feedback.md`의 `# 문제`에 기록하고, 해결 완료 시 `# 해결`로 이동한다.

7. 점검/리포트
- 이 단계는 메인 pane이 직접 닫지 않고 `feedback-followup-agent` 호출 단계로 처리한다.
- followup agent가 `check-code` skill과 `orc-cli-workflow` skill을 사용해 코드 체크, `.project/feedback.md` 갱신, `.project/drafts.yaml` 기준 문제 해결, 재검증을 수행한다.
- 확인: `./.project/drafts.yaml`, `./.project/feedback.md`

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
- 실패 또는 산출물 미완성(`./.project/drafts.yaml`, `./.project/feedback.md` 비정상) 상태면 동일 단계를 새 워커 pane에서 재실행한다.
- 완료 조건: 병렬 build 완료 후 `orc clit test -p . -m "<build 완료 기능 요약>"`가 실행되어 `./.project/feedback.md`가 생성되고, 이후 `check_code_draft -a`가 통과한 상태.
- `읽고 처리해줘` 트리거에서는 기존 `drafts.yaml` 문맥을 읽는 경로를 사용하고 추가 입력 준비 결과를 별도 기준 문서로 삼지 않는다.

## 단계별 위임 순서 (트리거별)
1. `"~~~을 만들어줘"` 또는 `"~~~을 추가해줘"`
- 워커 pane 열기 -> `orc send-tmux`로 `orc auto "<요구사항>"` 실행
- manager pane이 추가 질문을 수집
- 워커 pane별로 `orc init_code_plan -a`(또는 `orc add_code_plan -m "<추가요구>"`) -> `orc add_code_draft -a` -> `orc impl_code_draft` -> `orc clit test -p . -m "<build 완료 기능 요약>"` -> `orc check_code_draft -a` 순차 위임
2. `"~을 읽고 처리해줘"`
- 사전 조건: `./.project/drafts.yaml`가 존재해야 한다.
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
- 실패 시 우선 `./.project/drafts.yaml`, `./.project/feedback.md`를 확인한다.
- 프로세스 점검 내용은 `./.project/feedback.md`의 `# 문제` 또는 `#개선필요`에 기록한다.
- 재시도는 자동 반복보다 단계별 수동 재실행을 우선한다.
  - 예: `orc add_code_draft -a` -> `orc impl_code_draft` -> `orc clit test -p . -m "parallel build verification"`
- 동일 실패가 반복되면 실패 항목만 분리해 `add_code_draft -m`으로 축소 재진입한다.
- 내부 자동 재시도 제어:
  - `ORC_AUTO_RETRY_MAX` (기본 `0`, 0이면 해결될 때까지 반복)
  - `ORC_AUTO_RETRY_SLEEP_SEC` (기본 `2`)

## 2026-03-13 - Feedback Zero Gate
- `./.project/feedback.md`의 `# 문제`에 항목이 하나라도 남아 있으면 완료로 판정하지 않는다.
- 해결된 항목은 `# 문제`에서 `# 해결`로 이동해야 한다.
- `#개선필요`는 프로세스 개선 메모이므로 단독으로 완료를 막지 않는다.
- 고정 루프: `feedback 갱신 -> todo.md 병합(원인-해결 매핑) -> 코드/프로세스 실제 변경 -> 검증 재실행 -> feedback 재확인`.
- `orc check_code_draft -a` 통과, 테스트 통과, 산출물 생성은 보조 신호일 뿐이며 feedback의 남은 `# 문제`를 덮지 못한다.
- 사용자가 명시적으로 defer를 지시한 항목만 예외로 남길 수 있다.

## 2026-03-13 - Guard Checklist Step
- `check_code_draft -a` 이후, 완료 보고 전 settings checklist 가드를 확인한다.
- 체크리스트 기준: `# 문제` 잔여 항목 0건 + 해결 항목은 `# 해결`로 이동 + 필수 검증 명령 성공.
- 체크리스트 하나라도 미충족이면 해당 run은 실패로 간주하고 즉시 다음 수정 루프로 되돌아간다.
- manager-worker 경로에서도 manager pane은 체크리스트 통과 로그를 회수하기 전에는 완료 단계로 이동할 수 없다.

## Feedback Followup Pane
- 이 절은 `orc-cli-workflow`의 check 단계 자체다.
- 완료 직전 `.project/feedback.md`의 `# 문제`에 항목이 남아 있으면 `feedback-followup-agent` skill을 즉시 적용한다.
- 이 경우 메인 pane은 완료 보고를 멈추고 새 tmux pane을 연다.
  - `tmux split-window -h -P -F '#{pane_id}'`
- 새 pane에는 잔여 `# 문제`만 전담하는 followup agent를 보낸다.
  - 선호: `orc send-tmux <pane_id> "<followup command>" enter`
- followup agent 프롬프트에는 `check-code` skill과 `orc-cli-workflow` skill 사용을 반드시 포함한다.
- followup command는 반드시 `feedback을 읽고 drafts.yaml 기준으로 남은 문제를 해결하라`는 직접 명령을 포함한다.
- followup agent는 `orc-cli-workflow` 안의 `check_code_draft -a`/feedback 정리/재검증 구간을 전담 수행한다.
- followup agent는 수정 -> 검증 -> `# 해결` 이동을 반복해 `# 문제`가 0개가 될 때까지 종료할 수 없다.
