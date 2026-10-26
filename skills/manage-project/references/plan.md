# 계획·위임·마감 (`manage-project` — Plan, 오케스트레이터 전용)

오케스트레이터(메인 세션)가 행을 다른 에이전트에 맡길 때만 읽는다. 구현자(`T26_ROLE=worker`, `t26_<id>`)는 이 문서를 읽지 않는다.

## 1. 계획

- 요청은 `grist request-record`로 changes `# 요청`에 기록하고, 세부 사항마다 `grist task-add`로 행과 목표를 함께 만든다. 목표는 오케스트레이터만 쓴다(구현자의 `task-add`·`task-goal`은 CLI가 거부한다).
- 위임 프롬프트 파일에는 다음만 넣는다.
  1. 행 번호와 `task-add`·`task-goal` 출력의 `row` 줄
  2. 찾은 수정 경로·줄 범위·원인(`$J`), 그리고 `task-ready`(원인 확정) 또는 `task-red`(미확정)에 쓸 `--test`·`--tests`·`--paths`
  3. "`~/ai/skills/manage-project/references/work.md`를 따라 수정하고 `task-submit`까지 한다. 목표·요청을 바꾸지 않는다"는 지시
- 위임한 행은 `$H=확인`이 될 때까지(구현자 종료 뒤 마감·흐름 확인 중 포함) 오케스트레이터가 그 행의 수정 경로를 편집하지 않는다. 전역 규칙의 "1~2개 파일은 메인이 직접"은 아직 위임하지 않은 행에만 적용한다.

## 2. 실행

- 수정 파일 3개 이상 묶음만 위임한다(1~2개 행은 오케스트레이터가 직접 처리한다). 프로젝트(수정 경로 묶음)당 1개 구현자이며 한 번에 처리하는 에이전트는 최대 2개로 제한한다.
- 구현자는 기본으로 Codex다. tmux 안에서 다음으로 실행한다(`--agent gemini`나 Claude `t26_<id>` 서브에이전트는 사용자가 지정한 경우에만 쓴다).

  ```
  ~/ai/scripts/bin/dispatch-agent --agent codex|gemini --project <dir> --row <id> --prompt-file <F> [--add-dir <DIR> ...]
  ```

  - 프로젝트 밖을 고치는 행은 수정 경로를 `--add-dir`로 넘긴다. `~/ai/scripts` crate는 작업공간 루트(`--add-dir /home/tree/ai/scripts`)를 넘겨야 `cargo test`가 `target`·`Cargo.lock`에 쓸 수 있다(crate 폴더만 넘기면 target이 읽기 전용이 된다).
  - 여러 행은 한 번에 최대 2개 에이전트까지만 백그라운드로 동시에 실행한다. 같은 작업공간을 쓰면 구현자는 `cargo build`·버전 올리기를 하지 않고 메인이 마감 때 한 번 한다.
  - 구현자가 질문으로 끝나면(`last=`가 질문) 범위 안의 판단은 메인이 답하고 `dispatch-agent --resume --agent codex --project <dir> --row <id> --prompt-file <답변> [--add-dir …]`로 같은 codex 세션을 이어 실행한다(로그의 `session id`, 같은 sandbox·네트워크·worker 환경, 로그 이어 쓰기). 로그인·비용 같은 사용자 전용 동작만 사용자에게 넘긴다.

  - 새 pane에서 headless로 실행되고 `T26_ROW`·`T26_ROLE=worker`가 넘어간다. 권한은 작업 디렉터리 쓰기까지다(codex `-s workspace-write`, gemini `--approval-mode auto_edit`).
  - 명령은 끝날 때까지 기다린 뒤 `exit=<n> log=<path> last=<한 줄>`만 출력한다. 백그라운드로 실행하고 끝날 때 깨어난다. pane 화면을 읽지 않는다.
  - 기록은 `<dir>/.ai/dispatch/<id>.log`·`.exit`·`.last`이며 Git에서 제외된다. 실패 원인이 필요할 때만 `rg -n "error|FAIL" <log> | tail -n 40`으로 좁혀 읽는다.

## 3. 마감 (요청 대조)

1. 구현자가 끝나면 행 `$H`가 `검증`인지 `grist task <id>`로 한 번 확인한다.
2. 목표가 아니라 **요청과 계획**으로 판정한다. 최신 요청 원문(`~/ai/scripts/bin/grist request --project <dir>`, changes를 열지 않음)과 `task-submit` 테스트 출력(`grist task-show <id> --project <dir> --section 검증`)을 대조해, 요청 범위가 좁혀졌거나 빠진 항목이 없는지 본다.
   - diff는 전체를 읽지 않는다. `git -C <저장소> diff --stat -- <수정 경로>`로 바뀐 파일·줄 수를 보고, 목표 줄마다 그 목표의 함수·경로 이름으로 `git diff -- <파일> | rg -n "<이름>" -C 3`처럼 해당 부분만 읽는다. 계획에 없는 파일이 stat에 보이면 그 파일만 연다.
   - 읽지 않은 나머지 변경은 숨긴 사례(`task-hidden`)와 흐름 확인 뒤의 목표별 근거(`task-confirm --evidence`)로 판정한다.
3. 맞으면 `references/verify.md`대로 숨긴 사례·배포·최종 소비 확인 → `task-confirm` → `flow-wait`(일반 경로) → `task-complete --row <id> --project <dir>`를 실행한다. 숨긴 사례·배포·최종 소비 확인에서 결함이 나오면 메인은 소스를 고치지 않고 `task-update`로 `$I`(관측한 실패)·`$J`(원인 경계)·`$H=진단`을 기록한 뒤 4번으로 재위임한다.
4. 틀리거나 3번에서 결함이 나오면 어긋난 요청 항목·결함과 근거(`$I`·`$J`가 담긴 `row` 줄)를 프롬프트에 붙여 같은 구현자에게 다시 맡긴다. 재위임은 2회까지이며, 그 뒤에도 맞지 않으면 `$D`에 결과와 사유를 기록하고 사용자에게 넘긴다.
