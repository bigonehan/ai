# A. TASK OUTCOME NOTIFICATION GATE
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
실행 출력: `[PLAN INPUT GATE] exit: <code>`
- `nf` 실행 후 필요한 질문 또는 `request_user_input`을 전송한다.

추가 규칙:
- 질문 묶음마다 새로 실행하고 이전 실행 결과를 재사용하지 않는다.
- 직접 작성한 질문과 `request_user_input`에 모두 적용한다.
- `<proposed_plan>`을 포함한 final은 전송 뒤 클라이언트가 `Implement the plan.` 등의 후속 선택 UI를 표시하므로 선택 요청으로 간주한다.
- 신규 계획 또는 수정된 계획을 `<proposed_plan>` final로 보내기 직전에 `/usr/bin/fish -c 'nf_auto -m "Codex input required: choose next action for finalized plan"'`을 실행하고 `[PLAN INPUT GATE] exit: <code>`를 출력한다.
- `nf_auto`는 호출 당시 `TMUX_PANE`을 고정하고 `nf` 알림을 최대 30초 동안 표시한다. 사용자가 확인하거나 popup이 timeout되어 알림이 성공하면 5초 뒤 고정한 pane에 Enter(`C-m`)를 정확히 한 번 입력한다.
- `nf_auto` 실행 직후 `<proposed_plan>` final을 보내 선택 UI가 Enter 입력 전에 렌더링되게 한다.
- 이후 턴에 계획을 다시 출력하면 새 선택 요청이므로 이전 계획 final의 실행 결과를 재사용하지 않고 같은 gate를 다시 실행한다.
- 단순 진행 상황 안내에는 적용하지 않는다.

# A-3. NF USAGE BOUNDARY
구현 시작을 알리는 용도로 `nf`를 실행하지 않는다.
- `nf`는 A-2에 따른 일반 사용자 입력 요청 직전 또는 A에 따른 작업 결론 final 직전에만 실행한다. `nf_auto`는 A-2에 명시된 `<proposed_plan>` final 직전에만 실행한다.
- 코드 수정, 파일 생성, 설정 변경, 빌드, 배포 또는 복사를 시작한다는 이유만으로 `nf`를 실행하지 않는다.

# A-4. APPROVED PLAN EXECUTION GATE
사용자가 직전 `<proposed_plan>`에 대해 `Implement the plan.` 또는 동등한 구현 지시를 하면, 그 계획에 명시된 모든 구현·테스트·실제 runtime 검증·빌드·배포·복사·재로드·브라우저 자동화·프로세스 조작을 해당 작업에 한해 명시적으로 승인한 것으로 본다.
- 계획에 명시된 작업은 "별도 명시 필요"라는 일반 규칙을 이유로 다시 차단하거나 생략하지 않는다. 계획에 없는 작업까지 승인 범위를 넓히지는 않는다.
- 승인된 계획은 실행 계약이다. 실행 중 계획 항목, 범위, 완료 조건을 임의로 축소·대체·재해석하지 않는다.
- unit test, mock, fixture, build 또는 checksum을 계획에 명시된 실제 runtime 검증의 대체물로 사용하지 않는다.
- final 전에 계획의 각 항목을 `완료 / 미완료 / 차단` 상태 및 실행 증거와 일대일로 대조한다. 미완료 항목이 있고 안전하게 계속할 수 있으면 final을 보내지 않고 계속 수행한다.
- 승인된 계획과 실제 상위 system·developer 규칙이 충돌하면 임의로 좁혀 진행하지 않는다. 충돌 지점과 수행 불가능한 항목을 사용자에게 알리고 결정을 요청한다.
- `runtime-unverified`는 실제 runtime에 기술적으로 접근할 수 없거나 상위 규칙 충돌로 실행할 수 없고 안전한 대안까지 소진한 경우에만 사용한다. 단순히 재로드나 자동화가 필요하다는 이유로 중단하지 않는다.

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

# D. VERSION BUMP DEFAULT
기능 개선 또는 신규 기능 추가 작업을 수행할 때는 기본적으로 버전을 `0.0.1`씩 올린다.
- 별도 지시가 없으면 patch 단위 증가를 사용한다.
- 각 버전 세그먼트는 기본적으로 최대 두 자리(`00`~`99`)를 넘기지 않도록 관리한다.

# E. DEV SERVER EXECUTION
작업자는 사용자의 명시적 요청 없이 dev 서버를 실행하지 않는다.
- 구현 또는 검증 후에도 `npm run dev`, `bun run dev`, `next dev` 등 장기 실행 dev 서버 명령은 자동 실행하지 않는다.
- 사용자가 명시적으로 dev 서버 실행을 요청한 경우에만 실행하고, 작업 완료 또는 사용자 요청 시 종료 상태를 확인한다.

