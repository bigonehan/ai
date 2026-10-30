# 작업·교정 단계 (`manage-project` — Work, 대행자)

대행자는 위임 메시지(`판정`·`명령`·`진입점`·`제외`, `dispatch-agent`가 붙인 `[Role] row=`·`row:` 줄·목표, record-log ID, 수정 허용 경로)대로 구현만 한다. 행의 단계·소유 프로젝트·요청 범위를 다시 판정하지 않고, 요청·목표를 쓰거나 다른 행·에이전트를 다루지 않는다. 진입점이 실제 코드와 다르면 고치지 않고 차이를 마지막 줄 한 줄로 보고하고 끝낸다.

- UI·레이아웃 변경이 있으면 `implement-ui`, 테스트를 새로 쓰면 `verify-work` 경계 규칙을 함께 따른다.
- 행은 프롬프트의 `row:` 줄, 목표는 `목표:` 목록으로 읽고 Grist를 다시 조회하지 않는다(없을 때만 `grist task <id>`·`grist goal <id> --project <dir>` 1회). `.ai/changes`를 열거나 쓰지 않는다.
- 각 명령의 필수 인자·형식은 `~/ai/scripts/bin/grist <명령> --help`로 본다.

## 1. 실행 순서

1. **게이트 열기 (`$H=작업`)** — `row:` 줄에 원인(`$J`)이 있거나 메시지에 진입점이 있으면 `task-ready`, 없으면 `task-red`로 실패를 재현한다. `--test`는 메시지의 `명령` 줄이다. `--paths`는 `.`이 아닌 프로젝트 기준 최상위 경로(쉼표 구분)다.
   ```bash
   ~/ai/scripts/bin/grist task-ready <id> --project <dir> --purpose "<라벨>" --test "<메시지의 명령>" --tests <테스트 파일> --paths <최상위 경로들> --agent-id <ID>
   ~/ai/scripts/bin/grist task-red   <id> --project <dir> --purpose "<라벨>" --test "<명령>" --tests <테스트 파일> --paths <최상위 경로들> --agent-id <ID>
   ```
2. **구현과 집중 테스트 (`$H=교정`)** — `grist task-update <id> --project <dir> --fields '{"H": "교정"}'` 뒤 구현한다.
   - 입력에서 결과를 계산한다(기대값 하드코딩 금지). 같은 로직의 활성 분기(신규 생성·기존 읽기 등)에 모두 적용한다.
   - 테스트는 판정에 필요한 핵심 필드·한 줄 결과만 출력한다. 실제 런타임을 가짜로 꾸민 Mock으로 흐름 검증을 대신하지 않는다(흐름은 검증자 몫).
   - 외부 호출 경계에는 `record-log` 형식 한 줄(`ID | 함수 | 시도 | ack=… body.<필드>=… error=…`, 받은 ID)을 프로젝트 `.ai/logs/YYMMDD.log` 또는 기존 개발 로그에 남기고, 실패 경계마다 오류 문구가 남고 자격 정보가 빠지는지 테스트한다. ID를 받지 못했으면 구현 전에 질문으로 끝낸다.
   - 목표 줄마다 `grist task-step <id> --project <dir> --step "▶" --text "<구현·테스트 내역>" --goal <N>`.
3. **제출 (`$H=검증`)** — `grist task-submit <id> --project <dir>`. 실패 출력의 경계가 다음 수리 대상이다.
4. **끝내기** — `$H`·테스트 결과는 `task-submit`이 Grist에 기록하므로 마지막 줄에 다시 쓰지 않는다. 마지막 줄은 Grist에 없는 것(질문·진입점 차이·병목 원인과 수정)만, 없으면 `제출`. 버전 올리기·빌드·배포·사용자 질문·`확인`/`완료` 기록은 하지 않는다.

## 2. 막혔을 때

- 테스트·구현 중 원인을 모르는 실패를 만나면 `grist task-update <id> --project <dir> --fields '{"H": "진단", "I": "<관측 실패 한 줄>", "D": "[<오류 기호>] <점검 한 줄>"}'`로 기록하고 재현으로 원인을 찾는다. 원인은 `--fields '{"J": "[<오류 기호>] <원인> : {<에러 Body>}"}'`로 쓰고 그 출력 `row` 줄의 경계를 고친다. 오류 기호는 `~/ai/rules/rule_schema.md` 5절을 따른다.
- 재현 모드(메시지가 "재현"): `task-red`로 재현하고 `$J`만 기록한 뒤 소스를 고치지 않고 끝낸다.
- 사용자 동작 경로의 조기 반환·비동기 예외는 로그나 알림을 남긴다. 로그에 토큰·인증 헤더·원문 개인 payload·개인 전체 경로를 남기지 않는다.
- 이름 변경·레거시 제거는 `remove-legacy`를 따른다(이전 이름·alias를 남기지 않고, 깨진 외부 참조를 새 이름으로 고친다).
