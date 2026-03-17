---
name: build-code
description: " 코드를 구현하는 skill"
---

# 컨텍스트
- `task 단일 객체` : 현재 구현해야 할 task
다른 컨텍스트를 가정하지 않는다.

---
# 기본 규칙 
## 네이밍 규칙
- 코드명/명령명 네이밍은 공통 규칙 `/home/tree/ai/skills/rule-naming/SKILL.md`를 따른다.
## scope 규칙
여기서`scope` 는 `.project/drafts.ymal` 내부의 `item`중 현재 작업중인 item 항목을 의미한다.
- `scope`에 명시된 파일 경로만 수정/생성한다
- `scope`에 없는 파일/폴더는 생성/수정/삭제를 절대 하지 않는다
- `scope` 제약을 만족할 수 없으면 파일을 변경하지 말고 실패 사유만 보고한다
- `scope`가 비어있으면 `rule`, `step`, `name`을 바탕으로 파일 경로를 추론해 채운다

## 함수형 구조 규칙
- 전역 가변 상태를 직접 읽거나 쓰지 말고 인자/리턴으로 전달한다
- 배열/객체 갱신은 Copy-on-Write(복사 → 수정 → 반환)만 사용한다
- 인자 mutation을 금지한다
- 외부 I/O, 로깅, 시간/랜덤은 액션 레이어로 격리한다
- 액션 함수는 조립과 오케스트레이션만 수행하고 계산은 별도 함수로 분리한다

# 구현 규칙
## 코드 참조 
1. `.project/projet.md`에서`# rules`, `# constrints`, `# domains` 등을 확인한다 
3. `draft` 관련 구조는 `references/darft_item.yaml` 형식을 기준으로 해석한다
4. 전달받은 `task 단일 객체`의 `scope`, `step`, `rule`을 기준으로 구현한다

## step 처리
- 작업 시작시 `jj workspace`를 생성
- 작업이 시작하면 현재 `.project/drafts` 내부의 `item` 중 현재 작업중인 item의 `state`를 `work`로 변경한다.
- `job.md#requirement`에 있는 현재 작업중인 항목을 `./job.md#task#work`로 이동한다.
- 코드 검증 요구에 맞는 유닛 테스트 파일을 먼저 생성한후 fail을 확인한다. 
- 유닛테스트 검증을 합격하는 코드를 구현한다.
- 유닛테스트 검증이 pass가 되는것을 확인한다.
- `step`의 순서대로 구현한다.
- 검증 조건, 필터링은 함수의 맨 처음에 배치한다
- `파일 → 입력 → 동작 → 출력` 형태로 한 줄씩 처리한다
- 한 번에 작업할 수 있는 최소한의 단위로 수행한다
- 작업이 완료가 되었다면 `./job.md#task#work` 항목을 `./job.md#task#ready` 항목으로 이전 
- 모든 작업이 성공적으로 완료가 되었다면 merge 

# 금지 규칙 
- 불필요한 파일 생성 금지
- 과도한 리팩토링 금지
- scope 외 파일 수정 금지
- rule 위반 가능성이 있으면 즉시 수정해서 규칙을 만족할 것
# 완료 규칙 
- 완료된 drafts_item의 state를 `complete` 로 변경한 이후에 `./job.md#requirement` 에 있는 현재 작업 대상 항목을 `complete` 아래로 이동한다.
- 유넷테스트를 pass한 코드가 하드코드로 되어있는지를 확인하고 하드 코드 형식으면 함수 내부의 계산식으로 처리되게 변경한다. 
- 실행 실패 또는 대기시간 초과 시 `./job.md#requirement`에 있던 항목을`./job.md#task##fail`로 이동후 `항목 : 사유(한줄)` 로 기록한다.
