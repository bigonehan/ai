---
name: test-manager
description: Plan, write, run, and review unit, integration, regression, and end-to-end tests with explicit observation-unit selection, producer-consumer contracts, domain identity and duplicate-name collision resolution, semantic side-effect cardinality, propagation tracing, state transitions, retry boundaries, repeated-regression escalation, typed-value validation, real template rendering, URL decoding, Unicode, cross-OS path mapping, and actual file access. Use whenever Codex changes or runs tests, investigates a missed or repeatedly reported regression, claims behavior is verified, or debugs values or files that are present but cannot be consumed.
---

# Summary

Follow this order and keep each check distinct:

1. Read applicable rules, requirement history, project test commands, and existing tests.
2. Build a requirement-to-evidence ledger from the current request and `Input.md`, then freeze scope and one acceptance scenario for every target, site, surface, and lifecycle condition.
3. Split the visible artifact into data records, nodes inside one component root, document-level root or process instances, and pixels; measure each scope before selecting the authoritative observation unit.
4. Map producer, propagation, consumer, result, ownership, reinjection, reload, restart, and cleanup boundaries, then identify irreversible effects, semantic effect cardinality, fan-out, fan-in, retries, fallbacks, and persistence stages.
5. Read actual runtime logs or traces, identify the first failing boundary, reproduce the defect on the buggy baseline or an isolated equivalent mutant, and confirm the regression test is red.
6. Build stateful production-shaped fixtures and the applicable input, value, boundary, failure, restart, and size test matrix. Every contract declares whether user-editable input validation applies.
7. Run narrow unit tests and verify exact effect ledgers, payloads, order, consumer state, and final outcomes.
8. Run the built artifact in its real runtime and observe the authoritative consumer; unit, mock, jsdom, source-string, and helper-return checks cannot replace this step.
   Drive the initiating action through the production input boundary. Direct property assignment, synthetic event dispatch, handler invocation, or state injection is not user-action evidence.
9. Run the project-required full check and any applicable renderer, path, file-access, build, copy, or deployment verification.
10. Validate a runtime evidence record with `scripts/verify_runtime_evidence.py`; report completion only when the gate accepts it.

# Test Manager

Treat passing tests as evidence only when they exercise the behavior and boundary that can fail in production.

## Scope Contract Hard Gate

Before the first mutation, record the allowed initiating action, expected final effect, authoritative consumer, allowed code roots, watched roots, forbidden roots, and adjacent workflows. Treat a mentioned dependency, data source, shared helper, bridge, background process, or provider as read-only unless the user explicitly authorized changing it. A request to add behavior to one UI does not authorize changing a similar or downstream workflow in another project.

If the smallest valid fix requires another project or workflow, stop before editing it, run the applicable input-notification gate, and obtain explicit scope approval. Silence, approval of unrelated implementation details, and the fact that a dependency supplies data are not cross-project authorization.

Capture the pre-mutation filesystem state with `verify_runtime_evidence.py snapshot`. At validation, any new change outside `allowed_roots` or inside `forbidden_roots` fails completion. Include adjacent repositories in `watch_roots` when accidental cross-project editing is a realistic risk.

## Behavior Preservation and Guard Reachability Hard Gate

When changing a shared predicate, readiness check, early return, parser, retry classifier, fallback selector, or lifecycle gate, set `requires_behavior_preservation: true` in the runtime contract and record a `behavior_change_analysis` before editing production code.

For every changed contract point, record the before/after contract, every previously accepted state, every downstream branch reachable before the change, and the preservation scenarios that exercise them. Partition previous states and downstream branches exactly into preserved or intentionally removed sets. An intentionally removed state or branch requires a linked requirement ID whose source text explicitly authorizes that removal. Silence, a stricter-looking helper, a new safety check, or a passing test is not removal authorization.

Treat a guard and the code below it as one behavior surface. A test of the guard helper alone is insufficient when moving or tightening that guard can make a fallback, deferred path, retry, or provider-specific exception unreachable. Add a regression mutant that reproduces the changed guard or early return and prove the production-path scenario fails against it.

Every preservation scenario declares `coverage_kind: preservation` and the lifecycle conditions that affect reachability, such as foreground or background, active or inactive tab, visible or hidden document, sidebar open or closed, reload, and service-worker restart. Unit and runtime observations must report the same lifecycle conditions. Do not substitute an active-tab success for an inactive-tab requirement or combine provider-specific lifecycle behavior into one generic scenario.

Source-string assertions may confirm packaging but cannot prove behavior preservation, branch reachability, or lifecycle behavior. Exercise the production orchestrator through the changed guard to the authoritative consumer.

