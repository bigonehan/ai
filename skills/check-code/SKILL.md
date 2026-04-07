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
- `job.md`에 `# symptom`, `# success`, `# verify axes`가 없으면 검증을 시작하지 말고 먼저 세 항목을 채운다.
- `# symptom`에는 사용자 관점 실패 문장, `# success`에는 최종 성공 문장, `# verify axes`에는 `render`, `mutation`, `persistence`, `re-entry`, `negative-check`를 고정한다.
- `# check`는 최소한 `logic_checklist`와 `ui_checklist` 두 묶음으로 나눈다. UI 항목이 없으면 `ui_checklist`는 생략할 수 있다.
- 상태 변화 작업이면 `# check`를 `logic_checklist`, `ui_checklist`, `persistence_checklist`, `reentry_checklist`, `negative_checklist`로 분리한다. 해당 축이 비면 검증 실패다.
- `logic_checklist`와 `ui_checklist`는 별개다. ORC나 worker가 둘을 하나의 단일 목록으로 합치거나, `verified:` 단일 bullet 목록으로 평탄화하면 검증 실패로 기록한다.
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
- `real-equivalent`는 실제 persistence source를 사용하고, mock/stub 주입 없이 `reload`, `reopen`, `restart` 중 최소 1개를 포함한 검증에만 붙인다.
- UI/웹 증상 해소 판정은 `data source=real` 또는 `execution=browser`가 확인된 실제 브라우저 검증으로만 내린다. `real-equivalent`는 저장/재진입 보조 근거로만 사용한다.
- 사용자 원문에 `사라짐`, `재실행`, `종료 후`, `다시 열면`, `유지 안 됨`이 있으면 첫 검증 대상은 반드시 `persist -> read/load -> reload/re-entry` 경로여야 한다. 이 경로를 보기 전에 프런트 이벤트/렌더/부분 가설만 수정하고 완료 처리하면 검증 실패다.
- 사용자가 보고한 UI/상태변경 버그에서 `html.contains`, `js.contains`, 함수 존재 확인, selector 존재 확인 같은 정적 문자열 검증만으로 완료 처리하면 검증 실패다.
- 사용자가 보고한 실제 증상과 대응되는 핵심 사용자 플로우는 반드시 `실행 순서` 그대로 검증한다. 예: `생성 -> 선택/입력 -> 저장 -> 재조회/재진입 -> 최종 상태 확인`.
- 도메인에 `group`, `preset`, `slot`, `collection`, `list`처럼 항목 집합이 있으면 `empty 상태`를 별도 검증 축으로 다룬다.
  - `항목 0개`, `그룹만 있고 item 0개`, `선택값 없음`, `비어 있는 저장 파일/섹션` 중 현재 기능과 맞는 empty 케이스를 최소 1개 이상 체크리스트에 포함한다.
  - empty 상태가 `메타데이터 유지`, `재진입`, `저장/로드`, `할당 유지` 중 하나에 영향을 주는 도메인이면 이 케이스를 빼고 완료 처리하면 검증 실패다.
  - `빈 상태에서도 구조는 유지되어야 한다`와 `빈 상태에서는 선택/출력이 비워져야 한다`를 구분해서 기대값을 적는다.
- 도메인에 `group`, `scope`, `workspace`, `tenant`, `id`, `key` 같은 분리 축이 있으면 중복 검사/조회/수정/삭제가 그 축을 포함하는지 하드게이트로 점검한다. 축 누락 상태의 전역 비교나 전역 검색이면 검증 실패다.
- 도메인 분리 축 점검은 `create/read/update/delete/duplicate-check/persistence/reload` 전 축에 대해 수행한다. 한 레이어에서만 축을 쓰고 다른 레이어에서 전역 비교/초기화/덮어쓰기를 하면 검증 실패다.
- 테스트 기대값은 실제 UI 동작을 실행으로 먼저 확인한 뒤 고정한다. 브라우저를 띄우기 전에 `중복 에러가 날 것이다`, `버튼이 비활성화될 것이다` 같은 UX 추론으로 테스트를 작성해 통과 판정하면 검증 실패다.
- UI 버그 수정 턴에는 최소 1개의 `정상 플로우`와 1개의 `동일 경로 반증/경계 플로우`를 실제 브라우저에서 모두 실행해야 한다. 둘 중 하나라도 빠지면 하드게이트 실패다.
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
  - 데이터 소스(`real`, `real-equivalent`, `fixture`, `mock` 중 하나)
  - 실행 방식(`unit`, `cli`, `browser`, `integration` 등)
