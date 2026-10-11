## Project Boundary
- Treat this folder as an independent project.
- When Codex is running from this folder, modify only files inside this folder.
- Do not modify parent folders, sibling project folders, or files outside this folder unless the user explicitly names an external path and asks for that change.

## Completion Desktop Copy Rule
- 작업 완료 시 최신 작업 폴더를 바탕화면으로 복사한다.
- 기본 대상 경로: `H:\code\` (`/mnt/h/code/`)
- 기본 대상 폴더명: 현재 프로젝트 폴더명(`eagle_move`), 사용자가 이름을 지정하면 그 이름을 우선한다.

## Verification Artifact Workspace

- Shot 첫 프레임 이름 비교 오류는 Shot 패널의 `Test`에만 연결한다. Eagle에서 선택한 MP4 하나와 현재 설정의 대상 폴더로 실제 `find_shot_video_name`을 호출한다.
- `Test`에서는 파일 감시 시작, 이동, `eagle.item.addFromPath`, 원본 삭제를 호출하지 않는다.
- 표준 검사: `node --test tests/*.test.js`. 테스트용 디코더 대체 결과를 실제 Eagle 디코딩 성공으로 보고하지 않는다.
- 비교 진단은 실행 플러그인의 `test/tmp/shot-match-<runId>.json`에 기록한다.

- 테스트가 실제로 필요로 하는 임시 파일만 이 프로젝트의 `test/tmp/` 아래에서 생성하고, 작업이 끝나면 지운다.
- 위 검증용 임시 파일을 프로젝트 루트나 `/home/tree/extensions` 루트에 생성하지 않는다.
- 제품 런타임이 직접 읽는 설정·manifest·fixture·데이터 JSON/TXT는 이 규칙의 이동 대상이 아니며 기존 소유 경로를 유지한다.
