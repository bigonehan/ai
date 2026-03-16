---
name: check-code
description: "입력된 기능/목표 기준 체크리스트 생성 후 언어별 실행 검증을 수행하는 skill"
---

# Check Code

## 원칙
- 먼저 `./.project/drafts.yaml`에서 기능/목표를 읽는다.
- 위 입력을 바탕으로 검증 체크리스트를 구성하고 `./.project/feedback.md`를 단일 운영 문서로 사용한다.
- 체크 항목 형식은 반드시 아래 한 줄 패턴만 사용한다:
  - `- [ ] ${입력} -> ${출력} : 기능설명`

## 입력 우선순위
1. `./.project/drafts.yaml`을 사용한다.
2. 파일이 없으면 실패로 보고하고 생성/검증을 중단한다.

## feedback.md 반영 규칙
- 각 기능/목표를 검증 가능한 입력/출력 단위로 분해해 항목을 만든다.
- 별도 체크리스트 파일은 만들지 않는다.
- 검증 중 새로 발견한 문제는 `./.project/feedback.md`의 `# 문제`에 기록한다.
- 해결 완료가 확인된 문제는 `# 문제`에서 `# 해결`로 이동한다.
- 프로세스 자체 개선 메모는 `#개선필요`에 기록한다.

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
- 문자열/패턴 기반의 하드코딩 성공/실패 판정도 반드시 점검한다.
  - 예: `contains("Logout")`, `contains("success")`, `starts_with("ok")`, `ends_with("done")`
  - 위 조건이 입력/상태/외부결과 검증 없이 최종 판정 분기로 쓰이면 검증 실패로 기록한다.
  - 권장 점검 명령 예시:
    - `rg -n "contains\\(|starts_with\\(|ends_with\\(" src crates packages`

## 완료 기록 규칙
- 검증 결과와 체크 상태는 `./.project/feedback.md`에만 기록한다.
- 완료 시 해결된 항목은 `# 해결`로 이동하고, 남은 항목만 `# 문제`에 둔다.
