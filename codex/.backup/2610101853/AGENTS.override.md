# A. TASK OUTCOME NOTIFICATION GATE

# A-0. KOREAN RESPONSE REGISTER
- Korean user-facing responses must use polite `-요` speech consistently, including commentary, progress updates, input requests, and final responses.
- Do not imitate a user's profanity, banmal, or hostile register. Preserve the task's meaning while answering in a calm, respectful `-요` register.
- This is a default interaction policy, not a one-turn style inference. A user may explicitly request another register for a specific artifact, quotation, or role-play only.

## Shared runtime skill scope
- A shared runtime skill must not hardcode one product's artifact directories, local URL, external site, start action, or consumer. It must resolve the active project's runtime settings from its nearest `AGENTS.md`.

## Project workflow skill routing
- For a new project, or an existing project missing its Table7 Raw ID or Obsidian initialization path, use the `initial-project` skill (`initial_project` requested alias) before implementation.
- For feature additions, improvements, and bug fixes in an initialized project, use the `work-project` skill (`work_project` requested alias).
- These skills own project task decomposition and tracking; they do not authorize unrelated product workflows or external writes beyond the requested project initialization/work records.
- For `/home/tree/extensions` and its child projects, the management source is the Obsidian Vault-relative path stored in Grist document `au1EbKLtZbrW`, `Table7`, the `관리 프로그램` cell. This scoped rule overrides the generic project-root `Input.md` management source for extensions work; child project `AGENTS.md` files inherit it unless they explicitly define a narrower, user-requested exception.

# G-4. GRIST ACCESS THROUGH THE GRIST-ACCESS SKILL
- 에이전트가 Grist 문서(`au1EbKLtZbrW`의 Table7·Table26·Table27 등)를 읽거나 쓸 때는 반드시 `grist-access` Skill의 `python3 ~/.codex/skills/grist-access/scripts/grist.py`를 사용한다. API key는 이 Skill의 `.env`(Git 제외, 권한 600)에만 두며, 사용자에게 key를 요청하거나, 명령·파일·로그·변경 기록에 key를 쓰거나, curl·fetch·임시 스크립트로 Grist REST API를 직접 호출하지 않는다.
- Grist를 사용하는 Skill과 하위 프로젝트 지침은 무엇을 기록할지만 정하고, 연결 방법은 이 Skill에 위임한다. 이 규칙은 제품 코드(예: Grist Custom Widget, 제품 서버)가 런타임에 자신의 설정으로 Grist에 접근하는 기능에는 적용하지 않는다.
- 스크립트가 `config`·`connection`·`http_*` 경계로 실패하면 우회하지 않고 그 경계와 종료 코드를 보고한다.

# G-5. STAGEHAND THROUGH THE STAGEHAND-ACCESS SKILL
- 에이전트가 Stagehand(브라우저 AI `act`·`observe`·`extract`)를 쓸 때는 반드시 `stagehand-access` Skill의 `node ~/.codex/skills/stagehand-access/scripts/stagehand.mjs` 또는 그 `openStagehand()`를 사용한다. Stagehand 실행본·AI 모델 이름·API key는 이 Skill(`.env`, Git 제외, 권한 600)과 그 실행 폴더가 소유한다.
- 사용자에게 모델 key를 요청하거나, 명령·파일·로그·변경 기록에 key를 쓰거나, 에이전트 검사용으로 제품 프로젝트에 Stagehand를 새로 설치하거나, 별도 브라우저 프로필을 만들지 않는다. 무엇을 검사할지는 `run-cdp` Skill과 프로젝트 `AGENTS.md`가 정한다.
- 제품 코드가 자체 기능으로 Stagehand를 쓰는 경우(예: 프로젝트의 자동화 스크립트)는 이 규칙의 대상이 아니다.

# G-3. DEVELOPMENT DIAGNOSTIC MODE AND BLENDER THREAD SAFETY
- Development diagnostics in extensions with a settings UI must have a persistently stored `개발 모드`/development-mode toggle. For an in-development product, the default may be enabled, but turning it off must disable diagnostic file logging at the logger boundary.
- Diagnostic logs must carry a stable operation ID, owning system, operation, boundary, attempt, result, and thread role while excluding tokens, authorization data, raw private payloads, and full private paths.
- Blender `bpy`/RNA/UI mutation is main-thread-only. Network and worker threads may validate and enqueue commands but must not touch Blender data; a Blender timer or equivalent main-thread callback owns application and camera-keyframe mutation.
- A Blender crash report must preserve the first failing stack boundary and require a regression test for the former unsafe producer path plus a real Blender artifact run and authoritative camera-state readback before the fix is reported as complete.
작업의 결론을 담은 final 응답 전에 항상 적용한다. 결론이 완료, 미완료, 실패, 차단 또는 `runtime-unverified`인지와 무관하게 실행한다.
STEP 1. nf -m "<task-name> <outcome>"
- `<outcome>`에는 `complete`, `incomplete`, `failed`, `blocked`, `runtime-unverified` 중 실제 결론에 해당하는 값을 사용한다.
- Test Manager나 다른 검증 gate가 완료를 승인하지 않아도, 그 결과를 결론으로 보내기 전 `nf` 알림은 생략하지 않는다.
실행 출력: [GATE A-1] exit: <code>
- `nf` 실행 후 STEP 2를 진행한다.

STEP 2. → DENIAL SCAN (B) 실행

# A-2. INPUT NOTIFICATION GATE
사용자 질문, 확인 요청, 선택 요청 또는 `request_user_input` 호출이 필요할 때마다 질문을 보내기 직전에 적용한다.

STEP 1. `nf -m "Codex input required: <입력이 필요한 이유>"`
실행 출력: `[INPUT GATE] exit: <code>`
- `nf` 실행 후 필요한 질문 또는 `request_user_input`을 전송한다.

추가 규칙:
- 질문 묶음마다 새로 실행하고 이전 실행 결과를 재사용하지 않는다.
- 직접 작성한 질문과 `request_user_input`에 모두 적용한다.
- 단순 진행 상황 안내에는 적용하지 않는다.

# A-3. NF USAGE BOUNDARY
구현 시작을 알리는 용도로 `nf`를 실행하지 않는다.
- `nf`는 A-2에 따른 일반 사용자 입력 요청 직전 또는 A에 따른 작업 결론 final 직전에만 실행한다.
- 코드 수정, 파일 생성, 설정 변경, 빌드, 배포 또는 복사를 시작한다는 이유만으로 `nf`를 실행하지 않는다.

# A-6. RUNTIME-UNVERIFIED CONTINUATION
- Windows Host에서 동작하는 제품의 프로세스·포트·명령행 검사는 WSL의 `pgrep`·`/proc` 결과로 대체하지 말고 Host의 PowerShell/CIM/NetTCPConnection 결과를 사용한다.
- `runtime-unverified`는 자동 작업 중단 신호가 아니다. 실제 runtime에 접근할 수 있고 제품 코드·설정·배포 artifact·프로세스 재적재·브라우저 세션·로그 계측 중 Agent가 안전하게 수행할 수 있는 단계가 하나라도 남아 있으면, 해당 상태를 중간 결과로 기록하고 즉시 다음 수정·재실행·검증 단계로 계속한다.
- runtime runner, validator, browser automation, artifact load 또는 authoritative consumer readback이 실패하면 최초 실패 경계를 기록한 뒤 대체 경로를 순서대로 실행한다: 원인 코드/설정 수정 → 산출물 재빌드·배포 → 동일 runtime 재적재 또는 재연결 → 동일 사용자-visible 전체 흐름 재실행 → 실제 consumer readback.
- 위 대체 경로를 모두 실행했거나 상위 규칙이 특정 단계의 실행을 금지해 더 이상 Agent가 수행할 수 없을 때만 최종 결론에 `runtime-unverified`를 사용한다. 단순한 재로드 필요, stale process, stale browser artifact, runner retry, 관찰 누락 또는 첫 runtime 실패만으로 final을 보내지 않는다.
- 사용자가 “완료될 때까지”, “될 때까지”, “계속”을 지시한 작업에서는 runtime 실패를 결론으로 보고하지 말고, 안전한 자동 작업이 소진될 때까지 사용자 입력을 기다리지 않는다. 사용자 전용 동작만 남았을 때에만 input gate를 거쳐 한 번의 전체 흐름 실행을 요청한다.

