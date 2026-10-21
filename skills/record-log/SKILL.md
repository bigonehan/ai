---
name: record-log
description: Give external API calls a traceable project/row/agent ID and emit a short, credential-safe one-line result. Use when recording an external API call or tracing its caller.
---

# Record Log

An external API log line has four fields: `ID | calling function | attempt | result`. Use a two-digit attempt (`01`, `02`, …). The ID has 11 characters: six decimal project digits, four decimal Table26 row digits, and one agent character from `0-9A-HJ-NP-Z` (no `I` or `O`). A result is one line of at most 120 characters: `ok`, an error type, or a short message. Do not include a JSON object/array, token, Authorization header, original payload, or private full path. Keep those values out of the calling function field as well.

## Commands

Run the Rust CLI with `R="cargo run --quiet --manifest-path ~/ai/skills/record-log/scripts/record-log/Cargo.toml --"`:

```bash
$R start PROJECT_DIR ROW --role NAME [--conversation ID] [--caller CHAR NAME]
$R parse ID --project PROJECT_DIR
$R log-line ID CALLING_FUNCTION ATTEMPT RESULT
$R append ID CALLING_FUNCTION ATTEMPT RESULT [--project PROJECT_DIR]
$R find ID [--project PROJECT_DIR]
```

`start` (the caller runs it right after `invoke_subagent`; the subagent does not) allocates the next free agent character from `1` (`1`-`9`, `A`-`Z` without `I`/`O`: 33 sub-agent characters; exit 2 when exhausted); `0` is issued only with `--role main` (exit 2 if an `에이전트 0 :` line already exists), records one line `에이전트 <char> : <role> · <conversation>` (plus ` ← <caller char> <caller name>` with `--caller`; main is `0 main`) in that row's changes `### 메모` through `grist task-update ROW --fields '{"D": ...}'` (never edits the changes file), and prints the 11-character ID. The project digits come from the `ID :` in `PROJECT_DIR/AGENTS.md`. A per-row lock serializes parallel starts. Give the conversation's stable reference, not its message body.

`grist task-red --agent-id <ID>` uses the printed ID: it skips when 메모 has that agent line and only warns otherwise; without it task-red calls `start` once and prints `agent-id: <ID>`.

`parse` reads the row's `### 메모` through `grist task-show ROW --section 메모` and prints project, row, agent, role, and conversation. There is no separate state file.

`log-line` validates all four fields and prints the formatted line; it exits with code 2 and prints an error when the result starts with `{` or `[`, exceeds 120 characters, spans lines, contains a field separator, or carries common credential markers. Pass the line to the existing caller's logger. The command itself does not send it anywhere.

`append` runs the `log-line` validation and appends only the validated line to `<project dir>/.ai/logs/YYMMDD.log` (Asia/Seoul date, directory created). Without `--project`, the project dir is found by the six project digits in the `### grist` `ID :` header (first 8 lines) of `AGENTS.md` under `/home/tree/extensions/*` and `/home/tree/test/*`; `--project PROJECT_DIR` skips the scan.

`find ID` derives the project the same way (or from `--project`), checks the row exists in `.ai/changes`, prints the agent memo line (role, conversation, caller) via `grist task-show ROW --section 메모`, then only the `.ai/logs/*.log` lines containing the ID (at most 20 lines, 200 characters each). `parse` also prints the caller when present.
