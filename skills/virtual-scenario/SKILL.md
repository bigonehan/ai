---
name: virtual-scenario
description: 구조 개선 구현 전에 가상 시나리오로 실행 흐름을 한 줄 리스트로 요약하는 스킬.
---

# Virtual Scenario

핵심: 구조 개선 구현 전에 실행/파일/결과 흐름을 미리 요약해 오해를 줄인다.

## Trigger
- 구조 개선 구현 요청(아키텍처, 프로세스, 실행 흐름, 단계 재설계)일 때 사용한다.

## Mandatory Output
- 출력 형식은 한 줄 리스트로 고정한다.
- 각 줄은 아래 축약형 문장으로 작성한다.
  - `<입력/명령>를 읽고 -> <파일 생성/수정/처리>`
- 설명은 입력/수정/생성되는 파일 중심으로 작성한다.
- 영문 파이프 형식(`command | files touched | derived result`)은 내부 참고용으로만 두고, 사용자 출력은 축약형 문장을 우선한다.


## Output Examples
- `input.txt를 orc로 읽고 -> .project/mobile-todo/blueprint.yaml 생성`
- `.project/mobile-todo/blueprint.yaml을 읽고 -> .project/mobile-todo/project.yaml, .project/mobile-todo/tasks.yaml 생성`
- `.project/mobile-todo/tasks.yaml을 읽고 -> .project/mobile-todo/todos.yaml 생성/갱신`
- `.project/mobile-todo/todos.yaml을 읽고 -> task scope 코드 파일 처리`

## Minimal Flow
1. 요청을 실행 가능한 단계로 분해한다.
2. 각 단계의 명령/영향 파일/파생 결과를 한 줄로 작성한다.
3. 요약을 사용자에게 먼저 보여주고 구현을 시작한다.


## Output Visibility Rule
- `.project/scenario.md` 작성은 항상 필수다.
- 사용자 최초 지시에 `알아서 처리`가 포함되면, 가상 시나리오 요약을 대화에 별도 출력하지 않는다.
- 위 경우에도 `.project/scenario.md`에는 동일 형식으로 반드시 기록한다.

## Rules
- 과장/추측 대신 실제 실행 가능한 명령 기준으로 작성한다.
- 파일 경로는 가능한 구체적으로 적고, 결과는 생성/수정/처리 동사로 끝낸다.
- 구조 개선이 아닌 단순 수정에는 강제하지 않는다.
