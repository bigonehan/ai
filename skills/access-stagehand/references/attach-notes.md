# Stagehand 초기화·attach 기록

세션을 열기 전에 `먼저 할 일`을 따른다. 초기화·attach가 실패했거나 열기에 30초 넘게 걸리면 `[stagehand-summary]` 줄의 오류 경계 이름으로 아래 `오류별 제안`을 찾아 따른다. 표에 없는 경계·증상이면 같은 형식의 2줄(`증상 :` 경계 이름과 오류 원문, `제안 :` 확인된 복구 동작)을 추가하고, 있는 항목은 제안이 바뀔 때만 고친다. 날짜·프로젝트·소모 시간은 적지 않는다(changes `### 메모`에 남긴다). 비밀값·쿠키·프롬프트 원문은 적지 않는다.

## 먼저 할 일
1. `openStagehand({ url, extensions })`만 호출한다. 잠금, 죽은 보유자 회수·Runtime 리셋, hung 탭 점검, 오류별 1회 재시도, 확장 버전 비교·적재, 잠금 아래 브라우저 재시작은 스크립트가 한다. `doctor` 선행, `run_cdp --replace`, 셸 `timeout` 래핑을 하지 않는다.
2. `try { … } finally { await session.close(); }`로 끝낸다. 같은 셸 명령에 `pkill -f <스크립트명>`을 넣지 않는다(자기 셸도 죽는다).
3. 느리면 `node ~/.codex/skills/access-stagehand/scripts/stagehand.mjs stats`로 단계별 p50·p90을 본다.
4. iframe 안 요소는 `page.locator("iframe >> <selector>")`로 다룬다. `page.frames()`·`stagehand.context`는 없다. `url`을 생략해도 `session.page`·`session.pages`를 쓴다.
5. 프로젝트 `.ts`와 함께 임포트할 때는 `node --experimental-transform-types --input-type=module -e '...'`를 쓴다(`npx tsx -e`·strip-only `node` 금지).
6. Grist 레코드는 좌표 클릭 대신 URL 앵커 `#a1.s<section id>.r<row id>`로 연다.

## 오류별 제안
증상 : `runtime-already-initialized` — `A Stagehand instance is already initialized`·`initialization is already in progress`
제안 : 스크립트의 Runtime 재시작 1회 뒤에도 남으면 무응답 탭(무한 루프·대형 캔버스) 때문이다. `stagehand.mjs doctor --restart-runtime --close-hung` 한 번 뒤 재실행한다.

증상 : `stagehand-create-timeout` — `Stagehand initialization timed out after 60000ms`
제안 : 같은 프로세스에서 재초기화하지 않는다. `openStagehand()`가 진행 중인 create의 종료를 기다린 뒤 오류를 반환한다. 무거운 앱 탭을 점검하고 새 프로세스에서 한 번만 재시도한다.

증상 : `stagehand-create-unsettled` — 시간 초과 뒤 기존 create가 종료되지 않음
제안 : 세션 잠금이 풀려도 기존 호출이 남을 수 있다. 해당 프로세스를 종료하고 새 프로세스에서 시작한다.

증상 : `stagehand-internal-page-unsupported` — `chrome://`·`devtools://` URL
제안 : 내부 페이지 DOM을 관측하지 않는다. `extensions` 적재 결과와 `chrome-extension://<id>/` target으로 확장 상태를 확인한 뒤 일반 웹 페이지에서 동작을 검증한다.

증상 : `stagehand-connect-target`·`stagehand-create-target` — `Target.attachToTarget` `No target with given id found`
제안 : 다른 세션이 Runtime SW를 재적재해 targetId가 바뀐 것이다. 같은 스크립트를 1회 재실행하고, 계속되면 동시에 공용 브라우저를 쓰는 세션을 확인한다.

증상 : `browser-handle-attached` — `This browser is already attached to a Stagehand instance`
제안 : 죽은 보유자의 Runtime 상태가 남은 것이다. 일반 `openStagehand()` 재실행으로 풀리며, 남으면 `doctor --restart-runtime`.

증상 : `stagehand-extension-blocked` — `--disable-extensions-except`로 뜬 브라우저
제안 : 레거시 플래그 브라우저다. `stagehand.mjs browser --replace --extension <dir1,dir2>`로 교체한다(`/tmp/run_cdp-9222.log`의 `DidStartWorkerFail`도 같은 처리).

증상 : `stagehand-extension-unloadable`·`extension-debugging-unavailable` — `--enable-unsafe-extension-debugging` 없음
제안 : 오류 문구의 `run_cdp --replace -e <dir1,dir2>`를 그대로 실행한다. 여러 확장은 `-e` 반복이 아니라 쉼표 하나로 넘긴다.

증상 : `extension-version-stale` — `expected=<v> actual=none`
제안 : 확장이 비활성 상태로 적재됐다. 브라우저 플래그에 `--disable-extensions-except`가 있는지 보고 `browser --replace`로 교체한다. 플래그가 없으면 `manifest.json` 버전과 dist 빌드 시점을 확인한다.

증상 : `extension-load-failed`·`runtime-extension-load-failed`·`extension-manifest-absent`
제안 : 경로에 빌드된 `manifest.json`이 있는지 확인하고 빌드 후 재실행한다.

증상 : `runtime-wake-failed`·`runtime-worker-absent`·`runtime-worker-attach-failed`
제안 : Runtime SW가 깨어나지 않았다. `doctor --restart-runtime` 뒤 재실행하고, 남으면 `browser --replace`.

증상 : `browser-endpoint-not-ready`·`endpoint-absent` — 9222 `ECONNREFUSED`
제안 : 브라우저가 떠 있지 않거나 기동 중 죽었다. `stagehand.mjs browser --replace --extension <dirs> --detach`로 띄우고 `/tmp/run_cdp-9222.log`의 마지막 오류를 보고한다.

증상 : `browser-endpoint-still-running`·`browser-profile-still-running`
제안 : 다른 세션이 프로필을 쓰고 있다. 그 세션의 종료를 기다리거나 사용자에게 공용 브라우저 교체 허락을 받는다.

증상 : `runtime-lock-timeout` — `owner=<pid> stage=<단계>`
제안 : 다른 세션이 잠금을 쥐고 있다. `owner` pid가 살아 있으면 끝나기를 기다리고, 죽었으면 재실행한다(스크립트가 회수한다).

증상 : `cdp-timeout: <method>`·`RPC response timed out`
제안 : 대상 탭 renderer가 멈췄다. `doctor --close-hung` 뒤 재실행한다.