- UI/웹 항목은 추가로 `artifact`를 남긴다. 스크린샷, trace, 캡처 경로가 없으면 해당 항목은 실패다.
- `data source=fixture|mock` 인 검증은 증상 재현용 보조 근거로만 사용한다. 사용자가 보고한 실제 증상 해소 판정은 `data source=real|real-equivalent` 검증으로만 내린다.
- browser가 실제로 떠도 backend persistence/read path를 fixture/bootstrap/in-memory state로 우회하면 `real runtime verified`로 기록하면 안 된다. 이런 경우는 `fixture only pass` 또는 `real-equivalent`까지만 허용한다.
- 브라우저 실행이 실패한 뒤 API 호출, direct handler 호출, fixture bootstrap으로만 우회한 결과는 `실제 UI 검증 대체`로 인정하지 않는다. 이 경우는 `러너 실패` 또는 `실제 UI 미검증`으로 남겨야 한다.
- 설정/출력/상태변경 버그는 `입력 -> 상태 변화 트리거 -> 저장/재실행/재조회 -> 최종 출력 확인`까지 끝나지 않으면 검증 실패다.
- 상태 변화 작업은 `render`, `mutation`, `persistence`, `re-entry`, `negative-check` 다섯 축을 모두 채워야 한다.
- 상태 변화 작업에서 empty 상태가 의미 있는 도메인이면 `negative_checklist` 또는 `persistence_checklist`에 empty 케이스를 반드시 포함한다.
- 저장/불러오기 버그 검증에서는 `비어 있어도 남아야 하는 구조 메타데이터`와 `비어 있으니 사라져야 하는 실제 내용`을 분리해 확인한다.
- 버그 수정은 `정상 동작 확인 1개`와 `내 가설이 틀리면 실패해야 하는 반증 검증 1개 이상`을 반드시 함께 남긴다.
- 사용자 원문에 `재시작`, `다시 열기`, `다시 들어가면`, `사라짐`, `초기화`, `유지`가 있으면 `re-entry` 축 없이 완료 처리하면 안 된다.
- 상태 소실/재진입 버그는 `증상 재현 -> 저장 소스 확인 -> 로드 소스 확인 -> 덮어쓰기/초기화 지점 확인 -> 수정 -> 동일 시나리오 재실행` 순서를 기록해야 한다. 이 순서를 건너뛴 부분 가설 수정은 검증 실패다.
- 사용자가 같은 증상을 2회 이상 반복 제기한 턴에서는 검증 보고에 반드시 `실제 소실/오동작 지점 1개`와 `이전 검증이 왜 틀렸는지 1개`를 남겨야 한다. 둘 중 하나라도 없으면 검증 실패다.
- 사용자가 같은 실패를 여러 persona/worker/경로에서 반복 보고하면 개별 출력 품질 문제가 아니라 `입력 명령 / 프롬프트 / 상태 전이 / 규칙 충돌` 중 하나의 구조 문제로 먼저 분류한다. 이 분류 없이 fallback, notice, filter, retry 강화 같은 증상 완화 수정부터 진행하면 검증 실패다.
- 전원 무응답, 다수 주체 동일 탈락, 같은 validator reason 반복은 모델 품질보다 prompt/contract 충돌을 먼저 의심한다. 이 경우 `어떤 규칙들이 동시에 걸렸는지`, `서로 양립 가능한지`, `왜 같은 실패가 반복됐는지`를 검증 보고에 남기지 않으면 검증 실패다.
- 검증 시작 시 아래 순서를 하드게이트로 적용한다.
  - `이 실패가 출력 품질 문제인지, 입력/상태/규칙 문제인지 먼저 분류`
  - `반복 패턴인지 단일 실패인지 확인`
  - `여러 주체가 함께 실패했는지 확인`
  - `서로 충돌하는 제약이 같은 경로에 동시에 주입됐는지 확인`
  - `지금 하려는 수정이 원인 제거인지 증상 가리기인지 기록`
