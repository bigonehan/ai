# Project Instructions

## Initialization and current work

- Table7 Raw ID: 1203
- 관리 프로그램: 작업/종류/프로젝트/관리/작업관리기/작업관리기-문서.md
- Obsidian 프로젝트 문서: 작업/종류/프로젝트/코딩/261010.save_current_page.md
- Table26 task Raw ID: 12
- 현재 작업: Grist API 토큰 갱신 완료. 배포 확장 `0.0.23`의 service worker가 로컬 비밀 설정을 저장소에 반영했고, Grist 연결 확인에서 테이블 37개를 읽었습니다. 기존 Obsidian 키와 저장 경로는 유지합니다.
- Runtime: Grist 페이지 `http://localhost:8484`, 배포 확장 `/mnt/h/code/save_current_page`, 시작 동작은 service worker의 `grist-token.local.json` 가져오기와 확장 설정의 연결 확인, authoritative consumer는 연결 확인 결과와 확장 저장소의 Grist 키 일치 여부입니다.

## Project Boundary

- 이 디렉터리를 독립된 확장프로그램 프로젝트로 취급한다.
- 이 프로젝트 작업에서는 사용자 요청에 필요한 경우를 제외하고 상위 디렉터리나 다른 프로젝트 파일을 수정하지 않는다.

## Versioning

- 기능 개선, 신규 기능 추가 또는 버그 수정 시 기본적으로 patch 버전을 `0.0.1` 올린다.
- `manifest.json`과 `package.json`이 모두 존재하면 두 파일의 버전을 동일하게 유지한다.
- 별도 지시가 없으면 각 버전 세그먼트는 `00`~`99` 범위에서 관리한다.

## Delivery Copy

- 생성 또는 수정 작업이 끝나면 실행 가능한 결과물을 Windows `H:\code\save_current_page`에 복사한다.
- WSL에서는 `/mnt/h/code/save_current_page`를 대상 경로로 사용한다.
- 빌드 과정이 있으면 소스 전체가 아니라 빌드된 확장프로그램 산출물을 대상 폴더 바로 아래에 복사한다.
- 빌드 과정이 없으면 `manifest.json`과 manifest에서 참조하는 실행 필수 파일을 복사한다.
- 사용자 설정이나 런타임 데이터 파일이 대상 폴더에 이미 있으면 덮어쓰거나 삭제하지 않는다.
- 복사는 한 번 실행하고 종료 코드만 확인한다.

## Verification Artifact Workspace

- 테스트가 실제로 필요로 하는 임시 파일만 이 프로젝트의 `test/tmp/` 아래에서 생성하고, 작업이 끝나면 지운다.
- 위 검증용 임시 파일을 프로젝트 루트나 `/home/tree/extensions`에 생성하지 않는다.
- 제품 런타임이 직접 읽는 설정·manifest·fixture·데이터 JSON/TXT는 이 규칙의 이동 대상이 아니며 기존 소유 경로를 유지한다.
