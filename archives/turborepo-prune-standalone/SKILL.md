---
name: turborepo-prune-standalone
description: Extract a target app from a Turborepo with `turbo prune` and prepare it for handoff. Use when users ask to isolate `apps/...` (for example `app/web/next`) with only required local workspace packages, create a minimal build context, or convert the pruned mini-monorepo into a standalone single-project structure by inlining local packages and rewriting imports.
---

# Turborepo Prune Standalone

Isolate only what is needed for one app, then choose one of two outcomes:
1. Keep mini-monorepo shape from `turbo prune` (fastest and usually safest)
2. Convert to standalone polyrepo by inlining local packages and rewriting dependency/import paths

## Workflow

1. Confirm target and package manager
- Identify exact target (`app/web/next`, `apps/web`, etc.).
- Confirm package manager (`pnpm`, `npm`, `yarn`) and `turbo` availability.

2. Run prune
- Use:
```bash
npx turbo prune app/web/next --outDir=./dist/isolated-app
```
- If target differs, replace `app/web/next` accordingly.

3. Verify prune result
- Expect:
  - `dist/isolated-app/json`: manifest/lock/config subset for dependency install optimization
  - `dist/isolated-app/full`: source tree subset for the app and required local packages
- Confirm unrelated workspace packages are absent.

4. Decide handoff mode
- If user wants fastest extraction with existing workspace structure, stop at prune result.
- If user wants fully standalone app, continue with inlining workflow in `references/standalone-conversion.md`.

5. Run handoff checklist
- Always run checks in `references/handoff-checklist.md` before handoff.

## Decision Rules

- Prefer prune-only when:
  - Buyer/team can accept mini-monorepo structure.
  - Time-to-delivery is prioritized.
  - Workspace package boundaries are still useful.

- Use standalone conversion when:
  - Buyer demands single-project repository shape.
  - `workspace:*` dependencies must be eliminated.
  - Internal package namespace imports must be removed.

- Flag git history requirements early:
  - If commit history must be preserved, avoid plain file copy flow.
  - Use `git-filter-repo`-based extraction workflow from `references/git-history.md`.

## References

- For standalone conversion steps: `references/standalone-conversion.md`
- For security and delivery checks: `references/handoff-checklist.md`
- For history-preserving extraction: `references/git-history.md`
