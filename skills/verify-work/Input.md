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

## 2026-10-01

- 구조화된 작업 결과에서 Promise resolve와 실제 성공을 구분하도록 Test Manager의 success predicate, failure projection, UI success gate를 강화한다. `completed+succeeded`만 성공으로 허용하고 `failed`, `partial`, `delivery_failed`, 모순된 terminal 상태가 상위 consumer와 UI에서 실패로 유지되는지 검증한다. 관리 원본을 먼저 수정·검증한 뒤 활성본에 반영하며, 이 Skill 개선이 끝나기 전에는 `grist_video_maker` 제품 코드를 수정하지 않는다.
- 앱 테스트 조작 API를 딥링크·스와이프·텍스트 입력·터치·검증·앱 재시작의 필수 명령으로 제한하고 복합 동작은 이 명령의 조합으로만 만든다. 각 동작에는 결과 검증을 결합하고 검증 predicate가 동작 전 false, 동작 후 true인지 확인한다. 새 테스트는 병합 전에 반복 실행이 모두 통과해야 한다.

## 2026-10-05

- 검증 실패를 단순 중단 조건으로 취급하지 않고, 사용자 개입 없이 재현·계측·개선할 수 있는 경계를 모두 소진하는 개선 조건으로 사용한다. 브라우저의 `ended`, `buffered`, timer, message 전달처럼 합성 가능한 lifecycle 경계는 사용자에게 넘기기 전에 production-shaped 전체 흐름에서 자동 검증한다.
- 사용자 입력은 실제 사용자 프로필·인증·trusted UI 동작처럼 Agent가 수행할 수 없는 최종 경계만 남았을 때 요청한다. 그 전에는 분리된 단계 테스트를 실제 producer부터 안전한 authoritative consumer adapter까지 연결하고, validator 실패 항목을 작업 대기열로 바꿔 계속 수정·계측한다.
- mock은 개별 함수의 비정상 입력 처리와 함수 간 전달 payload·타입·순서·횟수 검증에만 사용한다. 최종 전체 흐름은 mock 없이 배포 artifact를 실제 runtime에 로드하고 Playwright 또는 Chrome 원격 디버깅 계열의 사용자 입력 경계로 시작해 authoritative consumer 결과까지 확인하도록 Test Manager 절차를 명시한다.
- Playwright/CDP 실제 브라우저 조작은 개별 함수 입력과 함수 간 mock 전달 검증이 전부 통과한 뒤에만 시작한다. 실제 흐름에서 예상하지 못한 오류가 나오면 누락 경계를 동일 계열 workflow에 재사용할 수 있는 상태축·계약·mutant로 보편화해 Skill에 반영하고, 개별 함수 단계부터 전체 검증을 다시 실행한다.
- 전체 offline suite의 병렬 부하에서 고정된 scheduler turn 횟수만 기다리는 polling이 일시적으로 timeout된 사례를 반영한다. 비동기 전파 관찰은 production deadline을 반영한 wall-clock 제한 또는 권위 있는 완료 신호를 사용하고, 짧은 event-loop 반복 횟수를 성공·실패 계약으로 사용하지 않는다.
- 최종 실브라우저 전체 흐름의 조작 도구를 Playwright 또는 직접 CDP script에서 Stagehand v4로 변경한다. 개별 함수 입력과 함수 간 전달이 모두 통과한 동일 revision에서만 Stagehand를 시작하고, 사용자 접속·기존 로그인 세션 인증·확장 토글·Suno 이동·재생 버튼을 Stagehand `observe`/`act`/`extract`로 수행한 뒤 Windows Downloads의 실제 MP3 bytes를 읽는다. Stagehand가 내부적으로 CDP를 사용하더라도 테스트 조작 API로 Playwright나 직접 CDP 명령을 섞지 않는다.
- Stagehand 규칙이 문서에만 남지 않도록 runtime evidence의 driver·provenance를 기계적으로 검사하고, Playwright·직접 CDP·mock·substitution·중간 단계 시작 mutant를 거부하는 전용 gate self-test를 추가한다.
