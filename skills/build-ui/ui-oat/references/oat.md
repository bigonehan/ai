# Oat 1.1.0 마크업 요약 (출처 https://oat.ink/components, /usage, /customizing)

### Button
- 기본=primary `<button>`, `data-variant="secondary|danger"`, `class="outline|ghost"`, 크기 `class="small|large"`, 링크 버튼 `<a class="button">`.
- 그룹: `<menu class="buttons"><li><button class="outline">…</button></li></menu>`.
- 아이콘 버튼: 공식 예시 없음 → `<button class="ghost small" aria-label="…"><svg …/></button>`(인라인 SVG, `stroke="currentColor"`).

### Form
- `<label data-field>이름 <input type="text" /></label>`, 힌트 `<small data-hint>`, 오류 `aria-invalid="true"` + `<div class="error" role="status">`.
- 입력 그룹 `<fieldset class="group"><input /><button>Go</button></fieldset>`, 가로 배치 `<fieldset class="hstack">`, 스위치 `<input type="checkbox" role="switch" />`.

### Tabs (JS 필요)
```html
<ot-tabs>
  <div role="tablist"><button role="tab">A</button><button role="tab">B</button></div>
  <div role="tabpanel">…</div><div role="tabpanel">…</div>
</ot-tabs>
```

### Dialog (modal)
- `<dialog id="d"><form method="dialog"><header><h3>제목</h3></header><div>…</div><footer><button type="button" class="outline">Cancel</button><button value="confirm">OK</button></footer></form></dialog>`. 열기는 `commandfor="d" command="show-modal"`(Chromium 135+) 또는 `d.showModal()`.
- 모든 modal은 아래 모양이다(shadcn Dialog 형태, 261107 eagle_folderStructure #745 기준):
  ```html
  <dialog id="d"><form method="dialog">
    <header>
      <div class="dialog-title-row"><h3>Edit profile</h3>
        <button class="ghost dialog-close" type="button" aria-label="닫기" title="닫기"><svg class="lucide lucide-x" …/></button></div>
      <p>Make changes to your profile here. Click save when you're done.</p>
    </header>
    <div>본문 필드</div>
    <footer><button class="icon-wide" aria-label="확인" title="확인"><svg class="lucide lucide-check" …/></button><button type="button" data-variant="secondary" aria-label="취소" title="취소"><svg class="lucide lucide-circle-x" …/></button></footer>
  </form></dialog>
  ```
  - 확인(`check`)·추가(`plus`) 버튼은 아이콘만 보이고 가로폭이 세로보다 넓다(`.icon-wide{min-width:calc(var(--space-6)*2.5);padding-inline:var(--space-4)}`처럼 배치 CSS만 둔다). 정사각·원형 아이콘 버튼으로 줄이지 않는다.
  - header 첫 줄 = 굵은 `h3` 제목 + 같은 줄 오른쪽 끝 닫기 `x`(flex `justify-content: space-between`, `align-items: center`), 둘째 줄 = muted `p` 설명(Oat `dialog>form>header>p`가 `--muted-foreground` 적용).
  - 닫기 버튼은 header 흐름 안에 둔다. `position: absolute`로 dialog 모서리·바깥에 띄우지 않는다(스크롤바·dialog 테두리와 겹친 outline 상자가 됨). 테두리·배경 없는 `ghost` 아이콘 버튼이며, build-ui Buttons 절 "ghost 금지"의 유일한 예외다.
  - 첫 입력 필드의 라벨("Preset name" 등)을 제목처럼 쓰지 않는다. 제목은 dialog가 하는 일(`Edit preset`)이고, 필드는 header 아래 본문 `div`에 둔다.
  - dialog 안에서 `card`·테두리 상자로 header·본문·footer를 다시 감싸지 않는다. 간격은 Oat `dialog>form>header|div|footer` padding(`--space-6`)을 그대로 쓰고 자체 `.dialog-content` 패딩·테두리로 덮지 않는다.
  - 스크롤은 본문 `div`만 한다(`max-height` + `overflow-y: auto`). header(제목·닫기)는 항상 보인다. 확인 → 취소는 `form>footer` 우하단.

### Card·Badge·Alert
- `<article class="card"><header><h3>…</h3></header>…<footer class="hstack">…</footer></article>`
- `<span class="badge" data-variant="success|warning|danger|secondary">`, `class="badge outline"`
- `<div role="alert" data-variant="success|warning|error">`

### Toast (JS, Popover 114+)
- `ot.toast(message, title, { variant: 'success|danger|warning', placement: 'top-right', duration: 4000 })`, `ot.toast.clear()`.

### Layout
- 사이드바 `<div data-sidebar-layout><aside data-sidebar><nav>…</nav></aside><main>…</main></div>`, 상단 `<header data-topnav>`.
- 그리드 `.container > .row > .col-1~12`, `offset-N`; 유틸 `hstack`·`vstack`·`flex`·`gap-2`·`p-4`·`mt-4`.

### 테마 변수
- `:root { --primary: …; --primary-foreground: …; }` 형식으로 재정의. 전체 목록은 GitHub `knadh/oat` `theme.css`.
