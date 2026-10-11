# AGENTS.md instructions for /home/tree/extensions

## Inherited Global Rule Reconciliation

- This directory and every child project inherit the current global `AGENTS.md` and `AGENTS.override.md` rules. The extensions-specific management source is Grist document `au1EbKLtZbrW`, `Table7`, the `관리 프로그램` cell, whose value is an Obsidian Vault-relative path.
- Project-local `AGENTS.md` files retain their product-specific boundaries, versioning, build, delivery, and runtime requirements. They must not restore the legacy project scope or replace the Table7 `관리 프로그램` source with a root `Input.md`.
- The Grist Table7/Table26 and Obsidian management-record writes performed by `initial-project` or `work-project` are the explicitly scoped workflow-record exception to a child project's product-source boundary; unrelated sibling or product changes remain forbidden.
- `.ai/changes` uses one record per Asia/Seoul calendar day: `YYMMDD.요약명.md`. Later work on the same day adds a child item under that day's existing single `# jobs` heading instead of creating another change file or another `# jobs` heading.
- 프로젝트 사용자 요구·변경·취소의 원본은 프로젝트 `Input.md`예요. 구현·검증 결과는 `.ai/changes/`에 기록하고, 확장 작업의 운영 task 상태는 계속 Table7 `관리 프로그램` 행의 Obsidian 관리 문서를 기준으로 해요.

## Project workflow skill routing
- 새 프로젝트 작업이거나 기존 프로젝트에 Table7 Raw ID 또는 Obsidian 초기화 경로가 없으면 `initial-project` 스킬(`initial_project` 요청 별칭)을 먼저 사용한다.
- 초기화된 프로젝트의 기능 추가·개선·오류 수정은 `work-project` 스킬(`work_project` 요청 별칭)을 사용한다.
- 두 스킬은 프로젝트별 관리 문서, 날짜별 `.ai/changes/YYMMDD.제목.md`, Grist `Table7`·`Table26`, Obsidian 초기 문서와 작업 상태를 연결하며, 요청하지 않은 인접 workflow를 변경하지 않는다.
- `/home/tree/extensions`의 작업 관리 문서는 확장 루트 `Input.md`가 아니라 Grist 문서 `au1EbKLtZbrW`의 `Table7` 행에 있는 `관리 프로그램` 셀의 Obsidian Vault-relative 경로로 resolve한다. Table7의 해당 셀이 관리 원본이다.

## Grist Access

- `/home/tree/extensions`와 모든 하위 프로젝트의 작업 관리 Grist 읽기·쓰기(Table7 `관리 프로그램`, Table26 `$H`·`$I`·`$J`, 행 생성·readback)는 반드시 `grist-access` Skill(`python3 ~/.codex/skills/grist-access/scripts/grist.py`)로 수행한다. API key는 그 Skill의 `.env`에만 두고, 사용자에게 요청하거나 명령·파일·로그·`.ai/changes`에 쓰지 않으며, curl·fetch·임시 스크립트로 Grist API를 직접 호출하지 않는다.
- 하위 프로젝트 `AGENTS.md`는 무엇을 기록할지만 정하고 Grist 연결 방법을 따로 지정하지 않는다. 직접 접근을 지시하는 하위 규칙이 생기면 이 절이 우선한다.
- 제품 코드가 런타임에 자체 설정으로 Grist에 접근하는 기능(Grist Custom Widget, `mobile_video_maker`의 `.env.local` 등 제품 서버)은 이 규칙의 대상이 아니다.
- 저장소의 `.env`, `.env.*`는 Git에서 제외하고 `.env.example`만 추적한다.

## Stagehand Access

- 에이전트 검사에서 Stagehand를 쓸 때는 반드시 `stagehand-access` Skill(`node ~/.codex/skills/stagehand-access/scripts/stagehand.mjs`, `openStagehand()`)을 사용한다. 실행본·AI 모델·API key는 그 Skill이 소유하며, key를 사용자에게 요청하거나 기록하지 않고, 하위 프로젝트에 검사용 Stagehand를 따로 설치하지 않는다.
- 제품 기능으로 Stagehand를 쓰는 하위 프로젝트 코드(예: `jw_downloader`의 자동화 스크립트)는 제품 소유 설정을 유지한다.

