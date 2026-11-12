---
name: build-ui
description: The single entry point for all UI work — building new web pages/screens/widgets/extension or Eagle plugin UI (through the internal `ui-oat` Skill), and designing, implementing, debugging, and verifying UI or UX changes against the rendered layout, including menu headers, sizing, spacing, alignment, responsive behavior, controls, event wiring (control → handler call), screenshots, and reference-image fidelity. Use for any frontend presentation work; do not use for non-visual backend-only changes.
---

# UI Design Implementation

Implement the requested visual behavior in the existing design system and verify the rendered result, not only source declarations or test snapshots.

## Routing: new web UI

- Every UI rule is reached through this Skill. A new web page, web screen, widget page, extension page, or Eagle plugin UI is built through the internal Skill `ui-oat/SKILL.md` (Oat, oat.ink). Read it only from here, before writing markup or styles, then return here for the menu header, layout contract, and rendered verification.
- Edits to an existing screen keep that screen's design system (see "Existing design behavior"); rebuild it through `ui-oat` only when the user asks for the switch.
- Completion and rendered checks follow this Skill and `verify-work`.

## Menu header (default)

- A screen with menus (tabs, sections, settings, actions) gets one header bar at the top of the screen. Hero/intro panes are not added unless requested.
- Unless the user specifies otherwise, each menu is an icon-only button (inline SVG, `stroke="currentColor"`, `aria-label` and `title` with the menu name), and the menu group is right-aligned in the header (`justify-content: flex-end` / `margin-left: auto`). The active menu shows a selected state (`aria-selected="true"` or `aria-current="page"`).
- Text labels, left alignment, or a sidebar are used only when the user asks for them.

## Pane

- A pane (panel, card, section box) groups related fields. The label that names each individual item inside a pane is bold and larger than the body text (e.g. label `font-weight: 600–700`, `font-size` ≥ body size × 1.1), so items read as headings over their values and controls.
- Titles and descriptions use different text colors: titles/labels use the main text color, descriptions and helper text use the muted text color of the design system (Oat: `--foreground` vs `--muted-foreground`).

## Alignment

- Buttons and inputs sit on imaginary horizontal and vertical lines. Controls in the same row share the same height and top/bottom edges (their centers on one horizontal line); controls stacked in a column share the same left or right edge (one vertical line). A button must not sit higher or lower than the input beside it, and a right-aligned button column must end on the same right edge.
- Verify with `getBoundingClientRect()`: same-row controls differ by ≤ 1px in `top` and `height`; same-column controls differ by ≤ 1px in `left` (or `right`).

## Modal

- Every modal has a header with a bold title and a muted description below it; the close button sits in the header flow on the title's row at the right end (not `position: absolute` on the dialog corner), shown as the borderless Lucide `x` icon with `aria-label="닫기"`. The header stays visible while only the body scrolls. No text "Close" button. Oat markup: `ui-oat/references/oat.md` → Dialog (modal).
- Modal content never exceeds the modal width (no horizontal scroll inside a modal); size the body to the dialog, not a fixed pixel width.
- Confirm and cancel buttons sit at the modal's bottom-right in the order confirm → cancel, and the two use different background colors (Oat: confirm = default primary, cancel = `data-variant="secondary"`).
- A modal with no confirm action (auto-saving settings, pickers that apply on click, info views) has no footer cancel button; the header `x` is its only close control. Never leave a lone cancel in the footer.

## Selection

- When several items are presented (lists, cards, tree nodes, tabs), the selected item differs from the others by background, border, or font weight, and carries `aria-selected="true"` or `aria-current`. With the color preset, the selected list item uses `background: var(--selected)` and `color: var(--selected-foreground)` (basic: `#8FBCBB`).

## Buttons

- Every button is distinguishable from its background by a background color or an outline. Borderless, backgroundless buttons are not used (in Oat: default, `outline`, or `data-variant`; never `ghost` alone). Sole exception: the modal header close `x` is `ghost`.
- Confirm (`check`) and add/create (`plus`) buttons show only the icon (no visible text; name in `aria-label` and `title`) and are wider than tall: a horizontal pill/rectangle, not a square or circle (e.g. `min-width` ≥ 1.5 × height, horizontal padding larger than vertical). Do not shrink them to a square icon button.

