---
name: add-function
description: 프로젝트에 특정 기능을 추가할 때 사용하는 스킬. 설계 문서 게이트를 먼저 통과한 뒤 최소 변경으로 구현/검증까지 진행한다.
---

# Add Function

프로젝트에 새 기능을 추가하거나 기존 기능에 확장을 붙일 때 사용한다.

## Trigger
- `./project/project.md`가 이미 존재하고 요청 기능(또는 feature)이 project에 명시되어 있으면 설계 재작성 없이 즉시 구현 단계로 진입한다.
- 단, 요청 대상이 메타 운영/문서성 수정이면 이 스킬을 트리거하지 않는다. 범위: `AGENTS*`, `**/SKILL.md`, 설정/구성 파일(`*config*`, `settings*`, `*.yaml`, `*.yml`, `*.toml`, `*.json`), 문서 파일(`*.md`, `*.txt`), 단순 단어 치환/문구 교정.
- 트리거가 애매하면 구현을 시작하지 말고 기능 범위(추가 대상/완료 조건)를 1문장으로 확인한다.

## Absolute Sequence (Mandatory)
- `add-function`에서는 아래 분기 순서를 고정한다.
  1) `./project/project.md` 존재 여부와 요청 feature 포함 여부 확인
  2) 포함됨: 구현 시작
  3) `./project/project.md` 작성/최신화 -> 계획 요약 공유 -> `create-drafts` 생성 
- 위 순서 전에는 코드 편집/의존성 설치/빌드/테스트를 시작하지 않는다.

## Hard Guardrails
- 설계 우선 트리거가 활성화된 턴에서는 `./project/project.md` 생성 전까지 코드 편집/의존성 설치/빌드를 시작하지 않는다.
- 구현 요청이 함께 있어도 순서는 고정한다: `설계 확정 -> project.md 저장 -> 구현`.
- 구현 단계로 넘어갈 때는 사용자에게 `project.md 기준으로 구현 단계 전환`을 1문장으로 명시한다.
- `./project/project.md`가 이미 있고 요청 feature가 project에 명시되어 있으면 `plan-project-code`를 다시 실행하지 않는다.
- 중간에 구현을 먼저 시작했음을 감지하면 즉시 중단하고, 누락된 `project.md`를 먼저 작성한 뒤 재개한다.

## Pre-Implementation Gate
- 아래 분기 조건이 충족되기 전에는 구현 금지:
  1) 공통: 모호함 해소 완료
  2) 공통: `./project/project.md` 파일 존재 + 요청 feature 확인 완료
  3) `project.md` 미존재/미포함인 경우에만 `references/problem-analysis.md` 작성/최신화 완료
  4) `project.md` 미존재/미포함인 경우에만 `./project/project.md` 작성/최신화 완료
  5) 공통: 완료 기준(검증 항목) 문서화 완료

## 기능 추가 작업 규칙
- 기존 코드 구조/스타일을 우선 재사용한다.
- 요청 범위를 넘어서는 리팩터링이나 추상화 추가를 금지한다.
- 변경은 최소 단위 파일/함수로 제한한다.
- 기능 완료 기준을 만족하는 검증(테스트/린트/실행 확인)을 반드시 수행한다.

## 실행 체크리스트
- 추가 기능의 입력/출력, 영향 범위를 `references/problem-analysis.md`에 기록했는가?
- 구현 단계를 파일 단위로 `./project/project.md`에 명시했는가?
- 사용자에게 3~6줄 계획 요약을 공유했는가?
- 구현 시작 직전에 아래 문구를 1회 명시했는가?
  - `plan-project-code 게이트 통과: 문제 파악 + 계획문서 완료, 지금부터 구현 시작`
- 검증 명령 결과를 보고했는가?
