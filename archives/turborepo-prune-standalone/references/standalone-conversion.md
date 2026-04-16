# Standalone Conversion

Convert pruned output (`dist/isolated-app/full`) from mini-monorepo layout into a standalone app.

## Preconditions

- Run `turbo prune` successfully.
- Work from a copy/branch, not the original production branch.
- Confirm the desired app root (example: `apps/web/next`).

## Steps

1. Locate local packages referenced by the app
- Inspect app `package.json` dependencies for workspace entries (`workspace:*`).
- Map each internal package (example: `@my-repo/A`) to its source folder (example: `packages/A`).

2. Move package code into the app
- Create internal app folder (example: `src/lib/A`).
- Copy source from `packages/A` into `apps/web/next/src/lib/A`.
- Keep only code required by the app; remove package-level scaffolding not needed for runtime/build.

3. Rewrite app dependencies
- Remove inlined workspace package entries from app `package.json`.
- Add any external dependencies that used to be transitive via the package.
- Run install and fix missing modules.

4. Rewrite imports
- Replace workspace alias imports:
```ts
import { x } from "@my-repo/A";
```
- With app-local imports (example):
```ts
import { x } from "@/lib/A";
```
- Perform project-wide search/replace carefully, then run type-check/build.

5. Adjust configs
- Update `tsconfig` paths and `baseUrl` if aliases changed.
- Update ESLint/Prettier/Jest/Next config references that pointed to workspace roots.
- Remove stale references to `packages/*`.

6. Validate standalone integrity
- Run tests/type-check/build in the standalone root.
- Ensure no `workspace:*` remains.
- Ensure no unresolved import path still references old package namespace.

## Common Failure Modes

- Hidden transitive dependency after inlining:
  - Symptom: runtime/build module not found.
  - Fix: add missing dependency explicitly to app `package.json`.

- Incomplete import rewrite:
  - Symptom: unresolved `@my-repo/*` imports.
  - Fix: run global search and patch remaining paths.

- Config drift:
  - Symptom: lint/type-check uses missing root config.
  - Fix: copy required config files or inline equivalent settings locally.
