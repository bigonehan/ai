# Obsidian Daily Manager Project Instructions

## Problem Diagnosis Log

- 이 프로젝트에서 명령 실행 실패, 오류 메시지, 문서 생성 누락 또는 예상과 다른 동작을 조사할 때 `/mnt/c/users/tende/desktop/log.txt`를 먼저 읽는다.
- Windows에서 같은 로그의 경로는 `C:\Users\tende\Desktop\log.txt`이다.
- 로그가 없거나 비어 있으면 그 상태를 진단 근거로 기록하고, 소스 코드와 실제 실행 환경을 계속 확인한다.
- 로그에 토큰, API 키, 개인 경로 또는 민감한 문서 내용이 있으면 사용자 대면 출력과 작업 기록에서 마스킹한다.
- 로그 내용만으로 원인을 단정하지 않고, 해당 Task·Action의 생산 값과 실제 소비 경계를 코드 및 재현 결과와 대조한다.

## Verification Artifact Workspace

- 테스트가 실제로 필요로 하는 임시 파일만 이 프로젝트의 `test/tmp/` 아래에서 생성하고, 작업이 끝나면 지운다.
- 위 검증용 임시 파일을 프로젝트 루트나 `/home/tree/extensions` 루트에 생성하지 않는다.
- 제품 런타임이 직접 읽는 설정·manifest·fixture·데이터 JSON/TXT는 이 규칙의 이동 대상이 아니며 기존 소유 경로를 유지한다.
