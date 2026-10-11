# Project Instructions

## CROSS-PROJECT ROLE: SIDE PROMPT GALLERY

- Side Prompt Gallery remains independently usable as a gallery and AI-site executor.
- For Bridge-originated initialization, background generation, and List image generation, it receives the validated request from Chrome Prompt Bridge, performs the AI-site interaction, and returns a structured response to Chrome Prompt Bridge.
- It does not publish Bridge-originated responses directly to Grist or own Grist correlation, persistence, tab orchestration, retry queues, or result delivery. Chrome Prompt Bridge hands those responses back to Grist.
- Before adding cross-project integration code, inspect `../chrome_prompt_bridge/contracts/grist-side-prompt-v1.json` and reuse the existing executor boundary.

## Project Boundary
- Treat this folder as an independent project.
- When Codex is running from this folder, modify only files inside this folder.
- Do not modify parent folders, sibling project folders, or files outside this folder unless the user explicitly names an external path and asks for that change.

## Versioning
- When this project gets a functional improvement or a new feature, bump the version in `package.json` by `0.0.1` unless the user explicitly requests a different versioning policy.
- Use a patch bump by default, and keep each version segment within two digits (`00`-`99`) unless the user explicitly requests otherwise.
- Keep `public/manifest.json` in sync with the same version, and ensure the built `dist/manifest.json` is generated with that updated version before delivery.

## Delivery
- After a requested functional improvement, feature addition, or fix is completed, run the project build.
- Copy only the built contents of `dist` to `/mnt/h/code/side_prompt_gallery/` so the delivered files are placed directly under `/mnt/h/code/side_prompt_gallery/`.
- Do not copy or move the entire project folder for delivery unless the user explicitly asks for it.

## Documentation
- When the Markdown structure, heading levels, parser behavior, or prompt composition rules change, update the `# 구조` section in `README.md` in the same task.
- Keep the structure example in `README.md` aligned with the actual parser behavior before building or delivering `dist`.

### Version History Note
- Whenever this project's version is increased, update `C:\Users\tende\OneDrive\일기장\작업\종류\프로젝트\영상\사이드 프롬프트\사이드 프롬프트.md` in the same task. Use `/mnt/c/Users/tende/OneDrive/일기장/작업/종류/프로젝트/영상/사이드 프롬프트/사이드 프롬프트.md` from WSL.
- Under the `## 2026` heading, keep exactly one concise summary line per date using the format `MMDD : [<version>] <한 줄 변경 요약>`.
- 같은 날짜에는 가장 높은 버전 한 줄만 유지하고, 날짜별 이력은 `MMDD` 내림차순으로 정렬해 최신 날짜를 첫 줄에 둔다.
- 기능 완료 시 `npm run version:history -- "<한 줄 변경 요약>"`을 실행하고 실제 Vault 문서의 첫 이력 날짜와 날짜별 줄 수를 다시 읽는다.
- Derive `MMDD` from the version-bump date, use the exact version written to `package.json` and the manifests, and summarize the user-visible purpose of that version in one line.
- Do not add a second line for a version already recorded. If the same task changes its summary before delivery, update that version's existing line instead.
- Preserve the surrounding Markdown structure and existing history entries.

## Response Discipline
- Do not use these Korean acknowledgement phrases in user-facing responses: `맞습니다`, `맞아요`, `인식했습니다`, `알겠습니다`, `그렇습니다`.
- Before every final response, scan the draft for those phrases. If any phrase appears, rewrite the full sentence before sending.
- When the user points out a mistake, do not respond with apology-first acknowledgement. State the concrete corrective action and carry it out.
- This rule exists to persist across future sessions through this project `AGENTS.md`; treat it as mandatory project behavior.

## Prompt Mapping Reference
- Use `/mnt/c/users/tende/desktop/current.png` as the current screenshot reference when mapping prompt input locations from a provided `current.png`.

## INITIALIZATION MODEL SELECTION OWNERSHIP

- Site-specific performance selectors in `content.js` are shared capabilities, not Settings-Test-owned implementations. Initialization uses ChatGPT `매우 높음`, Gemini `3.1 Pro + 확장된 사고 모델 체크`, DeepSeek `깊은 생각 켜짐 + 검색 켜짐`, and Claude `Opus`.
- Every Bridge-originated initialization document delivery calls these shared selectors. When model selection is the current failed Test scope, Settings Test must reuse the same selectors.
- Initialization must record and accept only `advanced-selected` or `advanced-already-selected` (DeepSeek: `instant-selected` or `instant-already-selected` after both toggles are read back) before any prompt insertion or Enter side effect. A selection failure ends that site's initialization delivery before prompt mutation.
- Function names must describe the reusable capability rather than one caller such as `ForTest`. When adding a consumer, search by the required final model label and selection decision as well as by function name.

## Verification Artifact Workspace

- 테스트가 실제로 필요로 하는 임시 파일만 이 프로젝트의 `test/tmp/` 아래에서 생성하고, 작업이 끝나면 지운다.
- 위 검증용 임시 파일을 프로젝트 루트나 `/home/tree/extensions` 루트에 생성하지 않는다.
- 제품 런타임이 직접 읽는 설정·manifest·fixture·데이터 JSON/TXT는 이 규칙의 이동 대상이 아니며 기존 소유 경로를 유지한다.

## Browser Automation (run-cdp shared profile)

