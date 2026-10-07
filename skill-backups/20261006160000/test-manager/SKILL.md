---
name: test-manager
description: Plan, write, run, and review unit, integration, regression, and end-to-end tests with restricted app-command APIs, action-bound transition assertions, pre-merge stability runs, explicit observation-unit selection, producer-consumer contracts, external API capture and fixture lineage, domain identity and duplicate-name collision resolution, semantic side-effect cardinality, propagation tracing, state transitions, retry boundaries, repeated-regression escalation, typed-value validation, real template rendering, URL decoding, Unicode, cross-OS path mapping, and actual file access. Use whenever Codex changes or runs tests, investigates a missed or repeatedly reported regression, claims behavior is verified, or debugs values or files that are present but cannot be consumed.
---

# Summary

Follow this order and keep each check distinct:

1. Read applicable rules, requirement history, project test commands, and existing tests. For a missed regression, explain exactly why the old test passed before designing its replacement.
2. Build a requirement-to-evidence ledger from the current request and `Input.md`, then freeze scope and one acceptance scenario for every target, site, surface, and lifecycle condition.
3. Split the visible artifact into data records, nodes inside one component root, document-level root or process instances, and pixels; measure each scope before selecting the authoritative observation unit.
4. Map producer, propagation, consumer, result, ownership, reinjection, reload, restart, and cleanup boundaries, then identify irreversible effects, semantic effect cardinality, fan-out, fan-in, retries, fallbacks, and persistence stages. For external or multistage work, design boundary-level diagnostics before implementation.
5. Read actual runtime logs or traces, identify the first failing boundary, reproduce the defect on the buggy baseline or an isolated equivalent mutant, and confirm the regression test is red.
6. Build stateful production-shaped fixtures and the applicable input, value, boundary, failure, restart, and size test matrix. Every contract declares whether user-editable input validation and external API consumer validation apply. App UI tests expose only the required command vocabulary, bind every action to a false-before/true-after transition assertion, and declare their pre-merge stability run.
7. Run narrow unit tests and verify exact effect ledgers, payloads, order, consumer state, and final outcomes.
8. Run the built artifact in its real runtime and observe the authoritative consumer; unit, mock, jsdom, source-string, and helper-return checks cannot replace this step.
   Drive the initiating action through the production input boundary. Direct property assignment, synthetic event dispatch, handler invocation, or state injection is not user-action evidence.
   Before any user handoff, exhaust every deterministic lifecycle and orchestration boundary that the agent can reproduce without the user's profile or credentials. Treat missing or failed evidence as an improvement queue, not as a reason to stop work.
9. Run the project-required full check and any applicable renderer, path, file-access, build, copy, or deployment verification.
10. Validate a runtime evidence record with `scripts/verify_runtime_evidence.py`; report completion only when the gate accepts it.
11. When delegation is available, require an independent verifier agent to review the requirement, production dependency topology, mutant-red proof, registered suite, and delivered artifact; validate its receipt with `scripts/verify_independent_review.py`.

## Missed-regression gate (read before testing)

For repeated or costly failures, read [references/false-positive-regressions.md](references/false-positive-regressions.md). Record the reported failure, log evidence, the old test’s substitution or omitted boundary, and the assertion that now rejects it. Prove red on the buggy baseline or an isolated equivalent mutant before accepting green. An unchanged passing test is not evidence of a fix.

### Receiver version mismatch missed by Bridge tests

- **요구조건:** When a Bridge is specified to deliver a particular wire schema, determine delivery compatibility from that schema and the receiver capabilities the flow actually needs. Treat build/runtime versions as deployment identity, not a delivery gate, unless the contract explicitly requires a version match.
- **통과조건:** With the required receiver capabilities present, a `PING=response, expected=0.3.14, actual=0.3.15, missing=none` case proceeds through real prompt submission and the authoritative final consumer; an unsupported schema or missing capability fails. Verify the loaded worker, all requested sites, and the complete Settings Test result in one correlated run. Prove that an exact-version-gate mutant fails the regression.
- **금지사항:** Do not use only a same-version PING fixture, combine a version mismatch and missing capability in the sole negative case, assert source text that enforces version equality as proof of correct behavior, or declare PING/model selection a full delivery success. Do not dismiss a loaded-worker mismatch without runtime evidence or ask the user to rerun Test for each site or stage.
- **실제 결과(잘못한 것):** Earlier tests supplied matching versions, confounded the negative case with a missing dependency, and even asserted exact version equality. Review then dropped evidence of a stale loaded worker. The reported production run failed at receiver readiness on a version-only mismatch before prompt submission, so those passing tests did not establish Bridge delivery.

