---
name: manage-global-rules
description: Own the global agent rules and settings — read or change `~/ai/rules/Agents.md` through the `agent-rules` CLI, apply settings to Claude·Codex·Gemini together, and handle the user's complaints about global rules or settings by first finding the existing rule and enforcing it with a hook or script instead of adding text. Use whenever the user mentions 전역 규칙·전역설정·설정·"앞으로" rules, points out that a rule or setting was not followed, or asks to read, add, replace, or remove a global rule line. Not for project `AGENTS.md` edits.
---

# Manage Global Rules

전역 규칙은 원본 `~/ai/rules/Agents.md` 하나로만 관리한다. 생성본(`~/.codex/AGENTS.md`·`~/.claude/CLAUDE.md`·`~/.gemini/GEMINI.md`·`~/.gemini/antigravity-cli/GEMINI.md`)은 직접 고치지 않는다.

## 1. 전역 규칙·설정 지적 처리 (가장 먼저)

사용자가 전역 규칙이나 설정이 지켜지지 않았다고 지적하거나 바꾸라고 하면 문구를 추가하기 전에 아래 순서를 따른다.

1. **기존 규칙 찾기:** `agent-rules read`·`rg -n "<키워드>" ~/ai/rules/Agents.md ~/ai/skills`로 같은 내용의 규칙이 전역 규칙·Skill에 이미 있는지 확인한다.
2. **이미 있으면 강제 수단으로 바꾼다:** 같은 문구를 다시 쓰거나 강조하지 않는다. 그 규칙을 기계적으로 막거나 수행하는 hook·script를 찾아 고치거나 새로 만든다.
   - 후보: PreToolUse 거부(`~/ai/scripts/main-edit-gate`·`agy-hook`), Stop 판정(`task-complete`), CLI 검증(`grist`·`agent-rules`·`dispatch-agent`의 인자·상태 검사), 알림(`ask-notify`).
   - 강제가 들어가면 그 규칙 문구는 전역 규칙에서 지우거나 "hook이 거부한다" 한 줄로 줄인다(예: `nf` 직접 실행 금지 → `main-edit-gate`가 거부).
   - hook·script 변경은 코드 변경이므로 `create-skill`의 위치·링크 규칙과 `manage-project` 위임 판정(구현은 `dispatch-agent` 대행자, 빌드는 위임자, hook 실측은 검증자 `--role verifier`)을 따른다. 실제 hook 환경에서 거부·통과 사례를 확인한다.
   - 기계적으로 판정할 수 없는 규칙(말투·판단 기준 등)만 문구 수정으로 처리하고, 그 이유를 보고에 한 줄로 적는다.
3. **없으면 추가한다:** 가장 좁은 소유 위치(프로젝트 `AGENTS.md` → 해당 Skill → 전역 규칙 순)에 한 줄로 넣는다. 강제 가능한 내용이면 2번의 hook·script를 함께 만든다.

## 2. 세 에이전트 공통 적용

- 설정·hook·규칙 변경 요청은 Claude·Gemini·Codex에 공통 적용할 수 있는 부분을 따로 묻지 않고 세 곳에 함께 적용한다.
  - 규칙: `agent-rules`가 생성본 4개를 함께 갱신한다.
  - Skill: `~/ai/skills/<name>` 원본을 `~/.claude/skills`·`~/.codex/skills`·`~/.gemini/config/skills`에 링크한다.
  - hook: `~/.claude/settings.json`, `~/ai/codex/hooks.json`(Codex), `~/ai/codex/hooks.agy.json`(Gemini, `agy-hook` 변환)에 같은 판정을 등록한다.

## 3. 어느 파일을 바꾸는가

- "전역규칙/전역설정"·"앞으로"가 있을 때만 원본을 바꾼다. "앞으로" 지적은 현재 작업만 고치지 않고 이 절차로 전역 규칙에도 반영한다.
- 일반 "AGENTS.md 수정"은 가장 가까운 프로젝트 `AGENTS.md`만 바꾼다. 프로젝트 전용 규칙은 그 프로젝트 `AGENTS.md`에 둔다.
- 2번 넘게 남은 오류나 구조적 특성은 프로젝트 `AGENTS.md` `# tip`에 한 줄로 추가한다.
- 스크립트는 Rust(`.rs`)로만 만들고(`stagehand.mjs` 예외) 기존 `.py`·`.mjs`는 지운다. Skill·hook을 만들거나 고칠 때는 `create-skill`을 따른다.
- `jj` 전역설정은 workspace 초안·테스트·보고까지만 하고, 사용자 전역설정 병합은 사용자가 위임한 경우에만 한다.

## 4. 읽기·고치기

원본은 `~/.local/bin/agent-rules`로만 읽고 고친다.

- 읽기: `read [--section <제목>]`, `sections`
- 고치기: `add --section <제목> --text <줄>`, `replace --old <A> --new <B>`, `remove --text <A>`, `write --file <F>` (규칙 문구에 백틱(`` ` ``)이나 `$`가 있을 때 큰따옴표 `"..."` 인자로 넘기면 셸 명령 치환으로 예시 명령이 실제 실행되므로 반드시 작은따옴표 `'...'` 또는 `write --file`로만 전달한다)
- 전역 규칙 텍스트 변경은 메인 세션이 `agent-rules`로 직접 수정하고 원본·생성본과 `agent-rules status`로 반영을 확인한다. 전역 규칙 또는 Skill 절차 텍스트(`SKILL.md`·`references/*.md`)를 바꾼 뒤에는 다른 하위 프로젝트 `AGENTS.md`에 변경된 지시와 충돌하는 명시적 기준이 있는지만 검사한다. 충돌이 없으면 결과를 `task-confirm`에 기록하고 `Complete_task`의 목표·증거 판정으로 `$H=확인`에 전환하며 `$H=검증`을 거치지 않는다. 충돌이 있으면 해당 기준을 바로잡은 뒤 전환한다. 이 행에는 Red 확인·`task-red`·`task-submit`을 적용하지 않는다. 제품 코드·CLI·hook·Skill 스크립트 변경에는 이 예외를 적용하지 않는다.
- 수정하면 생성본이 자동 갱신된다. 결과는 `agent-rules status`로 보고한다.
- `sync` exit 3이면 `~/ai/rules/.state/backup/` 내용을 원본에 병합하고 `sync --force`한다.

## 5. 점검

- 원본은 8KB 이하로 유지한다(`sync`가 경고한다). 넘으면 절차를 Skill로 옮긴다.
