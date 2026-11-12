---
name: build-win
description: Windows 데스크톱 앱(Blender 등) 런타임을 WSL에서 헤드리스로 실행해 확인하는 위임자 절차 — `~/ai/scripts/bin/win-smoke` 사용법과 출력 판정. Codex sandbox가 Windows 실행 파일을 돌리지 못하므로 위임자(메인)가 쓴다. 브라우저·확장 런타임(검증자의 run-cdp)이나 일반 단위 테스트에는 쓰지 않는다.
---

# Build Win

Windows 데스크톱 앱의 헤드리스 런타임 확인은 위임자가 `win-smoke`로 실행한다. 실행 중인 GUI 앱에 입력하거나 화면을 확인하는 검증자는 `role-verifier/references/runtime-app.md`에 따라 사용 가능한 Windows MCP 도구를 쓴다. Codex sandbox는 Windows 실행 파일을 돌리지 못하고, `blender.exe` 직접 실행은 세션이 닫히지 않아 `main-edit-gate`가 거부한다.

## 실행

```
~/ai/scripts/bin/win-smoke --project <dir> --script <py> [--exe <path>] [--user-prefs] [-- args]
~/ai/scripts/bin/win-smoke --tauri-exe <Windows에 배포한 exe의 WSL 경로>
```

- `--script`: 프로젝트 기준 상대 경로 또는 절대 경로의 Python 스크립트. WSL 경로는 `wslpath`로 바꿔 넘긴다.
- `.blend`의 렉·상태 원인 조사는 데이터블록을 한 번 조회해 판단한다. 엔진·조건별 `bpy.ops.render.render()` 반복 비교 벤치마크는 실행하지 않는다. `win-smoke`는 스크립트에 반복 렌더 호출 또는 `def bench` 루프가 있으면 실행 전 `exit=2`로 거부한다.
- `--exe`: 없으면 `/mnt/c/Program Files/Blender Foundation/Blender */blender.exe` 중 최신 버전을 고른다.
- 기본 인자: `-b --factory-startup --python-exit-code 1 --python <script>`. 사용자 설정·애드온이 필요하면 `--user-prefs`로 `--factory-startup`을 뺀다.
- `--` 뒤 인자는 스크립트에 그대로 넘긴다.
- `--tauri-exe`: 배포된 Tauri v2 exe를 Windows PowerShell로 실행하고, 콘솔 자식 프로세스 0개와 WebView2의 `overlay`·`settings` 페이지에서 `event.listen` ACL 및 오류 문구를 검사한다. Tauri 창 이름이 다른 프로젝트는 이 고정 검사가 맞지 않으므로 별도 구현이 필요하다.

## 판정

- 출력: `run: <실행한 명령>`, 스크립트가 찍은 키워드 줄, 마지막 `exit=<n>`.
- `exit=0`만 통과다. `exit=2`는 프로젝트·스크립트·`blender.exe` 없음, `exit=3`은 실행 불가, 그 밖의 값은 앱 안 스크립트 실패다.
- 실패면 위임자는 소스를 고치지 않고 `run:` 줄과 마지막 오류 줄을 `$I`로 기록해 대행자에게 재위임한다.
- 프로젝트에 버전 올리기·배포 전 런타임 확인이 필요하면 `.ai/docs/role-delegator.md` `# 제출점검`에 `win-smoke` 명령을 둔다(`dispatch-agent`가 제출마다 실행).
- Tauri v2 새 프로젝트는 제출점검에 `windows_subsystem = "windows"`, `src-tauri/capabilities/*.json`의 창 범위와 `core:event` listen 권한, `tauri.conf.json`의 capability 참조 검사를 둔다. 배포 후 `--tauri-exe`를 실행하고 `exit=0`과 `SMOKE` 줄을 확인한다. 배포본이 사라지면 `Get-MpThreatDetection`으로 Defender 격리를 확인한다.
