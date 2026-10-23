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
- 전역 규칙 텍스트 변경은 메인 세션이 `agent-rules`로 직접 수정하고 원본·생성본과 `agent-rules status`로 반영을 확인한다. 전역 규칙 또는 Skill 절차 텍스트(`SKILL.md`·`references/*.md`)를 바꾼 뒤에는 다른 하위 프로젝트 `AGENTS.md`에 변경된 지시와 충돌하는 명시적 기준이 있는지만 검사한다. 충돌이 없으면 결과를 `task-confirm`에 기록하고 `Complete_task`의 목표·증거 판정으로 `$H=확인`에 전환하며 `$H=검증`을 거치지 않는다. 충돌이 있으면 해당 기준을 바로잡은 뒤 전환한다. 이 행에는 Red 확인·`task-red`·`task-submit`을 적용하지 않는다. 제품 코드·CLI·hook·Skill 스크립트 변경에는 이 예외를 적용하지 않는다.
- 수정하면 생성본이 자동 갱신된다. 결과는 `agent-rules status`로 보고한다.
- `sync` exit 3이면 `~/ai/rules/.state/backup/` 내용을 원본에 병합하고 `sync --force`한다.

## 점검

- 원본은 8KB 이하로 유지한다(`sync`가 경고한다). 넘으면 절차를 Skill로 옮긴다.
- 사용자가 "앞으로"라고 지적한 내용은 현재 작업만 고치지 않고 이 절차로 전역 규칙에도 반영한다.