Never inject a production global or import a missing producer solely to make a test run. Follow the actual loader and record unresolved bindings. For timer, retry, queue, tab-lifecycle or paid-send scenarios, declare `requires_effect_safety: true` and the optional v6 effect contract described in that reference. Advance a virtual clock beyond every relevant deadline **after each trigger**, including alarm, response loss and worker restart. Count effects per stable logical request across transport-ID changes, including explicit zero counts for prohibited close, resend, reset and next-batch actions. Verify state from production transitions: failure, timeout and rate limit are not successful completion.

Treat user-run runtime execution as a scarce final observation, not an interactive debugger. Before asking the user to act, exhaust logs, static tracing, unit/integration coverage, mutants, and available browser automation. When a user action is still required, one invocation of the existing Test entrypoint must drive the complete authorized production workflow to its authoritative consumer, collect every reachable stage and independent target outcome, and return one correlated report. Do not rebind the button to one micro-stage and ask the user to repeat it for the next stage. A dependent stage may stop after its prerequisite fails, but other independent sites/items continue. Irreversible effects remain limited by the user's authorization and the workflow's idempotency contract.

## Verification Failure Improvement Continuity

A failed test, missing receipt, validator rejection, or incomplete runtime trace is a diagnosis and improvement input. It is not, by itself, a terminal condition for the implementation turn. Convert every failure into a boundary work item and keep working while any safe agent-executable item remains.

### First-command no-stop contract

Every user request that names an action, effect, or completion condition creates one active completion contract from that first action to its authoritative consumer. The agent must not send any terminal conclusion (`complete`, `incomplete`, `failed`, `blocked`, or `runtime-unverified`) while that contract has an unexecuted agent-owned step, an unattempted repair path, or an unread authoritative consumer. A validator rejection, absent receipt, stale runtime, user statement that an action was performed, or an initial runtime failure is an intermediate work item only.

The continuation order is mandatory: identify the first failed boundary; repair the owning code, configuration, loader, or artifact; rebuild and redeploy; reload or reconnect the same runtime; replay the same production start action; and read the authoritative consumer. Repeat this order for every safe agent-owned failure. For a file/download contract, the terminal predicate includes the new file's existence, nonzero size, SHA-256, and actual byte readback. If only a user-exclusive action remains, use the input gate for one continuation request; do not convert that request into a completion or failure report.

### Delegated completion-loop contract

When delegation is available and the user requests continuation until completion, the main session owns one active completion contract and the authoritative consumer decision. A worker's `complete` message is only a candidate result until the main session reads its diff, logs, receipt, artifact hashes, and persisted consumer state.

For every non-success worker result, the main session must create the next bounded work item from the newly identified first failing boundary and delegate it again. A re-delegation is a new cycle, not a blind retry: reproduce the failure, identify its cause, implement a new improvement at the owning boundary, test it, record the worker's success/failure judgment, and compare any success with the original user requirement. The work item must include: owner boundary, concrete repair objective, unchanged adjacent scope, exact production replay, expected evidence, and the completion predicate. The required loop is `delegate → reproduce → identify cause → implement improvement → test → inspect worker judgment → independent review → reload/reconnect → replay → read consumer`.

Do not send a terminal conclusion between loop iterations. `runtime-unverified`, `incomplete`, `failed`, `blocked`, missing evidence, and validator rejection are work-queue states while any worker-owned or main-session-owned repair, test, reload, replay, or readback remains. If worker creation, slot allocation, or lifecycle management fails, execute the same work item in the main session and retry independent delegation later.

The loop terminates only when the authoritative consumer predicate is true, or when the handoff-readiness table proves that exactly one user-exclusive action remains and the input gate is used for that continuation action. For file outputs, require a new file, nonzero size, SHA-256, and actual byte readback; never promote a request, ACK, intermediate receipt, or existing file.

Before a user handoff, classify every unverified boundary:

- **Agent-executable:** source and loader tracing; stateful unit or integration fixtures; production-artifact execution in an available process; timers; `play`, `pause`, `ended`, `error`, `updateend`, `buffered`, and duration transitions; message/port disconnect and reconnect; worker restart; retry and timeout advancement; conversion; filesystem or download-adapter readback; persisted-state inspection; log correlation; and reversible mutants. Implement missing observability and exercise these boundaries without user input.
- **User-exclusive:** a trusted interaction in the user's actual profile, credentials or hardware; a live service decision that cannot be reproduced safely; or an irreversible/costly effect that lacks prior authorization. These are the only boundaries that may remain at handoff.

For a multistage workflow, component tests do not collectively prove the flow merely because each file has a passing test. Add one correlated orchestration scenario that starts at the earliest production producer the agent can execute, loads the delivered production artifacts in their real order, and carries the same operation ID, payload identity, effect ledger, and terminal state through every agent-executable stage to the nearest safe authoritative consumer. Do not start a downstream test by injecting `capture-complete`, success, a prepared result, or another intermediate message when the upstream producer and transport can be connected in the same fixture. A safe adapter may replace only the genuinely unavailable external boundary, and its state must be derived from the complete received effect ledger rather than returning independent success.

