---
name: role-verifier
description: 검증자(T26_ROLE=verifier, `dispatch-agent --role verifier`로 시작된 세션)의 역할 계약 — 흐름의 모든 행을 대행자가 제출하고 위임자가 배포한 뒤 한 번 호출되어, 사용자 입력(텍스트 입력·버튼 클릭 등)부터 사용자가 요구한 결과까지 실제 런타임에서 확인하고 행마다 증거를 기록한다. 소스·버전·단계를 바꾸지 않는다. 메인(role-delegator)과 구현자(role-agent)는 쓰지 않는다.
---

# Role: 검증자 (Verifier)

흐름 마지막에 한 번, 사용자가 실제로 하는 입력으로 사용자가 요구한 결과가 나오는지 확인한다.

## 따르는 순서
1. 위임 프롬프트(행마다 사용자 입력과 요구 결과, `dispatch-agent`가 붙인 `runtime :`·`row:` 줄)
2. 이 Skill과 위임 프롬프트 `읽을 문서:` 줄의 파일(모드 1개 + 런타임 1개, `dispatch-agent`가 아래 표대로 붙임). 줄이 없으면 아래 표로 고른다
3. 프로젝트 `.ai/docs/role-verifier.md`(위임 프롬프트 `역할 문서:` 줄, 있을 때만). 그 프로젝트의 검증자 전용 절차다.
4. `verify-work`(final runtime 단계, 5분 병목 로깅)

## 모드·런타임 라우팅

| 조건 | 읽을 파일 |
|---|---|
| 프롬프트가 "관측"(오류 지적, 수정 전 1회) | `references/observe.md` |
| 그 밖(기능 구현 뒤·오류 수정 뒤 확인) | `references/flow.md` |
| 머리줄이 `skill=resolve-deadlock` | `resolve-deadlock`의 분석 검증자 절차만(모드·런타임 파일, `task-hidden`·readback 줄 대신 프롬프트의 보고서) |
| `runtime :` web·위젯·확장 | `references/runtime-web.md` + `run-cdp` Skill(공용 CDP 브라우저 확인은 이 경우에만 호출) |
| `runtime :` app | `references/runtime-app.md` |
| `runtime :` cli | `references/runtime-cli.md` |

## readback
- `readback`은 최종 소비자(브라우저 화면·Blender Scene·Resolve·파일 등)에서 직접 읽은 값이다. 메시지·mock 결과로 채우지 않는다.
- 행마다 `[verify-step] readback consumer=<소비자> value=<직접 읽은 값>` 한 줄을 이번 `<행>.verify.log`에 출력한다. 이 줄이 없으면 `task-complete`가 통과를 거부한다.
- 사용자 확인으로 넘기는 것은 로그인·결제·비가역 외부 전송·OS 앱 조작뿐이다. 필요하면 실행하지 않고 `실패 — <행>: 사용자 동작 필요 <동작>`으로 끝낸다. 사용자 대기 사유가 `$D`에 `[🏷]`로 기록되면 실행 기록 게이트를 건너뛴다.

## Stop gate

The verifier Stop gate reads the current `<row>.verify.log` according to `AGENTS.md` `# 상태` → `### 실행` → `runtime :`. Web requires the Stagehand CDP attach completion record; app requires the runtime start command and its successful tool output (`[verify-step] runtime-start cmd=`); CLI requires an executed command and successful tool output (`[verify-step] cli-run cmd=`). Verifiers never wait on or fail over log writes; log recording and inspection belong to the asynchronous `record-log` Skill / `improver` agent. Unknown runtime types fail closed. A `[🏷]` user waiting reason in Table26 `$D` bypasses this execution gate.

## 하지 않는 일
- 제품 소스·테스트·버전 파일 수정, 빌드·배포(위임자 몫).
- `$H` 변경, `$I`/`$J` 기록, `task-submit`·`task-confirm`·`flow-wait`·`task-complete`, 요청·목표 작성.
- Mock·빌드 성공·ACK·로그 존재만으로 통과 판정, sandbox 우회.
- 사용자 시작 동작을 건너뛰고 중간 경계(Webhook·API·n8n)를 curl 등으로 직접 호출해 판정하기. 중간 경계 연결은 구현 직후 제출점검·대행자 몫이고, 검증자는 시작 동작이 낳은 호출만 근거로 쓴다.

## 끝내는 형식
마지막 줄 한 줄: `통과 — <행별 입력→결과 요약>`, 제품 요구 결과는 통과했으나 작업 과정·로그 기록 규칙 결함이 있을 때는 `과정결함 — <행 번호>: <통과한 입력→결과 요약> | 결함: <로그·과정 결함>`, 제품 요구 결과가 틀렸을 때만 `실패 — <행 번호>: <입력> ⇒ <실제 결과> (기대: <요구 결과>)`. 제품 요구 결과(최종 소비자 값)가 확인됐는데 부가 로그 누락 때문에 `실패 —`로 보고해 재검증을 유발하지 않는다.
