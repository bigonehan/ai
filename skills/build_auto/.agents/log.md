## 2026-02-19 - 작업한일
- build_auto 스킬에 framework 기본 진입점 우선 규칙 추가
- tasks.yaml.main 충돌 시 기본 진입점 컨텍스트로 정규화 규칙 추가
- Merge/Checklist에 진입점 정규화 검증 항목 반영
## 2026-02-19 - 작업한일
- mono 내부 로직을 수정해 task별 기본 구현 파일 경로(`src/<task-module>.<ext>`)를 생성하도록 반영
- todo 생성/점검 프롬프트를 main 집중형에서 task 파일 분리형으로 변경
- build 마지막 단계에 엔트리 포인트 wiring 점검(step3) 추가
## 2026-02-19 - 작업한일
- Input 분해 산출물 경로를 ./.agents/input/*.txt 및 ./.agents/input.txt 기준으로 변경
- 기능 파일 포맷에 `- 규칙:`(다중), `> 시나리오:`(다중), `@ 상태:`(다중) 명시 규칙 추가
- ./.agents/func_template.md 참조 템플릿 추가
## 2026-02-19 - 작업한일
- mono tmux 실행을 pane/window로 세분화하고 window 전용 실행 함수 추가
- window 실행 경로에 완료 명령으로 `tmux kill-window` 종료 메시지 포함
- 일회성 대기 호출 경로를 window 전용 함수로 전환
## 2026-02-19 - 작업한일
- bootstrap 완료 게이트(파일/exit code/BOOTSTRAP_DONE 신호) 규칙 추가
- bootstrap 완료 전 domain/tasks/build 시작 금지 규칙 추가
- End-to-End 시나리오를 BOOTSTRAP_DONE 동기화 순서로 재정의