## Requirement Coverage Hard Gate

Before defining tests, read the current request and the complete relevant incident history in `Input.md`. Create one immutable `requirement_coverage` entry per distinct requested outcome and preserve its exact source text and digest. Do not collapse different sites, UI surfaces, output formats, open/closed states, reload states, or lifecycle conditions into one generic requirement.

Map every requirement ID to at least one `acceptance_scenarios` entry. Each scenario declares its own origin, initiating user action, authoritative consumer, and expected output count. The union of scenario origins must exactly equal `runtime_target.required_origins`; never reduce a multi-target request to the easiest available target. Every scenario must have its own passing unit receipt and, when regression evidence is required, its own live pre-fix failure and live post-fix success receipt.

Record every repeat of the same incident in `incident_history`, including the source statement and the evidence boundary missed previously. `repeated_report_count` must equal this history length. A partial result may be reported per scenario, but the overall workflow remains `runtime-unverified` until every mapped requirement is closed.

## Logs-First Diagnosis Hard Gate

Do not state a root cause or apply a functional fix solely from code inspection when a runtime failure is reported. Read the existing application, browser, extension, process, server, or platform logs and identify the first observed failing boundary. If usable logs do not exist, the only permitted first change is diagnostic instrumentation; run the exact user action, retrieve its logs, then decide the product fix.

Use `(initiating action, visible failure, authoritative consumer)` as the incident signature, independent of wording. On the second report of the same signature:

1. Mark every previous completion claim as a missed regression.
2. Freeze further hypothesis-driven product edits.
3. Explain why the preceding tests passed and name the unobserved boundary.
4. Require a runtime log, trace, or stack artifact and a named failing boundary before another functional patch.
5. Require the same scenario to fail before the fix and succeed after it.
6. Reproduce the exact runtime pre-state, including every condition that was simultaneously true, and drive the initiating action through every previously missed boundary to the authoritative consumer.
7. Do not claim completion until the delivered artifact succeeds in that same runtime pre-state and the authoritative consumer is read back. A different fixture, probe, no-collision case, earlier boundary, or user-visible intermediate state cannot close the incident.

Until runtime validation succeeds, all user-facing updates, including commentary, must say `candidate`, `log-grounded hypothesis`, or `runtime-unverified`. Do not use completion, resolution, confirmed-cause, or working language before the gate accepts the delivered artifact.

Do not combine diagnostic instrumentation with a functional fix when the log identifies only a category such as `prompt-surface`, `receiver missing`, or `download failed`. First deploy instrumentation only, reproduce the exact action, and capture the concrete selector, owner, value, request ID, status, or consumer error. A repeated incident requires `runtime_diagnosis.specific_failing_detail`; a generic boundary name cannot authorize a functional patch.

### Rotating log source integrity

Treat Chrome LevelDB, browser logs, rolling files, and process logs as rotating sources. Declare `runtime_log_sources` as directories or authoritative files before the action. Snapshot every source file's name, size, and mtime; after the action inspect every new or changed file and record the newest one. Never pin a filename such as `000004.log` and infer “no new event” without checking whether a newer `.log` or `.ldb` exists.

Every runtime action uses a unique run ID and action timestamps. Runtime log records must come from a declared source, contain the same run ID, have an mtime after the action began, and include the newest rotated or changed file. If freshness, rotation, extension identity, or run correlation cannot be established, absence of an event is not evidence.

## Compound State Boundary Matrix

Before writing a regression test, extract every independent state axis that participates in the observed branch, such as nullable current value, collision absent/present, first/subsequent item, active/inactive lifecycle, retry phase, and persisted/stale state. Record the exact observed tuple in the contract.

When a failure requires two or more simultaneous conditions:

1. Add one regression case for the exact conjunction. Tests that cover each condition separately do not cover the conjunction.
2. Add near-neighbor cases that change one axis at a time, so the suite distinguishes the failing interaction from either condition alone.
3. Exercise the production predicate, deduplication, fallback, or consumer that combines the axes; a helper returning the intended value is insufficient.
4. Prove the pre-fix source or an isolated equivalent mutant fails the exact-conjunction case with the same boundary error.
5. Assert both positive effects and negative scope at the authoritative consumer, including displaced, replaced, preserved, and absent entities.
6. For a repeated incident, run the post-fix delivered artifact with the exact conjunction in the actual runtime. Stop at no intermediate boundary, even if that boundary was the cause of an earlier attempt.

Use a compact matrix before accepting the suite:

| Axis | Observed failing value | Near neighbor | Consumer effect |
| --- | --- | --- | --- |
| State axis | Exact runtime value | One-axis variation | Persisted or visible readback |

