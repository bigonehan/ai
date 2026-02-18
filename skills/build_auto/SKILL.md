---
name: build_auto
description: "프로젝트 bootstrap + mono 오케스트레이션 + tmux 병렬 작업 규칙을 통합한 자동 구축 스킬."
version: 2026-02-18-unified
---

# Build Auto

## Trigger
- 사용자 입력이 아래 패턴이면 반드시 실행한다.
- 패턴: `ㅇㅇ를 이용해서 ~하는걸 만들어줄래`
- 여기서 `ㅇㅇ`는 사용 언어 또는 프레임워크다.

### 활성 키워드
- `자동구축`
- `자동오케스트레이션`
- `병렬빌드`
- `입력분해`
- `task생성`
- `tmux병렬`
- `build_auto`

## Goal
- 사용자 요청에서 언어/프레임워크를 추출해 초기 환경을 먼저 구성한다.
- 기능을 `#규칙(-)`과 `>순서`를 포함한 `input.txt`로 구조화한다.
- `mono` 명령으로 `tasks.yaml`을 만들고 TODO 생성/검토/구현까지 이어간다.
- 필요 시 tmux 병렬 pane을 열어 작업을 분산한다.

## Mandatory Constraint
- 사용자 입력의 `ㅇㅇ`(언어/프레임워크)을 최우선 제약으로 사용한다.
- LLM은 구현/파일 구조/코드 예시에서 `ㅇㅇ`를 반드시 준수한다.
- `tasks.yaml`의 `language`와도 일치해야 한다.
- `language`가 프레임워크면 해당 프레임워크의 기본 진입점을 기준으로 TODO를 수행한다.

## Hard Rules
- TypeScript 계열(package.json 기반)은 기본 패키지 매니저를 `bun`으로 사용한다.
- TypeScript 계열 unit test는 기본 `vitest`를 사용한다.
- Rust는 `cargo`를 사용한다.
- bootstrap 없이 `input.txt -> tasks.yaml -> build`로 바로 들어가지 않는다.
- bootstrap 완료 게이트를 통과하기 전에는 domain/tasks/build 단계를 시작하지 않는다.
- `domain 생성(= mono save_request_by_file)`은 `bootstrap 완료`와 `input.txt 완성`이 모두 끝난 뒤에만 시작한다.
- bootstrap 단계와 domain/tasks/build 단계를 비동기/동시 실행하지 않는다.
- 선행 단계 검증 실패 시 다음 단계를 시작하지 않고 즉시 중단한다.

## Jujutsu Workspace Rule (강제)
- build_auto에서 `jj` 작업 루트는 기본적으로 `cwd` 기준으로 고정한다.
- 모든 `jj` 명령 전에 반드시 아래를 선행한다:
```bash
set target (flow_ensure_jj_repo_for_cwd (pwd)); cd $target
```
- 루트 검증 실패 시 즉시 중단하고, `jj root`가 `target`과 일치할 때만 다음 단계를 진행한다.
- `jj st`, `jj log`, `jj diff`, `jj workspace list`를 루트 검증 전에 실행하지 않는다.
- `/tmp` 경로에 전용 workspace를 만드는 방식은 기본 금지다.
- 예외: 사용자가 `/tmp` 격리를 명시적으로 요청했거나, 고위험 구조 변경으로 물리 격리가 필요한 경우에만 허용한다.

## Shell Command Safety (fish) (강제)
- `fish -ic` 명령에서는 heredoc(`<<EOF`)을 사용하지 않는다.
- 파일 생성은 `printf` 또는 `bash -lc`로 처리한다.
- 기본값: `fish -ic`에서는 `printf`를 우선 사용한다.

## Bootstrap Completion Gate (강제)
아래 3가지를 모두 만족해야 bootstrap 완료로 간주한다.
1. 스택별 핵심 파일/디렉토리가 생성되었다.
2. 의존성 설치 명령이 0 exit code로 끝났다.
3. `BOOTSTRAP_DONE` 동기화 신호가 발행되었다.

스택별 최소 체크 예시:
- Next.js / React+Vite / Astro / TypeScript(Node): `test -f package.json`
- Rust: `test -f Cargo.toml`
- Python: `test -d .venv`

동기화 규칙:
- bootstrap pane 완료 명령은 반드시 `tmux wait-for -S BOOTSTRAP_DONE`를 포함한다.
- orchestrator는 `tmux wait-for BOOTSTRAP_DONE`이 끝나기 전까지 `mono init`, domain 단계, worker pane 분리를 시작하면 안 된다.

## Entry Point Resolution Rule (강제)
1. `language`가 프레임워크인 경우, 진입점은 `tasks.yaml.main`보다 프레임워크 기본값을 우선한다.
2. `tasks.yaml.main`은 "앱 시작점 대체"가 아니라 "프레임워크 진입점 아래에서 작업할 대상 모듈/라우트"로 해석한다.
3. `tasks.yaml.main`이 기본 진입점과 충돌하면 기본 진입점으로 정규화하고, 충돌값은 하위 작업 파일 경로로 재해석한다.
4. Worker는 TODO 작성 시 항상 "framework entrypoint context"를 먼저 명시한다.

