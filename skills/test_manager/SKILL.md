---
name: test-manager
description: Plan, write, run, and review unit, integration, regression, and end-to-end tests with explicit observation-unit selection, producer-consumer contracts, semantic side-effect cardinality, propagation tracing, state transitions, retry boundaries, repeated-regression escalation, typed-value validation, real template rendering, URL decoding, Unicode, cross-OS path mapping, and actual file access. Use whenever Codex changes or runs tests, investigates a missed or repeatedly reported regression, claims behavior is verified, or debugs values or files that are present but cannot be consumed.
---

# Summary

Follow this order and keep each check distinct:

1. Read applicable rules, requirement history, project test commands, and existing tests.
2. Write the producer-consumer behavior contract and expected user-visible outcome.
3. Split the visible artifact into data records, nodes inside one component root, document-level root or process instances, and pixels; measure each scope before selecting the authoritative observation unit.
4. Map producer, propagation, consumer, result, ownership, reinjection, reload, restart, and cleanup boundaries, then identify irreversible effects, semantic effect cardinality, fan-out, fan-in, retries, fallbacks, and persistence stages.
5. Reproduce the defect on the buggy baseline or an isolated equivalent mutant and confirm the regression test is red.
6. Build stateful production-shaped fixtures and the applicable value, boundary, failure, restart, and size test matrix.
7. Run narrow unit tests and verify exact effect ledgers, payloads, order, consumer state, and final outcomes.
8. Run boundary or integration tests against the nearest real consumer and disclose every mocked or unobserved layer.
9. Run the project-required full check and any applicable renderer, path, file-access, build, copy, or deployment verification.
10. Complete every item in Completion Check and report success only when the regression is green without an untested applicable gap.

# Test Manager

Treat passing tests as evidence only when they exercise the behavior and boundary that can fail in production.

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
- Explain why the previous tests passed and identify the assertion, mock, fixture, layer, or input boundary that failed to represent the complaint.
- Add a regression test that fails on the buggy baseline before changing the implementation. If the baseline is unavailable, prove the test rejects an isolated reversible mutant that recreates the duplicate or missing effect.
- Exercise the exact user-visible path and realistic boundary input. Do not accept a helper-only reproduction when orchestration, persistence, browser state, or another consumer layer participates.
- Change the deficient test contract together with the product fix. A workaround that leaves the previous false-positive test intact is incomplete.
- Map every repeated complaint to a named regression test and do not claim completion until the old failure is red and the corrected behavior is green.

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
- Were the project-required full checks executed?
- Was the bundled value/path self-test run when those boundaries apply?
- Did the project's real renderer and the mapped file reach the real consumer?

If any applicable answer is missing, testing is incomplete.
