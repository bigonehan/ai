---
name: grist-access
description: Read and write the extensions workflow's Grist document (au1EbKLtZbrW) — Table7 projects, Table26 task state ($H/$I/$J), Table27 and any other table — through scripts/grist.py, which loads the API key from this skill's .env. Use it whenever an extensions project task needs Grist, instead of asking the user for an API key or calling the Grist API directly.
---

# Grist access

Use `scripts/grist.py` for every Grist read or write in `/home/tree/extensions` work. The API key lives in this skill's `.env` (mode 600) and is loaded by the script; never ask the user for it, never read or print `.env`, and never put the key in commands, files, logs or change records.

Run with the absolute path:

```bash
python3 ~/.codex/skills/grist-access/scripts/grist.py <command> ...
```

## Commands

| Command | Purpose |
|---|---|
| `tables` | List table ids |
| `columns TABLE` | Column ids, labels, types, Choice values and date formats |
| `get TABLE [--id N ...] [--filter JSON] [--fields A,B]` | Read rows |
| `add TABLE --fields JSON` | Add one row, prints its id |
| `update TABLE --id N --fields JSON` | Update one row, prints it back |
| `project RAW_ID` | Table7 row (project, `관리 프로그램` path etc.) |
| `task RAW_ID` | Table26 row (task state) |
| `task-update RAW_ID --fields JSON` | Update a Table26 row |
| `task-add --fields JSON` | Add a Table26 row, prints its id |

## Tables

- **Table7** — projects. A project's `AGENTS.md` names its `Table7 Raw ID`.
- **Table26** — tasks: `A` 대상 (Ref: Table7 row id), `B` 항목, `C` 상태, `D` 메모, `E` 날짜 (epoch seconds), `F` 완료일, `G` 종류 (`기능`/`오류`), `H` 단계, `I` 오류, `J` 원인. `H` accepts only `초안`, `진단`, `교정`, `검증`, `대기`, `완료`; the script rejects anything else. A project's `AGENTS.md` names its `Table26 task Raw ID`. `D`, `I` and `J` hold one line each: the latest item with its number (`3. ...`), replacing the previous line; the full numbered history lives in the project's `.ai/changes` record.
- **Table27** and others — use `columns` first, then `get`/`add`/`update`.

## Examples

```bash
G=~/.codex/skills/grist-access/scripts/grist.py
python3 $G task 8
python3 $G task-update 6 --fields '{"H": "대기", "I": "경로: ... / 실제값: ... / 기대값: ... / artifact: ..."}'
python3 $G task-add --fields '{"A": 1202, "G": "기능", "B": "항목", "D": "메모", "H": "초안"}'
python3 $G get Table27 --fields A,E
```

After a write, the script prints the row back; compare it with what was intended. Errors go to stderr as JSON with a `boundary` (`config`, `connection`, `http_*`, `validation`, `arguments`) and exit code 2 — report them; do not retry with a key typed on the command line.
