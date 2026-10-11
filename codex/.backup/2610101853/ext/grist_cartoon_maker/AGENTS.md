# AGENTS.md

- Table7 Raw ID: 1200
- 관리 프로그램: 작업/종류/프로젝트/관리/작업관리기/작업관리기-문서.md
- Obsidian 프로젝트 문서: 작업/종류/프로젝트/코딩/261009.grist_cartoon_maker.md
- Table26 task Raw ID: 11
- 테스트에 필요한 임시 파일만 `test/tmp/`에 만들고 작업 후 지웁니다.

## Current work and agent handoff

- 현재 추가 작업: 만화 모드에서 원·네모·펜 도형을 컷당 하나의 SVG 말풍선 이미지로 확정하고 Eagle 프로젝트 Bubble 폴더에 `씬-컷` 이름으로 저장합니다. Grist 해당 행 `$J` Text에는 이미지 전체 위치 `x/y`를 저장합니다. 기존 말풍선은 개별 편집·삭제할 수 있지만 Eagle 활성 이미지는 컷당 한 장만 유지합니다. 텍스트 입력, Grid View, Conti→Shot 경로는 변경하지 않습니다. 완료 조건은 실제 위젯에서 확정/편집/삭제 후 Eagle Bubble Item과 `$J` fresh readback입니다.
- Bubble 0.0.40 상태: 위젯 100건 테스트·빌드 및 Bridge 0.0.26 core·Sequence 테스트는 통과했고 H: 복사본을 갱신했습니다. 5175 제품 서버는 새 산출물을 제공하지만 Host Eagle.exe가 소유한 포트 41600의 구 Bridge는 Bubble 조회에 HTTP 404를 반환합니다. 실행 중인 Eagle 플러그인의 새 버전 재적재와 실제 위젯 생성·편집·삭제, Eagle Item·Grist `$J` fresh readback이 남아 있어 완료로 처리하지 않습니다.
- 현재 Bubble 작업 runtime: Grist page `http://localhost:8484`, 위젯 `http://localhost:5175`, artifact `dist/index.html`. 시작 동작은 실제 위젯 만화 뷰에서 컷 선택 → 말풍선 생성 → 도형 그리기 → 확정이며, 이어 같은 컷의 편집·삭제를 확인합니다. authoritative consumer는 Eagle 프로젝트 Bubble 폴더의 해당 `씬-컷` 활성 Item 수·SVG 내용과 Grist 해당 행 `$J` fresh readback입니다.
- 이전 Table26 task Raw ID 9의 Grid/드롭 실제 runtime 검증은 별도로 미완료 상태를 유지합니다.

