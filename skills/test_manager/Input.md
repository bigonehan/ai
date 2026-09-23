# User Requirements

## 2026-08-18

- 사용자 입력 UI 구현 시 실제 첫 입력이 동작하는지 확인하지 않아 화면 전체가 사라진 누락 원인을 반영하고, Test Manager Skill에 입력 전용 헤더와 강제 검증 계약·self-test를 추가한다. Grist 제품 코드와 다른 프로젝트는 변경하지 않는다.

## 2026-08-21

- cross-file 전역 API 등록명과 consumer 호출명 불일치를 실제 사용자 이벤트 경계에서 검출하고, 테스트 변경은 작성자와 다른 Agent가 독립 검증하도록 Test Manager 절차와 기계적 gate를 강화한다.

## 2026-09-10

- 배경 생성 도중 ChatGPT 창이 닫히고 재생성되어 비용이 반복 발생했는데 기존 테스트가 이를 놓친 이유를 로그로 조사하고 Test Manager skill을 수정한다. 승인된 구현 범위는 스킬, 검증기, 오프라인 회귀검증 및 발견용 심볼릭 링크이다. 제품 코드 변경과 실제 AI 생성·전송·재시도는 범위 밖이다.
- 누락된 production 전역 함수를 테스트가 주입해 보완한 경우, 5분 알람 이후 닫기·초기화·재전송을 관측하지 않은 경우, 실패를 완료로 계산한 경우를 회귀검증한다. 테스트·상태조회·빌드 성공을 실제 생성 결과 수신 검증과 구분한다.
- 근거와 재발 방지 계약: [false-positive-regressions.md](references/false-positive-regressions.md).

## 2026-09-23

- 외부 API를 사용하는 테스트가 실제 API 응답 계약과 production consumer 경계를 반영하도록 Test Manager Skill과 검증기를 수정한다. live response의 안전한 schema capture, fixture 출처, 실행 환경, production consumer 실행, 필드명·타입 mutant 거부를 하나의 검증 계약으로 묶는다.
