---
name: feature-architecture-rules
description: 공통 아키텍처/네이밍 규칙을 정의한다. 파일 생성/명령 네이밍은 여기의 단일 규칙을 따른다.
---

# Feature Architecture Rules

## Naming Rule
- 코드명/명령명 기본 네이밍은 `동사-형용사-명사`를 우선 사용한다.
- 형용사가 불필요하면 `동사-명사`를 사용한다.
- 예시: `build-parallel-code`, `create-draft`, `select-project`.

## Function Naming Rule
- 함수명 기본 접두사는 아래를 우선 사용한다.
- `create_`, `add_`, `enter_`, `get_`, `set_`, `filter_`, `convert_`, `update_`, `remove_`, `load_`, `save_`, `flow_`
- 새 함수를 만들 때는 위 접두사로 시작하고, 뒤에 목적 명사를 snake_case로 붙인다.


## File Naming Rule
- 파일명 규칙은 함수명 규칙과 분리해 관리한다.
- 기본 규칙:
  1. `명사_동사` 순서를 사용한다.
  2. `snake_case` 형태를 사용한다.

## Usage
- 파일 생성/수정 관련 스킬은 이 문서의 네이밍 규칙을 공통으로 참조한다.
- 개별 스킬 문서에는 규칙 전문을 중복 복사하지 않고, 참조 한 줄만 유지한다.