## Workflow

1. Read the applicable project test commands and existing tests before changing code.
2. Write the behavior contract as:
   - producer value and type;
   - consumer expectation;
   - state before the action;
   - externally visible side effects;
   - success evidence;
   - retryable and terminal failure stages.
3. Select the observation unit with the procedure below.
4. Identify the first irreversible or user-visible effect, such as paste, Enter, tab creation, file write, API mutation, download, or notification.
5. Build the test matrix below before accepting the suite.
6. Run the narrow tests first, then the relevant integration or boundary tests, then the project-required full check.
7. Report what reached the real consumer and what remains mocked.

## Unit-to-Real-Runtime Verification Ladder

For every user-visible scenario, use one stable scenario ID and preserve the same meaningful input and expected outcome through all levels:

1. **Unit:** exercise the smallest producer, parser, state transition, or consumer contract. Record the exact command, boundary observed, input, output, effect ledger, and limitations.
2. **Boundary integration:** execute the production handler, loader, transport, template, or persistence boundary. Do not replace production code by directly flipping readiness, success, listener, submission, or consumer state in a fake.
3. **Built artifact:** build or package the deliverable and record its SHA-256. Unit source success does not prove the delivered bytes contain the change.
4. **Real runtime:** load those exact artifact bytes in the actual browser, process, CLI, host application, or external consumer; perform the initiating user action and read the authoritative consumer result.
5. **Cross-level comparison:** require the unit and real-runtime receipts to share the scenario ID and user action. Compare payload identity, effect count, order, final outcome, and applicable errors. A runtime result that differs from the unit contract fails the task even when both commands exit zero.

Run unit tests first to obtain fast diagnostic evidence, then integration tests, then the actual-runtime check. Never describe unit, mock, jsdom, static inspection, build, copy, or checksum evidence as actual operation. If the actual runtime is unavailable, report the unit and integration results separately and leave the feature `runtime-unverified`.

## Mandatory Runtime Evidence Gate

User-visible behavior is complete only after the production build or delivered artifact runs in its real runtime and the authoritative consumer is read back. A test named `e2e`, a production-shaped fake, jsdom, mocked Chrome APIs, a source-string assertion, build success, and copied-file checksums are not runtime evidence.

For browser and extension work, load the delivered extension in a real browser, perform the exact user action, and inspect the resulting DOM, editor value, message response, download, navigation, or persisted state. Exercise reload, reinjection, and service-worker restart when those lifecycle boundaries participate in the complaint.

### Runtime identity and output-location hard gate

Runtime identity is part of the test contract, not descriptive metadata. Declare `runtime_target` with the required environment, exact live origins, whether fixture origins are allowed, and the authoritative output root. Runtime observations must provide matching `runtime_provenance`.

- A localhost page, cloned DOM, production-shaped fixture, temporary profile, isolated Chromium, or headless browser may prove a boundary integration only. It cannot prove behavior on ChatGPT, Gemini, Grok, Flow, Kling, the user's browser profile, or any other named live service.
- When the contract names Windows user Chrome, evidence must come from headed Google Chrome on Windows with the actual user profile. Extension reload or version visibility proves deployment only, not the requested site action or download.
- For unpacked Windows Chrome extensions, declare the extension ID, WSL artifact path, Windows registration path, version, and Secure Preferences path. The validator must match the registered path and service-worker version and require runtime provenance to name the same loaded extension.
- Record every visited page origin. The validator requires all declared live origins and rejects loopback, `file:`, and `data:` origins when fixture origins are disallowed.
- For downloads and file creation, the authoritative consumer is the file in the declared user-visible output root. Record its absolute path, byte size, and SHA-256, then keep it present until validation reads the bytes. A file in an isolated browser's download directory does not prove a file exists in the user's Windows Downloads folder.
- Each output must be created after its scenario action begins, and its count must exactly match that scenario's declared output count. Reusing an older matching file is not success evidence.
- Do not delete, move, or clean up authoritative outputs before `validate`. Cleanup may occur only after the gate result has been recorded and must be reported separately.

If the actual service, browser/profile, origin, output root, or final bytes cannot be observed, label the result `runtime-unverified`. State precisely which lower test layer passed; never promote it to live-site or user-download evidence.

## Input Interaction Hard Gate

Every test contract must contain `input_validation` with an explicit boolean `applicable`, a non-empty `reason`, and a `surfaces` array. Set `applicable: true` whenever the task creates, changes, fixes, or depends on a user-editable text field, textarea, contenteditable surface, select, toggle, file input, paste/drop target, keyboard shortcut capture, or equivalent input producer. `applicable: false` is allowed only when no user-editable input surface participates; keep `surfaces` empty and state the concrete reason. Do not silently omit the decision.