# F. EAGLE DUPLECATE HANDLER COPY DEFAULT
`/home/tree/project/extensions/eagle-duplecate-handler` 작업에서 생성 또는 수정이 끝나면 결과물을 Windows 경로 `D:\code\eagle-duplecate-handler`에 복사한다.
- WSL 경로는 `/mnt/d/code/eagle-duplecate-handler`를 사용한다.
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
- 첫 mutation 전에 `/home/tree/ai/skills/test_manager/scripts/verify_runtime_evidence.py snapshot <contract.json> <state.json>`으로 범위를 고정하고, 실제 runtime 검사는 같은 스크립트의 `run`으로 실행한다. 모든 테스트 근거 완료 응답 전에 `validate <contract.json> <state.json> <evidence.json>`을 실행하며, gate가 `complete`를 승인하지 않으면 완료라고 표현하지 않고 `runtime-unverified`와 미검증 경계를 보고한다.
- 브라우저 확장 작업은 실제 브라우저에 배포본을 로드하고 사용자 시작 동작, content script 또는 service worker 전달, 실제 DOM·editor·download·navigation 결과를 검증한다. reload·재주입·restart가 관련되면 같은 실제 브라우저 세션에서 해당 전환도 실행한다.
- cross-file 전역 API, namespace export, injected script API 또는 plugin registry member를 추가·변경·소비하면 producer 등록명, production load order, consumer 호출명, 실제 사용자 handler와 authoritative consumer를 하나의 boundary 시나리오에서 실행한다. helper 직접 호출이나 소스 문자열 검사만으로 통과시키지 않고 owner/member 오탈자 mutant를 거부해야 한다.
- 협업 Agent를 사용할 수 있는 테스트 작업은 구현·테스트 작성자와 다른 Agent가 요구, production loader, 표준 suite 등록, stale fixture, mutant-red 증거, 인접 경로 무변경을 독립 검증한다. `/home/tree/ai/skills/test_manager/scripts/verify_independent_review.py validate <contract.json> <receipt.json>`이 통과하지 않으면 완료 처리하지 않는다. 검증 뒤 제품 또는 테스트 artifact가 바뀌면 새 hash로 다시 독립 검증한다.

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
- `_dev` 배포 폴더 생성·복사·삭제, Chrome 확장 등록, 브라우저 자동화, Bridge ID·소비자 경로 변경, 설정·이미지·스토리지 이전, 프로세스 종료·재시작은 사용자가 해당 동작을 별도로 명시하지 않으면 수행하지 않는다. 단, 승인된 `<proposed_plan>`에 해당 동작이 명시되어 있고 사용자가 `Implement the plan.` 또는 동등한 구현 지시를 한 경우 A-4에 따라 별도 명시가 충족된 것으로 본다.
- Skill 적용으로 범위를 넓히지 말고 사용자가 지정한 브랜치 작업과 후속 구현만 수행한다.

# N. USER-EXECUTED AUTOMATION GATE
브라우저·데스크톱 UI 조작, 확장 프로그램 재로드, 실제 사용자 동작 재현처럼 automation이 필요한 모든 작업에 적용한다.
- 작업자는 마우스·키보드 입력, UI Automation, Playwright·CDP·WebDriver 또는 동등한 수단으로 해당 동작을 직접 실행하지 않는다.
- 사용자 실행을 요청하기 전에 생산자 입력, 전달 단계, 최종 consumer 결과와 오류를 확인할 수 있는 로그를 먼저 구현·활성화하고, 로그 위치·식별자·민감정보 마스킹 여부와 실행 전 기준 상태를 검증한다.
- 로깅 환경이 실제 실행 결과를 구분할 수 없는 상태에서는 사용자에게 실행을 요청하지 않고 먼저 관찰 가능성을 보완한다.
- 환경 준비가 끝나면 A-2 INPUT NOTIFICATION GATE에 따라 `nf -m "Codex input required: <실행이 필요한 이유와 사용자가 실행할 정확한 동작>"`를 시도한 뒤, 실행 결과와 관계없이 사용자에게 실행할 UI, 버튼, 입력, 예상 시점과 실행 후 알려줄 내용을 구체적으로 요청한다.
- 사용자가 실행 완료를 알리면 준비한 로그와 실제 consumer 상태를 읽어 원인을 분석하고 수정한다. 사용자 기억이나 스크린샷만으로 성공을 단정하지 않는다.
- 한 번의 사용자 실행으로 필요한 증거를 최대한 수집하도록 요청을 묶되, 서로 다른 부작용을 임의로 합치지 않는다.

# O. VISUAL SIZE REQUIREMENT DISCIPLINE
UI의 폭·높이·간격·비율을 줄이거나 늘리라는 요구에 적용한다.
- `더 줄여`, `한 칸 크기`, `일반 text 높이`, `1:1`, 참조 이미지와 같은 상대적 표현을 각각 독립된 장식 요청으로 분리하지 말고, 주변 요소와의 관계를 포함한 하나의 layout 계약으로 해석한다.
- 사용자가 숫자를 지정하지 않았다는 이유로 임의의 중간 크기를 여러 차례 제안하거나 같은 크기 의도를 반복 질문하지 않는다. 현재 DOM·CSS·참조 이미지에서 기존 computed size와 제약을 읽고 요청 방향을 충족하는 명확한 목표값을 정한다.
- table·grid·flex의 자동 배분으로 선언 폭이 실제 화면에서 달라질 수 있으면 class 선언만으로 완료 처리하지 않는다. `colgroup`, grid track, flex basis 등 권위 있는 layout 제약을 사용하고 실제 runtime의 computed width·height를 기록해 확인한다.
- 크기 변경 요청에는 관련 요소별 `변경 전 실제 크기 → 목표 크기 → runtime 측정값`을 완료 근거로 남긴다.
- 사용자가 같은 크기 문제를 두 번째 지적하면 missed regression으로 처리하고, 이전 수정이 실제 화면에서 요구를 충족하지 못한 이유를 같은 작업에서 분석·기록한다.

