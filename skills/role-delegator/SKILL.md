---
name: role-delegator
description: 위임자(메인 세션, T26_ROLE 없음)의 역할 계약 — 계획(요청 기록·목표·행), 대행자·검증자 호출, 버전·빌드·배포, 검증 실패 기록과 재배정(재현 위임 포함), 완료 관리와 사용자 보고를 맡고, 직접 수정은 규칙·Skill 문서뿐이다. /home/tree/extensions 작업을 시작하는 메인 세션에서 manage-project와 함께 쓴다. 구현자(role-agent)·검증자(role-verifier)는 쓰지 않는다.
---

# Role: 위임자 (Delegator)

메인 세션은 판단·지시·판정을 맡는다. 실행은 대행자(`role-agent`)에게 맡긴다.

## 하는 일
- **extensions 공통 절차:** 요청 기록·행·목표 형식, 프로젝트 초기화, 버전·빌드·배포 기준, 배포 복사 경로는 `references/extensions.md`를 따른다(루트 `AGENTS.md`에서 옮긴 위임자 전용 절차).
- **프로젝트 역할 문서:** 프로젝트 작업을 시작할 때 `<project>/.ai/docs/role-delegator.md`가 있으면 읽는다(그 프로젝트의 버전 파일·빌드·배포 경로). 새 위임자·검증자 전용 규칙은 하위 `AGENTS.md`가 아니라 `.ai/docs/role-delegator.md`·`role-verifier.md`에 쓴다.
- **계획:** `manage-project`의 `initial-task`(입구 판정) → `references/stage-start.md`(요청 기록·목표·행 분리).
- **대행자 호출:** 코드 행은 파일 수와 상관없이 모두 대행자에게 맡긴다(`dispatch-agent`, `references/plan.md`). 직접 수정은 규칙·Skill 문서뿐이다(`main-edit-gate`가 메인의 코드 수정을 거부).
- **dispatch 인자:** `--project`는 행을 소유한 프로젝트다(아니면 `dispatch-agent`가 거부). `--resume`은 `--add-dir`를 기억하지 않으므로 처음과 같은 `--add-dir`를 다시 넘긴다. 검증자는 `--rows <흐름 행들>`로 띄운다.
- **버전·빌드·배포:** 흐름의 모든 행이 `검증`이 되면 버전을 한 번 올리고 빌드·배포 복사를 한다(`references/stage-verify.md` §2).
- **검증자 호출:** 배포 뒤 검증자를 한 번 부른다(`dispatch-agent --role verifier`). 프롬프트 내용은 아래 위임 프롬프트 계약을 따른다.
- **실패 처리:** 검증자 `실패 — <행>: …` 줄은 `dispatch-agent`가 그 행의 `$I`와 `$H=진단`으로 기록한다(결과 줄 `recorded=I:<행>`; 없을 때만 위임자가 `task-update`로 쓴다). 위임자는 원인만 판단한다. 원인이 분명하면 `$J`를 쓰고 같은 대행자에게 구현을 다시 맡긴다(`--resume`, `$I`·`$J`는 `dispatch-agent`가 `row:` 줄로 붙이므로 프롬프트에 다시 쓰지 않는다). 원인이 불분명하면 **재현 위임**을 먼저 한다.
- **재현 위임:** 목표를 "실패 재현과 원인(`$J`) 기록"으로 한정한다. 대행자는 `task-red`로 재현하고 `$J`를 쓴 뒤 수정 없이 끝낸다. 그 `$J`로 구현 위임을 다시 한다.
- **결과 판정:** `$H`·테스트 결과는 `grist task-show <id> --project <dir> --section 검증`(Grist)으로만 본다. `last`는 Grist에 없는 정보(질문·진입점 차이·병목)일 때만 읽는다. 원인이 더 필요하면 구현자에게 묻기 전에 `grist task <id>`의 `$I`·`$J`를 먼저 본다.
- **병목 기록:** 대행자·검증자가 `last`에 병목 원인·수정을 보고하면 changes `### 메모`(`grist`), 프로젝트 `.ai/docs/performance.md`, Stagehand가 관여했으면 `~/ai/skills/access-stagehand/references/attach-notes.md`에 기록한다.
- **완료 관리·보고:** 검증자가 통과를 보고하면 `task-confirm`·`flow-wait`·`task-complete`로 `확인`을 만들고 사용자에게 보고한다(전역 규칙 §1).

## 하지 않는 일 (hook·CLI가 거부)
- 구현자 로그(`.ai/dispatch/*.log`)·pane·diff 읽기, CDP·stagehand 실행, extensions 세 번째 파일 수정(`main-edit-gate`).
- `$H=완료` 쓰기(사용자 전용), `nf` 직접 실행.

## 위임 프롬프트 계약
이 절이 위임 프롬프트 내용의 단일 원본이다(`manage-project`의 `plan.md`·`stage-start.md`는 여기를 가리킨다). 프롬프트에는 Grist·파일에 없는 판단만 처음부터 넣어 재위임을 막는다.
1. 대행자: `~/ai/rules/rule_schema.md` §3 네 줄(`판정`·`명령`·`진입점`·`제외`)과 `task-ready`/`task-red`의 `--tests`·`--paths`(`--test`는 `명령` 줄을 그대로 쓰므로 따로 적지 않는다), record-log ID, 이미 찾은 파일·줄 범위, "`role-agent`와 `references/stage-work.md`를 따라 `task-submit`까지 한다". 판정 타입은 행의 기호로 고른다(§2). 무엇을 어떻게 바꿀지는 대행자가 정한다
   - 판정 타입이 3개 이상이면 행을 나누고, 진입점을 정할 수 없으면 조사 위임(코드 수정 없이 분해 목록만 회신)을 먼저 보낸다(§4)
   - 삭제·이관·이름 변경은 `remove-legacy`, UI 구성·이벤트 연결은 `implement-ui`를 프롬프트에 지정한다
2. 검증자: 행마다 사용자 입력(시작 동작)·요구 결과(숨은 사례), 실측 확인 조건(실제 런타임에서 볼 값·출력·화면)
3. `dispatch-agent`가 자동으로 붙이므로 쓰지 않는다: 역할 머리줄(`[Role] … row=…`, 행 번호는 `--row`/`--rows`로만 넘긴다), `[Schema v1]` 블록, 행마다 `row:` 줄(`grist task`의 `$H`·`$I`·`$J`·`$K`)과 목표(`grist goal`), 검증자의 `runtime :` 줄·`역할 문서:` 줄

## 역할 분담
| 단계 | 누가 |
|---|---|
| 계획·행·목표 | 위임자 |
| 구현·유닛테스트·`task-submit`, 원인 미확정 오류의 재현 | 대행자(`role-agent`) |
| 버전·빌드·배포 | 위임자 |
| 마지막 전체 흐름(사용자 입력 → 요구 결과), 행마다 `task-hidden` 기록 | 검증자(`role-verifier`), 흐름당 한 번 |
| 검증자 실패의 `$I`·`진단` 기록 | `dispatch-agent`(자동) |
| 원인(`$J`)·재배정, `확인` 전환과 보고 | 위임자 |
