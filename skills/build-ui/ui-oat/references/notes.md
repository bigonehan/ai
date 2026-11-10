# Oat 화면 기록

검증자가 Oat 화면 검증 중 생긴 오류·런타임 비호환·알아야 할 동작을 한 줄씩 추가한다. 형식: `- YYMMDD <프로젝트> #<행> <런타임·버전> — <관측> → <원인·대응>`. 같은 내용이 있으면 날짜·행만 덧붙인다.

- 261107 eagle_folderStructure #727 Eagle 4.0.0(Electron 22.3.7, Chromium 108) — daisyUI 5 oklch()·color-mix() 무효로 input 테두리 투명·글꼴 이상 → Oat로 전환, Oat도 light-dark()·중첩·color-mix 사용하므로 lightningcss `--targets "chrome 108"` 변환본 필요
- 261107 (조사) Oat 1.1.0 변환본에 남는 `var()` color-mix 13곳(`--_hov` 10, border-color 2, background-color 1) — Chromium 108에서 버튼 hover 색·secondary 테두리가 빠짐 → 자체 CSS에서 `--_hov: var(--muted)`처럼 Oat 변수만으로 폴백 지정(임의 색 금지)
- 261107 (조사) Oat 1.1.0 JS의 ot-dropdown·toast는 Popover API(Chromium 114+) — Chromium 108에서 쓰지 않음
- 261107 win_optimized_video #749·#751·#754 브리지 v0.1.13 — `/src/theme.css`·Oat CSS·JS가 404여서 기본 버튼 배경이 보이고 드래그 선택 사각형·카드 강조가 화면에서 사라짐 → 브리지 정적 파일 응답을 추가한 뒤 배포 화면에서 다시 확인
- 261107 win_optimized_video #749 브리지 v0.1.13 — Oat 1.1.0 JS가 아이콘 탭의 `title`을 초기화 뒤 tooltip으로 옮겨 hover·click 후 DOM `title`이 사라짐 → 속성은 초기 DOM에서 확인하고 동작 중에는 `aria-label`과 tooltip을 확인
- 261109 color_map #787 Vite v0.1.0·Chromium 153 — 색→영문자 입력칸 top=320px·height=39px, 확인 버튼 top=316px·height=43px로 동일 행 4px 불일치 → `.letter-answer-row`에서 두 컨트롤의 렌더 높이와 상단 정렬을 맞춘 뒤 CDP에서 재측정 필요
