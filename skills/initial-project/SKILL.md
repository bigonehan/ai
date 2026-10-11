---
name: initial-project
description: Initialize a new project with its AGENTS.md, Input.md, Grist Table7/Table26 tracking, Obsidian management record, and atomic task breakdown.
metadata:
  short-description: Initialize project tracking without legacy workflow artifacts
---

# Initial Project (`initial_project` alias)

Use this skill only for a genuinely new project or one missing its Table7 Raw ID or Obsidian management path. Read the nearest rules and resolve the `/home/tree/extensions` Table7 management document before mutation. Existing initialized projects use `work_project`.

## Initialization

1. Resolve the project name, Asia/Seoul date, Table7 row, and the Obsidian Vault-relative `관리 프로그램` path. Do not create a second transport, retry queue, or response reader.
   A new Table7 row starts with `$D` (상태) `진행`, 마감일 `$F` as the Unix timestamp of December 3 of next year (Asia/Seoul year + 1, UTC midnight), and `$K` (상금) left empty.
2. Run the existing `scripts/initialize_project.mjs` when available; it reads the Grist credentials from the `grist-access` skill's `.env`. Read back the Obsidian bytes and the Grist rows after writes with `python3 ~/.codex/skills/grist-access/scripts/grist.py project <Table7 Raw ID>` and `task <Table26 Raw ID>`. Never record credentials, authorization data, raw private payloads, or full private paths in output.
3. Create the project `AGENTS.md` starting with the standard header — `## 작업 상태` (`- Table7 Raw ID: <n>`, `- Table26 task Raw ID: <id>[, <id> …]` (one id per Table26 row created in step 4), `- 현재 작업: <one line>`, `- Runtime: <URL> / <artifact> / <start action> / <authoritative consumer>`) (no `## 완료 조건`: each row's goal lives in `Input.md` `### 목표 #<id>` with a one-line summary in Table26 `$K`, written in step 4) — plus `Input.md`, one dated `.ai/changes/YYMMDD.요약명.md` record with one `# jobs` heading, and the Obsidian task document. Preserve existing requirements.
4. Create one Table26 row per atomic user requirement. Store the Table7 Raw ID, requirement, timestamp, and the current stage `$H=초안`; do not write a stage to `$C`. Then write each row's concretized goal with `grist.py task-goal <id> --project <dir> --goal-file <file> --summary "<one line>"` in the goal format of the `grist-access` skill (detail → `Input.md`, summary → `$K`). Read the rows back and keep their identities in the management records.
5. Split only stated requirements into atomic items. Do not invent relationships, fields, transports, or adjacent workflows.

## Grist access

Every Grist read and write (Table7 project rows, Table26 task rows, readback after writes) goes through the `grist-access` skill: `python3 ~/.codex/skills/grist-access/scripts/grist.py <command>`. It loads the API key from its own `.env`. Do not ask the user for an API key, do not type a key into a command, and do not call the Grist REST API with curl, fetch, or ad-hoc scripts. If the script reports a `config` or `connection` error, report that boundary instead of working around it.

## Stages and completion

Table26 `$H` is a Choice column that stores one Korean value: `초안`, `작업`, `교정`, `검증`, `대기`, or `완료` (`진단` is legacy). English names are aliases only and are never written to `$H`. `검토`(REVIEW) and per-item `RUNTIME` are not stages.

After initialization, item work follows the `work-project` flow: one `t26_<id>` agent per row pins its goal with a failing test (`task-red`, `작업`) and submits the passing implementation with `grist.py task-submit` (`검증`), the main session runs the whole flow and `flow-wait` (`대기`), and `task-complete` writes `완료` after user confirmation. The Codex Stop/SubagentStop hook blocks ending a turn outside those states.

The project rules, Input.md, dated change record, Table26 state, command output, and authoritative consumer are the records. After every item is complete, run one project-level runtime whole flow through the real artifact and consumer.
