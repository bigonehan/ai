---
name: remove-legacy
description: Remove a user-specified legacy format, path, gate, or workflow completely across its active owners, then have a different agent independently audit for remnants. Use for Legacy·레거시·구형 제거 또는 새 형식 갱신 요청.
metadata:
  short-description: Remove obsolete behavior and independently audit active references
---

# Remove Legacy

사용자가 Legacy·레거시·구형 방식의 제거 또는 새 형식으로 갱신을 요구하면 이 Skill을 `work-project`와 함께 반드시 실행한다. 사용자가 지정한 새 입력 형식과 최종 효과를 작업의 기준으로 삼는다. 단순히 옛 문자열을 금지하는 검사나 하위 호환 분기를 추가하는 것으로 완료하지 않는다.

## 시작과 범위

1. `work-project`의 시작 절차에 따라 사용자 요청을 `Input.md`의 `# 요구 사항`에 한 줄로 요약하고, `# 세부 사항`에 `[📐|✒️|⌨️|🚿|✏️|🌍] 한 줄 요약`으로 실제 작업 단위와 새 형식의 생성 지점을 쓴다. 이미 답한 사항을 다시 묻지 않는다.
2. 새 형식의 `시작 동작 → producer → transport/handler → authoritative consumer → 최종 효과`를 한 줄로 정한다. 입력 형식을 지정했다면 producer의 실제 출력을 먼저 구현·확인한다.
3. 변경 전 `read-architecture`와 `rg -n`(파일 전체 읽기 금지, 필요한 섹션·줄·필드만 `rg -n` 또는 `grist task-show --section`으로 발췌)으로 소유 폴더(`src/`, `server/`, 해당 `~/ai/skills`, `~/ai/scripts`)를 먼저 고르고, 선택된 폴더만 대상으로 `rg -n`을 실행한다(`/home/tree/extensions` 루트 전체 스캔, `.ai/changes/` 이력 탐색, `git log`·`tail` 전체 뒤지기 금지; 제품 코드·테스트 수정은 `t26_<id>` 서브에이전트에 위임하며 메인 세션이 직접 수정하지 않는다).
   - 제품 소스(`src/`, `server/`) 안에 하드코딩된 Obsidian Vault·템플릿 상대/절대 경로(예: `구현/[타입]/YYMMDD.<이름>/...`)와 분기(신규 생성 및 `EEXIST` 기존 파일 읽기 분기 모두)를 대조해 실제 디스크 파일·`AGENTS.md` `# 상태`·Table7 `Obsidian_Id`와 일치시킨다.
   - 불필요한 해시·다이제스트 검증 및 해시 전용 로깅은 호환 유지하지 않고 제품 코드·테스트·Skill·스크립트에서 함께 제거한다.
   - 전역규칙은 `agent-rules`로만, Grist는 `access-grist`로만 읽고 쓴다. 관련된 등록·초기화 프로젝트만 다루며, 사용자가 보류한 미등록 프로젝트를 초기화하지 않는다.
4. 각 발견 항목을 **새 형식으로 이전**, **구형 전용이라 삭제**, **과거 사실로 보존** 중 하나로 판정한다. 현재 실행·판정에 쓰이는 구형 fallback, alias, 호환 검사, 옛 gate JSON·스냅샷·백업은 남기지 않는다. 과거 이력은 사실을 보존하되 활성 참조처럼 작동하지 않게 한다. 요구와 관계없는 형제 제품이나 workflow는 바꾸지 않는다.

## 구현과 확인

5. 소유 경계별로 새 형식을 구현하고 활성 참조를 한 번에 이전한다. 이전 이름을 다시 허용하는 호환 코드를 만들지 않는다. 변경된 producer 출력과 권위 있는 consumer 결과를 읽어 같은 입력이 새 형식으로 도착했는지 확인한다. 검증은 기존 테스트·CLI·실제 consumer readback을 사용하고, 옛 형식 금지 전용 validator·recorder·테스트 파일을 만들지 않는다.
6. 수정에 참여하지 않은 **다른 에이전트**를 새 컨텍스트(`fork_turns="none"`)로 호출한다. 새 계약과 검사 대상 폴더 경로(`src/`, `server/`, `~/ai/skills`, `~/ai/scripts` 등)만 전달하고 결론은 주지 않는다. 에이전트는 루트 전체나 `.ai/changes/` 이력을 스캔하지 않고 지정된 활성 소유 경로만 읽기 전용 `rg -n`으로 독립 검색하며, 발견한 활성 잔존 항목마다 경로와 근거를 보고한다. 그 에이전트가 수정하지 않는다.
7. 잔존 항목이 있으면 원 소유자가 고치고, **다시 다른 독립 검사**를 수행한다. 활성 잔존이 0이고 새 형식의 consumer readback이 일치할 때만 완료를 주장한다. 검색 제외 범위와 과거 이력 보존 항목은 최종 보고에 명시한다.

## 이 절차가 필요한 이유

- 과거에는 사용자가 지정한 입력 형식의 생성 함수를 고치기 전에 인접 프로젝트 초기화와 형식 검사를 진행해 핵심 요구가 늦게 구현됐다. 입력 producer를 첫 작업 단위로 고정한다.
- 한 문자열이나 경로만 고친 뒤 다른 Skill·전역규칙·등록 프로젝트 참조, 옛 gate·스냅샷을 남겼다. 변경 전 목록과 각 항목의 이전·삭제·보존 판정을 빠짐없이 처리한다.
- 자기 검색만으로 완료를 보고해 예상하지 못한 잔존 참조가 나중에 발견됐다. 구현에 참여하지 않은 별도 에이전트의 독립 재검사를 완료 조건으로 둔다.
- `Input.md` 기록을 건너뛰고 이미 답한 프로젝트 유형을 다시 물었다. 먼저 요구를 한 줄로 요약해 기록하고 보류한 미등록 프로젝트는 건드리지 않는다.
