# 계획·위임·마감 (`manage-project` — Plan, 오케스트레이터 전용)

오케스트레이터(메인 세션)가 행을 다른 에이전트에 맡길 때만 읽는다. 구현자(`T26_ROLE=worker`, `t26_<id>`)는 이 문서를 읽지 않는다.

## 1. 계획

- 요청 기록·행·목표 작성은 `stage-start.md`를 따른다.
- 위임 프롬프트 내용은 `role-delegator`의 위임 프롬프트 계약이 단일 원본이다. `row` 줄·목표·`$I`·`$J`·`runtime :` 줄은 `dispatch-agent`가 Grist·`AGENTS.md`에서 읽어 붙이므로 옮겨 적지 않는다.
- 위임한 행은 `$H=확인`이 될 때까지(구현자 종료 뒤 마감·흐름 확인 중 포함) 오케스트레이터가 그 행의 수정 경로를 편집하지 않는다.

## 2. 실행

- 무엇을 위임하는지(제품 코드는 대행자, 작업 도구는 위임자 직접)는 `role-delegator` 「대행자 호출」을 따른다. 대행자 1명은 구현 1개만 맡는다(구현은 파일 수가 아니라 결과 하나다). 대행자는 `--row`로 받은 행 하나만 `task-submit`할 수 있으므로(`--rows`는 검증자 전용) 행마다 따로 위임하고, 구현이 둘 이상이면 행을 나눈다(`dispatch-agent`가 거부).
- 동시에 실행하는 에이전트는 최대 5개이고 6개째부터는 앞 실행이 끝난 뒤 순차로 실행한다. 5개를 에이전트 하나에 넣지 않는다. 같은 작업공간을 쓰면 구현자는 `cargo build`·버전 올리기를 하지 않는다.
- 버전·빌드·배포 시점과 검증자 몫은 `role-delegator` 「버전·빌드·배포」「검증자 호출」과 `stage-verify.md` §1을 따른다. 로그 읽기·넓은 탐색은 대행자에게 실측 확인 조건과 함께 맡긴다.
- 구현자는 기본으로 Codex다(`--agent gemini`나 Claude `t26_<id>` 서브에이전트는 사용자가 지정한 경우에만). 호출 셸이 tmux 밖이면 `dispatch-agent`가 실행 중인 tmux 서버 첫 세션에 `dispatch-<id>` 분리 창을 만들어 실행하고 끝나면 닫는다.

  ```
  ~/ai/scripts/bin/dispatch-agent --agent codex|gemini [--role agent|verifier|improver] --project <dir> --row <id> --prompt-file <F> --expect-seconds <N> [--add-dir <DIR> ...] [--watch-log <record-log path> ...]
  # 검증자: --row <첫 행> --rows <행,행,…> 둘 다 필수, 프로젝트 .ai/logs가 없으면 --watch-log <런타임 로그>(예: Eagle %APPDATA%\Eagle\log.log) 필수
  ~/ai/scripts/bin/dispatch-agent --agent codex --role verifier --project <dir> --row <id> --rows <id,id> --prompt-file <F> --watch-log <path>
  ```

  - `--role agent`(기본)는 대행자(`T26_ROLE=worker`, `role-agent`), `--role verifier`는 검증자(`T26_ROLE=verifier`, `role-verifier`), `--role improver`는 흐름 마감 뒤 개선자(`T26_ROLE=improver`, `role-improver`, `~/ai` 쓰기 허용, 언제 부르는지는 `improve_process` §2)로 실행한다. 프롬프트 맨 앞에 `[Role] from=<보낸 역할> to=<받는 역할> row=<id> skill=<Skill>` 머리줄이 자동으로 붙는다. 검증자 기록은 `.ai/dispatch/<id>.verify.*`에 따로 남는다.
  - 프로젝트 밖을 고치는 행은 수정 경로를 `--add-dir`로 넘긴다. `~/ai/scripts` crate는 작업공간 루트(`--add-dir /home/tree/ai/scripts`)를 넘겨야 `cargo test`가 `target`·`Cargo.lock`에 쓸 수 있다.
  - `--expect-seconds` 기준: 짧은 CLI·확인 검증자 90초, 한 함수 수정 대행자 300초, 여러 파일 대행자 540초. Claude Bash 백그라운드 명령은 600초에 끊기므로 540초를 넘기지 않는다.
  - Windows 데스크톱 앱 런타임 확인은 `build-win`을 따른다.
  - 새 pane에서 headless로 실행되고 `T26_ROW`·`T26_ROLE=worker`가 넘어간다. 권한은 작업 디렉터리 쓰기까지다(codex `-s workspace-write`, gemini `--approval-mode auto_edit`).
  - 명령은 백그라운드로 실행하고 끝날 때 깨어난다. 출력은 `exit=<n> log=<path> last=<한 줄>[ case=<경로>]` 한 줄이며 pane 화면을 읽지 않는다.
  - 기록은 `<dir>/.ai/dispatch/<id>.log`·`.exit`·`.last`(Git 제외)이고 메인은 읽지 않는다(`main-edit-gate`가 거부). 결과 판정은 `role-delegator` 「결과 판정」을 따른다.

