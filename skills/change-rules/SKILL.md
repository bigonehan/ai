---
name: change-rules
description: Read or change the global agent rules through the `agent-rules` CLI. Use whenever the user mentions 전역 규칙·전역설정·"앞으로" rules, or asks to read, add, replace, or remove a line of the global rules source `~/ai/rules/Agents.md`. Not for project `AGENTS.md` edits.
---

# Change Rules

전역 규칙은 원본 `~/ai/rules/Agents.md` 하나로만 관리한다. 생성본(`~/.codex/AGENTS.md`·`~/.claude/CLAUDE.md`·`~/.gemini/GEMINI.md`·`~/.gemini/antigravity-cli/GEMINI.md`)은 직접 고치지 않는다.

## 어느 파일을 바꾸는가

- "전역규칙/전역설정"·"앞으로"가 있을 때만 원본을 바꾼다.
- 일반 "AGENTS.md 수정"은 가장 가까운 프로젝트 `AGENTS.md`만 바꾼다.
- 프로젝트 전용 규칙은 그 프로젝트 `AGENTS.md`에 둔다.

## 읽기·고치기

원본은 `~/.local/bin/agent-rules`로만 읽고 고친다.

- 읽기: `read [--section <제목>]`, `sections`
- 고치기: `add --section <제목> --text <줄>`, `replace --old <A> --new <B>`, `remove --text <A>`, `write --file <F>`
- 수정하면 생성본이 자동 갱신된다. 결과는 `agent-rules status`로 보고한다.
- `sync` exit 3이면 `~/ai/rules/.state/backup/` 내용을 원본에 병합하고 `sync --force`한다.

## 점검

- 원본은 8KB 이하로 유지한다(`sync`가 경고한다). 넘으면 절차를 Skill로 옮긴다.
- 사용자가 "앞으로"라고 지적한 내용은 현재 작업만 고치지 않고 이 절차로 전역 규칙에도 반영한다.
