# 문제

# 해결
- 이전 정리에서 비워 둔 문구는 `drafts.yaml`, `feedback.md` 기준 설명으로 다시 채웠다.
- `check-code`, `feedback-followup-agent`, `orc-cli-workflow`, `add-function`, `build-code-parallel`, `.codex/skills/manager`의 `project.md`, `plan.yaml`, `input.md`, `drafts_list.yaml`, `report.md`, `check-process.md`, `drafts.yaml.failed`, `log.md`, `draft.yaml` 참조를 정리했다.
- repository 밖 파일인 `.../.codex/skills/manager/SKILL.md`는 별도 `rg`와 `sed`로 검증하도록 바꿨다.
- 응답 라우팅 중 쉘이 backtick을 해석해 메시지가 깨진 문제는 backtick 없는 문장으로 다시 라우팅해 해결했다.
- `check-ui` skill을 새로 만들고 ORC 캡처, web reference 비교, `plan.md` 작성, `orc send-tmux` worker dispatch 흐름을 문서화했다.
- 새 skill은 `quick_validate.py`와 `git diff --check`로 검증했다.

#개선필요
- 문서 정리 요청에서는 "삭제"와 "참조 치환"을 구분해서 `todo.md`에 명시한다.
- 새 skill 생성 작업도 시작 전에 `todo.md`를 현재 목표로 바로 갈아쓴다.
