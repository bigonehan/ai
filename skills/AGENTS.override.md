# Override Rules For Skill Trigger

## Priority Branch (Highest First)
1. 메타 운영 파일 수정 요청이면 `plan-project-code`, `add-function`을 트리거하지 않는다.
2. 메타 운영 파일 수정이 아니고, 코드 구현/수정/작성 요청이면 기존 `plan-project-code`/`add-function` 트리거 규칙을 적용한다.

## Meta Operation Scope
- `AGENTS*` 파일 (`AGENTS.md`, `AGENTS.override`, `AGENTS.override.md` 등)
- 모든 스킬 정의 파일 (`**/SKILL.md`)
- 설정/구성 파일의 정책/문구 수정 (`*config*`, `settings*`, `*.yaml`, `*.yml`, `*.toml`, `*.json`)
- 문서 파일 수정 (`*.md`, `*.txt`)
- 코드 의미 변경 없는 단순 단어 치환/문구 교정

## Enforcement
- 메타 운영 파일 수정 요청에서는 최소 변경으로 해당 문서/설정만 수정한다.
- `references/problem-analysis.md`, `./project/project.md` 선행 게이트를 강제하지 않는다.

## Shell Performance Rule
- 단순 문자열 검색/파일 탐색은 셸 래핑 없이 `rg`를 직접 실행한다.
- 문자열 치환/파이프라인/다단계 텍스트 처리는 `bash -c` 한 번으로 묶어 실행한다.
- `fish -ic`는 fish 의존 기능이 필요한 경우에만 사용하고, 반복 호출을 피한다.
