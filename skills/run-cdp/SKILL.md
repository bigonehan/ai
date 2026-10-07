---
name: run-cdp
description: Open the dedicated WSL Chrome launched by the run_cdp Fish function, discover and verify its CDP endpoint, and attach to it for browser automation, browser tests, or site access.
---

# Run CDP browser

Use this skill when a task needs browser automation, browser testing, or access to a live website through the dedicated `run_cdp` browser. It provides the browser runtime and attachment boundary; it does not replace the task's product-specific test or UI skill.

## Runtime ownership

- `run_cdp` owns the dedicated WSL Chrome process and profile. Use that function for the browser start path.
- Treat an already-running endpoint as an attach target. Do not launch a second browser, replace the profile, or close the Chrome process during attach-mode cleanup.
- The default endpoint is loopback (`127.0.0.1`). This is sufficient for another agent in the same WSL runtime; do not widen the bind address unless the task explicitly requires a cross-OS or remote route.
- `run_cdp` runs in the foreground after CDP readiness. Keep that process alive while browser work is running. Stop it only when the task's owned browser session should end.

## Open and discover the session

1. Check whether the intended `run_cdp` endpoint is already healthy by querying `http://127.0.0.1:9222/json/version`.
2. If it is absent, invoke the real Fish function:

   ```fish
   run_cdp
   ```

   Preserve the long-running command and read the printed `endpoint=` value. Do not infer an endpoint from a port variable or a successful launch message alone.
3. Query the discovered endpoint's `/json/version` response and require HTTP success, valid JSON, a Browser field, Protocol-Version, and `webSocketDebuggerUrl`.
4. Confirm the running process owns the expected dedicated profile and remote-debugging port. Record only safe metadata such as PID, endpoint origin, Browser version, target count, and artifact digest.

## Authentication and login boundary

- Detect authentication requirements before account-specific actions: sign-in forms, login redirects, authentication overlays, expired-session pages, consent gates that require the user, or an otherwise blocked protected route.
- Never request, enter, copy, store, or log passwords, one-time codes, recovery codes, cookies, access tokens, or authorization headers. Do not extract credentials from the browser profile.
- Stop the protected flow at the authentication boundary and run:

  ```fish
  nf -m "Codex input required: browser authentication or login is required"
  ```

- After the notification, ask the user to complete authentication in the already-open `run_cdp` browser. Do not launch another browser or create a second profile. If `nf` is unavailable or fails, report that notification failure and still preserve the same no-credential boundary.
- After the user reports completion, reconnect or continue through the same CDP session, verify the authenticated page state, and resume the original user-visible flow. Login success alone is not the final result; read the declared authoritative consumer afterward.

## Attach and verify

- Prefer Playwright `connectOverCDP` or the runtime's supported CDP client for an existing Chromium session.
- After connecting, call real browser-level commands such as `Browser.getVersion`, `Target.getTargets`, and, for the selected target, `Target.getTargetInfo`. A `/json/version` response or transport connection alone is not an attach proof.
- Select the target using its actual origin, type, and current target metadata. Do not use a stale target ID or assume the first target is the requested page.
- In attach mode, disconnect the automation client only. Never call a launch path or close the user/`run_cdp` browser.
- For another Codex session, use an attach-only probe: do not run `run_cdp`, restart Chrome, kill a profile process, or modify the browser profile. Verify PID and endpoint survival after disconnect.
- Use raw CDP only for bootstrap, endpoint, target, and lifecycle verification. When the governing test workflow selects Stagehand, the user-visible flow itself must use Stagehand; do not mix raw CDP actions into that final flow.

## Website and browser-test use

- Drive the requested flow from the real user-visible start action through the production handler, transport, browser page, and authoritative consumer. Helper calls, ACKs, target discovery, and model selection are intermediate evidence.
- Keep one stable operation ID and record each boundary's safe metadata: owner, action, attempt, state, counts, byte lengths, status, target origin, and digest. Never record credentials, cookies, authorization headers, raw payloads, or private URL query values.
- Before the flow, snapshot the authoritative consumer state. Afterward, read back the actual page, persisted state, download, or other declared consumer; do not claim success from a screenshot or an existing filename alone.
- Exercise relevant reload, navigation, extension reload, service-worker restart, reconnect, retry, and cleanup transitions in the same attached runtime when they are part of the requested behavior.
- If the endpoint or artifact is stale, identify the first failed boundary, repair the product or runtime setup, reload the same session, and repeat the same user-visible flow. Do not silently switch to a newly launched browser.

## First-command completion continuity

- Register the user's first requested browser action as an active completion contract before attaching or probing. The action remains active until its declared authoritative consumer predicate is true.
- A stale endpoint, stale artifact, missing hook, failed attach, missing event, or absent download is an intermediate failure boundary. Do not end the task or send a conclusion report because the runtime is `runtime-unverified`, `incomplete`, or failed.
- While any repair, reload, reconnect, retry, artifact deployment, or consumer readback remains agent-executable, perform it in the same owned session and resume the same user-visible flow. Preserve the operation ID and record the first failing boundary.
- For download flows, completion requires the new file in the declared Downloads directory plus existence, nonzero size, SHA-256, and actual byte readback. Endpoint health, target discovery, hook presence, download request, or an existing filename is not completion evidence.
- Only when all remaining work is a user-exclusive action may this skill hand off through the input gate. That handoff is a continuation request for the active contract, not a terminal report.

## Delegated completion loop

- When the user says to continue until completion, register one active browser completion contract owned by the main session. The delegated agent is an implementer or diagnostician, never the completion authority.
- Before any terminal update, the main session reads the delegated result, diff, logs, artifact digest, and authoritative consumer. A delegated `complete` claim without consumer readback is an intermediate result.
- If the result is incomplete, failed, runtime-unverified, or missing evidence, immediately create a new bounded task for the next owner boundary and delegate it again. “Delegate again” means a new cycle of reproduce the failure, identify its cause, implement a new improvement at the owning boundary, test it, obtain the worker success/failure judgment, and compare success against the original user requirement; it is not a blind retry. The task must state the first failed boundary, the new repair objective, the exact runtime replay, and the evidence to return.
- `bounded` limits one worker task to a finite file scope, one owner boundary, explicit outputs, and explicit evidence; it is not a retry cap for the active completion contract. If the same boundary fails again, escalate ownership to the relevant configuration, loader, or runtime layer and create a new repair task instead of stopping.
- Repeat `failure boundary → repair delegation → artifact reload/reconnect → same production flow → consumer readback` until the completion predicate is true. Do not send a status-only conclusion between iterations.
- If delegation is unavailable or a worker slot fails, the main session performs the same repair and verification loop directly and resumes delegation when possible. A worker failure is not a reason to stop the active contract.
- For downloads, terminate only after the newly created file is read from the declared directory with nonzero size, SHA-256, and actual byte readback. Existing files, a download request, or a browser status are not completion evidence.

## Test Manager integration

For a website-related implementation, use this skill at the final overall-flow exploration stage after the applicable unit, production-boundary integration, and built-artifact checks. The Test Manager scenario must:

- start at the same user action and use the same input as earlier stages;
- use the production-loaded artifact in the `run_cdp` browser;
- trace producer → transport/handler → site/page → authoritative consumer;
- keep runtime evidence separate from lower-layer checks and record the first failed boundary;
- read back the authoritative consumer before marking the website flow verified;
- preserve adjacent workflows and disconnect without closing the owned browser.

If the endpoint cannot be reached, classify the exact boundary (missing listener, wrong route, stale artifact, attach failure, page failure, or consumer failure) and continue with safe repairs before reporting `runtime-unverified`.
