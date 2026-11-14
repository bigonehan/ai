---
name: manage_improve
description: 작업 과정 개선 요구를 `manage_improve` 프로젝트(/home/tree/extensions/manage_improve, Table7 1605)로 모으고 처리한다 — 개발 세션(위임자·대행자·검증자)은 2회 이상 실패·착각·과정 결함을 `manage_improve note`로 등록만 하고 작업을 이어 가며, 개선은 사용자 명령(`manage_improve run`)으로 행마다 독립 개선자 세션이 처리하고 `manage_improve done`으로 완료를 기록한다. 제품 코드 수정에는 쓰지 않는다.
---

# Manage Improve

명령: `/home/tree/extensions/manage_improve/scripts/bin/manage_improve` (설계 원본 `~/plan.md` 3~5·7절).

## 1. 등록 (개발 세션, 기록만)

- 작업 중 과정·병목·도구 결함을 발견하면 개선자를 띄우지 않고 등록만 하고 원래 작업을 계속한다. 종료·위임은 개선 처리로 막히지 않는다(사용자가 명시적으로 중간 개입을 지시한 경우만 `manage-intervention`).
  ```bash
  manage_improve note --from <원래 프로젝트 dir> --row <원래 행> --target <hook:이름|skill:이름|cli:이름:하위명령|rule:줄|doc:경로> --sign <오류 기호> --text '<증상> — 원인: <경계> / 착각: <잘못 가정한 것>'
  ```
- 대상: 같은 행에서 2회 이상 실패한 오류(검증 실패·재위임·재배포), 구현·진단 중 착각(틀린 추정 원인, 확인하지 않은 사전 조건, 잘못 고른 진입점, 규칙 위반), 작업 과정 병목. 없으면 등록하지 않는다.
- 중복 검증은 도구가 한다: 같은 `target`+`sign`의 열린 행이 있으면 그 행에 `- [ ]` 한 줄만 붙이고(`dup=#id`), 14일 안에 확인된 같은 키면 `재발` 행을 만들고, 없으면 새 행(`new=#id`)을 만든다.

## 2. 처리 (사용자 명령, 순차 독립)

- `manage_improve run [--limit N]`: 실행 잠금 아래 미확인 행을 오래된 순으로 하나씩, 행마다 독립 개선자 세션(`dispatch-agent --role improver`)으로 처리한다. 2회 실패한 행은 `- [!]`로 빼고 다음 행으로 넘어간다.
- 개선자는 `role-improver` 순서를 따른다: 원인 위치(전역 규칙·Skill·hook·CLI·프로젝트 `AGENTS.md`)를 고르고 소모·병목·환형대기 관점으로 판정한 뒤, 독립 단계로 **절차가 내는 결과 형식이 제대로 되는지 판정**(필드 누락·진입점별 출력 형식 불일치 점검)하여 절차의 결과 형식부터 고치고, 문구 추가나 차단 규칙은 그 뒤에 둔다(`manage-global-rules` §1). 작업 도구는 실제 hook 환경(`env -i HOME=… PATH=/usr/bin:/bin`)에서 확인한다.

## 3. 완료 기록

```bash
manage_improve done <id> --text '<바꾼 파일·명령> — <막는 대상>'
```
- 한 명령이 그 행의 `- [ ]`를 `- [x]`로 바꾸고 `- 처리 :` 줄을 남기며 Grist `$H=확인`으로 전환한다. `--text` 없으면 거부한다.
