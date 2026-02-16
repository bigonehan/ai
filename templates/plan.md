---
name: <이름>
description: <간단한 설명>
features:
  - <기능 1>
  - <기능 2>
libs:
  - <라이브러리@정확한버전>  # 필수 - 재현성 보장
  - <라이브러리@버전> <!-- 선택 이유, 대안과 비교 -->
design:
  architecture: <전체 구조 (레이어/모듈 관계)>
  patterns: <적용할 디자인 패턴>
  data_flow: <데이터/제어 흐름 핵심>
  interfaces: <주요 API/경계 정의>
warnings:
  - <금지사항 1>
constraints:
  - <기술적 제약/성능 목표>
dependencies:
  external: <외부 서비스/API>
  internal: <선행 작업/모듈>
---

## Goals (완료 정의)
- <조건 1> ← **검증 방법**: <명령/테스트>
- <조건 2> ← **검증 방법**: <how>

## Non-goals (이번에 안 함)
- <제외 1> <!-- 이유 또는 미래 버전 -->

## Files to touch
- **Create**:
  - `path/to/new_file`: <역할> | <핵심 책임>
- **Modify**:
  - `path/to/existing_file`: <변경 요약> | <영향 범위>
- **Delete** (필요시):
  - `path/to/deprecated`: <삭제 이유>

## Milestones
### 1) <마일스톤 1>
**Exit criteria**: <이 단계 완료 조건>
- [ ] <작업 1>
  - **Verify**: `<검증 명령>`
  - **Rollback**: <실패 시 되돌리기>
- [ ] <작업 2>
  - **Verify**: ...

### 2) <마일스톤 2>
**Exit criteria**: ...
- [ ] ...

## Risks & Mitigation
- **Risk**: <위험 1> → **Mitigation**: <대응책>
- **Risk**: <위험 2> → **Mitigation**: ...

## Open questions
- [ ] <질문 1> — **Blocker**: YES/NO
- [ ] <질문 2> — **Blocker**: YES/NO

## Decision log
- `2025-02-16`: <결정> — <이유> | <대안들>
