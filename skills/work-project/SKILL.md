---
name: work-project
description: Manage initialized projects through project AGENTS.md, Input.md and Table26 rows — main splits requests into rows and writes each row's goal (detail in the changes record `### 목표`, one line in $K), one t26_<id> agent per row works test-first (task-red → 작업 → 교정 → task-submit → 검증), main runs hidden cases and the whole flow (flow-wait → 확인) and completes after user confirmation (완료).
metadata:
  short-description: Run bounded work on an initialized project
allowed-tools: Bash(~/ai/scripts/bin/read-architecture:*)
---

# Work Project (`work_project` alias)

## Start: structure first

Loading this Skill runs the structure script automatically (Claude Code); its output for the current project is:

!`~/ai/scripts/bin/read-architecture 2>&1 || true`

Codex runs the architecture script through `SessionStart` and `UserPromptSubmit` hooks when the session is inside an extensions project or the prompt names its absolute path. If no script output appears, run it yourself as the first command: `~/ai/scripts/bin/read-architecture --project <dir> --query "<request keywords>"`. Pick the work folders and files from that output, then locate lines with `rg -n` and read only those ranges; do not re-explore the tree with `ls -R`, `find`, `grep -r` or whole-file `cat`. A `missing`/`empty template` result means: write `.ai/docs/architecture.md` from the real project first (`# 도메인` as `항목 : 설명`, `# 구조` as root and second-level folders, no file names), then rerun. When the work adds, removes or moves a folder, or the script reports a difference inside the work scope, update the doc in the same task. Run it again (`--project`) when the session moves to another project. Every `t26_<id>` agent runs it first as well.

Read test, build, runtime, and API logs with `rg -n "FAIL|Error|warn|<boundary keyword>" <log>` or a bounded `tail -n 40` / `head -c 4000` view. Narrow to the relevant lines and their nearby range; never print or read an entire log as evidence.

Then read the nearest `AGENTS.md`, `Input.md`, `.ai/docs`, the project Obsidian note (Table7 `Obsidian_Id`), today's `.ai/changes` record and current Table26 task before changing product files. The nearest project `AGENTS.md` is the current work-state source.

## Initialization check

Apply this first, before changing product files of an `/home/tree/extensions` subproject. When the project `AGENTS.md` has no `## 작업 상태` header with `- Table7 Raw ID: <n>`, or `~/ai/scripts/bin/grist project <n>` returns no row, the project is uninitialized: run the `initialize-project` Skill for it first, then record the current request through its step 3 and continue with this flow. A missing Table7 row is never a reason to skip the Grist records or to keep working without them. `grist gate` reports such a project with changed files as `uninitialized`, and the Stop hooks block ending the turn until it is initialized.

## Grist access

Every Grist read and write (Table7 project rows, Table26 task rows, readback after writes) goes through the `access-grist` skill: `~/ai/scripts/bin/grist <command>`. It loads the API key from its own `.env`. Do not ask the user for an API key, do not type a key into a command, and do not call the Grist REST API with curl, fetch, or ad-hoc scripts. If the script fails at a `config`, `connection` or `http_*` boundary, report that boundary and the exit code instead of working around it.

Typical calls: `task <Raw ID>` to read a row, `task-update <Raw ID> --fields '{"H": "작업"}'` for a stage change, `task-update <Raw ID> --fields '{"H": "진단", "I": "...", "J": "..."}'` for the failure record, `task-add --project <dir> --type "기능 추가|개선|오류 해결" --fields '{"A": <Table7 Raw ID>, "B": "...", "H": "초안"}'` for a new row (it also writes the changes item). The script rejects a `$H` value outside the Korean stages, rejects `검증`/`완료` (written only by `task-submit`/`task-complete`) and any `$K` write (only `task-goal`).

## Table26 work state

`$H` is a Choice column and the only stage field; never write a stage to `$C`. The row's goal is the `### 목표` list of its `## [구현] <항목> (#<id>)` block in the project's `.ai/changes` record (the single source of completion; legacy rows: `Input.md` `### 목표 (#<id>)`); Table26 `$K` (목표) holds only its one-line summary. Only `task-goal` writes either; agents only read them.

