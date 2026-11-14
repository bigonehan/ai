# Antigravity (agy) 전용 규칙

이 문서는 Antigravity(`~/.gemini/GEMINI.md`, `~/.gemini/antigravity-cli/GEMINI.md`) 환경에만 적용되는 전용 규칙과 도구 동작을 정의한다. 공통 전역 규칙(`~/ai/rules/Agents.md`)이나 공통 Skill·공통 게이트(`main-edit-gate`)에는 Antigravity 전용 도구 인자나 옵션을 섞지 않고 이 문서와 `agy-hook` 어댑터에서만 관리한다.

## 1. 백그라운드 명령 실행 (`run_command` — `WaitMsBeforeAsync`)

- `dispatch-agent`처럼 별도 tmux 창에서 도는 위임 명령을 `run_command`로 호출할 때는 동기 대기로 메인 세션 메시지 큐를 막지 않도록 `WaitMsBeforeAsync: 500`으로 호출해 즉시 백그라운드 태스크로 전환한다.
- 백그라운드로 전환된 명령을 기다리며 턴을 종료할 때는 태스크가 `MESSAGE_PRIORITY_LOW`로 취소되어도 세션이 깨어나도록 메인 세션(위임자)도 반드시 §3의 `schedule`(`DurationSeconds=180`, `TimerCondition="<task-id>"`) 깨우기 타이머를 함께 건다.

## 2. 내장 서브에이전트 호출 금지 (`invoke_subagent`, `define_subagent`, `manage_subagents`)

- Antigravity 내장 `invoke_subagent`·`define_subagent`·`manage_subagents`는 Table26 역할 게이트(`T26_ROLE`)와 감시·타임아웃이 적용되지 않으므로 사용하지 않는다(`agy-hook`이 PreToolUse에서 거부).
- 모든 대행자·검증자·개선자 위임은 `~/ai/scripts/bin/dispatch-agent`로 실행한다.

## 3. 백그라운드 깨우기 타이머 (`schedule` — `DurationSeconds`, `TimerCondition`)

- 메인 세션(위임자)이 `dispatch-agent`를 백그라운드로 실행하고 턴을 종료할 때(`DurationSeconds=180`), 또는 구현자·검증자가 Antigravity 세션 안에서 백그라운드 검증 명령(`task-*`)을 기다리며 턴을 종료할 때(`DurationSeconds=300`)는 백그라운드 취소·프로세스 정체에 대비해 `schedule`(`TimerCondition="<task-id>"`) 조기 종료 깨우기 타이머를 함께 건다.

## 4. Hook 어댑터·트랜스크립트(`agy-hook`) 및 전역 MCP 배포

- Antigravity의 camelCase 이벤트(`toolCall`, `WaitMsBeforeAsync`, `ask_question`, `USER_INPUT`, `PLANNER_RESPONSE`, `truncated_fields`)와 공통 게이트 사이의 변환 및 전용 차단은 `~/ai/scripts/agy-hook`이 전담한다.
- 전역 MCP 서버(`blender`, `windows-mcp`)는 `~/ai/codex/hook-manifest.json`의 `mcpServers`를 원본으로 `agent-rules sync`·`ensure`가 `~/.gemini/config/mcp_config.json`·`~/.gemini/settings.json`·`~/.gemini/antigravity-cli/settings.json`에 동기화한다. 세션 시작 시점에 `windows-mcp`가 아직 주입되지 않은 상태에서 실행 중인 Windows GUI 앱의 오류·화면 확인이 필요하면 정적 파일 추측이나 수동 해결책 안내로 넘기지 않고 `dispatch-agent --agent codex`로 위임해 `windows-mcp` 실측을 수행한다.
