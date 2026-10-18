---
name: work-project
description: Manage initialized projects through project AGENTS.md, Input.md and Table26 rows — main splits requests into rows and writes each row's goal (detail in the changes record `### 목표`, one line in $K), one t26_<id> agent per row works test-first (task-red → 작업 → 교정 → task-submit → 검증), main runs hidden cases and the whole flow (flow-wait → 확인) and completes after user confirmation (완료).
metadata:
  short-description: Run bounded work on an initialized project
allowed-tools: Bash(~/ai/scripts/bin/read-architecture:*)
---

# Work Project (`work_project` alias)

## 1. 스킬의 기본 역할

- 초기화된 `/home/tree/extensions` 프로젝트의 작업을 **작업 시작(`초안`) → 작업(`작업`·`교정`·`진단`·`검증`) → 검증 및 마감(`확인`·`완료`)** 3단계로 관리한다.
- 프로젝트 `AGENTS.md`(`## 작업 상태`), `Input.md`, `.ai/changes/YYMMDD.<제목>.md`, `.ai/docs/`, 프로젝트 Obsidian 문서(Table7 `Obsidian_Id`), Grist Table26 행이 작업 상태의 단일 원천이다.
- 스킬 로드 시 현재 프로젝트 구조가 자동 출력된다(안 나오면 직접 `~/ai/scripts/bin/read-architecture --project <dir> --query "<요청 키워드>"`를 첫 명령으로 실행한다):

!`~/ai/scripts/bin/read-architecture 2>&1 || true`

## 2. 상황별 필독 문서 안내 (반드시 해당 단계 문서를 먼저 읽고 실행)

현재 수행하려는 단계에 따라 아래 문서를 먼저 끝까지 읽고 그 순서대로 실행한다. 절차를 건너뛰거나 추측으로 대체하지 않는다.

| 현재 상황 / 단계 | 반드시 읽어야 할 문서 | 핵심 수행 내용 |
|---|---|---|
| **1단계: 작업 시작** (새 요청 접수, 프로젝트 첫 진입, 행 분리·목표 작성·에이전트 배정) | `~/ai/skills/work-project/references/start.md`<br>(미초기화면 `~/ai/skills/initialize-project/SKILL.md` 먼저 실행) | `read-architecture` 실행 → 초기화 확인 → `grist request-record` → `grist task-add` 및 `AGENTS.md` 헤더 갱신 → `grist task-goal` → `t26_<id>` 배정 |
| **2단계: 작업 및 구현** (`t26_<id>` 또는 메인의 TDD 구현, 단위 테스트, 오류·회귀 수정, 이름 변경) | `~/ai/skills/work-project/references/work.md`<br>(UI 작업은 `~/ai/skills/implement-ui/SKILL.md` 병행) | `### 목표` 확인 → `- [▷]` 및 `grist task-red`(`$H=작업`) → 구현·단위 테스트(`$H=교정`, `- [▶]`) → 오류 시 Failure loop(`$H=진단`, `$I`·`$J`) → `grist task-submit`(`$H=검증`) |
| **3단계: 검증 및 마감** (`$H=검증` 이후 숨긴 사례, 빌드·배포, CDP 전체 흐름 검증, 사용자 확인·완료) | `~/ai/skills/work-project/references/verify.md`<br>**필수 병행**: `~/ai/skills/verify-work/SKILL.md`, `~/ai/skills/end-project/SKILL.md` | `grist task-hidden` → 버전·빌드·배포 → `STAGEHAND_STEP_LOG=1` 전체 흐름 검증(백그라운드 시 `schedule(DurationSeconds=300)` 5분 병목 검사) → `grist flow-wait`(`$H=확인`) → 사용자 확인 후 `grist task-complete`(`$H=완료`) |

## 3. 작업 전반에 지켜야 하는 공통 규칙

### 구조 우선 탐색 및 로그 읽기 (`read-architecture` & `rg`)
- 파일 읽기·검색 전 첫 명령으로 `~/ai/scripts/bin/read-architecture --project <dir> --query "<요청 키워드>"`를 실행한다(다른 프로젝트로 이동 시 재실행, 모든 `t26_<id>` 에이전트도 첫 명령으로 실행).
- 출력된 후보 폴더·파일에서 `rg -n "<패턴>" <경로>`로 위치를 찾고 필요한 줄 범위만 읽는다(`ls -R`, `find`, `grep -r`, 파일 전체 `cat` 금지).
- 테스트·빌드·런타임·API 로그는 `rg -n "FAIL|Error|warn|<경계 키워드>"` 또는 `| tail -n 40`·`| head -c 4000`으로 좁혀 읽는다.
- 로그와 기록에 토큰·Authorization·원문 개인 payload·개인 전체 경로를 남기지 않는다.

