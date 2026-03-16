---
name: feedback-followup-agent
description: "`orc-cli-workflow`의 check 단계에서 호출되며, 새 tmux pane에서 `check-code`와 `orc-cli-workflow`를 사용해 코드 체크를 수행하고 `.project/feedback.md`를 갱신하며, `.project/drafts.yaml` 기준으로 남은 문제를 해결하는 followup agent다."
---

# Feedback Followup Agent

## 언제 사용하나
- 작업이 거의 끝난 것처럼 보여도 `.project/feedback.md`의 `# 문제`가 비어 있지 않을 때
- 완료 응답 전에 잔여 feedback를 별도 tmux pane에서 계속 처리해야 할 때
- `orc-cli-workflow`의 check 단계를 별도 pane agent로 수행해야 할 때

## 고정 규칙
- `# 문제` bullet이 1개라도 있으면 완료 응답을 하지 않는다.
- followup agent는 잔여 항목만 다룬다. 새 범위를 넓히지 않는다.
- 해결된 항목은 `# 문제`에서 `# 해결`로 이동한다.
- `# 문제`가 0개가 될 때까지 pane을 닫지 않는다.
- followup agent는 반드시 `check-code` skill과 `orc-cli-workflow` skill을 함께 사용한다.
- followup agent는 `orc-cli-workflow`의 check-code 구간 전담 agent다.
- followup agent는 코드 체크를 수행하고 `.project/feedback.md`를 직접 갱신해야 한다.
- followup agent는 feedback을 읽은 뒤 `./.project/drafts.yaml` 기준으로 남은 문제를 해결해야 한다.

## 실행 절차
1. 현재 작업 루트의 `.project/feedback.md`를 읽고 `# 문제` bullet만 추출한다.
2. bullet이 0개면 이 skill은 즉시 종료한다.
3. bullet이 남아 있으면 새 pane을 연다.
   - `tmux split-window -h -P -F '#{pane_id}'`
4. followup prompt를 만든다.
   - 작업 루트
   - 남은 `# 문제` bullet 목록
   - 최근 검증 명령
   - `check-code`, `orc-cli-workflow` skill 강제 사용 지시
   - `orc-cli-workflow`의 check 단계 대행이라는 역할 설명
   - feedback을 읽고 `drafts.yaml` 기준으로 문제를 해결하라는 직접 명령
   - `# 문제`를 0개로 만들기 전에는 종료 금지 규칙
5. pane에 followup agent를 보낸다.
   - 선호: `orc send-tmux <pane_id> "<command>" enter`
   - 대안: `tmux send-keys -t <pane_id> "<command>" C-m`
6. 메인 pane은 아래 파일만 기준으로 진행 상태를 확인한다.
   - `.project/feedback.md`
   - `.project/drafts.yaml`
7. `# 문제`가 0개가 되면 pane을 닫고 그때만 완료 단계로 돌아간다.

## Followup Prompt 템플릿
```text
현재 작업 루트: <workdir>
남은 feedback 문제:
- <문제 1>
- <문제 2>

규칙:
- 반드시 `check-code` skill과 `orc-cli-workflow` skill을 사용한다.
- 너는 `orc-cli-workflow`의 check 단계 전담 followup agent다.
- 먼저 `.project/feedback.md`를 읽고 `.project/drafts.yaml` 기준으로 남은 문제를 정리한다.
- 그 다음 코드 체크를 수행하고 `.project/feedback.md`를 갱신한다.
- `.project/feedback.md`의 `# 문제`를 0개로 만든다.
- 해결된 항목은 `# 해결`로 이동한다.
- 필요한 코드 수정과 검증을 직접 반복한다.
- 완료 보고를 하지 말고 문제를 모두 없앤 뒤에만 종료한다.
```

## pane 명령 예시
```text
cd <workdir> && codex exec --dangerously-bypass-approvals-and-sandbox "`.project/feedback.md`를 읽고 `.project/drafts.yaml` 기준으로 남은 문제를 해결하라. `check-code` skill과 `orc-cli-workflow` skill을 반드시 사용하라."
```

## 종료 게이트
- `.project/feedback.md`의 `# 문제` bullet 수가 0
- 필요한 상태 변경이 `.project/drafts.yaml`와 `.project/feedback.md`에 반영됨
- 필요한 검증 명령 성공
- 새로 생긴 blocking issue도 `# 문제`에서 정리 완료
