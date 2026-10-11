# Chrome Prompt Bridge Project Instructions

## CROSS-PROJECT ROLE: CHROME PROMPT BRIDGE

- Chrome Prompt Bridge is the sole integration gateway between Grist Video Maker and Side Prompt Gallery for project initialization, background generation, and List image generation.
- It owns request validation/persistence, correlation lifecycle, tab/job orchestration, retry/timeout policy, receipt of Side Prompt Gallery responses, and publication of correlated results back to Grist.
- Preserve existing wire message names and storage keys unless an explicit migration is approved. Extend the existing path instead of creating a parallel transport or queue.
- This project owns `contracts/grist-side-prompt-v1.json` and its validator.

## Project Boundary

- 이 파일이 있는 `/home/tree/extensions/chrome_prompt_bridge`를 독립 프로젝트로 취급한다.
- 사용자가 별도로 지시하지 않으면 이 프로젝트 밖의 파일을 수정하지 않는다.

## Version Bump Default

- 기능 개선 또는 신규 기능을 추가할 때 기본적으로 버전을 `0.0.1`씩 올린다.
- 별도 지시가 없으면 patch 단위로 증가시킨다.
- 각 버전 세그먼트는 최대 두 자리(`00`~`99`)를 넘기지 않도록 관리한다.
- `package.json`과 `manifest.json`의 버전을 항상 동일하게 유지한다.
- 빌드 후 `dist/manifest.json`도 갱신된 버전과 같은지 확인한다.

## Build And Delivery

- 기능 개선, 신규 기능 또는 수정 작업이 끝나면 테스트와 검사를 실행한 후 프로젝트를 빌드한다.
- 장기 실행 dev server는 사용자가 명시적으로 요청한 경우에만 실행한다.
- 빌드된 `dist`의 내용만 Windows 경로 `H:\code\chrome_prompt_bridge`에 복사한다.
- WSL에서는 `/mnt/h/code/chrome_prompt_bridge`를 대상 경로로 사용한다.
- 산출물이 `/mnt/h/code/chrome_prompt_bridge/manifest.json`처럼 대상 폴더 바로 아래에 놓이게 한다.
- 기존 대상 폴더 전체를 삭제하지 않고 빌드 산출물만 덮어쓴다.
- 복사는 한 번 실행하고 종료 코드만 확인한다.

## Verification Artifact Workspace

- 테스트가 실제로 필요로 하는 임시 파일만 이 프로젝트의 `test/tmp/` 아래에서 생성하고, 작업이 끝나면 지운다.
- 위 검증용 임시 파일을 프로젝트 루트나 `/home/tree/extensions` 루트에 생성하지 않는다.
- 제품 런타임이 직접 읽는 설정·manifest·fixture·데이터 JSON/TXT는 이 규칙의 이동 대상이 아니며 기존 소유 경로를 유지한다.

## Initialization Settings Test

- Grist Custom Widget 설정의 기존 Test 버튼 한 번을 일반 `DOCUMENT_PROMPT` 저장/상태 조회 → 네 사이트 `APPLY_DOCUMENT` → 제출 관측 → 최종 상관 결과 경로로 전달한다.
- 사용자용 Test에서 `testStage: receiver-connection` 또는 `model-selection`을 보내지 않는다. 이 값들은 단계별 자동 회귀 검증을 위해만 유지한다.
- 사이트별 상태를 즉시 영속화하고, 한 사이트의 실패 후에도 나머지 독립 사이트를 계속해 네 사이트의 최종 상태를 하나의 결과로 Grist에 반환한다.
