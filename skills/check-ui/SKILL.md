---
name: check-ui
description: Audit a running web UI against live references, write the gaps into plan.md, and dispatch an orc-cli-workflow worker to implement them.
---

# Check UI

## Overview
- Capture the current app with `orc clit` or the current ORC runner path.
- Search the web for 2 to 3 comparable live sites and capture matching reference screens.
- Compare current UI against those references and write the gaps into `./plan.md`.
- Open a fresh tmux pane and dispatch a Codex worker with `$orc-cli-workflow` to implement the UI improvements.

## Preconditions
- Run this skill only when the target app is already running or the current repo has a working ORC runner path for `orc clit test -p .`.
- If the app cannot be reached, the current screen cannot be captured, or no comparable live sites can be found, stop and resolve that blocker before dispatching implementation.
- For this skill, `./plan.md` is the intentional handoff artifact. Do not replace it with `todo.md`.

## Workflow
1. Capture the current app.
- Run `orc clit test -p . -m "ui audit capture"` or another task-specific mode string that still uses the current workdir.
- Keep the generated capture evidence from the run, including the screenshot path reported by ORC and any saved files such as `screen-capture.png`, `rect-capture.png`, or `.project/screenshot/*`.
- Record the exact current-app capture path in `./plan.md`.

2. Collect external references.
- Search the web for 2 to 3 live websites in the same product category and interaction pattern.
- Prefer current, public sites with strong UX, not static gallery shots or marketing-only pages.
- Capture one representative screen per reference at the same viewport class as the current app.
- Record each reference URL, screenshot path, and the specific UI qualities worth borrowing.

3. Compare the current UI against the references.
- Compare functional gaps first: missing flows, missing states, weak affordances, missing navigation, or missing UI feedback.
- Compare design gaps second: hierarchy, spacing, rhythm, typography, color use, density, empty states, error states, and responsiveness.
- Ignore differences that are only stylistic and do not improve usability or clarity.

4. Write `./plan.md`.
- Create or update `./plan.md`.
- Use these headings exactly:
  - `# current capture`
  - `# reference captures`
  - `# missing functionality`
  - `# missing design`
  - `# implementation order`
  - `# dispatch prompt`
- Under `# implementation order`, list the fixes in dependency order, not by visual preference.
- Under `# dispatch prompt`, write the exact Codex worker instruction that will be sent to the new pane.

5. Dispatch the implementation worker.
- Open a fresh pane with `tmux split-window -h -P -F '#{pane_id}'`.
- Send the worker command with `orc send-tmux <pane_id> "<command>" enter`.
- The worker command must:
  - `cd` into the current workdir
  - invoke Codex
  - explicitly mention `$orc-cli-workflow`
  - tell the worker to read `./plan.md`
  - tell the worker to implement the listed UI gaps
  - tell the worker to rerun verification and update `./.project/feedback.md`

## Worker Command Template
```text
cd <workdir> && codex exec --dangerously-bypass-approvals-and-sandbox "$orc-cli-workflow ./plan.md를 읽고 UI 부족 기능과 디자인 부족 항목을 순서대로 개선하라. 구현 후 ORC 검증을 다시 실행하고 ./.project/feedback.md를 갱신하라."
```

## Comparison Standard
- Functional gaps must be written as user-visible outcomes, not implementation guesses.
- Design gaps must be written as concrete deltas, such as `CTA hierarchy is weak`, `card spacing collapses on mobile`, or `empty state lacks guidance`.
- When a reference is strong in one area and weak in another, borrow only the strong part.
- If multiple references disagree, prefer the pattern that best matches the current app's product type and user flow.

## Completion Gate
- `./plan.md` exists and contains all required headings.
- `./plan.md` lists both missing functionality and missing design.
- At least one current-app capture and at least two reference captures are recorded.
- A fresh tmux pane was opened and the worker command was sent through `orc send-tmux`.