# A-7. ACTIVE WORK CARRY-OVER
- 활성 작업은 가장 가까운 프로젝트 `AGENTS.md`의 현재 작업 상태와 Table26 `$H`로 판단한다(X-11).
- 활성 작업이 있는 동안 새 사용자 요청을 `대체`, `추가`, `독립`으로 먼저 분류한다. 사용자가 기존 작업의 취소·대체를 명시하지 않은 한, 런타임 준비·스킬·도구·설정 변경 요청은 기존 작업에 대한 `추가` 작업으로 처리한다.
- `추가` 작업을 끝낸 뒤에는 프로젝트 `AGENTS.md`와 Table26에서 첫 미완료 agent-executable 단계(실패 경계 조사 → 소유 코드 수정 → 테스트 → 산출물 갱신 → 재연결 또는 재실행 → authoritative consumer readback)를 찾아 재개한다.
- terminal final 직전에는 프로젝트 `AGENTS.md`의 완료 조건, 마지막 authoritative consumer readback, 남은 agent-executable 단계, 실행 중이거나 읽지 않은 Agent 결과를 대조한다. 하나라도 남으면 final 대신 해당 단계를 계속 실행한다.
- 사용자가 작업 중단 또는 조기 종료를 지적하면 프로젝트 `AGENTS.md`의 현재 작업 상태로 돌아가 사용자 입력을 기다리지 않고 재개한다.
- 위임 Agent의 완료 알림은 사용자 질문 대기 신호가 아니다. 메인 세션은 도착 즉시 Agent가 일반 텍스트로 반환한 결과, diff, 테스트 출력을 읽고 실패·누락을 다음 수정 작업으로 전환한다.

# A-8. CDP WHOLE-FLOW EXECUTION HARD GATE
- 브라우저 검사는 `run-cdp` Skill로 화면이 보이는(`headless: false`) 전용 프로필 Chromium(`/home/tree/.config/codex-run-cdp`, endpoint `127.0.0.1:9222`)에서 Stagehand로 수행한다. headless는 프로젝트 `AGENTS.md`가 headless 전용 runtime을 명시한 경우에만 쓴다. 전용 프로필의 로그인(예: Grist)은 유지되며 다시 요청하지 않는다.
- 사용자가 CDP, browser automation, 실제 사이트 검증 또는 전체 흐름 검사를 지시했거나 제품 경로가 브라우저 확장·외부 사이트 consumer를 포함하면, 브라우저를 띄우거나 연결한 것만으로 해당 요구를 이행한 것으로 처리하지 않는다. 메인 세션은 같은 활성 작업 안에서 `stagehand-access`의 `openStagehand()`(정상 9222에 붙거나 공용 프로필로 launch) → 의도한 페이지·실제 배포 artifact·extension 버전 확인 → consumer baseline → 실제 생산 시작 동작(Stagehand) → producer·transport·handler·외부 site·authoritative consumer readback 순서를 실행한다.
- 빈 `about:blank`, endpoint health, target discovery, build, 로그 존재 또는 CDP 연결 성공은 전체 흐름 증거가 아니다. CDP 세션이 배포 artifact·필요 extension·목적 페이지를 아직 로드하지 않았으면 그 상태를 다음 agent-executable setup work item으로 기록하고, 사용자 입력·terminal 응답으로 전환하지 않는다.
- 사용자가 CDP 전체 흐름 검사와 실제 Test·생산 시작 동작을 명시한 경우, 로그인·결제·외부 전송·비가역 또는 비용 발생 효과를 제외한 선언된 production UI start action은 Agent가 같은 CDP 세션에서 직접 실행한다. 이 action을 사용자에게 넘기거나 input gate로 바꾸기 전에는, 해당 예외의 실제 경계를 target state와 action effect로 기록해야 한다.
- CDP 전체 흐름이 사용자 profile 인증이나 비용·비가역 외부 효과 때문에 사용자 동작을 필요로 할 때에도, 그 직전까지의 artifact load, extension lifecycle, target selection, producer/transport diagnostics, consumer baseline과 CDP readback을 먼저 끝낸다. 사용자에게 요청하는 경우는 정확히 하나의 남은 production action이어야 하며, 실행 뒤 메인 세션은 같은 CDP 세션에서 logs와 authoritative consumer를 직접 재확인한다.
- 이 순서는 활성 작업의 필수 실행 gate다. Agent 결과, 스킬 수정, 서버 기동 여부, CDP 창 표시 여부를 중간 보고로 소비하지 말고, 같은 세션에서 전체 흐름의 다음 미실행 경계를 계속 수행한다.
- extension 경로는 browser 시작 전에 배포 artifact 디렉터리로 명시하고, loaded extension ID·manifest version·bundle SHA-256·service worker 또는 production target을 같은 CDP 세션에서 읽어 source/deployment과 대조한다. extension이 없는 기본 `run_cdp` 세션을 extension 전체 흐름의 runtime으로 오인하지 않는다.
- shared 9222가 정상이면 붙기만 한다. 새 extension 빌드를 로드해야 할 때만 `run_cdp --replace -e <extension 디렉터리>`로 전용 프로필 브라우저를 재시작한다.
- CDP 전체 흐름 결과는 브라우저 버전·headless 여부, 대상 URL, extension ID/version/digest, consumer baseline, 실제 start action, producer/transport/handler/site 단계 결과와 final consumer readback을 명령 출력과 `.ai/changes` Markdown 기록에 남긴다.
- 브라우저는 기본적으로 화면이 보이는 상태다. 사용자가 headless를 요청하지 않았는데 `HeadlessChrome` 또는 headless endpoint가 발견되면 runtime 결과로 쓰지 않고, 전용 프로필 브라우저를 `run_cdp --replace`로 화면 있는 상태로 다시 띄운 뒤 재연결한다.
- CDP runtime identity는 선언값이 아니라 같은 operation의 실제 증거로 검증한다. `Browser.getVersion`, User-Agent, 실제 process argv, 선택된 target ID/origin/title, embedded widget iframe origin/title, 로드된 artifact digest, declared start action, authoritative consumer readback을 같은 operation의 출력으로 확인한다. sibling product/page/widget, stale artifact, local widget-only target, intermediate ACK는 다른 product의 완료 조건을 만족시킬 수 없다.
- browser visibility, page/widget identity, artifact identity, final consumer readback 중 하나라도 없거나 서로 다른 operation에서 나왔으면 runtime 통과로 판정하지 않는다. `headless: false` 같은 직접 적은 metadata는 Browser/process/page 관측을 대신하지 않는다.
- 한 검사는 새 launch 1회, 새 증거가 있는 실패 지점의 자동 retry 1회, browser work 총 5분만 허용한다. 정상 9222 endpoint는 붙기만 하고, 새 extension 빌드를 로드할 때만 `run_cdp --replace`로 재시작한다. 반복 launcher 호출·popup/target 생성·site 재방문은 새 증거 없이는 하지 않는다.

# B. DENIAL SCAN
모든 final 응답 전에 적용.
draft에서 아래 표현을 내부적으로 검사한다:
맞습니다
맞아요
인식했습니다
알겠습니다
그렇습니다

"있음" 항목 존재 → 해당 문장 전체 재작성 후 B 재실행
모두 "없음" → 출력 없이 final 응답 전송

# B-2. ACKNOWLEDGEMENT RESPONSE DISCIPLINE
모든 사용자 대면 응답에 적용한다.
- 위 B 목록의 금지 표현은 동의, 수긍, 확인, 사과, 요약 문맥에서도 사용 금지.
- 사용자가 실수, 위반, 누락을 지적하면 사과문이나 수긍문으로 시작하지 말고 바로 구체적 조치와 실행 결과를 말한다.
- "앞으로"라는 말로 미루지 말고, 다음 세션에도 적용되어야 하는 규칙은 전역 파일(`/home/tree/.codex/AGENTS.md`)에 즉시 반영한다.
- final 작성 전 B와 B-2를 함께 확인한다.

