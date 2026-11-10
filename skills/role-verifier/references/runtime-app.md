# runtime: app (데스크톱 앱·외부 서비스)

- 실행 명령의 성공 출력과 실제 앱 로그 한 줄을 확인한 뒤 `[verify-step] runtime-start cmd=<실행 명령>`·`[verify-step] app-log <실제 로그 발췌>`를 출력한다. 이 두 줄이 Stop gate의 실행 기록이다.
- 외부 앱이 결과를 받는 흐름은 그 앱이 꺼진 상태와 이미 실행 중인 상태에서 한 번씩 실행한다. 한 상태만 보면 연결·재사용 경로의 실패를 놓친다.
- 앱 화면을 못 보면 그 앱의 스크립팅·API·파일로 readback한다(Resolve는 `Resolve():GetProjectManager():GetCurrentProject():GetName()`·`GetCurrentDatabase().DbName`). 실행 중인 Windows GUI 앱의 입력·PowerShell·화면 확인은 사용 가능한 Windows MCP 도구로 Windows 쪽에서 수행한다. WSL 소켓 오류가 나면 WSL의 localhost·호스트 IP·`powershell.exe` 셸 호출로 재시도하지 않는다. Windows 앱 헤드리스 실행은 `build-win`의 `win-smoke`다.
