# 1단계: 작업 시작 (`work-project` — Start)

새 사용자 요청을 받았거나, 진행 중인 요청의 정정을 받았거나, 프로젝트를 처음 열었거나, 새 Table26 작업 행·목표를 만들고 에이전트를 배정할 때 이 문서를 읽고 순서대로 실행한다(프로젝트 문서·코드는 전체를 읽지 않고 필요한 섹션·줄·필드만 `rg -n` 또는 `grist task-show --section`으로 발췌해 읽고 출력한다).

## 1. 구조 파악 (`read-architecture`) 및 문서 확인

1. 파일 읽기·검색 전 첫 명령으로 `~/ai/scripts/bin/read-architecture --project <dir> --query "<요청 키워드>"`를 실행한다(다른 프로젝트로 옮기면 다시 실행).
   - 출력에서 작업 대상 폴더와 후보 파일을 고른 뒤 `rg -n "<패턴>" <파일>`로 위치를 찾고 필요한 범위만 읽는다(`ls -R`, `find`, `grep -r`, 전체 `cat` 금지).
   - `missing` 또는 빈 템플릿(exit 2)이면 실제 프로젝트 기준으로 `.ai/docs/architecture.md`(`# 도메인`은 `항목 : 설명`, `# 구조`는 루트·2단계 폴더, 파일명 제외)부터 작성한 뒤 다시 실행한다.
   - 작업 중 폴더가 추가·삭제·이동되거나 구조 차이가 작업 범위 안이면 같은 작업에서 `.ai/docs/architecture.md`를 함께 갱신한다.
2. 작업 경과와 현재 상태 확인은 `read-architecture` 출력과 `~/ai/scripts/bin/grist gate --cwd <dir>`(필요시 `grist task <id>`) JSON 출력만 사용한다. 컨텍스트 낭비와 자동 압축(`CHECKPOINT`)을 막기 위해 `AGENTS.md`(시스템 프롬프트에 이미 로드됨), `Input.md`, 프로젝트 Obsidian 개발 문서, `.ai/changes/YYMMDD.<제목>.md` 전체를 `read_file`로 읽지 않고, 필요한 섹션·줄·필드만 `rg -n` 또는 `~/ai/scripts/bin/grist task-show <id> --project <dir> --section <섹션>`으로 발췌해 읽고 출력한다.

## 2. 초기화 여부 확인 (`Initialization check`)

- `/home/tree/extensions` 하위 프로젝트의 제품 파일을 바꾸기 전에 가장 먼저 확인한다.
- 프로젝트 `AGENTS.md` 맨 위에 `# 상태`(`### grist` 아래 `ID : <n>`, `### obsidian` 아래 `문서 : <경로>`, `### 실행` 아래 `runtime : <요약>`) 헤더가 없거나 `~/ai/scripts/bin/grist project <n>`이 행을 반환하지 않으면 미초기화 프로젝트다.
- 미초기화 프로젝트는 먼저 사용자에게 프로젝트 타입(`업무`, `공방`, `게임`, `보조` 중 택1)을 물어 `initialize-project` Skill(`~/ai/skills/initialize-project/SKILL.md`)을 실행해 Obsidian 폴더·문서(`구현/[타입]/YYMMDD.<이름>/<이름>.md`, `일지/<이름>-개발.md`, `노트/`, `[타입].md`), Table7 행, `AGENTS.md`(`작업관리기.형식-Agents.md.md` 형식), `.ai/docs/`·`.ai/changes/`를 만든 뒤, 현재 요청을 아래 절차로 기록하고 이어간다.
- `grist gate`는 변경 파일이 있는 미초기화 프로젝트를 `uninitialized`로 보고하며, Stop hook이 턴 종료를 차단한다.

## 3. 요청 기록 → 행 분리 → 목표 구체화 → 배정 (`Flow 0~3`)

