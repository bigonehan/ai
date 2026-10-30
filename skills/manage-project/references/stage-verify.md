# 검증→확인 단계 (`manage-project` — Verify & Complete, 위임자)

흐름의 모든 행이 `$H=검증`이면 위임자는 아래 순서만 실행한다. 흐름 실행·숨긴 사례 기록·CDP·병목 로깅은 검증자(`role-verifier`) 몫이며 이 카드에 없다. 규칙·Skill 문서만 바꾼 행은 검증자 없이 `manage-global-rules` §4 예외로 마감한다.

## 마감 명령 순서 (필수 인자 포함)

| 순서 | 명령 | 필수 형식 |
|---|---|---|
| 1 | 버전·빌드·배포(아래 §1) | 제품 동작이 바뀐 흐름마다 patch +0.0.1 |
| 2 | `dispatch-agent --agent codex --role verifier --project <dir> --row <id> --rows <id,…> --prompt-file <F>` | 프롬프트: 행마다 사용자 입력(시작 동작)과 요구 결과(`runtime :`·`row:` 줄은 `dispatch-agent`가 붙임) |
| 3 | (검증자) `grist task-hidden <id> --project <dir> --purpose "<라벨>" --test "<재실행 명령>"` | 행마다 1개 이상. `task-confirm`보다 먼저 |
| 4 | `grist task-confirm <id> --project <dir> --summary "[기호] <입력> ⇒ <결과>" --check "<라벨>" --detail "<함수 ⇒ 대상 ▷ 값>" [--can "<사용자가 …할 수 있어요>"] --evidence /dev/stdin` | `유형 : 기능 추가`면 `--can` 필수(`수 있` 한 줄). evidence는 `grist goal <id>` 목표 줄 수와 같은 `N. <실제 관측값>` 줄 |
| 5 | `grist flow-wait --project <dir>` | 숨긴 사례 뒤 최신 `#### 확인`이 있어야 통과(`task-hidden`을 다시 하면 4를 다시) |
| 6 | `task-complete --row <id> --project <dir>` | 행마다. 통과하면 `$H=확인` |

- `$D`를 쓸 때(`grist task-update <id> --project <dir> --fields '{"D": "..."}'`)는 `[🅰️|🗝|❌|⭕️|🔢|🛣|🕧|👥|🏷|🚥|🔳] <한 줄>` 형식이다.
- 근거에는 실제로 실행한 명령·숨긴 사례·화면 값만 쓴다. 단위 테스트로만 확인한 값은 `(단위 테스트)`라고 밝힌다. J1은 근거가 목표를 덮는지만 보며 숨긴 사례·흐름 확인을 대신하지 않는다.
- 기록은 `grist task-show <id> --project <dir> --section 검증|확인`으로만 읽고 `.ai/changes`를 열거나 쓰지 않는다.
- 사용자 전용 동작(로그인·인증, 결제·비용, 비가역 외부 전송, 브라우저 밖 OS 앱 조작)만 남으면 `$D`에 사유를 쓰고 `AskUserQuestion`으로 단일 동작을 묻는다. 브라우저 클릭·확장 재로드·Settings `Test`는 넘기지 않는다.
- 검증자가 `실패 — <행>: …`로 끝나면 `dispatch-agent`가 그 행의 `$I`·`$H=진단`을 기록한다(결과 줄 `recorded=I:<행>`, 없을 때만 위임자가 `task-update`). 위임자는 `stage-start.md` 진단 갈림길에서 `$J`만 판단한다. 재위임은 `plan.md` §3을 따른다.

## 1. 단일 릴리스 및 배포 복사 (위임자)

- 모든 행이 `$H=검증`이 된 뒤 검증자 호출 전에 한 번만 한다.
- 제품 동작이 바뀌면 patch를 `0.0.1` 올리고 버전 파일(`package.json`, `manifest.json`, `public/manifest.json`, `runtime-contract.js`, `Cargo.toml` 등)과 빌드 산출물 버전을 맞춘다.
- 실행 가능한 산출물을 `/mnt/h/code/<프로젝트 디렉터리명>`(Obsidian 플러그인은 `.obsidian/plugins/<프로젝트명>`)에 복사한다. 공용 CDP에 새 빌드를 올리는 `run_cdp --replace`는 검증자의 시작 단계다.

## 2. 완료 판정 (`Complete_task`)

- 전역 규칙·Skill 절차 텍스트 행의 제출 예외는 `manage-global-rules` §4를 따른다.
- `task-complete`가 목표와 결과를 대조해 `### 구현`에서 실제 작업만 추린 `#### 완료 (#<id>)` 목록을 만들고 `# 완료`로 옮긴다. 완료 기호는 구조 `🏗️`, 기능 `✨`, 외부 연결 `🌍`, 누락 수정 `⭕️`, 속도·흐름 `🚀`, 오류 수정은 `~/ai/rules/rule_schema.md` 5절의 원인 기호다.
- 메인 세션은 현재 작업의 모든 행이 `# 완료`에 있고 `$H=확인`(또는 사용자 설정 `완료`)일 때 종료한다. 사용자의 "완료" 답변에 `task-complete`를 다시 돌려 `$H=완료`로 바꾸지 않는다.
