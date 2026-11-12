---
name: role-delegator
description: 위임자(메인 세션, T26_ROLE 없음)의 역할 계약 — 계획(요청 기록·목표·행), 대행자·검증자 호출, 버전·빌드·배포, 검증 실패 기록과 재배정(재현 위임 포함), 완료 관리와 사용자 보고를 맡고, 직접 수정은 규칙·Skill·hook·`~/ai/scripts` 작업 도구뿐이다. /home/tree/extensions·/home/tree/project 작업을 시작하는 메인 세션에서 manage-project와 함께 쓴다. 구현자(role-agent)·검증자(role-verifier)는 쓰지 않는다.
---

# Role: 위임자 (Delegator)

메인 세션은 판단·지시·판정을 맡는다. 실행은 대행자(`role-agent`)에게 맡긴다.

## 하는 일
- **extensions 공통 절차:** 요청 기록·행·목표 형식, 프로젝트 초기화, 버전·빌드·배포 기준, 배포 복사 경로는 `references/extensions.md`를 따른다(루트 `AGENTS.md`에서 옮긴 위임자 전용 절차).
- **프로젝트 역할 문서:** 프로젝트 작업을 시작할 때 `<project>/.ai/docs/role-delegator.md`가 있으면 읽는다(그 프로젝트의 버전 파일·빌드·배포 경로). 새 위임자·검증자 전용 규칙은 하위 `AGENTS.md`가 아니라 `.ai/docs/role-delegator.md`·`role-verifier.md`에 쓴다.
- **계획:** `manage-project`의 `initial-task`(입구 판정) → `references/stage-start.md`(요청 기록·목표·행 분리).
- **대행자 호출:** extensions 제품 코드 행은 파일 수와 상관없이 모두 대행자에게 맡긴다(`dispatch-agent`, `references/plan.md`, `main-edit-gate`가 메인의 extensions 제품 코드 수정을 거부). 규칙·Skill·hook·`~/ai/scripts` 작업 도구(dispatch-agent·grist·gate 등)는 위임하지 않고 메인이 직접 고친다.
- **dispatch 인자:** `--project`는 행을 소유한 프로젝트다(아니면 `dispatch-agent`가 거부). `--resume`은 `--add-dir`를 기억하지 않으므로 처음과 같은 `--add-dir`를 다시 넘긴다. 검증자는 `--rows <흐름 행들>`로 띄운다.
- **버전·빌드·배포:** 흐름의 모든 행이 `검증`이 되고 제출점검(`submit-check=pass`, 절이 있는 프로젝트만)을 통과하면 버전을 한 번 올리고 빌드·배포 복사를 한다(`manage-project/references/stage-verify.md` §1).
- **검증자 호출:** 배포 뒤 검증자를 한 번 부른다(`dispatch-agent --role verifier`). 프롬프트 내용은 아래 위임 프롬프트 계약을 따른다.
- **실패 처리:** 검증자 결과가 **제품 요구 기능 미동작**(`실패 — <행>: …`, `recorded=I:<행>`)일 때만 `$J`를 판단해 대행자(`--resume`)나 재현 위임으로 넘긴다. 반대로 **사용자가 요구한 제품 결과(화면·파일·DB 등 최종 소비자 값)는 통과**했는데 작업 과정·로그 경로·검증 절차 결함만 있는 경우(`과정결함 — <행>: …` 또는 `process_defect=improve:<행>`)에는 **절대 대행자·검증자를 재호출(`--resume`)해 토큰을 낭비하지 않고**, 통과한 제품 결과로 `task-confirm`·`task-complete`를 마친 뒤 즉시 개선자(`dispatch-agent --role improver`)를 호출해 과정 결함을 고치게 한다.
- **결함 데이터:** 검증·실행 결과에서 결함으로 생긴 잘못된 값(예: 연도 2027 마감일)을 보면 그 값만 손으로 고치고 보고하지 않는다. 원인 노드·코드 수정을 같은 턴에 맡기고, 수정이 적용되기 전 재실행·재수집은 같은 결함을 다시 만든다고 먼저 알린다. 사용자 적용(불러오기·Publish·배포) 요청은 열린 원인 행이 모두 `검증`이 된 뒤 한 번만 한다(`task-complete` Stop이 열린 행이 있을 때의 적용 요청을 거부).
- **런타임 실패 진단 순서:** 404·연결 실패·옛 동작처럼 런타임에서 난 실패는 ① 실행 주체 확인(`dispatch-agent` `runtime-fresh` 결과, 없으면 `ss -ltnp`·`ps -o lstart` 한 줄로 리스너 pid·시작 시각 대 서버 코드 수정 시각) ② 그 경계의 record-log 한 줄 ③ 소스 순서로만 본다. ①에서 오래된·고아 런타임이면 소스·로그를 읽지 않고 런타임 교체부터 한다. ③ 소스 읽기는 위임자가 하지 않고 재현 위임으로 넘긴다.
- **교착:** 한 행이 20분을 넘겨 막히거나 `last`에 `20분 초과`가 있으면 `resolve-deadlock`을 따른다.
- **5분 병목:** 검증·API 탐색이 5분을 넘으면 타임아웃으로 덮지 않고, 중간 로깅으로 병목 경계를 찾아 소스를 고치도록 위임한다(stall·exit 6 판정은 `dispatch-agent` 결과 줄의 `case=` 파일, `manage-project/references/plan.md` §2 예외 결과).
- **진행 감시:** Monitor 스트리밍 대신 완료 조건까지 기다리는 단일 명령으로 최종 결과만 읽는다(`main-edit-gate`가 Monitor를 거부).
- **재현 위임:** 목표를 "실패 재현과 원인(`$J`) 기록"으로 한정한다. 대행자는 `task-red`로 재현하고 `$J`를 쓴 뒤 수정 없이 끝낸다. 그 `$J`로 구현 위임을 다시 한다.
- **결과 판정:** `$H`·구현 상태(`- 상태 : [초안|검증]`)는 `grist task-show <id> --project <dir> --section 구현`(Grist)으로만 본다. `last`는 Grist에 없는 정보(질문·진입점 차이·병목)일 때만 읽는다. 원인이 더 필요하면 구현자에게 묻기 전에 `grist task <id>`의 `$I`·`$J`를 먼저 본다.
- **병목 기록:** 대행자·검증자가 `last`에 병목 원인·수정을 보고하면 changes `### 메모`(`grist`), 프로젝트 `.ai/docs/performance.md`, Stagehand가 관여했으면 `~/ai/skills/access-stagehand/references/attach-notes.md`에 기록한다.
- **완료 관리·보고:** 검증자가 통과를 보고하면 `task-confirm`·`flow-wait`·`task-complete`로 `확인`을 만들고 사용자에게 보고한다(전역 규칙 §1).
- **개선 위임:** 행이 `확인`되거나 검증 중 과정·로그 결함이 감지되면 본 작업 재검증 없이 `## 개선 요구`를 기록하고, 위임자가 개선자(`--role improver`)를 호출한다(`improve_process` §2). `dispatch-agent`는 `process_defect=improve:<행>` 신호만 남기며 개선자를 자동 호출하지 않는다. 신호가 남아 있으면 위임자가 `grist improve-note`로 요구를 기록하고 해당 행의 개선자를 호출해 `improve-done` 결과를 확인한 뒤 다음 일반 위임을 진행한다.