Deterministic browser lifecycle signals are mandatory matrix axes, not reasons for a manual run. When behavior depends on media completion, buffer coverage, playback state, timer expiry, document lifecycle, navigation, port loss, or worker restart, exercise both the signal-present and signal-absent states, advance every relevant deadline, and assert the downstream terminal consumer. For example, a capture flow must test full buffer with no `ended`, `ended` with partial or complete buffer, adjacent source generations, timeout, and exactly-one conversion/download effect before asking the user to play media.

Treat validator output as a work queue:

1. Preserve each gate's exit code and parsed errors; do not place a failing gate in a command sequence whose later success can mask it.
2. Separate schema/recorder failures from product failures. Repair missing observation emission, scenario coverage, loader hashes, lineage, or independent-review fields before interpreting the product verdict.
3. Map every error to an owner, boundary, automated reproduction, expected consumer evidence, and resolved/unresolved state.
4. Re-run the narrow failing gate, then the connected orchestration scenario, full suite, built artifact, deployment comparison, and validators.
5. Continue until the unresolved set contains only user-exclusive boundaries. `runtime-unverified` describes that residual boundary; it does not excuse skipped agent-executable work.

Immediately before requesting user input, record a handoff-readiness table containing every production stage, its latest evidence, and its classification. The request is allowed only when the table has zero unresolved agent-executable rows, observability is already active for the full run, the action ledger permits the request, and one user action will reach the authoritative consumer while recording every stage. If any deterministic event, intermediate consumer, failure projection, or final adapter can still be exercised automatically, continue improving and testing instead of handing it to the user.

Keep evidence levels explicit: unit/mock, status-read, build/copy, and actual runtime consumer verification. Costly generation/submission/retry is not an automatic follow-up to an offline test. Respect the authorized scope and cost constraints; missing runtime evidence remains `runtime-unverified`. A skill-only repair can finish after its offline suite and independent review without claiming that the product was fixed or replaying paid actions. Product runtime gates below apply to product runtime completion claims, not to this narrower skill maintenance result.

# Test Manager

Treat passing tests as evidence only when they exercise the behavior and boundary that can fail in production.

## Original Input and Runtime Evidence

Before creating a runtime recorder or reviewing evidence, read [references/input-lineage.md](references/input-lineage.md). Receipt schema **v6** requires each scenario to connect the original user action/value/file to the production loader, handler, transformations and authoritative consumer. Fingerprint the source without copying private raw values into receipts. Capture the actual boundary values and loader order; a middle-stage payload, injected namespace or forced success readback cannot establish runtime completion.

Mocks remain useful for unit tests. Declare their component, stage and reason in `substitutions`, identify `entry_stage`, and keep their results at unit level. Do not reuse synthetic validator fixtures as product evidence. The validator checks consistency; an independent verifier must inspect the recorder and how its captures were obtained, including the production-owned load list.

For a reported failure in an existing Settings Test workflow, keep the user's single initiating action and full producer-to-consumer path as the observation unit. Internal automated tests may isolate the failed item or stage, but the user-facing Test must not be narrowed and reassigned between runs. In a multi-target workflow, preserve completed targets from duplicate effects while continuing every independent unfinished target in the same run and reporting the complete matrix.

## Single User-Run Hard Gate

For every new or revised contract that requires a user-run runtime action, declare `user_execution` with `manual_run_required: true`, `max_user_runs: 1`, the complete set of `scenario_ids`, `production_path_complete: true`, `continues_independent_targets: true`, `micro_stage_reassignment: false`, and `post_run_debug_source` set to `captured_logs_and_automation`. Every linked scenario must use the same initiating user action. The single action must cover the production path through each scenario's authoritative consumer; a receiver-only, selector-only, ACK-only, or helper-only result is invalid when later authorized boundaries exist.

For each new or revised manual-run contract, also declare `user_execution.action_type` (`settings_test`, `extension_reload`, `generation`, or `other`) and an absolute `user_execution.action_ledger_path` under that project's `test/tmp/`. Backfill all known requests and observed user actions from the current incident before a request. Treat a request that the user reload an extension, regenerate an output, capture a screenshot, or export logs as a user action; agent-controlled automation is distinct. Never reset a ledger to regain the one-run budget. Run `user_action_gate.py reserve` **before writing any user-facing request**; a denied or missing gate means no request. Reserve consumes the budget even if the user has not replied. After the run, record its run ID with `observe`. Before claiming real-runtime completion, run `audit --require-observed`; the runtime evidence validator also checks linked ledgers and run IDs. This checks recorded events, so independently review the conversation against the ledger before relying on it; no local script can intercept an unrecorded chat message.