- 현재 작업 상태와 Agent가 수행해야 할 범위·완료 조건은 이 파일을 기준으로 확인합니다.
- Agent 호출 지시어에는 `이 프로젝트의 AGENTS.md를 먼저 읽고, 현재 작업 상태와 완료 조건을 기준으로 수행하라`는 내용을 포함합니다.
- Agent 프롬프트에는 실패 경계 1개, 수정 가능 경로, 완료 조건(명령과 기대 결과)만 적습니다. Agent는 진단→수정→검증을 한 번에 수행하고 결과를 일반 텍스트로 반환합니다.
- 메인 세션이 diff와 테스트 출력을 직접 읽고 `Input.md` 요구와 대조해 Table26 `$H=완료` 또는 다음 bounded repair를 결정합니다. 별도 completion-verification Agent는 Grist·Eagle·Bridge 쓰기 경로의 계약 변경, 공용 Bridge 계약 확장, 사용자 요청일 때만 호출합니다.
- Table26 `$H`는 `초안`, `진단`, `교정`, `검증`, `대기`, `완료` 중 한글 Choice 값 하나만 저장합니다. `대기`는 최종 사용자 입력만 남은 상태이며, 현재 단계는 `$H`로만 표시하고 `$C`에는 쓰지 않습니다. 단계가 바뀔 때만 씁니다.
- 검증 기록은 이 파일, `Input.md`, `.ai/changes` Markdown, Table26, 명령 출력과 authoritative consumer로 제한합니다.
- Table26 `$D`·`$I`·`$J`에는 지금 진행 중인 최신 항목 한 줄만 작업 행 안의 순번과 함께 쓰고(예: `3. 인증 단계 확립`), 1번부터의 전체 목록과 세부 내용은 같은 순번으로 날짜별 `.ai/changes`에만 기록합니다.
- 테스트: `npm test`(unit + server, 약 1초)가 기본입니다. 실제 runtime whole flow는 모든 항목 완료 후 `run-cdp`로 한 번만 수행합니다.
- runtime 설정: Grist page origin `http://localhost:8484`, 위젯 origin `http://localhost:5175`, artifact `dist/index.html`, 시작 동작은 Video Maker처럼 Conti에 있는 이미지 하나를 일치하는 만화 셀에 드롭.
- 현재 제품 작업: `grist_video_maker`의 실제 Conti→Shot 기능을 `grist_cartoon_maker` 만화 칸에 연결하는 작업입니다. GVM 선택기 클릭과 카드의 native `Files` 드롭은 서로 다른 시작 동작입니다. 이전 실제 Cartoon Maker 세션에서는 구 bundle이 로드되고 `drop-*` 로그가 0건이었습니다. 0.0.37에서는 확장자 없는 Shot 썸네일 URL의 Pixi 텍스처 파서를 명시했습니다. App 경계에는 기존 HTML/ID 드롭 테스트를 유지하고 native `Files` 드롭→지문→`/promote-shot-by-file`→Grist `$E` fresh readback→썸네일 상태를 별도 테스트로 추가했으며 전체 89개 테스트, 타입 검사, 빌드가 통과했습니다. Grist origin은 인증으로 HTTP 302이며 visible Chrome CDP endpoint가 없어 실제 사용자 시작 동작, Eagle Shot 승격, `$E` persisted readback은 여전히 미검증입니다. 이 상태를 전체 기능 완료로 전환하지 않습니다.
- authoritative consumer: 실제 Grist Cartoon Maker 위젯에서 Conti 드롭 후 Eagle Shot 승격과 해당 행 `$E` 저장값을 fresh readback한 결과입니다.
- 현재 Grid View 작업: 기존 Pixi 2열 Grid 해석을 폐기하고 `grist_video_maker`의 카드형 Grid를 기준으로 반응형 16:9 카드, Shot 이미지, 씬·컷 번호, 카드 선택, Conti 파일 드롭을 Cartoon Maker Grid에 이식합니다. 만화 뷰, 편집 drag, Grist/Eagle 저장 계약은 변경하지 않습니다. 완료 조건은 실제 위젯의 카드 렌더와 드롭 후 해당 행 `$E` 및 화면 이미지의 readback입니다.
- Grid View 0.0.38 상태: `GridCards`에 Video Maker형 카드 배치를 이식했고 9 files/92 tests 및 빌드가 통과했습니다. H: 배포본과 소스 `dist/index.html` 해시가 일치하고 위젯 서버가 새 JS·CSS를 제공합니다. 당시 Grist는 HTTP 302 인증 리디렉션이었고 Windows Host 9222 기준이라 카드의 실제 DOM 크기 및 새 Grid 드롭 후 `$E` fresh readback은 미검증입니다. 이후 runtime 검사는 `run-cdp` 공용 프로필(WSL, Grist 로그인 유지)에서 `stagehand-access`로 수행합니다. Table26 `$H=검증`을 유지합니다.
- 현재 추가 작업: Grid 카드의 `$F` 대사를 썸네일 중앙에 텍스트만 표시하고 배경 명암에 따라 흑·백 글자로 보이게 합니다. 두 뷰의 별도 웹 textarea·웹 대사 저장 UI를 제거하되 만화 뷰의 기존 말풍선 대사 렌더는 유지합니다. Grid 드롭·선택·Shot 표시 및 Grist `$E` 저장 경로는 변경하지 않습니다. 완료 조건은 실제 위젯의 Grid 중앙 대사·명암 반응·textarea 부재와 만화 뷰 말풍선의 readback입니다.
- `$F` 표시 0.0.39 상태: Grid 카드 중앙 텍스트에 흰색 `difference` 혼합을 적용하고 App의 웹 textarea 및 `$F` 편집 handler를 제거했습니다. 수정 전 focused 2건 실패, 수정 후 6건 통과, production-boundary 1건, 전체 9 files/93 tests, 타입 검사와 빌드가 통과했습니다. 실제 `$F`는 Text(길이 2)로 확인했으며 원문은 기록하지 않았습니다. 당시 Grist HTTP 302와 Windows Host 9222 리스너 부재로 인증된 위젯의 중앙 좌표·명암·textarea 부재 및 만화 말풍선 DOM readback은 아직 없습니다. 이후 검사는 `run-cdp` 공용 프로필(Grist 로그인 유지)에서 수행합니다. Table26 `$H=검증`을 유지하고 전체 완료로 판단하지 않습니다.
