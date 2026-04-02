---
name: orc_manager
description: 사용자의 요구를 먼저 /plan으로 정리한 뒤, manager session이 tmux 새 session들을 열어 job.md -> drafts.yaml -> impl -> dev 서버 유지 -> QA 시연 검증 -> check -> 개선 탐색까지 총괄 운영할 때 사용하는 skill.
---

# ORC Manager

## 목적
- 사용자 요구를 바로 구현하지 않고 먼저 `/plan`으로 작업 계획을 고정한다.
- manager는 현재 세션에서 직접 구현/점검하지 않고, 구현/QA/check/개선 탐색을 각각 새 tmux session에 위임한다.
- 모든 session의 공통 source of truth는 `job.md`다.
- 구현 session, QA session, check session, 개선 session은 시작 전에 반드시 `job.md`를 읽고 현재 상태를 기준으로만 동작한다.
- 사용자가 기존 UI 요소 아래에 무언가를 추가하라고 지시한 경우, manager와 worker는 해당 기존 UI 요소를 유지 대상으로 먼저 잠가야 한다.

## 시작 규칙
- 사용자 요구를 받으면 먼저 `/plan` 모드로 계획을 작성한다.
- `<proposed_plan>`이 확정되기 전에는 tmux worker session을 열지 않는다.
- plan이 확정되면 현재 session을 manager session으로 고정한다.
- manager session은 plan 고정 전에 사용자 원문에서 `유지해야 하는 기존 명시 요소`와 `추가해야 하는 새 요소`를 분리해 `job.md`에 반영해야 한다.
- manager session은 즉시 `orc manager-trace stage_global_override_read "<detail>"`, `orc manager-trace stage_job_md_locked "<detail>"`, `orc manager-trace stage_plan_done "<detail>"`를 순서대로 기록해야 한다.
- manager session은 직접 `orc impl_*`, `orc check_*`, dev 서버 실행, 브라우저 시연을 하지 않는다.
- manager session은 새 tmux session 생성과 상태 회수만 담당한다.
- session 간 명령 전달과 대기는 `orc worker-create`, `orc worker-send`, `orc worker-wait`, `orc worker-close`만 사용한다.
- session 이름에는 역할이 드러나야 한다.
  - 예: `impl-<task>`, `qa-<task>`, `check-<task>`, `improve-<task>`

## Session 생성 규칙
1. manager session은 `tmux new-session -d -s <session_name>` 또는 동등한 ORC 래퍼로 새 session을 만든다.
2. manager session은 구현, QA, check, 개선 탐색을 각각 다른 session으로 분리한다.
3. 한 session 안에서 새 pane을 추가해 역할을 섞지 않는다.
4. session 생성 직후 첫 명령은 항상 현재 작업 루트로 이동하고 `job.md`를 읽는 단계여야 한다.
5. manager session은 session 이름, 역할, 시작 시각, 종료 메시지를 추적한다.

## 구현 Session 루프
1. manager session은 구현용 tmux session을 새로 만든다.
2. manager session은 구현 session을 열기 전에 `orc check-manager-trace impl`을 통과해야 한다.
3. manager session은 `orc worker-send <worker_ref> "<command>" enter` 형태로만 명령을 보낸다. worker 환경 변수 전달이 필요하면 명령 문자열 안에서 명시한다.
4. 구현 session은 시작 즉시 `job.md`를 읽고 `# task`, `# problems`, `# check` 상태를 확인한다.
5. 구현 session 실행 순서는 아래로 고정한다.
   - `job.md` 생성 또는 갱신 확인
   - `orc add_orc_drafts`
   - `drafts.yaml`의 draft_item 생성 확인
   - `orc impl_orc_code`
   - 구현 완료 또는 실패 판단
   - 앱 실행 명령 선택
   - dev 서버 시작
   - health check 또는 준비 로그 확인
   - dev 서버를 유지한 채 manager session에 접속 URL 보고
