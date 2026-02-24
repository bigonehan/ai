---
name: build-code-parallel
description: "task 단일 객체를 받아 참조 파일을 바탕으로 코드를 구현하는 skill. CLI 병렬 실행 함수에 의해 task마다 독립적으로 호출된다."
---

# 컨텍스트
이 스킬이 실행될 때 codex가 알고 있는 정보는 아래 세 가지뿐이다.
- `.project/project.md` : 프로젝트 전체 구조, 도메인, 규칙 참조용
- `.project/drafts_list.yaml` : 전체 기능 목록 및 의존관계 참조용
- `task 단일 객체` : 현재 구현해야 할 task

위 세 가지 외의 컨텍스트를 가정하지 않는다.

---

# 구현 규칙

## 네이밍 규칙
- 코드명/명령명 네이밍은 공통 규칙 `/home/tree/ai/skills/feature_architecture_rules/SKILL.md`를 따른다.

## 참조 순서
1. `.project/project.md`에서 프로젝트 언어, 스택, 도메인, 규칙을 확인한다
2. `.project/drafts_list.yaml`에서 전체 기능 목록과 의존관계를 확인한다
3. 전달받은 `task 단일 객체`의 `scope`, `step`, `rule`을 기준으로 구현한다

## scope 규칙
- `scope`에 명시된 파일 경로만 수정/생성한다
- `scope`에 없는 파일/폴더는 생성/수정/삭제를 절대 하지 않는다
- `scope` 제약을 만족할 수 없으면 파일을 변경하지 말고 실패 사유만 보고한다
- `scope`가 비어있으면 `rule`, `step`, `name`을 바탕으로 파일 경로를 추론해 채운다

## 함수형 구조 규칙
- 모든 로직을 `ACTION`(부수효과)과 `CALC`(순수 계산)으로 분리한다
- 전역 가변 상태를 직접 읽거나 쓰지 말고 인자/리턴으로 전달한다
- 배열/객체 갱신은 Copy-on-Write(복사 → 수정 → 반환)만 사용한다
- 인자 mutation을 금지한다
- 외부 I/O, 로깅, 시간/랜덤은 액션 레이어로 격리한다
- 액션 함수는 조립과 오케스트레이션만 수행하고 계산은 별도 함수로 분리한다

## step 처리
- 작업 시작시 `jj workspace`를 생성
- `step`은 반드시 순서대로 수행한다
- 검증 조건, 필터링은 맨 처음에 배치한다
- `파일 → 입력 → 동작 → 출력` 형태로 한 줄씩 처리한다
- 한 번에 작업할 수 있는 최소한의 단위로 수행한다
- 실행 실패 또는 대기시간 초과 시 `./.project/log.md` 기록 후 실패 처리
- 전체 완료 후 작업을 했던 `drafts.yaml`이 들어있는  폴더를 `.project/clear/` 로 이동 
- `./.project/drafts_list.yaml`에서 `planned`에 있는 같은 기능을 `features`로 이동 

## 금지 사항
- 불필요한 파일 생성 금지
- 과도한 리팩토링 금지
- scope 외 파일 수정 금지
- rule 위반 가능성이 있으면 즉시 수정해서 규칙을 만족할 것


# 완료 보고
- 완료된 draft는 지워지고 `설치폴더/.project/drafts_lists.yaml`의 `feature` 항목에 한문장으로 요약하여 리스트 item에 추가된다.

