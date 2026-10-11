## Project Rules

### Project Boundary
- Treat this folder as an independent project.
- When Codex is running from this folder, modify only files inside this folder.
- Do not modify parent folders, sibling project folders, or files outside this folder unless the user explicitly names an external path and asks for that change.

### Desktop EXE Delivery Rule
- 이 저장소에서 모든 구현 작업이 완료되면 최종 보고 전에 반드시 Windows exe를 빌드하고 바탕화면으로 복사한다.
- 기본 산출물 경로는 `src-tauri/target/x86_64-pc-windows-gnu/release/tauri-shorclip.exe`로 고정한다.
- 바탕화면 전달 파일명은 항상 현재 시각 postfix를 포함한 `tauri-shorclip-DD-HH-MM-SS.exe` 형식으로 생성한다.
- 기본 복사 대상 경로는 `/mnt/c/Users/tende/Desktop/tauri-shorclip-DD-HH-MM-SS.exe` 패턴으로 고정한다.
- 기본 대상 경로 복사에 실패하면 동일 폴더에 대체 파일명(예: `tauri-shorclip-DD-HH-MM-SS-new.exe`)으로 재복사한다.
- 바탕화면 대상 파일 존재를 확인하기 전에는 완료 보고를 금지한다.

### Settings Completion Build Rule
- 설정 관련 기능 수정/개선 작업이 완료되면 최종 보고 전에 반드시 Windows exe 빌드를 실행한다.
- 필수 명령: `cargo build --release --target x86_64-pc-windows-gnu` (workdir: `src-tauri`)
- 빌드 후 산출물 `src-tauri/target/x86_64-pc-windows-gnu/release/tauri-shorclip.exe` 존재를 확인한다.
- 빌드 성공 후 산출물을 바탕화면 경로 `/mnt/c/Users/tende/Desktop/tauri-shorclip-DD-HH-MM-SS.exe` 형식의 새 파일명으로 복사한다.
- 바탕화면 대상 파일 존재를 확인하기 전에는 완료 보고를 금지한다.
- 바탕화면 복사 실패 시(권한/잠금/덮어쓰기 실패 포함) 동일 경로에 대체 파일명(예: `tauri-shorclip-DD-HH-MM-SS-new.exe`)으로 재복사해 산출물 전달을 완료한다.

### Completion Install Rule
- 이 저장소에서 구현 작업이 완료되면 최종 보고 전에 반드시 `cargo install --path src-tauri`를 실행한다.
- 설치 완료 후 `tauri-shorclip` CLI가 바로 실행 가능한 상태인지 확인하고 그 결과를 최종 보고에 포함한다.

### Cargo Version Bump Rule
- 이 저장소에서 `tauri_shortclip` 기능 추가나 개선 작업을 완료할 때마다 최종 보고 전에 `src-tauri/Cargo.toml`의 `version`을 반드시 올린다.
- 기본 증가 단위는 `0.0.1`이다.
- 마지막 patch 자릿수는 최대 2자리까지만 허용한다.
- 따라서 버전 증가는 `0.1.0 -> 0.1.1 -> ... -> 0.1.99 -> 0.2.0` 규칙으로 처리한다.
- 버전 상승 없이 기능 추가/개선 작업을 완료 처리하면 실패다.

### UI Consistency Memory Rule
- 사용자가 `current.png`로 UI 문제를 지적한 턴 이후에는 기본 기준 이미지를 `/mnt/c/Users/tende/Pictures/Screenshots/current.png`로 우선 확인한다.
- `current.png` 기반 UI 수정에서는 이미지에 보인 에셋의 상대 위치를 유지한다. 상하/좌우/포함/동일 컨테이너/헤더-리스트-item 계층 관계를 임의로 바꾸면 완료 보고를 금지한다.
- UI 변경 시 같은 화면 안의 action 버튼은 크기, radius, padding, 아이콘 크기, primary/secondary 색 체계를 섞지 않고 한 세트로 맞춘다.
- prefix 버튼/추가 폼/편집/확인/취소/footer 액션처럼 한 흐름에 속한 버튼이 브라우저 기본 스타일과 프로젝트 primary 버튼 스타일로 섞여 있으면 완료 보고를 금지한다.

### Explicit UI Preservation Rule
- 사용자가 기존 UI 요소 아래에 항목을 `추가`, `표시`, `보이게` 하라고 지시하면, 기존 UI 요소의 이름/헤더/라벨/버튼 텍스트는 기본적으로 유지 대상으로 간주한다.
- 예: `prefix group 아래 item 표시`는 `prefix group 헤더 유지 + 그 아래 item 추가 표시`로 해석한다.
- 사용자가 명시적으로 `지워`, `숨겨`, `이름 빼`, `헤더 제거`를 말하지 않았는데 기존 요소를 제거/치환/축소 해석하면 실패다.
- 구현 전 체크리스트에 `유지해야 하는 기존 명시 요소`와 `새로 추가해야 하는 하위 요소`를 분리해서 적는다.
- UI 검증 시에는 `기존 명시 요소가 그대로 보이는지`와 `추가 지시된 하위 요소가 그 아래에 보이는지`를 둘 다 확인해야 한다.

## Verification Artifact Workspace

- 테스트가 실제로 필요로 하는 임시 파일만 이 프로젝트의 `test/tmp/` 아래에서 생성하고, 작업이 끝나면 지운다.
- 위 검증용 임시 파일을 프로젝트 루트나 `/home/tree/extensions` 루트에 생성하지 않는다.
- 제품 런타임이 직접 읽는 설정·manifest·fixture·데이터 JSON/TXT는 이 규칙의 이동 대상이 아니며 기존 소유 경로를 유지한다.
