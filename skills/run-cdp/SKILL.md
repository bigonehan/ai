---
name: run-cdp
description: "Read network or storage values outside the visible screen of a deployed web·widget·extension runtime. Called by role-verifier only when the row goal needs those values."
---

# Run CDP browser

- 화면 클릭·타이핑·화면 글자·스크린샷은 windows-mcp 또는 상주 CDP 중계(`stagehand.mjs relay`)로 확인한다. 이 Skill의 화면 밖 조회(`run_cdp`, `relay storage`·`eval`·`network`)는 행 목표에 network·storage 값이 명시된 경우에만 쓴다. 확장 적재·전체 웹 흐름만으로 화면 밖 조회를 시작하지 않는다. `main-edit-gate`가 해당 조회 전에 `grist goal`로 판정한다.

## Browser and session

- `dispatch-agent --role verifier` 사전점검이 공용 `:9222` 브라우저와 상주 중계 데몬(`stagehand.mjs serve --detach`, 소켓 `/tmp/access-stagehand-locks/relay.sock`)을 먼저 띄운다. 검증자는 `openStagehand`나 `test/tmp/*.mjs` 스크립트를 직접 쓰지 않고(`main-edit-gate`가 거부) 아래 한 줄 요청만 보낸다:
  ```bash
  R=~/ai/skills/access-stagehand/scripts/stagehand.mjs
  node $R relay '{"op":"open","url":"https://example.com","extensions":["/mnt/h/code/ext"]}'
  node $R relay '{"op":"type","selector":"#host >> #api-key","text":"abc123"}'
  node $R relay '{"op":"click","selector":"#host >> button.save"}'
  node $R relay '{"op":"read","selector":"#host >> #api-key","field":"value"}'
  node $R relay '{"op":"storage","extensionId":"<id>","keys":["obsidianApiKey","gristApiKey"]}'
  node $R relay '{"op":"screenshot","path":"test/tmp/874.png"}'
  ```
- Windows Chrome 디버깅 프로필은 쓰지 않는다. Windows 호스트 프로세스·포트는 PowerShell/CIM으로 확인한다.
- `relay`는 `DOM.getDocument({ pierce: true })`로 닫힌(closed) Shadow DOM과 교차 출처 `iframe >> selector`를 탐색하고, `type`은 `Input.dispatchKeyEvent`로 실제 키 입력을 보낸다. 각 호출은 `[verify-step] relay op=… ok=true|false …`(300자 상한) 한 줄만 출력하며, 같은 `op`·`selector` 측정이 연속 2회 실패하면 `measure-failed (2회)`를 반환하므로 그 즉시 `[🔳]`로 종료한다.
- `chrome://extensions` 등 브라우저 내부 페이지는 DOM 대상이 아니다(`relay` `open`이 내부 URL을 거부한다). 확장 적재·버전 상태는 `{"op":"open","url":"<일반 페이지>","extensions":["<배포 dir>"]}` 응답으로 확인한다.

## Whole flow

`runtime :`·`row:` 줄과 행마다 시작 동작·요구 결과는 위임 프롬프트에 있다. `AGENTS.md`·`grist task`로 다시 찾지 않는다. 아래 절차는 행 목표가 network·storage의 전체 흐름을 요구할 때만 적용한다.

1. `node ~/ai/skills/access-stagehand/scripts/stagehand.mjs relay '{"op":"open","url":"…","extensions":["<배포 dir>"]}'`로 상주 중계 탭을 연다. 확장 버전 비교·적재는 `relay`가 하고, 맞지 않으면 그 `reason`이 실패 경계다.
   Grist `/p/47`에서는 `{"op":"wait","selector":"iframe >> [data-testid=test-custom-widget-ready]"}`와 작업 카드 선택 표시(`1 OF N`)를 기다린 뒤 대상 작품을 고르고 시작 동작을 한다.
2. 시작 동작 전에 최종 소비자 상태를 `relay` `read`·`storage`로 읽는다.
3. 실제 시작 동작(`click`·`type`)을 하고 최종 소비자를 다시 읽는다. 빈 페이지·target 목록·빌드·로그·ACK만으로는 런타임 결과가 아니다.
4. 결과는 `role-verifier`의 `task-hidden`(`--test`에도 `stagehand.mjs relay` 명령 사용)·`readback:` 줄로 남긴다. 자격 증명·쿠키·인증 헤더·비공개 쿼리 값은 기록하지 않는다.

Grist 화면에서 테스트할 작품·행과 위젯 연결 상태는 열린 공용 CDP 페이지의 렌더된 카드·iframe으로 찾는다. `_grist_Views_section`·`_grist_Filters` CLI 조회나 `*.bundle.js` curl 조회로 UI를 역추적하지 않는다(`main-edit-gate`가 거부). `grist get Table7/Table34`는 화면 동작 뒤 저장된 값의 최종 readback에만 쓴다.

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
