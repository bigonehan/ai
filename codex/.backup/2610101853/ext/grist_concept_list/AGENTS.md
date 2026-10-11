# AGENTS.md instructions for grist_concept_list

## Verification artifacts

- 테스트가 실제로 필요로 하는 임시 파일만 이 프로젝트의 `test/tmp/` 아래에서 생성하고, 작업이 끝나면 지운다.
- 제품 런타임이 직접 읽는 설정·manifest·fixture·데이터 파일은 소유 경로에 둔다.

## Product boundaries

- 이 프로젝트 밖의 Grist widget, Eagle 도구, Bridge 또는 외부 자동화 코드를 수정하지 않는다.
- Eagle 목록·상태·썸네일 읽기는 기존 Local API 경계를 유지하되, 이미지 쓰기는 `grist_video_maker/eagle-data-bridge`의 `POST /commands/import-to-folder` 계약만 사용한다. 이 프로젝트에 직접 Eagle import, annotation polling, import journal 또는 Local API 쓰기를 다시 추가하지 않는다.
- Grist 신규 행은 드롭 파일 하나당 정확히 한 번 추가하며 `$Eagle`만 기록한다.
- Eagle import가 성공한 뒤 Grist 쓰기가 실패한 재시도에서는 같은 request ID의 Eagle import를 반복하지 않는다.
- 개발 서버는 사용자 요청 없이 실행하지 않는다.

## External diagnostics

- 로그는 개발 모드에서만 기록하고 operation ID, system, operation, stage, attempt, result를 포함한다.
- 토큰, 원시 이미지 payload, Authorization 값, 전체 개인 경로를 로그에 남기지 않는다.
