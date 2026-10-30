---
name: run-cdp
description: "Verifier-only (T26_ROLE=verifier, dispatch-agent --role verifier, after deployment): the main session and implementers never run CDP; the main session puts the checks in the verifier prompt and judges only exit·last. Use for a real UI check or browser behavior whose final effect cannot be established from existing development-mode logs and persisted consumer state. When used, check the visible action and authoritative consumer in the shared CDP browser."
---

# Run CDP browser

- 메인(위임자)과 대행자는 이 Skill을 실행하지 않는다. 메인은 확인할 화면·상태값·소비 결과를 배포 뒤 검증자(`dispatch-agent --role verifier`) 프롬프트의 실측 확인 조건으로 넣고, 돌아온 `exit`·`last` 한 줄로만 판정한다(main-edit-gate가 메인·대행자의 `run_cdp`·`stagehand.mjs`·`:9222` 실행을 거부).

Choose whether to use CDP from the changed behavior and the user's requested evidence, then take the target URL, artifact (extension directory), start action, and authoritative consumer from the project's nearest `AGENTS.md`. An extension project by itself does not require CDP.

## CDP 적용 결정

| 변경·요청 경계 | 검증 |
|---|---|
| 브라우저·위젯(`runtime :` URL)·확장·외부 사이트가 관여하는 모든 제품 기능·오류 수정 | **CDP 필수.** Mock 단위 테스트, `curl` 단독 호출, 저장 상태·로그만으로 CDP 화면 확인을 생략하지 않는다. 반드시 실제 배포 런타임(`runtime :` URL·위젯 서버)과 공용 CDP 브라우저에서 시작 동작을 실행하고 화면의 실제 렌더링(`img.naturalWidth > 0`, 에러 토스트 유무 등)과 최종 소비 결과를 함께 판정한다. |
| 이미 작동하는 로거의 출력 문자열·형식만 변경하고 이벤트 수집·전달·저장·브라우저 효과는 그대로임 | **CDP 생략.** 형식 함수의 기대 문자열과 기존 로거 호출을 집중 단위 테스트로 확인한다. |
| Skill·규칙·문서만 변경하거나 브라우저와 무관한 CLI 출력·순수 함수만 변경함 | **CDP 생략.** 변경된 계약의 해당 경계에서 확인한다. |

- 로거라도 `chrome.downloads` 구독, 이벤트 연결, 저장, 누락·중단 집계가 바뀌면 요청 ID와 최종 소비 결과를 CDP에서 확인한다. 로그 문자열의 모양만 바뀌면 위 형식 검사로 끝낸다.
- 사용자가 오류를 보고했으면 기존 개발 모드 로그로 실패 경계를 먼저 찾는다. 원인과 결함 위치가 이미 지정됐거나 로그로 확인됐으면 수정 전 CDP로 같은 실패를 다시 재현하지 않지만, **수정 뒤에는 반드시 실제 배포 런타임과 CDP 브라우저 화면에서 시작 동작부터 최종 화면 표시(`naturalWidth > 0` 등)·소비 결과까지 직접 확인한다(로그·저장 상태·단위 테스트만으로 CDP 검증 생략 금지).**
- 전역 Skill 절차 문구 변경의 확인은 하위 프로젝트 규칙에 이전 기준이 명시되어 새 Skill과 충돌하는지 검사하는 데까지로 한다. 충돌이 없으면 그 확인으로 끝낸다.

## Browser and profile

- The browser is always visible: `headless: false`. Use headless only when the project's `AGENTS.md` explicitly requires a headless-only runtime.
- One shared endpoint `http://127.0.0.1:9222` and one dedicated profile `/home/tree/.config/codex-run-cdp`. The profile keeps logins (for example the local Grist at `http://localhost:8484`) across sessions; never create a second profile or copy cookies.
- A healthy endpoint is attached to, never replaced implicitly. Restart it only to load a new extension build, or when `stagehand.mjs` reports `extension-not-loaded`, `stagehand-extension-blocked` or `stagehand-extension-unloadable`: `run_cdp --replace -e <extension dir>`. It keeps the Stagehand Runtime in the extension allow-list; never start the shared browser with a hand-written `--disable-extensions-except` that leaves it out.
- Do not use Playwright, per-project profiles, or a Windows Chrome debugging profile. Sites already logged in on the shared profile are never asked about again. Check Windows host processes and ports with PowerShell/CIM.

## Run with Stagehand