# B-3. LOCAL AGENTS PRECHECK AND COMPLETION RULE SCAN
코드 수정, 파일 생성, 빌드, 배포, 복사, 버전 변경, 테스트 실행이 포함된 작업에 적용한다.
- 작업 시작 전에 현재 작업 디렉터리부터 상위 경로까지 적용 가능한 `AGENTS.md`, `AGENTS.override.md`를 먼저 읽고, 작업별 체크리스트에 반영한다.
- 사용자가 메시지에 붙여준 `<INSTRUCTIONS>`만으로 충분하다고 판단하지 않는다. 실제 파일에 있는 로컬 규칙을 별도로 확인한다.
- 작업 완료 전 final 응답 전에 적용 규칙에서 아래 키워드를 재검색한다: `복사`, `copy`, `배치`, `deploy`, `D:`, `/mnt/d`, `Desktop`, `version`, `dev server`, `nf`.
- 재검색 결과에 작업 완료 조건이 있으면 테스트 통과만으로 완료 처리하지 말고 해당 조건을 실행 및 검증한다.
- 로컬 규칙과 전역 규칙이 모두 있으면 더 구체적인 현재 프로젝트 규칙을 우선 적용한다.

# C. JJ GLOBAL CONFIG SAFETY
사용자가 `jj`, `jujutsu`, `전역설정`, `global config` 관련 작업을 지시하면 실제 사용자 전역설정 병합 전까지를 작업 범위로 본다.
- 작업자는 `jj workspace` 생성, 설정 초안 작성, 테스트 실행, 검증 결과 보고까지만 수행한다.
- 실제 사용자 전역설정 파일 병합, 기존 전역설정과의 최종 충돌 해결, 최종 반영 책임은 사용자에게 둔다.
- 전역설정 검증은 필요 시 `JJ_CONFIG` 또는 동등한 격리 경로를 사용해 사용자 기본 전역설정을 오염시키지 않는 방식으로 수행한다.
- 사용자가 명시적으로 병합까지 위임하지 않은 이상 자동 병합, 직접 덮어쓰기, 기존 사용자 전역설정 수정은 금지한다.

# C-2. TASK-SCOPED ARCHIVE PACKAGING
- ZIP·bundle·archive 생성은 해당 사용자 요청 또는 프로젝트별 명시 규칙에 한정한다. 공용 복사·배포 규칙만으로 다른 프로젝트의 ZIP 생성을 추론하지 않는다.
- 프로젝트별 ZIP 규칙은 대상 프로젝트, 정확한 배포 경로, 파일명과 ZIP 내부 최상위 디렉터리를 명시해야 한다. 이 조건이 없는 프로젝트에는 복사본만 만들고 ZIP은 만들지 않는다.

# D. VERSION BUMP DEFAULT
기능 추가 또는 기능 개선판이 실제 제품 동작을 변경하면 완료 전·후와 무관하게 기본적으로 버전을 `0.0.1`씩 올린다.
- 문제 재현, 원인 조사, 테스트 실패 자체, 제품 동작을 바꾸지 않는 runtime 재시도, 동일 산출물 재빌드·재배포 및 사용자 실행 대기만 수행한 경우에는 버전을 올리지 않는다.
- 사용자 요구조건을 충족하기 전에 오류를 발견해 원인 소유 코드·설정·산출물을 수정한 개선판을 만들면 해당 개선판에 대해 `0.0.1`을 올린다. 이 patch 증가는 작업 완료를 의미하지 않으며, 같은 활성 작업 안에서 요구조건 충족까지 계속한다.
- 사용자 요구조건이 실제 authoritative consumer에서 모두 충족된 최종 시점에도 기능 추가·개선판에 해당하는 patch 증가를 누락하지 않는다. 동일한 수정판에 대해 중복으로 증가하지 않도록 각 실제 제품 개선판마다 한 번만 올린다.
- 별도 지시가 없으면 patch 단위 증가를 사용한다.
- 각 버전 세그먼트는 기본적으로 최대 두 자리(`00`~`99`)를 넘기지 않도록 관리한다.

# E. DEV SERVER EXECUTION
작업자는 사용자의 명시적 요청 없이 dev 서버를 실행하지 않는다.
- 구현 또는 검증 후에도 `npm run dev`, `bun run dev`, `next dev` 등 장기 실행 dev 서버 명령은 자동 실행하지 않는다.
- 사용자가 명시적으로 dev 서버 실행을 요청한 경우에만 실행하고, 작업 완료 또는 사용자 요청 시 종료 상태를 확인한다.

# F. EAGLE DUPLICATE HANDLER COPY DEFAULT
`/home/tree/extensions/eagle_duplicate_handler` 작업에서 생성 또는 수정이 끝나면 결과물을 Windows 경로 `D:\code\eagle_duplicate_handler`에 복사한다.
- WSL 경로는 `/mnt/d/code/eagle_duplicate_handler`를 사용한다.
- 최소 복사 대상은 `manifest.json`, `logo.png`, `index.html`, `js/plugin.js`이다.
- 복사 후 대상 폴더의 파일 목록 또는 체크섬으로 복사 결과를 검증한다.

# G. DEBUG VALUE AND PATH BOUNDARIES
타입, 템플릿 문자열, URL 인코딩, Unicode 또는 Linux·Windows·WSL 경로 변환을 포함한 디버그에 적용한다. 목록에는 값이나 파일이 있지만 열기, 표시 또는 읽기에 실패하는 경우도 포함한다.
- `/home/tree/ai/skills/test_manager/SKILL.md`를 읽고 `$test-manager`의 값·템플릿·경로 경계 절차를 사용한다.
- 코드 수정 전에 `python3 /home/tree/ai/skills/test_manager/scripts/check_correct_path.py self-test`를 실행한다.
- 생산자의 실제 원시 값과 타입, 소비자의 기대 계약을 별도로 확인하고 토큰과 개인 경로는 기록에서 마스킹한다.
- 프로젝트의 실제 템플릿 렌더러가 만든 결과를 검사한다. 별도 렌더러를 만들어 예상 결과만 검사하지 않는다.
- 명시적인 계약 없이 `String(value)`, 숫자 문자열 강제 변환, 튜플의 마지막 요소를 ID로 사용하는 처리를 허용하지 않는다.
- percent encoding은 적용 여부를 먼저 확인하고 최대 한 번만 디코딩한다. 불투명 문자열은 계약이 없으면 원문 그대로 보존한다.
- 같은 파서, 강제 변환, 템플릿 렌더러, 디코더 또는 경로 매퍼를 사용하는 인접 필드도 검색해 회귀 검사를 추가한다.
- 단위 검사와 경계 통합 검사를 모두 실행한다. 파일 경로라면 변환된 실제 파일의 존재, 읽기 권한과 바이트 읽기를 확인해야 완료로 처리한다.

# G-2. EXTERNAL API DIAGNOSTIC DESIGN
외부 API에 접속하는 기능을 설계·구현·수정할 때 적용한다.
- 각 외부 요청 로그에는 안정적인 작업 ID와 함께 접속 대상 시스템의 명시적 이름, 수행 작업, 요청 단계, 시도 번호, 결과를 기록해 같은 오류 문구가 여러 경계에서 발생해도 최초 실패 주체를 구분할 수 있게 한다.
- 실패 로그에는 최소한 `어디에서`에 해당하는 소유 시스템·경계와 `어떤 작업 도중`에 해당하는 작업명을 포함한다. 전송 전 실패, 전송 결과 불명, HTTP 실패, 응답 판독 실패, 최종 consumer 검증 실패를 서로 다른 결과로 기록한다.
- 진단 로그 기능이 사용자 설정 UI를 가진 제품에 포함되면 설정 메뉴에 영구 저장되는 `개발 모드` 토글을 제공하고, 비활성 상태에서는 진단 로그를 기록하지 않는다.
- 토큰, API 키, Authorization 헤더, 원문 payload, 개인 파일 내용과 전체 개인 경로는 로그에 기록하지 않는다. 필요한 경우 길이·개수·상태 코드·식별자·digest·마스킹된 metadata만 사용한다.
- 개발 모드의 기본값, 저장·재로드 동작, 로그 활성·비활성 경계, 동일 메시지의 서로 다른 외부 API 실패를 회귀 테스트로 검증한다.

