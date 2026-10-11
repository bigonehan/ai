# Working Rules

## Project Boundary
- Treat this folder as an independent project.
- When Codex is running from this folder, modify only files inside this folder.
- Do not modify parent folders, sibling project folders, or files outside this folder unless the user explicitly names an external path and asks for that change.

## Version Bump
- When code is improved or a feature is added, increase the version by `0.0.1`.
- Keep `package.json` and `manifest.json` versions aligned.

## Dist Copy
- When code is modified for an improvement or new feature, rebuild `dist`.
- After rebuilding, copy the distributed output into `/mnt/h/code/flow_video_downloader`.
- Use `npm run build` to regenerate `dist` and refresh `/mnt/h/code/flow_video_downloader`.

## Path Alias
- When `current.png` is mentioned in this project workflow, it refers to the `/mnt/c/users/tende/desktop` location, with the default file path `/mnt/c/users/tende/desktop/current.png`.

## Verification Artifact Workspace

- 테스트가 실제로 필요로 하는 임시 파일만 이 프로젝트의 `test/tmp/` 아래에서 생성하고, 작업이 끝나면 지운다.
- 위 검증용 임시 파일을 프로젝트 루트나 `/home/tree/extensions` 루트에 생성하지 않는다.
- 제품 런타임이 직접 읽는 설정·manifest·fixture·데이터 JSON/TXT는 이 규칙의 이동 대상이 아니며 기존 소유 경로를 유지한다.
