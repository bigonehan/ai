# Grist CLI 세부 명령

`SKILL.md`의 짧은 표로 부족할 때(인자 세부, 호출자 판정, Record hook, Goal 형식, 테이블 열, 예시)만 필요한 절을 `rg -n`으로 찾아 읽는다.

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
| `task RAW_ID` | Table26 row read (`H`, `I`, `J`, `K`) once at work start for an existing row; `task-add`/`task-goal`/`task-update` print the same state as their `row` line |
| `task-update RAW_ID --fields JSON` | Update a Table26 row (rejects `H: "검증"`, `H: "확인"`, `H: "완료"` and any `K`; `add`/`update Table26` reject `K` too). `H: "진단"` needs `I` and `D` in the same call. Every `J` line and the single `D` line must start with an error symbol `[🅰️\|🗝\|❌\|⭕️\|🔢\|🛣\|👥\|🏷\|🚥\|🔳]` (manage-project 3절 `### 원인`) |
| `task-add --fields JSON [--project DIR --type 기능 추가\|개선\|오류 해결 --judge "[값\|상태] <한줄 판정 내용>" --goal-file F --summary S]` | Add a Table26 row and its goal. With `--project`, `--type`, `--goal-file` and `--summary` are required; the goal is checked (every line `N. [연결\|기능\|수정] <내용>`, J3) before anything is created, then written to changes `### 목표` and `$K`. Put the exact `[📐\|✒️\|⌨️\|🚿\|✏️\|🌍] 한 줄 요약` from `# 세부 사항` in `$B`. Prints `id`, `record` and the `row` line Goal lines joining two actions (a Hangul word ending in `고` + space/comma, or ` 및 `) add a `warnings` list to the output; the row is still written — split them with `inquire-task`. |
| `request-record --project DIR --input-file F --title T --summary S` | New user request, main only. Reads F (`# 요구 사항` one-line summary and marked `# 세부 사항` `- ` lines), creates today's record `YYMMDD.<T under 10 chars>.md` if missing, appends `## <요구 사항>` with the `- <세부 사항>` lines to its first section `# 요청` (newest last; J3 reads the newest block), and writes S under the Obsidian note's `# 📆` as one line per day, `MMDD : [version] summary` under `## YYYY / ### M월` (newest year, month and day first; the version comes from the project's version file and is left out when there is none; a later request the same day replaces that day's summary). `flow-wait` refreshes today's line with the released version |
| `sync [--project DIR \| --cwd DIR]` | Replay Table26 state of the project's open `# 작업` items into the records (same hook as below). Run by the Codex and Claude Code Stop hooks so stages changed in the Grist web UI are recorded. A user-set `$H=완료` outranks every other record and moves the item to `# 완료`. Idempotent |
| `gate --cwd DIR [--row N]` | Nearest `AGENTS.md` with `# 상태` (`### grist` `ID :`) and open `.ai/changes` `# 작업` rows (or row N) → `{raw_ids, rows:[{id,H,D,I}], project_dir}` (`raw_ids: []` if none). Used by the Codex Stop/SubagentStop hook |
| `task-show RAW_ID --project DIR [--section 제목]` | Extract only the needed section (`목표`, `구현`, `메모`, `오류`, `원인`), the latest stage record (`초안`, `검증`, `확인`, `완료`: the lines under that row's last `#### <stage> (#id)`), or goal+steps of a row from `.ai/changes` without reading the whole file |
| `request --project DIR` | Print only the newest `## ` block of the changes `# 요청` section (the request J3 judges); exit 1 when none. Corrections are new `request-record` blocks, never edits |
| `lists --project DIR` | Print only the `# 작업` and `# 완료` headings and list lines of the latest changes record that lists work (the same record `Complete_task` judges); exit 1 when none |
| `task-step RAW_ID --project DIR --step ▷\|▶ --text TEXT --goal N` | Record a `▷`/`▶` implementation line under `### 구현` and set `$K` = `[N/M <$H>] <goal line N>` (N = measurement unit being worked; `task-submit` and a `진단` switch update only the stage, `확인` leaves it); `○`/`●` are rejected (recorded by `task-confirm`) |
| `task-confirm RAW_ID --project DIR --summary TEXT --check LABEL --detail TEXT --evidence F` | Records `- [○] LABEL`, `- [●] 사용자 검토 대기` and `#### 확인 (#RAW_ID)` under `### 구현` in one call (idempotent for ○/●). F has one `N. <근거>` line per goal line (count must match, else rejected), written as `- 근거 N : …`; J1 judges these lines plus the flow confirmation, not the test output |
| `task-goal RAW_ID --project DIR --goal-file F --summary TEXT` | Rewrites the goal of a row still at `$H=초안` (main session only); new rows get their goal from `task-add`. Checks the goal file format below and runs jev J3 (goal grounded in the newest changes `# 요청` block); writes the detailed lines as the numbered `### 목표` list of the row's changes item (created from `$B`/`$G` when missing), and `$K` (목표) = the one-line `--summary` Same `warnings` for goal lines joining two actions. |
| `goal RAW_ID --project DIR` | Print only the changes `### 목표` lines as plain text (exit 2 `goal missing (#id)`); the only way to read a goal — never open the changes file for it |
| (task chain) | `task-add`, `task-red`/`task-ready`, `task-submit` and the completion `check` run one `TaskStore` trait chain (`grist/src/task.rs`): gate test record → `.ai/changes` → Table26 `$H` (add: Table26 row first, then changes). `check` is called only by `task-complete` and is the only writer of `$H=확인`. The gate record keeps a working-tree `snapshot` tree hash (no dirty file list) and a test-output digest (result lines, passed test names, failure lines with a filtered tail); every test record detail ends with the result line and duration, a failed `task-submit` is recorded too, and `task-red`/`task-ready`/`task-submit` print only `{id, H}`. |
| `task-red RAW_ID --project DIR --purpose LABEL --test CMD --tests T1,T2 [--paths P1,P2] [--detail TEXT]` | TDD red. Agent `t26_RAW_ID` (main delegates implementation and unit tests to `t26_RAW_ID`); the goal must exist. CMD must fail on an assertion (a missing module/skeleton is rejected). LABEL is a short label of the wanted result, never a command (backticks/`--options` are rejected). Keeps test, test files, editable paths and git base in `.ai/gate/RAW_ID.json`, records `#### 초안 (#RAW_ID)` under `### 구현` as `- 목적 : LABEL` / `- 결과 : 실패` / `- 상세 : <file:line message>` (TEXT overrides the detail) and sets `$H=작업`. Optional `--agent-id ID` (11 chars from `record-log start`): if 메모 has `에이전트 <char> :` it skips; if not it only warns on stderr and records nothing. Without `--agent-id` it runs record-log `start DIR RAW_ID --role t26_RAW_ID` and prints `agent-id: <ID>` on stderr; pass that ID to later task-red runs (record-log failure only warns). |
| `task-ready RAW_ID --project DIR --purpose LABEL --test CMD --tests T1,T2 --paths P1,P2 [--agent-id ID]` | 원인과 결함 경계가 이미 지정된 작업의 범위와 수정 후 실행할 집중 테스트를 등록한다. 수정 전 실패 테스트를 실행하거나 요구하지 않는다. `$H=작업`으로 바꾼다. |
| `task-submit RAW_ID --project DIR` | The only writer of `$H=검증`. Agent `t26_RAW_ID` (main delegates implementation and unit tests to `t26_RAW_ID`), row `작업`/`교정`/`진단`. Requires a `task-red` or `task-ready` record, runs its focused test (exit 0), rejects changes outside the editable paths of the flow and records `#### 검증 (#RAW_ID)` under `### 구현` (`- 목적 :` / `- 결과 : 성공` / `- 상세 :` actual test summary). Final goal matching belongs to `Complete_task`. Rejected before the test when any goal number 1..M has no `task-step --goal N` record (the missing numbers are listed). |
| `task-hidden RAW_ID --project DIR --purpose LABEL --test CMD [--detail TEXT]` | Hidden case, main only, row `검증`: CMD differs from the submitted test. Runs it, keeps it in `.ai/gate/RAW_ID.json` for `flow-wait`, records `#### 확인 (#RAW_ID)` under `### 구현` as `- 목적 :` / `- 결과 : 성공\|실패` / `- 상세 :`; failure exits 2 |
| `flow-wait --project DIR` | The flow evidence gate, main only. Every current row must be `검증`/`확인`/`완료`; each `검증` row needs a `task-hidden` case after its latest `#### 검증 (#RAW_ID)`; reruns submitted and hidden tests (failure → `진단` + `$I` `N. 메인 재실행 실패: <목적> / <상세>`); needs `#### 확인 (#RAW_ID)` (`- 요약 : [표시 기호] <입력 또는 이전 단계> ⇒ <구현 결과>` and `- [x] <목적>` + `결과 : 성공` + `상세 :`) written after the latest `#### 검증 (#RAW_ID)`; an error-fix request uses the cause-matched `🅰️\|❌\|⭕️\|🔢\|🚥\|🔳` marker; keeps `$H=검증` and writes that row's implementation summary without a number as the latest `$D` line. `Complete_task` alone changes it to `확인` after J1. |
| `~/ai/scripts/bin/task-complete --row RAW_ID --project DIR` | Owned by `Complete_task`: checks actual output against `### 목표` through Jev, records numbered `#### 완료 (#RAW_ID)` and moves the item to `# 완료` once, then writes `$H=확인`. This is a separate executable, not a `grist` subcommand. Only the user changes `$H` to `완료`. |

Callers: `CODEX_THREAD_ID` is resolved to the Codex rollout; a subagent spawned with `task_name` `t26_<id>` is that row's agent, any other subagent is a helper (no gate writes), and everything else is the main session. A wrong caller gets boundary `actor`.

### Record hook

Every Table26 write through this script (`add`, `update`, `task-*`, `flow-wait`) then updates the project records; a hook failure is printed to stderr as a `warning` and never undoes the Grist write (`GRIST_NO_HOOK=1` skips it):

- 작업 중 `$D`와 `$I`/`$J` → 최신 번호 한 줄을 해당 항목의 `### 메모`/`### 오류`/`### 원인` 이력에 추가한다. `$H=확인`의 `$D`는 번호 없이 `[표시 기호] <입력 또는 이전 단계> ⇒ <구현 결과>` 한 줄을 기록한다. `### 구현`에는 `- [▷|▶|○|●] ...`와 `#### 초안 (#id)` / `#### 검증 (#id)` / `#### 확인 (#id)` / `#### 완료 (#id)`를 둔다.
- `Complete_task` J1 pass → the item's `N. 항목 (#id)` line moves from `# 작업` to `# 완료` (both renumbered) once and `$H` remains `확인`. A user-set `$H=완료` is reflected by `sync` and, for `유형 : 기능 추가`, adds `- 항목` under the Obsidian note's `# 📍` without duplicating the list entry.

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
- **Table26** — tasks: `A` 대상 (Ref: Table7 row id), `B` 항목, `D` 메모, `E` 날짜 (epoch seconds), `F` 완료일 (Date, epoch seconds of UTC midnight; a trigger sets it when `H` becomes `완료`), `G` 종류 (`기능`/`오류`), `H` 단계, `I` 오류, `J` 원인, `K` 목표 (one-line goal summary; the detailed lines are in the changes item `### 목표`; written only by `task-goal`, read-only for agents). `H` accepts `초안`(초안), `진단`(오류 발생·수정 필요), `작업`(소스 전달·구현 중), `교정`(유닛테스트 중), `검증`(흐름 테스트 중), `확인`(사용자 테스트 필요), `완료`(사용자 최종 검토); the script rejects anything else. A project's current work rows are listed under `# 작업` and `# 완료` in `.ai/changes`. `D`, `I` and `J` hold one line each. 작업 중 `$D`와 `$I`/`$J`는 최신 번호 항목으로 덮고, `확인` 단계 `$D`는 번호 없는 구현 결과 요약으로 덮는다. 전체 이력은 `.ai/changes`에 남긴다.
- **Table27** and others — use `columns` first, then `get`/`add`/`update`.

## Examples

```bash
G=~/ai/scripts/bin/grist
$G task 8
$G task-update 6 --fields '{"I": "경로: ... / 실제값: ... / 기대값: ... / artifact: ..."}'
$G task-update 6 --fields '{"H": "진단", "J": "[⭕️] 저장 경로 미전달로 결과 파일 없음", "D": "[⭕️] save() 반환 경로와 실제 파일 대조"}'
$G task-add --fields '{"A": 1202, "G": "기능", "B": "[✒️] 문서 형식 도입", "D": "메모", "H": "초안"}'
$G get Table27 --fields A,E
```

After a write, the script prints the row back; compare it with what was intended. Errors go to stderr as JSON with a `boundary` (`config`, `connection`, `http_*`, `validation`, `arguments`, `actor`, `goal`, `red`, `submit`, `flow`, `completion`, `obsidian`) and exit code 2 — report them; do not retry with a key typed on the command line.

On a boundary failure, report only the boundary and exit code; do not work around it (no direct REST calls, no asking for the key). A product that uses Grist through its own settings is exempt from this skill.