## Single User-Run Test Scope
- 사용자에게 Test 실행을 요구하기 전에 로그, 정적 추적, 단위·통합 테스트, mutant, 가능한 브라우저 자동화로 모든 도달 가능한 분기를 먼저 검증한다.
- 사용자 실행이 꼭 필요하면 기존 Settings Test의 한 번 실행으로 승인된 실제 생산 흐름을 시작점부터 권위 있는 최종 consumer까지 통과하고, 모든 항목·사이트·단계의 결과를 하나의 상관된 보고서로 수집한다.
- 한 항목의 선행 단계가 실패하면 그 항목의 의존 단계만 중단하고, 독립적으로 진행 가능한 나머지 사이트·항목은 계속 실행한다.
- 실패 단계 각각을 Test 버튼에 재할당해 사용자에게 기능·사이트·단계별 추가 실행을 요구하지 않는다. 한 번의 사용자 실행 후에는 수집된 run ID·로그·영속 상태와 인증된 자동화 프로필로 진단·재검증한다.
- Test는 실제 생산 함수·모드·입력·전파 경로를 사용한다. 중간 helper 반환, read-only 상태, ACK, 수신기 PING, 모델 선택만으로 전체 통과를 선언하지 않는다.
- 사용자에게 요구하는 확장 새로고침·재로드·생성·재생성·스크린샷·로그 추출도 사용자 실행 요청으로 센다. 이미 한 번의 사용자 실행 또는 요청이 있었다면 같은 작업에서 다른 클릭·재로드를 추가로 요구하지 않는다. 자동화로 확인할 수 없으면 `runtime-unverified`로 보고한다.
- 사용자 실행을 요청하기 전에는 같은 작업의 `.ai/changes` 기록과 대화에서 이전 요청·관측된 실행을 확인하고, 현재 artifact로 이미 요청했다면 다시 요구하지 않는다.

## Fish nf Notification
- 모든 final 응답 직전에 `fish -c 'nf -m "<작업명> <결과>"'`를 실행하고 종료 코드를 확인한다. 코드 작업이 없는 응답에도 적용한다.
- 선택 질문, 확인 질문, 자료 요청, Test 버튼 실행 등 사용자 응답이나 입력을 요구하는 모든 요청 직전에 `fish -c 'nf -m "<요청 내용>"'`를 실행하고 종료 코드를 확인한다. Test 실행 요청에만 한정하지 않는다.
- 이 규칙은 `/home/tree/extensions` 아래 모든 프로젝트의 최소 공통 규칙이다. 하위 `AGENTS.md`는 더 엄격한 알림 규칙을 추가할 수 있지만 이 규칙의 적용 범위를 축소하거나 무효화할 수 없다.
- Bash에서 nf를 직접 호출하지 않는다. 실행 파일이나 alias가 없다고 결론내리기 전에 `fish -c 'type nf; functions nf'`로 정의를 확인한다.
- 알림 실패 시 실패 사실과 종료 코드를 보고하며 알림 성공으로 표시하지 않는다.

## Persistent Commitment
- `앞으로`, `다음부터`, `이후에는`처럼 향후 행동을 약속하는 답변을 텍스트로만 출력하지 않는다.
- 지속 가능한 작업 방침은 답변 전에 이 `AGENTS.md` 또는 해당 범위를 소유한 영구 설정 파일에 먼저 반영하고, final에는 실제로 변경한 설정과 경로를 보고한다.
- 현재 권한이나 환경 때문에 영구 설정을 변경할 수 없으면 변경하지 못한 사실을 명시하며, 지킬 수 없는 향후 행동을 약속하지 않는다.
- 이 규칙은 `/home/tree/extensions` 아래 모든 프로젝트에 적용되며 하위 `AGENTS.md`에서 축소하거나 무효화할 수 없다.

## Current Work State Source