## Icon

- Icons come from Lucide (https://lucide.dev, ISC license). In a no-build screen, keep only the needed Lucide SVGs in the project (inline or a small module); do not depend on a runtime CDN.
- Buttons for these functions are always icon-only, with the function name in `aria-label` and `title`: refresh `refresh-cw`, create `plus`, confirm `check`, cancel `circle-x`, close `x`, open `folder-open`, save `save`.

## Settings menu

- **단축키 설정 필수:** UI 등을 만들거나 고칠 때 단축키(키맵·핫키) 관련 기능은 무조건 설정 메뉴에서 수정할 수 있도록 설정 메뉴를 구축한다(사용자가 현 단계에서 명시적으로 보류한 경우만 예외).
- **기본 구성:** 설정 메뉴 구축 시 기본 레이아웃 구성은 탭(Tab)으로 한다.
- **파일 불러오기:** 파일·폴더 불러오기(경로 선택)는 직접 경로 문자열만 입력하게 하지 않고 OS/호스트의 native 탐색기(파일 대화상자)를 이용한다.
- **하단 액션 배치:** 확인 버튼과 취소 버튼은 가능하면 오른쪽 아래(lower-right)에 배치한다.

## Layout contract

Before changing UI code, record each requested target with:

- the user's target kind: `column`, `cell`, `content`, `thumbnail`, `row`, `container`, or `gap`;
- the exact selector, affected axis, and authoritative layout owner;
- the current runtime size, target size, and allowed tolerance;
- every constraint that may redistribute the size, including automatic tracks, percentages, intrinsic minimums, padding, borders, and min/max dimensions.

Measure the same semantic target after the change with `getBoundingClientRect()` or computed style. Do not substitute a child image, thumbnail, or content measurement for a requested column or cell. Use an authoritative layout constraint such as a `colgroup`, grid track, or flex basis when automatic distribution would override a local declaration.

Write the observations as JSON and run:

```bash
scripts/bin/validate-layout-contract <layout-contract.json>
```

The default tolerance is `1px`. A nonzero result blocks completion and any request for the user to refresh or reconfirm the same size requirement.

## Existing design behavior

- Put affirmative actions such as confirm, start, apply, or delete to the left of cancel or close actions. Keep both DOM order and visual order consistent.
- Give controls in the same action area consistent size, radius, padding, and icon size.
- Place save, close, confirm, and cancel controls in the screen or dialog's lower-right footer action area rather than inside a pane header or body ending.
- Prefer the existing design system, tokens, components, and theme. Do not introduce a competing button, color, or dark-theme system.
- Existing handlers, dialogs, or controls do not establish completion. Verify their visibility, order, disabled state, resulting dialog, and saved state in the rendered UI.
- Add or update assertions that directly cover the requested UI behavior; unrelated passing tests are insufficient.

## Reference screenshots

When a screenshot is provided, treat its visible layout as the contract. Record:

- existing elements that must remain;
- elements that must be added;
- explicitly removed elements;
- forbidden rearrangements.

Preserve vertical, horizontal, containment, shared-container, and hierarchy relationships unless the user requested a change. Do not relocate, shrink, split, or move an existing element merely because another arrangement seems preferable.

For `current.png`, compare the post-change screenshot with the reference and record the reproduction path, capture file, post-change screenshot, inspected differences, and any remaining difference. A passing test without a post-change visual comparison is not completion evidence.

## Rendered verification

Check the actual rendered surface for:

- items staying inside their wrapper;
- checkboxes remaining to the left of their item;
- edit and delete actions using the established icon treatment;
- group header, controls, and items preserving their required relative positions;
- no unintended text buttons remaining where icon actions are required.

For a repeated visual complaint, stop incremental style tweaks. Record why the earlier check passed, prove that the earlier runtime measurement is rejected, and verify the corrected target in the actual runtime.
