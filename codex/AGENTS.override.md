
## Mandatory Pre-Read
- If requirements are ambiguous or multiple interpretations exist: list options + ask 1 question OR pick safest option and state assumption.
- Keep changes minimal: no refactors, no formatting sweeps, no unrelated cleanup.
- Prefer simplest working code: no speculative abstractions or extra features.
- Before editing: identify exact file+location (search/read first). Don’t edit blind.
- Definition of done: run the project’s tests/lint (or stated check) and report results.
- Trivial tasks (<= ~10 LOC, 1 file, clear): skip long planning; still follow “minimal change + verify”.


## Mandatory Final Notification
- When final work is fully completed, run `nf -m "${작업명} complete"` exactly once.

## Completion Log Rule (Feature Addition)
- Whenever a feature addition is fully completed, append a completion record to `./.agents/log.md`.
- Use this heading format for each record: `## 날짜 - 작업한일`.
- Add one record per completed feature task and keep entries in chronological order.
## Shell Default Rule
- Default shell is `fish`.
- When shell-dependent commands are needed, execute them with `fish -ic`.
- Mandatory final notification command should be run in fish context (example: `fish -ic 'nf -m "<task-name> complete"'`).

