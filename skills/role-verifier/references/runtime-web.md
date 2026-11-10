# runtime: web·위젯·확장

- 브라우저·세션·확장 적재는 `run-cdp`와 `access-stagehand`를 따른다(공용 CDP `127.0.0.1:9222`, 배포본은 `openStagehand({ extensions: [배포 dir] })`가 버전 비교 후 적재).
- 실제 배포 런타임(`runtime :` URL)과 공용 CDP 화면에서 시작 동작부터 화면 렌더링(`img.naturalWidth > 0`, UI 상태)과 최종 소비 결과까지 실행한다.
- Stagehand/CDP 실행은 `STAGEHAND_STEP_LOG=1`로 `[stagehand-step]`·`[stagehand-bottleneck]` 로그를 남기고, 이번 실행 로그에 `[stagehand-step] cdp-attach-done`이 있어야 Stop gate를 통과한다. 5분을 넘기면 타임아웃으로 덮지 말고 중간 로그로 병목 경계를 찾아 마지막 줄에 보고한다.
- DOM·CDP·Stagehand 응답(JSON 객체·HTML)을 통째로 출력하지 않는다. 필요한 값(색·개수·rect top/height 등)만 뽑아 한 줄로 낸다(261109 color_map #785 덤프로 4분·8분 `내용 정체` 2회).
- 확장 런타임 함수 몽키패치·합성 이벤트로 소비자를 우회하지 않는다.
- Oat(`build-ui`의 내부 `ui-oat`)로 만든 화면에서 생긴 오류·런타임 비호환·알아야 할 동작은 통과·실패와 상관없이 `~/ai/skills/build-ui/ui-oat/references/notes.md`에 그 파일 형식으로 한 줄 추가한다(같은 내용이면 날짜·행만 덧붙임). 검증자가 쓰는 유일한 Skill 파일이다.
- 사용자나 위임 프롬프트가 요청하지 않은 dev 서버(`npm run dev` 등)는 실행하지 않는다.
