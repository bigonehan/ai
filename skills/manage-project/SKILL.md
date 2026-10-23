---
name: manage-project
description: Single entry for /home/tree/extensions projects — initializes an untracked project (references/initialize.md), then runs requests through the changes `# 요청` section and Table26 rows. The orchestrator (main session) records requests, writes goals, delegates rows (t26_<id> subagent or dispatch-agent worker) and closes them against the request; a worker (T26_ROLE=worker) or t26_<id> agent only implements, tests and submits its own row. Not for explanation-only questions that change no file or Grist record.
metadata:
  short-description: Initialize, plan, delegate and close project work
allowed-tools: Bash(~/ai/scripts/bin/read-architecture:*)
---

# Manage Project

## 0. 역할과 입구 판정 (가장 먼저)

- **역할:** 환경에 `T26_ROLE=worker`가 있거나 `t26_<id>` 에이전트로 실행됐으면 **구현자**다. 구현자는 `references/work.md`만 따르고 자기 행(`T26_ROW` 또는 `<id>`)만 다룬다. 다른 에이전트를 부르거나 요청·목표를 쓰거나(`request-record`·`task-add`·`task-goal`), `task-hidden`·`flow-wait`·`task-complete --row`를 실행하지 않는다(CLI가 거부한다). 그 밖의 메인 세션은 **오케스트레이터**이며 아래 1~3절을 따른다.
- **입구:** 소유 프로젝트 `AGENTS.md`에 `# 상태`(`### grist` `ID :`·`### obsidian` `문서 :`) 헤더가 없으면 미초기화다. 그때만 `references/initialize.md` 절차로 초기화한 뒤 1단계로 이어간다. 초기화된 프로젝트에서는 그 문서를 읽지 않는다.
- **위임:** 오케스트레이터가 행을 다른 에이전트에 맡길 때는 `references/plan.md`(계획·`dispatch-agent` 실행·요청 대조 마감)를 따른다.

## 1. 스킬의 기본 역할

- 초기화된 `/home/tree/extensions` 프로젝트의 작업을 **작업 시작(`초안`) → 작업(`작업`·`교정`·`진단`·`검증`) → 검증 및 마감(`확인`·`완료`)** 3단계로 관리한다.
- 사용자 메시지에서 **수정하라고 지목한 대상**을 사례·증상보다 먼저 판정한다. Skill·설계/검증 절차·에이전트의 판단/토큰 낭비를 고치라는 요청은 제품 사례가 나와도 작업관리기 루트 소유다. 제품 동작 수정이 따로 요청되지 않았다면 사례 제품의 소스·테스트·버전·설치·실행을 건드리지 않는다. 잘못 만든 제품 작업은 취소·원복하고 원래 제품 작업으로 재개하지 않는다.
- 설명·의견·현황 조회처럼 파일·Grist를 바꾸지 않고 답할 수 있는 질문은 `?` 유무와 상관없이 이 Skill의 기록 절차(`request-record`·`task-add`·`task-goal`·행 조회)를 하지 않고 필요한 줄만 `rg`로 읽어 답한다. "고쳐줄래?"·"~로 바꾸면?"처럼 수정을 요구하거나 답하다가 수정이 필요해지면, 또는 사용자가 진행을 지시하면 그때 1단계부터 시작한다. 행 없이 파일을 바꾸면 메인 Stop hook이 한 번 막는다.
- 진행 중 사용자 정정은 새 요청으로 단정하지 않는다. 소유 대상을 먼저 판정한 뒤 해당 프로젝트의 최신 요청(`~/ai/scripts/bin/grist request --project <dir>` 출력)과 열린 Table26 목표에 연결한다. 정정으로 확인된 수정 대상을 고친 뒤 결과를 보고한다.
- 행의 `$H`·`$I`·`$J`·`$K`는 작업 시작에 한 번 읽는다. 기존 행은 `grist task <id>`(ID를 모르면 `grist gate`로 찾은 직후), 새로 만들거나 기록한 행은 `task-add`·`task-goal`·`task-update` 출력의 `row` 줄(`H=… | I=… | J=… | K=…`)이 그 조회다. 같은 값을 다시 조회하지 않고, 담당 에이전트에는 메인이 받은 `row` 줄을 프롬프트로 넘긴다.
- 오류를 지적받으면 `$I`에 기록하고, 사용자가 알려준 원인 또는 재현해 찾은 원인을 `$J`에 기록한다. `$J`를 쓴 `task-update`의 `row` 줄에 있는 원인 경계를 수정 목표로 삼는다. 설치·버전·실행 상태는 수정 전 같은 사실을 확인하려고 반복 조회하지 않고, 수정 뒤 최종 소비 결과 검증에서 대조한다.
- 사용자가 Legacy·레거시·구형 제거 또는 새 형식 갱신을 요구하면 `~/ai/skills/remove-legacy/SKILL.md`를 함께 읽고, 관련 활성 참조 제거와 `check-residue` 잔존 검사(exit 0)까지 실행한다.
- 프로젝트 `AGENTS.md`(`# 상태` 헤더: `### grist` `ID :`, `### obsidian` `문서 :`, `### 실행` `runtime :`), `.ai/changes/YYMMDD.<제목>.md`(`구현/업무/261008.작업관리기/형식/작업관리기.형식-changes.md` 형식), `.ai/docs/`, 프로젝트 Obsidian 개발 문서(Table7 `Obsidian_Id`), Grist Table26 행이 작업 상태의 단일 원천이다.
- 현재 폴더를 작업 소유자로 간주해 구조를 자동 출력하지 않는다. 사용자 메시지로 소유 프로젝트를 판정한 다음 그 프로젝트의 첫 명령으로 `~/ai/scripts/bin/read-architecture --project <소유 프로젝트> --query "<요청 키워드>"`를 실행한다.

