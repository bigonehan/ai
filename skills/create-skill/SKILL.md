---
name: create-skill
description: Create or edit a Skill or hook under ~/ai/skills or ~/ai/codex/hooks*, keeping the originals in ~/ai and linking them into ~/.codex, ~/.claude and ~/.gemini/config. Use whenever the user asks to add, rename, move, or change a Skill or hook. Not for global rule text (use change-rules).
---

# Create Skill

## 위치와 이름

- Skill 원본은 `~/ai/skills/<name>/SKILL.md`, hook 원본은 `~/ai/codex/hooks*`에 둔다. 원본만 고친다.
- Skill 이름과 폴더명은 `동사-명사`(예: `access-grist`)로 짓는다.
- `SKILL.md`는 frontmatter(`name`, `description`)로 시작한다. `description`에 언제 쓰는지와 쓰지 않는 경우를 적는다.

## 심볼릭 링크

- 새 Skill은 `~/.codex/skills/<name>`·`~/.claude/skills/<name>`·`~/.gemini/config/skills/<name>`에서 원본 `~/ai/skills/<name>`을 가리키는 심볼릭 링크로 묶는다. 복사하지 않는다.
- hook도 `~/.codex`·`~/.claude`에서 원본 `~/ai/codex/hooks*`를 가리키는 심볼릭 링크로 묶는다.
- 이름을 바꾸거나 지우면 세 위치의 링크도 함께 바꾸거나 지운다.

## 스크립트

- 스크립트는 Rust(`.rs`)로만 만든다. `.py`·`.mjs`는 만들지 않고 기존 파일은 지운다(`stagehand.mjs` 예외).
- Skill 전용 스크립트는 `<Skill>/scripts/bin/`에 둔다. 공용은 `~/ai/scripts/bin/`에 둔다.
