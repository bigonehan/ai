---
name: dev-branch
description: Move or continue explicitly requested work on a Git development branch while strictly limiting changes to the branch operation the user named. Use only when the user explicitly asks to move, switch, continue, or isolate work on a dev/development branch, says `dev branch로 이전`, or invokes `$dev-branch`. Do not trigger merely because a repository already has a dev branch, a path contains `_dev`, or development work is being discussed.
---

# Dev Branch

Treat a dev-branch request as a Git branch operation by default. Do not infer deployment, browser, extension, storage, or integration migration.

## Freeze the request boundary

Before mutation, record:

- repository explicitly in scope;
- source branch or commit;
- requested target branch, defaulting to `dev` only when the user said dev branch without another name;
- whether existing working-tree changes must move with the checkout;
- exact work the user asked to continue after switching.

Inspect applicable instruction files, repository status, existing branches, worktrees, and remotes. Preserve unrelated and uncommitted user changes.

Ask only when the branch operation would overwrite changes, the repository is ambiguous, or multiple interpretations would materially change the result. Do not introduce a larger migration workflow to avoid asking one necessary question.

## Perform only the authorized branch operation

Use non-destructive Git commands.

1. If the named branch exists locally, switch to it when the working tree permits.
2. If it exists only on a remote, create a tracking local branch.
3. If it does not exist, create it from the explicitly named source or the current checked-out commit.
4. Continue only the implementation work included in the user's request.
5. Report the resulting branch, base commit, working-tree state, and files actually changed.

Do not merge, rebase, reset, force-push, delete branches, stash, commit, or push unless the user authorized that operation.

## Keep adjacent systems unchanged

Unless the user separately names each action, do not:

- create, copy, rename, deploy, or delete a `_dev` directory;
- change Chrome unpacked-extension registration or drive Chrome UI;
- change Bridge IDs, native-host settings, consumer paths, or integration configuration;
- copy browser extension settings, uploaded images, databases, caches, profiles, or local storage;
- stop or restart browsers, servers, workers, or other processes;
- build or deploy artifacts solely because the branch changed;
- synchronize `main` and `dev` beyond the exact Git operation requested.

A path containing `_dev` is not permission to create or migrate that path. A request to move to a dev branch is not permission to move runtime state.

## Handle explicit deployment requests separately

When the user explicitly requests a dev deployment in addition to the branch operation, keep it as a separate scoped step. Resolve the exact destination, required files, consumers that may be repointed, and data that may be copied before mutation. Apply project delivery rules, but do not add Chrome automation or storage migration unless explicitly requested.

## Verify narrowly

Verify only the requested boundary:

- `git branch --show-current` equals the requested branch;
- `git status --short` preserves expected working-tree changes;
- the branch base or upstream matches the requested source;
- forbidden adjacent paths and configurations were not modified.

Do not claim deployment or runtime migration from branch verification alone.
