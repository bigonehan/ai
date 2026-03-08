# problem
- `check-code` 스킬이 요청한 운영 원칙(input/draft 기반 체크리스트 생성 + 언어별 실행 검증)을 따르지 않는다.
- `plan-project-code`, `build-project-auto` 스킬이 현재 운영 대상에서 제외되어 `~/temp_bin`으로 이동이 필요하다.
- 존재하지 않는 레거시 스킬/경로 참조가 남아 있다.

# tasks
- `check-code/SKILL.md`를 다음 규칙으로 교체한다:
  - `input.md` 또는 `./.project/draft.yaml` 기반으로 `./.project/check_list.md` 생성
  - 체크리스트 형식: `- [ ] ${입력} -> ${출력} : 기능설명`
  - 언어별 검증 실행: Rust=`cargo`, JS 계열=`vitest`, `playwright`
- `/home/tree/ai/skills/plan-project-code`, `/home/tree/ai/skills/build-project-auto`를 `/home/tree/temp_bin/`으로 이동한다.
- 스킬 문서 전반에서 존재하지 않는 레거시 스킬/경로 참조 문구를 제거한다.

# check
- `rg`로 `check-code/SKILL.md`에 `check_list.md`, 체크리스트 형식, `cargo`, `vitest`, `playwright` 반영 여부 확인
- `ls`로 두 스킬 폴더가 `/home/tree/temp_bin/`으로 이동되었는지 확인
- `rg`로 레거시 참조(`plan-drafts-code`, `domain-create`, `/home/tree/ai/skills/plan-drafts`) 제거 여부 확인
