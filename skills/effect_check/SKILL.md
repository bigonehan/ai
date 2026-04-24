---
name: effect_check
description: "Effect TS 코드를 리뷰할 때 Tag, TaggedError, Layer, business logic, provide wiring, 실행 경계를 점검하는 skill"
---

# Effect Check

Effect TS 기반 TypeScript 코드를 점검할 때 이 스킬을 사용한다.
프레임워크가 Effect TS가 아니면 일반 코드 리뷰로 되돌린다.

## 목표
- Effect 의존성 선언과 주입 방식이 일관적인지 확인한다.
- 예외 처리와 실행 경계가 Effect 방식에 맞는지 확인한다.
- 구현체 직접 참조, 잘못된 Layer 선택, 누락된 provide 같은 구조적 실수를 찾는다.

## 리뷰 절차
1. Effect 관련 심볼 사용처를 먼저 찾는다.
   - `Context.Tag`, `Effect.Tag`, `Data.TaggedError`, `Layer.succeed`, `Layer.effect`, `Layer.scoped`, `Effect.provide`, `Effect.gen`, `yield*`, `runPromise`, `runPromiseExit`, `Exit.isSuccess`, `Cause.isFailType`
2. 서비스 정의, 에러 정의, Layer, 비즈니스 로직, 실행 엔트리 포인트를 각각 읽는다.
3. 아래 체크리스트 기준으로 위반 사항을 찾는다.
4. 결과는 반드시 파일/라인 기준 finding 형태로 정리한다.
5. 문제가 없으면 "no findings"를 명시하고, 남는 리스크가 있으면 별도로 적는다.

## 체크리스트

### 1. Tag 정의 확인
1. `class`를 이용해서 `context.tag`가 구현되어 있는가
   - 예시 형태: `class Foo extends Context.Tag("Foo")<Foo, {...}>() {}`
   - 함수형 팩토리나 상수 객체로 우회한 경우 일관성 문제로 본다.
2. 인터페이스가 클래스 안에 인라인으로 정의되어 있는가
   - `Context.Tag("Foo")<Foo, { ... }>()` 형태인지 본다.
   - 별도 `interface FooService`를 멀리 분리해 두면 추적 비용과 drift 위험을 지적한다.
3. Tag 이름 문자열이 클래스명과 일치하는가
   - `"Foo"`와 `class Foo`가 다르면 finding으로 기록한다.

### 2. 에러 타입 확인
1. `Data.TaggedError`로 정의되어 있는가
2. `_tag`가 있는가
3. `throw` 대신 `Effect.fail`을 쓰는가
   - Effect 내부에서 동기 `throw`로 도메인 에러를 던지면 finding이다.
   - 예외를 잡아 `Effect.try`, `Effect.tryPromise`, `Effect.catchTags` 등으로 경계 처리하는 경우는 문맥을 보고 예외로 둘 수 있다.
4. `Data.TaggedError`로 정의되어 있는가
   - 중복 확인 항목이다. 실제 리뷰에서는 "에러 타입이 TaggedError 계열인지"로 한 번 더 교차검증한다.

### 3. Layer 구현체
1. `Layer.succeed` / `Layer.effect` / `Layer.scoped` 중 하나로 감싸져 있는가
2. 첫 번째 인자가 Tag 클래스인가
3. 구현체 메서드가 Tag의 인터페이스를 전부 구현하는가
   - 누락 메서드, 시그니처 불일치, 반환 타입 불일치를 찾는다.
4. 리소스 해제가 필요한데 `Layer.scoped`가 아닌 `Layer.succeed`를 쓰지 않는가
   - DB 연결, 파일 핸들, 구독, 서버, worker, socket, lock처럼 수명 관리가 필요한 객체면 `Layer.scoped` 우선 검토 대상이다.

### 4. 비즈니스 로직
1. 구현체를 직접 import하지 않는가
   - 비즈니스 로직에서 live 구현체, mock 구현체, concrete class를 직접 참조하면 결합도 문제로 본다.
2. `yield* Tag` 형태로 의존성을 요청하는가
   - `Effect.gen` 문맥이면 `const foo = yield* Foo` 형태를 우선 본다.
   - `Effect.flatMap(Foo, ...)` 등 다른 합법적 패턴은 허용하되, 서비스 로케이터처럼 우회하면 지적한다.
3. Requirements 타입에 Tag가 잡히는가
   - 공개 함수/Effect의 Requirements가 실제 의존성과 맞는지 본다.
   - 구현은 `Foo`를 요구하는데 타입이 `never` 또는 다른 Tag만 잡혀 있으면 finding이다.

### 5. provide 연결
1. `runPromiseExit` 호출 전에 `Effect.provide`가 있는가
   - 최종 실행 전에 필요한 Layer 주입이 연결되어야 한다.
2. provide된 Layer의 Output이 Effect의 Requirements를 전부 커버하는가
   - 남는 Requirement가 있으면 실행 시점 문제로 본다.
3. Layer 자체도 필요한 게 있으면 `Layer.provide`로 채워져 있는가
   - 예: `UserRepoLive`가 `Db`를 요구하면, 최종 조합 또는 Layer 내부에서 그 의존성이 채워져야 한다.

### 6. 실행 경계
- `runPromiseExit`로 실행하는가
- `Exit.isSuccess` / `Cause.isFailType`로 분기하는가
- `try/catch`로 Effect 에러를 잡으려 하지 않는가
  - `runPromise` 결과를 `try/catch`로만 처리하거나, Effect 내부 도메인 에러를 JS 예외 흐름으로 다루면 finding이다.

## 리뷰 출력 규칙
- findings가 있으면 심각도 높은 순서로 정리한다.
- 각 finding은 아래 형식을 따른다.

`[severity] path:line - 문제 설명`

- 설명에는 아래 3가지를 짧게 포함한다.
  - 무엇이 잘못됐는지
  - 왜 Effect 구조에서 문제가 되는지
  - 기대되는 수정 방향

## 예외 처리 원칙
- 라이브러리 경계에서 발생한 예외를 `Effect.try` 계열로 감싸는 코드는 허용한다.
- `yield* Tag`만 절대 규칙으로 강제하지 않는다. 다만 구현체 직접 import보다 Tag 기반 의존성 요청을 우선 권장한다.
- 사용자 요구사항에 기존 코드 스타일 유지가 포함되어 있으면, 치명적 구조 문제와 스타일 문제를 분리해서 보고한다.
