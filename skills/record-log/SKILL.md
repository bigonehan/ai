---
name: record-log
description: Give external API calls a traceable project/row/agent ID and emit a short, credential-safe one-line result. Use when recording an external API call or tracing its caller.
---

# Record Log

Never write tokens, `Authorization` values, raw request/response payloads, or full personal paths into any log line.

An external API log line has five fields: `HH:MM:SS | ID | calling function | attempt | result` (Asia/Seoul). Add the time at the `append` or product recording boundary, not in caller code; `log-line` validates the four-field body. Use a two-digit attempt (`01`, `02`, …). The ID has 11 characters: six decimal project digits, four decimal Table26 row digits, and one agent character from `0-9A-HJ-NP-Z` (no `I` or `O`). A result is one line of at most 120 characters. Do not include a JSON object/array, token, Authorization header, original payload, or private full path. Keep those values out of the calling function field as well.

For an external API integration, put `ack=<received|rejected|expired|not_observed|not_sent|not_applicable>`, the body fields needed to identify the result (for example `body.status=failed` or `body.code=invalid_request`), and `error=<sanitized error message|none>` in `result`. Record `body=none` when no response body arrived. Preserve the meaningful error message after masking credentials and private paths; do not replace it with only a generic success/failure label. Never log the whole JSON body, a serialized request/response object, or raw body text. `ack=not_observed` means the ACK was not seen and does not claim that the receiver rejected or missed the request.

Fill `ack`, `body.*` and `stage` only from the value actually observed at that call; never write them as a constant string in the logger (for example a hard-coded `ack=not_observed body=none`). When the caller fails before sending, record `ack=not_sent`. Put `stage=<local_validate|send|receiver_reject|response_validate>` in every failure `result` to name where it failed: `local_validate` for the caller's own check before sending, `send` for a transport or timeout failure, `receiver_reject` for an error returned by the receiver, `response_validate` for the caller rejecting the received response.

A receiver on the other side of the boundary (for example a Web Prompt Bridge handler) logs each handled request in its own internal log, success and failure alike, with its own calling function name, `body.status=<ok|failed>`, the failing `stage` and `error=`. A receiver that only returns the error to the caller without logging it does not meet this rule.

A rejection names the observed shape that failed the check, not only a label: the request `path`, `len=<Content-Length>`, `ctype=<Content-Type|none>` and the parse error (for example `body.status=invalid_body len=0 ctype=none error=Expecting value stage=local_validate`). An unknown-route response and its log carry the requested path and the receiver's own version (`error=route_not_found path=/queries/conti-folder version=0.0.34`), so a stale installed Bridge shows in one line. Two different causes never share one error string, and every local validation that rejects a received value logs one line (261031 blender_bridge: `invalid_body` hid a body-less claim, `요청 경로를 찾지 못했습니다` hid a stale Eagle plugin, and `생성 이미지 형식 오류` had no log line).

## Commands

Run the prebuilt Rust CLI with `R=~/ai/skills/record-log/scripts/bin/record-log`:

```bash
$R log-line ID CALLING_FUNCTION ATTEMPT RESULT
$R append ID CALLING_FUNCTION ATTEMPT RESULT [--project PROJECT_DIR]
$R find ID [--project PROJECT_DIR]
$R parse ID --project PROJECT_DIR
$R latest [--project PROJECT_DIR] [--fn NAME] [--since HH:MM]
```

`dispatch-agent` runs `start` automatically when spawning a worker, records `N. <11-char ID> · <session>` in the row's changes `### 할당` through `grist task-memo ROW --line '<ID> · <session>'` (never touching `### 메모` or Table26 `$D`), and attaches `에이전트 ID: <ID>` to the worker prompt. Agents do not call `start` or `task-memo` directly.

`parse` and `find` read the row's `### 할당` through `grist task-show ROW --section 할당` to resolve the 11-character ID and session identifier; `find ID` also prints up to 20 matching timestamp-prefixed `.ai/logs/*.log` lines (at most 200 characters each).

`log-line` validates all four fields and prints the formatted line; it exits with code 2 and prints an error when the result starts with `{` or `[`, exceeds 120 characters, spans lines, contains a field separator, or carries common credential markers. Pass the line to the existing caller's logger. The command itself does not send it anywhere.

`append` runs the `log-line` validation, adds `HH:MM:SS` from the same Asia/Seoul clock reading used for the date, and appends the line to `<project dir>/.ai/logs/YYMMDD.log` (directory created). Without `--project`, the project dir is found by the six project digits in the `### grist` `ID :` header (first 8 lines) of `AGENTS.md` under `/home/tree/extensions/*` and `/home/tree/test/*`; `--project PROJECT_DIR` skips the scan. Implementers (`role-agent`) and verifiers (`role-verifier`) never block or stall their own work to write or wait on logs; log recording and inspection are delegated to an asynchronous `record-log` Skill / agent call while implementers and verifiers continue and finish their own tasks immediately.

`latest` reads today's Asia/Seoul log. Without `--project`, it uses the cwd `AGENTS.md` Grist ID to locate the project. `--fn` selects one calling function and `--since HH:MM` includes lines from that local time onward. It groups by ID and function, sorts by latest time descending, and prints at most 20 lines of 200 characters. Each line is `<last time> | <ID> | <function> | last=<ok|fail> stage=<value|-> ack=<value|-> error=<value|-> last_ok=<time|-> fails=<count>`. `error=none` means success; other values mean failure. A failure without `stage` adds `warn=no_stage`. Two or more failures with the same observed `ack` and `body` tokens add `warn=fixed_ack_body`.
