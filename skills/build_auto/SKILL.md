---
name: build_auto
description: "mono + tmux 기반으로 입력 명령을 기능/규칙/순서로 분해하고 tasks.yaml 생성-구현까지 자동 오케스트레이션하는 스킬."
version: 2026-02-18
---

# Build Auto

## Trigger
- 사용자 요청에 `병렬처리`, `build auto`, `자동 구축`, `input.txt -> tasks.yaml`, `mono rt`, `run_ai_in_tmux_and_wait`가 포함되면 실행한다.
- 아래 활성 키워드도 동일 트리거로 처리한다.

### 활성 키워드
- `자동구축`
- `자동오케스트레이션`
- `병렬빌드`
- `입력분해`
- `task생성`
- `tmux병렬`
- `build_auto`

## Goal
- 사용자의 1줄 명령을 기능 단위로 분해한다.
- 기능을 `#규칙(-)`과 `>순서`를 포함한 `input.txt`로 구조화한다.
- `mono` 명령으로 `tasks.yaml`을 만들고, TODO 생성/검토/구현까지 이어간다.
- 필요 시 tmux 병렬 pane을 열어 작업을 분산한다.

## Core Functions / Commands
- `mono init -l "<language>"`
  - 기본 구현 언어를 `config.yaml`에 저장한다.
- `mono save_request_by_file`
  - `input.txt`를 읽어 `tasks.yaml` 생성 파이프라인을 수행한다.
- `mono build_task`
  - `tasks.yaml` 기준으로 TODO 구현 작업을 실행한다.
- `mono rt -m "..." -w <true|false> -c "..."`
- `mono rtw -m "..." -w <true|false> -c "..."`
  - `run_ai_in_tmux_and_wait` 축약 명령. 새 tmux pane에서 Codex 작업을 실행한다.

## Role Split Rule
1. Orchestrator pane
- 사용자 요구 해석, 분해 기준 확정, 입력/출력 파일 최종 승인 담당.

2. Worker pane(s)
- 개별 기능의 TODO 구현, 테스트, 리포트 작성 담당.
- Worker는 자기 책임 범위 외 파일 수정 금지.

3. Reviewer pane (선택)
- 충돌 검토, 규칙 위반 검사, 최종 머지 승인 담당.

## Communication Contract
- 데이터 교환은 공용 파일을 사용한다(메시지/결과는 파일, 제어는 tmux).
- 권장 경로:
  - `./.agents/bus/inbox.ndjson`
  - `./.agents/bus/outbox.ndjson`
  - `./.agents/bus/status/<worker>.json`

### NDJSON Message Schema
```json
{"id":"msg-1","from":"orchestrator","to":"worker-a","type":"task","ts":"2026-02-18T00:00:00Z","payload":{"function":"..."}}
```

### State Transition (고정)
- `idle -> assigned -> running -> done`
- 실패 시 `running -> blocked` 후 원인/재시도 조건 기록.

## Merge Rule
1. 우선순위
- `blocked` 해결 > `done` 반영 > 신규 기능 착수.

2. 충돌 시 우선 기준
- 사용자 명시 요구 > plan 파일 > tasks.yaml 규칙 > worker 제안.

3. 검토 단계
- `tasks.yaml`의 `language`, `main`, `depends_on` 준수 여부 확인.
- 테스트 통과 후에만 완료로 표시.

## End-to-End Scenario (권장 절차)
1. 초기 설정
```bash
mono init -l "python"
```

2. 사용자 명령을 기능으로 분해
- 입력 예: "회원가입/로그인/비밀번호 재설정 API 구축"
- 분해 결과를 `input.txt`에 작성:
```text
# 회원가입 API
- 이메일/비밀번호 유효성 검사를 수행해야 한다
> 요청 스키마 검증
> 중복 이메일 확인
> 사용자 생성

# 로그인 API
- 인증 실패 시 표준 에러를 반환해야 한다
> 요청 스키마 검증
> 비밀번호 검증
> 토큰 발급

# 비밀번호 재설정 API
- 재설정 토큰 만료 시간을 검증해야 한다
> 토큰 검증
> 비밀번호 갱신
```

3. input.txt -> tasks.yaml 변환/보강
```bash
mono save_request_by_file
```

4. 필요 시 병렬 pane 작업 분산
```bash
mono rt -m "tasks.yaml를 읽고 API 테스트 코드 초안 작성" -w false
mono rt -m "tasks.yaml를 읽고 인증 모듈 구현" -w false
```

5. TODO 실제 구현 실행
```bash
mono build_task
```

6. 검증
- 테스트/체크 명령 실행 후 실패 시 `tasks.yaml`의 TODO/depends_on 재정렬.

## Execution Guardrails
- `tasks.yaml`의 `language`를 반드시 따른다.
- domain이 비어 있으면 `scope(feature/component/utility)` 경로를 따른다.
- `depends_on` 없는 작업부터 먼저 병렬 실행하고, 의존 작업은 후순위로 실행한다.

## Output Checklist
- `input.txt`가 기능/규칙/순서 형식으로 작성되었는가?
- `tasks.yaml`가 생성되었고 `main/language`가 `config.yaml`과 일치하는가?
- 병렬 작업 결과가 bus 파일 또는 로그로 추적 가능한가?
- 충돌 해결/검토 단계를 거쳐 최종 구현이 완료되었는가?
