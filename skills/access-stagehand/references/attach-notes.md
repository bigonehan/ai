# Stagehand 초기화·attach 기록

세션을 열기 전에 이 문서를 읽고 `먼저 할 일`부터 따른다. 초기화·attach가 실패했거나 열기에 30초 넘게 걸렸다면, 작업을 끝내기 전에 아래 `기록` 형식으로 항목을 추가한다. 같은 원인이면 기존 항목의 `재발`에 날짜와 프로젝트를 덧붙인다. 원인이 확인되어 스크립트나 절차가 바뀌면 `먼저 할 일`을 고친다. 비밀값·쿠키·프롬프트 원문은 적지 않는다.

## 먼저 할 일
1. `openStagehand()`는 이제 attach 직전에 자동으로 방치된 이전 Stagehand 클라이언트 프로세스를 정리(`killStaleStagehandClients`)하고 `runDoctor(..., { restartRuntime: true, closeHung: true })`로 무응답 탭 복구 및 Stagehand Runtime service worker 재시작을 수행하며, 유휴 세션 자동 해제 타이머(`leaseTimeoutMs`, 기본 90초)와 0.3초대의 빠른 `session.close()`를 내장한다.
2. 수동 점검이 필요할 때만 `node ~/.codex/skills/access-stagehand/scripts/stagehand.mjs doctor --restart-runtime --close-hung` 또는 `node ~/.codex/skills/access-stagehand/scripts/stagehand.mjs open --url about:blank`를 실행한다(정상 ~1초).
3. 스크립트는 `try { … } finally { await session.close(); }`로 끝내고, 셸 `timeout`은 스크립트 예상 시간보다 길게 잡는다.
4. Grist 위젯처럼 iframe 안의 요소는 `page.locator("iframe >> <selector>")`로 다룬다. Stagehand의 `page`에는 `frames()`가 없고 `stagehand.context`도 없다(`session.page`와 `session.pages`를 바로 사용한다).
5. Node 25 인라인 스크립트에서 프로젝트 `.ts` 파일(`server/obsidian.ts` 등)과 `openStagehand()`를 함께 임포트할 때는 반드시 `node --experimental-transform-types --input-type=module -e '...'`를 사용한다(`npx tsx -e`는 CJS top-level `await` 오류, 기본 `node` strip-only 모드는 `readonly` 생성자 파라미터 구문 오류 발생).
6. Grist 레코드 이동은 화면 좌표 클릭(`page.click(x, y)`) 대신 URL 앵커 `#a1.s<section id>.r<row id>`로 연다. section id는 `~/ai/scripts/bin/grist get _grist_Views_section`의 `parentId`(=page의 viewRef)로 찾는다.

## 기록
형식: `### YYMMDD 증상 한 줄` 아래에 `- 프로젝트 :`·`- 증상 :`(오류 원문)·`- 원인 :`·`- 복구 :`·`- 소모 :`(낭비한 시간·시도 수)·`- 재발 방지 :`·`- 재발 :`.

### 261014 Runtime 고착 — already initialized / initialization is already in progress
- 프로젝트 : grist_video_maker (발상하기 흐름 테스트). 이전 grist_video_maker #31 기록에도 같은 증상.
- 증상 : `A Stagehand instance is already initialized`, Runtime 확장 재로드·service worker 재시작 뒤에는 `Stagehand initialization is already in progress`(첫 시도 12초 대기 후 실패).
- 원인 : 공유 브라우저의 page target 3개(`localhost:5184`, Grist `/o/docs/` 2개)의 renderer가 응답하지 않았다(`Runtime.evaluate` 4초 무응답, 대화상자·디버거 정지·frozen 아님). Runtime 초기화가 모든 page를 붙이다 멈춰 `initializationInProgress`가 풀리지 않았다.
  - 재현: 무한 루프 탭 1개를 만든 뒤 Runtime을 재시작하자 `open`이 `Stagehand initialization timed out after 60000ms`로 실패했다.
  - 클라이언트 강제 종료(SIGKILL)만으로는 고착이 재현되지 않았다.
- 복구 : 무응답 탭 `Page.reload` → Runtime service worker에서 `chrome.runtime.reload()` → `open` attach 0.4초 성공.
  - 지금은 `stagehand.mjs doctor --restart-runtime` 한 번이 같은 일을 하고, reload로 안 풀리는 탭(무한 루프)은 `--close-hung`로 닫는다.
  - `Extensions.loadUnpacked` 재로드와 SW `Target.closeTarget`은 효과가 없었다.