기본 진입점 기준:
- Next.js(App Router): `app/page.tsx` (또는 `app/<segment>/page.tsx`)
- React + Vite: `src/main.tsx`
- Astro: `src/pages/index.astro`
- TypeScript(Node): 템플릿에서 생성된 `main` 파일(예: `src/main.ts` 또는 `index.ts`)
- Rust: `src/main.rs`
- Python: 프로젝트 기본 실행 파일(예: `main.py`)

## Stack Bootstrap
### Next.js (TypeScript)
```bash
bun create next-app@latest app --ts --eslint --src-dir --app --import-alias "@/*"
cd app
bun add -d vitest @vitest/ui @testing-library/react @testing-library/jest-dom jsdom
```
### React + Vite (TypeScript)
```bash
bun create vite app --template react-ts
cd app
bun install
bun add -d vitest @vitest/ui @testing-library/react @testing-library/jest-dom jsdom
```
### Astro (TypeScript)
```bash
bun create astro@latest app --template basics --typescript strict
cd app
bun install
bun add -d vitest @vitest/ui jsdom
```
### TypeScript (Node)
```bash
mkdir app && cd app
bun init -y
bun add -d typescript vitest @types/node tsx
bunx tsc --init
```
### Rust
```bash
cargo new app
cd app
```
### Python
```bash
python -m venv .venv
. .venv/bin/activate
pip install -U pip pytest
```

## Core Functions / Commands
- `mono init -l "<language>"`
- `mono save_request_by_file`
- `mono build_task`
- `mono rt -m "..." -w <true|false> -c "..."`
- `mono rtw -m "..." -w <true|false> -c "..."`

## Pane Control Rules (필수)
- pane 생성은 반드시 `mono rt` 또는 `mono rtw`만 사용한다.
- "각 pane은 저장이 끝나면 종료한다"는 문구만 쓰지 말고, 항상 명령까지 같이 쓴다.
- 병렬 worker 기본은 `-w false`다.
- 결과 저장이 필수인 worker는 `-w true`와 `-c`를 함께 사용해 완료 액션을 명시한다.
- 문서/프롬프트에는 아래 3요소를 항상 같이 적는다.
1. pane 시작 명령 (`mono rt|rtw ...`)
2. 결과 저장 파일명 (`${index}_func.txt`)
3. 종료/완료 액션 (`-c` 또는 완료 후 종료 규칙)

권장 예시:
```bash
mono rtw -m "기능1 규칙/시나리오 작성 후 1_func.txt 저장" -w true -c "echo done_1"
```

## Role Split Rule
1. Orchestrator pane
- 사용자 요구 해석, 분해 기준 확정, 입력/출력 파일 최종 승인 담당.
2. Worker pane(s)
- 개별 기능의 TODO 구현, 테스트, 리포트 작성 담당.
3. Reviewer pane (선택)
- 충돌 검토, 규칙 위반 검사, 최종 머지 승인 담당.

## Communication Contract
- 데이터 교환은 공용 파일을 사용한다(메시지/결과는 파일, 제어는 tmux).
- 권장 경로:
- `./.agents/bus/inbox.ndjson`
- `./.agents/bus/outbox.ndjson`
- `./.agents/bus/status/<worker>.json`

### State Transition (고정)
- `idle -> assigned -> running -> done`
- 실패 시 `running -> blocked`

## Merge Rule
1. 우선순위: `blocked` 해결 > `done` 반영 > 신규 기능 착수
2. 충돌 기준: 사용자 요구 > plan > tasks.yaml 규칙 > worker 제안
3. 검토: `tasks.yaml`의 `language/depends_on` 준수 + `main`은 Entry Point Resolution Rule로 정규화 + 테스트 통과

## Input.txt Generation Protocol (강제)
1. 앱 정의에 필요한 핵심 기능 5개를 먼저 확정한다.
2. 기능 5개를 각각 별도 tmux pane으로 분리한다.
3. 각 pane은 아래 형식으로 작성한다. (`규칙/시나리오/상태`는 여러개 가능)
- `# 기능명`
- `- 규칙: ...` (1개 이상, 여러개 가능)
- `> 시나리오: ...` (1개 이상, 여러개 가능)
- `@ 상태: ...` (0개 이상, 여러개 가능)
4. 각 pane은 완료 시 `./.agents/input/${index}_func.txt`로 저장한다. (`1_func.txt` ~ `5_func.txt`)
5. 참조 템플릿은 `./.agents/func_template.md`를 사용한다.
6. 메인 pane은 `./.agents/input/1_func.txt` ~ `./.agents/input/5_func.txt`를 합쳐 `./.agents/input.txt`를 만든다.
7. `mono save_request_by_file` 호환을 위해 `./.agents/input.txt`를 `./input.txt`로 복사한다.
8. `input.txt` 병합 전에 worker 완료 게이트를 반드시 통과한다. (완료 마커/파일 존재/파일 비어있지 않음)

