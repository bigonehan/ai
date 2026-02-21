## 2026-02-18 - 작업한일
- functional-code-structure 분석 기반으로 plan-architecture 스킬 신설
- 코드 구현 요청 트리거("코드를 짜줘/만들어줘/구현해줘/리팩토링해줘")와 아키텍처 우선 워크플로/함수형 구조 규칙 정의
- skill validator 통과

## 2026-02-18 - 작업한일
- functional-code-structure 스킬 발동 조건에 "코드 개선 및 점검" 요청을 명시적으로 추가
- 스킬 설명 본문도 동일 기준으로 업데이트해 코드 개선 명령 시 발동되도록 정렬

## 2026-02-18 - 작업한일
- /home/tree/ai/codex/AGENTS.override.md에 jj 필수 절차 강화: 작업 시작 전 repo 확인/생성, 시작 시 workspace 생성, 완료 시 merge 의무화

## 2026-02-21 - 작업한일
- 기능 추가 요청 전용 `add-function` 스킬 신설 (`add-function/SKILL.md`)
- `plan-code`의 핵심 게이트(Absolute Sequence, Hard Guardrails, Pre-Implementation Gate) 내용을 복사해 포함
- 설계 문서 선행 후 구현하는 워크플로와 최소 변경/검증 체크리스트를 기능 추가 컨텍스트에 맞게 정리