- 위 분류가 `입력/상태/규칙 문제`로 나오면 fallback, 하드코딩 응답, 사용자 책임 notice, 후처리 필터 추가를 금지한다. 먼저 입력 프롬프트, 상태 해석, 규칙 충돌, source of truth를 수정해야 하며 이를 건너뛰면 검증 실패다.
- 검증 보고에는 반드시 아래 두 항목이 함께 있어야 한다.
  - `왜 처음 판단이 잘못됐는지`
  - `왜 이번 수정이 증상 완화가 아니라 원인 제거인지`
- 사용자가 “전부 다”, “아무도 못함”, “계속 헛소리”, “같은 말 반복”처럼 집단 실패를 지적한 턴에서는 `개별 응답 보정`을 원인 해결로 인정하지 않는다. prompt/contract/state 구조 수정이 없으면 검증 실패다.
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
- 상태 변화 작업이면 `logic_checklist`, `ui_checklist`, `persistence_checklist`, `reentry_checklist`, `negative_checklist`를 유지하고, worker는 섹션 이름을 바꾸거나 합치지 않는다.
- 이때 `item` 입력 형식은 `draft_item.name`:`검사해볼 기능` 형식으로 작성한다.
- 검증 체크리스트에 알맞은 유닛테스트들을 생성한다. 
- 사용자 입력들이 필요한 경우 mock 객체를 생성해서 사용 
- 유넷 테스트 실행후 pass 확인 
- playwirght로(js/ts인 경우)로 headless 브라우저로 기능 실행후 `checklist`에 있는 기능을 수행하는지 스크린샷 
- 디자인이나 화면 ui 요청이 있는경우또한 playwirght로 실행후 스크린샷 캡쳐 
- UI 검증 결과는 `ui_checklist`의 각 항목에 1:1로 대응되어야 한다. 항목 하나라도 증거 없이 넘어가면 검증 실패다.
- 사용자가 실제 실행 증상을 보고한 경우에는 fixture 브라우저 검증 전에 실제 저장 데이터 또는 실제 런타임 소스로 먼저 재현을 시도한다.
- 사용자가 “안 된다”, “사라진다”, “추가가 안 된다”, “저장이 안 된다”처럼 상태변경 실패를 직접 보고한 경우, `정적 테스트 -> 구현 완료` 순서를 금지한다. 반드시 `증상 재현 -> 원인 후보 축소 -> 수정 -> 동일 시나리오 재실행` 순서로 기록한다.
- 저장/재진입 검증에는 가능하면 아래 둘을 함께 둔다.
  - `non-empty 기준 정상 플로우`
  - `empty 기준 경계 플로우`
  둘 중 하나라도 빠지면 저장 구조의 누락을 놓칠 수 있으므로 검증 실패로 본다.
- `unit only pass`, `fixture only pass`, `real-equivalent verified`, `real runtime verified`를 분리 기록한다. UI/웹 완료 처리는 `real runtime verified`만 허용하고, `real-equivalent verified`는 persistence/re-entry 보조 판정으로만 사용한다.
- `negative_checklist`에는 최소 한 항목 이상 있어야 하며, 없으면 검증 실패다.

# 완료 처리 
- 완료 시 해결된 항목은 `.job.md#task#verify` 에서 `.job.md#task#complete`로 이동 
- 완료 시 미해결된 항목은 `.job.md#problems`에 유지 
- 미해결된 항목이 있는 경우 `# problems` 해결 -> `# check` 재생성 -> 재검증 루프를 반복함 
