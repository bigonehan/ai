# Missed regressions: evidence and prevention

## 2026-09-10 background-generation incident

The user reported premature ChatGPT window closure and repeated generation costs after earlier tests passed. The approved repair is to this skill and its offline validators. Product code and actual AI requests are outside scope.

Read-only evidence: `/tmp/grist-video-maker-background-generation.jsonl`, previous test-writing history, and the corresponding product test source. Log times below are UTC. Raw prompts are deliberately omitted.

- Batch `dd2be0da-2441-4c00-a44b-a46a72f2ad02`: slots 1–5 opened at 10:22:20–21; slot 3 returned at 10:23:09. Slot 4 opened again at 10:27:41 and slots 1, 2, 5 at 10:27:50–51. Slots 2 and 5 reported a missing prompt editor at 10:28:06–07; slot 4 opened again at 10:32:55 and slot 1 at 10:33:28.
- Batch `acb558e8-77bb-47b7-b440-086c668a19c1`: slots 1–4 opened at 12:16:31–32; slots 2 and 3 reported `wait is not defined` at 12:16:46. Slot 4 reopened at 12:21:55 and slot 1 at 12:22:31. Slot 1 reported the same missing global at 12:22:36; slot 4 returned at 12:22:43.

These logs establish repeated opening and error timing. Attribution to the alarm/cleanup/requeue path also uses production control-flow inspection; the log alone does not prove every tab-close call or the amount billed.

| Old test/report gap | Why it could pass | Required replacement evidence |
| --- | --- | --- |
| Injected `wait: async () => {}` | Repaired a missing production global inside the test | Actual loader/producer provenance; missing binding red; no test-only repair |
| Replaced `sendBackgroundSlot`, persistence and `scheduleBackgroundExpiryAlarm`; discarded `chrome.tabs.remove` calls | Skipped delayed cleanup, identity reset and resubmission | Real shared control flow with virtual clock, stateful transport/storage adapters and an effect ledger through every relevant deadline |
| Used `status: i === 0 ? 'failed' : 'completed'` as finished wave | Encoded failure as completion despite the all-complete requirement | Separate success/failure/rate-limit/timeout cases and consumer-derived completion state |
| Reported unit, status-read, build or copy as success | Did not establish installed runtime version or final image receipt | Separate evidence levels, delivered/loaded hashes, final consumer observation or explicit `runtime-unverified` |

Existing skill rules already forbade much of this, but the workflow did not apply them. The skill was absent from the session catalog/discovery directory. A single discovery symlink now points to the canonical skill; this makes it discoverable on a future skill-catalog refresh and does not prove it was loaded in an already-running session.

## Optional v6 effect-safety contract

Existing v6 contracts without effect safety remain valid. For a **new or revised scenario involving timers, retry, queues, tab lifecycle or costly effects**, the author must set `requires_effect_safety: true`; the independent reviewer checks applicability rather than trusting automatic keyword inference. Existing receipt signatures and schema version remain unchanged.

## 2026-09-13 stage-by-stage Settings Test incident

The earlier skill edit mistranslated “minimize user execution” into “bind the Test button to only the current failed micro-stage.” That let receiver-only and model-only checks pass while the later production path remained unobserved, and it forced the user to act as a manual debugger for each newly exposed boundary.

The replacement contract separates two scopes: automated regression tests may isolate a failing stage, while the one user-run Test retains the complete authorized producer-to-consumer workflow. One failed target does not suppress independent targets, and the final report contains every target's first failing boundary. A contract that permits more than one user run, micro-stage reassignment, or an intermediate observation boundary must fail validation.

Each scenario declares `effect_safety`:

- `requirement_id`: an ID in the scenario's `requirement_ids`.
- `deadlines_ms`: nonempty nonnegative integer delays covering the relevant production timers. `observation_ms` exceeds their maximum. Freeze these from code and requirement evidence, not from a conveniently short test timeout.
- `required_triggers`: unique lifecycle names; use separate narrow scenarios when different states need different triggers. Every logical request must execute every declared trigger, and observation must continue longer than the maximum declared delay after each trigger.
- `limits`: map each monitored effect to integer `{min, max}` counts per stable logical request. For a no-replay pending-image scenario, count the initial submit once and explicitly require zero close, automatic resend, identity replacement and next-wave actions. Declare all relevant effects, even those expected never to occur.
- `bindings`: explicit list of consumed production dependencies (may be empty for scenarios without relevant bindings).
- Optional `completion_gate`: `{effect, required_state}`. The recorder reads the actual production state before the counted effect; the reviewer must verify its derivation, including all members of a wave.

An observation references `effect_safety_capture: {path, sha256}`. Store the JSON under the owning project's `test/tmp/`. It contains matching `scenario_id`, `run_id`, `phase`; `evidence_kind` (`unit_capture` for unit, `runtime_capture` otherwise); `observed_ms`; nonempty unique `logical_request_ids`; `monitored_effects` exactly the sorted contract effect names; ordered `events: [{at_ms, logical_request_id, effect, state_before?}]`; `triggers: [{name, at_ms, logical_request_id}]`; `bindings`; and an explicit `uncaught_errors` list.

Each binding records `name`, `origin: production`, `injected: false`, and fingerprinted `producer` and `loader` files. They must match the frozen scenario `input_lineage.artifact_order` and `input_lineage.loader_sources`. An actually unresolved binding is represented as `resolution: missing, producer: null` with its real loader. This is a behavioral failure, admissible in a failing pre-fix capture but rejected in a successful post-fix/unit capture. Do not invent a producer to satisfy the gate.

Pre-fix failure evidence must satisfy structural/correlation rules and demonstrate at least one behavioral violation; its count/error violations are the expected red evidence. They cannot become successful post-fix evidence. Runtime captures additionally pass the existing original-input, loader, consumer, version and independent-review gates. A changed evidence label alone never makes an offline run real.

## Recorder review and regression suite

The validator checks evidence consistency, not recorder honesty. Inspect the recorder's hooks for complete effect coverage, stable logical identity, state derivation, real dependency loading, clock advancement and actual trigger execution. Inspect the old test and equivalent mutant: a mocked scheduler that never invokes production callbacks, hardcoded success state, fabricated zero ledger or altered expected result must fail review even if its JSON passes schema checks.

Run `python3 /home/tree/ai/skills/test_manager/scripts/run_self_tests.py` after skill changes. It registers existing path, runtime-receipt and independent-review self-tests plus effect-safety regressions. The latter executes an isolated missing-`wait` Node example and exercises synthetic positive/negative receipt fixtures for delayed effects, false completion, lost response/restart duplication, shortened observation, substituted dependencies and incorrect evidence levels. These are validator unit tests, not product baseline replay or real browser verification.

The reported product remains `runtime-unverified` until separately authorized, correctly scoped runtime evidence exists. Do not spend generation tokens merely to turn this skill-only maintenance task green.
