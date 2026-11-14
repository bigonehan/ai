---
name: ui-oat
description: build-ui 내부 Skill — Oat(https://oat.ink, `@knadh/oat`) 설치·런타임 호환·마크업·테마 규칙. build-ui가 새 웹 UI를 만들 때만 읽는다. 직접 로드하지 않는다.
---

# UI Oat

`build-ui`가 새 웹 UI를 만들 때 따르는 Oat 규칙이다. 컴포넌트 마크업은 `references/oat.md`, 런타임별 주의·과거 오류는 `references/notes.md`를 먼저 읽는다(필요한 절만 `rg -n`).

## 1. 설치
- 기본: `<link rel="stylesheet" href="…/oat.min.css">`, `<script src="…/oat.min.js" defer></script>`. 버전을 고정한다(`https://unpkg.com/@knadh/oat@1.1.0/oat.min.css`·`oat.min.js`). 번들러가 있으면 `npm i @knadh/oat@1.1.0`과 `import '@knadh/oat/oat.min.css'`·`import '@knadh/oat/oat.min.js'`.
- Tailwind·daisyUI·별도 컴포넌트 CSS를 함께 쓰지 않는다. 같은 역할의 자체 버튼·입력·탭·다이얼로그 CSS를 새로 만들지 않고, 자체 CSS는 배치(grid·flex·간격)만 둔다.

## 2. 런타임 호환 (먼저 판정)
- 실행 런타임의 Chromium 버전을 먼저 확인한다(Electron 앱은 Electron 버전 → Chromium 대응). Oat 1.1.0 CSS는 `light-dark()`(Chromium 123+), CSS 중첩 `&`(112+), `color-mix()`(111+)를, JS는 Popover API(114+)를, 문서 예시의 `commandfor`/`command`는 135+를 쓴다.
- Chromium 123 미만(예: Eagle 4 = Electron 22 = Chromium 108)이면 CDN 원본을 쓰지 않고 변환본을 프로젝트에 둔다:
  `npx -y lightningcss-cli@1 --targets "chrome <버전>" --minify oat.min.css -o vendor/oat.css` (`light-dark()`·중첩은 풀리고, `var()`를 섞는 `color-mix()` 13곳은 남는다 — hover 색 `--_hov`·secondary 테두리. 그 값은 `references/notes.md`의 폴백 규칙을 따른다).
- Popover 미지원 런타임에서는 `ot-dropdown`·toast를 쓰지 않고, 다이얼로그는 `commandfor` 대신 `dialog.showModal()`/`close()`를 JS로 부른다.

## 3. 테마·색
- 기본 테마(4절 color 프리셋, 지정 없으면 `basic`)를 쓴다. 색은 Oat 변수(`--primary`·`--primary-foreground`·`--secondary`·`--background`·`--foreground`·`--card`·`--muted`·`--border`·`--input`·`--danger`·`--selected`·`--selected-foreground` 등)와 `data-variant`(`secondary`·`danger`·`success`·`warning`)로만 지정한다.
- 목록(list·메뉴 목록·트리·카드 목록)의 현재 선택 항목은 배경 `var(--selected)`(basic: `#8FBCBB`)·글자 `var(--selected-foreground)`·굵게(`--font-semibold`)로 표시하고 `aria-selected="true"` 또는 `aria-current`를 단다. 선택되지 않은 항목은 `--card` 배경·`--border` 테두리를 유지하고 hover는 `--accent`로만 바꾼다.
- 임의 색 지정 금지: hex·`rgb()`·`oklch()`·이름 색을 마크업·자체 CSS에 쓰지 않는다. 사용자가 색을 명시하면 `:root`의 Oat 변수만 재정의한다.
- 다크 모드는 color 프리셋의 `light-dark()` 값과 OS·앱 설정(`prefers-color-scheme`)을 따른다. `basic`은 밝은·다크 값을 모두 정의하며, 강제 전환은 사용자가 요청할 때만 만든다.

## 4. Preset
- 프리셋은 `Presets/color/<이름>.json`(+ `<이름>.theme.css`)과 `Presets/icon/<이름>.json`이다. 사용자가 다른 프리셋을 지정하지 않으면 color·icon 모두 `basic`을 쓴다.
- color: `Presets/color/<이름>.theme.css`(`basic`, `melange_me`)의 `:root` 변수를 프로젝트 테마로 쓴다(Oat CSS 뒤에 로드하거나 그 값으로 `:root`를 재정의). Chromium 123 미만이면 `light-dark()`를 2절 변환본과 같은 방식(lightningcss)으로 풀어 넣는다.
- icon: `Presets/icon/basic.json`의 라이브러리·SVG 속성·기능별 아이콘(`actions`)·모달 닫기·메뉴 규칙을 따른다(build-ui `## Icon`과 값이 다르면 이 파일이 우선이며 둘을 함께 고친다).

## 5. 기록
- `references/notes.md`는 Oat 화면의 오류·런타임 비호환·알아야 할 동작 기록이다(검증자가 추가, 형식은 파일 맨 위).