- **정정 분류:** 새 메시지를 받으면 현재 `Input.md`의 `# 요구 사항`·`# 세부 사항`과 열린 Table26 `### 목표`를 먼저 대조한다. 기존 작업의 누락·오해·범위를 지적한 메시지는 그 작업의 정정으로 반영한다. 사례로 거론된 프로젝트나 명령을 별도 구현 요구로 바꾸지 않는다. 실제 수정 파일이 공용 Skill·hook·전역 설정이면 작업관리기 루트가 소유하고, 제품 자체 동작을 바꾸는 경우에만 해당 제품 프로젝트가 소유한다.
- **정정 실행:** 사용자가 지적한 미충족 목표를 원래 요청의 한 줄 요약에 반영하고 `# 세부 사항`을 실제 수정 단위로 바로잡는다. 기존 행의 목표·기록으로 처리할 수 있으면 새 행을 만들지 않는다. 별개의 Skill·설정 변경을 명시한 경우에만 그 변경을 행으로 분리한다. 관련 파일의 수정과 소비 경계 확인을 마친 다음 결과를 보고한다.
- **범위·의도 파악:**
  - 사용자가 명시한 시작 동작·구조·값·최종 효과만 범위다. 이름이 나오지 않은 인접 workflow, Bridge, 공유 helper는 바꾸지 않는다.
  - 요구 1개는 `시작 동작 → producer → transport/handler → authoritative consumer → 최종 효과` 경로 하나로 검증해 완료 조건 한 줄과 맞추고, 요구에 없는 검증·증거를 더하지 않는다.
  - 증거용 Python·validator·recorder·contract 파일을 새로 만들지 않는다.
  - 이미 정해진 사실(사용자 메시지·`Input.md`·코드·규칙)은 다시 묻지 않고, 해석이 둘 이상이라 결과 workflow가 달라질 때만 수정 전에 묻는다.

0. **요청 기록 (`request-record`, main)**
   - `작업관리기.형식-input.md` 형식의 내용을 표준 입력으로 `request-record`에 전달해 프로젝트 `Input.md`에 기록한다. `# 요구 사항`에는 채팅 원문을 복사하지 않고 사용자 요청을 한 줄로 요약한다. `# 세부 사항`에는 분리한 작업 단위마다 `[📐|✒️|⌨️|🚿|✏️|🌍] 한 줄 요약`을 쓴다. `📐`는 스키마·함수 타입 일치와 변환, `✒️`는 문서·템플릿 새 형식, `⌨️`는 설정 옵션, `🚿`는 필터, `✏️`는 계산식, `🌍`는 API·DB 연동이다. 요청 기록을 위해 `test/tmp/`나 다른 경로에 입력 사본을 만들지 않는다:
     ```bash
     ~/ai/scripts/bin/grist request-record --project <dir> --input-file /dev/stdin --title "<10자 미만 제목>" --summary "<요청 한 줄>" <<'INPUT'
     # 요구 사항
     - <사용자 요청의 한 줄 요약>

     # 세부 사항
     - [✒️] <문서·템플릿 새 형식 작업 단위 한 줄 요약>
     INPUT
     ```
   - `Input.md`가 교체되고 오늘자 `.ai/changes` 파일(`구현/업무/261008.작업관리기/형식/작업관리기.형식-changes.md` 형식)이 준비되며 Obsidian 개발 문서 `# 📆`가 갱신된다. changes 파일명에 쓰는 날짜별 제목에는 현재 프로젝트·소스명을 넣지 않는다.
   - 사용자가 `[표시 기호] 한줄 요약`처럼 정확한 입력 형식을 제시했다면 그 형식과 입력을 만드는 지점을 첫 작업 단위에 보존한다. 독립된 인접 요청은 별도 작업 단위로 기록하고, 첫 형식의 실제 출력 확인을 인접 작업의 검사나 문서 정리로 대신하지 않는다.