| `$H` | Written by | Meaning | Turn may end |
|---|---|---|---|
| `초안` | main (`task-add`, then `task-goal`) | split from `Input.md`, goal being concretized | – |
| `진단` | agent, or a gate on a failed rerun/judgment | an error occurred and needs diagnosis/fixing | no |
| `작업` | `task-red` | source passed to agent and implementation in progress | no |
| `교정` | agent | unit testing in progress | no |
| `검증` | `task-submit` | unit test passes, scope/literal/jev checks passed; flow testing in progress | agent yes / main no |
| `확인` | main via `flow-wait`, or main before a question | whole flow passed; user testing/confirmation needed | main yes |
| `완료` | main via `task-complete` | every goal line judged with evidence after user confirmation | yes |

Write `$H` only when the stage actually changes.

## Management records

- `Input.md` holds only the current request: `# 요구 사항` (what the user said) and `# 세부 사항` (one line per work unit, each becomes a Table26 row). Every new request replaces it (rm and rewrite) through `request-record`, which first moves open legacy goals into the changes record.
- `.ai/changes/YYMMDD.<제목>.md` is one file per Asia/Seoul day, named after the day's first request (under 10 characters) and kept afterwards. It has `# 작업` (open items `N. 항목 (#id)`), `# 완료` (finished items) and `# 진행` (one `## [구현] <제목> (#id)` block per row with `### 목표` / `### 메모` / `### 오류` / `### 원인` / `### 구현`). Grist cells keep only the latest line; these lists keep every line and are never deleted. Stage records (`#### 초안 (#id)`, `#### 검증 (#id)`, `#### 확인 (#id)`, `#### 완료 (#id)`) are placed under `### 구현`.
- **Changes record formatting rules**:
  1. **제목 작성 규칙**:
     - 현재 구현하고자 하는 프로젝트·소스명은 이미 폴더 내에서 사용자가 알고 있으므로 제목에 언급하지 않고 간결하게 쓴다 (예: `알림 메시지를 전송하는 mobile_notify CLI 소스와 설정을 구현하고 nf 명령과 연동한다. #166` → `알림 메시지를 전송구현 nf와 연동 (#166)`).
     - gate 및 항목 관련 식별 숫자는 항상 마지막에 괄호 안 숫자로 표현한다 (`(#166)`).
     - 항목 블록 제목은 `## [구현] <간결한 제목> (#id)` 형태로 쓴다.
     - 타입 등을 표시할 때는 `[타입]`으로 표시한다.
  2. **문자표 작성 규칙**:
     - 어떤 상태나 객체가 리턴하는 것을 표현: `▷ 리턴값` (예: `계산함수(숫자입력) ▷ True`)
     - 어떤 상태나 객체가 다른 곳에 전달하는 것을 표현: `⇒ 전달 대상` (예: `getServerName(숫자입력) ⇒ api 서버`)
  3. **Item 구성 (`### 목표` / `### 메모` / `### 오류` / `### 원인` / `### 구현`)**:
     - `### 목표`: `1. [연결 | 기능 | 수정] <목표 내용>` 번호 목록으로 작성한다.
       - `[연결]`: 인증 구현, API, 외부 사이트·DB 연동
       - `[기능]`: 새로운 기능 추가
       - `[수정]`: 발견한 오류와 문제 수정 요청
     - `### 메모`: 진행 메모 번호 목록
     - `### 오류`: 구현 중에 발견한 오류 혹은 수정해야 할 문제사항이 생기면 한 줄로 요약
     - `### 원인`: 원인을 발견할 경우 `[오류 타입] 한줄로 요약 : {리턴된 에러메시지 Body}` 형식으로 한 줄로 요약
       - `[유실]`: 계산식이 전달이 안 되거나 잘못된 곳에 저장할 경우
       - `[문자]`: 문자열, 파서, 타입 오류
       - `[인증]`: CORS, 인증 문제 등으로 값 전달 실패 (예: `[인증] 학교 서버에서 값 전달 실패 : 404 not found`)
     - `### 구현`: `- [상태 타입] <구현 내역>` 형식으로 작성하며, 에이전트가 각 단계를 시작·기록할 때는 별도 `## 제목` 대신 `### 구현` 아래에 `#### <단계 제목> (#id)`(`#### 초안 (#166)`, `#### 검증 (#166)`, `#### 확인 (#166)`, `#### 완료 (#166)`)로 제목을 달고 상세 내역을 적는다.
       - `[▷]`: red test용으로 무조건 실패하는 껍데기만 구현 (`#### 초안 (#id)`)
       - `[▶]`: 내용을 구현하고 Mock·단위 테스트를 통한 함수 검증 확인 (`#### 검증 (#id)`)
       - `[○]`: 전체 흐름 테스트(CDP 등)를 통해서 흐름 속에 정상 작동 확인 (`#### 확인 (#id)`)
       - `[●]`: 최종 완료 후 사용자 검증만 남은 상태 (`#### 완료 (#id)`)
       - `#### 완료 (#id)` 작성 규칙: 최종 완료를 통해 무엇을 했는지 `결과`/`상세` 없이 5줄 내외의 숫자 리스트(`1. [구현 타입] 한줄요약`)로 표현한다.
         - `[📦]`: 새롭게 생성된 도메인, 기능 등을 단어 리스트로 표현
         - `[📬]`: 새롭게 연결된 외부 사이트, API가 새로 있다면 여기에 기록
         - `[⚙️]`: 어떠한 문제를 수정했다면 그 수정한 문제를 기록