## 2. 단계 카드 (카드대로 실행하고, 참고 문서는 카드에 없는 경우에만 해당 절을 발췌)

카드의 명령 순서가 각 단계의 절차다. 미초기화 프로젝트, 정정·오분류, 에이전트 배정, 실패 루프 세부, 이름 변경, 버전·배포·CDP처럼 카드에 없는 상황에서만 오른쪽 문서의 해당 절을 `rg -n "<절 제목>" <문서>`로 찾아 그 범위만 읽는다. 이미 읽은 문서는 같은 작업에서 다시 읽지 않는다.

| 단계 | 카드 (명령 순서) | 예외 시 문서 |
|---|---|---|
| **1. 시작** (main) | 소유 프로젝트 판정 → `read-architecture --project <dir> --query "<키워드>"` → 기존 행 `grist task <id>` → `grist request-record --project <dir> --title "<10자 미만>" --summary "<한 줄>" --input-file /dev/stdin` → 세부 사항마다 `inquire-task`로 목표를 한 줄 = 관측값 하나로 분해 → `grist task-add --project <dir> --type "<유형>" --judge "[값\|상태] <판정>" --fields '{"A": <t7>, "B": "<기호 요약>", "H": "초안"}' --goal-file /dev/stdin --summary '<한 줄>'`(목표까지 함께 기록, 출력 `row` 줄 = 행 조회) | `references/start.md` (미초기화면 `references/initialize.md`) |
| **2. 작업** (수정 파일 1~2개 행은 main, 3개 이상 묶음은 `t26_<id>` 또는 `dispatch-agent` 구현자) | 원인 확정: `grist task-ready <id> --project <dir> --purpose "<라벨>" --test "<명령>" --tests <파일> --paths <최상위 경로>` / 미확정: 같은 인자로 `task-red` → 목표 줄 N마다 수정 → `grist task-step <id> --project <dir> --step "▶" --text "<내역>" --goal N`(`$K`=`[N/M 단계] 목표 N줄`) → `grist task-submit <id> --project <dir>`(`$H=검증`) | `references/work.md` (UI는 `implement-ui`) |
| **3. 검증·마감** (main) | `grist task-hidden` → 버전·빌드·배포(제품 변경 시) → 최종 소비 결과 확인 → `grist task-confirm`(`○`·`●`·`#### 확인` 한 번에) → `grist flow-wait`(문서·규칙 행은 생략) → `task-complete --row <id> --project <dir>`(`$H=확인`까지) | `references/verify.md`, 테스트는 `verify-work` |