# H. CURRENT.PNG REFERENCE IMAGE
사용자가 별도 경로 없이 `current.png`를 언급하면 Windows 바탕화면의 참조 이미지로 해석한다.
- Windows 경로는 `C:\Users\tende\Desktop\current.png`이다.
- WSL에서 사용하는 경로는 `/mnt/c/users/tende/Desktop/current.png`이다.
- UI 또는 디자인 작업에서는 이 이미지를 먼저 열어 시각적 기준으로 사용한다.

# I. USER REQUIREMENT LOG
프로젝트 작업에 관한 사용자 요구를 다음 세션에서도 추적할 수 있도록 프로젝트 루트의 `Input.md`에 기록한다.
- 구현, 수정, 파일 생성, 빌드, 배포 또는 설정 변경을 시작하기 전에 `Input.md`를 읽고, 파일이 없으면 새로 만든다.
- 프로젝트 루트는 현재 작업에 적용되는 가장 가까운 프로젝트별 `AGENTS.md` 또는 `AGENTS.override.md`가 있는 디렉터리로 정한다. 해당 파일이 없으면 현재 작업 디렉터리를 사용한다.
- 날짜별 `## YYYY-MM-DD` 제목 아래에 그날 받은 요구를 시간순으로 추가한다.
- 각 항목에는 사용자의 요구, 명시된 범위와 완료 조건을 의미가 바뀌지 않게 간결히 정리한다. 사용자가 말하지 않은 의도나 제한을 추가하지 않는다.
- 후속 요청이 기존 요구를 변경하거나 취소하면 이전 기록을 덮어쓰지 않고 같은 날짜 아래에 변경 내용을 새 항목으로 추가한다.
- 토큰, 비밀번호, 개인 식별 정보와 같은 민감한 값은 기록하지 않고 마스킹한다.
- 단순 질문이나 읽기 전용 확인처럼 프로젝트 변경을 요구하지 않는 메시지는 기록 대상에서 제외한다.

# J. EAGLE PLUGIN ICON STYLE
Eagle 관련 플러그인의 아이콘은 흰색 라인아트 스타일로 만든다.
- 작은 크기에서도 식별되는 단순한 외곽선과 충분한 선 굵기를 사용한다.
- 색상 중심의 일러스트나 입체 렌더 대신 흰색 선을 주 시각 요소로 사용한다.

# K. TEST MANAGER
테스트 작성, 수정, 실행, 검토 또는 테스트 결과를 근거로 완료를 주장하는 모든 작업에 적용한다.
- 작업 전에 `/home/tree/ai/skills/test_manager/SKILL.md`를 끝까지 읽고 `$test-manager` 절차를 사용한다.
- 실제 runtime 단계의 브라우저 검사는 `run-cdp` Skill(공용 프로필, `headless: false`), Stagehand 동작은 `stagehand-access` Skill, Table26·Grist 읽기·쓰기는 `grist-access` Skill로 수행한다.
- 프로젝트별 테스트 규칙 및 다른 적용 Skill과 함께 사용하며, 더 구체적인 프로젝트 규칙을 우선한다.
- 사용자-visible 시나리오는 같은 scenario ID와 입력으로 `unit test → production boundary integration → 빌드 artifact → 실제 runtime consumer` 순서로 검증한다. unit test가 통과해도 실제 runtime에서 동일 결과를 직접 읽지 못하면 완료 처리하지 않는다.
- 재시도 흐름은 부작용 발생 전과 후를 구분하고, 붙여넣기·Enter·탭 생성·파일 쓰기·API 변경 같은 부작용의 호출 횟수와 순서를 검사한다.
- 부작용 뒤 응답 유실, 부분 성공, timeout, 중복 request ID를 포함한 적용 가능한 경계를 검증하지 않으면 테스트 완료로 처리하지 않는다.
- 중복되거나 새로고침 뒤 남는 UI는 저장 데이터 수, 단일 컴포넌트 내부 노드 수, 문서 루트 수, listener·worker·process 인스턴스 수, 실제 화면 결과를 각각 측정한다. 스크린샷만으로 데이터 중복을 단정하지 않고, 이전 인스턴스의 stale DOM을 남긴 재주입·reload·update·SPA 이동·cleanup 테스트에서 최종 owner와 루트가 하나인지 검증한다.
- 서로 다른 도메인 Item이 같은 표시 이름을 가지는 문제는 중복 실행이나 UI 중복과 분리한다. mutation 전에 stable ID·원문 이름·현재 scope·삭제 상태·역할을 전부 목록화하고 identity key와 uniqueness key를 따로 선언하며, query 순서나 첫 일치 항목으로 승자를 정하지 않는다. 계약에 따른 승자·패자 우선순위, null 현재 Item, 복수 collision, scope 밖 동명 Item, rollback을 테스트하고 실제 runtime consumer에서 모든 후보의 최종 ID·scope와 미변경 대상을 다시 읽기 전에는 완료 처리하지 않는다.
- 사용자 관점에서 한 번인 붙여넣기·쓰기·업로드·전송을 여러 consumer 호출이나 chunk로 나누면 각 호출을 별도 부작용으로 계산한다. 명시적인 streaming 계약이 없으면 전체 payload를 담은 호출 정확히 한 번을 검사한다.
- 같은 사용자-visible 실패가 두 번째 보고되면 missed regression으로 처리하고, 기존 테스트가 통과한 이유를 분석한 뒤 수정 전 baseline 또는 격리된 동일 mutant에서 실패하는 회귀 테스트를 증명해야 완료할 수 있다.
- 실패가 둘 이상의 상태가 동시에 참일 때만 발생하면 각 조건을 따로 검사한 테스트를 회귀 근거로 인정하지 않는다. 실제 실패 당시의 결합 상태를 테스트 계약에 기록하고, 정확한 결합 1건과 한 축씩만 바꾼 인접 상태를 생산 predicate·deduplication·fallback·consumer 경계에서 검사한다.
- 같은 사용자 요구 또는 같은 `(시작 동작, visible 실패, authoritative consumer)`가 두 번째 보고되면, 이전 수정이 멈춘 경계와 기존 테스트가 놓친 결합 상태를 기록한다. 그 뒤에는 동일한 실제 runtime 시작 동작과 동일한 결합 pre-state로 배포 artifact를 실행해 모든 이전 미관찰 경계를 통과하고 authoritative consumer의 persisted 결과를 직접 읽기 전까지 완료로 판단하지 않는다. 다른 fixture, probe, no-collision 상태, 중간 응답 또는 사용자 화면만으로 대체하지 않는다.
- 테스트의 관찰 단위는 사용자 동작에서 실제 consumer 상태까지 생산자·handler·전파·consumer·결과 경계를 추적해 정한다. 함수 호출 횟수를 실제 효과 횟수로 대신하지 말고 bubbling, 다중 listener, fan-out·fan-in, fallback과 deduplication 뒤의 권위 있는 consumer 결과를 별도로 검사한다.
- 사용자-visible 기능은 unit test, mock, jsdom, fake Chrome API, 소스 문자열 검사, 빌드 또는 체크섬만으로 완료 처리하지 않는다. 배포 artifact를 실제 브라우저·프로세스·CLI 등 해당 런타임에서 실행하고 권위 있는 consumer 결과를 직접 읽어야 한다.
- 브라우저 확장 작업은 실제 브라우저에 배포본을 로드하고 사용자 시작 동작, content script 또는 service worker 전달, 실제 DOM·editor·download·navigation 결과를 검증한다. reload·재주입·restart가 관련되면 같은 실제 브라우저 세션에서 해당 전환도 실행한다.
- cross-file 전역 API, namespace export, injected script API 또는 plugin registry member를 추가·변경·소비하면 producer 등록명, production load order, consumer 호출명, 실제 사용자 handler와 authoritative consumer를 하나의 boundary 시나리오에서 실행한다. helper 직접 호출이나 소스 문자열 검사만으로 통과시키지 않고 owner/member 오탈자 mutant를 거부해야 한다.

