---
name: check-code
description: "입력된 기능/목표 기준 체크리스트 생성 후 언어별 실행 검증을 수행하는 skill"
---

# Check Code

## 원칙
- 체크 항목 형식은 반드시 아래 한 줄 패턴만 사용한다:
- 입력 형식은 다음과 같다 ` ${입력} -> ${출력} : 기능설명`

# 테스트 항목 설정 
## job.md 반영 규칙
- 별도 체크리스트 파일은 만들지 않는다.
- 각 기능/목표를 검증 가능한 입력/출력 단위로 분해해 항목을 만든다.
- `job.md`를 먼저 읽고 `# task ## verify` 항목과 requirement를 기준으로 `# check` 체크리스트를 재구성한다.
- `# check`는 최소한 `logic_checklist`와 `ui_checklist` 두 묶음으로 나눈다. UI 항목이 없으면 `ui_checklist`는 생략할 수 있다.
- `.project/project.md#architecture`에 `name:` 값이 있으면 같은 이름의 architecture skill contract를 함께 읽고 그 제약을 `# check`에 포함한다.
- `# problems`에 있는 항목을 `# check`보다 먼저 처리한다.
- 검증 중 새로 발견한 문제는 `job.md#problems` 에 추가한다. 이때 항목은 `${입력} 되면 ${출력}~야 한다` 식으로 추가한다. 
- 관련된 유닛테스트를 돌리고, unit test가 없다면 생성해서 검증을 통과하는진 확인한다 
- `# problems`가 남아 있으면 문제 해결 -> `# check` 재생성 -> 재검증 루프를 반복한다.
- 검증 완료가 확인된 항목은 `job.md#task#verify`에서 `job.md#task#complete`로 이동한다.
- 해결 되지 못한 문제는 `job.md#problems`에 남긴다.
# 테스트 실행 
## 실행 검증 규칙
- 현재 코드 언어를 확인하고 아래 명령 체계를 사용한다.
- Rust:
  - `cargo test`
  - 필요 시 `cargo check`
- JS/TS 계열:
  - `vitest` 기반 테스트 실행
  - 라우팅/폼 제출 같은 사용자 흐름은 `playwright` E2E 실행
- 유닛테스트는 보조 검증이다. UI, 설정, 출력, 상태 저장/불러오기, 런타임 조합 문제는 실제 실행 검증 없이 완료 처리하면 안 된다.
- `playwright`, 브라우저 자동화, CLI 실행이 있더라도 fixture/bootstrap/mock/in-memory state만 사용하면 `실제 실행 검증`으로 인정하지 않는다.
- UI 점검이 포함된 턴이면 `build-design`을 함께 읽고, 거기 적힌 제약조건을 `job.md#check#ui_checklist`에 직접 체크리스트로 기록한다.
- UI 점검은 `build-design` 규칙을 요약하거나 완화하지 않는다. 사용자 원문과 `build-design`의 제약조건을 그대로 옮겨 적고, 각 항목마다 대응 결과를 남긴다.
- `ui_checklist`는 체크 항목마다 최소 `입력 -> 기대 출력 : 제약조건` 형식을 유지하고, 검증 결과는 같은 `job.md` 안에서 항목별로 `통과/실패/증거`를 대응시킨다.
- `build-design`의 UI 기본 점검 하드게이트가 적용되는 경우, 최소 다음 항목을 `ui_checklist`에 그대로 포함한다.
  - `item이 wrapper 밖으로 나오면 실패`
  - `checkbox가 item 왼쪽이 아니면 실패`
  - `편집/삭제가 icon이 아니면 실패`
  - `current.png 대비 group header / control / item의 상대 위치가 바뀌면 실패`
  - `텍스트 버튼이 남아 있으면 실패`
- UI 점검은 항목별 대응 방식이 반드시 보여야 한다. 예: `어떤 selector/스크린샷/실행 경로로 확인했는지`, `어떤 항목이 아직 실패인지`.
- 로직 점검은 `logic_checklist`, UI 점검은 `ui_checklist`로 분리 기록하고, 둘 중 하나라도 미해결 항목이 남아 있으면 완료 처리하면 안 된다.
- 검증 항목마다 최소한 아래 4가지를 함께 남긴다.
  - 입력
  - 기대 출력
  - 데이터 소스(`real`, `fixture`, `mock` 중 하나)
  - 실행 방식(`unit`, `cli`, `browser`, `integration` 등)
