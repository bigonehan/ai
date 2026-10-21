---
name: access-grist
description: Read and write the extensions workflow's Grist document (au1EbKLtZbrW) — Table7 projects, Table26 task state ($H/$I/$J), Table27 and any other table — through ~/ai/scripts/bin/grist, which loads the API key from this skill's .env. Use it whenever an extensions project task needs Grist, instead of asking the user for an API key or calling the Grist API directly.
---

# Grist access

Use `~/ai/scripts/bin/grist` for every Grist read or write in `/home/tree/extensions` work. The API key lives in this skill's `.env` (mode 600) and is loaded by the script; never ask the user for it, never read or print `.env`, and never put the key in commands, files, logs or change records.

Run with the absolute path:

```bash
~/ai/scripts/bin/grist <command> ...
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
| `task-add --fields JSON [--project DIR --type 기능 추가\|개선\|오류 해결 [--judge "[값\|상태] <한줄 판정 내용>"]]` | Add a Table26 row, prints its id. Put the exact `[📐\|✒️\|⌨️\|🚿\|✏️\|🌍] 한 줄 요약` from `Input.md` `# 세부 사항` in `$B`. `$E` defaults to the current Asia/Seoul date as epoch seconds when omitted. With `--project` also writes that marked title as `N. [기호] 항목 (#id)` under `# 작업` and the `## [구현] [기호] 항목 (#id)` block under `# 진행` of today's changes record; `$G` defaults from the type (기능 추가·개선 → 기능, 오류 해결 → 오류) |
| `request-record --project DIR --input-file F --title T --summary S` | New user request, main only. Removes `Input.md` and writes F (`# 요구 사항` one-line summary and marked `# 세부 사항` lines), creates today's record `YYMMDD.<T under 10 chars>.md` if missing, and writes S under the Obsidian note's `# 📆` as one line per day, `MMDD : [version] summary` under `## YYYY / ### M월` (newest year, month and day first; the version comes from the project's version file and is left out when there is none; a later request the same day replaces that day's summary). `flow-wait` refreshes today's line with the released version |
| `sync [--project DIR \| --cwd DIR]` | Replay Table26 state of the project's open `# 작업` items into the records (same hook as below). Run by the Codex and Claude Code Stop hooks so stages changed in the Grist web UI are recorded. Idempotent |
| `gate --cwd DIR [--row N]` | Nearest `AGENTS.md` with `# 상태` (`### grist` `ID :`) and open `.ai/changes` `# 작업` rows (or row N) → `{raw_ids, rows:[{id,H,D,I}], project_dir}` (`raw_ids: []` if none). Used by the Codex Stop/SubagentStop hook |
| `task-show RAW_ID --project DIR [--section 제목]` | Extract only the needed section (`목표`, `구현`, `메모`, `오류`, `원인`) or goal+steps of a row from `.ai/changes` without reading the whole file |
| `task-step RAW_ID --project DIR --step ▷\|▶\|○\|● --text TEXT` | Record an implementation step line under `### 구현` for `RAW_ID` |
| `task-confirm RAW_ID --project DIR --summary TEXT --check LABEL --detail TEXT` | Append `#### 확인 (#RAW_ID)` under `### 구현` |
| `task-goal RAW_ID --project DIR --goal-file F --summary TEXT` | The only writer of the goal. Main session only, row `$H=초안`. Checks the goal file format below and runs jev J3 (goal grounded in the newest `Input.md` request); writes the detailed lines as the numbered `### 목표` list of the row's changes item (created from `$B`/`$G` when missing), and `$K` (목표) = the one-line `--summary` |
| `task-red RAW_ID --project DIR --purpose LABEL --test CMD --tests T1,T2 [--paths P1,P2] [--detail TEXT]` | TDD red. Agent `t26_RAW_ID` (main delegates implementation and unit tests to `t26_RAW_ID`); the goal must exist. CMD must fail on an assertion (a missing module/skeleton is rejected). LABEL is a short label of the wanted result, never a command (backticks/`--options` are rejected). Keeps test, test files, editable paths and git base in `.ai/gate/RAW_ID.json`, records `#### 초안 (#RAW_ID)` under `### 구현` as `- 목적 : LABEL` / `- 결과 : 실패` / `- 상세 : <file:line message>` (TEXT overrides the detail) and sets `$H=작업`. |
| `task-submit RAW_ID --project DIR` | The only writer of `$H=검증`. Agent `t26_RAW_ID` (main delegates implementation and unit tests to `t26_RAW_ID`), row `작업`/`교정`/`진단`. Requires the red record, reruns its test (exit 0), rejects changes outside the editable paths of the flow, runs jev J1 (test output vs goal; enforce failure → `진단` + `$I`). Records `#### 검증 (#RAW_ID)` under `### 구현` (`- 목적 :` / `- 결과 : 성공` / `- 상세 :` Jev summary) |
| `task-hidden RAW_ID --project DIR --purpose LABEL --test CMD [--detail TEXT]` | Hidden case, main only, row `검증`: CMD differs from the submitted test. Runs it, keeps it in `.ai/gate/RAW_ID.json` for `flow-wait`, records `#### 확인 (#RAW_ID)` under `### 구현` as `- 목적 :` / `- 결과 : 성공\|실패` / `- 상세 :`; failure exits 2 |
| `flow-wait --project DIR` | The flow gate to `$H=확인`, main only. Every header row `검증`/`확인`/`완료`; each `검증` row needs a `task-hidden` case after its latest `#### 검증 (#RAW_ID)`; reruns submitted and hidden tests (failure → `진단` + `$I` `N. 메인 재실행 실패: <목적> / <상세>`); needs `#### 확인 (#RAW_ID)` (`- 요약 : [표시 기호] <입력 또는 이전 단계> ⇒ <구현 결과>` and `- [x] <목적>` + `결과 : 성공` + `상세 :`) written after the latest `#### 검증 (#RAW_ID)`; an error-fix request uses the cause-matched `🅰️\|❌\|⭕️\|🔢\|🚥\|🔳` marker; writes `$H=확인` and that row's implementation summary without a number as the latest `$D` line |
| `task-complete RAW_ID --project DIR` | The only writer of `$H=완료`, main only. Checks `#### 완료 (#RAW_ID)` under `### 구현` in today's record for numbered `[표시 기호] 한 줄 요약` entries (around 5 lines, without `결과`/`상세`) and sets `$H=완료`. For error fixes, the author chooses `🅰️\|❌\|⭕️\|🔢\|🚥\|🔳` by cause; this gate checks the entry format, not the marker kind. |

