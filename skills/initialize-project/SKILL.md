---
name: initialize-project
description: Initialize a new project with its AGENTS.md, Input.md, Grist Table7/Table26 tracking, Obsidian management record, and atomic task breakdown.
metadata:
  short-description: Initialize project tracking without legacy workflow artifacts
---

# Initial Project (`initial_project` alias)

Use this skill only for a genuinely new project or one missing its Table7 Raw ID or Obsidian management path. Read the nearest rules and resolve the `/home/tree/extensions` Table7 management document before mutation. Existing initialized projects use `work-project`.

## Initialization

1. Resolve the project name, Asia/Seoul date and a one-line project description. Do not create a second transport, retry queue, or response reader.
   A new Table7 row starts with `$D` (상태) `진행`, 마감일 `$F` as the Unix timestamp of December 3 of next year (Asia/Seoul year + 1, UTC midnight), and `$K` (상금) left empty.
2. Run `scripts/bin/initialize-project --project-root <dir> --project-name <name> --description "<한 줄 설명>"`; it reads the Grist credentials from the `access-grist` skill's `.env` and creates (keeping whatever already exists):
   - the Table7 row with `Obsidian_Id` = `구현/진행/YYMMDD.<name>/YYMMDD.<name>-개발.md`;
   - that Obsidian note from `작업관리기/작업관리기-초기문서.md` (instruction fences replaced, `# 📋` filled, the user area from `# 💡` untouched);
   - the project `AGENTS.md` standard header `## 작업 상태` (`- Table7 Raw ID`, `- Table26 task Raw ID`, `- 현재 작업`, `- Runtime`), prepended to an existing file;
   - `.ai/docs/architecture.md` and `.ai/docs/performance.md` from the 작업관리기 templates, and `.ai/changes/`.
   Read back the Obsidian bytes and `~/ai/scripts/bin/grist project <Table7 Raw ID>`. Never record credentials, authorization data, raw private payloads, or full private paths in output.
3. Record the first request exactly as `work-project` steps 0–1 do: `grist request-record` (writes `Input.md` with `# 요구 사항`/`# 세부 사항`, today's `.ai/changes/YYMMDD.<제목>.md` with `# 작업`/`# 완료`/`# 진행`, and the note's `# 📆`), then `grist task-add --project <dir> --type <유형>` per `# 세부 사항` line (`$H=초안`), put the ids in the `AGENTS.md` header, and write each goal with `grist task-goal` (detail → changes `### 목표`, summary → `$K`).
4. Fill `.ai/docs/architecture.md` (domains and root/second-level folders, no file names) and the note's `# 🏗` from the actual project; keep the project `AGENTS.md` under 32KB.
5. Split only stated requirements into atomic items. Do not invent relationships, fields, transports, or adjacent workflows.

## Grist access

Every Grist read and write (Table7 project rows, Table26 task rows, readback after writes) goes through the `access-grist` skill: `~/ai/scripts/bin/grist <command>`. It loads the API key from its own `.env`. Do not ask the user for an API key, do not type a key into a command, and do not call the Grist REST API with curl, fetch, or ad-hoc scripts. If the script fails at a `config`, `connection` or `http_*` boundary, report that boundary and the exit code instead of working around it.

## Stages and completion

Table26 `$H` is a Choice column that stores one Korean value: `초안`(초안), `진단`(오류 발생·수정 필요), `작업`(소스 전달·구현 중), `교정`(유닛테스트 중), `검증`(흐름 테스트 중), `확인`(사용자 테스트 필요), or `완료`(최종 완료). English names are aliases only and are never written to `$H`. `검토`(REVIEW) and per-item `RUNTIME` are not stages.

After initialization, item work follows the `work-project` flow: one `t26_<id>` agent per row pins its goal with a failing test (`task-red`, `작업`), runs unit tests (`교정`, or `진단` on error) and submits the passing implementation with `grist task-submit` (`검증`), the main session runs the whole flow and `flow-wait` (`확인`), and `task-complete` writes `완료` after user confirmation. The Codex Stop/SubagentStop hook blocks ending a turn outside those states.

The project rules, Input.md, dated change record, Table26 state, command output, and authoritative consumer are the records. After every item is complete, run one project-level runtime whole flow through the real artifact and consumer.
