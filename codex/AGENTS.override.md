# A. TASK COMPLETION GATE
작업 완료 시에만 적용. 완료 후 final 응답 전에 실행한다.
STEP 1. nf -m "<task-name> complete" 
실행 출력: [GATE A-1] exit: <code>
- code != 0 → [GATE A-1] HALT 출력 후 중단. final 응답 없음.
- code = 0 → STEP 2

STEP 2. → DENIAL SCAN (B) 실행

# A-2. PLAN MODE INPUT NOTIFICATION GATE
Plan Mode에서 사용자 질문, 확인 요청, 선택 요청 또는 `request_user_input` 호출이 필요할 때마다 질문을 보내기 직전에 적용한다.

STEP 1. `nf -m "Codex input required: <입력이 필요한 이유>"`
실행 출력: `[PLAN INPUT GATE] exit: <code>`
- code != 0 → `[PLAN INPUT GATE] HALT`를 출력하고 사용자에게 질문하지 않으며 `request_user_input`도 호출하지 않는다.
- code = 0 → 필요한 질문 또는 `request_user_input`을 전송한다.

추가 규칙:
- 질문 묶음마다 새로 실행하고 이전 실행 결과를 재사용하지 않는다.
- 직접 작성한 질문과 `request_user_input`에 모두 적용한다.
- 단순 진행 상황 안내와 완성된 계획 전달에는 적용하지 않는다.
- `nf` 명령이 없거나 실행할 수 없으면 gate 실패로 처리한다.

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
- 재시도 흐름은 부작용 발생 전과 후를 구분하고, 붙여넣기·Enter·탭 생성·파일 쓰기·API 변경 같은 부작용의 호출 횟수와 순서를 검사한다.
- 부작용 뒤 응답 유실, 부분 성공, timeout, 중복 request ID를 포함한 적용 가능한 경계를 검증하지 않으면 테스트 완료로 처리하지 않는다.
- 중복되거나 새로고침 뒤 남는 UI는 저장 데이터 수, 단일 컴포넌트 내부 노드 수, 문서 루트 수, listener·worker·process 인스턴스 수, 실제 화면 결과를 각각 측정한다. 스크린샷만으로 데이터 중복을 단정하지 않고, 이전 인스턴스의 stale DOM을 남긴 재주입·reload·update·SPA 이동·cleanup 테스트에서 최종 owner와 루트가 하나인지 검증한다.
- 사용자 관점에서 한 번인 붙여넣기·쓰기·업로드·전송을 여러 consumer 호출이나 chunk로 나누면 각 호출을 별도 부작용으로 계산한다. 명시적인 streaming 계약이 없으면 전체 payload를 담은 호출 정확히 한 번을 검사한다.
- 같은 사용자-visible 실패가 두 번째 보고되면 missed regression으로 처리하고, 기존 테스트가 통과한 이유를 분석한 뒤 수정 전 baseline 또는 격리된 동일 mutant에서 실패하는 회귀 테스트를 증명해야 완료할 수 있다.
- 테스트의 관찰 단위는 사용자 동작에서 실제 consumer 상태까지 생산자·handler·전파·consumer·결과 경계를 추적해 정한다. 함수 호출 횟수를 실제 효과 횟수로 대신하지 말고 bubbling, 다중 listener, fan-out·fan-in, fallback과 deduplication 뒤의 권위 있는 consumer 결과를 별도로 검사한다.

# L. STRICT REQUEST SCOPE AND ADJACENT WORKFLOW ISOLATION
기능 추가, 수정, 버그 해결에서 사용자가 명시한 동작 경로만 작업 범위로 본다.
- `여기`, `현재 사이트`, `이 버튼`, `Item 클릭`처럼 현재 UI 동작을 가리키는 요청은 해당 UI 이벤트에서 직접 도달하는 생산자·handler·consumer 경로로 한정한다.
- 사용자가 이름을 말하지 않은 Bridge, background 작업, 외부 자동화, batch, queue, 다른 입력 경로, 공유 integration은 기존 기능이 비슷하거나 helper를 재사용할 수 있다는 이유만으로 수정하지 않는다.
- 구현 전에 내부 체크리스트에 `허용된 시작 동작`, `허용된 최종 효과`, `수정 금지 인접 경로`를 각각 기록한다.
- 요청을 가장 좁은 호출 지점에서 구현할 수 있으면 그 호출 지점에서만 처리한다. 공유 helper 변경으로 둘 이상의 workflow 동작이 달라지게 만들지 않는다.
- 합리적인 해석이 둘 이상이고 선택에 따라 서로 다른 workflow가 변경될 수 있으면 파일 수정 전에 사용자에게 범위를 질문한다.
- 테스트에는 요청 경로의 기대 결과뿐 아니라 이름이 언급되지 않은 인접 workflow의 payload, 저장 설정, 부작용이 변경되지 않았다는 negative-scope 회귀 검사를 포함한다.
- 작업 중 명시되지 않은 경로가 변경된 사실을 발견하면 완료 처리하지 말고 같은 작업에서 해당 변경을 되돌리고 경계 검사를 추가한다.