- 각 프로젝트의 현재 작업 상태·범위·완료 조건은 가장 가까운 프로젝트 `AGENTS.md`와 Table26 `$H`에 기록합니다.
- `$H`는 Choice 컬럼이며 `초안`, `진단`, `교정`, `검증`, `대기`, `완료` 중 하나의 한글 값만 저장합니다. `대기`는 최종 사용자 입력만 남은 상태입니다. 현재 단계는 `$H` 하나로만 표시하며 `$C`(상태)에는 단계를 쓰지 않습니다. 영문 `SCOPE`·`DIAGNOSE`·`REPAIR`·`VERIFY`·`HANDOFF`·`DONE`은 문서상 별칭이며 `$H`에 쓰지 않습니다. `검토`와 개별 항목의 `RUNTIME`은 단계가 아닙니다.
- Agent를 호출할 때는 해당 프로젝트 `AGENTS.md`를 먼저 읽도록 지시하고, 프롬프트에 실패 경계 1개·수정 가능 경로·완료 조건(명령과 기대 결과)만 적습니다. Agent는 진단→수정→검증을 한 번에 수행하고 결과를 일반 텍스트로 반환합니다.
- 오류·회귀·동작하지 않는 기능 보고의 관찰값과 진단은 해당 작업의 Table26 행 `$I`(오류)·`$J`(원인)에만 기록합니다. 이를 위해 별도 행을 만들거나 `$G` 종류를 바꾸지 않습니다. 해당 작업 행이 없을 때만 일반 절차로 작업 행을 만들고 프로젝트 `AGENTS.md`의 task Raw ID를 그 행으로 맞춥니다.
- Table26 텍스트 칸(`$D` 메모, `$I` 오류, `$J` 원인)에는 지금 진행 중인 최신 항목 한 줄만 작업 행 안의 순번과 함께 씁니다(예: `3. 인증 단계 확립`). 새 항목이 생기면 덧붙이지 않고 새 순번의 한 줄로 교체하며, 1번부터의 전체 목록과 세부 내용은 같은 순번으로 날짜별 `.ai/changes` 기록에만 남깁니다.
- 코드 수정 전에 실제 runtime(콘솔 예외, 렌더 화면, 실패 경로의 실제 상태 값, 로드된 artifact hash)을 먼저 읽고, 명령 출력에서 복사한 `경로 / 실제값 / 기대값 / artifact`를 `.ai/changes`의 해당 순번 항목에 기록합니다. `$I`에는 그 순번과 한 줄 요약만 씁니다. 추론은 관찰값으로 쓰지 않습니다.
- 원인의 최초 실패 경계, 소유 코드 위치, 실제값을 재현하는 회귀 테스트 이름은 `.ai/changes`의 해당 순번 항목에 기록하고, `$J`에는 같은 순번의 원인 한 줄만 씁니다. 한 증상의 원인이 여럿이면 `.ai/changes`에 번호로 나열하고 `$J`에는 최신 원인만 둡니다.
- `$H` `진단→교정`은 `$I`·`$J`가 모두 채워졌을 때만, `교정→검증`은 `$J` 테스트가 수정 전 코드에서 실패하고 수정 후 통과했을 때만 전환합니다. mock·fixture 입력은 `$I`에 캡처된 실제 모양을 사용하며, runtime에서 관찰하지 않은 모양의 테스트는 회귀 근거로 인정하지 않습니다.
- 사용자 동작 경로의 조기 return과 비동기 예외는 로그나 화면 메시지를 남기며, 조용한 no-op 자체를 실패 경계로 봅니다. 사용자 실행을 요청하기 전에 브라우저에 로드된 artifact hash가 빌드 artifact와 같은지 확인합니다.
- 메인 세션은 diff와 테스트 출력을 직접 읽고 `Input.md` 요구와 대조해 Table26 `$H=완료` 또는 다음 bounded 작업을 결정합니다. 별도 completion-verification Agent는 외부 데이터 변경 경로의 계약 변경, 공용 Bridge 계약 확장, 사용자 명시 요청일 때만 호출합니다.
- 각 항목은 focused/unit → production-boundary integration → built artifact 순서로 검증하고, 모든 항목이 완료된 뒤 한 번의 프로젝트-level runtime whole flow만 수행합니다.

## Response Discipline
- 사용자 응답에서 한국어 확인·동의 관용구 `맞습니다`, `맞아요`, `인식했습니다`, `알겠습니다`, `그렇습니다`를 사용하지 않는다.
- 사용자가 실수를 지적하면 사과나 동의 표현으로 시작하지 말고, 확인한 원인과 실제로 수행한 수정부터 제시한다.
- 모든 final 응답을 보내기 전에 금지 표현을 문자열 단위로 검사하고, 하나라도 있으면 해당 문장 전체를 다시 작성한다.
- 이 규칙을 설명하거나 인용하는 사용자 응답에서도 금지 표현 자체를 반복 출력하지 않는다.
- 이 규칙은 `/home/tree/extensions` 아래 모든 프로젝트의 최소 공통 규칙이며 하위 `AGENTS.md`에서 축소하거나 무효화할 수 없다.

## Version Bump Default
기능 개선 또는 신규 기능 추가 작업을 수행할 때는 기본적으로 버전을 `0.0.1`씩 올린다.
- 별도 지시가 없으면 patch 단위 증가를 사용한다.
- 각 버전 세그먼트는 기본적으로 최대 두 자리(`00`~`99`)를 넘기지 않도록 관리한다.
- Obsidian 플러그인은 `manifest.json`과 `package.json`의 버전을 함께 갱신한다.