```text
python3 scripts/user_action_gate.py init <project/test/tmp/task/action-ledger.json> <task-id>
python3 scripts/user_action_gate.py record-existing <ledger.json> requested|observed <action-type> <scope> <source-reference> [--run-id <id>]
python3 scripts/user_action_gate.py reserve <contract.json> <ledger.json> <action-type> <full-workflow-scope> <source-reference>
python3 scripts/user_action_gate.py observe <ledger.json> <run-id> <source-reference>
python3 scripts/user_action_gate.py audit <ledger.json> --contract <contract.json> --require-observed --run-id <id>
```

After that run, use the correlated run ID, captured logs, persisted state, and an authenticated automation profile for diagnosis and safe replay. Do not ask the user for another stage/site/feature click. If the one-run boundary cannot be built safely within current authorization, report the missing boundary before requesting any execution.

Keep v5 evidence archived unchanged. Freeze a v6 contract and recapture missing source, loader and boundary observations; changing a version number is not migration. Missing real-runtime access is `runtime-unverified` only after every agent-executable repair, artifact reload, reconnect, replay, and consumer-readback path has been exhausted and only a user-exclusive boundary remains. For changes to this skill’s validators, report schema/unit-test results separately without claiming product runtime verification.

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

## Producer–Namespace–Consumer Binding Hard Gate

Whenever code adds, changes, or consumes a cross-file global, namespace export, injected script API, plugin registry member, or equivalent runtime binding, declare and verify the complete binding path:

1. Read the production-owned injection or load-order source; do not reconstruct an expected order only inside the test.
2. Record `producer artifact → exported owner path → member → expected type → consumer expression → user handler → authoritative consumer` for every changed binding.
3. Load the actual build artifacts in production order and execute the changed user handler through its production listener. Resolve and call the member at the consumer site, then verify the final consumer and an empty uncaught-error ledger.
4. A CommonJS import, destructured helper call, source `.includes()` assertion, or producer-only export test cannot prove this cross-file binding.
5. Create an isolated owner/member typo mutant and require the same boundary scenario to reject it before the product fix can pass.
6. Search every consumer for unregistered owner paths. An optional chain does not excuse an owner typo when the feature requires that binding.
7. Register the regression in the project's standard check command. A standalone script is not coverage unless the standard suite invokes it or the contract explicitly classifies it as user-run runtime verification.
8. Assert the fixture's selectors, storage keys, and schema fields still exist in production. Fail stale fixtures before interpreting their result.

For browser extensions, the authoritative topology is the script list used by the real popup, service worker, manifest, or injection handler. The boundary observation must include every loaded artifact hash, the delegated pointer/keyboard path, the resolved owner/member type, final DOM or persisted consumer, and `error` plus `unhandledrejection` records.

## Independent Verifier Agent Hard Gate

When collaboration/delegation is available, the implementation author cannot be the sole test reviewer.

1. Give a different agent the exact requirement, current diff or artifact hashes, standard test command, and relevant production loader. Do not give it the intended verdict.
2. The verifier independently identifies the observation unit and missed boundary, checks suite registration and stale fixtures, inspects or runs the owner/member typo mutant, and checks negative scope.
3. Record a JSON receipt containing distinct `author_id` and `verifier_id`, scenario IDs, reviewed artifact SHA-256 values, commands, `mutant_rejected`, `standard_suite_registered`, `stale_fixture_check`, `negative_scope_checked`, findings, and dispositions.
4. Declare `independent_verification` in the contract with `author_id`, exact `scenario_ids`, `standard_suite_command`, and exact `reviewed_artifacts`, then run `python3 scripts/verify_independent_review.py validate <contract.json> <receipt.json>`. A missing receipt, identical author/verifier identity, uncovered scenario, stale fixture, unregistered regression, surviving mutant, unresolved finding, failed command, or mismatched artifact hash blocks completion.
5. After any product or test change made in response to findings, generate a new receipt for the new artifact hashes; do not reuse the earlier review.
6. If delegation is unavailable, the main session performs the independent review checks itself and records the missing verifier as a review limitation. Do not stop an active completion contract or emit a terminal runtime result solely because a worker cannot be created; use `runtime-unverified` only after all agent-executable repair and consumer-readback paths are exhausted.

## Design-Time Diagnostic Gate for External and Multistage Work

