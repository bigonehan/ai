# 전역 규칙

이 파일의 모든 항목은 필수다. "필요할 때만", "조건부"로 해석하지 않는다.
우선순위: 사용자 명시 요구 → 가장 가까운 프로젝트 `AGENTS.md` → 이 파일 → 선택된 Skill. 하위 항목이 상위 요구를 넓히거나 바꾸지 않는다.

## 1. 응답

- 사용자 대면 응답(진행 안내·질문·최종 보고)은 한국어 `-요`체로 쓴다. 코드·명령·경로·식별자는 원문 그대로 둔다. 사용자의 욕설·반말을 따라 하지 않는다.
- final 전에 `맞습니다`, `맞아요`, `인식했습니다`, `알겠습니다`, `그렇습니다`가 있으면 그 문장을 다시 쓴다.
- 지적을 받으면 사과·수긍 없이 `실행한 수정 → 검증 결과 → 남은 경계` 순서로 보고한다.
- 사용자에게 넘기는 동작은 로그인·인증, 결제·비용, 비가역 외부 전송, 브라우저 밖 OS 앱 조작뿐이다.
- nf는 메인만 final·질문·동작 요청 전에 1회 실행한다(세부 절차는 `work-project`).
- "앞으로" 지적은 현재 작업만 고치지 않고 전역 규칙에도 반영한다.

## 2. 작업 절차

- `/home/tree/extensions` 작업: 초기화 전 `initialize-project`, 이후 `work-project`.
- UI·레이아웃 작업: `implement-ui`.
- 테스트 작성·실행·완료 주장: `verify-work`. 값·경로·인코딩 디버그: `verify-work`의 경계 절차.
- dev 브랜치 이전을 명시한 경우: `/home/tree/ai/skills/move-branch`.
- 선택된 Skill 절차를 생략·대체하지 않는다.
- 외부 API 로그를 남기거나 호출자를 추적할 때: `record-log`.




## 7. 기본값

- 제품 동작을 바꾼 개선판마다 patch 버전을 `0.0.1` 올린다(세그먼트 `00`~`99`). 재현·조사·재배포만 했으면 올리지 않는다.
- dev 서버(`npm run dev` 등)는 사용자가 요청할 때만 실행한다.
- 제품 `.py`는 수정 전 경로·이유를 알리고 동의를 받는다(사용자 지목·진행 지시·승인 도구 예외).
- 로그에 토큰·Authorization·원문 payload·개인 전체 경로를 남기지 않는다.
- 검증·API 탐색이 5분 넘으면 중간 로깅으로 병목 경계를 찾아 소스를 고친다(타임아웃 금지).

## 8. 규칙·설정 변경

- 전역 규칙은 `~/ai/rules/Agents.md`로만 관리하고 생성본(`~/.codex/AGENTS.md`·`~/.claude/CLAUDE.md`·`~/.gemini/GEMINI.md`·`~/.gemini/antigravity-cli/GEMINI.md`)은 수정 금지다.
- 전역 규칙을 읽거나 고칠 때(사용자의 "전역규칙/전역설정"·"앞으로" 포함)는 `change-rules` Skill을 따른다. 일반 "AGENTS.md 수정"은 가장 가까운 프로젝트 `AGENTS.md`만 바꾼다.
- 스크립트는 Rust(`.rs`)로만 만들고(`stagehand.mjs` 예외) 기존 `.py`·`.mjs`는 지운다. Skill·hook을 만들거나 고칠 때는 `create-skill` Skill을 따른다.
- `jj` 전역설정은 workspace 초안·테스트·보고까지만 하고, 사용자 전역설정 병합은 사용자가 위임한 경우에만 한다.
- 2번 넘게 남은 오류나 구조적 특성은 프로젝트 `AGENTS.md` `# tip`에 한 줄로 추가한다.
- 프로젝트 전용 규칙은 그 프로젝트 `AGENTS.md`에 둔다.

## 9. 코드·로그 읽기

- `/home/tree/extensions` 프로젝트에서는 파일 읽기·검색 전 첫 명령으로 `~/ai/scripts/bin/read-architecture --project <dir> --query "<요청 키워드>"`를 실행한다(다른 프로젝트로 옮기면 다시 실행). 출력의 `.ai/docs/architecture.md`(도메인·폴더 구조)로 대상 폴더를 고르고 `ls -R`·`find`로 구조를 다시 찾지 않는다. exit 2면 문서부터 작성한다.
- 검색은 `rg`만 쓴다(`grep -r`, `find | xargs grep` 금지). `rg -n "<패턴>" <dir>`(`-g`·`-t`·`-l`·`-m`)로 위치를 찾고 그 주변 범위만 읽는다.
- 파일·문서·객체 전체 읽기나 `console.log`·CLI 전체 덤프를 금하고 필요한 줄·필드만 떼어 다루며(`rg -n`, `grist task-show --section`), 로그는 `rg -n "FAIL|Error|warn"`·`| tail -n 40`·`| head -c 4000`으로 걸러 읽은 범위는 재읽기하지 않는다.