- `data source=fixture|mock` 인 검증은 증상 재현용 보조 근거로만 사용한다. 사용자가 보고한 실제 증상 해소 판정은 `data source=real` 검증으로만 내린다.
- 설정/출력/상태변경 버그는 `입력 -> 상태 변화 트리거 -> 저장/재실행/재조회 -> 최종 출력 확인`까지 끝나지 않으면 검증 실패다.
- 구현 함수의 반환값이 실제 로직 없이 고정값인지 반드시 점검한다.
  - 예: `Ok(false)`, `Ok(true)`, `return false`, `return true` 같은 하드코딩 반환
  - 입력값/상태/외부결과를 사용하지 않는 고정 반환이면 검증 실패로 기록한다.
- 문자열/패턴 기반의 하드코딩 성공/실패 판정도 반드시 점검한다.
  - 예: `contains("Logout")`, `contains("success")`, `starts_with("ok")`, `ends_with("done")`
  - 위 조건이 입력/상태/외부결과 검증 없이 최종 판정 분기로 쓰이면 검증 실패로 기록한다.
  - 권장 점검 명령 예시:
    - `rg -n "contains\\(|starts_with\\(|ends_with\\(" src crates packages`
- 상태/단계/토픽 해석이 enum·공통 resolver·공통 read helper를 우회하는지 반드시 점검한다.
  - 예: UI/handler/test에서 로컬 문자열 분기(`pane === "rules"` -> `"rules"`), 직접 배열 인덱스 접근(`steps[index]`), 직접 `findIndex(...)`로 상태를 다시 해석하는 경우
  - 이미 `enum`, `parse*`, `resolve*`, `read*` helper가 있는데 다른 레이어에서 같은 해석 로직을 재구현하면 검증 실패로 기록한다.
  - 상태 해석이 필요한 코드는 `enum/source of truth -> parse/resolve -> read helper -> consumer` 경로만 사용해야 한다.
  - 권장 점검 명령 예시:
    - `rg -n "findIndex\\(|\\[[A-Za-z0-9_]+\\]|=== \\\"rules\\\"|=== \\\"constraints\\\"" src packages`
    - `rg -n "parse[A-Z]|resolve[A-Z]|read[A-Z]" src packages`

## 테스트 워크 플로우 
- 먼저 `drafts.yaml` 내부의 `item`들을 순회하면서 `drafts_item.yaml`에서 `constraints`기능/목표를 읽는다.
- 위 입력과 `job.md#task#verify`, `job.md#problems`를 바탕으로 검증 체크리스트를 구성하고 `.job.md#check`에 item 들을 더한다 
- UI 작업이면 `job.md#check` 아래에 `logic_checklist`, `ui_checklist`를 만들고, `ui_checklist`는 `build-design` 제약조건을 그대로 복사해 넣는다.
- 이때 `item` 입력 형식은 `draft_item.name`:`검사해볼 기능` 형식으로 작성한다.
- 검증 체크리스트에 알맞은 유닛테스트들을 생성한다. 
- 사용자 입력들이 필요한 경우 mock 객체를 생성해서 사용 
- 유넷 테스트 실행후 pass 확인 
- playwirght로(js/ts인 경우)로 headless 브라우저로 기능 실행후 `checklist`에 있는 기능을 수행하는지 스크린샷 
- 디자인이나 화면 ui 요청이 있는경우또한 playwirght로 실행후 스크린샷 캡쳐 
- UI 검증 결과는 `ui_checklist`의 각 항목에 1:1로 대응되어야 한다. 항목 하나라도 증거 없이 넘어가면 검증 실패다.
- 사용자가 실제 실행 증상을 보고한 경우에는 fixture 브라우저 검증 전에 실제 저장 데이터 또는 실제 런타임 소스로 먼저 재현을 시도한다.
- `unit only pass`, `fixture only pass`, `real runtime verified`를 분리 기록하고, 완료 처리는 `real runtime verified`만 허용한다.

# 완료 처리 
- 완료 시 해결된 항목은 `.job.md#task#verify` 에서 `.job.md#task#complete`로 이동 
- 완료 시 미해결된 항목은 `.job.md#problems`에 유지 
- 미해결된 항목이 있는 경우 `# problems` 해결 -> `# check` 재생성 -> 재검증 루프를 반복함 