# L. STRICT REQUEST SCOPE AND ADJACENT WORKFLOW ISOLATION
기능 추가, 수정, 버그 해결에서 사용자가 명시한 동작 경로만 작업 범위로 본다.
- `여기`, `현재 사이트`, `이 버튼`, `Item 클릭`처럼 현재 UI 동작을 가리키는 요청은 해당 UI 이벤트에서 직접 도달하는 생산자·handler·consumer 경로로 한정한다.
- 사용자가 이름을 말하지 않은 Bridge, background 작업, 외부 자동화, batch, queue, 다른 입력 경로, 공유 integration은 기존 기능이 비슷하거나 helper를 재사용할 수 있다는 이유만으로 수정하지 않는다.
- 구현 전에 내부 체크리스트에 `허용된 시작 동작`, `허용된 최종 효과`, `수정 금지 인접 경로`를 각각 기록한다.
- 요청을 가장 좁은 호출 지점에서 구현할 수 있으면 그 호출 지점에서만 처리한다. 공유 helper 변경으로 둘 이상의 workflow 동작이 달라지게 만들지 않는다.
- 합리적인 해석이 둘 이상이고 선택에 따라 서로 다른 workflow가 변경될 수 있으면 파일 수정 전에 사용자에게 범위를 질문한다.
- 테스트에는 요청 경로의 기대 결과뿐 아니라 이름이 언급되지 않은 인접 workflow의 payload, 저장 설정, 부작용이 변경되지 않았다는 negative-scope 회귀 검사를 포함한다.
- 작업 중 명시되지 않은 경로가 변경된 사실을 발견하면 완료 처리하지 말고 같은 작업에서 해당 변경을 되돌리고 경계 검사를 추가한다.

# M. DEV BRANCH SKILL
사용자가 `dev branch로 이전`, `개발 브랜치로 전환`, dev 브랜치에서 계속 작업, 또는 `$dev-branch`를 명시한 경우에만 `/home/tree/ai/skills/dev-branch/SKILL.md`를 끝까지 읽고 적용한다.
- 단순 개발 작업, 기존 `dev` 브랜치 존재, `_dev`가 포함된 경로만으로는 이 Skill을 적용하지 않는다.
- dev 브랜치 이전은 기본적으로 사용자가 지정한 저장소의 Git 브랜치 작업만 의미한다.
- `_dev` 배포 폴더 생성·복사·삭제, Chrome 확장 등록, 브라우저 자동화, Bridge ID·소비자 경로 변경, 설정·이미지·스토리지 이전, 프로세스 종료·재시작은 사용자가 해당 동작을 별도로 명시하지 않으면 수행하지 않는다.
- Skill 적용으로 범위를 넓히지 말고 사용자가 지정한 브랜치 작업과 후속 구현만 수행한다.

# N. USER-EXECUTED AUTOMATION GATE
브라우저·데스크톱 UI 조작, 확장 프로그램 재로드, 실제 사용자 동작 재현처럼 automation이 필요한 모든 작업에 적용한다.
- 브라우저 안의 동작(페이지·Grist 위젯·확장 UI 클릭과 입력, 확장 재로드, 기존 Settings `Test` 버튼 실행)은 에이전트가 `run-cdp` Skill과 `stagehand-access` Skill로 공용 프로필 브라우저(`/home/tree/.config/codex-run-cdp`, 화면 있는 상태)에서 직접 수행한다. 사용자에게 이 동작을 넘기지 않는다.
- 사용자에게 넘기는 동작은 로그인·인증, 결제·비용 발생, 되돌릴 수 없는 외부 전송, 브라우저 밖의 데스크톱(OS) 앱 조작으로 한정한다. 공용 프로필에서 한 번 로그인한 사이트는 다시 요청하지 않는다.
- Playwright 직접 실행, 프로젝트별 별도 브라우저 프로필, Windows Chrome 원격 디버깅 프로필은 legacy이며 새로 쓰지 않는다. CDP 원시 호출(`/json/version`, `/json/list`)은 endpoint·target 확인 진단에만 쓴다.
- 사용자 실행을 요청하기 전에 생산자 입력, 전달 단계, 최종 consumer 결과와 오류를 확인할 수 있는 로그를 먼저 구현·활성화하고, 로그 위치·식별자·민감정보 마스킹 여부와 실행 전 기준 상태를 검증한다.
- 로깅 환경이 실제 실행 결과를 구분할 수 없는 상태에서는 사용자에게 실행을 요청하지 않고 먼저 관찰 가능성을 보완한다.
- 사용자 동작이 필요하면 A-2 INPUT NOTIFICATION GATE에 따라 `nf -m "Codex input required: <실행이 필요한 이유와 사용자가 실행할 정확한 동작>"`를 시도한 뒤, 실행 결과와 관계없이 사용자에게 실행할 동작, 예상 시점과 실행 후 알려줄 내용을 구체적으로 요청한다.
- 사용자가 실행 완료를 알리면 준비한 로그와 실제 consumer 상태를 읽어 원인을 분석하고 수정한다. 사용자 기억이나 스크린샷만으로 성공을 단정하지 않는다.
- 한 번의 사용자 실행으로 필요한 증거를 최대한 수집하도록 요청을 묶되, 서로 다른 부작용을 임의로 합치지 않는다.
- 동일 활성 작업의 사용자 실행은 누적되는 단일 관찰 예산이다. 이미 얻은 Host 로그·consumer readback을 측정값 부족이나 중간 Agent 결과만으로 무효화하거나 같은 동작을 다시 요청하지 않는다. 새 배포 artifact가 이전 사용자 실행 뒤 실제로 로드되어 기존 관찰을 대체할 때에만, 새 artifact digest와 정확히 하나의 묶인 전체 흐름을 기록한 뒤 한 번의 replay를 요청할 수 있다.
- 사용자가 동일 artifact generation에서 사용자 실행 요청을 거절·취소·반복 요청으로 지적하면, 해당 요청은 소진·금지 상태로 기록한다. 같은 동작을 다시 요청하거나 표현만 바꿔 재요청하지 말고, 기존 Host 로그·consumer readback과 agent-executable 단계로 계속한다. 새 artifact가 실제로 배포·로드되어 prior generation을 대체한 경우에만 새 digest와 남은 전체 흐름 1건을 기록해 replay 여부를 판단한다.

# O. VISUAL SIZE REQUIREMENT DISCIPLINE
UI의 폭·높이·간격·비율을 줄이거나 늘리라는 요구에 적용한다.
- `더 줄여`, `한 칸 크기`, `일반 text 높이`, `1:1`, 참조 이미지와 같은 상대적 표현을 각각 독립된 장식 요청으로 분리하지 말고, 주변 요소와의 관계를 포함한 하나의 layout 계약으로 해석한다.
- 사용자가 숫자를 지정하지 않았다는 이유로 임의의 중간 크기를 여러 차례 제안하거나 같은 크기 의도를 반복 질문하지 않는다. 현재 DOM·CSS·참조 이미지에서 기존 computed size와 제약을 읽고 요청 방향을 충족하는 명확한 목표값을 정한다.
- table·grid·flex의 자동 배분으로 선언 폭이 실제 화면에서 달라질 수 있으면 class 선언만으로 완료 처리하지 않는다. `colgroup`, grid track, flex basis 등 권위 있는 layout 제약을 사용하고 실제 runtime의 computed width·height를 기록해 확인한다.
- 크기 변경 요청에는 관련 요소별 `변경 전 실제 크기 → 목표 크기 → runtime 측정값`을 완료 근거로 남긴다.
- 사용자가 같은 크기 문제를 두 번째 지적하면 missed regression으로 처리하고, 이전 수정이 실제 화면에서 요구를 충족하지 못한 이유를 같은 작업에서 분석·기록한다.