### 예외 결과 (case)

결과 줄에 `case=<경로>`가 붙으면 그 파일만 읽고 따른다. `exit=0`이고 `case=`가 없으면 아무 case 파일도 읽지 않고 §3으로 간다. `dispatch-agent`가 내지 않는 신호(30초 안에 로그 없음, 창이 사라졌는데 `.exit` 없음)는 `cases/crash.md`를 읽는다.

| 신호 | 상황 | 파일 |
|---|---|---|
| exit 6 `진행 —` | 기대 시간·5분 초과 | `cases/slow.md` |
| exit 4 `stall` | 같은 오류 반복·로그 정체 | `cases/stall.md` |
| exit 2·7, `tmux 없음`, 로그 없음, 창 사라짐 | 기동 실패·중간 종료 | `cases/crash.md` |
| exit 5 `사전점검`, exit 8 `제출점검` | 시작 전·제출 후 점검 실패 | `cases/check-fail.md` |
| `last`가 `?`로 끝남 | 구현자 질문 | `cases/question.md` |
| `20분 초과` | 교착 | `resolve-deadlock` |

## 3. 마감 (요청 대조)

1. 구현자가 끝나면 `grist task-show <id> --project <dir> --section 검증` 한 번으로 `$H=검증`(제출 기록 존재)과 `task-submit` 테스트 출력을 함께 본다.
2. 목표가 아니라 **요청과 계획**으로 판정한다. 최신 요청 원문(`~/ai/scripts/bin/grist request --project <dir>`, changes를 열지 않음)과 1의 테스트 출력을 대조해, 요청 범위가 좁혀졌거나 빠진 항목이 없는지 본다.
   - diff는 전체를 읽지 않는다. `git -C <저장소> diff --stat -- <수정 경로>`로 바뀐 파일·줄 수를 보고, 목표 줄마다 그 목표의 함수·경로 이름으로 `git diff -- <파일> | rg -n "<이름>" -C 3`처럼 해당 부분만 읽는다. 계획에 없는 파일이 stat에 보이면 그 파일만 연다.
   - 읽지 않은 나머지 변경은 숨긴 사례(`task-hidden`)와 흐름 확인 뒤의 목표별 근거(`task-confirm --evidence`)로 판정한다.
3. 맞으면 `stage-verify.md` 마감 명령 순서를 따른다. 그 과정에서 결함이 나오면 메인은 소스를 고치지 않고 `$I`·`$J`·`$H=진단`을 기록한 뒤 4번으로 재위임한다.
4. 틀리거나 3번에서 결함이 나오면 어긋난 요청 항목·결함을 `$I`·`$J`에 기록하고(`dispatch-agent`가 그 `row:` 줄을 붙인다) 같은 구현자에게 다시 맡긴다(`--resume`, 프롬프트에는 Grist에 없는 지시만). 재위임은 2회까지이며, 그 뒤에도 맞지 않으면 `$D`에 결과와 사유를 기록하고 사용자에게 넘긴다.
