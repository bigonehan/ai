---
name: development-environment
description: Configure and verify development environments that cross operating systems, host processes, browser automation, network endpoints, deployed artifacts, or similar runtime boundaries. Use when setup, connectivity, process ownership, artifact loading, or environment-variable instructions could fail before the application starts.
---

# Development environment

Use this skill when development or runtime work crosses operating systems, host processes, browser profiles, network endpoints, deployed artifacts, or environment variables. Establish an evidence-backed environment contract before application code, automation, paid actions, or irreversible effects run.

## Required workflow

1. Read applicable project and global instructions. Record the requested setup and exact acceptance boundary in the owning project log when the workspace requires it.
2. Classify ownership before acting:
   - `user-owned`: the user already launched the process or browser; the runner may attach and disconnect but must not launch, terminate, replace, or mutate its profile;
   - `runner-owned`: the runner explicitly launches a dedicated process/profile and is responsible for its lifecycle.
   Never mix the two modes or silently switch modes after a failure.
3. Map producer, transport, and consumer separately. Name the process, endpoint, OS boundary, loaded artifact, handler, and authoritative final consumer.
4. Perform a bounded preflight before automation or irreversible effects. Inspect the actual process and listening socket, query the authoritative health endpoint, validate its status and schema, and verify the transport address. Do not infer readiness from an environment variable, command-line flag, launch result, lower-layer test, or expected port.
5. Verify the exact artifact bytes that the production loader will consume. Compare declared paths, loader order, existence, permissions, and SHA-256 before runtime control.
6. Stop at the first concrete failed boundary and report a repair path. Distinguish configuration, listener absence, network-path failure, malformed health response, artifact mismatch, process/runtime failure, and final-consumer failure.
7. Keep credentials, cookies, tokens, private payloads, and full private paths out of logs. Record only masked endpoint metadata, process identity, status, schema fields, artifact digests, and safe counts.

## Process and browser attachment

- An already-running user process cannot reliably gain a debug endpoint after launch. If the required endpoint is absent, report that fact and request the smallest explicit user-side restart or launch action; do not kill or replace the process automatically.
- In attach mode, use the runtime's supported connect API. Do not launch a second window, use a different profile, or close the user's process during cleanup. Disconnect only the automation transport.
- In launch mode, use a dedicated profile and record the launch arguments, owner, and cleanup responsibility. Never attach to a user's normal profile as a substitute for a dedicated runtime.
- For cross-OS endpoints, determine the route and bind address from the current machine state. Do not hard-code a gateway, localhost assumption, port forwarding rule, or firewall result.
- A successful health endpoint is necessary but not sufficient: verify that the attached process owns the expected runtime, the expected artifacts are loaded, and the authoritative consumer can be read.

## Runtime verification contract

- Keep lower-layer, boundary-integration, built-artifact, and real-runtime evidence separate.
- A contract/schema test, status read, acknowledgement, helper return, or successful transport connection is not final success.
- For a user-visible workflow, run the same initiating action through the production loader and handler chain to the authoritative consumer. Preserve one stable operation identity, payload lineage, effect count, order, terminal state, and error projection across every boundary.
- Do not stop or report success at an intermediate stage when later deterministic stages remain executable. The runtime result is successful only when the declared authoritative consumer is read back.
- If the authoritative consumer cannot be reached because the user's process, credentials, live service, or hardware is unavailable, report `runtime-unverified` and name only that residual boundary after all agent-executable checks are complete.

## Command-file boundary

- `/home/tree/instruction.txt` is command-only. Store only the copyable commands the user explicitly needs for the requested environment operation.
- Do not store explanations, diagnoses, policies, test results, product-specific workflow text, or unrelated test commands in that file.
- When the requested operation is starting or exposing a runtime endpoint, keep the file limited to that endpoint-startup operation. Put test invocation and verification results in the project instructions or response unless the user explicitly requests those commands as part of the same command block.

## Regression matrix

Retain coverage for:

- listener absent versus listener present;
- loopback-only binding versus reachable cross-OS binding;
- connection refusal, timeout, firewall rejection, and malformed health response;
- valid endpoint with stale or missing loaded artifacts;
- valid endpoint and artifacts with a later handler, process, or authoritative-consumer failure;
- attach-mode cleanup that disconnects automation without closing the user-owned process;
- launch-mode cleanup that closes only the runner-owned dedicated process.

The first failed boundary must be observable in the output. Lower-layer mocks, process arguments, environment variables, and test contracts alone cannot establish runtime success.
