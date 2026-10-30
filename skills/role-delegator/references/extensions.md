# extensions 위임자 전용 절차

루트 `/home/tree/extensions/AGENTS.md`에서 옮긴 위임자(메인) 전용 절차다. 대행자·검증자는 읽지 않는다.

## 작업 관리

- 프로젝트를 새로 초기화할 때는 사용자에게 프로젝트 타입(`업무`, `공방`, `게임`, `보조` 중 택1)을 물어 `manage-project` 초기화 절차(`initialize-project --project-type <타입>`)로 Obsidian 아래에 `구현/[프로젝트타입]/YYMMDD.<프로젝트 이름>/<프로젝트 이름>.md`(`구현/업무/261008.작업관리기/형식/작업관리기.형식-문서.md` 형식, 상단 `[[구현/[프로젝트타입]|📁]]`, `# 🔨` 아래 `[[<프로젝트 이름>-개발.md]]`, `# 📆` 아래 `## YYYY / ### M월 / MMDD : 문서 생성`), `구현/[프로젝트타입]/YYMMDD.<프로젝트 이름>/일지/<프로젝트 이름>-개발.md`(`구현/업무/261008.작업관리기/형식/작업관리기.형식-개발.md` 형식, 상단 `[[<프로젝트 이름>|📁]]`, `# 📆` 아래 `## YYYY / ### M월`), `구현/[프로젝트타입]/YYMMDD.<프로젝트 이름>/노트/` 폴더를 만들고, `구현/[프로젝트타입]/[프로젝트타입].md`의 `# 🗄` 아래에 `[[<프로젝트 이름>]]`을 추가한다.
- 프로젝트 Obsidian 개발 문서는 Grist `au1EbKLtZbrW` `Table7`의 `Obsidian_Id`(Vault 상대 경로 `구현/[프로젝트타입]/YYMMDD.<프로젝트 이름>/일지/<프로젝트 이름>-개발.md`)다. 형식 템플릿은 `구현/업무/261008.작업관리기/형식/`의 `작업관리기.형식-문서.md`·`작업관리기.형식-개발.md`·`작업관리기.형식-Agents.md.md`·`작업관리기.형식-changes.md`·`작업관리기.형식-architecture.md`·`작업관리기.형식-performance.md`다.
- 사용자 요청은 `grist request-record`(오늘자 `.ai/changes/YYMMDD.제목.md`의 `# 요청`·Obsidian `# 📆`)로 기록하고, `# 세부 사항` 한 줄(`[📐|✒️|⌨️|🚿|✏️|🌍] 요약`)마다 `grist task-add`로 Table26 행을 만든다. changes는 Asia/Seoul 하루 1파일이며 그날 첫 요구의 10자 미만 제목을 바꾸지 않는다.
- 프로젝트 작업은 `manage-project` Skill 절차대로 진행한다(미초기화면 그 초기화 절차부터). Table7·Table26·Obsidian 관리 기록 쓰기는 이 Skill의 범위 안에서 허용된다. 형제 프로젝트 제품 파일은 수정하지 않는다.
- 초기화된 하위 프로젝트 `AGENTS.md`는 `구현/업무/261008.작업관리기/형식/작업관리기.형식-Agents.md.md` 형식을 따르며 맨 위에 아래 `# 상태` 헤더를 가진다. Stop hook과 `grist gate`·`task-complete`가 이 형식을 읽는다.

```
# 상태
### grist
ID : <Table7 ID>
### obsidian
문서 : <obsidian 안에 만들어진 개발 문서 Path>
### 실행
runtime : <진입지점 혹은 실행 방법 한줄 요약>
```
- 현재 흐름의 작업 행은 `.ai/changes/`의 `# 작업`과 `# 완료` 항목(`N. 항목 (#id)`)이 소유한다. `Complete_task`가 목표·출력 통과 항목을 `# 완료`로 옮긴다. 메인 세션 Stop hook은 모든 현재 행이 사용자 설정 `완료`이거나, `$H=확인`이고 `# 완료` 목록에 있을 때만 종료를 허용한다. 사용자가 Grist에서 쓴 `$H=완료`는 changes·gate·Jev 등 다른 기록의 미완 판정보다 우선하며, `grist sync`가 그 항목을 `# 완료`로 옮긴다.
- 행의 목표는 changes 항목 `## [구현] <제목> (#<Raw ID>)`의 `### 목표` 목록이 소유하고, Table26 `$K`(목표)에는 그 한 줄 요약만 둔다. 메인이 사용자 요구를 `[연결|기능|수정]` 줄로 구체화해 `grist task-add --goal-file <F> --summary "<한 줄>"`(새 행) 또는 `grist task-goal <id>`(초안 정정)로만 쓰고, 읽기는 `grist goal <id> --project <dir>`로만 한다. 요구에 없는 조건을 넣지 않는다.

