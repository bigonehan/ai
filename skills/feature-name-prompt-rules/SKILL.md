---
name: feature-name-prompt-rules
description: FEATURE_NAME 출력 규칙을 단일 스킬로 정의하고, 프롬프트에서 공통으로 참조할 때 사용한다.
---

# Feature Name Prompt Rules

## Prompt Snippet
FEATURE_NAME 규칙:
- 출력은 정확히 한 줄만 허용: `FEATURE_NAME: <name>`
- `<name>`은 영문 소문자 `snake_case`만 허용
- 반드시 `동사_명사` 형태를 사용
- 공백, 하이픈(`-`), 한글, 설명 문장을 포함하지 않는다
- `_`는 최소 1개 이상 포함한다
- 예시: `render_cube`, `select_project`, `load_preset`

## Usage
- FEATURE_NAME을 생성하거나 정규화하는 모든 프롬프트는 위 `Prompt Snippet`을 그대로 포함해야 한다.
- 규칙이 변경되면 스킬 파일만 수정하고, 코드 프롬프트는 스킬 내용을 다시 읽어 자동 반영한다.
