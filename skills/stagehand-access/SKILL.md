---
name: stagehand-access
description: Use Stagehand (browser AI act/observe/extract) for extensions work through scripts/stagehand.mjs, which owns the pinned Stagehand runtime, the AI model name and API key in this skill's .env, and opens a visible (headless: false) session on the shared CDP endpoint with the dedicated profile. Use it whenever a task needs Stagehand, instead of installing Stagehand in a project, asking the user for a model key, or wiring Stagehand by hand.
---

# Stagehand access

Every Stagehand use in `/home/tree/extensions` work goes through `scripts/stagehand.mjs`. The model name and API key live in this skill's `.env` (mode 600, excluded from Git) and are loaded by the script; never ask the user for a key, never read or print `.env`, and never put a key in commands, files, logs or change records. Do not install Stagehand in a product project for agent checks, and do not create another browser profile.

## Commands

```bash
H=~/.codex/skills/stagehand-access/scripts/stagehand.mjs
node $H status                                   # runtime version, model, whether a key is set
node $H install                                  # (re)install the pinned runtime
node $H open --url <URL> [--extension <dir>]     # visible session, prints mode/browser/headless/title
```

`status` and `open` print JSON; errors go to stderr as JSON with exit code 1 (2 for wrong arguments).

## In a script

```js
import { openStagehand, loadZod } from "/home/tree/.codex/skills/stagehand-access/scripts/stagehand.mjs";
const session = await openStagehand({ url, extensions: [extensionDir] });
await session.stagehand.act("click the Test button");        // AI: needs the key
const { z } = await loadZod();
const data = await session.stagehand.extract("read the result", z.object({ ok: z.boolean() }));
await session.close();                                        // disconnects; the browser stays open
```

- `session.mode` is `attach` when `http://127.0.0.1:9222` was healthy, `launch` when the script started the dedicated-profile Chromium (`/home/tree/.config/codex-run-cdp`) itself with `headless: false` and `keepAlive`.
- `session.aiEnabled` is false while `.env` has no `STAGEHAND_API_KEY`; navigation and page reads still work, `act`/`observe`/`extract` do not.
- Which page to test, what to click and how to judge the result come from the `run-cdp` skill and the project's `AGENTS.md`.

## Files

- `.env` — `STAGEHAND_MODEL` (provider/model, default `google/gemini-3-flash-preview`) and `STAGEHAND_API_KEY`. Environment variables of the same names override it.
- `runtime/package.json` — pinned `@browserbasehq/stagehand` and `zod`; `install` copies it to the runtime directory `/home/tree/.local/share/stagehand-access` (override with `STAGEHAND_RUNTIME_DIR`) and runs `npm install` there.
