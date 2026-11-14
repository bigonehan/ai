---
name: manage_decision
description: 사용자 프롬프트를 개선·기능·해결·질문 4분류로 판정하는 "판단" 영역 — /home/tree/extensions/manage_decision CLI(decide·mode·log·mark)로 판정·방식 선택·오판 기록을 한다. hook(prompt-decision.rs)이 프롬프트당 1회 호출하며, 판정 결과를 확인·조정하거나 오판을 표시할 때 쓴다. Table26 목표 대조(J1)·완료 메시지 분류(J2)·목표 범위(J3)는 access-jev 몫이라 쓰지 않는다.
---

# Manage Decision

## 4분류

- 개선 : AI 작업 과정의 결함·개선 요구(다른 프로젝트 작업에도 이어지는 절차·규칙·Skill·hook·작업 도구·대행자/검증자/개선자 동작). 개선자가 담당하며 hook은 개입 턴(`manage-intervention`)으로 처리한다.
- 기능 : 개별 제품 프로젝트에 새 기능·옵션 추가 요청.
- 해결 : 개별 제품 프로젝트 내부의 결함·오류 지적과 수정 요구.
- 질문 : 단순한 사용자 질문(진행 현황·지연 이유 포함). 작업 과정 중이면 작업보다 답변을 먼저 한다.
- "이전 작업 계속"은 사용자가 명시적으로 말하므로 따로 판정하지 않는다.

## CLI (`/home/tree/extensions/manage_decision/target/release/manage_decision`)

- `decide` : 표준 입력(또는 `--prompt-file F`)의 프롬프트를 판정해 `{"id","label","confidence","provider","escalated","ms"}` 한 줄을 낸다. 모든 구현체가 실패하면 stderr JSON·exit 2이고 hook은 판정을 건너뛴다(fail-open).
- `mode set <방식> [--threshold N]` / `mode show` : 방식은 `d3`·`jev`·`gemini` 또는 `<1차>+<2차>`. 1차 confidence가 기준값 미만이거나 1차가 실패하면 2차를 쓴다. 선택은 `config.json`에 저장돼 바꾸기 전까지 유지되고, 파일이 없으면 기본값 `gemini+jev 0.9`다.
- `log show [--wrong] [--last N]` / `mark <id> --label <4분류>` : 판정 로그(`logs/decisions.jsonl`, Git 제외)를 보고, 오판에 정답 라벨을 붙여 `--wrong`으로 모은다.
- 키는 프로젝트 `.env`(`TYPESAFE_API_KEY`·`GEMINI_API_KEY`, Git 제외)에서만 읽는다. 키를 출력·로그·명령에 넣지 않는다. 저장 위치는 `MANAGE_DECISION_HOME`, 실행 파일은 hook에서 `MANAGE_DECISION_BIN`으로 덮어쓸 수 있다.

## 구현체 특성 (261114 측정, changes 요청 문장 62개)

- gemini+jev 0.9 : 55/62, jev 호출 20회, 문장당 약 1~1.4초. Gemini(`gemini-2.5-flash`)는 logprobs를 거부해 JSON 자기 신뢰도를 쓴다.
- d3+jev 0.5 : 54/62, 문장당 약 0.1초. d3는 keep_alive 20분으로 약 6.5GB GPU를 쓰고, 끄려면 `ollama stop d3:4b-q8_0`.
- jev 단독 53/62, d3 단독 52/62. 공통 약점은 제품 동작 흐름을 설명한 기능 요청을 개선으로, 내부 용어(대행자·게이트)만 쓴 과정 문장을 기능·해결로 보는 경계다.

## hook 연결

- `~/ai/scripts/prompt-decision.rs`가 `main-edit-gate`·`task-complete`에서 프롬프트당 1회 `decide`를 호출하고 세션 상태 `process_turn/<session>`에 라벨을 캐시한다. `"그렇게 해줘"`·`"진행해줘"` 같은 짧은 진행 동의 입력은 `계속`으로 처리하고, `logs/decisions.jsonl`에 `mark`(`truth`) 정정이 기록된 프롬프트는 캐시된 라벨보다 `truth` 정정 라벨을 우선 반영한다.
- 판정이 틀렸다고 사용자가 지적하면 `log show --last 5`로 id를 찾아 `mark`로 정답을 기록한다. 기준·정의를 바꿀 때는 manage_decision 프로젝트 행으로 대행자에게 맡긴다.
