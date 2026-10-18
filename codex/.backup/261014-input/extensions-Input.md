## tasks
- [x] `/home/tree/extensions` 작업 관리를 Grist 문서 `au1EbKLtZbrW`의 `Table7` `관리 프로그램` 셀에 기록된 Obsidian 관리 파일 경로를 기준으로 처리한다.
- [x] 완료된 사용자의 요청

## 2026-10-09

- [x] 전역 스킬·규칙 개편과 충돌하는 하위 프로젝트의 legacy 관리 산출물과 규칙을 전수 정리한다. 실행 가능한 제품 경계는 보존하고, 구형 `.project`·`.agents`·`.rc-cache.json`·`terminal-capture.txt` 및 폐기된 작업 상태 참조는 제거하거나 최신 `AGENTS.md`·`.ai/changes` 절차로 갱신한다.
- [x] 정정: `.ai/changes/`의 기존 기록 내용과 파일은 삭제하지 않고 보존한다. 같은 날짜의 새 변경은 기존 일일 기록에 추가하며, 기존 시간형·과거 기록은 migration 대상에서 제외한다.
- [x] `.ai/changes/YYMMDD.요약명.md`는 Asia/Seoul 날짜마다 하나만 사용하고, 같은 날짜의 후속 작업은 기존 파일의 하나뿐인 `# jobs` 아래 하위 item으로 추가한다.
- [x] `initial_project`의 독립 초기화 Script가 CDP 없이 Grist HTTP API를 사용하고, Skill-local `.env`에서 Grist 접속 설정을 읽으며 해당 `.env`가 Git 추적되지 않도록 한다.
- [x] [취소됨] 이전 해석: 사용자 입력의 원본은 프로젝트 `Input.md`로 통합하고 `.ai/plan`은 staged 실행계획 상태로만 유지한다.
- [x] 정정: plan 관련 체계 전체를 구버전으로 폐기한다. 사용자 요구는 `Input.md`, 구현·검증 기록은 `.ai/changes/`에만 기록하고, plan Skill·plan 규칙·프로젝트 plan 파일과 참조를 제거한다.
- [x] 모든 프로젝트 폴더를 순회해 `.ai/plan` 폴더와 내부 자료를 제거하고, 각 프로젝트의 `.ai/changes/YYMMDDHHmm.md` 기록을 날짜별 `YYMMDD.제목.md` 하나로 통합한다.

## 2026-10-14

- [x] 반복 미해결 오류와 새 에이전트가 알아야 할 구조적 특성은 프로젝트 `AGENTS.md`의 `# tip` 아래 한 줄로 기록하도록 전역 규칙에 넣는다.
- [x] Skill 이름과 폴더명은 같은 `동사-명사`(소문자·하이픈)로 짓도록 전역 규칙에 넣고, 기존 Skill 이름을 모두 바꾼다. `test-manager`는 `verify-work`로 바꾼다. 이후 extensions의 `AGENTS.md`가 최신 이름을 쓰는지 확인한다.
- [x] 사용하지 않는 Effect TS Skill(`design-domain`, `review-effect`)과 `configure-environment`는 `skill-backups/legacy/`로 옮긴다.
- [x] `route-failure` Skill은 삭제한다.
- [x] Grist Table26 완료일(`F`) `AttributeError`를 해결한다.
- [x] 현재 상태의 extensions와 ai 저장소를 `feat: save legacy`로 커밋한다.
- [x] extensions 루트 작업은 작업관리기 프로젝트로 보고 그 기록을 갱신한다.
- [ ] 보류: `.ai/changes` 기록 기능을 `access-grist`에서 분리해 `record-changes` Skill로 옮기고 `work-project`가 사용하게 한다.
- [ ] 보류: Obsidian 문서 경로는 프로젝트 `AGENTS.md` 대신 Grist Table7 `Obsidian_Id`에서 읽는다(`grist.py obsidian` 명령, `AGENTS.md`의 경로 줄 제거).
- [ ] 보류: Obsidian 헤더 단위 읽기·쓰기를 맡는 `access-obsidian` Skill을 만든다.