1. **작업 행 분리 (`task-add`, main)**
   - `# 세부 사항` 한 줄이 Grist에 작성할 작업 단위 하나다(같은 파일을 건드리는 요구는 한 행에 묶는다). 기호를 포함한 그 한 줄을 Table26 `$B` 항목명과 changes `# 작업`·`## [구현]` 제목에 동일하게 전달한다. `# 요구 사항`의 문장을 Table26 행으로 대체하지 않는다.
   - 요구마다 `$H=초안`으로 Table26 행과 changes 블록(`# 작업`의 `N. <세부 사항과 동일한 기호·문구> (#<id>)` 및 `# 진행`의 `## [구현] <세부 사항과 동일한 기호·문구> (#<id>)`)을 만든다(`- 판정 :` 줄은 `[값]` 또는 `[상태]`를 골라 `--judge "[값|상태] <한줄 판정 내용>"`으로 작성한다):
     ```bash
     ~/ai/scripts/bin/grist task-add --project <dir> --type "기능 추가|개선|오류 해결" --judge "[값|상태] <한줄 판정 내용>" --fields '{"A": <Table7 ID>, "B": "[✒️] <Input.md 세부 사항 한 줄 요약>", "H": "초안"}'
     ```

2. **목표 구체화 (`task-goal`, main)**
   - 각 행의 상세 목표를 `[연결|기능|수정]` 태그와 `▷`(리턴값) / `⇒`(전달 대상) 기호를 쓴 한 줄 요약들로 작성한다(JSON 원문 객체나 `변형:` 줄은 쓰지 않고 `message`/`body` 또는 핵심 값만 적는다).
   - 사용자가 명시한 요구만 넣으며, 해석이 둘 이상이라 결과 workflow가 달라질 때만 수정 전에 묻는다.
   - 다음 명령으로 changes `### 목표`와 Table26 `$K`(한 줄 요약)에 기록한다. 목표 전달에도 임시 파일을 만들지 않는다:
     ```bash
     ~/ai/scripts/bin/grist task-goal <id> --project <dir> --goal-file /dev/stdin --summary "<한 줄 요약>" <<'GOAL'
     1. [기능] <목표 한 줄>
     GOAL
     ```

3. **에이전트 배정 (`Assign`, main — 메인 직접 구현 금지)**
   - 메인 세션이 직접 제품 소스·테스트(`src/`, `server/` 등)를 읽거나 `edit_file`·`task-red`·`task-submit`을 수행하면 메인 컨텍스트가 누적되어 자동 컨텍스트 압축(`CHECKPOINT`)이 발생하므로, **단일 소규모 행이라도 메인 세션이 직접 구현하지 않고 반드시 행마다 `t26_<id>`(`fork_turns="none"`) 서브에이전트를 호출해 위임**한다.
   - 에이전트 프롬프트에는 다음만 전달한다:
     - 프로젝트 경로와 첫 명령 `~/ai/scripts/bin/read-architecture --project <dir> --query "<키워드>"` 실행 지시
     - `Table26 #<id>` 및 `~/ai/scripts/bin/grist task-show <id> --project <dir> --section 목표`(또는 `rg -n`)로 필요한 섹션·줄·필드만 발췌해 읽고 출력하라는 지시
     - 수정 허용 경로 목록(형제 프로젝트, 전역 설정, Skill, 다른 행의 파일 수정 금지)
     - `~/ai/skills/work-project/references/work.md`를 읽고 TDD 절차(`task-red` → 구현 → `task-submit`)를 수행하라는 지시
   - `invoke_subagent` 직후 호출자가 `agent-id start <project dir> <row> --role t26_<row> --conversation <conversationId> --caller <own char> <own name>`을 실행해 에이전트 번호와 호출자를 행 메모에 기록한다(메인은 `0 main`). 출력된 11자 ID를 `send_message`로 서브에이전트에 전달하고, 서브에이전트는 `task-red --agent-id <ID>`로 실행해 기록 중복을 막는다. 서브에이전트 자신은 start를 실행하지 않는다.
   - 수정 경로가 겹치지 않는 행만 병렬로 실행한다.
   - 서브에이전트는 동시에 최대 2개까지만 실행한다. 3개 이상이면 2개씩 묶어 호출하고, 둘 다 완료 메시지를 받은 뒤 다음 묶음을 호출한다.