### Worker Completion Gate (강제)
- `input` 생성 단계는 worker 완료 확인이 끝나기 전 병합/변환 단계로 넘어가면 안 된다.
- 아래 둘 중 하나를 반드시 사용한다.
1. worker 명령에 `-w true -c "touch ./.agents/input/<index>.done"`를 사용하고, done 파일 5개를 확인한 뒤 병합한다.
2. `./.agents/input/1_func.txt` ~ `5_func.txt`가 모두 존재하고 비어있지 않음을 확인한 뒤 병합한다.
- 게이트 검증 명령이 실패하면 즉시 중단한다(`mono save_request_by_file` 금지).

## End-to-End Scenario
1. 사용자 요청에서 `ㅇㅇ` 추출
2. bootstrap 수행 + 완료 신호 발행
```bash
# 예시(Next.js): bootstrap pane 종료 직전에 완료 신호 발행
test -f package.json && tmux wait-for -S BOOTSTRAP_DONE
```
3. orchestrator가 bootstrap 완료 신호를 확인 (다음 단계 진입 전 필수)
```bash
tmux wait-for BOOTSTRAP_DONE
```
4. `mono init -l "<ㅇㅇ>"`
5. 템플릿/입력 디렉터리 준비 (`fish -ic` 안전 방식)
```bash
mkdir -p ./.agents/input
printf "%s\n" "# 기능명" "- 규칙: ..." "> 시나리오: ..." "@ 상태: ..." > ./.agents/func_template.md
```
6. 5개 worker pane 실행 (예시, 완료 마커 필수)
```bash
mono rtw -m "기능1을 ./.agents/func_template.md 형식으로 작성 후 ./.agents/input/1_func.txt 저장" -w true -c "touch ./.agents/input/1.done"
mono rtw -m "기능2를 ./.agents/func_template.md 형식으로 작성 후 ./.agents/input/2_func.txt 저장" -w true -c "touch ./.agents/input/2.done"
mono rtw -m "기능3을 ./.agents/func_template.md 형식으로 작성 후 ./.agents/input/3_func.txt 저장" -w true -c "touch ./.agents/input/3.done"
mono rtw -m "기능4를 ./.agents/func_template.md 형식으로 작성 후 ./.agents/input/4_func.txt 저장" -w true -c "touch ./.agents/input/4.done"
mono rtw -m "기능5를 ./.agents/func_template.md 형식으로 작성 후 ./.agents/input/5_func.txt 저장" -w true -c "touch ./.agents/input/5.done"
```
7. worker 완료 게이트 확인 (실패 시 즉시 중단)
```bash
test -f ./.agents/input/1.done \
  && test -f ./.agents/input/2.done \
  && test -f ./.agents/input/3.done \
  && test -f ./.agents/input/4.done \
  && test -f ./.agents/input/5.done
```
8. 메인 pane 병합
```bash
cat ./.agents/input/1_func.txt ./.agents/input/2_func.txt ./.agents/input/3_func.txt ./.agents/input/4_func.txt ./.agents/input/5_func.txt > ./.agents/input.txt
cp ./.agents/input.txt ./input.txt
```
9. `input.txt` 완성 게이트 검증 (실패 시 즉시 중단)
```bash
test -s ./.agents/input.txt \
  && [ "$(rg -n "^# " ./.agents/input.txt | wc -l)" -ge 5 ] \
  && [ "$(rg -n "^- 규칙:" ./.agents/input/*.txt | wc -l)" -ge 5 ] \
  && [ "$(rg -n "^> 시나리오:" ./.agents/input/*.txt | wc -l)" -ge 5 ]
```
10. domain 생성 단계 시작 (`mono save_request_by_file`)
```bash
mono save_request_by_file
```
11. 변환/구현
```bash
mono build_task
```

## Phase Barrier Rule (강제)
- 아래 단계는 반드시 순차 실행한다: `bootstrap -> input.txt 생성 -> domain 생성 -> build`.
- orchestrator는 이전 단계의 완료 증거(파일/exit code/wait-for 신호)를 확인하기 전 다음 명령을 실행하면 안 된다.
- 금지: `tmux wait-for BOOTSTRAP_DONE` 이전에 `mono save_request_by_file` 실행.
- 금지: `./input.txt` 생성/검증 이전에 `mono save_request_by_file` 실행.
- 금지: `mono save_request_by_file` 완료 전에 `mono build_task` 실행.

## Output Checklist
- bootstrap 완료 게이트(`BOOTSTRAP_DONE`)를 통과한 뒤 domain 단계가 시작되었는가?
- `./.agents/input/` 아래에 `${index}_func.txt`가 생성되었는가?
- `./.agents/input.txt` 병합이 완료되었는가?
- `규칙/시나리오`가 각 기능 파일에 1개 이상 포함되었는가?
- `상태(@ 상태)`가 필요한 기능에 충분히 기록되었는가?
- 게이트 검증을 통과했는가?
- `tasks.yaml.language`가 요청 스택과 일치하는가?
- `tasks.yaml.main`이 framework 기본 진입점 컨텍스트로 정규화되었는가?
