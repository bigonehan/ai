---
name: jev-access
description: TypeSafe Jev judgments (goal scope J3, output-vs-goal match J1, final-message classification J2) for the Table26 gates. Run inside grist-access `grist.py` and the Codex completion hook; Codex does not call it directly.
metadata:
  short-description: Jev judgments behind the Table26 gates
---

# Jev Access

`scripts/jev.py` sends a state and typed questions to TypeSafe Jev (`POST https://api.typesafe.ai/v1/systemone`) and prints a JSON verdict. It is a backend of the gates, not a step in the work procedure:

| Caller | Command | Judgment |
|---|---|---|
| `grist.py task-goal` | `scope --request-file F --goal-file G` | **J3** every goal line is grounded in the newest `Input.md` request, and the request is fully covered |
| `grist.py task-submit`, `flow-wait` (`[화면]`), `task-complete` (`근거:`) | `match --goal-file G --output-file O` | **J1** the actual output satisfies each `[값]`/`[상태]`/`[화면]` line |
| `~/.codex/hooks/completion_gate.py` (main Stop) | `stop --message-file M` | **J2** final message is 완료 보고 / 사용자 질문 / 실패를 결론으로 종료 / 진행 보고, and, only for 사용자 질문, whether the requested user action is one of the four allowed |

Do not run `jev.py` as part of project work and do not paste its output into Grist; the gates record a one-line summary in `.ai/changes`.

## Configuration

`.env` in this skill directory (mode 600, excluded from Git; template `.env.example`):

- `TYPESAFE_API_KEY` — entered by the user; never ask for it, print it, or put it in commands, logs or records.
- `JEV_MODEL` (`jev-latest`), `JEV_MODE` (`shadow` | `enforce`), `JEV_PASS` (0.9), `JEV_FLOOR` (0.5).

A Noul value or Choice confidence `>= JEV_PASS` is `pass`, `< JEV_FLOOR` is `fail`, otherwise `uncertain`. In `shadow` (default) callers only log; in `enforce` they reject on `fail`.

## Output and failures

stdout: `{"ok": true, "mode", "verdict": "pass|uncertain|fail", "command", "items": [{name, line, value, verdict}], "usage"}`.

Errors go to stderr as JSON with a `boundary` (`config`, `connection`, `http_<code>`, `arguments`) and exit 2. Callers treat any failure as "judgment skipped" (fail-open) and continue.

States are masked (`Bearer …`, `*_KEY=…`, home path → `~`) and cut to 12,000 characters before sending. Every judgment is appended to `~/.codex/tmp/jev/judgments.jsonl` with the state's sha256 (never the text) for threshold tuning during shadow mode.

`JEV_FAKE_RESPONSE=<json file>` replaces the API call with a canned response; use it only in tests.