2·3단계 경량 경로: `task-submit` 기록의 `submit.changed`가 1~2인 행은 `task-hidden`과 `flow-wait`를 생략하고, 버전·배포가 필요한 제품 변경은 그대로 처리한 뒤 최종 소비 결과를 `task-confirm`에 기록하고 `task-complete`를 실행한다. 변경 파일이 0개·3개 이상이거나 수치가 없으면 일반 경로를 따른다.

사용자의 "완료" 답변은 이미 아는 행·기록에 연결하고 새 구조·명령 탐색이나 자동 `$H=완료` 전환을 하지 않는다. 문서는 전체를 읽지 않고 필요한 섹션·줄·필드만 `rg -n`으로 발췌한다. changes는 열지 않고 스크립트로만 읽는다: 최신 요청은 `grist request`, 행 목표는 `grist goal`, 행의 절(`구현`·`메모`·`오류`·`원인`)과 단계 기록(`초안`·`검증`·`확인`·`완료`)은 `grist task-show <id> --project <dir> --section <이름>`, 열린 행의 단계는 `grist gate`, `# 작업`·`# 완료` 목록은 `grist lists --project <dir>`.

## 3. 작업 전반에 지켜야 하는 공통 규칙

### 구조 우선 탐색 및 로그 읽기 (`read-architecture` & `rg`)
- 새 작업에서 프로젝트 파일을 처음 읽거나 검색하기 전 `~/ai/scripts/bin/read-architecture --project <dir> --query "<요청 키워드>"`를 실행한다(다른 프로젝트를 새 작업 대상으로 처음 열 때도 실행, 모든 `t26_<id>` 에이전트도 첫 명령으로 실행). 이미 구조·행 ID·기록 경로를 확인한 작업의 사용자 완료 확인은 새 작업이 아니므로 같은 조회를 반복하지 않는다.
- 출력된 후보 폴더·파일에서 `rg -n "<패턴>" <경로>`로 위치를 찾고 필요한 줄 범위만 읽는다(`ls -R`, `find`, `grep -r`, 파일 전체 `cat` 금지).
- 테스트·빌드·런타임·API 로그는 `rg -n "FAIL|Error|warn|<경계 키워드>"` 또는 `| tail -n 40`·`| head -c 4000`으로 좁혀 읽는다.
- 로그와 기록에 토큰·Authorization·원문 개인 payload·개인 전체 경로를 남기지 않는다.

### Grist 접근 (`access-grist`) 및 Table26 `$H` 단계
- 모든 Grist 읽기·쓰기(Table7, Table26, readback)는 `~/ai/scripts/bin/grist <command>`로만 수행한다(API 키 질문, curl/fetch 직접 호출, 우회 스크립트 금지; 경계 실패 시 경계명과 exit code만 보고).
- `$H`는 유일한 단계(Choice) 열이며 실제 단계가 바뀔 때만 쓴다(`$C`에 단계 기록 금지, `검증`은 `task-submit`만, 자동 `확인`의 최종 판정은 `Complete_task`만, `$K`는 `task-add`·`task-goal`만 기록). 자동화는 `$H=완료`를 쓰지 않는다:

