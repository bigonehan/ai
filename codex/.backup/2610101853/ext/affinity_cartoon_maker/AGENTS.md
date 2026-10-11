# Affinity Cartoon Maker Project Instructions

## File Copy

- 기능 추가·개선 또는 수정 작업이 완료되면 실행에 필요한 파일과 빌드 산출물을 `H:\code\affinity_cartoon_maker`에 복사한다.
- WSL 대상 경로는 `/mnt/h/code/affinity_cartoon_maker`이다.
- 빌드 과정이 있는 경우 빌드를 완료한 뒤 산출물을 대상 폴더 바로 아래에 복사한다.
- 기존 대상 폴더를 통째로 삭제하지 않으며, 사용자 설정·데이터 파일을 덮어쓰거나 삭제하지 않는다.
- 복사는 한 번 실행하고 종료 코드만 확인한다.

## Version Bump

- 기능 추가 또는 개선 작업을 완료할 때마다 버전을 기본적으로 `0.0.1`씩 올린다.
- 별도 지시가 없으면 patch 단위로 증가시킨다.
- 각 버전 세그먼트는 최대 두 자리(`0`~`99`)로 관리한다. 예: `0.0.99 → 0.1.0`.
- `package.json`, `manifest.json` 등 버전을 관리하는 파일이 여러 개 있으면 동일한 버전으로 함께 갱신한다.
- 문서나 작업 규칙만 변경하는 경우에는 버전을 올리지 않는다.

## Verification Artifact Workspace

- 테스트가 실제로 필요로 하는 임시 파일만 이 프로젝트의 `test/tmp/` 아래에서 생성하고, 작업이 끝나면 지운다.
- 위 검증용 임시 파일을 프로젝트 루트나 상위 프로젝트 모음 디렉터리에 생성하지 않는다.
- 제품 런타임이 직접 읽는 설정·manifest·fixture·데이터 JSON/TXT는 기존 소유 경로를 유지한다.
