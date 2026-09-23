---
name: ui-design-implementation
description: Design, implement, debug, and verify UI or UX changes against the rendered layout, including sizing, spacing, alignment, responsive behavior, controls, screenshots, and reference-image fidelity. Use for frontend presentation work; do not use for non-visual backend-only changes.
---

# UI Design Implementation

Implement the requested visual behavior in the existing design system and verify the rendered result, not only source declarations or test snapshots.

## Layout contract

Before changing UI code, record each requested target with:

- the user's target kind: `column`, `cell`, `content`, `thumbnail`, `row`, `container`, or `gap`;
- the exact selector, affected axis, and authoritative layout owner;
- the current runtime size, target size, and allowed tolerance;
- every constraint that may redistribute the size, including automatic tracks, percentages, intrinsic minimums, padding, borders, and min/max dimensions.

Measure the same semantic target after the change with `getBoundingClientRect()` or computed style. Do not substitute a child image, thumbnail, or content measurement for a requested column or cell. Use an authoritative layout constraint such as a `colgroup`, grid track, or flex basis when automatic distribution would override a local declaration.

Write the observations as JSON and run:

```bash
python3 scripts/validate_layout_contract.py <layout-contract.json>
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
