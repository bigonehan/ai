---
name: build_domain
description: "도메인 생성 시 과분해를 막고 최소 책임 단위로 안정적으로 수렴시키는 스킬."
version: improved-2026-02-18-v2
---

# Domain Create

## Trigger
- 사용자 요청에 `도메인 생성` 문구가 포함되면 반드시 실행한다.
- `도메인 설계`, `domain create`, `domain 제안`도 동일 트리거로 처리한다.

## Input Source Rule
- 외부 Skill/plan 문서를 추가로 읽으려 하지 않는다.
- 현재 요청에서 전달된 spec.yaml의 프로젝트 정의(name, description, framework, rule)를 최우선 근거로 사용한다.
- 프로젝트 정의와 features/tasks 정보를 바탕으로 필요한 domain, task.rule, tasks를 보강한다.

## Goal
- 기능 목록을 **최소 개수 도메인**으로 수렴시킨다.
- 먼저 상위 단일 도메인(umbrella)을 제시하고, 분해 근거가 있을 때만 분리한다.

### 1-2. 도메인 질문
```
Q1: 이 시스템의 주체는 누구인가?
Q2: 그 주체가 수행하는 핵심 행위는?
Q3: 중심 객체는?
Q4: 그 객체의 상태는?
Q5: 그 객체에 가해지는 동작은?
```

### 1-3. 단계 완료 후 모호함 체크
→ 모호함 있으면 질문, 해소 후 2단계 진행


## 2단계: 필터링

1단계에서 확정된 객체/상태/동작으로 흐름 문장을 생성한다.

### 흐름 문장 3가지 관점
```
Q1: 상태변화 — "[객체]가 [동작]되면 [상태]로 변한다"
Q2: 도메인간 상호작용 — "[객체A]가 [객체B]를 [어떻게] 참조/영향을 주는가"
Q3: 가능한 동작 목록
```

### 이터레이션 분리
- 위 3가지에 포함되지 않는 것 → 다음 이터레이션 목록으로 분리
- 1차 이터레이션 = 흐름 문장이 end-to-end로 작동하는 최소 단위

### 단계 완료 후 모호함 체크
→ 모호함 있으면 질문, 해소 후 3단계 진행



## Decision Policy (중요)
1. 기본값은 `1 domain`이다.
- 같은 제품/맥락 기능이면 먼저 하나의 상위 도메인으로 묶는다.
- 예: 통화 녹음/번호 기억/스팸 차단 -> `phone` 우선

2. 분해는 "증거 기반"으로만 허용한다.
- 아래 3가지를 모두 만족할 때만 분해 가능:
- 독립 정책: 규칙 변경이 다른 기능과 독립적이다.
- 독립 수명주기: 배포/릴리즈 주기가 분리 가능하다.
- 독립 용어체계: 팀이 별도 유비쿼터스 언어로 관리한다.

3. 위 3가지 중 하나라도 불충분하면 병합한다.

## Hard Rules
1. 도메인 정의
- `domain = 독립 정책/규칙/용어를 가진 비즈니스 책임 단위`

2. 금지 패턴 (후보 탈락)
- 상태/저장/세션/전송수단: `*_session`, `*_persistence`, `cache`, `transport`
- 주체 분기: `member_*`, `guest_*`, `admin_*`
- 내부 구성요소: `*_item`, `*_option`, `*_detail`
- 동작명 중심: `*_control`, `*_management`, `*_handling` (예외적 상황 외 금지)

3. 중복 정리
- 의미 동일 후보는 1개로 통합
- 부모-자식 동시 제안 시 부모만 유지
- 예: `cart_item`, `cart_item_option`, `member_cart`, `guest_cart` -> `cart`

4. 네이밍
- 짧은 명사 1개 우선 (`phone`, `cart`, `inventory`)
- 허용 문자: 영문/숫자/`_`/`-`

5. 스코프 우선 네이밍
- concern(temperature, recording 등)보다 object/product(`refrigerator`, `phone`)를 우선한다.
- concern 이름은 기본적으로 도메인명이 아니라 도메인 내부 하위 책임으로 둔다.
- 예: `refrigerator_temperature`보다 `refrigerator`를 우선한다.
- concern을 도메인으로 승격하려면 Decision Policy의 3가지 분해 증거를 모두 만족해야 한다.

## Workflow
1. 기능 묶음의 공통 상위 책임을 먼저 찾는다.
2. 상위 책임 1개 도메인을 먼저 제안한다.
3. 제안된 이름이 concern 중심이면 object/product 이름으로 다시 올린다.
4. 분해 필요 시에만 3가지 증거(독립 정책/수명주기/용어체계)를 확인한다.
5. 증거 부족하면 즉시 상위 도메인으로 되돌린다.
6. 최종 결과를 최소 도메인 집합으로 출력한다.

## Prompt Template
```text
다음 기능 리스트와 현재 domain 목록을 보고 domain을 제안해줘.
중요 규칙:
- 먼저 모든 기능을 1개의 상위 domain으로 묶을 수 있는지 판단하고, 가능하면 1개만 제안
- domain 분해는 독립 정책/독립 수명주기/독립 용어체계를 모두 만족할 때만 허용
- domain은 더 이상 쪼개지지 않는 최소 책임 단위여야 함
- concern 중심 이름(예: refrigerator_temperature)보다 object/product 이름(예: refrigerator)을 우선
- 상태/저장소/세션/전송수단, member/guest 주체 구분, item/option/detail 내부 구성요소는 domain으로 금지
- 의미가 겹치면 반드시 하나로 병합
- 결과는 DOMAIN_NAME만 쉼표로 출력 (영문/숫자/_/- 만 허용)
기능: {functions}
현재 domain: {domains}
```

## Validation Checklist
- 먼저 단일 상위 도메인 안을 검토했는가?
- 도메인명이 concern이 아닌 object/product 기준으로 정해졌는가?
- 분해한 경우 3가지 증거를 모두 만족하는가?
- 금지 패턴 도메인이 제거되었는가?
- 결과가 최소 개수 도메인 집합인가?