- **요구조건:** Before implementation, map the ordered producer-to-consumer stages and every external call, retry, persistence, and irreversible-effect boundary. Specify a stable logical-operation ID across stages and attempts; record the stage and owning boundary, attempt, outcome, and safe expected/observed values needed to distinguish each failure. Distinguish not submitted, submission outcome unknown, and submitted but not verified.
- **통과조건:** Exercise the production logging path with the same error text arising at two different boundaries and prove the records identify the first failing stage and owner for each operation. Prove that identical repeated failures each produce a record, later states do not inherit stale error details, and pre-effect failure, lost submission response, post-effect readback failure, and final consumer success remain distinguishable. For a comparison or selection failure, record candidate counts and the decisive criteria or values without raw private input.
- **금지사항:** Do not rely on message-only logs, suppress records because the message repeats, flatten nested errors into a generic category, or infer a root cause from an unowned `ECONNRESET` or similar error. Do not log raw prompts, credentials, file contents, or sensitive payloads; use IDs, digests, or redacted metadata for sensitive paths. Do not treat an ACK, helper return, or intermediate status as final consumer proof.

## Structured Result Success Gate

For every workflow that returns a structured result, explicitly separate transport completion, Promise settlement, producer terminal state, authoritative consumer success, and the UI outcome. A resolved Promise or a syntactically valid response is not success.

Declare `result_validation` in the runtime contract:

- `applicable`, with a non-empty reason. When false, keep the remaining result-validation fields absent.
- `success_predicate` with the exact production expression, tokens for every required producer and authoritative-consumer comparison, every required field path, the hashed production consumer artifact, and a hashed mutation receipt proving that the same production consumer rejects a Promise-resolution-only predicate. The contract fixes the executable prefix and exact argv index of the consumer artifact; merely mentioning its path in another command is invalid. The receipt must bind the current consumer SHA-256 and that exact execution argv, so changing the consumer invalidates the receipt. The predicate must require consumer success, not only a transport or task terminal flag.
- `terminal_state_matrix` with exactly the semantic cases `success`, `failed`, `partial`, `delivery_failed`, and `contradictory_terminal`. Preserve a distinct production `status_value` for each case. The success case alone may set `consumer_success: true` and `ui_outcome: success`; every other case must remain `consumer_success: false` and `ui_outcome: error` even when `promise_state` is `resolved`.
- `failure_projection` naming the producer boundary, consumer boundary, error field paths, and requiring both terminal failures and error detail to reach the consumer.
- `ui_success_gate` naming the UI boundary and requiring authoritative consumer success while explicitly rejecting Promise resolution as a success signal.

Every unit and runtime observation for an applicable scenario must include `result_validation` with the matched `case_kind`, whether the predicate evaluated true, authoritative consumer success, projected outcome, and UI outcome. Non-success observations must also name exactly the projected error field paths and provide a digest of the projected error detail without storing raw sensitive text. A receipt marked `outcome: success` is invalid unless all success signals agree. A failure receipt is invalid if it projects any failed, partial, delivery-failed, or contradictory terminal result to a success UI or cannot prove which error detail reached the consumer.

Tests must execute the production result consumer and prove that a mutant which replaces the success predicate with “Promise resolved” is rejected. A happy-path-only fixture or an assertion on the producer result alone is incomplete.
- **실제 결과(잘못한 것):** The Resolve bridge kept only changed status messages and dropped the stage, boundary owner, and expected-versus-observed readback. Its `ECONNRESET`, unmatched-Shot, and post-append failures therefore could not be diagnosed from the historical log even though the workflow had many passing tests.

## External API Consumer Contract Hard Gate

Read [references/external-api-consumer-contract.md](references/external-api-consumer-contract.md) whenever a test target sends to or consumes a response from an external HTTP API, local host API, bridge endpoint, SDK-backed remote service, or API reached through a different OS boundary.

Every new or revised runtime contract must declare `external_api_validation.applicable`, a reason, and `contracts`. Every acceptance scenario must declare `external_api_contract_ids`, including an empty array when it uses no external API. For each applicable API, bind the exact system, operation, method, sanitized endpoint pattern, authoritative execution environment, response field paths and JSON types, capture-derived fixtures, production consumer artifact, and rejected field-name and field-type mutants. A hand-authored success fixture without a safe capture lineage is not contract evidence.

Use a read-only live probe first when the API provides one. Store only an allowlisted schema and non-sensitive status values; never store credentials, authorization headers, cookies, raw bodies, private payloads, or query secrets. If no safe probe exists, record that fact and use a recorded response or authoritative documentation as a lower evidence layer; this does not replace successful live-response evidence at the actual runtime consumer.

Unit evidence must execute the production consumer against the capture-derived fixture and reject both declared mutants. Successful runtime evidence must come from a non-mocked live response in the declared environment and include authoritative consumer readback. A successful connection, HTTP status, parsed helper result, or fixture-only consumer result cannot close the scenario when the final UI, persistence, file, or downstream consumer remains unobserved.

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
6. For app UI tests, freeze the six-command API, pair each action with its false-before/true-after predicate, and define the stability batch before running it.
7. Run the narrow tests first, then the relevant integration or boundary tests, then the project-required full check.
8. Run the pre-merge stability batch for every new or changed app test and reject the batch on any failure.
9. Report what reached the real consumer and what remains mocked.