For every applicable surface, declare a stable `surface_id`, `input_kind`, linked `scenario_ids`, `ime_applicable`, `commit_required`, `cancel_required`, `persistence_required`, `authoritative_consumer`, and `required_runtime_observations`. Each surface must link to at least one acceptance scenario, and every linked scenario must produce its own input evidence.

Before accepting an input implementation, test the full state transition rather than the presence of a handler:

1. Render the real target, hit-test its coordinates, focus it through the production pointer or keyboard path, and record the focused element after Shadow DOM retargeting.
2. Enter the first character through the native input channel, then continue with multiple characters. Text-like inputs must also exercise deletion, replacement, and selection when those operations are supported.
3. When `ime_applicable` is true, exercise `compositionstart → compositionupdate → beforeinput/input → compositionend` with a composed string such as Korean input and verify the committed value once, without assuming one keydown per character.
4. Snapshot DOM or framework event-derived values synchronously before passing them into deferred callbacks, promise continuations, state updater functions, queues, timers, or effects. Add an isolated mutant that releases or nulls the event target before the updater runs; the regression test must fail against the delayed-read mutant.
5. After the first input and after continuous or composed input, verify the controlled value, component/root count, visible surface, focus ownership, and uncaught error list. An input test fails if the intended value exists but the dialog, component, root, or page disappears or remounts unexpectedly.
6. Exercise every supported commit route and cancel route independently. Verify the authoritative consumer after commit and prove cancel leaves it unchanged. When persistence is required, reload or restart through the production lifecycle and read the value back.
7. Exercise relevant host capture listeners, shortcuts, `preventDefault`, `stopPropagation`, overlays, and remounts. Confirm that input events neither leak into unrelated shortcuts nor get swallowed before reaching the editor.

Runtime evidence for each applicable surface must include `rendered_target`, `hit_test`, `focus`, `first_input`, `continuous_input`, `event_trace`, `event_value_snapshot`, `value_after_input`, `root_count_after_input`, and `uncaught_errors`. `event_value_snapshot: true` means the value was synchronously copied before deferred work; the released-target mutant must make this assertion fail when the implementation reads the event later. `value_after_input` records only `length` and SHA-256, never raw text. Add `ime_composition`, `commit_readback`, `cancel_readback`, and `reload_readback` when their surface flags require them. Property assignment, `dispatchEvent`, direct handler calls, framework state injection, or helper-only tests cannot satisfy this gate.

## User-Action Fidelity Hard Gate

Running inside a real browser or process does not by itself make a check an actual-runtime user test. Drive the scenario through the same production input boundary the user crosses. For pointer and keyboard UI, use browser automation input primitives at rendered coordinates, verify hit testing and focus, type through the keyboard input channel, and then observe the production event path and authoritative consumer.

The following are synthetic shortcuts and cannot prove the initiating user action: assigning `value`, `checked`, `textContent`, framework state, storage, or editor state; calling `click()`, a handler, helper, listener, submit function, or consumer directly; using `dispatchEvent`; or invoking page functions to manufacture focus or success. They may be used for unit setup only and must be reported as bypassed layers, never as runtime evidence.

For editable UI, capture the rendered target and coordinates, hit-test result, `activeElement` including Shadow DOM focus, capture/target/bubble event ledger, `defaultPrevented`, value after keyboard input, commit or blur transition, persistence after reload or restart, and authoritative read-back. Exercise hostile host-page capture listeners, keyboard shortcuts, `preventDefault`, `stopPropagation`, Shadow DOM retargeting, overlays, and remounts when the UI is embedded in a third-party page or extension surface. A test that only proves programmatic assignment persists does not cover clickability, focusability, or keyboard entry.

Every runtime contract must declare `input_fidelity` with `production_boundary`, `required_observations`, and `forbidden_shortcuts`. Every runtime observation must name the input driver, match that boundary, contain no bypassed layers or synthetic shortcuts, and provide each declared observation. If the production input boundary cannot be automated or directly observed, report `runtime-unverified`.

Use a contract JSON outside the repository. It must contain `task_id`, `workflow`, `user_action`, `expected_outcome`, `unit_observation`, `unit_test_commands`, `authoritative_consumer`, `requirement_coverage`, `acceptance_scenarios`, `incident_history`, `runtime_log_sources`, `input_fidelity`, `runtime_target`, `repeated_report_count`, `allowed_roots`, `watch_roots`, `forbidden_roots`, `adjacent_workflows`, and `delivered_artifacts`. Repeated reports also require `runtime_diagnosis` with a runtime artifact, named failing boundary, and concrete `specific_failing_detail`. Shared control-flow changes additionally require `requires_behavior_preservation: true`, `behavior_change_analysis`, preservation scenarios, and matching lifecycle observations.