## 버전·빌드·배포

- 제품 동작이 바뀐 흐름마다 patch를 `0.0.1` 올린다(세그먼트 `0`~`99`, `0.0.99 → 0.1.0`). 버전을 가진 파일(`package.json`, `manifest.json`, `public/manifest.json`, `Cargo.toml` 등)을 같은 버전으로 맞추고, 빌드 산출물 `manifest.json`의 버전도 확인한다. 문서·규칙만 바뀌면 올리지 않는다.
- 빌드 과정이 있으면 빌드 산출물만, 없으면 실행에 필요한 파일만 배포한다. 대상 폴더를 통째로 지우지 않고, 대상의 사용자 설정·런타임 데이터 파일은 덮어쓰거나 지우지 않는다.
- 실행 중인 데스크톱 앱(Eagle, DaVinci Resolve, Blender 등)에 새 코드를 적재하는 일은 Agent가 PowerShell 등으로 직접 한다. 사용자 작업 중이라 직접 할 수 없을 때만 `$D`에 사유를 쓰고 사용자에게 요청한다. `$H`는 직접 바꾸지 않으며, 사용자가 Grist에서 `$H=완료`로 처리하면 그대로 종료한다.

## 배포 복사

- 기능 추가·개선판은 실행 가능한 결과물을 `/mnt/h/code/<프로젝트 디렉터리명>`(Windows `H:\code`)에 한 번 `cp`하고 exit code만 확인한다. `test/tmp`, `__pycache__`, 로그, 비밀값은 제외한다. 마운트 사전검사는 하지 않으며, 실패하면 복사만 `deployment-deferred`로 기록하고 나머지 작업을 계속한다.
- ZIP은 기본 동작이 아니다. `blender_camera_control`만 `/mnt/h/code/blender_camera_control_<버전>.zip`(내부 최상위 `blender_camera_control/`)을 만든다.
- Obsidian 플러그인은 `/mnt/c/Users/tende/OneDrive/일기장/.obsidian/plugins/<프로젝트명>`에 `manifest.json`, `main.js`, 있으면 `styles.css`를 복사한다. `data.json`은 덮어쓰지 않는다.

## tip
- 문서·규칙 전용 `task-confirm` 행에는 `.ai/gate/<id>.json`이나 `▶`/`○` 구현 단계가 없을 수 있다. `Complete_task`는 최신 성공 `#### 확인`의 실제 요약·상세와 `task-confirm --evidence`의 목표별 `- 근거 N` 줄을 증거와 완료 목록 재료로 사용하며, 근거가 없으면 차단한다.
- `grist update`·`task-update`는 자동 `$H=완료`를 거부한다. `Complete_task`가 `# 완료` 이동과 `$H=확인`을 검증하고, 사용자 Grist 변경 뒤 `grist sync`가 최종 `$H=완료`를 최우선으로 반영해 항목을 `# 완료`로 옮기고 Obsidian `# 📍`를 기록한다. 테스트는 `GRIST_OBSIDIAN_FILE`로 Obsidian 사본을 대상으로 한다.
- 여러 하위 프로젝트를 함께 고치는 작업은 코드 묶음마다 프로젝트별 Codex 대행자(`dispatch-agent --agent codex`)로 병렬 위임하고, 규칙·Skill 문서 묶음만 메인이 `rg -n`으로 필요한 줄만 읽어 직접 고친다. `grist` CLI의 필수 인자는 `access-grist` 명령 표를 참조해 재호출 없이 한 번에 실행한다.
- 메인 세션의 토큰 낭비는 대부분 스크래치패드 `node $S/*.mjs`로 우회한 직접 브라우저 조작·로그 폴링(261030 cartoon_maker 메인 741회 호출)과 스크린샷 반복 Read(46장)에서 났다. main-edit-gate가 이를 막으므로 deny가 나오면 우회하지 말고 실측 조건을 넣어 `dispatch-agent`에 맡긴다.
