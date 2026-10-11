---
name: run-cdp
description: Run a project's web/browser runtime check in a visible (headless: false) WSL Chromium with the dedicated login-keeping profile on the shared CDP endpoint 127.0.0.1:9222, driven by Stagehand through the stagehand-access skill, from the real start action to the authoritative consumer readback.
---

# Run CDP browser

Use this skill when a project needs a real browser check (web page, extension, Grist widget, external site) after its focused/unit, production-boundary, and built-artifact checks pass. Take the target URL, artifact (extension directory), start action, and authoritative consumer from the project's nearest `AGENTS.md`; never hardcode another product's values.

## Browser and profile

- The browser is always visible: `headless: false`. Use headless only when the project's `AGENTS.md` explicitly requires a headless-only runtime.
- One shared endpoint `http://127.0.0.1:9222` and one dedicated profile `/home/tree/.config/codex-run-cdp`. The profile keeps logins (for example the local Grist at `http://localhost:8484`) across sessions; never create a second profile or copy cookies.
- A healthy endpoint is attached to, never replaced implicitly. Restart it only to load a new extension build: `run_cdp --replace -e <extension dir>`.

## Run with Stagehand

Stagehand itself (runtime, AI model and key, opening the session) belongs to the `stagehand-access` skill. Open the session through it; it attaches to the healthy endpoint or launches the dedicated-profile Chromium with `headless: false` and `keepAlive`, and closing the session only disconnects.

```js
import { openStagehand } from "/home/tree/.codex/skills/stagehand-access/scripts/stagehand.mjs";
const session = await openStagehand({ url, extensions: [extensionDir] });
// session.stagehand.act / observe / extract, session.page, session.mode ("attach" | "launch")
await session.close();
```

Quick check from a shell: `node ~/.codex/skills/stagehand-access/scripts/stagehand.mjs open --url <URL> [--extension <dir>]` prints mode, browser, `headless`, whether AI is enabled, URL and title.

- Use Stagehand for the user-visible flow. Raw CDP (`/json/version`, `/json/list`) is only for checking the endpoint and targets.
- The `run_cdp` fish function opens the same visible browser without Stagehand, for example for a one-time login: `run_cdp --url http://localhost:8484 --detach`.

## Whole flow

1. Read the project's runtime settings from `AGENTS.md`; read Table26 through the `grist-access` skill when the stage matters.
2. Open the session (with the deployed extension directory when the product is an extension) and confirm the loaded extension version/digest matches the build.
3. Read the authoritative consumer's state before acting.
4. Perform the real start action, then follow producer → transport/handler → page or site → authoritative consumer, and read the consumer back. A blank page, target list, build, log, or ACK alone is not a runtime result.
5. Report the browser version, `headless: false`, target URL/title, artifact version, and the consumer readback in the command output and the project's `.ai/changes` record. Never record credentials, cookies, authorization headers, or private query values.

Allow one retry for a newly evidenced failure and about five minutes of browser work per check; do not open repeated tabs or sites without new evidence. An exhausted budget is not a reason to end the task: record the first failing step in Table26 `$I` and continue with the next repair boundary.

## Checks that decide the result

- Use Stagehand for the visible user flow and its page object (`session.page`) for exact DOM reads; raw CDP calls are diagnostics only. Playwright scripts and per-project browser profiles are legacy and are not used.
- After an extension reload/update, service-worker restart, page reload or SPA navigation, read back the loaded extension version and one product-owned readiness marker before continuing; a reload command alone does not prove the page uses the new build. Watch for stale listeners, ports and target IDs.
- Start from the real visible control (button, editor, upload, playback, download) rather than a popup, helper endpoint, synthetic event or internal function.
- Name the first failing step instead of "CDP failed": endpoint absent/unreachable, wrong page or profile, stale or missing extension build, user action not received, message dropped or duplicated, handler rejected the payload, browser effect missing, consumer missing/stale/empty/unreadable.
- For a download or file consumer, list the destination before the flow and require a new file afterwards with the expected name, positive size, settled modification time and readable bytes (record its SHA-256). Resolve the real download folder from the running profile; an existing file with a matching name is not a result.

## Login and user-only actions

If the target shows a login page (for example Grist `403`/sign-in in the dedicated profile), payment, an irreversible send, or a cost, stop there, run the project's input notification rule, and ask the user for exactly that one action in the already-open visible browser. A login made in the dedicated profile persists, so later checks reuse it. Resume the same session afterwards and read the consumer back.

## Completion

The check passes only when the declared authoritative consumer shows the expected result. If it fails, set Table26 `$H=진단`, repair the owning code, rebuild/reload the same artifact, repeat the same start action, and read the consumer again. Item state follows `work-project`.
