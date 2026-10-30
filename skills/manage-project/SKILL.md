---
name: manage-project
description: Route /home/tree/extensions project requests by role and Table26 stage. Use for project work and initialization; skip explanation-only questions with no file or Grist change.
metadata:
  short-description: Initialize, plan, delegate and close project work
allowed-tools: Bash(~/ai/scripts/bin/read-architecture:*)
---

# Manage Project

## 역할·입구

- `T26_ROLE=worker` 또는 `t26_<id>` 에이전트는 구현자다. 자기 행(`T26_ROW` 또는 `<id>`)만 다루고 `references/stage-work.md`만 따른다. 요청·목표 작성, 다른 에이전트 호출, 마감 명령은 맡지 않는다. 나머지 메인 세션은 오케스트레이터다.
- 메인 세션은 첫 `/home/tree/extensions` 작업이나 다른 프로젝트로 옮길 때 `initial-task/SKILL.md`를 함께 읽어 소유 프로젝트·순수 질문·기존 행을 판정한다. 프로젝트 `AGENTS.md`의 `# 상태`에 Grist ID·Obsidian 문서가 없으면 `references/initialize.md`로 초기화한다.
- 사용자 지적의 원인 경계와 이미 확인한 `row` 줄을 우선 사용한다. Legacy·레거시·구형 제거 또는 새 형식 갱신은 `remove-legacy`를 함께 적용한다.

## 하위 진입 표

| 진입 상태 | 카드 |
|---|---|
| 진입 전·같은 세션 후속 요청 | `initial-task/SKILL.md` |
| 미초기화 | `references/initialize.md` |
| `초안`·`진단` | `references/stage-start.md` |
| `작업`·`교정` | `references/stage-work.md` |
| `검증` → `확인` | `references/stage-verify.md` |
| 위임·요청 대조 | `references/plan.md` |

각 단계 명령 출력의 `next`가 다음 카드 경로를 알려준다(행 500).

기록 형식은 grist CLI가 검사하므로 오류 메시지를 따른다.
