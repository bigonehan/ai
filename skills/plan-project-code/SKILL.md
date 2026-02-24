---
name: plan-project-code
description: 모호함을 제거해 구조화된 프로젝트 설계 파일을 (`./project/project.md`)을 만든다.
---

# Coding Design Pipeline

- 추론 가능한 항목은 먼저 채우고 확인만 받는다.
- `references/project.md` 파일 형식을 바탕으로 `project.md` 파일을 만드는것이 목표이다.
- 사용자 언급이 없다면 .Flow의 stage는 features에서 자연스럽게 흘러가는 순서로 추론한다.
- `project.md`가 생성되기 전에는 코드 편집/의존성 설치/빌드/테스트를 시작하지 않는다.
- 모호한 코딩 요청을 단계적으로 구체화하여 저성능 모델이 판단 없이 구현할 수 있는 설계문을 생성한다.
- "더 이상 모호한 것이 없을 때까지 구체화한다"
- 설계는 사람과 함께, 구현은 모델에 위임한다.
- 해석이 여러 개면 → 제시하고 선택받기, 혼자 결정하지 않는다
- 더 단순한 방법이 보이면 → 말하기
- 불명확하면 → 멈추고 질문하기
- 코드명/명령명 네이밍은 공통 규칙 `/home/tree/ai/skills/feature_architecture_rules/SKILL.md`를 따른다.

## Absolute Sequence
- 한 줄 입력을 받아 최소한의 QA로`현재폴더/.project/project.md` 파일을 생성한다. 
- 구현 요청 시 `./project/project.md`가 이미 존재하고 요청 기능(또는 feature)이 project에 명시되어 있으면 `plan-project-code`를 재실행하지 않고 `plan-drafts-code`로 즉시 전환한다.
- skill을 실행후 질의응답을 통해 채워지는 항목은 `info`의 ` name, description, spec, goal, feature가 채워진다.
- 원하는 기능들을 추가적인 입력으로 받아 리스트로 작성해서 featrues에 추가한다. 
- features 한 줄로 된 리스트로 작성한다. `명령 | 실행/변경 파일 | 파생 결과` 형식이다.
- project.md를 만든다음 기능 추가 이후에 도메인 추가를 한다
- 도메인은 별도 `build-domain` 스킬로 처리한다. `project.md의 domains`부분을 채운다

## Hard Guardrails 
- `./project/project.md`가 이미 있고 요청 기능이 project에 포함된 경우에는 질문을 통해서 설계 단계를보충하거나 건너뛰고 `plan-drafts-code` 구현 단계로 즉시 전환한다.
- 중간에 구현을 먼저 시작했음을 감지하면 즉시 중단하고, 누락된 `project.md`를 먼저 작성한 뒤 재개한다.
- 모호함 있으면 질문한다, 모호함 해소 후 2단계 진행

## Pre-Implementation Gate
아래 질문이 모두 충족되기 전에는 구현 금지:
1) 모호함 해소 완료
2) `./project/project.md` 파일 생성/최신화 완료
3) 완료 기준(검증 항목) 문서화 완료
4) "domain-create" 스킬이 실행되었는가?
5) QA가 1회 왕복으로 끝났는가?
6) features가 3~7개 사이인가?
7) Flow의 stage가 features 흐름과 일치하는가?
8) Constraints와 Verification이 미완으로 명시되어 있는가?


## 기능 문서 작성
"[도메인]이 [동작]했을 때 [결과]가 된다"
형태로 검증 조건을 명시한다.
## 제약조건
정리된 비즈니스 규칙을 제약 리스트로 정리
- 유닛테스트 등으로 검증이 가는한 제약조건을 포함할것 
- 방향성 제약 (성능 최적화, 보안 고려 등)만 포함

## 결과물

수집된 모든 정보를 `./project/project.md`로 저장한다.



## 완성 후
설계문이 완성되면 구현 전환 절차를 아래 순서로 수행한다.
사용자에게 `plan-drafts-code` 단계로 전환할것인지 묻고, 아래 명령을 실행한다

## Domains Output Contract
- `# Domains`는 반드시 `references/project.md`의 형식을 따른다.
- 요약형 문장(`제안 도메인`, `근거`, `책임`)으로 대체하지 않는다.
- 출력은 아래 블록 반복 형식만 허용한다.

```markdown
### domain
- **name**: `<domain_name>`
- **description**: `<설명>`
- **state**: `<상태 목록>`
- **action**: `<동작 목록>`
- **rule**:
  - `<규칙>`
- **variable**:
  - `<변수>`
```

- domain 내용 산출 시에는 `build_domain` 스킬 판단 결과를 사용하되, 결과 표현은 위 구조로 정규화한다.

## Template Copy Rule
- `references/project.md`를 템플릿으로 사용할 때는 대상 경로(`./.project/project.md`)에 먼저 복사한다.
- 복사된 파일에서 설명문/예시/주석을 제거한 뒤, 속성값만 채워 수정한다.
