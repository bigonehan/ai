---
name: work-project
description: Manage initialized projects through project AGENTS.md, Input.md and Table26 rows — main splits requests into rows and writes each row's goal (detail in the changes record `### 목표`, one line in $K), one t26_ID agent per row works test-first (task-red → 작업 → 교정 → task-submit → 검증), main runs hidden cases and the whole flow (flow-wait → 확인) and completes after user confirmation (완료).
metadata:
  short-description: Run bounded work on an initialized project
allowed-tools: Bash(~/ai/scripts/bin/read-architecture:*)
---

# Work Project (`work_project` alias)

## 1. 스킬의 기본 역할

- 초기화된 `/home/tree/extensions` 프로젝트의 작업을 **작업 시작(`초안`) → 작업(`작업`·`교정`·`진단`·`검증`) → 검증 및 마감(`확인`·`완료`)** 3단계로 관리한다.
- 진행 중 사용자 정정은 새 요청으로 단정하지 않는다. 현재 `Input.md`의 요구와 열린 Table26 목표에 먼저 연결하고, 언급된 사례의 프로젝트와 실제 수정할 Skill·설정의 소유 프로젝트를 구분한다. 정정으로 확인된 수정 대상을 고친 뒤 결과를 보고한다.
- 사용자가 Legacy·레거시·구형 제거 또는 새 형식 갱신을 요구하면 `~/ai/skills/remove-legacy/SKILL.md`를 함께 읽고, 관련 활성 참조 제거와 수정에 참여하지 않은 에이전트의 독립 재검사까지 실행한다.
- 프로젝트 `AGENTS.md`(`# 상태` 헤더: `### grist` `ID :`, `### obsidian` `문서 :`, `### 실행` `runtime :`), `Input.md`(`구현/업무/261008.작업관리기/형식/작업관리기.형식-input.md` 형식), `.ai/changes/YYMMDD.<제목>.md`(`구현/업무/261008.작업관리기/형식/작업관리기.형식-changes.md` 형식), `.ai/docs/`, 프로젝트 Obsidian 개발 문서(Table7 `Obsidian_Id`), Grist Table26 행이 작업 상태의 단일 원천이다.
- 스킬 로드 시 현재 프로젝트 구조가 자동 출력된다(안 나오면 직접 `~/ai/scripts/bin/read-architecture --project <dir> --query "<요청 키워드>"`를 첫 명령으로 실행한다):

!`~/ai/scripts/bin/read-architecture 2>&1 || true`

## 2. 상황별 필독 문서 안내 (반드시 해당 단계 문서를 먼저 읽고 실행)

현재 수행하려는 단계에 따라 아래 문서를 먼저 읽고 그 순서대로 실행한다(프로젝트 문서·코드는 전체를 읽지 않고 필요한 섹션·줄·필드만 `rg -n` 또는 `grist task-show --section`으로 발췌해 읽고 출력한다). 절차를 건너뛰거나 추측으로 대체하지 않는다.

| 현재 상황 / 단계 | 반드시 읽어야 할 문서 | 핵심 수행 내용 |
|---|---|---|
| **1단계: 작업 시작** (새 요청 접수, 프로젝트 첫 진입, 행 분리·목표 작성·에이전트 배정) | `~/ai/skills/work-project/references/start.md`<br>(미초기화면 `~/ai/skills/initialize-project/SKILL.md` 먼저 실행) | `read-architecture` 실행 → `grist gate`로 상태 확인(`AGENTS.md`·`Input.md`·`changes`·Obsidian 문서 전체 읽기 금지, 필요한 섹션·줄·필드만 `rg -n` 또는 `grist task-show --section`으로 발췌) → `grist request-record` → `grist task-add` → `grist task-goal` → `t26_<id>` 서브에이전트 배정(메인 직접 구현 금지) |
| **2단계: 작업 및 구현** (`t26_<id>` 서브에이전트의 TDD 구현, 단위 테스트, 오류·회귀 수정, 이름 변경) | `~/ai/skills/work-project/references/work.md`<br>(UI 작업은 `~/ai/skills/implement-ui/SKILL.md` 병행) | `grist task-show <id> --project <dir> --section 목표`로 필요한 섹션만 발췌 확인 → `- [▷]` 및 `grist task-red`(`$H=작업`) → 구현·단위 테스트(`$H=교정`, `- [▶]`) → 오류 시 Failure loop(`$H=진단`, `$I`·`$J`) → `grist task-submit`(`$H=검증`) |
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

