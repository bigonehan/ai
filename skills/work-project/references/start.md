# 1단계: 작업 시작 (`work-project` — Start)

새 사용자 요청을 받았거나, 프로젝트를 처음 열었거나, 새 Table26 작업 행·목표를 만들고 에이전트를 배정할 때 이 문서를 끝까지 읽고 순서대로 실행한다.

## 1. 구조 파악 (`read-architecture`) 및 문서 확인

1. 파일 읽기·검색 전 첫 명령으로 `~/ai/scripts/bin/read-architecture --project <dir> --query "<요청 키워드>"`를 실행한다(다른 프로젝트로 옮기면 다시 실행).
   - 출력에서 작업 대상 폴더와 후보 파일을 고른 뒤 `rg -n "<패턴>" <파일>`로 위치를 찾고 필요한 범위만 읽는다(`ls -R`, `find`, `grep -r`, 전체 `cat` 금지).
   - `missing` 또는 빈 템플릿(exit 2)이면 실제 프로젝트 기준으로 `.ai/docs/architecture.md`(`# 도메인`은 `항목 : 설명`, `# 구조`는 루트·2단계 폴더, 파일명 제외)부터 작성한 뒤 다시 실행한다.
   - 작업 중 폴더가 추가·삭제·이동되거나 구조 차이가 작업 범위 안이면 같은 작업에서 `.ai/docs/architecture.md`를 함께 갱신한다.
2. 제품 파일을 바꾸기 전에 가장 가까운 `AGENTS.md`, `Input.md`, `.ai/docs/architecture.md`, `.ai/docs/performance.md`, 프로젝트 Obsidian 문서(Table7 `Obsidian_Id`), 오늘자 `.ai/changes/YYMMDD.<제목>.md`, 현재 Table26 행을 읽는다.

## 2. 초기화 여부 확인 (`Initialization check`)

- `/home/tree/extensions` 하위 프로젝트의 제품 파일을 바꾸기 전에 가장 먼저 확인한다.
- 프로젝트 `AGENTS.md` 맨 위에 `- Table7 Raw ID: <n>`을 가진 `## 작업 상태` 헤더가 없거나 `~/ai/scripts/bin/grist project <n>`이 행을 반환하지 않으면 미초기화 프로젝트다.
- 미초기화 프로젝트는 먼저 `initialize-project` Skill(`~/ai/skills/initialize-project/SKILL.md`)을 실행해 Table7 행·Obsidian 문서·표준 헤더를 만든 뒤, 현재 요청을 아래 절차로 기록하고 이어간다.
- `grist gate`는 변경 파일이 있는 미초기화 프로젝트를 `uninitialized`로 보고하며, Stop hook이 턴 종료를 차단한다.

## 3. 요청 기록 → 행 분리 → 목표 구체화 → 배정 (`Flow 0~3`)

0. **요청 기록 (`request-record`, main)**
   - 새 `Input.md`(`# 요구 사항`은 사용자 원문 요약, `# 세부 사항`은 작업 단위당 한 줄)를 임시 파일에 작성하고 실행한다:
     ```bash
     ~/ai/scripts/bin/grist request-record --project <dir> --input-file <file> --title "<10자 미만 제목>" --summary "<요청 한 줄>"
     ```
   - 기존 열린 레거시 목표는 자동으로 `.ai/changes` 기록으로 옮겨지고 `Input.md`가 교체되며 Obsidian `# 📆`가 갱신된다. 제목에는 현재 프로젝트·소스명을 넣지 않는다.

1. **작업 행 분리 (`task-add`, main)**
   - `# 세부 사항` 한 줄이 요구사항 하나다(같은 파일을 건드리는 요구는 한 행에 묶는다).
   - 요구마다 `$H=초안`으로 Table26 행과 changes 블록을 만든다(`- 판정 :` 줄은 `[값]` 또는 `[상태]`를 골라 `--judge "[값|상태] <한줄 판정 내용>"`으로 작성한다):
     ```bash
     ~/ai/scripts/bin/grist task-add --project <dir> --type "기능 추가|개선|오류 해결" --judge "[값|상태] <한줄 판정 내용>" --fields '{"A": <Table7 Raw ID>, "B": "<간결한 제목>", "H": "초안"}'
     ```
   - 프로젝트 `AGENTS.md` 상단 헤더를 새 행 목록과 현재 작업으로 교체한다:
     ```markdown
     - Table26 task Raw ID: <id>[, <id> …]
     - 현재 작업: <한 줄>
     ```

2. **목표 구체화 (`task-goal`, main)**
   - 각 행의 상세 목표를 `[연결|기능|수정]` 태그와 `▷`(리턴값) / `⇒`(전달 대상) 기호를 쓴 한 줄 요약들로 작성한다(JSON 원문 객체나 `변형:` 줄은 쓰지 않고 `message`/`body` 또는 핵심 값만 적는다).
   - 사용자가 명시한 요구만 넣으며, 해석이 둘 이상이라 결과 workflow가 달라질 때만 수정 전에 묻는다.
   - 다음 명령으로 changes `### 목표`와 Table26 `$K`(한 줄 요약)에 기록한다:
     ```bash
     ~/ai/scripts/bin/grist task-goal <id> --project <dir> --goal-file <file> --summary "<한 줄 요약>"
     ```

3. **에이전트 배정 (`Assign`, main)**
   - 행마다 `task_name`을 정확히 `t26_<id>`, `fork_turns="none"`으로 에이전트를 실행하거나, 단일 소규모 행은 메인 세션이 직접 `references/work.md`의 동일 명령 절차를 수행한다.
   - 에이전트 프롬프트에는 다음만 전달한다:
     - 프로젝트 경로와 첫 명령 `~/ai/scripts/bin/read-architecture --project <dir> --query "<키워드>"` 실행 및 `AGENTS.md` 읽기 지시
     - `Table26 #<id>` 및 `.ai/changes`의 `## [구현] … (#<id>)` → `### 목표` 읽기 지시
     - 수정 허용 경로 목록(형제 프로젝트, 전역 설정, Skill, 다른 행의 파일 수정 금지)
     - `~/ai/skills/work-project/references/work.md`를 읽고 TDD 절차를 수행하라는 지시
   - 수정 경로가 겹치지 않는 행만 병렬로 실행한다.