## Extension Project Directory Naming
`/home/tree/extensions` 아래에 새 확장프로그램 프로젝트 디렉터리를 만들 때 이름 구분자로 `-` 대신 `_`를 사용한다.
- 이 규칙은 새 프로젝트 디렉터리 이름에 적용한다.
- `package-lock.json` 같은 도구 고정 파일명과 기존 프로젝트 내부 파일명은 일괄 변경하지 않는다.

## Development Copy Drive
개발·기능 추가 후 복사하도록 설정된 Windows `H:\code` 및 WSL `/mnt/h/code`를 배포 대상 루트로 사용한다.
- 개선이(Codex)가 기능 추가 또는 개선판을 실제 제품 파일에 반영하면 프로젝트 디렉터리와 같은 이름의 `/mnt/h/code/<프로젝트 디렉터리명>`으로 실행 가능한 결과물을 복사한다.
- 복사 대상에는 제품 소스와 배포 산출물을 포함하고, `test/tmp`, `__pycache__`, 로그 및 운영용 비밀값은 제외한다.
- H:\\code ZIP 생성은 기본 복사 동작이 아니다. `/home/tree/extensions/blender_camera_control`만 현재 버전의 단일 `/mnt/h/code/blender_camera_control_<버전>.zip`을 생성하며, ZIP 내부 최상위에는 `blender_camera_control/` 폴더가 있어야 한다. 그 밖의 프로젝트는 사용자가 명시적으로 요청하거나 해당 프로젝트의 더 구체적인 규칙이 있을 때만 ZIP을 생성한다.
- 일반 배포 복사는 한 번의 `cp` 실행과 종료 코드만으로 처리한다. 사용자가 checksum·파일 목록 검증을 명시하거나 파일 자체가 최종 산출물인 경우에만 추가 검증을 한다.
- 프로젝트별 대상 폴더명과 추가로 복사할 산출물 범위는 각 프로젝트의 규칙을 유지한다.
- H: 마운트는 사전 검사하지 않고 복사를 한 번 시도한다. 실패하면 복사만 `deployment-deferred`로 기록하고, 소스 수정·빌드·테스트·CDP 준비와 실행은 계속한다.
- 별도로 지정된 C: 드라이브의 Obsidian Vault 및 바탕화면 경로는 기존 규칙을 따른다.

## Shared Integration Bridges

- `/home/tree/extensions` 아래에서 새 Plugin 또는 script를 만들거나 기존 기능에 외부 연동을 추가할 때, Eagle 데이터의 조회·생성·수정·이동은 먼저 `grist_video_maker/eagle-data-bridge`의 `README.md`, command/query 계약과 production handler를 확인하고 그 형식을 통해 수행한다.
- Eagle Local API나 Eagle SDK를 직접 호출하는 별도 transport, 중복 import journal, retry queue 또는 응답 판독기를 새 consumer 프로젝트에 만들지 않는다. 필요한 기능이 기존 Eagle Data Bridge 계약에 없으면 직접 우회하지 않고 Data Bridge에 좁은 command/query 계약을 추가한 뒤 consumer가 이를 사용하게 한다.
- 브라우저를 사용하는 외부 AI 사이트에 데이터를 보내거나 결과를 받을 때는 `chrome_prompt_bridge`의 기존 contract와 production workflow를 먼저 확인하고 Chrome Prompt Bridge를 사용한다.
- 외부 AI 사이트를 직접 여는 tab orchestrator, 사이트별 receiver, 전송 retry queue 또는 결과 correlation ledger를 새 Plugin이나 script에 중복 구현하지 않는다. 필요한 기능이 없으면 직접 transport를 추가하지 않고 Prompt Bridge의 소유 계약을 확장한다.
- Bridge 계약을 확장할 때는 Bridge의 기존 consumer와 인접 workflow를 변경하지 않는 회귀 검사, 요청 ID 기반 효과 횟수, 응답 유실·재시도 경계와 최종 consumer readback을 함께 검증한다.

## CDP Browser Checks

