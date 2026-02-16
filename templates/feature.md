---
name: <feature-name>
description: <one-line summary>
owner: <owner>
status: draft # draft | ready | in-progress | done
related_plan: ./.agents/plan.md
related_change: <jj-change-id>
---

# features
- [ ] F1. <feature requirement>
  - objective: <why>
  - acceptance_criteria:
    - <measurable condition>
- [ ] F2. <feature requirement>
  - objective: <why>
  - acceptance_criteria:
    - <measurable condition>

# files
## create
- `path/to/new.file`: <responsibility>

## modify
- `path/to/existing.file`: <what changes and why>

## delete
- `path/to/remove.file`: <reason>

# rule
- Follow architecture order: `domain -> port -> adapter -> composition`.
- For UI changes, add components under `packages/ui/shadcn` and consume via `@ui/shadcn`.
- Use Jujutsu workflow: `jj new` for localized changes, `jj workspace` for structural changes.
- Do not implement code before `./.agents/task.md` is created from this template.
- Keep changes scoped to this feature document.

# implementation_steps
1. <step>
2. <step>
3. <step>

# validation
- [ ] Type check: `<command>`
- [ ] Tests: `<command>`
- [ ] Manual checks:
  - <scenario>

# done_definition
- [ ] All acceptance criteria are met.
- [ ] Validation commands pass.
- [ ] Task document and decision notes are updated.
