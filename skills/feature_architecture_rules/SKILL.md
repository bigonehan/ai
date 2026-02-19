---
name: feature_architecture_rules
description: "기능 추가 시 공통 아키텍처/네이밍/단계 규칙을 강제하는 스킬."
version: 2026-02-19
---

# Feature Architecture Rules

## Trigger
- 사용자가 기존 프로젝트에 기능 추가를 요청할 때 적용한다.
- 다른 build 계열 스킬에서 공통 규칙으로 참조할 때 적용한다.

## Core Rules
1. 시작부터 종료까지 Stage 계약(입력/출력/성공/실패)을 먼저 고정한다.
2. 단계 실행 함수는 `stage_*`, 단계 간 전환/오케스트레이션 함수는 `flow_*`를 사용한다.
3. 런타임 엔트리포인트는 언어 관례를 따르고, 오케스트레이션 진입점은 `flow_main`으로 둔다.
4. 사용자 입력 함수는 `input_*`를 사용한다.
5. 입력 파라미터는 `required`와 `optional(default)`를 분리해 정의한다.
6. 설정/템플릿은 YAML을 사용하되 필수 키/타입 검증을 둔다.
7. 사용자 설정은 초기에 로드하고 우선순위를 `default < file < env < cli`로 고정한다.
8. 동시성 작업은 상태전이, 메시지 포맷, 타임아웃, 종료 신호를 명시한다.
9. 기능 추가 전에 코드베이스에서 유사 기능/중복 구현 여부를 먼저 탐색한다.
10. `src/domain`에는 도메인 모델/규칙을 둔다. port 위치는 팀 규칙(domain 또는 application)을 고정한다.
11. port 파일명은 `<domain>_port` 템플릿으로 고정한다.
- 예: `user_port`, `billing_port`
12. 구현 순서는 아래를 따른다.
- 기능 분석
- domain 존재 확인/생성
- port 존재 확인/추가
- adapter 생성(필요 포트를 인수로 받는 방식)
- wiring/검증
13. adapter 파일명은 기능명+책임(또는 외부 시스템명) 기반으로 충돌 없이 정한다.
- 예: `login_oauth_adapter`, `order_payment_adapter`

## Completion Checklist
- `stage_*`, `flow_*`, `input_*` 규칙이 코드에서 일관되게 적용되었는가?
- 입력값 required/optional(default) 구분이 명확한가?
- port/adapter 파일명이 템플릿 규칙을 따르는가?
- 기능 구현 전 유사 기능 탐색이 수행되었는가?