| `$H` | 기록 주체 | 의미 | 턴 종료 가능 여부 |
|---|---|---|---|
| `초안` | main (`task-add`, 정정 시 `task-goal`) | changes `# 요청`에서 분리되어 목표 구체화 중 | – |
| `진단` | agent 또는 게이트 실패 시 | 사용자가 지적한 오류는 `$I`, 알려준 원인 또는 재현해 찾은 원인은 `$J`에 기록하고 그 `row` 줄의 원인을 수정 | 불가 |
| `작업` | `grist task-red` 또는 `grist task-ready` | 원인 미확정 시 실패 경계 조사, 원인 확정 시 지정된 소유 경계 수정 중 | 불가 |
| `교정` | agent (`task-update`) | 단위 테스트 실행·교정 중 | 불가 |
| `검증` | `grist task-submit` | 단위 테스트·수정 범위 통과, 전체 흐름과 `Complete_task` 목표 판정 대기 | agent 가능 / main 불가 |
| `확인` | `Complete_task` (`task-complete`) | 전체 흐름 또는 Skill·규칙 확인 증거와 목표의 Jev 판정 통과, 사용자 확인 대기 | main 가능 |
| `완료` | 사용자 (Grist에서 직접 수정) | 사용자가 최종 결과를 검토·수정함 | 가능 |

전역 규칙 원문 또는 Skill 절차 텍스트(`SKILL.md`·`references/*.md`)만 바꾸는 행은 코드 제출 흐름의 예외다. 메인이 전역 규칙은 `agent-rules`로 수정하고 원본·생성본·`agent-rules status`를 확인한다. 두 종류 모두 수정 후 다른 하위 프로젝트 `AGENTS.md`의 명시적 지시가 새 기준과 충돌하는지만 검사한다. 충돌이 없으면 `task-confirm`으로 실제 결과를 기록하고 `task-complete --row <id> --project <dir>`의 Jev 판정을 거쳐 `$H=확인`으로 전환한다. 충돌이 있으면 해소한 뒤 전환한다. 이 행에는 Red, `task-submit`, `task-hidden`, `flow-wait`를 요구하지 않는다. CLI·hook·제품 코드·Skill 스크립트 변경에는 일반 제출 흐름을 적용한다.

Table26 `$D`(메모)는 작업 중 비워 두고, 오류를 발견해 `진단`에 들어갈 때만 `[오류 기호] <점검할 내용 한 줄>`을 쓴다. `확인` 전환 시에는 작업 유형에 맞는 완료 기호를 골라 행별 `#### 확인 (#id)`에 `- 요약 : [표시 기호] <입력 또는 이전 단계> ⇒ <구현 결과>`를 쓰고, 그 요약을 번호 없이 `$D` 최신 한 줄로 기록한다. 오류 수정 요구는 아래 `### 원인`의 오류 기호 10개 중 원인에 맞는 하나를 고른다.

