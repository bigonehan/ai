---
name: access-grist
description: Read and write the extensions workflow's Grist document (au1EbKLtZbrW) — Table7 projects, Table26 task state ($H/$I/$J), Table27 and any other table — through ~/ai/scripts/bin/grist, which loads the API key from this skill's .env. Use it whenever an extensions project task needs Grist, instead of asking the user for an API key or calling the Grist API directly.
---

# Grist access

Use `~/ai/scripts/bin/grist` for every Grist read or write in `/home/tree/extensions` work. The API key lives in this skill's `.env` (mode 600) and is loaded by the script; never ask the user for it, never read or print `.env`, and never put the key in commands, files, logs or change records.

Read a Table26 row's `$H`·`$I`·`$J`·`$K` once at work start: `grist task <id>` for an existing row (use `gate` only to find an unknown ID); `task-add`, `task-goal` and `task-update` print the same state as their `row` line (`H=… | I=… | J=… | K=…`), which is the read for a new or just-written row. Do not re-read it; pass that line to a `t26_<id>` agent. On an error, record the user's reported failure in `$I`; record the user's stated cause in `$J`, or reproduce the failure to identify it, and repair the cause shown in that update's `row` line.

Run with the absolute path:

```bash
~/ai/scripts/bin/grist <command> ...
```

## Commands

Full arguments, caller rules, the record hook, goal format, tables and examples: `references/commands.md` (read only the section you need).

| Command | Purpose |
|---|---|
| `get TABLE [--id N] [--filter JSON] [--fields A,B]` / `add` / `update --id N` / `delete --id N` | Generic row access (`Table26` rejects `K`; `H` stages are gate-owned) |
| `tables` / `columns TABLE` / `column-update TABLE COLUMN --fields JSON` | Schema |
| `project RAW_ID` | Table7 row |
| `task RAW_ID` | Existing Table26 row state, once at work start |
| `request-record --project DIR --input-file F --title T --summary S` | New request → today's `.ai/changes` `# 요청` block, Obsidian `# 📆` (main) |
| `task-add --project DIR --type 기능 추가\|개선\|오류 해결 --judge "[값\|상태] <판정>" --fields '{"A": <t7>, "B": "<기호 요약>", "H": "초안"}' --goal-file F --summary S` | New row + changes block + goal (J3 first; blocked goal creates nothing); prints `id` and `row` |
| `task-goal RAW_ID --project DIR --goal-file F --summary TEXT` | Rewrite a goal while `$H=초안` (main); prints `row` |
| `goal RAW_ID --project DIR` | Print only the `### 목표` lines; the only way to read a goal |
| `task-update RAW_ID --fields JSON` | `$D`/`$I`/`$J`/`교정`·`진단`; prints `row` |
| `task-red` / `task-ready RAW_ID --project DIR --purpose L --test CMD --tests T --paths P` | Work gate (`$H=작업`); red only when the cause is unknown |
| `task-step RAW_ID --project DIR --step ▷\|▶ --text T --goal N` / `task-show RAW_ID --project DIR --section 제목` | Record / read one changes section or stage record (`초안`·`검증`·`확인`·`완료`) |
| `request --project DIR` | Print only the newest changes `# 요청` block (same text J3 reads) |
| `lists --project DIR` | Print only the `# 작업`·`# 완료` lists of the latest changes record |
| `task-submit RAW_ID --project DIR` | Runs the registered test, checks paths → `$H=검증` |
| `task-confirm RAW_ID --project DIR --summary S --check L --detail D --evidence F` | Records `- [○] L`, `- [●] 사용자 검토 대기`, `#### 확인` and one `- 근거 N` line per goal line (F: `N. <근거>`) at once |
| `task-hidden` / `flow-wait --project DIR` | Flow evidence (main) |
| `gate --cwd DIR [--row N]` / `sync --cwd DIR` | Open rows of a project / replay Grist state (hooks) |
| `~/ai/scripts/bin/task-complete --row RAW_ID --project DIR` | `Complete_task` judgment → `$H=확인` |
