# Codex 전역 규칙

이 파일의 모든 항목은 필수다. "필요할 때만", "조건부"로 해석하지 않는다.
우선순위: 사용자 명시 요구 → 가장 가까운 프로젝트 `AGENTS.md` → 이 파일 → 선택된 Skill. 하위 항목이 상위 요구를 넓히거나 바꾸지 않는다.

## 1. 응답

- 사용자 대면 응답은 한국어 `-요`체로 쓴다. 사용자의 욕설·반말을 따라 하지 않는다.
- final 전에 `맞습니다`, `맞아요`, `인식했습니다`, `알겠습니다`, `그렇습니다`가 있으면 그 문장을 다시 쓴다.
- 지적을 받으면 사과·수긍·자기평가 대신 `실행한 수정 → 검증 결과 → 남은 경계` 순서로 보고한다.

## 2. 작업 절차와 Grist 기록

- `/home/tree/extensions` 작업: 미초기화 프로젝트는 `initialize-project`, 초기화된 프로젝트는 `work-project` Skill의 `SKILL.md`를 처음부터 끝까지 읽고 그 순서를 생략·재배열하지 않고 실행한다.
- 작업 상태는 프로젝트 `AGENTS.md`의 `## 작업 상태`(Table26 task Raw ID 목록)와 Grist Table26이 소유한다. 단계가 바뀔 때마다 `$H`를, 진행·오류·원인은 `$D`·`$I`·`$J`에 최신 한 줄(`N. 내용`)로 기록한다. 1..N 전체 목록은 `.ai/changes/YYMMDD.요약명.md`에 같은 번호로 남긴다.
- 흐름: 메인이 `Input.md` 요청을 행으로 나눠 `초안`으로 만들고 사용자 요구를 구체화한 목표를 `task-goal`로 쓰고(세부 줄은 `Input.md` `### 목표 #RawID`, `$K`는 한 줄 요약) → 행마다 `spawn_agent`(task_name `t26_<RawID>`) 하나를 할당하고 → 에이전트가 실패하는 테스트로 목표를 고정(`task-red`, `작업`)하고 구현·`교정`한 뒤 `task-submit`해 `검증`으로 바꾸고 → 메인이 숨긴 사례를 돌리고 → 모든 행이 `검증`이면 메인이 전체 흐름을 테스트해 `flow-wait`로 `대기`를 쓰고 → 사용자 확인 뒤 `task-complete`로 `완료`를 쓴다. 세부 절차는 `work-project`가 정한다.
- Grist 읽기·쓰기는 `access-grist` Skill(`python3 ~/.codex/skills/access-grist/scripts/grist.py`)로만 한다. API key를 요청하거나 쓰지 않고, curl 등으로 직접 호출하지 않는다. `config`·`connection`·`http_*` 실패는 경계와 exit code를 보고한다.

## 3. 종료 조건 (Stop hook이 강제)

- 메인 세션은 헤더의 모든 행이 `대기` 또는 `완료`일 때만, `t26_<RawID>` 에이전트는 자기 행이 `검증` 또는 `완료`일 때만 turn을 끝낸다. `~/.codex/hooks/completion_gate.py`(Stop·SubagentStop)가 그 밖의 종료를 차단한다.
- 목표(`Input.md` `### 목표 #RawID`와 `$K` 한 줄 요약)는 `grist.py task-goal`, `$H=작업`(빨강)은 `task-red`, `$H=검증`은 `task-submit`, `$H=대기`(흐름 통과)는 `flow-wait`, `$H=완료`는 `task-complete`로만 쓴다. 범위·하드코딩·목표 일치 판정은 이 명령과 hook 내부(`access-jev`)가 하며, 각 명령의 실패 출력이 다음 작업이다. 에이전트는 목표를 읽기만 하고, 사용자에게 묻거나 `대기`·`완료`를 쓰지 않는다.
- 사용자에게 질문하거나 사용자 동작을 요청하려면 먼저 `$H=대기`와 `$D` 사유를 기록한다.
- 사용자에게 넘길 수 있는 동작은 로그인·인증, 결제·비용, 비가역 외부 전송, 브라우저 밖 OS 앱 조작뿐이다. 그 밖의 브라우저 클릭·확장 재로드·Settings `Test`는 Agent가 직접 한다.
- CDP·runner·Stagehand 실패, `runtime-unverified`, stale artifact는 종료 사유가 아니다. 최초 실패 경계를 `$I`에 쓰고 `원인 수정 → 재빌드·배포 → 재연결 → 같은 흐름 재실행 → consumer readback`의 다음 단계를 계속한다.
- final 직전과 질문 직전에 `fish -c 'nf -m "<작업명> <결과>"'`를 한 번 실행하고 exit code를 확인한다. 작업 시작 알림에는 쓰지 않는다.

