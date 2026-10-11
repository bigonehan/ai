# Suno Downloader Project Rules

## Version

- 기능 개선, 신규 기능 또는 수정 작업 시 patch 버전을 `0.0.1` 올린다.
- `manifest.json`과 `package.json`의 버전을 항상 동일하게 유지한다.

## Copy

- 작업 완료 후 실행 파일을 `/mnt/h/code/suno_downloader`에 폴더 없이 직접 복사한다.
- 복사 대상은 `manifest.json`, `core.js`, `content.js`, `popup.html`, `popup.css`, `popup.js`이다.
- Debug 관련 파일인 `background.js`, `debug.js`도 함께 복사한다.
- 다운로드 상태 관련 파일인 `download-state.js`도 함께 복사한다.
- 복사는 한 번 실행하고 종료 코드만 확인한다.

## Download Debug Inspection

- Suno 다운로드 동작을 점검할 때 Windows 바탕화면의 `C:\Users\tende\Desktop\suno-log.txt`를 확인한다.
- WSL에서는 `/mnt/c/users/tende/Desktop/suno-log.txt`를 사용한다.
- 최신 `runId`에서 `popup_click`, `content_command_received`, `mp3_native_click`, `mp3_dom_click_observed`, `mp3_menu_dismissed`, `browser_download_created` 순서와 마지막 오류 단계를 확인해 클릭 누락과 전달 실패를 구분한다.

## Verification Artifact Workspace

- 테스트가 실제로 필요로 하는 임시 파일만 이 프로젝트의 `test/tmp/` 아래에서 생성하고, 작업이 끝나면 지운다.
- 위 검증용 임시 파일을 프로젝트 루트나 `/home/tree/extensions` 루트에 생성하지 않는다.
- 제품 런타임이 직접 읽는 설정·manifest·fixture·데이터 JSON/TXT는 이 규칙의 이동 대상이 아니며 기존 소유 경로를 유지한다.