Callers: `CODEX_THREAD_ID` is resolved to the Codex rollout; a subagent spawned with `task_name` `t26_<id>` is that row's agent, any other subagent is a helper (no gate writes), and everything else is the main session. A wrong caller gets boundary `actor`.

### Record hook

Every Table26 write through this script (`add`, `update`, `task-*`, `flow-wait`) then updates the project records; a hook failure is printed to stderr as a `warning` and never undoes the Grist write (`GRIST_NO_HOOK=1` skips it):

- 작업 중 `$D`와 `$I`/`$J` → 최신 번호 한 줄을 해당 항목의 `### 메모`/`### 오류`/`### 원인` 이력에 추가한다. `$H=확인`의 `$D`는 번호 없이 `[표시 기호] <입력 또는 이전 단계> ⇒ <구현 결과>` 한 줄을 기록한다. `### 구현`에는 `- [▷|▶|○|●] ...`와 `#### 초안 (#id)` / `#### 검증 (#id)` / `#### 확인 (#id)` / `#### 완료 (#id)`를 둔다.
- `$H=완료` → the item's `N. 항목 (#id)` line moves from `# 작업` to `# 완료` (both renumbered); a `유형 : 기능 추가` item is also added as `- 항목` under the Obsidian note's `# 📍`.

The project is `--project`, else the nearest `AGENTS.md` with `# 상태` (`### grist` `ID :`) owning the row in `.ai/changes`, else any extensions project owning it. The Obsidian note is Table7 `Obsidian_Id` of the `AGENTS.md` `# 상태` `### grist` `ID :` under the Vault (`GRIST_OBSIDIAN_FILE` overrides it for tests); a note without `# 📆`/`# 📍` fails with boundary `obsidian`. Record edits are locked and written atomically, so parallel agents do not lose lines.

### Goal

The detailed goal (the row's completion source) is the numbered `### 목표` list of the row's `## [구현] [기호] 항목 (#RAW_ID)` block in the changes record; Table26 `$K` holds only its one-line summary. `--goal-file` holds these lines (stored as `1. [기능] …`, using `▷` for return values and `⇒` for delivery targets):

```
- [연결] <API, auth, external site or DB integration with ⇒ / ▷>
- [기능] <new feature behavior with ⇒ / ▷>
- [수정] <error or bug fix behavior with ⇒ / ▷>
```

At least one `[연결|기능|수정]` line (do not write `변형:` lines). Jev judgments come from the `access-jev` skill and are skipped when it is missing or failing.

## Tables

- **Table7** — projects. A project's `AGENTS.md` names its Table7 ID under `# 상태` → `### grist` → `ID :`.
- **Table26** — tasks: `A` 대상 (Ref: Table7 row id), `B` 항목, `D` 메모, `E` 날짜 (epoch seconds), `F` 완료일 (Date, epoch seconds of UTC midnight; a trigger sets it when `H` becomes `완료`), `G` 종류 (`기능`/`오류`), `H` 단계, `I` 오류, `J` 원인, `K` 목표 (one-line goal summary; the detailed lines are in the changes item `### 목표`; written only by `task-goal`, read-only for agents). `H` accepts `초안`(초안), `진단`(오류 발생·수정 필요), `작업`(소스 전달·구현 중), `교정`(유닛테스트 중), `검증`(흐름 테스트 중), `확인`(사용자 테스트 필요), `완료`(최종 완료); the script rejects anything else. A project's active Table26 rows are listed under `# 작업` in `.ai/changes`. `D`, `I` and `J` hold one line each. 작업 중 `$D`와 `$I`/`$J`는 최신 번호 항목으로 덮고, `확인` 단계 `$D`는 번호 없는 구현 결과 요약으로 덮는다. 전체 이력은 `.ai/changes`에 남긴다.
- **Table27** and others — use `columns` first, then `get`/`add`/`update`.

## Examples

```bash
G=~/ai/scripts/bin/grist
$G task 8
$G task-update 6 --fields '{"H": "확인", "I": "경로: ... / 실제값: ... / 기대값: ... / artifact: ..."}'
$G task-add --fields '{"A": 1202, "G": "기능", "B": "[✒️] 문서 형식 도입", "D": "메모", "H": "초안"}'
$G get Table27 --fields A,E
```

After a write, the script prints the row back; compare it with what was intended. Errors go to stderr as JSON with a `boundary` (`config`, `connection`, `http_*`, `validation`, `arguments`, `actor`, `goal`, `red`, `submit`, `flow`, `completion`, `obsidian`) and exit code 2 — report them; do not retry with a key typed on the command line.
