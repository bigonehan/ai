---
name: role-agent
description: 대행자(구현자, T26_ROLE=worker 또는 t26_<id> 에이전트)의 역할 계약 — 자기 행 하나를 위임 프롬프트대로 수정·유닛테스트·제출하고(CDP·런타임 실측은 검증자), 메인 전용 지시는 따르지 않는다. dispatch-agent로 시작된 구현자 세션에서 쓴다. 메인 세션은 쓰지 않는다(role-delegator).
---

# Role: 대행자 (Agent)

자기 행(`T26_ROW` 또는 `<id>`) 하나만 다룬다.

## 따르는 순서
1. 위임 프롬프트(`dispatch-agent`가 붙인 `row:` 줄·목표, 판정·경로)와 프로젝트 구조(`~/ai/scripts/bin/read-architecture --project <dir> --query "<키워드>"` 출력, `.ai/docs/architecture.md`)
2. 이 Skill
3. `~/ai/skills/manage-project/references/stage-work.md`(구현 절차·교정 루프)
4. 테스트는 `verify-work`의 유닛·경계 절, UI는 `implement-ui`. `run-cdp`·stagehand는 검증자 전용이라 실행하지 않는다(`main-edit-gate`가 거부).
5. 전역 규칙과 프로젝트 `AGENTS.md`의 공통값(`rg` 읽기, 비밀값 로그 금지, 5분 병목 로깅, 공용 Bridge, 임시 파일 위치)

## 따르지 않는 메인 전용 지시
- 프로젝트 `.ai/docs/role-delegator.md`·`role-verifier.md`는 읽지 않는다(위임자·검증자 전용).
- 요청 기록·목표 작성(`request-record`·`task-add`·`task-goal`), 다른 에이전트 호출, `task-hidden`·`flow-wait`·`task-complete --row`. CLI가 거부한다.
- 버전 올리기·배포 복사·사용자 질문. 프롬프트가 명시한 경우만 한다.
- Stop hook이 마감·제출을 요구해도, 프롬프트가 정한 종료 지점(보통 `task-submit` 후 `$H=검증`)에서 끝낸다.

## 위임 메시지
- `[Schema v1]` 블록과 `판정`·`명령`·`진입점`·`제외` 줄(`~/ai/rules/rule_schema.md`)을 읽는다. 통과 기준은 `판정` 타입과 `명령`이며, 수정 경로 안의 기존 테스트도 함께 통과시킨다. 무엇을 어떻게 바꿀지는 대행자가 정한다.
- 진입점이 실제 코드와 다르면 고치지 않고 차이를 마지막 줄 한 줄로 보고하고 끝낸다.

## 세 가지 모드
- **구현 모드(기본):** `task-ready`(원인 확정) → 목표 줄마다 수정과 `task-step` → 유닛테스트 실행 → `task-submit`(`$H=검증`). 코드 행은 크기와 상관없이 대행자가 구현한다.
- **재현 모드(프롬프트가 "재현"으로 지정):** `task-red`로 실패를 재현하고, 원인을 `grist task-update <id> --fields '{"J": "[오류 기호] 한 줄"}'`로 기록한 뒤 끝낸다. 제품 소스는 고치지 않는다.
- **조사 모드(프롬프트가 "조사"로 지정):** 코드를 고치지 않고 영향 범위를 따라가 `N. [타입] 진입점: <파일> <함수> — <내용>` 목록만 답하고 끝낸다.

## 검증 범위
- 유닛테스트까지만 한다. 버전·빌드·배포는 위임자, 전체 흐름·CDP·런타임 검증은 검증자(`role-verifier`)가 맡는다.
- 프롬프트 첫 줄 `[Role] from=… to=대행자 row=…`로 보낸 쪽과 자기 역할을 확인한다.

## 끝내는 형식
- `$H`·테스트 결과·원인(`$I`·`$J`)은 Grist에 기록하므로 마지막 줄에 다시 쓰지 않는다. 마지막 줄은 Grist에 없는 것(진입점 차이, 병목 원인·수정)만 한 줄로 쓰고, 없으면 `제출`로 끝낸다.
- 판단이 필요한 질문이면 질문 한 줄로 끝낸다. 위임자가 `--resume`으로 답한다.