- Settings의 사이트별 `Test` 버튼을 통한 고급 모델 선택·프롬프트 전송 검증과 확장 재로드는 에이전트가 `run-cdp` 공용 프로필 브라우저(`/home/tree/.config/codex-run-cdp`, 화면 있는 상태)에 배포 artifact를 로드하고 `stagehand-access`로 직접 수행한다.
- 네 AI 사이트 로그인은 공용 프로필에서 사용자가 한 번만 수행하며, 이후 검사에서 다시 요청하지 않는다. 인증정보·쿠키·프롬프트 원문은 로그 또는 저장소에 기록하지 않는다.
- 허용된 대상 사이트의 Test 동작과 확장 재로드·검증 외의 탭, 다운로드, Bridge, 사용자 데이터에는 자동화를 사용하지 않는다. 검사 뒤 공용 프로필 브라우저는 닫지 않는다.
- 이전의 Playwright 조작과 Windows Chrome 전용 프로필(`H:\code\side_prompt_gallery_playwright_profile`)은 legacy이며 사용하지 않는다.
- 사용자가 여러 사이트 중 문제가 있는 사이트를 특정하면 Agent의 자동 재현 범위만 그 사이트의 기존 Settings `Test` 동작으로 좁힌다. 사용자에게 그 버튼이나 다른 사이트·단계의 버튼을 반복 실행하도록 요구하는 예외가 아니다. 이미 전달 성공을 확인한 사이트, 전체 Grist 초기화, 배경 생성, List 이미지 생성은 같은 수정의 자동 재검증 하드게이트에 포함하지 않는다.
- 확장 재로드를 Agent가 허용된 자동화 범위에서 수행하는 것과 사용자에게 재로드를 요구하는 것은 다르다. 후자는 추가 사용자 실행으로 세며, 기존 한 번의 실행 이후에는 요구하지 않는다. 이전 사용자 실행·요청 이력은 `.ai/changes` 기록과 대화에서 확인하고, 현재 artifact로 이미 요청했다면 다시 요구하지 않고 `runtime-unverified`로 보고한다.
- 2026-09-14 배경 동시 제작 혼합 사고의 5개 항목 실행은 `requestId=7ec7074d-fb7c-4344-93db-271263311300`으로 이미 관측됐고 이후 사용자에게 확장 재로드를 요청한 기록도 있다. 이 사고의 추가 사용자 Test·생성·재로드 요청은 금지한다. 이력 근거는 기존 배경 생성 로그를 사용한다.

## Item Closing Repository Persistence

- Item 마무리 문구의 영구 저장 기준은 프로젝트 Git 폴더의 `item-closings.json`이다. Chrome storage는 실행 캐시로만 사용한다.
- 기존 Chrome 저장값은 미초기화 Git 파일에 최초 한 번 이전하며, 이미 저장된 Git 파일을 오래된 브라우저 값으로 덮어쓰지 않는다.
- Settings 저장은 Git 파일 쓰기·재읽기 성공 후에만 완료로 표시한다. 파일 연결 실패 시 기존 문구를 보존한다.
- `native-host/install-config.json`은 이 컴퓨터의 연결 정보이며 Git 대상에서 제외한다. 마무리 문구는 이 파일에 기록하지 않는다.

## Initialization Settings Test

- Grist Custom Widget 설정의 기존 Test 버튼 한 번이 유일한 사용자 실행 진입점이다. 요청과 결과는 Chrome Prompt Bridge의 정상 `DOCUMENT_PROMPT` 경로를 통과한다.
- Settings Test는 실제 초기화 Markdown으로 ChatGPT·DeepSeek·Gemini·Claude의 수신기 준비, 모델 선택, 입력, Enter, 제출 관측 및 Bridge 최종 결과까지 실행한다.
- 사이트 하나가 실패해도 나머지 독립 사이트를 계속하고, 사이트별 최초 실패 단계를 Grist의 하나의 최종 결과로 반환한다.
- 좌표 실패는 guard 이전의 rect와 탐색 경로·control tag/role·connected/display/visibility를 기록한다. 잘못 잡힌 컨테이너의 텍스트/outerHTML은 로그에 복사하지 않으며 허용 모델 표시 외에는 길이만 기록한다.
- 사용자 오류의 currentModelLabel 빈값·좌표 실패와, 별도 프로필의 Instant 정상 선택은 서로 다른 관측이다. 후자를 원래 오류 해결로 보고하지 않는다.
- 상대 XPath가 반환한 빈 0×0 DIV는 준비된 모델 컨트롤이 아니다. 연결된 양수 크기 후보만 반환하고, 나머지는 다음 후보 및 기존 대기로 이어간다. 보이는 모델 표시의 상위 요소가 0 크기이면 표시 요소 자체를 보존한다.
- 상대 XPath가 양수 크기의 `data-testid="send-button"`을 반환해도 모델 trigger로 사용하지 않는다. 전송 버튼을 제외하고 의미 있는 모델 후보 또는 기존 제한 시간의 준비 대기로 진행하며, 이 충돌 Test는 프롬프트 driver·입력·Enter를 실행하지 않는다.
- GPT 5.6 TerraLight의 `reasoning-slider-range` 실패에서는 실제 `aria-valuemin`, `aria-valuemax`, `aria-valuenow`와 제한된 slider 후보 메타데이터를 Bridge 문서 진단에 전달한다. 실제 값을 확인하기 전에는 지원 범위나 키 입력 횟수를 추측해 바꾸지 않는다.
- 실패 대상이 바뀌어도 사용자용 Test handler를 미세 단계로 재할당하지 않는다. 단계별 고립 검증은 자동 회귀 테스트에서만 수행한다.