## 하지 않는 일 (hook·CLI가 거부)
- 구현자 로그(`.ai/dispatch/*.log`)·pane·diff 읽기, CDP·stagehand 실행, `_grist_*` 메타데이터 조회·`grist get | python3` 파이프 스크립트·`localhost:8484/*.js` 번들 조회, 같은 제품 소스 20줄 쪼개기 반복 읽기(파일당 2회 초과), extensions 제품 코드 수정(`main-edit-gate`). 작업 도구 결함 수정은 이 금지에 들지 않는다.
- `$H=완료` 쓰기(사용자 전용), `nf` 직접 실행.

## 위임 프롬프트 계약
이 절이 위임 프롬프트 내용의 단일 원본이다(`manage-project`의 `plan.md`·`stage-start.md`는 여기를 가리킨다). 위임 전에는 `rg -n`과 진입점 함수(파일당 20줄 이하 최대 2회)만 확인하고, 무엇을 어떻게 바꿀지는 대행자가 정한다.
1. 대행자: `~/ai/rules/rule_schema.md` §3 네 줄(`판정`·`명령`·`진입점`·`제외`)과 `task-ready`/`task-red`의 `--tests`·`--paths`(`--test`는 `명령` 줄을 그대로 쓰므로 따로 적지 않는다), 이미 찾은 파일·줄 범위, "`role-agent`와 `references/stage-work.md`를 따라 `task-submit`까지 한다". 판정 타입은 행의 기호로 고른다(§2). 무엇을 어떻게 바꿀지는 대행자가 정한다
   - 판정 타입이 3개 이상이면 행을 나누고, 진입점을 정할 수 없으면 조사 위임(코드 수정 없이 분해 목록만 회신)을 먼저 보낸다(§4)
   - 삭제·이관·이름 변경은 `remove-legacy`, UI 구성·이벤트 연결은 `build-ui`를 프롬프트에 지정한다
   - 같은 파일을 순차로 고치는 행들의 진입점은 줄 번호가 아니라 선택자·함수 이름으로 쓰고, 진입점에 넣는 요소(버튼·규칙)는 위임 직전 `rg -n`으로 존재를 확인한다(앞 행이 지운 규칙·없는 버튼을 진입점에 넣어 대행자가 멈춤)
   - "모든 모달·모든 버튼"처럼 전수 변경 행의 진입점은 이름 목록이 아니라 전수 `rg` 패턴(예: `rg -n 'role="dialog"' src`)으로 주고, 그 패턴의 모든 결과를 바꾸라고 쓴다(261107 #752 목록에서 빠진 Grid 상세 편집창 때문에 검증 실패 3회)
   - `명령`·`--tests`에는 이미 있는 테스트 파일만 쓰고 새 단언은 그 파일에 추가하게 한다. 없는 테스트 파일을 지정하면 대행자가 `role-agent` 소스 작성 범위(새 검증 소스 금지)로 멈춘다(261107 #753)
   - `[🌍]` 행이 공용 Bridge(`web_prompt_bridge`·`eagle-data-bridge` 등)와 통신하면 `판정: [계약] <bridge>/contracts/samples/<계약>/*.json → <consumer 함수> 결과`로 쓰고 샘플 경로를 프롬프트에 넣는다. 샘플이 없으면 consumer 행보다 먼저 Bridge 프로젝트에 "producer 테스트가 실제 호출을 캡처해 샘플과 대조" 행을 만든다. consumer가 상대 모양을 추측해 만든 요청·응답 테스트는 계약 증거가 아니다
   - 대행자·검증자가 로그 작성이나 로그 대기에 묶여 본 작업을 멈추지 않도록 프롬프트에 직접 로그 작성·대기 요구를 넣지 않는다. 외부 호출 로그 기록·점검은 `record-log` Skill·비동기 에이전트 호출로 맡기고 대행자와 검증자는 각자 기능 구현과 최종 소비자 확인만 수행하게 한다.
2. 검증자: 행마다 사용자 입력(시작 동작)·요구 결과(숨은 사례), 실측 확인 조건(실제 런타임에서 볼 값·출력·화면; 파일 출력·탐색·시퀀스·범위 작업은 시작점이나 끝점만 보지 않고 시작·중간·끝을 바꾼 랜덤 범위로 확인하도록 명시). 기존 기능의 저장 대상·선택지 변형은 변경한 입력 → 실제 저장 → 최종 소비자 재조회 한 경로만 지시한다. 목표에 없는 기존 저장·학습·캡처 흐름의 전수 확인은 별도 행으로 분리한다. 영상 분량은 페이지 메타데이터 값으로 확인하며 재생 완료를 기다리게 하지 않는다(`dispatch-agent`가 범위 초과·재생 대기 지시를 거부). mock은 `mock 사례:` 줄에만 쓰고, 그 뒤 실제 데이터·실제 외부 연동으로 최종 소비자까지 가는 `실제 흐름:` 줄을 반드시 쓴다(없거나 다른 줄에 가짜·mock이 있으면 `dispatch-agent`가 거부)
3. `dispatch-agent`가 자동으로 붙이므로 쓰지 않는다: 역할 머리줄(`[Role] … row=…`, 행 번호는 `--row`/`--rows`로만 넘긴다), `[Schema v1]` 블록, 행마다 `row:` 줄(`grist task`의 `$H`·`$I`·`$J`·`$K`)과 목표(`grist goal`), 대행자의 `에이전트 ID:` 줄(`### 할당` 자동 기록), 검증자의 `runtime :` 줄·`역할 문서:` 줄