Capture scope before mutation:

```bash
python3 /home/tree/ai/skills/test_manager/scripts/verify_runtime_evidence.py snapshot <contract.json> <state.json>
```

Run each pre-fix and post-fix runtime command through the validator rather than writing evidence manually:

```bash
python3 /home/tree/ai/skills/test_manager/scripts/verify_runtime_evidence.py run <contract.json> <state.json> <evidence.json> -- <runtime-command...>
```

Every command must write structured observation JSON to `TEST_MANAGER_OBSERVATION_PATH`. Unit observations use `phase: unit`, `observation_level: unit`, the declared scenario and requirement IDs, and the declared `unit_observation`. Actual checks add `phase: pre_fix` or `post_fix`, `observation_level: runtime`, run ID, action timestamps, actual runtime provenance, authoritative consumer, authoritative outputs, loaded artifact hashes, fresh runtime log records, uncaught errors, unobserved layers, and input-fidelity evidence. Preserve the same scenario ID, requirement IDs, and user action across levels. The validator signs each receipt and records the command output and exit code; hand-authored command claims are rejected.

Before any completion claim or completion notification, run:

```bash
python3 /home/tree/ai/skills/test_manager/scripts/verify_runtime_evidence.py validate <contract.json> <state.json> <evidence.json>
```

Validation requires every requirement to map to scenarios, every scenario to have the required unit/pre/post receipts, an exact origin union, an unchanged scope contract, no unauthorized changes, fresh run-correlated logs including rotated files, no uncaught errors or unobserved layers, correct extension registration, and hashes proving the runtime loaded every delivered artifact and created every expected output after the action. If any boundary is unavailable, report `runtime-unverified`; do not claim completion and do not run the task-complete notification.

Run the gate's own negative and positive fixtures after changing this skill:

```bash
python3 /home/tree/ai/skills/test_manager/scripts/verify_runtime_evidence.py self-test
```

## Observation Unit Selection

Choose the observation unit from the reported user behavior before choosing mocks or assertions:

1. Write the initiating action and expected visible outcome in domain terms, such as `one thumbnail click -> one image attached`.
2. Trace the production path in order: producer action, event or request, handler, helper, propagation or transport, consumer callback, authoritative consumer state, and visible result.
3. Record cardinality at every boundary. Distinguish producer actions, handler invocations, low-level calls, propagated deliveries, consumer invocations, and final state changes; never substitute one count for another.
4. Mark fan-out and fan-in points. Include DOM capture and bubbling, delegated listeners, multiple targets, subscriptions, queues, retries, fallbacks, broadcasts, batches, and deduplication.
5. Select the first authoritative observation point that directly represents the complaint. Prefer consumed records, attached files, committed rows, sent requests, or rendered state over an upstream helper call or return value.
6. Assert both ends: the initiating action occurs as intended and the authoritative consumer effect has the required count, identity, payload, order, and final state.
7. Build a production-shaped fixture with the topology that can multiply or collapse effects, such as nested DOM targets and bubbling listeners, multiple subscribers, repeated messages, or fallback branches.
8. Make stateful fakes update their state from received events or calls. Do not stub the final verifier independently of the path being tested.
9. If the authoritative consumer cannot be exercised safely, observe the nearest contract boundary, state what remains unobserved, and do not claim end-to-end coverage.

For duplicate or persistent UI reports, do not infer duplicated data from duplicated pixels. Measure these scopes separately before choosing a cause:

1. Count source records or persisted entries by stable identity.
2. Count repeated nodes inside one component or Shadow DOM root.
3. Count document-level root containers, mounted app instances, listeners, workers, tabs, or processes and record their owner or generation identity.
4. Determine which lifecycle transition created each instance: initial mount, same-context render, reinjection, extension update, reload, SPA navigation, reconnect, or failed cleanup.
5. Capture the actual runtime DOM or process topology. A screenshot establishes the visible symptom but cannot distinguish repeated children from overlapping independent roots.
6. Reproduce with stale state left by the previous instance, start the new instance through the production boot path, and assert one authoritative owner, one root, one listener/effect set, and one visible result.
7. Exercise cleanup and remount independently. A render-only test is insufficient when the complaint survives refresh, reload, navigation, or restart.

## Domain Identity and Duplicate-Name Collisions

