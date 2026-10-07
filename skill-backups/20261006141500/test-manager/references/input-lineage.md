# Original-input evidence (receipt schema v6)

## Capture and review

Start with the user’s reported action and original value/file. Read a file from its real source through the product reader; enter text through the real input control and listener. Do not start from the parsed object, queued message, normalized path, prepared namespace or final UI state. If the original is unavailable, say which boundary remains unverified. An explicitly authorized fixture can establish only its declared fixture scenario, not reproduce an incident whose original input was unavailable.

Record fingerprints at capture time. Use UTF-8 for text and exact bytes for files; declare a deterministic representation for actions and structured values before running. `source_ref` identifies the source without embedding private text or credentials. A hash is not encryption: keep captures local and limit sensitive references. Transform steps must observe their actual inputs/outputs; chain each output hash into the next input hash. Preserve all materially relevant transformations and transports.

Read the production-owned loader (manifest, injection list, HTML entrypoint or CLI). Freeze its actual ordered artifacts, using delivered paths. The recorder observes that loader executing and writes its own loader capture; do not supplement missing modules with test imports or injected globals. Inspect recorder source and actual process/browser captures independently: a hash, receipt signature or self-reported `mocked: false` cannot establish that an arbitrary recorder is truthful.

Define the authoritative consumer readback as a deterministic projection of the requested behavior. For example, observe DeepSeek’s actual fast-mode selection plus enabled thinking/search controls, rather than a helper returning success or an expert-mode flag. For nondeterministic output, fingerprint a stable assertion-relevant projection, and review that projection for omitted requirements. Never copy expected values into the recorder as observed values.

## Frozen scenario fields

Each `acceptance_scenarios` item adds `input_lineage`:

- `original_input`: exactly `source_kind`, `source_ref`, `value_type`, `byte_length`, `sha256`. Source kinds: `user_action`, `user_value`, `user_file`, `authorized_fixture`. The last also requires `fixture_authorization` in the lineage specification.
- `production_path`: ordered objects with unique `id`, `role`, and actual `interface`. Start at `input`, include `handler`, finish at `consumer`; use `loader` and `transform` for the corresponding boundaries.
- `loader_sources`: nonempty absolute `{path, sha256}` references to production-owned load definitions.
- `artifact_order`: nonempty ordered `{path, sha256}` references extracted from those definitions. Each scenario must prove its required modules; another scenario’s loaded-file evidence cannot fill a gap. The current delivery gate also requires every declared delivered artifact in each post-fix receipt, so scope delivery contracts to artifacts exercised by every included scenario.
- `expected_result`: exactly `value_type`, `byte_length`, `sha256` for the requested consumer projection.

All observations declare `original_input`, `entry_stage`, and `substitutions`. A unit test may start at a helper and replace dependencies; inventory each replacement as `{component, stage, reason}` and set `mocked: true`. Runtime observations require `entry_stage: original_input`, `mocked: false`, and `substitutions: []`.

## Trace and capture files

Runtime observations add `input_lineage: {start: "original_input", trace: {path, sha256}}`. All referenced files must exist and match their fingerprints.

A trace has `evidence_kind: runtime_capture`, matching `scenario_id`, `run_id`, `phase`, `outcome`, `original_input`, empty `substitutions`, matching `loader_sources`, `loaded_artifacts`, `loader_capture`, and ordered `steps`. `loaded_artifacts` must match both the frozen `artifact_order` and the receipt. `loader_capture` references JSON with matching `run_id`, `scenario_id`, `phase`, `loader_sources`, and `artifact_order` obtained from actual loading.

Each step contains its declared `id/role/interface`, `input_sha256`, `output_sha256`, `at_ns`, `substituted: false`, `status`, and a `{path, sha256}` capture reference. Times are ordered within the action interval. A successful run reaches every step. A pre-fix failure can stop at the failing boundary with final status `failure`; do not invent downstream successes.

Each step’s capture JSON repeats matching `scenario_id/run_id/phase/step_id/at_ns/input_sha256/output_sha256/status`, plus an observed `channel` (`browser-event`, `browser-protocol`, `process-io`, `filesystem`, or `user-recording`) and `observation`. The observation contains `readback: {value_type, byte_length, sha256}` matching the step output. The input capture also identifies `source_ref` and matches the original fingerprint. The consumer capture identifies `consumer` and, for success, matches `expected_result`. A `success: true` flag alone is insufficient.

## Independent review and migration

Independent review receipts add one `lineage_reviews` entry per declared scenario: `scenario_id`, concrete `original_input_review`, `mock_boundary_review`, `loader_order_review`, a `runtime_observation` file reference to the post-fix observation JSON (not its outer signed receipt), and nonempty `recorder_artifacts` file references. Recorder paths must also appear in `reviewed_artifacts`. Inspect those files and source captures; review statements are not a substitute for inspection. The independent checker validates the trace and substitutions again.

Archive older v5 receipts unchanged. Capture a new v6 contract, snapshot and observations using the real recorder. Keep temporary evidence in the owning project’s `test/tmp/`. `lineage_test_fixtures.py` exists solely for validator self-tests; its synthetic traces must never be submitted as product captures. `verify_runtime_evidence.py self-test` always emits a mocked unit observation, even if its environment requests `post_fix`.
