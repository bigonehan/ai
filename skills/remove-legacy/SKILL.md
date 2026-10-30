---
name: remove-legacy
description: Remove a user-specified legacy format, path, gate, or workflow completely across its active owners, then audit remnants with the deterministic `check-residue` command. Use for Legacy·레거시·구형 제거 또는 새 형식 갱신 요청.
metadata:
  short-description: Remove obsolete behavior and audit active references with check-residue
---

# Remove Legacy

사용자가 Legacy·레거시·구형 방식의 제거 또는 새 형식으로 갱신을 요구하면 이 Skill을 `manage-project`와 함께 반드시 실행한다. 사용자가 지정한 새 입력 형식과 최종 효과를 작업의 기준으로 삼는다. 단순히 옛 문자열을 금지하는 검사나 하위 호환 분기를 추가하는 것으로 완료하지 않는다.

## 시작과 범위

1. `manage-project`의 시작 절차에 따라 사용자 요청을 `grist request-record` 입력의 `# 요구 사항`에 한 줄로 요약하고(changes `# 요청` 절에 기록), `# 세부 사항`에 `[📐|✒️|⌨️|🚿|✏️|🌍] 한 줄 요약`으로 실제 작업 단위와 새 형식의 생성 지점을 쓴다. 이미 답한 사항을 다시 묻지 않는다.
2. 새 형식의 `시작 동작 → producer → transport/handler → authoritative consumer → 최종 효과`를 한 줄로 정한다. 입력 형식을 지정했다면 producer의 실제 출력을 먼저 구현·확인한다.
3. 변경 전 `read-architecture` 직후 현재 Table26 행의 `$H`·`$I`·`$J`·`$K`를 한 번 읽는다(이미 받은 `row` 줄이 있으면 그 줄, 없으면 `grist task <id>`). 그다음 `rg -n`(파일 전체 읽기 금지, 필요한 섹션·줄·필드만 `rg -n` 또는 `grist task-show --section`으로 발췌)으로 소유 폴더(`src/`, `server/`, 해당 `~/ai/skills`, `~/ai/scripts`)를 고르고, 선택된 폴더만 대상으로 `rg -n`을 실행한다(`/home/tree/extensions` 루트 전체 스캔, `.ai/changes/` 이력 탐색, `git log`·`tail` 전체 뒤지기 금지; 제품 코드·테스트 수정은 파일 수와 상관없이 대행자(`role-agent`, `dispatch-agent`)가 맡고 메인은 규칙·Skill 문서만 직접 고친다).
   - 제품 소스(`src/`, `server/`) 안에 하드코딩된 Obsidian Vault·템플릿 상대/절대 경로(예: `구현/[타입]/YYMMDD.<이름>/...`)와 분기(신규 생성 및 `EEXIST` 기존 파일 읽기 분기 모두)를 대조해 실제 디스크 파일·`AGENTS.md` `# 상태`·Table7 `Obsidian_Id`와 일치시킨다.
   - 불필요한 해시·다이제스트 검증 및 해시 전용 로깅은 호환 유지하지 않고 제품 코드·테스트·Skill·스크립트에서 함께 제거한다.
   - 전역규칙은 `agent-rules`로만, Grist는 `access-grist`로만 읽고 쓴다. 관련된 등록·초기화 프로젝트만 다루며, 사용자가 보류한 미등록 프로젝트를 초기화하지 않는다.
4. 각 발견 항목을 **새 형식으로 이전**, **구형 전용이라 삭제**, **과거 사실로 보존** 중 하나로 판정한다. 현재 실행·판정에 쓰이는 구형 fallback, alias, 호환 검사, 옛 gate JSON·스냅샷·백업은 남기지 않는다. 과거 이력은 사실을 보존하되 활성 참조처럼 작동하지 않게 한다. 요구와 관계없는 형제 제품이나 workflow는 바꾸지 않는다.

## 구현과 확인

5. 소유 경계별로 새 형식을 구현하고 활성 참조를 한 번에 이전한다. 이전 이름을 다시 허용하는 호환 코드를 만들지 않는다. 변경된 producer 출력과 권위 있는 consumer 결과를 읽어 같은 입력이 새 형식으로 도착했는지 확인한다. 검증은 기존 테스트·CLI·실제 consumer readback을 사용하고, 옛 형식 금지 전용 validator·recorder·테스트 파일을 만들지 않는다.
6. 잔존 검사는 에이전트를 부르지 않고 `~/ai/skills/remove-legacy/scripts/bin/check-residue --paths <활성 소유 경로,…> --pattern <옛 이름·경로 정규식> [--pattern …] [--exclude <glob>]`로 한다. 검사 경로는 `src/`, `server/`, 해당 `~/ai/skills`, `~/ai/scripts` 같은 활성 소유 경로만 넣고(루트 전체 금지), 패턴은 4번에서 이전·삭제로 판정한 옛 이름·경로·형식 전부를 넣는다. `target`·`.git`·`node_modules`·`.ai/changes`·`.ai/dispatch`는 자동 제외되며, 과거 사실로 보존한 경로만 `--exclude`로 더 뺀다(절대 경로 검사에서도 걸리도록 `**/.ai/docs/**`처럼 `**/`로 시작한다). 출력은 패턴별 개수와 `경로:줄: 내용`, `total`뿐이고 exit 0이 잔존 0, 1이 잔존 있음이다.
7. exit 1이면 출력된 줄마다 이전·삭제·보존을 다시 판정해 원 소유자가 고치고 같은 명령을 다시 실행한다. exit 0이고 새 형식의 consumer readback이 일치할 때만 완료를 주장한다. 실행한 명령(경로·패턴·제외)과 과거 이력 보존 항목은 최종 보고에 명시한다.

## 이 절차가 필요한 이유

- 과거에는 사용자가 지정한 입력 형식의 생성 함수를 고치기 전에 인접 프로젝트 초기화와 형식 검사를 진행해 핵심 요구가 늦게 구현됐다. 입력 producer를 첫 작업 단위로 고정한다.
- 한 문자열이나 경로만 고친 뒤 다른 Skill·전역규칙·등록 프로젝트 참조, 옛 gate·스냅샷을 남겼다. 변경 전 목록과 각 항목의 이전·삭제·보존 판정을 빠짐없이 처리한다.
- 자기 검색만으로 완료를 보고해 예상하지 못한 잔존 참조가 나중에 발견됐다. 4번 판정의 옛 패턴 전부를 `check-residue`로 같은 범위에서 검사하고 exit 0을 완료 조건으로 둔다(별도 에이전트 재검사는 1회 약 1만8천 토큰이 들어 명령으로 바꿨다).
- 요청 기록(changes `# 요청`)을 건너뛰고 이미 답한 프로젝트 유형을 다시 물었다. 먼저 요구를 한 줄로 요약해 기록하고 보류한 미등록 프로젝트는 건드리지 않는다.
