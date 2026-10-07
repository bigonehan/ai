---
name: automation-browser
description: Configure, attach, and verify browser automation across CDP, Playwright, and Stagehand when browser ownership, cross-OS networking, extension artifacts, or final download/readback boundaries can affect correctness. Do not use for ordinary browser advice that does not require runtime automation.
---

# Automation Browser

Use this skill for browser automation that crosses a process, browser profile, extension, OS, CDP endpoint, or authoritative filesystem/download consumer. Keep the browser mode, artifact, session, and final consumer explicit before any UI action.

## 1. Select one browser ownership mode

Choose exactly one mode and record it in the run log:

- `attach`: the user already launched the browser and may have authenticated it. Attach to the existing process and profile; do not launch a second window, close the browser, replace its profile, or silently switch to a different browser.
- `launch`: the runner launches a dedicated browser process with a dedicated user-data directory. The runner owns cleanup for that process only.

Do not use a runner-owned browser to claim results from a user-owned session. Do not use a user profile as a substitute for a dedicated launch profile.

## 2. CDP preflight before automation

Before Playwright, Stagehand, or raw CDP control:

1. Identify the browser process owner and command line without exposing cookies, tokens, or profile contents.
2. Query the configured CDP endpoint's `/json/version` and validate HTTP status, JSON shape, browser identity, and `webSocketDebuggerUrl`.
3. Confirm the endpoint is reachable from the runner's OS. For WSL or another cross-OS boundary, discover the current route/bind address; do not assume `localhost`, a fixed gateway, port forwarding, or firewall state.
4. Query `/json/list` and select the intended page by origin/title/target type. Never use the first target as an identity contract.
5. Verify the exact loaded extension artifact and version in the browser. Compare the source build, deployed directory, manifest version, and SHA-256 before interpreting a runtime result.
6. Preserve the preflight result, target ID, endpoint origin, artifact digest, and ownership mode as safe metadata.

When a shell interprets CDP flags, quote wildcard-bearing arguments. For example, in fish use `--remote-allow-origins='*'`; an unquoted `*` is a filesystem glob, not a Chrome argument. Keep endpoint-startup commands separate from test commands.

## 3. Choose the automation layer

- Use Playwright's `connectOverCDP` (or the runtime's supported equivalent) for an existing Chromium CDP session when deterministic DOM inspection and direct browser attachment are required.
- Use Stagehand only when semantic `observe`/`act`/`extract` behavior is part of the requested test. Configure a valid model name, API-key environment variable, CDP URL, timeout, extension ID, and download root before creating the Stagehand instance.
- In attach mode, Stagehand/Playwright must connect and disconnect only. It must not call a launch path, create a second profile, or close the user's browser.
- In launch mode, use a dedicated profile, load the intended unpacked extension before the flow, and close both the automation wrapper and the runner-owned browser in `finally`.
- Do not mix raw CDP navigation, Playwright actions, and Stagehand actions in one authoritative flow unless the contract explicitly names the boundary. If a raw probe is needed, label it diagnostic and do not treat it as the final user-visible proof.

## 4. Extension and page lifecycle

Verify the production artifact in the actual browser session, then handle lifecycle transitions explicitly:

- extension reload/update;
- page reload and SPA navigation;
- content-script or MAIN-world injection;
- service-worker restart/reconnect;
- stale listeners, DOM roots, ports, and target IDs.

After each relevant transition, read back the actual loaded version and one production-owned readiness marker. A successful extension reload command alone is not evidence that the page consumed the new artifact.

Prefer the user's visible start action in the real page. Do not use a popup, helper endpoint, synthetic event, or direct internal function as a substitute when the acceptance boundary is a page button, editor, playback control, upload, or download.

## 5. Observe the whole producer-to-consumer path

Give every run one stable, non-secret operation ID and trace:

`user action → page/MAIN producer → isolated bridge or transport → background/handler → converter or external service → browser download → authoritative consumer readback`.

At each boundary record only safe metadata: boundary name, operation ID, attempt, state, counts, byte lengths, status, target origin, and digests. Do not log credentials, Authorization headers, cookies, private payloads, full personal paths, or raw media.

Distinguish these failures instead of collapsing them into “CDP failed”:

- endpoint absent or unreachable;
- malformed health response;
- wrong target or wrong browser profile;
- stale/missing extension artifact;
- producer did not receive the user action;
- transport/bridge dropped or duplicated the message;
- handler/converter rejected the payload;
- browser download effect did not occur;
- consumer file was absent, stale, overwritten, empty, or unreadable.

## 6. Runtime acceptance and retries

Use the same scenario and input through unit, boundary integration, build artifact, and real runtime. A helper return, DOM state, ACK, model response, `capture-chunk`, `capture-complete`, conversion result, or download API call is intermediate evidence, not final success.

Before the flow, snapshot the consumer directory and define the output contract. After the flow, require a new file with the expected extension/name lineage, positive size, stable mtime, readable bytes, and SHA-256. Do not accept an existing file merely because its name matches. Resolve the actual browser download directory from the running profile/configuration; do not hard-code a Windows path for WSL Chromium or vice versa.

If a runtime attempt is unverified, keep working while safe agent-executable steps remain: identify the first failed boundary, fix code/configuration, rebuild, redeploy, reload the same runtime, repeat the same visible action, and read the consumer again. Do not stop at a first timeout, stale process, missing observation, or runner retry. Do not manufacture success by dispatching an ended event, seeking to the end, or writing a fixture into the consumer directory; synthetic probes are diagnostic only and must be separated from authoritative evidence.

## 7. Stagehand baseline configuration

Keep these values explicit and validated before `new Stagehand(...)`:

- browser mode: attach or launch;
- CDP URL and `/json/version` timeout;
- valid model identifier and API-key environment variable name;
- browser/action/runtime timeout;
- extension ID and exact loaded version;
- song/page origin and intended target selection rule;
- download root and baseline snapshot path;
- run/evidence/observation paths under the owning project's temporary test directory.

When Stagehand initialization fails, close only resources that the selected ownership mode allows. In attach mode, disconnect the automation transport and leave the user's browser and profile running. In launch mode, close the dedicated browser and wrapper. Always preserve the first concrete failure boundary and do not report lower-layer receipts as a successful browser run.

## 8. Common regression matrix

Keep coverage for:

- CDP listener present/absent, timeout, refusal, malformed JSON, and wrong target;
- loopback-only versus cross-OS reachable endpoint;
- attached user browser versus dedicated launched browser;
- stale extension version after rebuild/reload;
- SPA navigation, full reload, extension update, and service-worker restart;
- duplicated listeners/ports/workers and one-action versus multiple-effect behavior;
- partial success, response loss, retry, timeout, and duplicate operation ID;
- valid conversion/download API result versus missing/empty/stale/unreadable final file;
- cleanup that disconnects an attached browser without closing it;
- cleanup that closes only runner-owned resources.