Stagehand itself (runtime, AI model and key, opening the session) belongs to the `access-stagehand` skill. Open the session through it; it attaches to the healthy endpoint or launches the dedicated-profile Chromium with `headless: false` and `keepAlive`, and closing the session only disconnects.

```js
import { openStagehand } from "/home/tree/.codex/skills/access-stagehand/scripts/stagehand.mjs";
const session = await openStagehand({ url, extensions: [extensionDir] });
// session.stagehand.act / observe / extract, session.page, session.mode ("attach" | "launch")
await session.close();
```

Quick check from a shell: `node ~/.codex/skills/access-stagehand/scripts/stagehand.mjs open --url <URL> [--extension <dir>]` prints mode, browser, `headless`, whether AI is enabled, URL and title.

- Use Stagehand for the user-visible flow. Raw CDP (`/json/version`, `/json/list`) normally checks only the endpoint and targets. For a drag, use Stagehand only when it sends a continuous browser-level mouse gesture; otherwise send sequential browser-level CDP mouse input to the same visible tab. Do not replace the gesture with `page.evaluate`, synthetic DOM events, or direct handler calls.
- The `run_cdp` fish function opens the same visible browser without Stagehand, for example for a one-time login: `run_cdp --url http://localhost:8484 --detach`.

## Whole flow

1. Extract the project's `runtime :` line from `AGENTS.md` with `rg -n` (do not read full files). For every Table26 row involved in this check, use the `row` line already in hand, or run `grist task <id>` once when none, before runtime work.
2. Open the session (with the deployed extension directory when the product is an extension) and confirm the loaded extension version matches the build.
3. Read the authoritative consumer's state before acting.
4. Perform the real start action, then follow producer → transport/handler → page or site → authoritative consumer, and read the consumer back. A blank page, target list, build, log, or ACK alone is not a runtime result.
5. Report the browser version, `headless: false`, target URL/title, artifact version, and the consumer readback in the command output and the project's `.ai/changes` record. Never record credentials, cookies, authorization headers, or private query values.

Allow one retry for a newly evidenced failure and about five minutes of browser work per check; do not open repeated tabs or sites without new evidence. An exhausted budget is not a reason to end the task: record the first failing step in Table26 `$I` and continue with the next repair boundary.

## Checks that decide the result

- Use Stagehand for the visible user flow and its page object (`session.page`) for exact DOM reads; use browser-level CDP input only for a gesture that Stagehand cannot perform. Playwright scripts and per-project browser profiles are legacy and are not used.
- After an extension reload/update, service-worker restart, page reload or SPA navigation, read back the loaded extension version and one product-owned readiness marker before continuing; a reload command alone does not prove the page uses the new build. Watch for stale listeners, ports and target IDs.
- Start from the real visible control (button, editor, upload, playback, download) rather than a popup, helper endpoint, synthetic event or internal function.
- For a requested drag gesture, move the browser pointer to the visible starting object, press, move through the intended range while held, and release at the visible endpoint. Capture and open the visible browser screen once while the mouse is held and again after release. In both images, check the rendered selection range, which expected objects are included or missed, and whether text or images are selected unintentionally. If either drag screenshot is unavailable or obscured, fail the interaction check until the screen can be inspected. A `dragAndDrop` return value, successful `Input.dispatchMouseEvent` replies, DOM checkbox counts, `window.getSelection()` values, or a passing unit test alone never establishes that the drag worked. If the screen disagrees with those values, treat the runtime check as failed and repair it before reporting success.
- Name the first failing step instead of "CDP failed": endpoint absent/unreachable, Stagehand Runtime not running (the `stagehand.mjs` error boundary; `Stagehand initialization timed out` alone is not a cause — read the browser's extension flags), wrong page or profile, stale or missing extension build, user action not received, message dropped or duplicated, handler rejected the payload, browser effect missing, consumer missing/stale/empty/unreadable.
- For a download or file consumer, list the destination before the flow and require a new file afterwards with the expected name, positive size, settled modification time and readable bytes. Resolve the real download folder from the running profile; an existing file with a matching name is not a result.

## Login and user-only actions

If the target shows a login page (for example Grist `403`/sign-in in the dedicated profile), payment, an irreversible send, or a cost, stop there, run the project's input notification rule, and ask the user for exactly that one action in the already-open visible browser. A login made in the dedicated profile persists, so later checks reuse it. Resume the same session afterwards and read the consumer back.

## Completion

The check passes only when the declared authoritative consumer shows the expected result. If it fails, set Table26 `$H=진단`, repair the owning code, rebuild/reload the same artifact, repeat the same start action, and read the consumer again. Item state follows `manage-project`.