# O-2. UI SKILL MANDATORY GATE
UI/UX 또는 frontend 표현을 설계·구현·수정·디버그·검증하는 모든 작업에 적용한다. `width`, `height`, column, cell, table, grid, flex, spacing, responsive, typography, color, hover, preview, density, alignment, screenshot, `current.png` 또는 반복된 시각 불만이 포함된 요청도 모두 포함한다.
- 첫 mutation 전에 `/home/tree/.codex/skills/ui-design-implementation/SKILL.md`를 끝까지 읽고 `$ui-design-implementation` 절차를 사용한다. 파일을 읽지 못하면 UI mutation을 수행하지 않는다.
- Skill 사용 사실과 이로 인해 수행하는 검사 또는 중단 조건을 첫 commentary update에서 사용자에게 알린다.
- 첫 mutation 전에 사용자의 정확한 크기 대상 명사를 `column / cell / content / thumbnail / row / container / gap` 중 해당 의미로 고정하고 selector, axis, 권위 있는 layout owner, 변경 전 runtime 값, 목표값, 허용 오차와 충돌 제약을 기록한다.
- 사용자 요구가 column이면 thumbnail, cell content 또는 내부 이미지의 크기를 변경하거나 측정한 결과로 대체하지 않는다. 요청 대상과 측정 대상의 semantic kind가 다르면 검증 실패로 처리한다.
- `width: 100%`, 자동 track, percentage width, intrinsic minimum, padding, border, min/max width처럼 목표 크기를 재분배하는 모든 제약을 확인하고 권위 있는 layout owner에서 해소한다. 새 width 선언을 추가한 사실만으로 구현 완료 처리하지 않는다.
- 실제 runtime의 요청 대상에 대해 `getBoundingClientRect()` 또는 computed style을 읽고 `요청 대상 → layout owner → 충돌 제약 → 변경 전 → 목표 → 실측 → 오차`를 기록한다. 기본 허용 오차는 `±1px`이다.
- `/home/tree/.codex/skills/ui-design-implementation/scripts/validate_layout_contract.py`의 검증 결과가 code 0이 아니면 완료 보고, 사용자 새로고침 요청 또는 같은 요구의 재확인 요청을 보내지 않고 원인 분석과 수정을 계속한다.
- 같은 시각 문제가 두 번째 보고되면 추가 style tweak를 동결하고 missed regression으로 기록한다. 이전 테스트가 선언값·build·screenshot·하위 요소 측정만 확인했는지 밝히고, 이전 runtime 실측값을 거부하는 회귀 검사를 먼저 증명한다.

# P. REQUIREMENT SEMANTIC BINDING GATE
기능 추가, 수정, 버그 해결 또는 설정 변경에서 사용자의 용어와 구조 경계를 구현 개념에 고정한 뒤 작업한다.
- 첫 mutation 전에 현재 요청을 `사용자가 명시한 구조 경계`, `현재 선택 상태`, `재현용 예시`, `요구된 최종 효과`, `변경 금지 범위`로 분리해 내부 체크리스트에 기록한다.
- 사용자가 헤더 단계, selector, 필드, 타입, 경로 또는 UI 계층을 명시하면 실제 parser·schema·DOM·코드 모델을 읽어 그 용어가 대응하는 생산자와 consumer를 확인한다. 비슷한 내부 용어나 상하위 계층으로 치환하지 않는다.
- 특정 키, 이름, 값 또는 한 번의 실패 사례는 사용자가 분류 기준이라고 명시한 경우에만 구현 분기나 데이터 구조의 축으로 사용한다. 그 외에는 재현 입력과 회귀 테스트 벡터로만 취급한다.
- 사용자의 후속 정정이 기존 해석, 요구 기록 또는 테스트 계약의 구조 경계를 바꾸면 이전 계약을 즉시 폐기하고 정정된 경계로 다시 작성한다. 계약을 갱신하기 전에는 추가 제품 수정을 진행하지 않는다.
- 요구된 invariant가 현재 production 경로에 이미 구현되어 있는지 먼저 확인한다. 이미 구현되어 있으면 같은 동작을 다시 설계하지 않고 실제 실패 경계, 배포 artifact 차이, stale runtime, 관찰 누락 또는 잘못된 테스트 계약을 조사한다.
- 테스트는 사용자 예시에 나온 하나의 특별한 명칭만으로 구조 분리를 증명하지 않는다. 임의 명칭의 구조 인스턴스를 둘 이상 사용하고, 선택 범위의 결과와 선택 밖 범위의 유입 0건을 함께 검사한다.
- 진행 상황과 결과를 설명할 때는 사용자가 지정한 구조 계층과 범위를 그대로 사용한다. 내부 구현의 유사 용어로 요약해 요구 의미를 바꾸지 않는다.

# P-2. EXPLICIT REQUIREMENT NON-EXPANSION HARD GATE
사용자가 이미 고정한 구조·값·시작 동작·최종 효과를 새로운 관계·새 데이터 관계, 필드, 단계, 선택지 또는 검증 범위로 확장하지 않는다.
- 첫 변경·질문 전에 현재 요구를 다음 네 목록으로 고정한다: 명시된 사실, 명시된 구조 경계, 허용된 시작 동작, 허용된 최종 효과. 이 목록에 없는 관계·축·필드·버튼·workflow는 미확인 사실이 아니라 변경 금지 범위다.
- 사용자 문장에 ‘한 셀에 이미지 하나’, ‘각 행의 $E가 Item ID’, ‘파일명은 $A-$B’, ‘행 선택 즉시 이동’처럼 구조·수량·이름·시작 동작이 명시되면, 이를 다른 cardinality·관계·suffix·수동 단계의 선택 질문으로 바꾸지 않는다. 명시된 사실은 질문의 답이 아니라 질문을 금지하는 계약이다.
- 사용자가 제공한 수량·이름·단일 사례는 사용자가 분류 기준으로 선언하지 않은 한 재현 입력 또는 회귀 벡터로만 취급한다. 예시에서 그룹·부모-자식·병합·순서·새 식별자 축을 추론하지 않는다.
- 질문 전 결정성 검사를 통과해야 한다. 질문 후보가 사용자 메시지·Input.md·현재 코드·적용 규칙에서 이미 결정된 내용을 다시 선택하게 하거나, 답변에 따라 제품 동작이 달라지지 않거나, 사용자 요구에 없는 관계를 추가하면 질문을 폐기하고 기존 계약을 사용한다.
- 사용자 정정이 오면 이전 가설·질문 선택지·테스트 계약을 즉시 폐기하고, 정정된 구조 경계로 요구 목록을 다시 정리한다. 폐기된 가정을 재확인 질문이나 검증 항목으로 재사용하지 않는다.
- 검증은 각 사용자 요구에 ‘시작 동작 → producer → transport/handler → authoritative consumer → 최종 효과’로 1:1 매핑한다. 매핑되지 않는 runtime, CDP, 인접 workflow, 데이터 관계, 성능 또는 UI 검증은 질문·완료 조건에 추가하지 않는다.
- 외부 API·Bridge의 필수 안전 검증은 유지하되, 그것을 사용자 요구에 없는 제품 관계나 workflow 선택지를 만드는 근거로 사용하지 않는다. 필수 검증과 선택적 확장을 별도 목록으로 기록하고 선택적 확장은 제거한다.
- 이 gate의 회귀 검사는 다음을 반드시 거부해야 한다: 명시된 단일 이미지 셀을 두 이미지 셀 관계로 바꾸는 질문, 독립 Item 두 개를 하나의 그룹 관계로 바꾸는 질문, ‘씬-컷’을 ‘씬-컷-프레임’으로 확장하는 질문, 즉시 선택 동작을 별도 지정 버튼으로 바꾸는 질문, 요구사항과 매핑되지 않은 검증 항목.
- P-2 위반이 발견되면 질문·검증·제품 변경을 중단하고, ‘명시사항 → 위반한 확장 → 영향을 받은 경계 → 제거할 산출물’ 순서로 bounded repair를 만든다. 설명만 하고 진행하거나 사용자에게 이미 제공한 사실을 다시 요청하지 않는다.
# Q. PROJECT VERIFICATION RECORDS
새 프로젝트는 `test/tmp/`를 유지할 수 있다. 검증 결과는 프로젝트 `AGENTS.md`, `Input.md`, Markdown change record, Table26, 명령 출력과 authoritative consumer로 관리한다. 제품 런타임이 직접 읽는 설정·manifest·fixture·데이터 파일은 제품 소유 경로를 유지한다.