작업 중 Table26 `$D`는 현재 작업을 번호 목록으로 기록한다. `확인` 전환 시에는 작업 유형에 맞는 완료 기호를 골라 행별 `#### 확인 (#id)`에 `- 요약 : [표시 기호] <입력 또는 이전 단계> ⇒ <구현 결과>`를 쓰고, 그 요약을 번호 없이 `$D` 최신 한 줄로 기록한다. 오류 수정 요구는 아래 여섯 오류 기호 중 원인에 맞는 하나를 고른다.

### 관리 기록 및 `.ai/changes/YYMMDD.<제목>.md` 작성 규칙
- `Input.md`는 현재 요청(`# 요구 사항`의 한 줄 요약, `# 세부 사항`의 `[표시 기호] 한 줄 요약` 목록)만 담으며 새 요청마다 `grist request-record`로 교체한다. 세부 기호는 `📐` 스키마·함수 타입/변환, `✒️` 문서·템플릿 형식, `⌨️` 설정 옵션, `🚿` 필터, `✏️` 계산식, `🌍` API·DB 연동이다. 각 세부 사항은 기호를 포함한 그대로 Table26 `$B`와 changes 작업 제목에 전달한다.
- `.ai/changes/YYMMDD.<제목>.md`는 Asia/Seoul 하루 1파일(그날 첫 요청 10자 미만 제목 고정)이며 `# 작업`·`# 완료`·`# 진행`(`## [구현] <Input.md 세부 사항과 동일한 기호·문구> (#id)` 아래 `- 유형 : ...`, `- 판정 : [값|상태] <한줄 판정 내용>`)으로 구성한다. 제목은 세부 사항을 쓸 때 한 번 요약하고 전달 중에는 다시 줄이거나 바꾸지 않는다:
  - **판정 줄**: `- 판정 : [값|상태] <한줄 판정 내용>`(예: `- 판정 : [상태] 랜더링 성능 개선`)으로 한 줄로 작성한다.
  - **문자표**: 리턴값은 `▷ 리턴값`(예: `계산함수(입력) ▷ True`), 전달 대상은 `⇒ 전달 대상`(예: `send(입력) ⇒ api 서버`)으로 표기하며 JSON 원문 객체(`{ key: value }`)나 명령·파일 경로는 적지 않는다.
  - **`### 목표`**: `1. [연결|기능|수정] <내용>` 번호 목록(`grist task-goal`로만 기록).
  - **`### 메모` / `### 오류` / `### 원인`**: 오류는 한 줄 요약, 원인은 `1. [유실|문자|인증] 한줄 요약 : {에러 Body}` 형식.
  - **`### 구현`**: `- [▷|▶|○|●] <구현 내역>` 목록 아래에 단계별 소제목 `#### 초안 (#id)`, `#### 검증 (#id)`, `#### 확인 (#id)`를 달고 테스트 기록 3줄(`- 목적 : <결과 라벨>`, `- 결과 : 성공|실패`, `- 상세 : <한 줄 요약>`)을 적는다.
  - **`#### 완료 (#id)`**: `결과`/`상세` 없이 5줄 내외 숫자 리스트(`1. [표시 기호] 한 줄 요약`)로 작성한다. 일반 작업은 `🏗️` 구조 변경, `✨` 기능, `🌍` 외부 연결, `🚀` 속도·흐름 개선을 고른다. 사용자 요구가 오류 수정이면 `🅰️` 타입 불일치, `❌` 권한 중단, `⭕️` 결과 파일·변수·값 부재, `🔢` 계산식 오류, `🚥` 병목, `🔳` 빈 화면 중 원인에 맞는 하나를 쓴다.
  - `$H=완료` 시 `# 작업` → `# 완료` 이동과 Obsidian `# 📍` 추가는 `grist`가 자동 수행하므로 수동 편집하지 않는다.