### 관리 기록 및 `.ai/changes/YYMMDD.<제목>.md` 작성 규칙
- 요청 원본은 changes 첫 절 `# 요청`이다. `grist request-record`가 새 요청마다 `## <요구 사항 한 줄 요약>`과 `- [표시 기호] <세부 사항 한 줄 요약>` 목록을 아래에 누적하고, J3와 `grist request`는 최신 블록을 읽는다. 정정은 기존 블록을 고치지 않고 `request-record`로 새 블록을 추가한다. 세부 기호는 `📐` 스키마·함수 타입/변환, `✒️` 문서·템플릿 형식, `⌨️` 설정 옵션, `🚿` 필터, `✏️` 계산식, `🌍` API·DB 연동이다. 각 세부 사항은 기호를 포함한 그대로 Table26 `$B`와 changes 작업 제목에 전달한다.
- `.ai/changes/YYMMDD.<제목>.md`는 Asia/Seoul 하루 1파일(그날 첫 요청 10자 미만 제목 고정)이며 `# 요청`·`# 작업`·`# 완료`·`# 진행`(`## [구현] <# 요청 세부 사항과 동일한 기호·문구> (#id)` 아래 `- 유형 : ...`, `- 판정 : [값|상태] <한줄 판정 내용>`)으로 구성한다. 제목은 세부 사항을 쓸 때 한 번 요약하고 전달 중에는 다시 줄이거나 바꾸지 않는다:
  - **판정 줄**: `- 판정 : [값|상태] <한줄 판정 내용>`(예: `- 판정 : [상태] 랜더링 성능 개선`)으로 한 줄로 작성한다.
  - **문자표**: 리턴값은 `▷ 리턴값`(예: `계산함수(입력) ▷ True`), 전달 대상은 `⇒ 전달 대상`(예: `send(입력) ⇒ api 서버`)으로 표기하며 JSON 원문 객체(`{ key: value }`)나 명령·파일 경로는 적지 않는다.
  - **`### 목표`**: `1. [연결|기능|수정] <내용>` 번호 목록(`연결`: 인증·API·외부 사이트·DB 연동, `기능`: 새 기능, `수정`: 발견한 오류·문제 수정). 쓰기는 `task-add --goal-file`(새 행)·`task-goal`(초안 정정), 읽기는 `grist goal <id> --project <dir>`로만 하고 changes를 열지 않는다.
  - **`### 메모` / `### 오류` / `### 원인`**: 오류는 한 줄 요약, 원인은 `1. [오류 기호] 한줄 요약 : {에러 Body}` 형식이며 `$J`도 같은 `[오류 기호] 한 줄`이다(재현에 성공했거나 사용자가 원인을 지목했을 때 `진단` 진입 시 기록). 오류 기호(단일 원본): `🅰️` 내부 스키마·입출력 Type 불일치, `🗝` 외부 계약 불일치(사이트 구조·API 응답·CLI 형식 변경), `❌` 권한·인증·연결 끊김·비정상 종료로 작업 중단, `⭕️` 결과 파일·변수·값 없음, `🔢` 계산식이 틀림, `🛣` 조건 분기·처리 경로가 틀려 기대와 다른 값, `👥` 특정 함수 중복 실행, `🏷` 버전 불일치·개발/배포 환경 불일치, `🚥` 병목 지점 발생, `🔳` 화면 표시 단계 문제(빈 화면·잘못된 값 표시·UI 겹침). 기호는 결과가 아니라 원인으로 고른다: 함수가 원인이면 화면 문제여도 `🛣`·`🔢`, 우리 코드 안의 경계는 `🅰️`·바꿀 수 없는 외부 쪽 변경은 `🗝`, 연결 자체가 막히면 `❌`.
  - **`### 구현`**: `- [▷|▶|○|●] <구현 내역>` 목록(`▷`: red test용 실패 초안 구현, `▶`: 구현과 Mock·단위 함수 검증, `○`: 전체 흐름 테스트 정상 작동, `●`: 검증 통과 후 사용자 검토 대기; `▷`·`▶`는 `task-step`, `○`·`●`는 `task-confirm`이 기록) 아래에 단계별 소제목 `#### 초안 (#id)`, `#### 검증 (#id)`, `#### 확인 (#id)`를 달고 테스트 기록 3줄(`- 목적 : <결과 라벨>`, `- 결과 : 성공|실패`, `- 상세 : <한 줄 요약>`)을 적는다.
  - **`#### 완료 (#id)`**: `결과`/`상세` 없이 5줄 내외 숫자 리스트(`1. [표시 기호] 한 줄 요약`)로 작성한다. 일반 작업은 `🏗️` 구조 변경, `✨` 기능, `🌍` 외부 연결, `🚀` 속도·흐름 개선을 고른다. 사용자 요구가 오류 수정이면 `### 원인`의 오류 기호 10개 중 원인에 맞는 하나를 쓴다.
  - `Complete_task`가 목표·출력을 확인해 통과시키면 `# 작업` → `# 완료` 이동을 중복 없이 처리하고 `$H=확인`까지만 기록한다. 사용자가 Grist에서 `$H=완료`로 수정하면 동기화는 목록을 중복 이동하지 않고 Obsidian `# 📍`를 반영한다.