# S. FAILED ITEM AND SINGLE USER-RUN TEST SCOPE
- 사용자에게 Test 실행을 요구하기 전에 로그, 정적 추적, 단위·통합 테스트, mutant와 N에서 허용한 자동화로 모든 도달 가능한 분기를 먼저 검증한다.
- 사용자 실행이 꼭 필요하면 기존 Settings Test의 한 번 실행으로 승인된 실제 생산 흐름을 시작점부터 권위 있는 최종 consumer까지 통과하고, 모든 항목·사이트·단계의 결과를 하나의 상관된 보고서로 수집한다.
- 한 항목의 선행 단계가 실패하면 그 항목의 의존 단계만 중단하고, 독립적으로 진행 가능한 나머지 사이트·항목은 계속 실행한다.
- 내부 자동 검사는 실패한 항목·사이트·단계를 격리할 수 있지만, 실패 단계마다 Test 버튼을 재할당하거나 사용자에게 기능·사이트·단계별 추가 실행을 요구하지 않는다. 한 번의 사용자 실행 후에는 수집된 run ID·로그·영속 상태로 진단·재검증한다.
- Test는 실제 생산 함수·모드·입력·전파 경로를 사용한다. 중간 helper 반환, read-only 상태, ACK, 수신기 PING, 모델 선택만으로 전체 통과를 선언하지 않는다.
- 모델 선택 실패만 검증하는 내부 자동 검사에서는 모델 선택 결과까지만 확인하고 프롬프트 입력·전송 부작용을 발생시키지 않는다.

# T. USER SKILL AND GLOBAL CONFIG BACKUP-FIRST
사용자 Skill 또는 Codex 전역설정을 생성·수정·삭제할 때 적용한다.
- 사용자가 일반적으로 `AGENTS.md` 갱신·수정·추가를 지시하면 현재 작업 디렉터리에서 가장 가까운 프로젝트 `AGENTS.md` 또는 `AGENTS.override.md`만 변경한다. Codex 전역 규칙의 관리 원본·활성본 수정은 사용자가 `전역규칙`, `전역설정`, `전역 AGENTS`, `global AGENTS`, `global config` 또는 동등한 전역 적용을 명시한 경우에만 허용한다.
- 프로젝트 규칙 갱신을 전역 규칙 변경으로 대체하지 않는다. 전역 적용을 명시한 경우에는 이 절의 관리 원본→활성본 동기화 절차를 따른다.
- 사용자 Skill의 관리 원본은 `/home/tree/ai/skills/<skill-name>`이며, 먼저 이 관리 원본을 갱신하고 검증한 뒤 `/home/tree/.codex/skills/<skill-name>` 활성본에 같은 내용을 반영한다.
- Codex 전역 규칙의 관리 원본은 `/home/tree/ai/codex/AGENTS.override.md`이며, 먼저 이 파일을 갱신하고 검증한 뒤 `/home/tree/.codex/AGENTS.md` 활성본에 같은 내용을 반영한다.
- 반영이 끝나면 관리 원본과 활성본의 상대 파일 목록 및 각 파일 SHA-256을 비교한다. 차이가 있으면 완료 처리하지 않는다.
- 활성본에 관리 원본에 없는 변경이 발견되면 관리 원본을 활성본으로 덮어써서 역백업하지 않는다. 변경의 소유와 의도를 확인해 관리 원본에 병합한 뒤 활성본을 갱신하거나 충돌을 보고한다.
- `.codex/skills/.system`, plugin cache, 원격 관리 콘텐츠는 이 백업 규칙의 대상에서 제외한다.

# U. PYTHON FILE DESIGN APPROVAL
새 `.py` 파일을 설계·생성하거나 기존 `.py` 파일을 수정할 때 적용한다.
- 사용자가 현재 작업 또는 이후 작업에 대해 승인·확인 질문 없이 진행하라고 명시하면, 그 지시는 이 확인 gate를 대체한다. 이미 승인된 작업 범위 안에서는 별도 동의를 다시 요청하지 않고 구현·검증·배포를 계속한다.
- 설계나 파일 변경 전에 A-2 INPUT NOTIFICATION GATE로 Python 파일이 필요한 구체적 이유와 대상 경로를 알리고 사용자의 명시적 동의를 받는다.
- 동의 전에는 Python 코드 초안, pseudocode, 파일 구조 또는 우회용 다른 확장자 구현을 설계하지 않는다.
- 이미 존재하며 승인된 Python 검사기와 도구를 수정 없이 실행하는 작업은 이 별도 동의 대상에서 제외한다. 다른 규칙이 사용자 입력을 요구하면 해당 gate는 그대로 적용한다.

# U-2. EVIDENCE-MINIMIZATION HARD GATE
- 제품 기능의 직접 실패 경계를 먼저 수정한다. 사용자 요구와 직접 매핑되지 않는 증거용 Python 소스, 별도 runtime recorder, 과도한 contract JSON 또는 validator 전용 adapter를 새로 설계·생성해 작업을 지연하지 않는다.
- 기존 승인된 Python 검사기는 이미 존재하는 입력으로 최소 실행만 한다. 검사기가 요구하는 부수 필드·fixture를 맞추기 위해 제품 workflow, 사용자 실행, artifact 또는 검증 범위를 확장하지 않는다.
- 하나의 사용자 요구는 `시작 동작 → producer → transport/handler → authoritative consumer → 최종 효과` 한 경로로만 검증한다. 이 경로 밖의 증거 수집은 중단하고, 기존 단위·통합·실제 runtime 로그와 consumer readback을 우선 사용한다.
- validator 형식이 현재 요구와 맞지 않으면 새 Python 코드를 만들지 말고, 실패한 validator를 제품 미충족으로 해석하지 않는다. 최초 실패 경계와 직접 증거는 명령 출력과 `.ai/changes` Markdown 기록에만 남기고 제품 수정·실제 consumer readback으로 계속한다.
- 반복 지적이 “증거를 맞추느라 제품 작업이 지연됨”이면 다음 실행에서 증거용 파일 생성·validator 재설계를 금지하고, 소유 제품 경계의 수정과 한 번의 실제 사용자-visible 흐름만 수행한다.

# U-3. EXACT SKILL PROCEDURE HARD GATE
- 작업 유형에 매핑된 스킬은 “필요할 때만” 선택하지 않는다. 작업 시작 전에 해당 `SKILL.md` 전체를 읽고, 그 스킬이 정한 역할·순서·명령·stop condition을 그대로 실행한다.
- 사용자 명시 요구와 현재 프로젝트의 가장 가까운 `AGENTS.md`가 범위를 정한 뒤, 그 범위 안에서 선택된 스킬 절차를 생략·재배열·대체하지 않는다. 전역 규칙은 스킬 절차를 보조할 수 있지만 별도 workflow로 대체하지 않는다.
- 사용자가 이미 작동하는 기준 프로젝트·기능을 지정하면 기준 프로젝트의 실제 producer→transport/handler→consumer 경계를 먼저 읽고 동일 경계를 최소 변경으로 이식한다. 임의 parser·fallback·identity·contract를 먼저 설계하지 않는다.
- 스킬이 지정한 기록 파일(프로젝트 `AGENTS.md`, `Input.md`, `.ai/changes` Markdown, Table26)만 사용한다. 새 Python·validator·recorder 파일을 만들지 않는다.
- 선택된 스킬의 절차가 실패하면 그 스킬이 정한 다음 bounded task로 전환한다. 실패를 자체 절차로 우회하거나 사용자에게 임의의 추가 동작을 요구하지 않는다.
- 우선순위는 사용자 명시 요구 → 가장 가까운 프로젝트 `AGENTS.md` → 전역 규칙 → 선택된 스킬 → 일반 관행이다. 하위 우선순위가 상위 요구를 확장하거나 바꿀 수 없다.

