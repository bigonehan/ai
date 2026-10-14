---
name: access-grist
description: Read and write the extensions workflow's Grist document (au1EbKLtZbrW) — Table7 projects, Table26 task state ($H/$I/$J), Table27 and any other table — through scripts/grist.py, which loads the API key from this skill's .env. Use it whenever an extensions project task needs Grist, instead of asking the user for an API key or calling the Grist API directly.
---

# Grist access

Use `scripts/grist.py` for every Grist read or write in `/home/tree/extensions` work. The API key lives in this skill's `.env` (mode 600) and is loaded by the script; never ask the user for it, never read or print `.env`, and never put the key in commands, files, logs or change records.

Run with the absolute path:

```bash
python3 ~/.codex/skills/access-grist/scripts/grist.py <command> ...
```

## Commands

| Command | Purpose |
|---|---|
| `tables` | List table ids |
| `columns TABLE` | Column ids, labels, types, Choice values and date formats |
| `get TABLE [--id N ...] [--filter JSON] [--fields A,B]` | Read rows |
| `add TABLE --fields JSON` | Add one row, prints its id |
| `update TABLE --id N --fields JSON` | Update one row, prints it back |
| `delete TABLE --id N` | Delete one row, prints whether it is gone |
| `column-update TABLE COLUMN --fields JSON` | Change one column's settings (e.g. `{"formula": "..."}`), prints them back |
| `project RAW_ID` | Table7 row (project, `관리 프로그램` path etc.) |
| `task RAW_ID` | Table26 row (task state) |
| `task-update RAW_ID --fields JSON` | Update a Table26 row (rejects `H: "검증"`, `H: "완료"` and any `K`; `add`/`update Table26` reject `K` too) |
| `task-add --fields JSON` | Add a Table26 row, prints its id |
| `gate --cwd DIR [--row N]` | Nearest `AGENTS.md` with `Table26 task Raw ID: 14[, 15 …]` (or row N) → `{raw_ids, rows:[{id,H,D,I}], project_dir}` (`raw_ids: []` if none). Used by the Codex Stop/SubagentStop hook |
| `task-goal RAW_ID --project DIR --goal-file F --summary TEXT` | The only writer of the goal. Main session only, row `$H=초안`. Checks the goal file format below and runs jev J3 (goal grounded in the newest `Input.md` request); writes the detailed lines to the project `Input.md` as `### 목표 #RAW_ID` (replacing that row's block, appended at the end), `$K` (목표) = the one-line `--summary`, and records `## 목표 #RAW_ID` with the detailed goal's sha256 |
| `task-red RAW_ID --project DIR --test CMD --tests T1,T2 [--paths P1,P2]` | TDD red. Main or agent `t26_RAW_ID`; the goal must exist. CMD must fail on an assertion (a missing module/skeleton is rejected). Records `## 빨강 #RAW_ID` (test, test-file hashes, goal hash, editable paths, git base) and sets `$H=작업`. Changing tests later needs a new red run |
| `task-submit RAW_ID --project DIR` | The only writer of `$H=검증`. Main or agent `t26_RAW_ID`, row `작업`/`교정`. Requires the red record with unchanged goal and test files, reruns its test (exit 0), rejects changes outside the editable paths of the flow and goal expected-output literals in implementation diffs, runs jev J1 (test output vs goal; enforce failure → `교정` + `$I`). Records `## 제출 #RAW_ID` |
| `flow-wait --project DIR [--waiting TEXT]` | The flow gate to `$H=대기`, main only. Every header row `검증`/`대기`/`완료`; each `검증` row needs a `## 숨긴 사례 #RAW_ID` (`- 테스트: \`CMD\``, not the submitted test) written after its latest `## 제출`; reruns submitted and hidden tests (failure → `교정` + `$I`); needs a `## 흐름 판정` (`- [x]` + `근거:`) written after the latest `## 제출`; jev J1 on `[화면]` lines; writes `$H=대기` with `[사용자]` lines (or TEXT) in `$D` |
| `task-complete RAW_ID --project DIR` | The only writer of `$H=완료`, main only. Every goal line (`[유형] text`; legacy rows without a goal use `AGENTS.md` `## 완료 조건`) must appear as `- [x] <same text>` plus a `근거:` line under `## 완료 판정 #RAW_ID` in today's record; jev J1 checks the evidence; otherwise exit 2 with `unchecked`/`missing_evidence` |

Callers: `CODEX_THREAD_ID` is resolved to the Codex rollout; a subagent spawned with `task_name` `t26_<id>` is that row's agent, any other subagent is a helper (no gate writes), and everything else is the main session. A wrong caller gets boundary `actor`.

### Goal

The detailed goal (the row's completion source) is the `### 목표 #RAW_ID` block of the project `Input.md`; Table26 `$K` holds only its one-line summary. The gate commands read the block and fail (boundary `goal`) when it differs from the sha256 of the latest `## 목표 #RAW_ID` record, so only `task-goal` changes it. Rows without a block are legacy rows whose multi-line `$K` is the goal. The block (and `--goal-file`) holds only these lines:

```
- [값] <input> → <expected output>
- [상태] <action> → <readback result>
- [수치] <measurement> → <threshold>
- [불변] <what must not change>
- [화면] <screen condition>
- [사용자] <what only the user can confirm>
- 변형: <how the output follows a changed input>
```

At least one typed line; `[값]`/`[상태]`/`[수치]` need `→`; `[값]`/`[상태]` need a `변형:` line. Jev judgments come from the `access-jev` skill and are skipped when it is missing or failing.

## Tables

- **Table7** — projects. A project's `AGENTS.md` names its `Table7 Raw ID`.
- **Table26** — tasks: `A` 대상 (Ref: Table7 row id), `B` 항목, `D` 메모, `E` 날짜 (epoch seconds), `F` 완료일 (Date, epoch seconds of UTC midnight; a trigger sets it when `H` becomes `완료`), `G` 종류 (`기능`/`오류`), `H` 단계, `I` 오류, `J` 원인, `K` 목표 (one-line goal summary; the detailed lines are in `Input.md` `### 목표 #N`; written only by `task-goal`, read-only for agents). `H` accepts only `초안`, `진단`, `작업`, `교정`, `검증`, `대기`, `완료`; the script rejects anything else. A project's `AGENTS.md` names its `Table26 task Raw ID`. `D`, `I` and `J` hold one line each: the latest item with its number (`3. ...`), replacing the previous line; the full numbered history lives in the project's `.ai/changes` record.
- **Table27** and others — use `columns` first, then `get`/`add`/`update`.

## Examples

```bash
G=~/.codex/skills/access-grist/scripts/grist.py
python3 $G task 8
python3 $G task-update 6 --fields '{"H": "대기", "I": "경로: ... / 실제값: ... / 기대값: ... / artifact: ..."}'
python3 $G task-add --fields '{"A": 1202, "G": "기능", "B": "항목", "D": "메모", "H": "초안"}'
python3 $G get Table27 --fields A,E
```

After a write, the script prints the row back; compare it with what was intended. Errors go to stderr as JSON with a `boundary` (`config`, `connection`, `http_*`, `validation`, `arguments`, `actor`, `goal`, `red`, `submit`, `flow`, `completion`) and exit code 2 — report them; do not retry with a key typed on the command line.
