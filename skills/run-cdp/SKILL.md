---
name: run-cdp
description: "Check the visible start action, rendered screen and authoritative consumer of a deployed web·widget·extension runtime in the shared CDP browser. Called by role-verifier for `runtime :` web·widget·extension rows."
---

# Run CDP browser

- 확인 범위는 시작 동작부터 화면 렌더링(`img.naturalWidth > 0`, 에러 토스트 유무 등)과 최종 소비 결과까지다(아래 Whole flow). 로거 변경이라도 `chrome.downloads` 구독, 이벤트 연결, 저장, 누락·중단 집계가 바뀌었으면 요청 ID와 최종 소비 결과를 확인한다.

## Browser and session

- 브라우저·프로필·세션 열기·확장 로드와 갱신은 `access-stagehand`를 이용한다(`headless: false`, 공용 `:9222`, 전용 프로필 하나). headless는 프로젝트 `AGENTS.md`가 요구할 때만 쓴다.
- Windows Chrome 디버깅 프로필은 쓰지 않는다. Windows 호스트 프로세스·포트는 PowerShell/CIM으로 확인한다.
- 사용자 흐름은 Stagehand로, 정확한 DOM 읽기는 `session.page`로 한다. raw CDP(`/json/version`, `/json/list`)는 endpoint·target 확인에만 쓴다.

## Whole flow

`runtime :`·`row:` 줄과 행마다 시작 동작·요구 결과는 위임 프롬프트에 있다. `AGENTS.md`·`grist task`로 다시 찾지 않는다.

1. `openStagehand({ url, extensions: [배포 dir] })`로 세션을 연다. 확장 버전 비교·적재는 `openStagehand`가 하고, 맞지 않으면 그 오류 줄이 실패 경계다.
2. 시작 동작 전에 최종 소비자 상태를 읽는다.
3. 실제 시작 동작을 하고 producer → transport/handler → 페이지·사이트 → 최종 소비자를 따라가 소비자를 다시 읽는다. 빈 페이지·target 목록·빌드·로그·ACK만으로는 런타임 결과가 아니다.
4. 결과는 `role-verifier`의 `task-hidden`·`readback:` 줄로 남긴다. 자격 증명·쿠키·인증 헤더·비공개 쿼리 값은 기록하지 않는다.

반복 측정마다 공용 브라우저의 탭·창을 만들거나 닫지 않는다. attach해 기존 target을 읽고 연결만 끊으며, 페이지가 필요한 확인은 탭 하나를 한 번 열어 재사용한다.

새 근거가 있는 실패만 1회 재시도하고, 확인 하나에 브라우저 작업은 약 5분이다. 새 근거 없이 탭·사이트를 반복해 열지 않는다. 5분을 넘기면 중간 로그로 첫 실패 단계를 찾아 끝내는 줄에 쓴다.

## 시작 동작 라우팅

위임 프롬프트의 행별 시작 동작 종류에 맞는 파일 하나만 읽는다. 여러 종류가 섞인 행은 해당 파일을 모두 읽는다.

| 시작 동작 | 읽을 파일 |
|---|---|
| 사용자 입력: 버튼 클릭·텍스트 입력·업로드·재생·다운로드 | `references/input.md` |
| 사용자 행동: drag 등 연속 마우스 제스처 | `references/action.md` |
| 로그인·결제·비가역 전송·비용이 걸린 화면 | `references/login.md` |

## Checks that decide the result (공통)

- After a service-worker restart, page reload or SPA navigation, read one product-owned readiness marker before continuing; a reload command alone does not prove the page uses the new build. Watch for stale listeners, ports and target IDs.
- Name the first failing step instead of "CDP failed": endpoint absent/unreachable, Stagehand Runtime not running (the `stagehand.mjs` error boundary; `Stagehand initialization timed out` alone is not a cause — read the browser's extension flags), wrong page or profile, stale or missing extension build, user action not received, message dropped or duplicated, handler rejected the payload, browser effect missing, consumer missing/stale/empty/unreadable.

## Completion

선언된 최종 소비자가 기대 결과를 보일 때만 통과다. 실패하면 소스·버전·`$H`를 바꾸지 않고 첫 실패 단계를 `실패 — <행>: <입력> ⇒ <실제 결과> (기대: <요구 결과>)`로 끝낸다. `$I`·`$H=진단` 기록과 재위임은 `dispatch-agent`·위임자가 한다.
