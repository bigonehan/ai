---
name: check-code
description: "입력된 기능/목표 기준 체크리스트 생성 후 언어별 실행 검증을 수행하는 skill"
---

# Check Code

## 원칙
- 먼저 `input.md` 또는 `./.project/draft.yaml`에서 기능/목표를 읽는다.
- 위 입력을 바탕으로 `./.project/check_list.md`를 생성/갱신한다.
- 체크 항목 형식은 반드시 아래 한 줄 패턴만 사용한다:
  - `- [ ] ${입력} -> ${출력} : 기능설명`

## 입력 우선순위
1. `input.md`가 있으면 우선 사용한다.
2. `input.md`가 없고 `./.project/draft.yaml`이 있으면 이를 사용한다.
3. 둘 다 없으면 실패로 보고하고 생성/검증을 중단한다.

## check_list.md 작성 규칙
- 각 기능/목표를 검증 가능한 입력/출력 단위로 분해해 항목을 만든다.
- 체크리스트 파일 경로는 고정: `./.project/check_list.md`
- 항목은 미완료 상태(`- [ ]`)로 기록한다.

## 실행 검증 규칙
- 현재 코드 언어를 확인하고 아래 명령 체계를 사용한다.
- Rust:
  - `cargo test`
  - 필요 시 `cargo check`
- JS/TS 계열:
  - `vitest` 기반 테스트 실행
  - 라우팅/폼 제출 같은 사용자 흐름은 `playwright` E2E 실행
- 구현 함수의 반환값이 실제 로직 없이 고정값인지 반드시 점검한다.
  - 예: `Ok(false)`, `Ok(true)`, `return false`, `return true` 같은 하드코딩 반환
  - 입력값/상태/외부결과를 사용하지 않는 고정 반환이면 검증 실패로 기록한다.

## 완료 보고 규칙
- 검증 결과는 `report.md`에 기록한다.
- `report.md` 헤더는 아래 2개만 사용한다:
  - `# 구현 확인`
  - `# 발견된 문제`
