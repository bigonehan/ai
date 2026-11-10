---
name: access-stagehand
description: "Use Stagehand (browser AI act/observe/extract) for extensions work through scripts/stagehand.mjs, which owns the pinned Stagehand runtime, the AI model name and API key in this skill's .env, and opens a visible (headless: false) session on the shared CDP endpoint with the dedicated profile. Use it whenever a task needs Stagehand, instead of installing Stagehand in a project, asking the user for a model key, or wiring Stagehand by hand."
---

# Stagehand access

Every Stagehand use in `/home/tree/extensions` work goes through `scripts/stagehand.mjs`. The model name and API key live in this skill's `.env` (mode 600, excluded from Git) and are loaded by the script; never ask the user for a key, never read or print `.env`, and never put a key in commands, files, logs or change records. Do not install Stagehand in a product project for agent checks, and do not create another browser profile.

## Before and after a session

- Read `references/attach-notes.md` before opening a session and follow its `먼저 할 일` (checks that avoid known init/attach costs).
- `openStagehand()` owns recovery: it serializes sessions with a socket lock, resets the Runtime once when the previous owner died, probes and closes hung tabs, retries each error class once, and loads or updates `extensions` itself. Do not run `doctor` first, wrap the script in extra restarts, or call `run_cdp --replace` before it. When it still fails, its error names the boundary (see below); report that line.
- After an init/attach failure or an open slower than 30 s, look up the boundary name in `references/attach-notes.md` `오류별 제안` and follow it; for a new boundary or symptom add one two-line entry (`증상 :` boundary name and error text, `제안 :` the confirmed recovery action), and update `먼저 할 일` when the procedure changes. Never record keys, cookies or prompt text there.

## Commands

```bash
H=~/.codex/skills/access-stagehand/scripts/stagehand.mjs
node $H status                                   # runtime version, model, whether a key is set
node $H install                                  # (re)install the pinned runtime
node $H open --url <URL> [--extension <dir>]     # visible session, prints mode/browser/headless/title
node $H browser [--replace] [--extension a,b] [--url URL] [--detach]  # start/attach the shared browser (run_cdp calls this)
node $H stats [--last N]                         # p50/p90 per open stage and top error boundaries
node $H doctor [--restart-runtime] [--close-hung]  # manual probe only; openStagehand already does this
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

- `session.page` (always populated even when `url` is omitted) and `session.pages` (`await stagehand.browser.context.pages()`) are returned on `session`. Note that Stagehand v3/v4 `page` does **not** have `page.frames()`, and `stagehand.context` does **not** exist (`stagehand.browser.context.pages()` is used internally). For iframe elements, use `page.locator("iframe >> <selector>")`.
- When importing TypeScript server/src files alongside `openStagehand()` in Node 25 one-liners, always run `node --experimental-transform-types --input-type=module -e '...'` (do not use `npx tsx -e`, which defaults to CommonJS and fails on top-level `await`, and do not omit `--experimental-transform-types`, which fails on `readonly` parameter properties).
- One browser launcher: `openStagehand()` and `run_cdp` both go through `stagehand.mjs browser`, which starts the dedicated-profile Chromium (`/home/tree/.config/codex-run-cdp`, `headless: false`) with the Stagehand Runtime and `--enable-unsafe-extension-debugging`, and remembers extensions in `<profile>/run_cdp-extensions`. `session.mode` is `attach` or `launch`.
- `extensions` are compared with each `manifest.json` version and loaded (`Extensions.loadUnpacked`) or reloaded in place (`chrome.runtime.reload()`) without restarting the browser. `openStagehand()` never closes or restarts the shared browser (a browser started inside a Codex sandbox dies with the sandbox); when the in-place update cannot reach the new version it fails with `extension-reload-failed: … expected=… actual=…`, and the host restarts it with `run_cdp --replace`.
- The Stagehand Runtime allows one initialized client per browser, so sessions on a port run one at a time behind the socket lock `<tmp>/access-stagehand-locks/<port>.sock` (`STAGEHAND_LOCK_DIR`). A waiter prints `[stagehand-step] runtime-lock-wait owner=<pid> stage=<stage>` every 30 s and gives up after 4 min with `runtime-lock-timeout`. SIGINT/SIGTERM/SIGHUP close the session and release the lock; `STAGEHAND_LEASE_TIMEOUT_MS` forces a close after that long (default: none).
- Tabs the session opened are closed by `session.close()` (`keepPages: true` keeps them); tabs left by a dead session are closed by the next one.
- Each open prints `[stagehand-summary] {"lockWaitMs",…,"retries","tabs","mode"}` and appends it to `~/.local/state/access-stagehand/sessions.jsonl` (`STAGEHAND_SESSION_LOG_FILE`); `stats` summarizes it. Error boundaries: `endpoint-absent`, `endpoint-unreachable`, `cdp-timeout: <method>`, `runtime-lock-timeout`, `runtime-worker-absent`, `runtime-worker-attach-failed`, `runtime-wake-failed`, `extension-manifest-absent`, `extension-load-failed`, `extension-reload-failed`, `stagehand-extension-blocked`, `stagehand-extension-unloadable`.
- `session.aiEnabled` is false while `.env` has no `STAGEHAND_API_KEY`; navigation and page reads still work, `act`/`observe`/`extract` do not.
- Which page to test, what to click and how to judge the result come from the `run-cdp` skill and the project's `AGENTS.md`.

## Files

- `.env` — `STAGEHAND_MODEL` (provider/model, default `google/gemini-3-flash-preview`) and `STAGEHAND_API_KEY`. Environment variables of the same names override it.
- `references/attach-notes.md` — init/attach checklist and failure records that later agents read first.
- `runtime/package.json` — pinned `@browserbasehq/stagehand` and `zod`; `install` copies it to the runtime directory `/home/tree/.local/share/access-stagehand` (override with `STAGEHAND_RUNTIME_DIR`) and runs `npm install` there.