## Unit-to-Real-Runtime Verification Ladder

For every user-visible scenario, use one stable scenario ID and preserve the same meaningful input and expected outcome through all levels:

1. **Unit:** exercise the smallest producer, parser, state transition, or consumer contract. Record the exact command, boundary observed, input, output, effect ledger, and limitations.
2. **Boundary integration:** execute the production handler, loader, transport, template, or persistence boundary. Do not replace production code by directly flipping readiness, success, listener, submission, or consumer state in a fake.
3. **Built artifact:** build or package the deliverable and record its SHA-256. Unit source success does not prove the delivered bytes contain the change.
4. **Real runtime:** load those exact artifact bytes in the actual browser, process, CLI, host application, or external consumer; perform the initiating user action and read the authoritative consumer result.
5. **Cross-level comparison:** require the unit and real-runtime receipts to share the scenario ID and user action. Compare payload identity, effect count, order, final outcome, and applicable errors. A runtime result that differs from the unit contract fails the task even when both commands exit zero.

Run unit tests first to obtain fast diagnostic evidence, then integration tests, then the actual-runtime check. Never describe unit, mock, jsdom, static inspection, build, copy, or checksum evidence as actual operation. If the actual runtime is unavailable, report the unit and integration results separately and leave the feature `runtime-unverified`.

## Mock Boundary and No-Mock Final Flow Gate

Use mocks only in the lower two layers, and label those results as `unit/mock` or `boundary integration`:

1. **Function input contract:** Pass valid, malformed, missing, wrong-type, boundary-size, and stale values into one production function. Assert its return, state transition, error projection, and prohibited effects. A mock may replace dependencies outside that function, but it must not independently return the expected final success.
2. **Producer-consumer transfer contract:** Execute the real producer and consumer functions around each function-to-function boundary. A stateful fake may stand in for the unavailable transport only. Assert the exact payload value and type, stable operation identity, order, retry classification, and semantic effect count received by the next production consumer. Derive fake state from received calls instead of flipping a success flag.
3. **Final full flow:** Use the built or deployed artifact in the declared real runtime with `mocked: false`, `substitutions: []`, and `entry_stage: original_input`. Start only through the same rendered user-input boundary as the user and read the authoritative final consumer. For a browser extension download, this means the real browser, live origin, installed extension bytes, real page media lifecycle, actual conversion, `chrome.downloads`, and the resulting file bytes in the declared download directory.

Do not start Stagehand or another real-browser driver until every declared function-input row and every producer-consumer transfer row is green against the same source revision, and the built artifact hash has been recorded. A lower-layer failure blocks the browser stage; fix it and restart the ladder at the first function-input layer rather than continuing from a partly valid browser run.

For browser automation, browser tests, or live-site access, use the `$run-cdp` Skill for the browser runtime boundary. It must invoke the real `run_cdp` Fish function when a dedicated browser is needed, discover the endpoint from `/json/version`, verify the Browser/version/WebSocket fields, and pass the discovered endpoint to the authorized browser driver. At the final website-flow stage, use the same user action and input as the lower stages, attach to the `run_cdp` browser, trace the production path to the authoritative consumer, and read that consumer back before claiming success. Keep attach-only cleanup separate from browser ownership: disconnect the driver without closing the `run_cdp` Chrome, and do not launch a second browser when the endpoint is healthy. A target-list read or CDP transport connection is intermediate evidence; the final result still requires the live page/site and authoritative consumer. Raw CDP commands such as `Browser.getVersion`, `Target.getTargets`, and `Target.getTargetInfo` are bootstrap/target/lifecycle evidence only; when Stagehand is the selected final-flow driver, do not mix raw CDP actions into the user-visible flow. If another Codex session performs the attach check, it must not rerun `run_cdp`, restart Chrome, or mutate the profile, and it must report PID/endpoint survival after disconnect.

For browser workflows, use Stagehand v4 as the final-flow automation surface when the active environment and governing rules authorize browser automation. Connect to or launch the intended browser through Stagehand, preserve the declared authenticated session, and perform access, authentication-state verification, rendered button input, and navigation with Stagehand `observe`, `act`, and `extract`. Do not mix Playwright calls or direct CDP commands into the test-facing flow; Stagehand's internal browser transport does not make a direct CDP script acceptable evidence. Treat profile paths, session context identifiers, cookies, and model/API credentials as secrets and never emit them in receipts. Record hit testing or observed target identity, focus/event propagation where available, loader identity, and final readback. Do not use `element.click()`, `dispatchEvent`, direct listener or handler calls, DOM property assignment, storage injection, prepared messages, fake media lifecycle events, mocked Chrome APIs, a fake codec, or a fake download callback as final-flow evidence.

