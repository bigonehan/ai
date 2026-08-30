# User Requirements

## 2026-08-18

- 사용자 입력 UI 구현 시 실제 첫 입력이 동작하는지 확인하지 않아 화면 전체가 사라진 누락 원인을 반영하고, Test Manager Skill에 입력 전용 헤더와 강제 검증 계약·self-test를 추가한다. Grist 제품 코드와 다른 프로젝트는 변경하지 않는다.

## 2026-08-21

- cross-file 전역 API 등록명과 consumer 호출명 불일치를 실제 사용자 이벤트 경계에서 검출하고, 테스트 변경은 작성자와 다른 Agent가 독립 검증하도록 Test Manager 절차와 기계적 gate를 강화한다.