Treat distinct domain entities that share a display name as an identity collision, not as a duplicate side effect or duplicated UI. Display-name equality never proves identity equality.

Before mutation, enumerate every candidate using its stable ID, raw display name, contract-authorized normalized name if any, current scope or container, deleted or stale status, and role in the operation. State the identity key separately from the uniqueness key; for example, identity may be `item ID` while uniqueness is `(project ID, target folder ID, target name)`. Do not select a winner by query order, display order, first match, last match, or tuple position.

Use this resolution procedure:

1. Inventory the intended current entity, incoming candidate, every same-name collision in the target scope, same-name entities outside that scope, and deleted or stale candidates. Record missing or nullable entities explicitly instead of inserting them into an ID comparison.
2. Build a collision table before changing state:

   | Role | Stable ID | Current scope | Raw name | Status | Expected final scope |
   | --- | --- | --- | --- | --- | --- |
   | Current, incoming, collision, or unrelated | Contract ID or null | Folder/container ID | Exact value | Active, deleted, or stale | Exact folder/container ID |

3. Derive deterministic winner and loser precedence from the product or user contract. If precedence is absent or ambiguous, stop before mutation and request it; do not invent a first-match policy.
4. Snapshot every entity that can change. Apply displacement, rename, promotion, and persistence as one logical transition; on any partial failure, restore every touched entity and verify the rollback by stable ID.
5. Assert the authoritative result by stable ID and scope: winner location and name, every loser location and name, save or move cardinality, unchanged out-of-scope entities, and zero mutation of deleted or stale candidates. A name-only search is insufficient runtime evidence.
6. In the delivered runtime, read the complete collision set before and after the initiating action from the authoritative consumer. Correlate each pre-state ID to exactly one final state and fail on missing, extra, or multiply mutated IDs.

Cover at least these applicable matrix rows: no collision; one collision; multiple collisions; the same stable ID returned through aliases; nullable current entity plus collision; explicit current entity plus collision; same name outside the target scope; and deleted or stale collision. Add one-axis near neighbors for any compound failure. For repeated incidents, reproduce the actual runtime collision set and report the missing identity, cardinality, scope, precedence, or rollback axis that let the earlier test pass; do not summarize it only as an edge case.

Use a short observation table before writing the test:

| Boundary | Unit | Expected count | Evidence |
| --- | --- | ---: | --- |
| Producer | User or upstream action | Contract-defined | Input/event trace |
| Propagation | Delivered event/request | Derived, not assumed | Complete ledger |
| Consumer | Authoritative mutation callback | Contract-defined | Consumer spy/state |
| Result | Visible or persisted outcome | Contract-defined | Read-back/DOM/state |

An upstream invocation is valid evidence only when the production contract proves a one-to-one mapping from that invocation to the authoritative consumer effect.

## Value, Template, and Path Boundaries

For work involving typed IDs or references, templates, percent encoding, Unicode, or Linux, Windows, and WSL paths:

1. Run the bundled checker self-test before changing code:

   ```bash
   python3 /home/tree/ai/skills/test_manager/scripts/check_correct_path.py self-test
   ```

2. Capture the producer's actual runtime value and type. Mask tokens and personal paths in logs.
3. State the consumer's semantic contract separately. Do not infer a typed ID from display text, coerce a numeric string without an explicit contract, or take the last element of an error tuple as an ID.
4. Trace each applicable stage independently: selection, serialization, template rendering, URL decoding, OS path mapping, and filesystem access.
5. Inspect output produced by the project's real template renderer. Do not replace it with a test-only renderer that merely produces the expected result.
6. Preserve opaque strings unchanged unless a documented contract says otherwise.
7. Determine whether percent encoding is present before decoding. Decode at most once, preserve `+`, reject malformed escapes, and report escapes that remain after one decode as possible double encoding.
8. Search sibling fields that share the same parser, coercion, renderer, decoder, or path mapper and add regression coverage.
9. Run both unit tests and a boundary integration test against the real consumer. For a file path, verify that the mapped file exists, is readable, and returns nonzero bytes when content is expected.

The bundled checker supports:

```bash
python3 /home/tree/ai/skills/test_manager/scripts/check_correct_path.py value ...
python3 /home/tree/ai/skills/test_manager/scripts/check_correct_path.py template ...
python3 /home/tree/ai/skills/test_manager/scripts/check_correct_path.py path ...
python3 /home/tree/ai/skills/test_manager/scripts/check_correct_path.py pipeline ...
```

Use `value` for exact JSON types, `template` for project-rendered source/output pairs, `path` for decoding and filesystem evidence, and `pipeline` to identify whether template rendering or path consumption is the first failing boundary.

