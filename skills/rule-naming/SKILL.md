---
name: rule-naming
description: 공통 네이밍 규칙과 FEATURE_NAME 출력 규칙을 단일 스킬로 통합한다. 파일/명령/함수 네이밍 및 FEATURE_NAME 정규화 프롬프트 작성 시 사용한다.
---

# Rule Naming

## Naming Rule
- 코드명/명령명 기본 네이밍은 `동사-형용사-명사`를 우선 사용한다.
- 형용사가 불필요하면 `동사-명사`를 사용한다.
- 예시: `build-parallel-code`, `create-draft`, `select-project`.

## Function Naming Rule
- 함수명 기본 접두사는 아래를 우선 사용한다.
- `create_`, `add_`, `enter_`, `get_`, `set_`, `filter_`, `convert_`, `update_`, `remove_`, `load_`, `save_`, `flow_`
- 새 함수를 만들 때는 위 접두사로 시작하고, 뒤에 목적 명사를 snake_case로 붙인다.
- `calc_`, `action_` 접두사는 사용하지 않는다(기존 코드도 점진적으로 제거).

## File Naming Rule
- 파일명 규칙은 함수명 규칙과 분리해 관리한다.
- 기본 규칙:
  1. `명사-동사` 순서를 사용한다.
  2. `kebab-case` 형태를 사용한다.

## Prompt Snippet
FEATURE_NAME 규칙:
- 출력은 정확히 한 줄만 허용: `FEATURE_NAME: <name>`
- `<name>`은 영문 소문자 `kebab-case`만 허용
- 반드시 `동사-명사` 형태를 사용
- 공백, 언더스코어(`_`), 한글, 설명 문장을 포함하지 않는다
- `-`는 최소 1개 이상 포함한다
- 예시: `render-cube`, `select-project`, `load-preset`

## Usage
- 파일 생성/수정 관련 스킬과 FEATURE_NAME 생성/정규화 프롬프트는 이 문서를 공통 참조한다.
- 규칙 변경 시 이 스킬만 수정하고, 개별 프롬프트/코드는 재참조해 반영한다.
