# Project Instructions

## Project Boundary
- Treat this folder as an independent project.
- When Codex is running from this folder, modify only files inside this folder.
- Do not modify parent folders, sibling project folders, or files outside this folder unless the user explicitly names an external path and asks for that change.

## Versioning
- When this project gets a functional improvement or a new feature, bump the version in `package.json` by `0.0.1` unless the user explicitly requests a different versioning policy.
- Use a patch bump by default, and keep each version segment within two digits (`00`-`99`) unless the user explicitly requests otherwise.
- Keep `public/manifest.json` in sync with the same version, and ensure the built `dist/manifest.json` is generated with that updated version before delivery.

## Delivery
- After a requested functional improvement, feature addition, or fix is completed, run the project build.
- Copy only the built contents of `dist` to `/mnt/h/code/flow_video_maker/` so the delivered files are placed directly under `/mnt/h/code/flow_video_maker/`.
- Do not copy or move the entire project folder for delivery unless the user explicitly asks for it.

## Screenshot Reference
- Unless the user explicitly says otherwise, treat the current screenshot path as `/mnt/c/users/tende/desktop/current.png`.
- When the user refers to `current.png`, resolve it to that path first.

## Persistence Rule
- If the user says `될때까지`, `해결될 때까지`, `성공할 때까지`, or `작동할 때까지`, treat it as a persistence directive.
- A persistence directive means the task stays in progress until one of the following is true:
  - the requested outcome is verified as achieved;
  - the user explicitly stops or redirects the work; or
  - no further reasonable attempt is possible because a required external permission, credential, device state, or user action is missing and that blocker has been confirmed.
- While a persistence directive is active, do not send a `final` response for intermediate failures, partial diagnosis, or speculative next steps.
- Intermediate failures must be reported only as brief progress updates, followed immediately by the next concrete fix, workaround, or retry.
- Do not present a failure summary, blocker explanation, or status recap as if the task were complete while a persistence directive is still active.

## Completion Gate
- After completing a task, run `nf -m "<task-name> complete"`.
- Immediately before the final response, check whether `nf -m` has already been run before giving any completion report; if it has not, run `nf -m`.
- The completion check order is fixed: `nf -m 실행 -> 종료 코드 확인 -> final 응답`.
- A persistence directive controls whether a `final` response is allowed at all; the completion gate applies only after a `final` response becomes allowed.

## Verification Artifact Workspace

- 테스트가 실제로 필요로 하는 임시 파일만 이 프로젝트의 `test/tmp/` 아래에서 생성하고, 작업이 끝나면 지운다.
- 위 검증용 임시 파일을 프로젝트 루트나 `/home/tree/extensions` 루트에 생성하지 않는다.
- 제품 런타임이 직접 읽는 설정·manifest·fixture·데이터 JSON/TXT는 이 규칙의 이동 대상이 아니며 기존 소유 경로를 유지한다.
