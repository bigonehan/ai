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
3. Record the first request and rows with `stage-start.md` steps 0–2 (single source; not repeated here).
4. Fill `.ai/docs/architecture.md` (domains and root/second-level folders, no file names) and the development note's `# 🏗` from the actual project; keep the project `AGENTS.md` under 32KB.
