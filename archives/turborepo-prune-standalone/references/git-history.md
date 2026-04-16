# Git History Preservation

Use this flow only when the recipient needs commit history for the extracted app/packages.

## Why

Simple copy-based extraction (`turbo prune`, manual file copy) does not preserve original commit history.

## High-Level Approach

1. Identify required paths
- App path (example: `apps/web/next`)
- Required local package paths (example: `packages/A`)
- Shared config paths that must remain meaningful

2. Extract filtered history
- Use `git-filter-repo` (or equivalent history-rewrite tooling) to keep only relevant paths.
- Perform history filtering in a dedicated clone, never in the primary repository.

3. Reshape repository if needed
- If delivering standalone, move filtered paths into final layout after filtering.
- Re-run tests/build and ensure repository still works after reshaping.

4. Deliver with provenance note
- Document that history is path-filtered.
- Include source commit or tag reference from original monorepo.

## Cautions

- History rewriting changes commit hashes.
- Tags/branches may require explicit migration.
- Submodules and CI config often need manual adjustments after filtering.
