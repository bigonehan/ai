# 초기화: 미초기화 프로젝트 (`manage-project` — Initialize)

Use this procedure only for a genuinely new project or one missing its `# 상태` (`### grist` `ID :` / `### obsidian` `문서 :`) header in `AGENTS.md`. Check only the needed `# 상태` lines via `rg -n` (do not read full `AGENTS.md`, `.ai/changes/`, or Obsidian documents) before mutation. Existing initialized projects skip this document and use the `manage-project` stage cards.

## Initialization

1. Ask the user to choose the project type (`프로젝트 타입`) from the 4 allowed types:
   - `업무`, `공방`, `게임`, `보조`
   Resolve the project name (`<name>`), Asia/Seoul date (`YYMMDD`, `YYYY`, `M월`, `MMDD`), and a one-line project description.
   A new Table7 row starts with `$D` (상태) `진행`, 마감일 `$F` as the Unix timestamp of December 3 of next year (Asia/Seoul year + 1, UTC midnight), and `$K` (상금) left empty.
2. Run `~/ai/skills/manage-project/scripts/bin/initialize-project --project-root <dir> --project-name <name> --project-type <업무|공방|게임|보조> --description "<한 줄 설명>" [--runtime "<실행 방법 한줄 요약>"]`; it reads the Grist credentials from the `access-grist` skill's `.env` and creates (keeping whatever already exists):
   - **Obsidian project folder and files**:
     - `구현/<타입>/YYMMDD.<name>/<name>.md` from `구현/업무/261008.작업관리기/형식/작업관리기.형식-문서.md`:
       - Top folder link set to `[[구현/<타입>|📁]]`
       - Under `# 🔨`: `[[<name>-개발.md]]`
       - Under `# 📆`: `## YYYY` / `### M월` / `MMDD : 문서 생성`
     - `구현/<타입>/YYMMDD.<name>/일지/<name>-개발.md` from `구현/업무/261008.작업관리기/형식/작업관리기.형식-개발.md`:
       - Top folder link set to `[[<name>|📁]]`
       - `# 📋` filled with `- <한 줄 설명>`, instruction fences removed, user area from `# 💡` preserved
       - Under `# 📆`: `## YYYY` / `### M월`
     - `구현/<타입>/YYMMDD.<name>/노트/` directory
     - `구현/<타입>/<타입>.md`: adds `[[<name>]]` under `# 🗄`
   - **Grist Table7 row**:
     - `$Obsidian_Id` = `구현/<타입>/YYMMDD.<name>/일지/<name>-개발.md`
   - **Project `AGENTS.md` and `.ai/`**:
     - `AGENTS.md` from `구현/업무/261008.작업관리기/형식/작업관리기.형식-Agents.md.md` with `# 상태` updated:
       ```markdown
       # 상태
       ### grist
       ID : <Table7 ID>
       ### obsidian
       문서 : 구현/<타입>/YYMMDD.<name>/일지/<name>-개발.md
       ### 실행
       runtime : <진입지점 혹은 실행 방법 한줄 요약>
       ```
     - `.ai/docs/architecture.md` from `구현/업무/261008.작업관리기/형식/작업관리기.형식-architecture.md`
     - `.ai/docs/performance.md` from `구현/업무/261008.작업관리기/형식/작업관리기.형식-performance.md`
     - `.ai/changes/` directory
   Verify only the needed headings/lines of the created Obsidian files with `rg -n` (do not dump full files) and check `~/ai/scripts/bin/grist project <Table7 ID>`. Never record credentials, authorization data, raw private payloads, or full private paths in output.
3. Record the first request exactly as the `manage-project` start card does: summarize `# 요구 사항` in one line, choose a `📐|✒️|⌨️|🚿|✏️|🌍` marker for each `# 세부 사항`, and carry each marked line to changes and Table26 `$B`:
   - `grist request-record` (takes `# 요구 사항`/`# 세부 사항` input and writes today's `.ai/changes/YYMMDD.<제목>.md` following `구현/업무/261008.작업관리기/형식/작업관리기.형식-changes.md` with `# 요청`/`# 작업`/`# 완료`/`# 진행`, and the development note's `# 📆`),
   - `grist task-add --project <dir> --type <유형> --judge "[값|상태] <한줄 판정 내용>"` per marked `# 세부 사항` line, with the same line in `$B` (`$H=초안`),
     with `--goal-file F --summary S` (detail → changes `### 목표`, summary → `$K`); the printed `row` line is the new row's `$H`/`$I`/`$J`/`$K` read.
4. Fill `.ai/docs/architecture.md` (domains and root/second-level folders, no file names) and the development note's `# 🏗` from the actual project; keep the project `AGENTS.md` under 32KB.
5. Split only stated requirements into atomic items. Do not invent relationships, fields, transports, or adjacent workflows.

## Grist access

Every Grist read and write (Table7 project rows, Table26 task rows, readback after writes) goes through the `access-grist` skill: `~/ai/scripts/bin/grist <command>`. It loads the API key from its own `.env`. Do not ask the user for an API key, do not type a key into a command, and do not call the Grist REST API with curl, fetch, or ad-hoc scripts. If the script fails at a `config`, `connection` or `http_*` boundary, report that boundary and the exit code instead of working around it.

## Stages and completion

Table26 `$H` is a Choice column that stores one Korean value: `초안`(초안), `진단`(오류 발생·수정 필요), `작업`(소스 전달·구현 중), `교정`(유닛테스트 중), `검증`(흐름 테스트 중), `확인`(사용자 테스트 필요), or `완료`(최종 완료).

After initialization, item work follows the `manage-project` stage cards: the row is read once (the `row` line passed by the main session, or `grist task <id>` when none), the worker records the user's reported error in `$I` and stated cause in `$J`, or reproduces an unknown cause before recording `$J`, then repairs the cause shown in that update's `row` line. It uses `task-red` for an unknown cause or `task-ready` for an already identified cause, runs unit tests (`교정`, or `진단` on error) and submits the passing implementation with `grist task-submit` (`검증`). The main session runs the whole flow and `flow-wait` while the row stays at `검증`; then `Complete_task` checks the goal and completion list and writes `확인`. The user alone changes the final state to `완료`. The Codex Stop/SubagentStop hook calls `task-complete` for the end decision.