- 브라우저가 필요한 검사는 `run-cdp` Skill로 수행한다. 화면이 보이는(`headless: false`) WSL Chromium을 전용 프로필 `/home/tree/.config/codex-run-cdp`와 endpoint `http://127.0.0.1:9222`로 쓰고, `scripts/stagehand_session.mjs`로 Stagehand가 붙거나(정상 endpoint) 직접 띄운다(`keepAlive`). headless는 프로젝트 `AGENTS.md`가 headless 전용 runtime을 명시할 때만 쓴다.
- 대상 URL, artifact(extension 디렉터리), 시작 동작, authoritative consumer는 프로젝트 `AGENTS.md`의 runtime 설정에서만 읽는다. 공용 설정에 제품 값을 고정하지 않고, Grist를 쓰지 않는 프로젝트를 Grist 대상으로 검사하지 않는다.
- 정상 endpoint는 교체하지 않고 붙는다. 새 extension 빌드를 로드할 때만 `run_cdp --replace -e <extension 디렉터리>`로 전용 프로필 브라우저를 재시작한다. Windows 프로세스·포트 소유권은 PowerShell/CIM/NetTCPConnection으로 확인한다.
- 시작 동작 전에 실제로 로드된 extension version·digest가 빌드와 같은지, 목적 페이지가 열렸는지 확인하고 consumer baseline을 읽는다. `about:blank`, target 목록, 로그, build 성공만으로 runtime 준비를 통과시키지 않는다.
- 전용 프로필은 로그인(로컬 Grist `http://localhost:8484` 등)을 유지한다. 로그인 페이지가 나오면 열린 브라우저에서 사용자에게 로그인 한 번만 요청하고, 이후 검사에서 다시 요청하지 않는다. 결제·비가역 전송·비용 발생 동작도 사용자 단일 동작으로 남긴다.
- 한 검사는 새 증거가 있는 재시도 1회와 브라우저 작업 약 5분 안에서 끝낸다. 결과(브라우저 버전, headless 여부, 대상 URL, extension 버전, 실제 시작 동작, consumer readback)는 명령 출력과 `.ai/changes`에 남기고, 토큰·인증 헤더·cookie·원문 payload·개인 전체 경로는 기록하지 않는다.

## Obsidian Plugin Copy Default
`/home/tree/extensions` 아래의 Obsidian 플러그인 작업에서 생성 또는 수정이 끝나면 실행 가능한 결과물을 아래 Vault의 플러그인 폴더로 복사한다.
- Windows 대상 루트는 `C:\Users\tende\OneDrive\일기장\.obsidian\plugins`이다.
- WSL 대상 루트는 `/mnt/c/Users/tende/OneDrive/일기장/.obsidian/plugins`이다.
- 대상 폴더명은 프로젝트 디렉터리명과 동일하게 둔다.
- 최소 복사 대상은 `manifest.json`, `main.js`이며, 프로젝트에 `styles.css`가 있으면 함께 복사한다.
- 기존 대상의 `data.json`처럼 사용자 설정을 담은 파일은 덮어쓰거나 삭제하지 않는다.
- 복사는 한 번 실행하고 종료 코드만 확인한다. 사용자가 checksum 검증을 명시한 경우에만 추가 검증을 한다.

## Desktop Copy Default
빌드 산출물을 바탕화면에 복사하라는 지시가 있으면 Windows 바탕화면 경로를 사용한다.
- 기본 대상 경로는 `/mnt/c/users/tende/Desktop`이다.
- `/home/tree/mnt/c/...` 경로를 Windows 바탕화면으로 사용하지 않는다.
- Obsidian 플러그인은 실행 가능한 플러그인 폴더 형태로 복사한다.
- 기본 폴더명은 프로젝트 디렉터리명과 동일하게 둔다.
- Obsidian 플러그인 복사 대상에는 최소 `manifest.json`, `main.js`를 포함하고, `styles.css`가 있으면 함께 포함한다.
- 복사는 한 번 실행하고 종료 코드만 확인한다. 사용자가 checksum 또는 파일 목록 검증을 명시한 경우에만 추가 검증을 한다.

## Verification Artifact Workspace

- 테스트가 실제로 필요로 하는 임시 `.json`·`.txt`(fixture 출력, 스크린샷 등)만 이 프로젝트의 `test/tmp/` 아래에서 생성하고, 작업이 끝나면 지운다.
- 위 검증용 임시 파일을 프로젝트 루트나 `/home/tree/extensions` 루트에 생성하지 않는다.
- 제품 런타임이 직접 읽는 설정·manifest·fixture·데이터 JSON/TXT는 이 규칙의 이동 대상이 아니며 기존 소유 경로를 유지한다.
- 모든 하위 프로젝트의 `test/tmp/` 디렉터리와 그 안의 내용은 로컬 임시 검증 자료로 취급하며 Git에 추가하거나 커밋하지 않는다. 빈 디렉터리 유지를 위한 `.gitkeep`도 포함하지 않는다.
