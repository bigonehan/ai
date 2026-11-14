# runtime: web·위젯·확장

- 화면·DOM·입력 확인은 windows-mcp(`screen-evidence <row> <screenshot.png> <consumer> <observed-value>`) 또는 상주 CDP 중계(`node ~/ai/skills/access-stagehand/scripts/stagehand.mjs relay '<json>'`)로 한다. `relay`는 닫힌 Shadow DOM(`DOM.getDocument pierce:true`), `iframe >> selector` 체인, 실제 `Input.dispatchKeyEvent` 타이핑을 처리하고 `[verify-step] relay op=… ok=true value=…` 한 줄만 돌려준다. 행 목표의 바뀐 화면·값만 확인하고, 목표에 없는 저장·캡처 흐름은 실행하지 않는다. `test/tmp/*.mjs` 브라우저 스크립트나 `openStagehand` 원라이너는 쓰지 않는다(`main-edit-gate`가 거부).
- 화면에 보이지 않는 network·storage 값이 목표일 때는 `run-cdp`와 `stagehand.mjs relay '{"op":"storage",…}'`·`'{"op":"eval",…}'`를 쓴다. 같은 `op`·`selector` 측정이 연속 2회 실패해 `measure-failed (2회)`가 나오면 즉시 `[🔳]`로 멈춘다.
- DOM·CDP·Stagehand 응답(JSON 객체·HTML)을 통째로 출력하지 않는다. 필요한 값(색·개수·rect top/height 등)만 뽑아 한 줄로 낸다(261109 color_map #785 덤프로 4분·8분 `내용 정체` 2회).
- 확장 런타임 함수 몽키패치·합성 이벤트로 소비자를 우회하지 않는다.
- Oat(`build-ui`의 내부 `ui-oat`)로 만든 화면에서 생긴 오류·런타임 비호환·알아야 할 동작은 통과·실패와 상관없이 `~/ai/skills/build-ui/ui-oat/references/notes.md`에 그 파일 형식으로 한 줄 추가한다(같은 내용이면 날짜·행만 덧붙임). 검증자가 쓰는 유일한 Skill 파일이다.
- 사용자나 위임 프롬프트가 요청하지 않은 dev 서버(`npm run dev` 등)는 실행하지 않는다.
- `runtime :`에 진입 URL이 여러 개면 실제로 응답하는 URL을 골라 `[verify-step] runtime-url=<URL> row=<행>`을 먼저 출력한다. 응답하지 않는 첫 URL이 살아날 때까지 기다리지 않는다. 어느 URL도 응답하지 않으면 `[verify-step] goal-failed row=<행> observation=runtime-entry-unavailable`을 출력하고 적재 실패로 끝낸다.