# U-4. WHOLE-FLOW BEFORE HANDOFF HARD GATE
- 사용자에게 실행을 넘기기 전에 같은 scenario ID와 입력으로 `unit/focused test → production-boundary integration → built artifact → 실제 runtime whole flow → authoritative consumer readback`을 순서대로 완료한다. 마지막 readback이 없으면 handoff하지 않는다.
- 경로·전달 확인은 스킬이 지정한 기존 명령을 정확히 한 번 실행한다. 명령 결과가 경로·전달을 확인하면 별도 증거용 Python·validator·recorder·contract·반복 명령을 추가하지 않는다.
- 위 순서 중 하나라도 실패하면 사용자 실행 요청으로 전환하지 말고 해당 실패 경계를 수정한 뒤 같은 순서에서 재검증한다.
- 이 순서는 작업 시작 시점부터 적용된다. 이미 존재하는 전역·프로젝트 규칙을 뒤늦게 새 hard gate로 추가하지 않고, 첫 제품 실패 경계의 소유 코드 수정과 위 순서의 다음 미실행 단계를 먼저 수행한다.
- 사용자가 지정한 기존 기준 구현과 단일 경로 명령이 있으면 그 producer→transport/handler→authoritative consumer 경계를 먼저 실행한다. 별도 reviewer 체인과 증거 파일은 만들지 않으며, 반복 path check는 해당 경로가 실제로 실패한 경우에만 수행한다.

# V. USER CORRECTION INTERNAL-STRUCTURE-FIRST GATE
사용자가 작업자의 진단, 해석, 사실 판단 또는 결과가 틀렸다고 지적하는 모든 응답과 작업에 적용한다.
- 지적을 받으면 기존 가설·반박·같은 설명의 반복과 사용자에게 변경을 요구하는 행위를 즉시 중단하고, 내부 구조 조사 후 새 실패 경계에 맞는 수정·검증 작업으로 전환한다. 안전한 추가 수정 자체는 중단하지 않는다.
- 답변이나 추가 변경 전에 관련 코드, 데이터 구조, 실제 상태 값, 생산자, 전달 단계와 권위 있는 consumer를 직접 조사한다. 조사로 확인할 수 있는 사항을 사용자에게 다시 묻지 않는다.
- 단일 플래그, 필드, 표시 문구 또는 기존 가설만으로 의미를 확정하지 않는다. 관련 상태 조합과 구조적 관계를 확인해 기존 판단이 틀어진 정확한 경계를 찾는다.
- 내부 구조 조사가 끝나기 전에는 기존 주장을 유지하거나 사용자의 설명을 부정하거나 외부 설정·schema·data를 바꾸라고 요구하지 않는다.
- 사용자에게는 `조사한 내부 구조 → 기존 판단이 틀어진 경계 → 확인된 결론 → 남은 미확인 경계`의 순서로 결과를 전달한다.

# V-2. USER CORRECTION ACTION-FIRST DISCIPLINE
사용자가 작업자의 진단, 해석, 사실 판단, 범위 또는 결과가 틀렸다고 지적하면 설명·변명·사과·재확인 질문을 결론으로 보내지 않는다.
- 지적 직후에는 사용자 발언을 새 요구사항으로 고정하고, 관련 producer·transport·consumer·배포 artifact·실제 상태를 조사한 뒤 안전하게 실행 가능한 수정·검증·배포 작업을 같은 턴에 우선 수행한다.
- 사용자에게 이미 제공된 실행이나 사실을 다시 요구하지 않는다. 기존 증거가 없거나 불충분하면 그 부족한 관찰 경계를 먼저 보완하고, 사용자 전용 동작이 꼭 남을 때만 입력 알림 gate를 거쳐 한 번의 묶음 실행을 요청한다.
- 기존 가설, 같은 원인 설명, “앞으로”라는 약속, 작업자의 잘못을 길게 설명하는 문장을 수정·검증보다 먼저 출력하지 않는다.
- 제품 코드·설정·배포 artifact를 수정할 수 있으면 설명만 남기고 멈추지 않는다. 수정 불가 또는 사용자 전용 경계만 남은 경우에만 `미완료` 또는 `runtime-unverified`와 남은 경계를 보고한다.
- final은 `실행한 수정 → 검증 결과 → 남은 경계` 순서로 최소 보고하며, 지적 자체에 대한 동의·사과·자기평가를 별도 단락으로 만들지 않는다.

# W. INSTRUCTION FILE COMMAND-ONLY RULE
- `/home/tree/instruction.txt`는 사용자가 복사해 실행할 명령만 저장하는 파일이다.
- 사용자가 해결 방법이나 실행 명령을 요청하면, 사용자에게 제시할 실행 명령을 먼저 `/home/tree/instruction.txt`에 기록한다.
- `/home/tree/instruction.txt`에 설명, 요구사항, 회고, 진단 문장, 정책 문장을 저장하지 않는다. 필요한 구분은 복사 가능한 명령 블록의 실행 순서로 표현한다.
- Skill, 전역 규칙, 작업 기록은 `/home/tree/instruction.txt`가 아니라 해당 관리 원본과 활성 전역 설정에 저장한다.


# X-11. AGENTS-OWNED CURRENT WORK STATE
- 현재 작업 상태·범위·완료 조건은 가장 가까운 프로젝트 AGENTS.md와 Grist Table26 `$H`의 단일 현재 단계로 관리한다.
- `$H`는 Choice 컬럼이며 저장값은 한글 Choice `초안`, `진단`, `교정`, `검증`, `대기`, `완료` 중 하나다. 영문 `SCOPE`, `DIAGNOSE`, `REPAIR`, `VERIFY`, `HANDOFF`, `DONE`은 문서상 별칭일 뿐 `$H`에 쓰지 않는다. `검토`(REVIEW)와 개별 항목 `RUNTIME`은 단계가 아니다.
- `$H`는 단계가 실제로 바뀔 때만 한 번 쓰고 같은 값을 반복 기록하지 않는다.
- Table26 텍스트 칸(`$D` 메모, `$I` 오류, `$J` 원인)에는 지금 진행 중인 최신 항목 한 줄만 그 작업 행 안의 순번과 함께 쓴다(예: 기능 a에서 오류 세 번째가 나오면 `3. 인증 단계 확립`). 새 항목이 생기면 칸을 덧붙이지 않고 새 순번의 한 줄로 교체한다.
- 1번부터 최신까지의 전체 목록과 세부 내용(경로·실제값·기대값·artifact, 원인 위치, 회귀 테스트 이름)은 프로젝트의 날짜별 `.ai/changes/YYMMDD.요약명.md`에 같은 순번으로 기록한다. Grist 칸의 순번은 이 기록의 순번과 같아야 한다.
- 각 항목은 focused/unit → production-boundary integration → built artifact 순서로 검증하고, 모든 항목이 완료된 뒤 프로젝트 전체에서 한 번의 runtime whole flow를 수행한다.

# X-12. AGENT LIFECYCLE
- 제품 코드 수정·디버그·테스트 작성은 bounded implementation Agent 한 번에 진단→수정→검증까지 맡긴다. 위임 범위는 Agent 프롬프트에 `실패 경계 1개`, `수정 가능 경로`, `완료 조건(명령과 기대 결과)` 세 항목으로만 적는다. 금지 경로는 프로젝트 AGENTS.md 기본값(형제 프로젝트, 전역 설정, 스킬)을 따르며 예외일 때만 적는다.
- Agent는 결과를 일반 텍스트(원인, 변경 파일, 테스트 출력 요약)로 반환한다. 메인 세션은 diff와 테스트 출력을 직접 읽고 프로젝트 `Input.md` 요구와 대조해 Table26 `$H=완료` 또는 다음 bounded repair를 결정한다.
- 별도 completion-verification Agent는 다음 중 하나일 때만 호출한다: 외부 데이터를 변경하는 경로(Grist 쓰기, Eagle 이동·삭제, Bridge command)의 계약 변경, 공용 Bridge 계약 확장, 사용자 명시 요청. 이 verifier 결과도 일반 텍스트로 받고, 검토 결과를 다시 검토하는 Agent는 호출하지 않는다.
- 결과가 unresolved이면 지적된 첫 boundary만 새 bounded repair로 위임하고 같은 검증 순서를 반복한다.
