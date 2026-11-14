# runtime: app (데스크톱 앱·외부 서비스)

- 실행 명령의 성공 출력과 최종 소비자 결과를 확인한 뒤 `[verify-step] runtime-start cmd=<실행 명령>`을 출력한다. 검증자는 로그 기록을 기다리거나 로그 누락으로 대기에 걸리지 않고 최종 소비자 `readback`만 확인해 끝낸다.
- 외부 앱이 결과를 받는 흐름은 그 앱이 꺼진 상태와 이미 실행 중인 상태에서 한 번씩 실행한다. 한 상태만 보면 연결·재사용 경로의 실패를 놓친다.
- 앱 화면을 못 보면 그 앱의 스크립팅·API·파일로 readback한다(Resolve는 `Resolve():GetProjectManager():GetCurrentProject():GetName()`·`GetCurrentDatabase().DbName`). 실행 중인 Windows GUI 앱의 입력·PowerShell·화면 확인은 사용 가능한 Windows MCP 도구로 Windows 쪽에서 수행한다. WSL 소켓 오류가 나면 WSL의 localhost·호스트 IP·`powershell.exe` 셸 호출로 재시도하지 않는다. Windows 앱 헤드리스 실행은 `build-win`의 `win-smoke`다.
- Windows GUI 목표를 확인하기 전, 배포본에서 목표 창이 처음부터 보이는지와 숨김 창에 들어가는 실제 경로(트레이 메뉴·실행 인자·앱 명령)를 프로젝트 실행 문서와 배포본 동작으로 확인한다. 숨김 창인데 진입 경로를 실행할 수 없으면 창이 나타나기를 기다리지 말고 `[verify-step] goal-failed row=<행> observation=hidden-window-entry-unavailable`을 출력하고 관측 불가 실패로 끝낸다. 소스에만 있는 인자를 배포본의 동작으로 간주하지 않는다.
- Windows MCP 호출 전에는 `[verify-step] mcp-start row=<행> action=<대상 동작>`을 셸에서 출력하고, 응답 직후 `[verify-step] mcp-done row=<행> action=<대상 동작> result=<관측 상태>`를 출력한다. 여러 호출을 묶어 침묵하지 않는다. 호출 자체가 정체되면 마지막 `mcp-start`의 동작을 정체 경계로 보고하며, 보지 못한 창 상태를 성공으로 기록하지 않는다.
- 숨김 트레이 메뉴·다수의 WebView 컨트롤 상태를 검증할 때는 역할 문서에 지정된 배포본 진입 동작을 한 번 실행하고, 그 실행이 새로 기록한 완료·실패와 목표별 최종 값을 한 번에 읽는다. 파일이 없거나 이전 실행의 값이거나 완료되지 않았으면 관측 불가로 즉시 실패를 보고한다. GUI 서브시스템 실행 파일의 표준 오류 출력, 트레이 좌표 반복 탐색, 컨트롤별 UIA 반복 조회로 대체하지 않는다. 저장값의 기대 인코딩은 기존 소비자가 읽는 형식과 대조하고, 다르면 첫 불일치에서 `goal-failed`로 끝낸다.