## Mandatory Retry Boundary

Classify failures by when they occur:

- **Before the effect:** bounded retry may be valid, such as an editor not existing yet.
- **After the effect starts:** do not repeat the full effect unless the operation has a documented idempotency contract.
- **After the effect succeeds but the response is lost:** recover by request ID, persisted state, read-back, or consumer evidence; do not blindly repeat.
- **Partial multi-target success:** retry only unfinished targets unless the input version changed and the contract explicitly resets every target.

Never use one generic retry loop for all failure stages.

For every retrying workflow, tests must assert:

- exact effect call count;
- call order;
- maximum attempts or timeout;
- state persisted after each target or stage;
- terminal errors stop immediately;
- retryable pre-effect errors retry;
- response loss after an effect does not duplicate it;
- a new request can retry after the previous request has terminated.

Mocks must expose the same stage information as production. Do not make a mock return a generic failure when production distinguishes `retryable`, `submitted`, `verified`, committed, or persisted states. Do not preserve an observed bug merely because an old test expected it.

## Semantic Effect Cardinality

Define effect counts at the user-visible operation boundary before accepting an implementation:

1. Name the logical effect and its scope key, such as `(request ID, target, input version)`.
2. State the allowed consumer-call count and payload shape. If the contract says paste, write, upload, or submit once, require one consumer-facing call containing the complete payload.
3. Count every consumer-facing chunk, batch fragment, fallback, and resumed call as a separate effect. Splitting one logical operation into distinct calls does not make those calls one effect.
4. Permit streaming or multipart delivery only when the consumer contract explicitly supports it. Assert stable fragment identity or offset, complete coverage, no gaps or overlap, no duplicate fragment after retry, and idempotent recovery.
5. Record an effect ledger containing order, effect type, scope key, target, and payload length or digest. Assert the entire ledger, not only a reassembled payload or one selected call.
6. Derive mock consumer state and verification results from that ledger. Do not return success independently of the effects the mock received.
7. Apply the same cardinality assertions below, at, and above every size, chunking, batching, pagination, or fallback threshold.
8. Test the full orchestrator scope as well as the helper scope. Include repeated top-level messages, response loss, process or service-worker restart, and continuation to the next target when applicable.

When a multi-call protocol is not explicitly documented, preserve the original value in one call rather than inventing chunking as a workaround.

## Repeated Regression Escalation

Before fixing a reported regression, search the current conversation, requirement log, issue history, and relevant tests for the same user-visible failure.

- On the second report of the same failure, classify it as a missed regression rather than an ordinary new defect.
- Stop further functional patches until actual runtime logs or traces identify the first failing boundary. Static inspection remains a hypothesis, not a confirmed cause.
- Explain why the previous tests passed and identify the assertion, mock, fixture, layer, or input boundary that failed to represent the complaint.
- Add a regression test that fails on the buggy baseline before changing the implementation. If the baseline is unavailable, prove the test rejects an isolated reversible mutant that recreates the duplicate or missing effect.
- Exercise the exact user-visible path and realistic boundary input. Do not accept a helper-only reproduction when orchestration, persistence, browser state, or another consumer layer participates.
- Change the deficient test contract together with the product fix. A workaround that leaves the previous false-positive test intact is incomplete.
- Map every repeated complaint to a named regression test and do not claim completion until the old failure is red and the corrected behavior is green.
- Reject a test fake that flips readiness, success, listener presence, submission, or consumer state because a filename or helper call appeared. Execute the production loader and derive state from its actual effects and exceptions.

## Required Test Matrix

Cover the applicable rows:

