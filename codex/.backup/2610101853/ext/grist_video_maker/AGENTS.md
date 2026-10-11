# Project Instructions

## CROSS-PROJECT ROLE: GRIST VIDEO MAKER

- Grist Video Maker owns the Grist UI, Grist data reads/writes, outbound request production, and consumption of results published by Chrome Prompt Bridge.
- Project initialization, background generation, and List image generation send external-AI work through Chrome Prompt Bridge. Do not add a second direct AI-site transport, Side Prompt receiver, tab orchestrator, retry queue, or integration ledger here.
- A Side Prompt Gallery result becomes authoritative for Grist only after Chrome Prompt Bridge receives, correlates, and publishes it back to Grist.
- Before adding cross-project integration code, inspect `../chrome_prompt_bridge/contracts/grist-side-prompt-v1.json` and the existing Bridge workflow.

## Version History Note

- Whenever this Grist Custom Widget project's version is increased, update `C:\Users\tende\OneDrive\일기장\작업\종류\프로젝트\영상\콘티 생성기\콘티 생성기.md` in the same task. Use `/mnt/c/Users/tende/OneDrive/일기장/작업/종류/프로젝트/영상/콘티 생성기/콘티 생성기.md` from WSL.
- Under the `## 2026` heading, record exactly one concise summary line per date using the format `MMDD : [<version>] <한 줄 변경 요약>`.
- 날짜별 이력 줄은 `MMDD` 내림차순으로 정렬해 가장 최신 날짜가 `## 2026` 바로 아래 첫 줄에 오게 한다.
- Derive `MMDD` from the version-bump date, use the exact version written to `package.json`, and summarize the user-visible purpose of that version in one line.
- If one or more lines for the same `MMDD` already exist, replace all of them with a single line for the newest version from that date. For example, when `0.1.58`, `0.1.59`, and `0.1.60` share `0822`, keep only the `0822 : [0.1.60] ...` line.
- 기능 완료 시 `npm run version:history -- "<한 줄 변경 요약>"`을 실행해 실제 Vault 문서를 갱신하고, 날짜별 줄 수와 기록 버전이 `package.json`의 해당 날짜 최신 버전과 일치하는지 다시 읽는다.
- Do not add a second line for a version already recorded. If the same task changes its summary before delivery, update that date's existing line instead.
- Preserve the surrounding Markdown structure and existing history entries.

## Verification Artifact Workspace

- 테스트가 실제로 필요로 하는 임시 파일만 이 프로젝트의 `test/tmp/` 아래에서 생성하고, 작업이 끝나면 지운다.
- 위 검증용 임시 파일을 프로젝트 루트나 `/home/tree/extensions` 루트에 생성하지 않는다.
- 제품 런타임이 직접 읽는 설정·manifest·fixture·데이터 JSON/TXT는 이 규칙의 이동 대상이 아니며 기존 소유 경로를 유지한다.

## Initialization Settings Test

- Grist Custom Widget 설정의 기존 Test 버튼 한 번으로 실제 초기화 Markdown 준비 → DOCUMENT_PROMPT 저장/조회 → ChatGPT·Gemini·Claude 수신기·모델·입력·Enter·제출 관측 → Bridge 최종 결과 → Custom Widget 표시까지 전체 경로를 실행한다.
- 빈 프롬프트의 `receiver-connection` 또는 `model-selection` 결과를 사용자용 Test 통과로 취급하지 않는다. 이 모드는 단위 진단에서만 사용한다.
- 한 사이트 실패는 그 사이트의 의존 단계만 멈추고 나머지 사이트는 계속한다. 하나의 request ID로 ChatGPT·Gemini·Claude 결과를 하나의 최종 메시지에 집계한다.

## Initialization Record

- Table7 Raw ID: 1201
- 관리 프로그램: 작업/종류/프로젝트/관리/작업관리기/작업관리기-문서.md
- Obsidian 프로젝트 문서: 작업/종류/프로젝트/코딩/261009.grist_video_maker.md
- Table26 task Raw ID: 4
- Table26 `$D`·`$I`·`$J`에는 지금 진행 중인 최신 항목 한 줄만 작업 행 안의 순번과 함께 쓰고(예: `3. 인증 단계 확립`), 1번부터의 전체 목록과 세부 내용은 같은 순번으로 날짜별 `.ai/changes`에만 기록합니다.
- 테스트에 필요한 임시 파일만 `test/tmp/`에 만들고 작업 후 지웁니다.
