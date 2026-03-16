# problem
- `check-ui` skill이 아직 없고, 사용자가 요청한 ORC 기반 UI 감사 -> web reference 비교 -> `plan.md` 작성 -> tmux pane 위임 흐름을 담아야 한다.
- 새 skill은 `orc clit` 캡처, 인터넷 검색, reference screenshot 비교, `plan.md` 작성, `orc send-tmux` 위임을 모두 포함해야 한다.

# tasks
- `skill-creator` 절차대로 `check-ui` skill 골격을 만든다.
- `check-ui/SKILL.md`에 UI 감사, reference 검색, 비교 기준, `plan.md` 형식, tmux pane dispatch 흐름을 작성한다.
- `agents/openai.yaml`을 확인하고 validator를 실행한다.
- 완료 로그를 `./.agents/log.md`에 남긴다.
- 응답 라우팅용 메시지는 shell command 안에서 backtick 없이 구성한다.

# check
- `python3 /home/tree/.codex/skills/.system/skill-creator/scripts/quick_validate.py /home/tree/ai/skills/check-ui`
- `git -C /home/tree/ai diff --check -- /home/tree/ai/skills/check-ui/SKILL.md /home/tree/ai/skills/check-ui/agents/openai.yaml /home/tree/ai/todo.md /home/tree/ai/.project/feedback.md /home/tree/ai/.agents/log.md`
