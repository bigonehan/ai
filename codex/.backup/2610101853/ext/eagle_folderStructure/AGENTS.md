## Project Boundary
- Treat this folder as an independent project.
- When Codex is running from this folder, modify only files inside this folder.
- Do not modify parent folders, sibling project folders, or files outside this folder unless the user explicitly names an external path and asks for that change.

## Work State and Deployment
- 현재 작업 상태·완료 조건은 이 파일, `Input.md`, `.ai/changes/YYMMDD.요약명.md`, Grist Table26으로 관리한다. Table7 Raw ID가 없으면 작업 전에 `initial-project`로 초기화한다.
- Table26 `$D`·`$I`·`$J`에는 지금 진행 중인 최신 항목 한 줄만 작업 행 안의 순번과 함께 쓰고(예: `3. 인증 단계 확립`), 1번부터의 전체 목록과 세부 내용은 같은 순번으로 날짜별 `.ai/changes`에만 기록한다.
- 소스는 이 폴더에서 수정하고, 완료 후 `/mnt/h/code/eagle_folderStructure`에는 빌드/런타임 산출물만 복사한다.
- Eagle 플러그인 배치 대상에는 최소 `manifest.json`, `index.html`, `styles.css`, `logo` 파일, `js/`, `src/`를 포함하고, `tests/`, `package.json`, `AGENTS.md` 같은 개발/관리 파일은 제외한다.
- 검증 전에는 관련 테스트 또는 CLI 체크를 실행한다.

## Verification Artifact Workspace

- 테스트가 실제로 필요로 하는 임시 파일만 이 프로젝트의 `test/tmp/` 아래에서 생성하고, 작업이 끝나면 지운다.
- 위 검증용 임시 파일을 프로젝트 루트나 `/home/tree/extensions` 루트에 생성하지 않는다.
- 제품 런타임이 직접 읽는 설정·manifest·fixture·데이터 JSON/TXT는 이 규칙의 이동 대상이 아니며 기존 소유 경로를 유지한다.
