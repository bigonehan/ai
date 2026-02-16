## Mandatory Pre-Read
- Before starting any task, read `/home/tree/ai/doc/guide.md`.
- If this file cannot be read, stop immediately and report it.

## Plan Mode Rule (Feature Work)
- In Plan mode, the first objective is:
  1. Ask for and confirm the feature scope and requirements.
  2. Create a plan file using the template at `/home/tree/ai/templates/plan.md`.
- The plan file must be created before any implementation work starts.
- Save the plan file at `./.agents/plan.md` (relative to the current working directory where Codex was invoked).
- Treat that plan file as the execution source of truth for subsequent task work and updates.
- If `/home/tree/ai/templates/plan.md` cannot be read, stop immediately and report it.

## Completion Log Rule (Feature Addition)
- Whenever a feature addition is fully completed, append a completion record to `./.agents/log.md`.
- Use this heading format for each record: `## 날짜 - 작업한일`.
- Add one record per completed feature task and keep entries in chronological order.

## Version Control: Jujutsu (jj)
- Use **Jujutsu (`jj`)** as the default local VCS workflow for feature add/change work.

### Core Rules
- For feature additions/changes, `jj` usage is mandatory.
- Use `jj new` to split logical units of work into separate changes.
- Use `jj workspace` when you need physical workspace isolation for high-risk structural work.

### Decision Guide
1. **Small / localized change**: Use `jj new`.
2. **High-risk structural change**: Use `jj workspace`.

### Standard Flow (Small / Localized)
1. Check status:
```bash
jj st
jj log -n 10
```
2. Create a change:
```bash
jj new -m "feat(scope): summary"
```
3. Implement and verify with project checks.
4. Confirm diff:
```bash
jj st
jj diff
```
5. Finalize:
```bash
jj describe -m "feat(scope): summary"
```
6. If remote sync is needed:
```bash
jj git fetch
jj rebase -d trunk
jj git push
```
