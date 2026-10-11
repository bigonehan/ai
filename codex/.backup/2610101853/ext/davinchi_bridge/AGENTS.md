# davinchi_bridge 작업 규칙

## 버전업
- 신규 배포 버전은 `0.0.1`부터 시작한다.
- 기능 개선·추가 또는 수정 배포마다 patch를 `0.0.1` 올린다.
- 각 세그먼트는 0~99로 유지하며 patch 99 다음은 minor를 올리고 patch를 0으로, minor 99 다음은 major를 올린다.
- `package.json`, `resolve/manifest.xml`의 버전을 일치시킨다. 빌드된 Resolve package.json도 같은 버전이어야 한다.

## 코드 복사
- 배포 루트는 Windows `H:\code\davinchi_bridge`, WSL `/mnt/h/code/davinchi_bridge`이다.
- 기능 추가·개선·버그 수정이 끝나면 사용자에게 재실행을 요청하지 말고 자동으로 `npm test` → `npm run build` → `npm run deploy`를 실행한다.
- 배포 후 Windows PowerShell이 사용 가능하면 `H:\code\davinchi_bridge\install-resolve.ps1`을 `ExecutionPolicy Bypass`로 실행해 `%PROGRAMDATA%`의 Resolve Workflow Integration Plugin을 갱신하고 SHA-256을 확인한다. WSL에서 PowerShell을 사용할 수 없으면 `/mnt/c/ProgramData/Blackmagic Design/DaVinci Resolve/Support/Workflow Integration Plugins/com.tree.davinchi_bridge`에 `dist/resolve`를 직접 복사하고 모든 파일의 SHA-256을 비교한다. 두 경로 모두 실제 갱신을 실행하며, 설치 명령만 보고 멈추지 않는다.
- Resolve가 실행 중이면 자동화 가능한 범위에서 Davinchi Bridge Workflow Integration을 종료한 뒤 설치하고 Resolve를 다시 시작한다. 실행 중인 사용자 작업을 임의로 중단할 수 없으면 설치 파일만 갱신하고 `runtime-unverified`로 남긴다.
- 위 자동 갱신 절차는 기능 변경마다 한 번만 수행하며, 이미 생성된 동일 버전의 배포 산출물을 반복 설치하지 않는다.
- `resolve/`에는 Resolve 실행 파일, `src/`, 공식 설치본에서 가져온 `WorkflowIntegration.node`를 포함한다. 별도 Eagle 플러그인을 만들거나 배포하지 않는다.
- README와 Windows 설치 스크립트를 함께 배포한다. 테스트, 임시 증거, 개발 전용 파일은 배포하지 않는다.
- Resolve 설치 대상은 `%PROGRAMDATA%\Blackmagic Design\DaVinci Resolve\Support\Workflow Integration Plugins\com.tree.davinchi_bridge`이다.
- 원본/배포본/Resolve 설치본의 SHA-256을 비교한다. 런타임 상태·사용자 설정을 덮어쓰거나 삭제하지 않는다.
- 영구 동작 기본값은 Git 소유의 `src/config.cjs`에서 관리한다. 실행 중 감시 목록과 처리 기록은 `%APPDATA%\davinchi_bridge`에 저장하며 Git 설정과 구분한다.

## 동작과 검증
- Eagle 접근은 기존 `../grist_video_maker/eagle-data-bridge`만 사용한다. `GET /events/sequence` SSE로 연결 중 새로 도착한 영상 신호만 받으며 Resolve 측에서 Eagle 폴더 목록을 주기 조회하지 않는다.
- Eagle Data Bridge는 현재 라이브러리의 모든 `Sequence` 폴더를 확인한다. UI 선택 폴더에 의존하지 않는다. `Sequence` 바로 위 폴더를 Root 프로젝트 출처로 기록하며, 그룹 루트나 물리 `.library/images` 경로를 프로젝트로 쓰지 않는다.
- Eagle Root 출처와 Resolve 프로젝트 이름/ID를 비교하거나 프로젝트를 자동 전환하지 않는다. 신호 도착 당시 현재 Resolve 타임라인에만 배치하며 처리 중 타임라인 전환은 취소 사유다.
- 최초 연결·재연결·라이브러리 전환 시 기존 영상은 기준 목록이다. 연결이 끊기면 대기 작업을 취소하고, 종료/오프라인/일시정지 중 추가된 영상을 재생하지 않는다.
- SDK 항목 추가 이벤트가 없어 Eagle 측에서만 1.5초 diff와 파일 안정화 검사를 한다. SSE heartbeat는 Resolve 작업을 유발하지 않는다. 상태 기본값은 `Eagle 신호 대기`다.
- `Shot` 영상 트랙 이미지와 새 영상 첫 프레임이 하나만 일치하면 Shot과 같은 시작·끝 프레임에 배치한다. 이미지는 보존하고 위의 잠기지 않은 활성 빈 영상 트랙을 쓰며 점유 중이면 같은 구간의 더 위 트랙/신규 트랙을 쓴다.
- Shot 없음·불일치는 배치 생략한다. 동명 Shot 트랙·복수 이미지 일치·이미지 읽기 실패는 보류한다. 타임라인 끝 배치 fallback을 추가하지 않는다.
- 일치 영상은 재인코딩·배치용 사본을 만들지 않고 원본을 Media Pool에 그대로 가져온다. `AppendToTimeline`의 `startFrame: 0`, `endFrame: Shot 길이 - 1`, `recordFrame`으로 Resolve 내부에서 긴 영상의 사용 범위만 조절한다. 원본이 Shot보다 짧으면 자동으로 마지막 프레임을 복제하지 말고 배치를 보류한다.
- `Sequence 신호 Test`는 Eagle Sequence 영상 파일 선택 → Eagle의 `POST /commands/test-sequence-signal` → 해당 연결에만 preview-only SSE → 실제 공용 소비자/`prepareArrival` 비교까지 실행한다. Media Pool 가져오기·변환·트랙 생성·타임라인 배치·처리 기록 변경을 하지 않는다.
- 배치 전 `GET /queries/sequence-item`으로 신호의 라이브러리·Sequence·ID·실제 경로·크기·mtime을 재확인한다. UI 폴더 선택 변경만으로 취소하지 않는다.
- 중복 실행/배치를 막는다. 오래된 pending은 자동 재개하지 않는다. 이미 제출했으나 결과가 불확실한 기록은 실제 배치 결과 확인 없이 재전송하지 않는다.
- 표준 `npm test`에는 신호/HTTP/실제 UI loader 연결·Shot 배치·Windows FFmpeg 실파일 검사를 포함한다. Linux의 Windows 전용 skip과 실제 Resolve 런타임 확인을 구분한다. 가짜 Resolve API 검사를 실제 타임라인 성공으로 보고하지 않는다.
- 테스트는 독립된 가짜 Eagle/Resolve 어댑터를 사용하고 사용자 타임라인을 테스트 목적으로 변경하지 않는다. 검증용 계약·증거 파일은 만들지 않고, 필요한 임시 파일만 `test/tmp/`에 만들었다가 작업 후 지운다.
- final 응답 직전 및 사용자 실행 요청 직전에 `fish -c 'nf -m "작업명 결과"'`를 실행하고 종료 코드를 확인한다.
