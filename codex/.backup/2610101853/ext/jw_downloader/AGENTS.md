# jw_downloader Working Rules

## Project Boundary
- 이 폴더를 독립 프로젝트로 취급하고 사용자 요청 없이 형제 프로젝트를 수정하지 않는다.

## Versioning
- 기능 추가·개선 시 `manifest.json`과 `package.json` 버전을 함께 `0.0.1` 올린다.

## Delivery
- 구현·검증 후 `npm run build`로 `dist`를 생성한다.
- 실행 가능한 `dist` 내용만 `/mnt/h/code/jw_downloader`에 한 번 복사하고 종료 코드만 확인한다.

## Verification Artifacts
- 테스트가 실제로 필요로 하는 임시 파일만 이 프로젝트의 `test/tmp/` 아래에서 생성하고, 작업이 끝나면 지운다.
- 제품 manifest·fixture·설정은 소유 경로에 유지한다.

## Test Layer Contract
- mock은 개별 production 함수의 정상·이상 입력과 인접 production consumer로 전달되는 payload·타입·순서·횟수 검증에만 사용한다.
- mock Chrome API, fake media event, fake codec, fake download callback이 포함된 결과는 `offline` 또는 `boundary integration`으로만 표시하고 전체 흐름 성공으로 요약하지 않는다.
- 최종 전체 흐름은 배포 artifact를 `run-cdp` 공용 프로필 브라우저(`/home/tree/.config/codex-run-cdp`, 화면 있는 WSL Chromium)의 Suno origin에서 mock·substitution 없이 로드하고, `stagehand-access`로 렌더된 UI를 조작해 시작한 뒤 그 프로필의 실제 다운로드 폴더에 생긴 새 MP3 파일 bytes를 권위 있는 consumer로 판독한다.
- 브라우저 UI 자동화는 에이전트가 `run-cdp`·`stagehand-access`로 직접 수행한다. Suno 로그인이 공용 프로필에 없을 때만 사용자에게 그 로그인 한 번을 요청한다.
- `npm test`는 build와 offline 단계 결과만 반환한다. 프로젝트 자체 Stagehand runner·raw CDP runner·runtime evidence 검사기(`scripts/stagehand-runtime.mjs` 등)는 legacy로 삭제했으며, 실제 브라우저 단계는 `run-cdp`·`stagehand-access`로만 수행한다.
- Stagehand 실제 브라우저 단계는 동일 revision의 개별 함수 입력·함수 간 전달·build artifact 검증이 전부 통과한 뒤에만 진행한다. Stagehand는 `stagehand-access`의 `openStagehand({ extensions: [배포 디렉터리] })`로 공용 프로필 브라우저를 쓰며, 별도 user-data-dir이나 Windows CDP·portproxy·Hyper-V firewall에 의존하지 않는다.
- Stagehand는 공용 프로필의 인증 세션을 사용하고, 세션·쿠키·API key·전체 개인 경로를 로그에 남기지 않는다. 로그인이 필요하면 임의의 인증 우회나 storage 주입을 하지 않고 인증 필요 경계로 실패한다.
- 실제 브라우저에서 예상하지 못한 오류가 나오면 해당 값만 특별 처리하지 않고 loader timing, SPA, trusted event, media generation, Chrome API lifecycle, codec, filesystem, retry, consumer readback 중 누락 상태축으로 보편화한다. 정확한 실패 조합과 인접 상태, mutant-red 회귀를 추가한 뒤 개별 함수 단계부터 전체 사다리를 다시 실행한다.
- 비동기 전파 테스트의 기다림은 권위 있는 완료 신호 또는 명시적 wall-clock deadline으로 표현한다. 고정된 microtask·`setImmediate`·polling turn 횟수를 timeout 계약으로 사용하지 않고, 이 경계를 수정하면 동일 병렬 전체 suite를 반복해 안정성을 확인한다.
