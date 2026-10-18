# 3단계: 검증·전체 흐름·완료 (`work-project` — Verify & Complete)

행이 `$H=검증`에 도달해 숨긴 사례(`task-hidden`), 빌드·배포, 실제 런타임 전체 흐름 검증(`flow-wait`), 사용자 확인 후 최종 완료(`task-complete`)를 수행할 때 이 문서를 끝까지 읽고 실행한다.

> **필독**: 검증 명령이나 테스트를 실행하기 전에 반드시 `~/ai/skills/verify-work/SKILL.md`와 `~/ai/skills/end-project/SKILL.md`를 끝까지 읽고 그 절차를 따른다.

## 1. 숨긴 사례 검증 (`Flow 5: Hidden cases`, main)

- 각 행이 `$H=검증`이 되면 에이전트가 보지 못한 새로운 입력·경계값·빈 값 사례로 `task-hidden`을 실행한다:
  ```bash
  ~/ai/scripts/bin/grist task-hidden <id> --project <dir> --purpose "<원하는 결과 라벨>" --test "<새 입력 테스트 명령>"
  ```
- 통과하면 `.ai/changes`의 `### 구현` 아래에 다음 형식으로 기록된다:
  ```markdown
  #### 확인 (#<id>)
  - 목적 : <원하는 결과 라벨>
  - 결과 : 성공|실패
  - 상세 : <함수명(message/body) ▷ 반환값 또는 실패 위치 한 줄>
  ```
- 실패하면 해당 행 에이전트에 수리 경계 1회로 재배정한다.

## 2. 버전·빌드·배포 및 전체 흐름 테스트 (`Flow 6: Flow test`, main)

1. **단일 릴리스 및 배포 복사**
   - 현재 흐름의 모든 행이 `$H=검증`이 된 뒤 전체 흐름 테스트 전에 메인이 한 번만 수행한다:
     - 제품 동작이 바뀐 경우 patch 버전을 `0.0.1` 올리고(`package.json`, `manifest.json`, `public/manifest.json`, `runtime-contract.js`, `Cargo.toml` 등 일치), 빌드 산출물(`dist/manifest.json` 등) 버전을 확인한다.
     - 실행 가능한 산출물을 `/mnt/h/code/<프로젝트 디렉터리명>`(Obsidian 플러그인은 `.obsidian/plugins/<프로젝트명>`)에 복사한다.
     - 새 확장 빌드를 공용 CDP 브라우저(`127.0.0.1:9222`, `/home/tree/.config/codex-run-cdp`)에 로드할 때는 `fish -c 'run_cdp --replace -e /mnt/h/code/<프로젝트명> --detach'`를 사용한다.
2. **실제 런타임 전체 흐름 실행 (`verify-work` + `run-cdp` + `access-stagehand`)**
   - 각 항목은 `focused/unit → production-boundary integration → built artifact` 순서를 거치며, 모든 행이 `검증`이 된 뒤 프로젝트 단위 실제 런타임 전체 흐름을 한 번 실행한다(`시작 동작 → producer → transport/handler → authoritative consumer → 최종 효과`).
   - Mock, 소스 문자열 검사, 빌드 성공, 단순 ACK, 빈 페이지는 사용자-visible 기능의 완료 증거가 아니다.
3. **5분 병목 감지 로깅 및 자동 깨우기 타이머 (`verify-work` 필수 절차)**
   - Stagehand/CDP 검증 실행 시 `STAGEHAND_STEP_LOG=1`을 켜서 `[stagehand-step]`(`cdp-attach`, `stagehand-create`, `page-resolve`, `session-ready`, `session-close`) 및 `[stagehand-bottleneck]` 중간 과정 로그가 남게 한다.
   - 검증·런타임 명령이 백그라운드 태스크(`task-*`)로 전환되면 **반드시 같은 턴에서 턴을 끝내기 전에 `schedule(DurationSeconds=300, TimerCondition="<task-id>")` 5분 깨우기 타이머를 설정**한다.
   - 5분 타이머가 울리거나 검증·API 탐색이 5분 이상 걸리면 타임아웃으로 덮지 말고 즉시 중간 로그를 읽어 병목 경계(핸들 미해제, 파이프 대기, 무한 폴링 등)를 찾고 소스를 수정한 뒤 `.ai/docs/performance.md` 또는 `### 메모`에 기록한다.
4. **흐름 결과 기록 및 `flow-wait`**
   - 전체 흐름 확인 후 `### 구현` 아래에 `- [○] <전체 흐름 확인 내역>`, `- [●] <최종 완료 준비 내역>`을 추가하고, `#### 확인 (#<id>)` 아래에 체크리스트를 덧붙인다:
     ```markdown
     #### 확인 (#<id>)
     - [x] <목적: 원하는 결과 라벨>
       결과 : 성공
       상세 : <실행한 함수(message/body 내용) ⇒ 전달 대상 ▷ 반환값/message 한 줄 요약 (JSON 원문 제외)>
     ```
   - 이어서 다음 명령을 실행한다:
     ```bash
     ~/ai/scripts/bin/grist flow-wait --project <dir>
     ```
   - `flow-wait`는 제출·숨긴 테스트를 재실행하고 흐름 기록을 검사해 `$H=확인`과 `$D`를 기록한다.
   - 사용자 전용 동작(로그인·인증, 결제·비용, 비가역 외부 전송, 브라우저 밖 OS 앱 조작)이 남은 경우에만 구체적인 단일 동작을 안내하고, 사용자 확인 후 `완료`로 마감된다고 안내한다.

## 3. 완료 게이트 및 최종 완료 (`Flow 7: Complete` & `end-project`)

1. **턴 종료 전 게이트 확인 (`end-project`)**
   - `~/ai/skills/end-project/SKILL.md`에 따라 메인 세션은 헤더의 모든 행이 `확인` 또는 `완료`일 때만 턴을 끝낼 수 있다(`~/ai/scripts/bin/grist gate --cwd <dir>` 확인).
   - 최종 응답 직전과 질문·사용자 동작 요청 직전에는 `fish -c 'nf -m "<작업명> <결과·요청 요약>"'`를 실행하고 exit code `0`을 확인한다.
2. **사용자 확인 후 최종 완료 (`task-complete` → `$H=완료`)**
   - 사용자가 확인(이번 세션 또는 이후 세션에서 "완료", "잘 된다" 등)하면 아직 `확인` 상태인 헤더 행마다 `### 구현` 아래에 `결과`/`상세` 없이 5줄 내외의 번호 목록으로 `#### 완료 (#<id>)`를 추가한다:
     ```markdown
     #### 완료 (#<id>)
     1. [📦] <새롭게 생성된 도메인, 기능 단어 리스트>
     2. [📬] <새롭게 연결된 외부 사이트, API>
     3. [⚙️] <수정한 문제 한줄요약>
     ```
   - 그 뒤 `~/ai/scripts/bin/grist task-complete <id> --project <dir>`를 실행해 `$H=완료`로 마감한다(`# 작업` → `# 완료` 이동과 `기능 추가`의 Obsidian `# 📍` 기록은 `grist`가 자동 처리하므로 수동 편집하지 않는다).