6. tmux worker wrapper 또는 manager는 구현 시작/완료 시 `stage_impl_session_started`, `stage_impl_done` trace를 남겨야 한다.
7. 구현 session 완료 메시지 형식은 고정한다.
  - 성공: `worker:<session_name>:done:dev=<url>;report=<report>`
  - 실패: `worker:<session_name>:fail:<reason>`

## dev 서버 유지 규칙
- 구현 session은 구현 완료 후 종료하지 않고 dev 서버를 유지해야 한다.
- dev 서버 포트는 프로젝트별로 달라질 수 있으므로 상수로 고정하지 않는다. worker는 `job.md`, 프로젝트 설정, 실행 로그를 기준으로 실제 bind 포트를 결정하고 최종 URL을 명시적으로 회수해야 한다.
- worker는 dev 서버 시작 전에 후보 포트 점유 여부를 먼저 확인하고, 이미 다른 프로세스가 점유 중이면 해당 프로세스가 방금 띄운 서버인지 로그/명령행으로 검증해야 한다. 검증되지 않으면 다른 사용 가능한 포트로 재기동하고 보고 URL도 함께 갱신한다.
- 준비 확인은 가능하면 ORC 헬퍼 명령으로 표준화한다.
  - `orc wait-ready <pane_id> "<ready pattern>" 30000 120`
  - `orc http-healthcheck <url> 10000`
- 준비 확인은 단순 프로세스 생존만으로 끝내지 않는다. `orc http-healthcheck` 또는 동등한 `curl -I`로 최종 URL이 `200` 또는 기대 상태코드를 반환하는지 확인하고, `404 File not found`가 나오면 URL 경로 또는 서버 루트를 수정한 뒤 다시 확인한다.
- worker는 dev 서버 PID, 실제 bind 포트, document root 또는 앱 실행 루트를 함께 기록하고, manager가 `orc capture-pane`으로 바로 재검증할 수 있어야 한다.
- manager session은 구현 worker의 `done` 메시지를 받은 뒤 그 문자열만 신뢰하지 않고 `orc worker-dev-url <worker_ref|pane_id>`로 최신 실제 dev URL을 다시 회수해야 한다.
- 준비 완료 메시지는 아래 형식 중 하나로 고정한다.
  - `worker:<session_name>:done:dev=http://127.0.0.1:<actual_port>;report=<report>`
  - `worker:<session_name>:fail:dev-server:<reason>`
- `done` 메시지에는 실제로 healthcheck를 통과한 URL만 넣는다. worker가 추정한 포트, 이전 재시도에서 실패한 URL, 404를 반환한 URL을 그대로 재사용하면 실패다.
- manager session은 dev URL을 회수하기 전까지 QA session을 열지 않는다.
- dev 서버가 준비되지 않았으면 check/개선 단계로 넘어가지 않는다.

## QA Session 루프
1. manager session은 구현 session의 dev URL을 회수한 뒤 QA용 tmux session을 새로 만든다.
1.1. dev URL 회수는 `worker done` 메시지의 `dev=` 값과 `orc worker-dev-url <worker_ref|pane_id>` 결과가 일치하는지 먼저 확인하고, 불일치하면 pane tail 기준 최신 URL을 source of truth로 사용한다.
2. QA session도 시작 즉시 `job.md`를 읽고 `# check`, `# problems`, `## verify`를 확인한다.
3. QA session은 `playwright-cli` 또는 `npx playwright-cli`로 방금 열린 dev 서버에 접속한다.
4. QA session은 브라우저를 열기 전에 manager가 넘긴 dev URL에 대해 먼저 `orc http-healthcheck <url>` 또는 동등한 HTTP 확인을 수행한다. 여기서 `404`, `connection refused`, 잘못된 포트 응답이 나오면 즉시 구현 session 로그를 회수하고 잘못된 URL 보고로 되돌린다.
5. Playwright 런타임이 즉시 준비되지 않으면 바로 브라우저 실패로 끝내지 않는다. 우선 사용 가능한 실행 경로를 순서대로 시도한다.
   - 저장소 로컬 의존성
   - `npx playwright ...`
   - npx 캐시의 `node_modules/playwright`를 `NODE_PATH`에 주입한 Node 스크립트
