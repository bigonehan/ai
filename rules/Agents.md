# 전역 규칙

이 파일의 모든 항목은 필수다. "필요할 때만", "조건부"로 해석하지 않는다.
우선순위: 사용자 명시 요구 → 가장 가까운 프로젝트 `AGENTS.md` → 이 파일 → 선택된 Skill. 하위 항목이 상위 요구를 넓히거나 바꾸지 않는다.

## 1. 응답

- 사용자 대면 응답(진행 안내·질문·최종 보고)은 한국어 `-요`체로 쓴다. 코드·명령·경로·식별자는 원문 그대로 둔다. 사용자의 욕설·반말을 따라 하지 않는다.
- final 전에 `맞습니다`, `맞아요`, `인식했습니다`, `알겠습니다`, `그렇습니다`가 있으면 그 문장을 다시 쓴다.
- 지적을 받으면 사과·수긍·자기평가 대신 `실행한 수정 → 검증 결과 → 남은 경계` 순서로 보고한다.
- 사용자에게 넘기는 동작은 로그인·인증, 결제·비용, 비가역 외부 전송, 브라우저 밖 OS 앱 조작뿐이고, 브라우저 클릭·확장 재로드·Settings `Test`는 직접 한다.
- final 직전과 질문·사용자 동작 요청 직전에 `fish -c 'nf -m "<작업명> <결과·요청 요약>"'`를 한 번 실행하고 exit code를 확인한다. 작업 시작 알림에는 쓰지 않는다.

## 2. 작업 절차

- `/home/tree/extensions` 작업은 미초기화면 `initialize-project`, 초기화됐으면 `work-project` Skill의 `SKILL.md`를 끝까지 읽고 그 순서대로 실행한다. 작업 상태·Grist 기록·흐름은 그 Skill이 정한다.

## 3. 범위와 증거 최소화

- 사용자가 명시한 시작 동작·구조·값·최종 효과만 범위다. 이름이 나오지 않은 인접 workflow, Bridge, 공유 helper는 바꾸지 않는다.
- 요구 1개는 `시작 동작 → producer → transport/handler → authoritative consumer → 최종 효과` 경로 하나로 검증해 완료 조건 한 줄과 맞추고, 요구에 없는 검증·증거를 더하지 않는다.
- 증거용 Python·validator·recorder·contract 파일을 새로 만들지 않는다. 기존 테스트, 명령 출력, consumer readback을 쓴다.
- 이미 정해진 사실(사용자 메시지·`Input.md`·코드·규칙)은 다시 묻지 않고, 해석이 둘 이상이라 결과 workflow가 달라질 때만 수정 전에 묻는다.
- 사용자-visible 기능은 mock·빌드·소스 검사로 완료하지 않고, 배포 artifact를 실제 runtime에서 실행해 consumer 결과를 읽는다.

## 4. 브라우저·Stagehand

- 모든 브라우저 검사(웹·Grist 위젯·확장·외부 사이트)는 `run-cdp` Skill로 화면이 보이는 공용 프로필 `/home/tree/.config/codex-run-cdp`(`127.0.0.1:9222`)에서 `access-stagehand`로 한다. Playwright·프로젝트별 프로필·Windows Chrome 디버깅 프로필은 쓰지 않고, 공용 프로필에 로그인된 사이트는 다시 묻지 않는다.
- Stagehand는 `access-stagehand`(`scripts/stagehand.mjs`·`openStagehand()`)로만 쓴다. 모델 키를 묻거나 기록하지 않고, 제품 프로젝트에 설치하거나 새 프로필을 만들지 않는다.
- 정상 9222에는 붙기만 하고, 새 extension 빌드를 로드할 때만 `run_cdp --replace -e <dir>`를 쓴다. 한 시도는 새 증거가 있는 retry 1회·약 5분이며, 소진되면 `$I`를 기록하고 다음 수정 경계로 간다(종료 아님).
- `about:blank`, endpoint health, target 목록, CDP 연결 성공은 전체 흐름 증거가 아니다. Windows Host 프로세스·포트 확인은 PowerShell/CIM으로 한다.

## 5. Grist

- Grist `au1EbKLtZbrW`(Table7·Table26·Table27)는 `access-grist`(`~/ai/scripts/bin/grist`)로만 읽고 쓰며, 경계 실패는 우회 없이 경계·exit code만 보고한다(키 질문·REST 직접 호출 금지, 제품 자체 Grist 예외).
- Table26 `$D`·`$I`·`$J`는 최신 한 줄만 쓰고, 상세는 `.ai/changes/YYMMDD.요약명.md`의 `## [구현] <소스명 뺀 제목> (#id)` 아래 `### 목표`(`1. [연결|기능|수정] ...`)·`### 메모`·`### 오류`·`### 원인`(`1. [유실|문자|인증] 요약 : {에러 Body}`)·`### 구현`(`- [▷|▶|○|●] 내역`, 리턴 `▷`·전달 `⇒`, gate `(#id)`)에 둔다.

