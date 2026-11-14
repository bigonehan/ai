# 터미널 및 Antigravity CLI 테마 색상 안내 (`theme.json`)

이 문서는 `~/ai/theme/theme.json`에 정의된 각 색상 코드가 실제 화면(Windows Terminal `melange` 스킴 + Antigravity CLI `colorScheme: "terminal"`)의 어느 요소에 사용되는지 정리한 안내서다.

## 1. 적용 구조 (`environment`)

- **실제 색상 소유 파일**: `/mnt/c/Users/tende/AppData/Local/Packages/Microsoft.WindowsTerminal_8wekyb3d8bbwe/LocalState/settings.json` (`schemes` 내 `"name": "melange"`)
- **동작 방식**: Antigravity CLI(`agy`)가 기본값 `"colorScheme": "terminal"` 상태에서 ANSI 색상 코드를 출력하면, Windows Terminal이 `melange` 팔레트 값을 매핑해 화면에 그린다.

---

## 2. 역할별 핵심 색상 (`roles`)

| `theme.json` 키 | 색상 코드 | 매핑 팔레트 (ANSI 코드) | 실제 화면 사용처 |
|---|---|---|---|
| `roles.background` | `#F1F1F1` | `palette.background` | 터미널 전체 기본 바탕색 |
| `roles.defaultText` | `#54433A` | `palette.foreground` (`39`) | AI 응답 일반 본문, 마크다운 소제목(`###`), 목록 기본 글자색 |
| `roles.parenthesesText` | `#7A6C63` | `palette.white` (`37`) | 도구 호출의 `()` 괄호 안 인자 텍스트(`Read(~/ai/...)`, `Bash(...)`), 우측 접기 안내 `(ctrl+o to expand)`, 셸 프롬프트 시각 보조 문자 |
| `roles.infoMessages.sessionMetaAndDivider` | `#767676` | `palette.brightBlack` (`90`) | 시작 화면의 계정·모델·작업 경로 안내(`tendertree@gmail.com`, `Gemini 4 Carbon`, `~/extensions`) 및 가로 구분선(`────`) |
| `roles.infoMessages.tokenUsageAndToolHeader` | `#BC5C00` | `palette.yellow` (`33`, `1;33`) | 턴 상단 토큰·생각량 요약 안내(`51.3k in \| 10.2k out \| 7.9k thinking`), 도구 이름 강조(`Read`, `Bash`, `WebSearch`), 실행 중 상태 점(`●`) |
| `roles.infoMessages.toolCompletedIndicator` | `#6E9B72` | `palette.green` (`32`) | 완료된 도구 호출 줄 맨 앞의 초록색 상태 점(`●`) |
| `roles.infoMessages.userPromptLine` | `#7892BD` | `palette.blue` (`1;34`) | 사용자가 입력한 프롬프트 줄(`> ...`) |
| `roles.infoMessages.inlineCodeAndCommand` | `#465AA4` | `palette.brightBlue` (`94`) | 응답 문장 속 인라인 코드·슬래시 명령·설정 키 강조(`/settings`, `/config`, `"colorScheme"`, `Ctrl+K`) |
| `roles.infoMessages.errorMessage` | `#C77B8B` | `palette.red` (`31`) | 일반 오류·실패 상태 표시 및 Git 변경 경고 기호 |
| `roles.infoMessages.criticalAlert` | `#BF0021` | `palette.brightRed` (`91`) | 강조 경고·차단 오류 메시지 |
| `roles.ui.cursor` | `#7D6658` | `palette.cursorColor` | 터미널 입력 커서 색상 |
| `roles.ui.selectionBackground` | `#D9D3CE` | `palette.selectionBackground` | 마우스로 드래그해 선택한 영역의 배경색 |

---

## 3. 전체 ANSI 16색 팔레트 (`palette`)

Windows Terminal `settings.json`의 `schemes` 배열에 그대로 복사해 넣을 수 있는 `melange` 전체 항목이다.

| 키 | 색상 코드 | ANSI SGR | 비고 |
|---|---|---|---|
| `background` | `#F1F1F1` | 배경 기본 | 밝은 웜 오프화이트 바탕 |
| `foreground` | `#54433A` | 전경 기본 (`39`) | 진한 웜 브라운 차콜 본문색 |
| `cursorColor` | `#7D6658` | 커서 | 중간 톤 브라운 |
| `selectionBackground` | `#D9D3CE` | 선택 영역 | 옅은 베이지 그레이 |
| `black` | `#0C0C0C` | `30` | 검정 |
| `red` | `#C77B8B` | `31` | 차분한 로즈 레드 |
| `green` | `#6E9B72` | `32` | 도구 완료 점(`●`), 셸 성공 표시 |
| `yellow` | `#BC5C00` | `33` | 토큰 안내 줄, 도구 이름(`Read` 등) |
| `blue` | `#7892BD` | `34` | 사용자 질문 입력 줄(`> ...`) |
| `purple` | `#BE79BB` | `35` | 셸 Git 브랜치 표시(`main`) |
| `cyan` | `#739797` | `36` | 셸 작업 디렉터리 경로 표시 |
| `white` | `#7A6C63` | `37` | `Read(...)` 괄호 안 경로·인자, `(ctrl+o to expand)` 보조 텍스트 (기존 `#CCCCCC`에서 가독성 개선) |
| `brightBlack` | `#767676` | `90` | 상단 메타 정보 및 구분선 |
| `brightRed` | `#BF0021` | `91` | 강한 오류·경고 |
| `brightGreen` | `#3A684A` | `92` | 진한 초록(런타임 버전 표시 등) |
| `brightYellow` | `#A06D00` | `93` | 밝은 황토색 강조 |
| `brightBlue` | `#465AA4` | `94` | 인라인 코드·명령어·옵션 스팬 |
| `brightPurple` | `#904180` | `95` | 진한 보라색 강조 |
| `brightCyan` | `#3D6568` | `96` | 진한 청록색 강조 |
| `brightWhite` | `#F2F2F2` | `97` | 밝은 배경 보조 흰색 |

---

## 4. 색상 수정 방법

1. `~/ai/theme/theme.json`에서 원하는 역할(`roles`)에 매핑된 `palette` 키를 확인한다(예: `()` 괄호 안 색상은 `palette.white`).
2. `~/ai/theme/theme.json`과 Windows Terminal 설정 파일(`/mnt/c/Users/tende/AppData/Local/Packages/Microsoft.WindowsTerminal_8wekyb3d8bbwe/LocalState/settings.json`의 `"name": "melange"` 블록)에서 해당 키의 HEX 값을 함께 수정한다.
3. 저장 즉시 열려 있는 Windows Terminal 화면에 실시간으로 반영된다.
