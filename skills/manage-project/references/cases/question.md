# 질문으로 끝남: `last`가 질문

구현자가 질문으로 끝났을 때(`exit=0`이고 `last=`가 `?`로 끝남) 위임자가 읽는다.

- 요청·목표 범위 안의 판단은 위임자가 답한다. 답을 파일로 써서 `dispatch-agent --resume --agent codex --project <dir> --row <id> --prompt-file <답변> [--add-dir …]`로 같은 codex 세션을 이어 실행한다(로그의 `session id`, 같은 sandbox·네트워크·worker 환경, 로그 이어 쓰기).
- 답변 프롬프트에는 Grist에 없는 판단만 넣는다. `row` 줄·목표·`$I`·`$J`는 `dispatch-agent`가 붙인다.
- 로그인·비용 같은 사용자 전용 동작(전역 규칙 §1)만 사용자에게 넘긴다. 새벽 2~9시(KST)에는 묻지 않고 되돌릴 수 있는 기본값으로 답한다.