For a Stagehand browser contract, run `node scripts/verify_stagehand_driver.mjs self-test` from this skill and validate the captured evidence with `node scripts/verify_stagehand_driver.mjs validate <evidence.json>` before the general runtime evidence validator. The driver gate must reject Playwright, direct-CDP, mocked, substituted, or intermediate-entry runtime receipts; prose or a package-script name is not enforcement.

When browser automation is prohibited or the required authenticated profile is inaccessible, prepare the same no-mock recorder and use the single-user-run gate. Do not weaken the final-flow contract or promote the lower layer; keep the aggregate result `runtime-unverified`.

If the no-mock browser flow reveals an unanticipated error, treat it as a missed test-model boundary rather than a one-off runtime exception:

1. Preserve the failing run ID, artifact hash, user action, complete pre-state, first failing owner/boundary, actual value or error projection, and authoritative-consumer result.
2. Identify the omitted general axis, such as loader timing, SPA navigation, trusted-event behavior, media generation, browser API lifecycle, permission, service-worker restart, codec/container shape, output filesystem, retry phase, or consumer readback. Do not encode only the reported URL, selector, title, duration, or content ID unless the product contract makes it semantically special.
3. Add that axis to the reusable contract, matrix, diagnostics, and this skill when it applies to the wider workflow class. Add the exact failing conjunction plus one-axis neighbors and a buggy-baseline or reversible-mutant red proof.
4. Apply the product fix only after the generalized regression is red.
5. Re-run the whole ladder from function-input tests, through every transfer boundary and built artifact, to a fresh no-mock browser flow. Replaying only the failed browser step is not closure evidence.
6. Repeat this loop for each newly exposed first failing boundary. Completion requires one uninterrupted final run from original user input to authoritative consumer with no mocks or substitutions.

Apply the same restart rule when an unanticipated failure first appears in a lower layer. In particular, async propagation waits must use an authoritative completion signal or a wall-clock deadline derived from the production contract. A fixed count of `setImmediate`, microtask, animation-frame, or polling turns is not a timeout contract and may become a false failure under parallel-suite load. After repairing a test-harness timing gap, rerun the complete lower-layer suite in the same parallel mode repeatedly before advancing to the browser stage.

For a user-visible product, keep commands and reporting separated:

- `test:unit` or `test:offline` may exit zero after lower-layer checks.
- `test:runtime` must run the no-mock runtime evidence validator and propagate its nonzero exit.
- The aggregate `test`, `check`, or release gate must include `test:runtime`; it must not exit zero while runtime evidence is absent or rejected.
- Never summarize only the lower-layer count as “all tests passed.” Report `offline N/N passed; runtime gate failed/runtime-unverified` unless the aggregate gate exits zero.

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

## Restricted App Command and Transition-Assertion Hard Gate

Apply this gate whenever test code manipulates a mobile app or equivalent app UI. The test-facing command API is closed to these primitives:

- `deep_link(destination)` — enter the app at a contract-named destination;
- `swipe(direction_or_path)` — perform one gesture through the production input boundary;
- `enter_text(target, value)` — enter text through the native input channel;
- `touch(target)` — perform one rendered-coordinate touch;
- `assert(predicate)` — read an observable predicate from the authoritative UI or consumer;
- `restart_app()` — terminate and start the app through the production lifecycle.

Framework-specific drivers, selectors, waits, polling, screenshots, process controls, and platform calls belong behind the adapter that implements these primitives. Test bodies and reusable user flows must not expose aliases, raw driver calls, direct handlers, state injection, or new convenience commands. Build complex behavior only by composing the six primitives. A project may narrow the vocabulary further, but adding a test-facing primitive requires an explicit product testing requirement and a Test Manager contract update. The standard suite must enumerate command invocations in new or changed app tests and fail on an undeclared command; source naming alone is not enough, so execute the composed flow through the production adapter as boundary evidence.

Every action command other than `assert` must own a paired transition assertion. Define one predicate in domain terms and record both observations:

1. Immediately before the action, `assert(not predicate)` must pass.
2. Execute exactly one action command.
3. Within a bounded observation window, `assert(predicate)` must pass at the authoritative UI or consumer.

The predicate must identify the effect caused by that action, not a screen element or state that was already true. For disappearance, phrase the predicate as the achieved state, such as `target is absent`, so it is false before and true after. For `restart_app`, use a lifecycle transition such as a new process/session generation plus the required restored state. If the pre-action predicate is already true, fail the test as a vacuous assertion; do not continue, silently reset, or accept the post-action truth. If the action intentionally has no observable transition, it cannot serve as an acceptance step under this API until the contract names an observable effect.

