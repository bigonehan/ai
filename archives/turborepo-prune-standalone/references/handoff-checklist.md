# Handoff Checklist

Run this checklist before delivering extracted or standalone output.

## Security

- Check `.env*` files for secrets (API keys, tokens, internal URLs).
- Remove or redact secrets before handoff.
- Confirm no private certificates or internal credentials are included.

## Licensing and Docs

- Update `README.md` to match extracted app scope.
- Update `LICENSE`/NOTICE files for redistributed code.
- Remove monorepo-specific instructions that no longer apply.

## Build and Runtime

- Verify install command works from handoff root.
- Verify build command succeeds.
- Verify start/dev command runs without referencing removed packages.

## Dependency Hygiene

- Confirm no `workspace:*` dependencies remain if delivering standalone.
- Confirm lockfile matches delivered structure.
- Confirm no imports point to removed monorepo paths.

## Output Sanity

- Ensure only required app and dependencies are present.
- Remove unrelated packages and temporary files.
- Provide a short handoff note:
  - extraction mode (`prune-only` or `standalone-converted`)
  - target app path
  - known limitations or manual follow-ups