## 6. 작업 유형별 Skill

- UI·레이아웃: `implement-ui`. 테스트 작성·실행·완료 주장: `verify-work`. 값·경로·인코딩 디버그: `verify-work`의 경계 절차. dev 브랜치 이전을 명시한 경우: `/home/tree/ai/skills/move-branch`.
- 선택된 Skill 절차를 생략·대체하지 않는다. Skill 이름·폴더명은 `동사-명사`(예: `access-grist`)로 짓고, 검증·API 탐색이 5분 넘으면 중간 로깅으로 병목 경계를 찾아 소스를 고친다(타임아웃 금지).

## 7. 기본값

- 제품 동작을 바꾼 개선판마다 patch 버전을 `0.0.1` 올린다(세그먼트 `00`~`99`). 재현·조사·재배포만 했으면 올리지 않는다.
- dev 서버(`npm run dev` 등)는 사용자가 요청할 때만 실행한다.
- 제품 `.py`는 수정 전 경로·이유를 알리고 동의를 받는다(사용자 지목·진행 지시·승인 도구 예외).
- 로그에 토큰·Authorization·원문 payload·개인 전체 경로를 남기지 않고, 외부 API 로그에는 작업 ID·시스템·작업·경계·시도·결과를 남긴다.
- 실행 명령을 요청받으면 명령만 `/home/tree/instruction.txt`에 쓴다.

## 8. 규칙·설정 변경

- 전역 규칙 원본은 `~/ai/rules/Agents.md` 하나다. `~/.codex/AGENTS.md`·`~/.claude/CLAUDE.md`·`~/.gemini/GEMINI.md`·`~/.gemini/antigravity-cli/GEMINI.md`는 생성본이라 직접 고치지 않는다.
- 사용자가 전역 규칙을 말하면 `~/.local/bin/agent-rules`로만 읽고(`read [--section <제목>]`·`sections`) 고친다(`add --section <제목> --text <줄>`·`replace --old <A> --new <B>`·`remove --text <A>`·`write --file <F>`). 수정 시 생성본이 자동 갱신되며 결과는 `agent-rules status`로 보고한다. `sync` exit 3이면 `~/ai/rules/.state/backup/` 내용을 원본에 병합하고 `sync --force`한다.
- 지적을 받았다고 규칙을 늘리지 않는다. "전역규칙/전역설정"을 명시한 경우에만 원본을 바꾸고, 일반 "AGENTS.md 수정"은 가장 가까운 프로젝트 `AGENTS.md`만 바꾼다.
- 스크립트는 `.py`·`.mjs`로 만들지 않고 기존 `.py`·`.mjs`는 지우며, 공용은 `~/ai/scripts/bin/`, Skill 전용은 `<Skill>/scripts/bin/`에 Rust(`.rs`)로만 둔다(`stagehand.mjs` 예외). hook `~/ai/codex/hooks*`·Skill `~/ai/skills/<name>`은 `~/.codex`·`~/.claude`에서 원본을 가리키는 심볼릭 링크로 묶어 원본만 고친다.
- `jj` 전역설정은 workspace 초안·테스트·보고까지만 하고, 사용자 전역설정 병합은 사용자가 위임한 경우에만 한다.
- 같은 오류가 2번 넘게 고쳐도 남거나 새 에이전트가 알아야 할 구조적 특성을 찾으면 프로젝트 `AGENTS.md` `# tip`(없으면 만든다)에 한 줄로 추가한다.
- 원본은 8KB 이하로 유지하고(`sync`가 경고), 프로젝트 전용 규칙은 그 프로젝트 `AGENTS.md`에 둔다.

## 9. 코드·로그 읽기

- `/home/tree/extensions` 프로젝트에서는 파일 읽기·검색 전 첫 명령으로 `~/ai/scripts/bin/read-architecture --project <dir> --query "<요청 키워드>"`를 실행한다(다른 프로젝트로 옮기면 다시 실행). exit 2면 문서부터 작성한다.
- 탐색 전에 `.ai/docs/architecture.md`(도메인·폴더 구조)로 대상 폴더를 고르고 `ls -R`·`find`·`grep -r`로 구조를 다시 찾지 않는다. 파일이 없으면 알리고 생성을 제안한다.
- 검색은 `rg`만 쓴다(`grep -r`, `find | xargs grep` 금지). `rg -n "<패턴>" <dir>`(`-g`·`-t`·`-l`·`-m`)로 위치를 찾고 그 주변 범위만 읽는다.
- 소스 전체나 여러 파일을 한 번에 `cat`하지 않고, 약 300줄이 넘으면 필요한 범위만 읽는다. 테스트·빌드·로그는 `rg -n "FAIL|Error|warn"`·`| tail -n 40`·`| head -c 4000`으로 걸러 보고, 읽은 범위는 다시 읽지 않는다.