- 소모 : 원인 파악 전 재시도·우회(raw CDP 클릭) 약 20분, Stagehand 열기 시도 8회.
- 재발 방지 : `먼저 할 일` 2·3번. `openStagehand()`는 이 오류를 `runtime-stuck: … doctor --restart-runtime` 안내로 바꿔 던진다.
- 재발 : 261015 grist_video_maker 예상시간 화면 확인 전 `open --url about:blank`가 60초 timeout. `doctor --restart-runtime`에서 hung 0·runtimeRestarted true, 재시도 attach 0.57초 성공. Grist URL 직접 open에서도 60초 timeout 재발, doctor에서 hung 0·runtimeRestarted true, 재시도 attach 0.83초 성공. List 씬 렌더 UI 측정 전 `open --url about:blank`도 60초 timeout. 원인 target 무응답은 관측되지 않아 미확정. 같은 날 #60 흐름 측정 전에도 about:blank 60초 timeout → doctor(checked 22, hung 0, runtimeRestarted true) 뒤 1.4초 attach. 이후 실행마다 doctor를 먼저 돌려 3회 모두 성공. 261018 grist_video_maker #167 초기문서 생성 검증에서 `doctor` 없이 `openStagehand()`를 바로 호출해 백그라운드 태스크로 넘어가며 약 20분 대기 발생 → `timeout 30s` 래핑 및 `doctor --restart-runtime` 선행 규칙을 `verify-work`에 반영. 261020 grist_mindmap #217 흐름 검증 첫 시도에서 60초 timeout → `doctor --restart-runtime --close-hung`(checked 6, hung 0, runtimeRestarted true) 뒤 재시도 1.67초 attach 성공. 261020 grist_video_maker #224 흐름 검증 첫 시도에서 `page.locator("iframe >> ...")` 호출이 40초 timeout → `doctor --restart-runtime --close-hung`(checked 8, hung 0, runtimeRestarted true) 후 재시도 3.0초 attach 및 `page.evaluate` 검증 성공.

### 261015 세션 종료 직후 다음 init 시간 초과 반복 (무거운 ComfyUI 탭)
- 프로젝트 : comfyui_dh_nodes
- 증상 : `Stagehand initialization timed out after 60000ms`가 정상 종료(`session.close()`)한 세션 바로 다음 실행마다 반복. doctor 결과 `hung` 없음 또는 18188 탭 1개.
- 원인 : 확인 못 함. ComfyUI 프론트엔드 탭(캔버스·대형 그래프)이 열린 상태에서만 재현.
- 복구 : 매 실행 직전 `doctor --restart-runtime --close-hung` 후 열기 → 매번 성공.
- 소모 : 시간 초과 7회, 약 8분.
- 재발 방지 : 무거운 앱 탭을 반복 측정할 때는 실행마다 doctor를 먼저 돌린다. 스크립트에서 `pkill -f <스크립트명>`을 같은 셸 명령에 넣지 않는다(자기 셸까지 종료됨).
- 재발 :

### 261015 openStagehand(url) 첫 attach에서 `No target with given id found`
- 프로젝트 : grist_video_maker
- 증상 : `doctor --restart-runtime`(hung 0) → `open --url about:blank` 성공 직후, 스크립트 `openStagehand({ url })`가 `Target.attachToTarget` `No target with given id found`로 실패.
- 원인 : 확인 못 함. 직전 `open`이 닫은 target id를 다음 attach가 참조한 것으로 추정.
- 복구 : `doctor --restart-runtime` 한 번 더(checked 32, hung 0) 실행 후 같은 스크립트 재실행 → 성공.
- 소모 : 실패 1회, 약 1분.
- 재발 방지 : `open --url about:blank` 확인 직후 바로 스크립트를 열 때도 직전에 doctor를 한 번 더 돌린다.
- 재발 : 261020 grist_video_maker #224 `connectOrLaunch()`가 재시도 루프(`createAttempt`) 안에서 매번 `runDoctor({ restartRuntime: true })`를 재호출해 Stagehand Runtime service worker 재시작 중에 다시 SW를 죽이면서 `No target with given id found`가 연쇄 발생함. 또한 `openStagehand()`에 `url` 미전달 시 `session.page`가 `undefined`였고, `page.frames()`·`stagehand.context.pages()`·`npx tsx -e`·`node`(strip-only) 시행착오로 6회(약 5분) 소모. `stagehand.mjs`에서 `runDoctor`는 최초 1회(또는 `already initialized` 시에만) 실행하도록 분리하고 `url` 생략 시에도 `session.page`·`session.pages`를 자동 바인딩하도록 수정.

### 261016 다른 세션이 공용 브라우저를 함께 쓸 때 attach·page 세션 유실 반복
- 프로젝트 : jw_downloader
- 증상 : `doctor --restart-runtime`(hung 0) 직후에도 `Target.attachToTarget` `No target with given id found` 3회, `RPC response timed out: page.url` 1회, 진행 중 세션에서 `Session with given id not found` 뒤 Suno 탭 자체가 사라짐. 같은 시각 다른 세션의 Grist 탭 3개가 새로 열림.
- 원인 : 확인 못 함. 여러 세션이 같은 9222 브라우저에서 doctor·탭 열기/닫기를 동시에 하는 경합으로 추정.
- 복구 : 없음(재시도 한도 소진). 첫 시도만 성공해 첫 곡 판독.
- 소모 : 실패 4회, 약 12분.
- 재발 방지 : 긴 재생 흐름 전에 `/json/list`로 다른 세션 탭 변화를 보고, 다른 세션이 활동 중이면 끝난 뒤 실행한다. 스크립트 정리에 `pkill -f`를 쓰지 않는다.
- 재발 :