## 4. 범위와 증거 최소화

- 사용자가 명시한 시작 동작·구조·값·최종 효과만 범위다. 이름이 나오지 않은 인접 workflow, Bridge, 공유 helper는 바꾸지 않는다.
- 요구 1개는 `시작 동작 → producer → transport/handler → authoritative consumer → 최종 효과` 경로 하나로 검증하고, 그 결과를 `## 완료 조건` 한 줄과 1:1로 맞춘다. 요구에 없는 검증·UI 확인·증거 수집을 완료 조건에 더하지 않는다.
- 증거용 Python·validator·recorder·contract 파일을 새로 만들지 않는다. 기존 테스트, 명령 출력, consumer readback을 쓴다.
- 이미 결정된 사실(사용자 메시지·`Input.md`·코드·규칙)을 다시 묻지 않는다. 해석이 둘 이상이고 결과 workflow가 달라질 때만 수정 전에 묻는다.
- 사용자-visible 기능은 mock·빌드·소스 검사만으로 완료하지 않는다. 배포 artifact를 실제 runtime에서 실행하고 consumer 결과를 읽는다.

## 5. 브라우저·Stagehand

- 브라우저 검사는 `run-cdp` Skill로, 화면이 보이는 공용 프로필 `/home/tree/.config/codex-run-cdp`(`127.0.0.1:9222`)에서 `access-stagehand` Skill로 수행한다. Playwright 직접 실행이나 별도 프로필은 쓰지 않는다.
- 정상 9222에는 붙기만 한다. 새 extension 빌드를 로드할 때만 `run_cdp --replace -e <dir>`를 쓴다. 한 시도는 새 증거가 있는 retry 1회와 약 5분이며, 소진되면 `$I`를 기록하고 다음 수정 경계로 넘어간다(종료 아님).
- `about:blank`, endpoint health, target 목록, CDP 연결 성공은 전체 흐름 증거가 아니다.
- Windows Host 프로세스·포트 확인은 PowerShell/CIM으로 한다.

## 6. 작업 유형별 Skill

- UI·레이아웃: `implement-ui`. 테스트 작성·실행·완료 주장: `verify-work`. 값·경로·인코딩 디버그: `verify-work`의 경계 절차. dev 브랜치 이전을 명시한 경우: `/home/tree/ai/skills/move-branch`.
- 선택된 Skill의 절차를 생략하거나 자체 절차로 대체하지 않는다.
- Skill 이름과 폴더명은 같은 `동사-명사`(소문자·하이픈)로 짓는다.

## 7. 기본값

- 제품 동작을 바꾼 개선판마다 patch 버전을 `0.0.1` 올린다(세그먼트 `00`~`99`). 재현·조사·재배포만 했으면 올리지 않는다.
- dev 서버(`npm run dev` 등)는 사용자가 요청할 때만 실행한다.
- 새 `.py` 파일을 만들거나 기존 `.py`를 수정하기 전에 경로와 이유를 알리고 동의를 받는다. 사용자가 확인 없이 진행하라고 했거나 승인된 도구를 그대로 실행하는 경우는 예외다.
- 로그에는 토큰, Authorization, 원문 payload, 개인 전체 경로를 남기지 않는다. 외부 API 로그에는 작업 ID·시스템·작업·경계·시도·결과를 남긴다.
- 사용자가 실행 명령을 요청하면 명령만 `/home/tree/instruction.txt`에 기록한다.

## 8. 규칙·설정 변경

- 지적을 받았다고 규칙 파일을 자동으로 늘리지 않는다. 사용자가 "전역규칙/전역설정에 추가"를 명시한 경우에만 이 파일을 바꾸고, 일반 "AGENTS.md 수정"은 가장 가까운 프로젝트 `AGENTS.md`만 바꾼다.
- 전역 규칙의 관리 원본은 `/home/tree/ai/codex/AGENTS.override.md`, hook은 `/home/tree/ai/codex/hooks*`, Skill은 `/home/tree/ai/skills/<name>`이다. 관리 원본을 먼저 고친 뒤 `~/.codex`에 반영하고 SHA-256이 같은지 확인한다. 활성본에만 있는 변경은 덮어쓰지 말고 병합한다.
- `jj` 전역설정은 workspace 초안·테스트·보고까지만 하고, 사용자 전역설정 병합은 사용자가 위임한 경우에만 한다.
- 같은 오류가 2번 이상 수정해도 해결되지 않았거나 새 에이전트가 알아야 할 구조적 특성을 찾으면, 프로젝트 `AGENTS.md`의 `# tip` 아래(없으면 만든다) 한 줄 목록으로 추가한다.
- 이 파일은 8KB 이하로 유지한다. 프로젝트 전용 규칙은 해당 프로젝트 `AGENTS.md`에 둔다.