1. Happy path reaches the real consumer.
2. Invalid type or malformed payload fails before effects.
3. Consumer not ready retries within a bound.
4. First effect succeeds and the next stage fails; the first effect occurs once.
5. Response or message channel disappears after the effect; no duplicate effect occurs.
6. Timeout leaves an explicit retryable or terminal state.
7. Duplicate request ID is idempotent.
8. Partial target success retries only intended targets.
9. Input or content version changes reset or reject state according to the documented contract.
10. Process, tab, page, or service-worker restart preserves the required state.
11. Valid typed representation is accepted and numeric display text or structured error values are rejected.
12. Fully rendered templates pass while unchanged and partially rendered templates fail.
13. Encoded and unencoded Unicode paths work; malformed and double encoding are rejected or reported.
14. Linux, Windows, and WSL mappings preserve Unicode and spaces and reach the actual readable file.
15. Payloads below, at, and above a size-dependent branch preserve the same semantic effect cardinality.
16. Repeated top-level delivery, consumer restart, and response loss do not multiply one logical effect.
17. A duplicate-effect mutant or known buggy baseline makes the regression test fail.
18. One producer action is traced through propagation to the authoritative consumer count and final state.
19. Fan-out, fan-in, bubbling, delegated listeners, subscriptions, fallbacks, and deduplication preserve the documented observation-unit mapping.
20. Duplicate or persistent UI tests distinguish stored records, children within one root, document-level roots, and process or listener instances.
21. Initial mount, same-context rerender, stale-root reinjection, reload or update, SPA navigation, and cleanup leave the documented owner and instance count.
22. The initiating action uses the production input boundary; a mutant that replaces it with property assignment, `click()`, `dispatchEvent`, or direct handler invocation is rejected.
23. Editable embedded UI preserves hit testing, focus, keyboard input, event propagation, commit, and persistence under hostile host capture listeners and Shadow DOM retargeting.
24. A changed guard preserves every previously accepted state and downstream branch unless a linked requirement explicitly authorizes removal.
25. Foreground/background, active/inactive, visible/hidden, open/closed, reload, and restart conditions that affect reachability have separate preservation scenarios and matching runtime receipts.
26. Every contract explicitly declares `input_validation.applicable` and a reason; applicable surfaces are linked to acceptance scenarios.
27. Every applicable input surface proves first input, continuous input, event order, controlled value, root persistence, and no uncaught error through the production input channel.
28. Text-like inputs with IME support prove composition without duplicate or lost commits, and a delayed event-target read mutant fails.
29. Commit and cancel routes read the authoritative consumer independently; required persistence is read back after reload or restart.
30. Input evidence stores only length and SHA-256 for entered values and cannot expose raw user text.

If a row is relevant but cannot be tested, state the gap before claiming completion.

## Boundary and Integration Evidence

- Verify observable consumer state, not only a helper return value.
- For DOM input, read the stabilized editor value and separately verify submission evidence.
- For files, verify existence, readability, and nonzero bytes when content is expected.
- For storage, read the persisted record after the write.
- For external calls, assert the exact payload and the resulting consumer state when safely possible.
- Keep irreversible or live-system tests isolated and require authorization when they would mutate user data.

## Completion Check

Before reporting success, answer:

- Which test would fail if the effect were repeated?
- What is the authoritative observation unit for the reported behavior, and why is it closer to the complaint than the helper invocation?
- For duplicated or persistent visuals, what are the separate data-record, component-child, document-root, listener/process-instance, and visible-result counts?
- Which owner or generation created each root, and which reinjection, reload, restart, navigation, or cleanup test proves stale instances cannot overlap the new one?
- Does the test record producer, propagation, consumer, and result counts separately?
- Which fan-out, fan-in, bubbling, listener, retry, fallback, or deduplication path could change those counts?
- Which assertion fails if one logical effect is split into multiple consumer-facing calls?
- Which test distinguishes pre-effect retry from post-effect terminal failure?
- Which assertion proves the real consumer observed the value?
- Does the mock derive consumer state from the complete effect ledger instead of returning independent success?
- Do size, chunk, batch, fallback, and restart paths preserve the same semantic cardinality?
- Was the requirement history checked for a repeated report, and does a named regression test cover it?
- Was the regression test shown red on the buggy baseline or an isolated equivalent mutant before it passed on the fix?
- Did any mock encode the implementation instead of the contract?
- Does the behavior change analysis partition every previous state and downstream branch into preserved or explicitly authorized removal sets?
- Which production-path mutant proves a tightened guard or earlier return cannot make a required fallback unreachable?
- Do lifecycle observations match the declared foreground/background, active/inactive, visible/hidden, open/closed, reload, and restart conditions?
- Were the project-required full checks executed?
- Did `verify_runtime_evidence.py` accept a record produced by the delivered artifact in the real runtime?
- Did the runtime test drive the production input boundary without property assignment, synthetic event dispatch, direct `click()`, handler invocation, or state injection?
- For editable UI, did it verify hit testing, Shadow DOM-aware focus, keyboard input, event/default-prevention traces, commit, reload persistence, and hostile host-page interference?
- Does the contract explicitly declare whether input validation applies, and does every applicable surface have a linked scenario and complete runtime receipt?
- Did first-character, continuous, IME, rerender/root survival, commit, cancel, and persistence checks run where their surface flags require them?
- Does a released-event-target mutant fail, and are entered values represented only by length and SHA-256 in evidence?
- Was the bundled value/path self-test run when those boundaries apply?
- Did the project's real renderer and the mapped file reach the real consumer?

If any applicable answer is missing, testing is incomplete.
