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
3. Create the project `AGENTS.md`, `Input.md`, one dated `.ai/changes/YYMMDD.요약명.md` record with one `# jobs` heading, and the Obsidian task document. Preserve existing requirements.
4. Create one Table26 row per atomic user requirement. Store the Table7 Raw ID, requirement, timestamp, and the current stage `$H=초안`; do not write a stage to `$C`. Read the rows back and keep their identities in the management records.
5. Split only stated requirements into atomic items. Do not invent relationships, fields, transports, or adjacent workflows.

## Grist access

Every Grist read and write (Table7 project rows, Table26 task rows, readback after writes) goes through the `grist-access` skill: `python3 ~/.codex/skills/grist-access/scripts/grist.py <command>`. It loads the API key from its own `.env`. Do not ask the user for an API key, do not type a key into a command, and do not call the Grist REST API with curl, fetch, or ad-hoc scripts. If the script reports a `config` or `connection` error, report that boundary instead of working around it.

## Stages and completion

Table26 `$H` is a Choice column that stores one Korean value: `초안` (SCOPE), `진단` (DIAGNOSE), `교정` (REPAIR), `검증` (VERIFY), `대기` (HANDOFF), or `완료` (DONE). English names are aliases only and are never written to `$H`. `검토`(REVIEW) and per-item `RUNTIME` are not stages.

After initialization, item work follows `work-project`: one bounded implementation agent per item, main-session comparison of diff and tests with `Input.md`, and a separate verifier only for external-data mutation contracts, shared Bridge contract extensions, or explicit user requests.

The project rules, Input.md, dated change record, Table26 state, command output, and authoritative consumer are the records. After every item is complete, run one project-level runtime whole flow through the real artifact and consumer.