### Grist 접근 (`access-grist`) 및 Table26 `$H` 단계
- 모든 Grist 읽기·쓰기(Table7, Table26, readback)는 `~/ai/scripts/bin/grist <command>`로만 수행한다(API 키 질문, curl/fetch 직접 호출, 우회 스크립트 금지; 경계 실패 시 경계명과 exit code만 보고).
- `$H`는 유일한 단계(Choice) 열이며 실제 단계가 바뀔 때만 쓴다(`$C`에 단계 기록 금지, `검증`은 `task-submit`만, `완료`는 `task-complete`만, `$K`는 `task-goal`만 기록):

| `$H` | 기록 주체 | 의미 | 턴 종료 가능 여부 |
|---|---|---|---|
| `초안` | main (`task-add`, `task-goal`) | `Input.md`에서 분리되어 목표 구체화 중 | – |
| `진단` | agent 또는 게이트 실패 시 | 오류 발생·원인 진단 및 수리 필요 (`$I`·`$J` 기록) | 불가 |
| `작업` | `grist task-red` | Red 테스트 통과(assertion 실패 확인) 후 구현 중 | 불가 |
| `교정` | agent (`task-update`) | 단위 테스트 실행·교정 중 | 불가 |
| `검증` | `grist task-submit` | 단위 테스트·범위·리터럴·Jev 통과, 전체 흐름 검증 대기 | agent 가능 / main 불가 |
| `확인` | main (`grist flow-wait`) | 전체 흐름 통과, 사용자 확인 또는 사용자 전용 동작 대기 | main 가능 |
| `완료` | main (`grist task-complete`) | 사용자 확인 후 모든 목표 줄 증거 판정 완료 | 가능 |

### 관리 기록 및 `.ai/changes/YYMMDD.<제목>.md` 작성 규칙
- `Input.md`는 현재 요청(`# 요구 사항`, `# 세부 사항`)만 담으며 새 요청마다 `grist request-record`로 교체한다.
- `.ai/changes/YYMMDD.<제목>.md`는 Asia/Seoul 하루 1파일(그날 첫 요청 10자 미만 제목 고정)이며 `# 작업`·`# 완료`·`# 진행`(`## [구현] <프로젝트·소스명 뺀 간결한 제목> (#id)` 아래 `- 유형 : ...`, `- 판정 : [값|상태] <한줄 판정 내용>`)으로 구성한다:
  - **판정 줄**: `- 판정 : [값|상태] <한줄 판정 내용>`(예: `- 판정 : [상태] 랜더링 성능 개선`)으로 한 줄로 작성하며, 고정 문구(`목표의 [값]/[상태]/[수치] 줄` 등)는 쓰지 않는다(기존에 작성된 항목은 그대로 둔다).
  - **문자표**: 리턴값은 `▷ 리턴값`(예: `계산함수(입력) ▷ True`), 전달 대상은 `⇒ 전달 대상`(예: `send(입력) ⇒ api 서버`)으로 표기하며 JSON 원문 객체(`{ key: value }`)나 명령·파일 경로는 적지 않는다.
  - **`### 목표`**: `1. [연결|기능|수정] <내용>` 번호 목록(`grist task-goal`로만 기록).
  - **`### 메모` / `### 오류` / `### 원인`**: 오류는 한 줄 요약, 원인은 `1. [유실|문자|인증] 한줄 요약 : {에러 Body}` 형식.
  - **`### 구현`**: `- [▷|▶|○|●] <구현 내역>` 목록 아래에 단계별 소제목 `#### 초안 (#id)`, `#### 검증 (#id)`, `#### 확인 (#id)`를 달고 테스트 기록 3줄(`- 목적 : <결과 라벨>`, `- 결과 : 성공|실패`, `- 상세 : <한 줄 요약>`)을 적는다.
  - **`#### 완료 (#id)`**: `결과`/`상세` 없이 5줄 내외 숫자 리스트(`1. [📦|📬|⚙️] 한줄요약`; `📦` 새 도메인·기능, `📬` 새 외부 사이트·API, `⚙️` 수정한 문제)로 작성한다.
  - `$H=완료` 시 `# 작업` → `# 완료` 이동과 Obsidian `# 📍` 추가는 `grist`가 자동 수행하므로 수동 편집하지 않는다.