6. QA용 스크린샷이 404 에러 페이지, 빈 서버 인덱스, 다른 앱 화면이면 UI 검증 성공으로 처리하지 않는다. 이 경우는 기능 실패가 아니라 dev URL/서버 매칭 실패로 분류하고 구현 단계로 되돌린다.
7. QA session은 `job.md`의 `# check`와 현재 requirement를 기준으로 핵심 기능 흐름을 실제로 시연한다.
8. 저장/삭제/생성/수정처럼 상태가 바뀌는 기능은 `접속 -> 입력/클릭 -> 상태 변화 트리거 -> reload/reopen 또는 동등 재조회 -> selector/assert 확인`까지 포함한다.
8.1. 사용자가 `A 아래 B 표시`처럼 기존 요소 유지 + 하위 요소 추가를 요구한 경우, QA session은 최소한 아래 두 가지를 따로 확인해야 한다.
   - 기존 요소 A가 그대로 보이는지
   - 새 요소 B가 A의 하위 영역에 실제로 보이는지
9. QA session은 실패 시 즉시 재현 절차, 문제 단계, 콘솔/네트워크/스냅샷 또는 스크린샷 경로를 함께 보고한다.
10. QA session 완료 메시지 형식은 고정한다.
   - 성공: `worker:<session_name>:done:qa=<report>;source=<real|fixture|mock>;artifact=<path>`
   - 실패: `worker:<session_name>:fail:qa=<step>;reason=<reason>;artifact=<path>`
11. QA session은 사용한 데이터 소스를 명시해야 한다.
   - 실제 앱 데이터, 실제 import 파일, 실제 런타임 state를 사용했으면 `source=real`
   - `?e2e=1`, bootstrap state, fixture html, mock 저장소스면 `source=fixture`
   - 직접 주입한 객체나 stub만으로 검증했으면 `source=mock`
12. 사용자가 보고한 실제 증상 해소 판정은 `source=real` 결과로만 할 수 있다. `source=fixture|mock` 성공은 보조 근거일 뿐 완료 조건이 아니다.

## Check Session 루프
1. manager session은 구현/QA 결과를 회수한 뒤 check 전용 tmux session을 새로 만든다.
2. manager session은 check session을 열기 전에 `orc check-manager-trace check`를 통과해야 한다.
3. check session은 시작 즉시 `job.md`를 읽고 `# problems`, `# check`, `## verify`를 확인한다.
4. check session은 `check-code` skill 기준으로 점검을 수행한다.
5. check session은 `job.md`를 source of truth로 삼아 아래 순서를 따른다.
   - `# problems` 우선 처리
   - `# check` 체크리스트 재구성 또는 재확인
   - 유닛테스트/필요한 E2E/시연 검증 실행
   - QA 결과의 `source`가 `real`인지 먼저 확인
   - 새 문제 발견 시 `# problems`에 추가
   - 모든 문제가 해결되면 `## verify` 항목을 `## complete`로 이동
6.1. `source=fixture|mock` 인 QA 결과만 존재하면 check session은 성공 처리하면 안 된다. 이 경우 원인과 함께 실패로 되돌린다.
6. tmux worker wrapper 또는 manager는 check 시작/완료 시 `stage_check_session_started`, `stage_check_done` trace를 남겨야 한다.
7. check session 완료 메시지 형식은 고정한다.
  - 성공: `worker:<session_name>:done:check=<report>`
  - 실패: `worker:<session_name>:fail:check=<reason>`

