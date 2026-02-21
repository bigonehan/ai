---
name: plan-drafts-code
description: "./project/project.md를 바탕으로 draft.yaml을 생성하는 skill"
---

# 기본 규칙 
1. task의 이름은 snake_case를 따른다.
2. 전체 복사하지 않는다
# task 분리 기준
- task의 갯수는  `./.project/project.md`에서`## features` 항목보다 많거나 더 적을 수 있다.
- 함께 성공하거나 함께 실패해야 하는 동작은 하나의 task로 묶는다
- 독립적으로 성공/실패해도 괜찮은 동작만 분리한다
- 다른 파일을 건드리더라도 트랜잭션이 같으면 하나의 task로 유지한다
# 순서 
1. `./.project/project.md`에서`## features` 항목을 읽는다 
2. `./.project/drafts_list.yaml`이 있는지 확인, 없으면 생성한다. 
3. `references/drafts_list.yaml` 형식을 바탕으로 `./.project/drafts_list.yaml`의 `planned` 항목에  `./.project/project.md`에서`## features` 항목에 어울리는 기능을 리스트 item으로 추가한다. 
4. 추가된 `planned` 항목 리스트를 읽고 한줄씩 아래의 drafts 세부 작성 기능을 수행한다
## drafts 세부 작성 
3. `./.project/feature/` 폴더에 기능명을 요약한 폴더명을 생성한다. 
4. `references/draft.yaml`을 형식을 바탕으로  `./.project/feature/생성된 폴더명/draft.yaml` 를 생성한다.
5. `name` 과 `description`을 채운다 
6. 적합한 도메인을 `drafts_list.ymal의 domain`을 보고 채워 놓는다. 
7. 적합한 도메인이 없다면 도메인 명을 `util`로 잡는다.
8. 기능상 의존하는 작업이 있으면 `depends_on`에 추가한다.
9.   `./.project/project.md`에서 `rule`을 읽고 현재 drafts에 적합한것이 있다고 판단되면 추가한다.
# drafts_list.yaml 생성 규칙 
## features.domain 처리
- 해당 기능의 tasks에 포함된 domain들의 합집합으로 채운다
- 예: tasks에 [player], [experience]가 있으면 features.domain: [player, experience]
# draft.yaml 생성 규칙 
## type labeling
type은 아래 기준으로 판단한다:
  - calc: 입력을 받아 결과를 반환하고 외부 상태를 변경하지 않는 순수 계산
  - action: 파일 생성/수정, DB 변경, 외부 호출 등 부수효과가 있는 작업
## Step 부분 처리하기 
1. 파일 입력 동작 출력의 형태를 가진 한줄로 작성한다.
2. 한번에 작업할 수있는 최소한의 단위로 작성한다
3. 검증 조건, 필터링은 맨처음에 배치한다. 
## scope 부분 처리하기
- scope는 이 task가 생성하거나 수정할 파일 경로를 루트 기준으로 작성한다.
- 아직 존재하지 않는 파일도 생성 예정이면 포함한다.
## domain 부분 처리 
- drafts_list.yaml의 domain 목록을 보고 이 기능이 관여하는 도메인을 선택
- 적합한 도메인이 없으면 util로 지정
## 완료 작업 
- 생성 완료후 이름을 짧게 camelcase로 요약해서 `drafts_list.yaml`의 `planned` 항목에 리스트 item으로 추가한다. 
# 출력물
1. `./.project/feature/생성된 폴더/draft.yaml`이 여러개 생성이 된다 
2. 생성된 폴더는 여기서 `project.md` 의 `features` 항목의 기능을 함수명으로 요약한 것을 말한다. 
