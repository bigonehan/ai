# test_project 작업 규칙

## 범위

- 프로젝트 루트는 `/home/tree/extensions/test_project`예요.
- 허용 파일은 이 디렉터리 아래뿐이에요.
- 상위 디렉터리, 다른 child project, 전역 skill 파일은 수정하지 않아요.
- 검증 산출물은 `Input.md`, 이 파일, `.ai/changes/261009.test-project.md`, 명령 출력으로 관리해요.

## Table26 현재 단계

Table26 `$H`는 Choice 컬럼이고, 한 번에 하나의 한글 값만 저장해요: `초안`, `진단`, `교정`, `검증`, `대기`, `완료`. 영문 `SCOPE`·`DIAGNOSE`·`REPAIR`·`VERIFY`·`HANDOFF`·`DONE`은 별칭일 뿐 `$H`에 쓰지 않아요. `검토`와 개별 항목의 `RUNTIME`은 단계로 사용하지 않아요.

Table26 `$D`·`$I`·`$J`에는 지금 진행 중인 최신 항목 한 줄만 작업 행 안의 순번과 함께 써요(예: `3. 인증 단계 확립`). 1번부터의 전체 목록과 세부 내용은 같은 순번으로 날짜별 `.ai/changes`에만 기록해요.

## 현재 작업

- 단일 deliverable: TypeScript `plus(a: number, b: number): number` 구현과 단위 테스트예요.
- acceptance: `npm test`, `npm run check`, `npm run build`예요.
- 현재 단계: `$H=완료`.

## 완료 확인

implementation Agent는 진단→수정→검증을 한 번에 하고 결과를 일반 텍스트로 반환해요. 메인 세션이 diff와 테스트 출력을 직접 읽고 `Input.md` 요구와 대조해 Table26 `$H=완료` 또는 다음 bounded repair를 결정해요. 별도 completion-verification Agent는 외부 데이터 쓰기 계약 변경, 공용 Bridge 계약 확장, 사용자 요청일 때만 불러요.

