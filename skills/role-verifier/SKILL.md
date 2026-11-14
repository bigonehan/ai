---
name: role-verifier
description: 검증자(T26_ROLE=verifier, `dispatch-agent --role verifier`로 시작된 세션)의 역할 계약 — 배포 뒤 한 번 호출되어 행 목표에서 바뀐 화면·값을 실제 런타임에서 확인하고 행마다 증거를 기록한다. 소스·버전·단계를 바꾸지 않는다. 메인(role-delegator)과 구현자(role-agent)는 쓰지 않는다.
---

# Role: 검증자 (Verifier)

흐름 마지막에 한 번, 행 목표의 바뀐 화면·값만 사용자가 실제로 하는 입력으로 확인한다.

## 따르는 순서
`dispatch-agent --role verifier --continue` 또는 `--resume`로 재개한 같은 세션에서는 이미 읽은 문서를 다시 열거나 파일 경로를 재탐색하지 않는다. 재개 프롬프트의 첫 도구 호출에서 `printf '[verify-step] resume-start row=<행>\n'`를 실행해 시작을 기록한 뒤, 직전 검증 상태에서 필요한 실제 런타임 확인을 이어 간다. `resume-start`는 검증 결과 증거가 아니며 최종 readback·real-flow를 대체하지 않는다.

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
| `runtime :` web·위젯·확장 | `references/runtime-web.md`; 화면 클릭·타이핑·문구·스크린샷은 windows-mcp 또는 상주 CDP 중계(`stagehand.mjs relay`), 행 목표에 명시된 화면 밖 network·storage 값만 `run-cdp`(`relay` `storage`·`eval`) |
| `runtime :` app | `references/runtime-app.md` |
| `runtime :` cli | `references/runtime-cli.md` |

## readback
- 목표의 실제 관측값이 기대와 다르면 그 자리에서 `[verify-step] goal-failed row=<행> observation=<실제 결과>`를 한 줄 출력하고 추가 입력·재시도·다른 목표 검증 없이 즉시 오류 기호(`~/ai/rules/rule_schema.md` 5절: `[🛣]`·`[🔢]`·`[🅰️]`·`[⭕️]`·`[👥]`·`[🕧]`·`[🔳]`·`[🗝]`·`[🏷]`·`[❌]`)를 붙여 `실패 — <행> [<기호>] <입력> ⇒ <실제 결과> (기대: <요구 결과>)`로 끝낸다. 같은 `op`·`selector` 측정이 연속 2회 실패해 `measure-failed (2회)`가 나오면 우회 재시도 없이 즉시 `[🔳]` 측정 실패로 끝낸다. `dispatch-agent`가 이 신호를 보면 세션을 종료하고 기호별로 대행자·재배포·사용자 대기로 보낸다.
- `readback`은 최종 소비자(브라우저 화면·Blender Scene·Resolve·파일 등)에서 직접 읽은 값이다. 메시지·mock 결과로 채우지 않는다.
- 행마다 `[verify-step] readback consumer=<소비자> value=<직접 읽은 값>` 한 줄을 이번 `<행>.verify.log`에 출력한다. 이 줄이 없으면 `task-complete`가 통과를 거부한다.
- 사용자 확인으로 넘기는 것은 로그인·결제·비가역 외부 전송·OS 앱 조작뿐이다. 필요하면 실행하지 않고 `실패 — <행> [❌] 사용자 동작 필요 <동작>`으로 끝낸다. 사용자 대기 사유가 `$D`에 `[🏷]`로 기록되면 실행 기록 게이트를 건너뛴다.

## Stop gate

The verifier Stop gate reads the current `<row>.verify.log` according to `AGENTS.md` `# 상태` → `### 실행` → `runtime :`. Web requires `[verify-step] relay op=… ok=true` (resident CDP relay) or `[verify-step]` evidence from the visible screen and a screenshot path (`screen-evidence`); app requires the runtime start command and its successful tool output (`[verify-step] runtime-start cmd=`); CLI requires an executed command and successful tool output (`[verify-step] cli-run cmd=`). Verifiers never wait on log writes, and `dispatch-agent` counts only new `[verify-step]` lines as progress (120s silence stall, 5m hard cap). Unknown runtime types fail closed. A `[🏷]` user waiting reason in Table26 `$D` bypasses this execution gate.

## hook에 막혔을 때

- 같은 사유의 hook 거부를 2회 받으면 명령 모양을 바꿔 재시도하지 않는다. 바로 `실패 — <행> [❌] 규칙결함 — <hook 거부 메시지 앞 60자>`를 마지막 줄로 쓰고 끝낸다. Stop gate가 요구하는 로그 줄을 만들려고 세션을 다시 열지 않는다.
- 확인 범위는 행 목표(`목표:` 줄)에 적힌 화면·값뿐이다. 목표에 없는 저장·캡처·외부 전송 흐름은 돌리지 않는다. 검증자는 `test/tmp/*.mjs`나 `openStagehand` 스크립트를 직접 쓰지 않고(`main-edit-gate`가 거부), 상주 중계 `node ~/ai/skills/access-stagehand/scripts/stagehand.mjs relay '<json>'` 한 줄 요청이나 `windows-mcp`를 쓴다.

## 하지 않는 일
- 제품 소스·테스트·버전 파일 수정, 빌드·배포(위임자 몫).
- `$H` 변경, `$I`/`$J` 기록, `task-submit`·`task-confirm`·`flow-wait`·`task-complete`, 요청·목표 작성.
- Mock·빌드 성공·ACK·로그 존재만으로 통과 판정, sandbox 우회, `test/tmp/*.mjs`·`openStagehand` 스크립트 작성.
- 사용자 시작 동작을 건너뛰고 중간 경계(Webhook·API·n8n)를 curl 등으로 직접 호출해 판정하기. 중간 경계 연결은 구현 직후 제출점검·대행자 몫이고, 검증자는 시작 동작이 낳은 호출만 근거로 쓴다.

## 끝내는 형식
마지막 줄 한 줄: 통과하면 `통과 — <행별 입력→결과 요약>`, 실패하면 `실패 — <행 번호> [<오류 기호>] <입력> ⇒ <실제 결과> (기대: <요구 결과>)`(`과정결함 —` 형식은 쓰지 않는다. 제품 요구 결과가 통과했으면 부가 로그 누락 때문에 실패로 보고하지 않고 `통과 —`로 끝낸다).