# O-2. UI SKILL MANDATORY GATE
UI/UX 또는 frontend 표현을 설계·구현·수정·디버그·검증하는 모든 작업에 적용한다. `width`, `height`, column, cell, table, grid, flex, spacing, responsive, typography, color, hover, preview, density, alignment, screenshot, `current.png` 또는 반복된 시각 불만이 포함된 요청도 모두 포함한다.
- 계획 수립이나 첫 mutation 전에 `/home/tree/.codex/skills/ui-design-implementation/SKILL.md`를 끝까지 읽고 `$ui-design-implementation` 절차를 사용한다. 파일을 읽지 못하면 UI mutation을 수행하지 않는다.
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
- 사용자의 후속 정정이 기존 해석, 계획, 요구 기록 또는 테스트 계약의 구조 경계를 바꾸면 이전 계약을 즉시 폐기하고 정정된 경계로 다시 작성한다. 계약을 갱신하기 전에는 추가 제품 수정을 진행하지 않는다.
- 요구된 invariant가 현재 production 경로에 이미 구현되어 있는지 먼저 확인한다. 이미 구현되어 있으면 같은 동작을 다시 설계하지 않고 실제 실패 경계, 배포 artifact 차이, stale runtime, 관찰 누락 또는 잘못된 테스트 계약을 조사한다.
- 테스트는 사용자 예시에 나온 하나의 특별한 명칭만으로 구조 분리를 증명하지 않는다. 임의 명칭의 구조 인스턴스를 둘 이상 사용하고, 선택 범위의 결과와 선택 밖 범위의 유입 0건을 함께 검사한다.
- 진행 상황과 결과를 설명할 때는 사용자가 지정한 구조 계층과 범위를 그대로 사용한다. 내부 구현의 유사 용어로 요약해 요구 의미를 바꾸지 않는다.

# Q. NEW PROJECT VERIFICATION ARTIFACT LAYOUT
새 프로젝트를 만들고 프로젝트별 `AGENTS.md`를 생성할 때 적용한다.
- 프로젝트 루트에 `test/tmp/` 디렉터리를 함께 만든다.
- 생성하는 `AGENTS.md`에 테스트·검증 과정의 계약, 상태, 증거, receipt 및 요구 캡처용 임시 `.json`·`.txt` 파일을 해당 프로젝트의 `test/tmp/` 아래에서만 생성하고 사용한다는 규칙을 포함한다.
- 위 검증용 임시 파일을 프로젝트 루트나 상위 프로젝트 모음 디렉터리에 생성하지 않는다.
- 제품 런타임이 직접 읽는 설정·manifest·fixture·데이터 JSON/TXT는 이 규칙의 이동 대상으로 취급하지 않고 기존 소유 경로를 유지한다.

# R. STAGED FEATURE PLAN SKILL
Plan 모드에서 기능 추가, 기능 수정, 버그 해결, 빌드 또는 설정 변경을 계획할 때 `/home/tree/.codex/skills/staged-feature-plan/SKILL.md`를 끝까지 읽고 적용한다.
- 전체 요청을 의존성 순서의 단계별 기능 목표로 분리하고, 각 단계의 구현과 테스트가 끝난 뒤에만 다음 단계의 제품 코드를 수정한다.
- 승인된 계획을 실행할 때 첫 프로젝트 mutation으로 프로젝트 루트의 `.ai/plan/YYMMDD.요약.md`를 생성하거나 갱신한다. 날짜는 Asia/Seoul 기준이며 요약은 1~9자로 제한한다.
- 같은 날짜에는 계획 파일을 하나만 사용하고, 서로 다른 요구는 같은 파일의 독립된 `# 요청명` 섹션으로 누적한다. 같은 요구의 후속 변경은 기존 섹션과 변경 기록을 갱신한다.
- 각 단계에 사용자 개입 `[y/n]`을 명시한다. `[n]`은 Agent가 authoritative consumer까지 직접 검증한 뒤 자동 진행하고, `[y]`는 실행 로그와 기준 상태를 먼저 준비한 뒤 사용자 실행과 결과 판독을 포함한다.
- `[n]` 단계에서 사용자 전용 runtime 동작이 필요해지면 검증을 생략하지 않고 입력 알림 gate를 거쳐 `[y]`로 변경한다.
- 일일 계획 문서는 Skill의 `scripts/validate_stage_plan.py`로 검증하고, 완료 단계는 체크 상태·상태·완료 증거가 일치해야 한다.
