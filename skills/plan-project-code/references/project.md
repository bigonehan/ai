# info

- name : 프로젝트 이름 
- description : 프로젝트 설명 
- spec : 프로젝트에 사용되는 라이브러리 (예시:rust, rataui, tokio)
- goal : 프로젝트 설계 목표 

## rule

- project가 가지고 있는 규칙들, goal과 description을 통해서 채울것 

## features
추가되어야 하는 기능들, 사용자의 입력을 받고  명령 | 실행/변경 파일 | 파생 결과 형식으로 제시할것
```
출력 예시:
1. 프로젝트 정보 입력 | .project/project.md 생성 | 설계 기준 문서 확보
2. features 리스트 입력 | project.md features 항목 업데이트 | 구현 범위 확정
3. features 항목 분석 | .project/features/work/기능이름/draft.yaml 생성 | 기능별 구현 명세 확보
4. draft.yaml 읽기 | 각 기능 폴더 내 코드 파일 생성/수정 | 기능 구현 완료```

## structure
최소한 생성되어야 하는 파일들 리스트와 Path 위치 
```
출력 예시:
- src/main : main 함수
- src/assets : prompt, config, style, templates에 관한 파일들이 있다.
- src/tmux : tmux 메시지 전송 관련 함수 모음 (세션 관리 아님)
- src/ui : ui 관련 함수 모음
- src/config : `assets/config/config.yaml` 관련 파일 관리
``` 
# Domains
프로젝트에서 사용할 도메인 리스트 `build_domain` Skill을 활용해서 채워놓는다.
### domain
필요한 도메인을 다음과 같은 형식으로 작성한다. 필요한 갯수만큼 생성한다 
name : 도메인 이름 
description : 설명 
state : 도메인이 가지는 상태 
action : 도메인이 할 수 있는 동작 
rule : 도메인이 가지고 있는 제약조건, 규칙
variable : 도메인이 가지고 있는 정보, 변수 
```
출력예시 :
### prompt

- **name**: `prompt`
- **Description**: llm에게 내리는 지시, 명령 모음
- **States**: 추가됨, 삭제, 작업중, 완료
- **Actions**: 생성, 삭제, 수정
- **rule**:
    - llm에게 내리는 명령어가 담긴 파일
    - `소스 파일/assets/templates/prompts` 에 텍스트 파일로 담는다.
- **variable**:
    - msg : 메시지
```


# Stage
프로젝트가 제공하는 논리적인 작업 흐름 단위를 나열한다
```
출력예시 :
## stage list

1. project 관리 모드 진입
2. draft 생성
3. tasks 생성
4. 병렬 처리 작업
```
# UI
프로젝트가 제공하는 화면 단위를 나열한다.
```
출력예시 : 

### project menu

- description: 프로젝트 목록을 보여주는 메인 화면
- flow: project info, detail menu
- domain: project
- action: 프로젝트 생성, 프로젝트 추가, 프로젝트 삭제, detail 진입
- rule:
    1. `configs/project.yaml` 에서 project 목록을 가져와서 표시
    2. project 목록은 리스트 형식으로 프로젝트 이름, 생성 날짜, 최근 수정날짜, 설명이 보임
    3. project list에서 하나의 item을 선택 후 enter키를 누르면 detail menu에 진입
```

# Step
- 사용자가 프로젝트를 통해서 어떤 작업을 하는지를 `#도메인>##기능` 순으로 나열한다 
- 이때 가급적이면 사용자의 입력이나 행동으로 생성되는 파일이나 결과가 보이게 작성한다.
```
출력예시 : 프로젝트 생성 기능의 경우 
## 프로젝트
### 프로젝트 생성

1. Main menu에서 project 생성 버튼 선택
2. `plan-project-code` Skill 실행 명령과 함께 project 이름, 설명, path(기본값: 현재 위치) form 표시
3. 사용자가 입력 완료 시 `project.md` 파일을 `project path/.project/` 위치에 생성 명령
4. `domain_create` Skill 실행 명령으로 `project.md` 내부 도메인 확정 및 생성
5. `domain 폴더(기본값 ./src/domains)` 에 기본 도메인 파일 생성. 이때 project.md의 언어에 맞는 확장명 사용
6. `project.md` 내 `language`와 `framework`에 맞는 프로젝트 초기화 실행
7. 생성된 폴더 내부가 비어있을 때 `jj git init` 으로 작업 초기화
8. 생성된 폴더 내에 `.project` 폴더가 있을 경우 `./project/project.md` 파일을 로드하여 기존 프로젝트로 처리
```

# Constraints
프로그램 내에서 제약사항들을 표시한다. 
```
출력예시 :
## ui

- 각 pane은 활성화/비활성화 상태를 스타일 변수인 active, inactive로 적용한다
- 각 대화창은 기본 입력값이 존재한다
- 모든 pane은 화살표로 포커스 이동
- list item 선택 시 esc는 list에서 pane으로, 다시 esc는 pane 비활성화 (2회)
- task 상태(inactive/active/clear)는 색상 또는 모양으로 실시간 표시
```
