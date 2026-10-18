---
name: access-stagehand
description: Use Stagehand (browser AI act/observe/extract) for extensions work through scripts/stagehand.mjs, which owns the pinned Stagehand runtime, the AI model name and API key in this skill's .env, and opens a visible (headless: false) session on the shared CDP endpoint with the dedicated profile. Use it whenever a task needs Stagehand, instead of installing Stagehand in a project, asking the user for a model key, or wiring Stagehand by hand.
---

# Stagehand access

Every Stagehand use in `/home/tree/extensions` work goes through `scripts/stagehand.mjs`. The model name and API key live in this skill's `.env` (mode 600, excluded from Git) and are loaded by the script; never ask the user for a key, never read or print `.env`, and never put a key in commands, files, logs or change records. Do not install Stagehand in a product project for agent checks, and do not create another browser profile.

## Before and after a session

- Read `references/attach-notes.md` before opening a session and follow its `먼저 할 일` (checks that avoid known init/attach costs).
- On `runtime-stuck`, `A Stagehand instance is already initialized` or `Stagehand initialization is already in progress`, run `node ~/.codex/skills/access-stagehand/scripts/stagehand.mjs doctor --restart-runtime` once before any other retry: it reloads unresponsive page targets and restarts the Stagehand Runtime service worker, then reports JSON. When its `stillHung` list is not empty, rerun it with `--close-hung` to close only those tabs; a page that never answers makes every new Runtime init time out.
- After an init/attach failure or an open slower than 30 s, add an entry to `references/attach-notes.md` (symptom text, cause, recovery, time spent, prevention) or extend the matching entry's `재발`, and update `먼저 할 일` when the procedure changes. Never record keys, cookies or prompt text there.

## Commands

```bash
H=~/.codex/skills/access-stagehand/scripts/stagehand.mjs
node $H status                                   # runtime version, model, whether a key is set
node $H install                                  # (re)install the pinned runtime
node $H open --url <URL> [--extension <dir>]     # visible session, prints mode/browser/headless/title
node $H doctor [--restart-runtime] [--close-hung]  # probe page targets, reload unresponsive ones, restart the Runtime
```

`status` and `open` print JSON; errors go to stderr as JSON with exit code 1 (2 for wrong arguments).

## In a script

```js
import { openStagehand, loadZod } from "/home/tree/.codex/skills/access-stagehand/scripts/stagehand.mjs";
const session = await openStagehand({ url, extensions: [extensionDir] });
await session.stagehand.act("click the Test button");        // AI: needs the key
const { z } = await loadZod();
const data = await session.stagehand.extract("read the result", z.object({ ok: z.boolean() }));
await session.close();                                        // disconnects; the browser stays open
```

- `session.mode` is `attach` when `http://127.0.0.1:9222` was healthy, `launch` when the script started the dedicated-profile Chromium (`/home/tree/.config/codex-run-cdp`) itself with `headless: false` and `keepAlive`.
- Stagehand needs its own `Stagehand Runtime` extension (`/home/tree/.local/share/access-stagehand/node_modules/@browserbasehq/stagehand/dist/extension`) running in the browser. Both launchers (`run_cdp -e` and this script's `launch`) put it in `--load-extension` and `--disable-extensions-except` next to the product extension, and `run_cdp` adds `--enable-unsafe-extension-debugging`.
- Before attaching, the script reads the running browser's flags and fails at once (instead of the 60 s `Stagehand initialization timed out`) with one of these boundaries; each message names the `run_cdp --replace -e <dir>` restart that fixes it:
  - `extension-not-loaded` — an `--extension` you passed is not loaded in that browser.
  - `stagehand-extension-blocked` — `--disable-extensions-except` leaves out the Stagehand Runtime.
  - `stagehand-extension-unloadable` — no allow-list, but `--enable-unsafe-extension-debugging` is missing, so Stagehand cannot load its runtime.
  Restarting the shared dedicated-profile browser this way is part of the check, not a user action; logins persist in the profile.
- `session.aiEnabled` is false while `.env` has no `STAGEHAND_API_KEY`; navigation and page reads still work, `act`/`observe`/`extract` do not.
- Which page to test, what to click and how to judge the result come from the `run-cdp` skill and the project's `AGENTS.md`.

## Files

- `.env` — `STAGEHAND_MODEL` (provider/model, default `google/gemini-3-flash-preview`) and `STAGEHAND_API_KEY`. Environment variables of the same names override it.
- `references/attach-notes.md` — init/attach checklist and failure records that later agents read first.
- `runtime/package.json` — pinned `@browserbasehq/stagehand` and `zod`; `install` copies it to the runtime directory `/home/tree/.local/share/access-stagehand` (override with `STAGEHAND_RUNTIME_DIR`) and runs `npm install` there.