## manager 검증 루프
- worker session의 `done` 메시지를 받아도 바로 성공으로 판정하지 않는다.
- manager session은 항상 다음 순서로 검증한다.
  - 최신 `job.md` 읽기
  - 구현 session의 dev URL 및 report 확인
  - `orc worker-dev-url <worker_ref|pane_id>` 결과와 worker 보고 URL 일치 여부 확인
  - QA session 시연 결과 확인
  - QA session의 `source=real` 여부 확인
  - check session 점검 결과 확인
  - 남아 있는 `# problems` 확인
  - `## verify`가 남아 있는지 확인
  - 개선 필요 여부 판단
- manager session은 사용자 원문 증상이 실제로 재현되었는지, 그리고 같은 입력에서 실제로 사라졌는지까지 확인해야 한다. UI가 비슷하거나 fixture 시나리오만 통과한 것은 완료 근거가 아니다.
- manager session은 사용자가 명시한 기존 요소가 구현 중 사라지지 않았는지 반드시 확인해야 한다. `추가 표시` 요구를 `기존 요소 대체`로 처리했으면 즉시 실패로 되돌린다.
- manager session은 실패 원인을 재판단할 때 필요하면 `orc capture-pane <pane_id> 120`으로 각 session의 최근 로그를 회수한다.
- manager session은 render-only 검증과 state-change verified 결과를 구분해 기록해야 하며, 전자만으로는 성공 처리하지 않는다.
- manager session은 `source=fixture|mock` 인 QA 성공 메시지를 받으면 즉시 실제 실행 검증 누락으로 재분류해야 한다.
- manager session은 `job.md` 재확인까지 끝난 뒤 `orc manager-trace stage_manager_reverified "<detail>"`를 기록해야 한다.

## 개선 탐색 Session 루프
- 기본 구현/점검 루프가 끝난 뒤 manager session은 개선 탐색용 새 tmux session을 다시 연다.
- 개선 session도 시작 즉시 `job.md`를 읽고 현재 상태를 파악해야 한다.
- manager session은 개선 session에 반드시 아래 메시지를 보낸다.
  - `현재 job.md 이외에 개선할 사항을 개선해`
- 개선 session은 `job.md` 밖의 개선점까지 찾아 수정하거나 보고해야 한다.
- 개선점이 발견되면 manager session은 다시 `job.md`, 구현/QA/check 결과를 읽고 루프를 반복한다.

## 종료 조건
- `job.md`에 남은 blocking issue가 없다.
- 구현 session이 dev 서버 URL을 정상 보고했다.
- QA session이 실제 시연 검증을 성공했다.
- check session이 성공했다.
- `job.md`의 `## verify` 항목이 모두 비었다.
- 개선 탐색 session도 더 이상 개선점을 찾지 못했다.
- 그 다음에만 완료를 보고한다.

## 하드게이트
- plan 없이 ORC worker session을 열면 실패다.
- manager session에서 직접 ORC 구현/점검 명령을 실행하면 실패다.
- manager session에서 `impl_write_draft`, `check_write`를 직접 실행하면 저장소 가드가 차단해야 한다.
- manager session에서 직접 dev 서버 실행 또는 브라우저 시연을 하면 실패다.
- 구현/QA/check를 같은 tmux session 안에 섞으면 실패다.
- worker session이 시작 전에 `job.md`를 읽지 않으면 실패다.
- worker `done` 메시지만 보고 `job.md` 재확인 없이 종료하면 실패다.
- 구현 후 dev 서버 유지와 QA 시연 검증을 생략하면 실패다.
- check session을 생략하면 실패다.
- `현재 job.md 이외에 개선할 사항을 개선해` 추가 탐색을 생략하면 실패다.
- 완료 보고 전에 `orc check-manager-trace final`이 통과하지 않으면 실패다.

## 권장 조합
- ORC 세부 실행은 `orc-cli-workflow` skill 규칙을 함께 따른다.
- 코드 점검은 `check-code` skill과 함께 사용한다.
- UI/웹 기능 시연 검증은 `playwright-cli` skill 또는 동등한 브라우저 자동화 규칙과 함께 사용한다.
