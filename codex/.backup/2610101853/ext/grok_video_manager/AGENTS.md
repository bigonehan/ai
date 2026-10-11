## Project Boundary
- Treat this folder as an independent project.
- When Codex is running from this folder, modify only files inside this folder.
- Do not modify parent folders, sibling project folders, or files outside this folder unless the user explicitly names an external path and asks for that change.

# Version Rule
- 내부적으로 기능 추가, 기능 개선 작업을 완료할 때마다 `package.json`의 버전을 `0.0.1`씩 올린다.
- 예시: `1.2.1 -> 1.2.2`, `1.2.9 -> 1.2.10`.
- 버전 변경은 구현 완료 직후 같은 작업에서 함께 반영한다.
- 이 규칙은 내부 버전 관리 기준으로 기억하며, `package.json`을 우선 기준으로 사용한다.

# Build Deploy Rule
- 기능 개선, 수정, 추가 작업을 완료하면 build 결과물 `dist`를 기준으로 배포한다.
- build 결과물은 zip으로 만들지 않고 `/mnt/h/code/` 아래에 풀어둔다.
- 기본 배포 대상 경로는 `/mnt/h/code/grok_video_remover`로 본다.
- 대상 폴더가 이미 있으면 최신 `dist` 내용으로 다시 복사하거나 덮어쓴다.

## Verification Artifact Workspace

- 테스트가 실제로 필요로 하는 임시 파일만 이 프로젝트의 `test/tmp/` 아래에서 생성하고, 작업이 끝나면 지운다.
- 위 검증용 임시 파일을 프로젝트 루트나 `/home/tree/extensions` 루트에 생성하지 않는다.
- 제품 런타임이 직접 읽는 설정·manifest·fixture·데이터 JSON/TXT는 이 규칙의 이동 대상이 아니며 기존 소유 경로를 유지한다.
