---
name: access-jev
description: TypeSafe Jev judgments (goal scope J3, output-vs-goal match J1, final-message classification J2) for the Table26 gates. Run inside access-grist `grist` and the Codex completion hook; Codex does not call it directly.
metadata:
  short-description: Jev judgments behind the Table26 gates
---

# Jev Access

`~/ai/scripts/bin/jev` sends a state and typed questions to TypeSafe Jev (`POST https://api.typesafe.ai/v1/systemone`) and prints a JSON verdict. It is a backend of the gates, not a step in the work procedure:

| Caller | Command | Judgment |
|---|---|---|
| `grist task-goal` | `scope --request-file F --goal-file G` | **J3** every goal line is grounded in the newest changes `# 요청` block, and the request is fully covered |
| `~/ai/scripts/bin/task-complete` | `match --goal-file G --output-file O` | **J1** the actual output satisfies each `[연결]`/`[기능]`/`[수정]` line; `Complete_task` alone owns the completion judgment |
| `~/ai/scripts/bin/task-complete` (main Stop) | `stop --message-file M` | **J2** final message is 완료 보고 / 사용자 질문 / 실패를 결론으로 종료 / 진행 보고 / 질문 답변 / 대기 보고, and, only for 사용자 질문, whether it asks for one of the four allowed actions, a `.py` consent naming path and reason, or a choice between concrete interpretations before editing |

Doc-only rows are decided in code, not by the model: a J1 goal line, or a J3 request whose goal lines all name a doc·rule target (`AGENTS.md`·`CLAUDE.md`·`SKILL.md`·`.ai/docs`·`Skill`·전역 규칙/설정), gets an instruction without the product-runtime clause (`doc_rule_marker` in `jev/src/main.rs`). Mixed or product goals keep it.

Do not run `jev` as part of project work and do not paste its output into Grist; the gates record a one-line summary in `.ai/changes`.

## Configuration

`.env` in this skill directory (mode 600, excluded from Git; template `.env.example`):

- `TYPESAFE_API_KEY` — entered by the user; never ask for it, print it, or put it in commands, logs or records.
- `JEV_MODEL` (`jev-latest`), `JEV_PASS` (0.8), `JEV_FLOOR` (0.5).

A Noul value or Choice confidence `>= JEV_PASS` is `pass`, `< JEV_FLOOR` is `fail`, otherwise `uncertain`. `jev` always runs in `enforce` mode: J3 rejects on `fail`, J1 requires every goal/request item to be `pass`, and J2 blocks progress/waiting reports and disallowed user questions.

## Output and failures

stdout: `{"ok": true, "mode", "verdict": "pass|uncertain|fail", "command", "items": [{name, line, value, verdict}], "usage"}`.

Errors go to stderr as JSON with a `boundary` (`config`, `connection`, `http_<code>`, `arguments`) and exit 2. Callers treat any failure as "judgment skipped" (fail-open) and continue.

States are masked (`Bearer …`, `*_KEY=…`, home path → `~`) and cut to 12,000 characters before sending. Every judgment is appended to `~/.codex/tmp/jev/judgments.jsonl` with the state's byte length (never the text).

`JEV_FAKE_RESPONSE=<json file>` replaces the API call with a canned response; use it only in tests.