- Test records in changes are three short lines: `- 목적 : <wanted result as a one-line label>`, `- 결과 : 성공|실패`, `- 상세 : <on failure the failing file:line, variable or branch in one line; on success the actual value>` placed under `### 구현`'s `#### 초안 (#id)` / `#### 검증 (#id)` / `#### 확인 (#id)`. `#### 완료 (#id)` is written as a numbered list of around 5 lines (`1. [📦|📬|⚙️] 한줄요약`) describing what was done without `결과` / `상세`. In `### 목표` and `### 구현` subheadings, never write raw JSON objects (`{ key: value, ... }`); summarize in one line using only the executed function and its `message`/`body` text or returned value with `▷` (return) and `⇒` (deliver) (e.g., `buildNtfyRequest("빌드 완료") ▷ "빌드 완료"`, `sendNotification("워치 테스트 알림") ⇒ /tree-watch-notify ▷ 200 ("워치 테스트 알림")`). Never write the command, the goal file, the test files or the files you checked there: `grist` keeps commands and paths in `.ai/gate/<id>.json`. A label names the result (`빈 제목 거부`), not the command. Records written before this format stay as they are.
- The project Obsidian note (Table7 `Obsidian_Id`, 초기문서 format) gets one line per day under `# 📆`: `MMDD : [version] summary` under `## YYYY / ### M월`, newest first. A later request that day replaces the summary, and `flow-wait` updates the line to the released version and each finished `기능 추가` item under `# 📍`.
- `.ai/docs/architecture.md` (domains and folder structure) and `.ai/docs/performance.md` (bottlenecks and references) are the reference documents for agents. Keep the project `AGENTS.md` under 32KB; move long structure notes to `.ai/docs`.
- `grist` writes these records itself: every Table26 write mirrors `$D`/`$I`/`$J` into the item's lists, and `$H=완료` moves the item from `# 작업` to `# 완료` and, for `기능 추가`, adds it to `# 📍`. The Stop hook runs `grist sync`, so a stage changed in the Grist web UI is recorded the same way. Do not move items or edit `# 📍` by hand.

## Flow: Input.md → goals → agents (TDD) → 검증 → hidden cases → 확인 → 완료