Declare `app_command_api` for applicable contracts with the six-command allowlist, adapter boundary, every action step and its predicate, pre-action and post-action observation sources, timeout, and authoritative consumer. Also declare a negative check that rejects an unknown-command mutant and a vacuous-assertion mutant whose predicate is already true before the action.

Declare `stability_validation` with the policy source, precommitted repetition count, built-artifact hash, environment/device identity, scenario IDs, per-iteration receipts, and `failures: 0`. Treat the batch as evidence only when every receipt covers the full scenario through the authoritative consumer.

Before merge, every new or changed app test must pass a declared stability batch against the same built artifact and controlled environment. Set the repetition count before execution; use the project policy when present, otherwise require at least 10 consecutive full-scenario passes. Record artifact hash, environment/device identity, scenario IDs, iteration order, result, duration, and failure boundary for every run. One failure invalidates the batch. Diagnose the cause, change the test or product as required, rebuild when bytes change, and restart the entire consecutive-pass batch; rerunning only the failed iteration or hiding it with retries is not stability evidence. Merge remains blocked while any new or changed test is flaky, quarantined, skipped, or lacks the complete all-pass batch.

Keep temporary contracts, snapshots, receipts and capture JSON/TXT in the project’s `test/tmp/`; retain runtime-owned configuration and fixtures in their owning paths. It must contain `task_id`, `workflow`, `user_action`, `expected_outcome`, `unit_observation`, `unit_test_commands`, `authoritative_consumer`, `requirement_coverage`, `acceptance_scenarios`, `incident_history`, `runtime_log_sources`, `input_fidelity`, `runtime_target`, `repeated_report_count`, `allowed_roots`, `watch_roots`, `forbidden_roots`, `adjacent_workflows`, and `delivered_artifacts`. App UI tests additionally require `app_command_api` and `stability_validation`. Repeated reports also require `runtime_diagnosis` with a runtime artifact, named failing boundary, and concrete `specific_failing_detail`. Shared control-flow changes additionally require `requires_behavior_preservation: true`, `behavior_change_analysis`, preservation scenarios, and matching lifecycle observations.

The contract also requires `external_api_validation` with an explicit applicability decision. Every acceptance scenario requires `external_api_contract_ids`. Applicable API contracts and observations use the schema and lineage rules in [references/external-api-consumer-contract.md](references/external-api-consumer-contract.md).

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

Validation requires every requirement to map to scenarios, every scenario to have the required unit/pre/post receipts, an exact origin union, an unchanged scope contract, no unauthorized changes, fresh run-correlated logs including rotated files, no uncaught errors or unobserved layers, correct extension registration, and hashes proving the runtime loaded every delivered artifact and created every expected output after the action. If any boundary is unavailable, classify it as an intermediate work item and continue all safe repairs, reloads, replays, and readbacks. Only a residual user-exclusive boundary may be reported as `runtime-unverified`; do not claim completion before the authoritative consumer predicate is true.

Run the gate's own negative and positive fixtures after changing this skill:

```bash
python3 /home/tree/ai/skills/test_manager/scripts/run_self_tests.py
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
31. Every scenario declares its external API contract IDs; applicable APIs use a safe capture-derived fixture, execute the production consumer, reject a field-name mutant, and reach the live authoritative consumer in the declared environment.
32. Structured results declare a success predicate and exercise success, failed, partial, delivery-failed, and contradictory terminal cases through failure projection to the UI; a Promise-resolution-as-success mutant is rejected.
33. App UI tests use only `deep_link`, `swipe`, `enter_text`, `touch`, `assert`, and `restart_app` at the test-facing boundary; an undeclared-command mutant is rejected.
34. Every app action predicate is false before its action and true afterward at the authoritative consumer; a predicate that is already true before the action fails as vacuous.
35. Every new or changed app test completes its declared pre-merge consecutive-pass batch with zero failures against one identified built artifact and controlled environment.

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
- Does the contract explicitly declare whether external API validation applies, and does every linked scenario prove capture lineage, production-consumer execution, environment identity, mutant rejection, and live consumer readback?
- Does the contract explicitly declare whether structured-result validation applies, and do non-success terminal results remain failures through the authoritative consumer and UI even when the Promise resolves?
- Do app UI test bodies use only the six allowed commands, with framework-specific operations confined to the adapter and an unknown-command mutant rejected?
- For every app action, which predicate was false before and true after, and which authoritative observation proves that the action caused the transition?
- Did every new or changed app test finish its predeclared consecutive stability batch with zero failures against the recorded artifact and environment?
- Did first-character, continuous, IME, rerender/root survival, commit, cancel, and persistence checks run where their surface flags require them?
- Does a released-event-target mutant fail, and are entered values represented only by length and SHA-256 in evidence?
- Was the bundled value/path self-test run when those boundaries apply?
- Did the project's real renderer and the mapped file reach the real consumer?

If any applicable answer is missing, testing is incomplete.