0. **Record the request (main).** Write the new `Input.md` (`# 요구 사항`, `# 세부 사항`) to a temporary file and run `grist request-record --project <dir> --input-file <file> --title "<10자 미만 제목>" --summary "<요청 한 줄>"`. Omit the current project/source name from item titles.
1. **Split (main).** Each `# 세부 사항` line is one requirement; requirements that touch the same files stay in one row. `task-add --project <dir> --type <유형>` one row per requirement with `$H=초안`, and set the project `AGENTS.md` header to `- Table26 task Raw ID: <id>, <id>, …`.
2. **Concretize the goal (main).** For each row write the detailed goal as one-line summaries using `[연결|기능|수정]` tags and `▷` (return) / `⇒` (deliver) symbols (write only the `message`/`body` or core value, never raw JSON objects; do not write `변형:` lines). Use only what the request states; ask the user only when two readings lead to different results. Write it with `grist task-goal <id> --project <dir> --goal-file <file> --summary "<one line>"`: the lines go to the item's changes `### 목표`, the summary to `$K`; its format/scope failure is the next edit.
3. **Assign (main).** `spawn_agent` per row with `task_name` exactly `t26_<id>` and `fork_turns="none"`. The prompt holds only: project dir and "run `~/ai/scripts/bin/read-architecture --project <dir> --query <goal keywords>` first, then read its `AGENTS.md`"; `Table26 #<id>` and "read the goal in `.ai/changes` `## … (#<id>)` → `### 목표`"; the editable paths (sibling projects, global settings, skills and other rows' files are forbidden); the agent procedure below. Rows run in parallel only when their editable paths do not overlap.
4. **Agent procedure (TDD).**
   1. Read the goal (changes `## … (#<id>)` → `### 목표`; legacy rows: `Input.md` `### 목표 (#<id>)` or `$K`); never edit it.
   2. Create or adjust the function skeleton (record `- [▷] ...` and `#### 초안 (#<id>)` under `### 구현`), then write a test that asserts the goal lines on real inputs and prints the actual results.
   3. `grist task-red <id> --project <dir> --purpose "<wanted result label>" --test "<cmd>" --tests <test files> --paths <editable paths>`; it accepts only an assertion failure, records `#### 초안 (#<id>)` (목적/결과/상세) under `### 구현` and sets `$H=작업`.
   4. Implement and run unit tests (set `$H=교정` during unit testing, record `- [▶] ...` and `#### 검증 (#<id>)` under `### 구현`); on an error or failing boundary set `$H=진단` with `$I`/`$J` (Failure loop below). Compute results from inputs; never return goal expected values as literals.
   5. `grist task-submit <id> --project <dir>` writes `$H=검증` only when the red record exists, the test passes, nothing outside the editable paths changed, and the output meets the goal. Its failure output is the next repair.
   - The SubagentStop hook blocks the agent until its row is `검증`. Agents change only sources and tests: no version bump, build, version history or deploy copy. They do not ask the user, spawn agents, write the goal/`확인`/`완료`, or touch other rows; they return cause, changed files and test summary as text.
5. **Hidden cases (main).** After a row is `검증`, run a case the agent never saw (different input, boundary, empty value) with `grist task-hidden <id> --project <dir> --purpose "<wanted result label>" --test "<command with a new input>"`. Record under `### 구현`:
   ```
   #### 확인 (#<id>)
   - 목적 : <wanted result label>
   - 결과 : 성공|실패
   - 상세 : <함수명(message/body) ▷ 반환값 또는 실패 위치 한 줄>
   ```
   A failure is reassigned to the same agent row as one bounded repair.
6. **Flow test (main).** When every row is `검증`, first release once as the project `AGENTS.md` and `/home/tree/extensions/AGENTS.md` say (version bump, build, version history, deploy copy, reload into a running desktop app). Then run the whole flow once through the real artifact and authoritative consumer (see Verification order), record `- [○] ...` and `- [●] ...` under `### 구현`, and append under `#### 확인 (#<id>)`:
   ```
   #### 확인 (#<id>)
   - [x] <목적: wanted result label>
     결과 : 성공
     상세 : <실행한 함수(message/body 내용) ⇒ 전달 대상 ▷ 반환값/message 한 줄 요약 (JSON 원문 제외)>
   ```
   Then `grist flow-wait --project <dir>`. It reruns submitted and hidden tests (failures go to `진단`), checks the flow record, and writes `$H=확인` with the `[사용자]` lines in `$D`. Ask the user only now, for those lines: the final report after `flow-wait` always ends by asking the user to check them (or the delivered behaviour when there are none) and saying the rows become `완료` once confirmed.
7. **Complete (main).** When the user confirms (in this or a later session, including "tested, it works"), first handle the header rows still at `확인`: append per row under `### 구현` as a numbered list of around 5 lines (`1. [📦|📬|⚙️] 한줄요약`, no `결과` / `상세`):
   ```
   #### 완료 (#<id>)
   1. [📦] <새롭게 생성된 도메인, 기능 단어 리스트>
   2. [📬] <새롭게 연결된 외부 사이트, API>
   3. [⚙️] <수정한 문제 한줄요약>
   ```
   and run `grist task-complete <id> --project <dir>`.

A single small request may be one row handled by the main session itself with the same commands. Judgments inside the gates come from `access-jev`; they are not a separate step.

Markdown change records, project `AGENTS.md`, `Input.md`, Table26 state, command output, and the product's real consumer are the work records.

## Verification order

For each item use the same scenario and input through `focused/unit → production-boundary integration → built artifact`. Do not run the whole-flow runtime per item. After every row is `검증`, run one project-level runtime flow through the real artifact and authoritative consumer. A mock, source inspection, build, or intermediate acknowledgement is not completion evidence for a user-visible item.

If a user-only action remains (login, payment/cost, irreversible external send, OS app action), set `$H=확인` and write the reason in `$D` before asking; prepare the exact single action and expected result, and request it only after all agent-executable checks are complete. Any question to the user also requires `$H=확인` first.

## Completion gate

End-of-turn verification is owned by the `end-project` Skill. When a row or the whole flow is finished (steps 4.5, 6 and 7 above), run `end-project`; it lists the end conditions that `~/ai/scripts/bin/completion-gate` enforces and the gate commands that rerun the tests, take the Jev judgment and update Grist.

## Renames

This section is the single owner of the rename rule. No other Skill or rule file repeats it. When the user renames a name, feature name, function name, folder, path, skill or plugin, change every occurrence in one pass: product files, sibling consumers, manifests and display names, deploy artifact names (delete old-name artifacts in the deploy target), the Obsidian note and Table7 `C`/`Obsidian_Id`. Do not keep the old name, an alias or a legacy path for compatibility, and do not warn about reinstall or compatibility. Then run the tests. A failure means an external reference still uses the old name, so fix that reference instead of restoring the old name. Only historical evidence lines in `.ai/changes` keep the old name.

## Failure loop

This loop also covers reported errors, regressions, and features that did not work.

1. Record the observation and diagnosis only in the current task's Table26 row: `$I`(오류) and `$J`(원인). Each of `$D`, `$I` and `$J` holds one line: the latest item with its number in this task (for example `3. 인증 단계 확립`), replacing the previous line. The full numbered list 1..N with details goes only into the dated `.ai/changes` record under the same numbers. Do not create a separate error row or change `$G` for this. Create a task row only when the work has none, and point the project `AGENTS.md` task Raw ID at it.
2. Observe before editing code. Read the real runtime first (console exceptions, rendered screen, the actual state value on the failing path) and write `### 오류` in `.ai/changes` as a one-line summary of the problem and `$I`(오류, Text) as copied command output, not inference.
3. Write `### 원인` in `.ai/changes` and `$J`(원인, Text) as `[유실|문자|인증] 한줄로 요약 : {리턴된 에러메시지 Body}` (`[유실]`: missing/misrouted expression or value, `[문자]`: string/parser/type error, `[인증]`: CORS/auth delivery failure). When one symptom has several causes, number them in the `.ai/changes` record and keep only the latest numbered cause in `$J`.
4. Stage gates: `작업/교정→진단` records `$I` and `$J`. `진단/교정→검증` (`task-submit`) only when the `$J` test fails on the pre-fix code and passes after the fix. A mock or fixture must use the input shape captured in `$I`; a shape never observed at runtime is not regression evidence.
5. Every early return and async exception on the user-action path logs or shows a message; a silent no-op is itself a failing boundary.

Keep the item open, record the combined reproducing state in the change record, then dispatch a new bounded repair for that boundary. Preserve adjacent workflows and do not add fields, relationships, transports, or validation artifacts outside the request.

Keep credentials, authorization data, raw private payloads, and full private paths out of logs. External API diagnostics use operation ID, owner, operation, boundary, attempt, result, and thread role.
