"""Offline validator regression tests. These are not product runtime evidence."""
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

from effect_safety import validate_contract, validate_observation
from input_lineage import read_artifact
from lineage_test_fixtures import artifact, write
import verify_runtime_evidence as runtime


class EffectSafetyTests(unittest.TestCase):
    def setUp(self):
        workspace = Path(__file__).resolve().parents[1] / "test" / "tmp"
        workspace.mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(dir=workspace, prefix="effect-safety-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.producer = self.root / "producer.js"
        self.producer.write_text("const wait = async () => {};\n")
        self.loader = self.root / "loader.js"
        self.loader.write_text("require('./producer.js');\n")
        self.scenario = {"scenario_id": "no-replay", "requirement_ids": ["req-1"], "user_action": "original action", "requires_effect_safety": True,
            "effect_safety": {"requirement_id": "req-1", "deadlines_ms": [300000, 600000], "observation_ms": 600001,
                "required_triggers": ["rate_limit_alarm", "worker_restart", "response_loss"], "bindings": ["wait"],
                "limits": {"submit": {"min": 1, "max": 1}, "close_tab": {"min": 0, "max": 0}, "replace_identity": {"min": 0, "max": 0}, "next_batch": {"min": 0, "max": 1}},
                "completion_gate": {"effect": "next_batch", "required_state": "all_images_received"}}}
        self.scenario["input_lineage"] = {"loader_sources": [artifact(self.loader)], "artifact_order": [artifact(self.producer)]}
        self.capture = {"scenario_id": "no-replay", "run_id": "run-1", "phase": "unit", "evidence_kind": "unit_capture", "observed_ms": 900003,
            "logical_request_ids": ["request-1"], "monitored_effects": sorted(self.scenario["effect_safety"]["limits"]),
            "events": [{"at_ms": 0, "logical_request_id": "request-1", "effect": "submit"}],
            "triggers": [{"name": n, "at_ms": t, "logical_request_id": "request-1"} for n, t in zip(self.scenario["effect_safety"]["required_triggers"], [300000, 300001, 300002])],
            "bindings": [{"name": "wait", "origin": "production", "injected": False, "producer": artifact(self.producer), "loader": artifact(self.loader)}], "uncaught_errors": []}
        self.observation = {"scenario_id": "no-replay", "run_id": "run-1", "phase": "unit", "outcome": "success"}

    def errors(self):
        self.observation["effect_safety_capture"] = write(self.root / "capture.json", self.capture)
        return validate_observation(self.scenario, self.observation, read_artifact)

    def event(self, effect, **extra):
        self.capture["events"].append({"at_ms": 300000, "logical_request_id": "request-1", "effect": effect, **extra})

    def test_valid_and_legacy(self):
        self.assertEqual(self.errors(), [])
        self.assertEqual(validate_observation({}, {}, read_artifact), [])

    def test_five_minute_close_and_resubmit_mutant(self):
        self.event("close_tab")
        self.event("replace_identity")
        self.event("submit")
        errors = self.errors()
        self.assertTrue(any("close_tab count" in e for e in errors))
        self.assertTrue(any("submit count" in e for e in errors))
        self.assertTrue(any("replace_identity count" in e for e in errors))

    def test_failure_is_not_completion(self):
        self.event("next_batch", state_before="completed_or_failed")
        self.assertTrue(any("completion state" in e for e in self.errors()))
        self.capture["events"][-1]["state_before"] = "all_images_received"
        self.assertEqual(self.errors(), [])

    def test_response_loss_and_restart_cannot_repeat_submit(self):
        self.event("submit")
        self.assertTrue(any("submit count" in e for e in self.errors()))

    def test_missing_wait_is_not_repaired_by_test(self):
        # Execute equivalent broken code, showing how a naive injected fake hid it.
        code = "async function run(){await wait(1)};run().catch(e=>{console.error(e.message);process.exitCode=1})"
        broken = subprocess.run(["node", "-e", code], capture_output=True, text=True)
        self.assertNotEqual(broken.returncode, 0)
        self.assertIn("wait is not defined", broken.stderr)
        fake = subprocess.run(["node", "-e", "const wait=async()=>{};" + code], capture_output=True, text=True)
        self.assertEqual(fake.returncode, 0)
        self.capture["bindings"][0].update(origin="test", injected=True)
        self.assertTrue(any("test-supplied" in e for e in self.errors()))

    def test_timing_and_forbidden_effect_instrumentation(self):
        self.capture["observed_ms"] = 30000
        self.assertTrue(any("delayed effects" in e for e in self.errors()))
        self.capture["observed_ms"] = 900003
        self.capture["monitored_effects"].remove("close_tab")
        self.assertTrue(any("instrumented" in e for e in self.errors()))

    def test_trigger_execution_required(self):
        self.capture["triggers"] = []
        self.assertTrue(any("not executed" in e for e in self.errors()))

    def test_no_success_flag_or_build_as_runtime(self):
        self.observation.update(phase="post_fix")
        self.capture.update(phase="post_fix", evidence_kind="build_capture")
        self.assertTrue(any("promote" in e for e in self.errors()))
        self.capture = {"success": True}
        self.assertTrue(self.errors())

    def test_prefixed_failure_is_evidence_not_completion(self):
        self.observation.update(phase="pre_fix", outcome="failure")
        self.capture.update(phase="pre_fix", evidence_kind="runtime_capture")
        self.assertTrue(any("demonstrate" in e for e in self.errors()))
        self.event("submit")
        self.assertEqual(self.errors(), [])
        self.observation.update(phase="post_fix", outcome="success")
        self.capture.update(phase="post_fix")
        self.assertTrue(any("submit count" in e for e in self.errors()))

    def test_malformed_contract_and_events_fail_closed(self):
        for field, value in [("limits", []), ("bindings", [None]), ("required_triggers", [{}]), ("completion_gate", {"effect": []}), ("observation_ms", True)]:
            candidate = copy.deepcopy(self.scenario)
            candidate["effect_safety"][field] = value
            self.assertTrue(validate_contract(candidate))
        for value in [None, {"logical_request_id": [], "effect": []}, {"at_ms": True}]:
            self.capture["events"] = [value]
            self.assertTrue(self.errors())

    def test_late_and_unattributed_triggers_rejected(self):
        self.capture["triggers"][0]["at_ms"] = self.capture["observed_ms"]
        self.assertTrue(any("after each lifecycle" in e for e in self.errors()))
        self.capture["triggers"][0].pop("logical_request_id")
        self.assertTrue(any("logical request ID" in e for e in self.errors()))

    def test_null_requirements_fail_closed(self):
        self.scenario["requirement_ids"] = None
        self.assertTrue(validate_contract(self.scenario))

    def test_missing_production_dependency_can_prove_prefix_failure(self):
        self.observation.update(phase="pre_fix", outcome="failure")
        self.capture.update(phase="pre_fix", evidence_kind="runtime_capture", uncaught_errors=["wait is not defined"])
        self.capture["bindings"][0].update(resolution="missing", producer=None)
        self.assertEqual(self.errors(), [])
        self.observation.update(phase="post_fix", outcome="success")
        self.capture.update(phase="post_fix")
        self.assertTrue(any("unresolved" in e for e in self.errors()))

    def test_contradictory_and_unknown_binding_resolution_rejected(self):
        self.capture["bindings"][0]["resolution"] = "missing"
        self.assertTrue(any("null producer" in e for e in self.errors()))
        self.capture["bindings"][0]["resolution"] = "unknown"
        self.assertTrue(any("resolution must" in e for e in self.errors()))

    def test_binding_must_match_frozen_provenance(self):
        self.capture["bindings"][0]["producer"] = artifact(self.loader)
        self.assertTrue(any("artifact_order" in e for e in self.errors()))
        self.capture["bindings"][0]["loader"] = artifact(self.producer)
        self.assertTrue(any("loader_sources" in e for e in self.errors()))

    def test_real_unit_receipt_gate_invokes_effect_validator(self):
        self.errors()
        self.observation.update(runtime="offline fixture", observed_consumer="unit consumer", user_action="original action", requirement_ids=["req-1"],
            observation_level="unit", entry_stage="handler", mocked=True, substitutions=[{"component":"browser", "stage":"transport", "reason":"offline only"}])
        contract = {"acceptance_scenarios": [self.scenario], "unit_observation": "unit consumer", "unit_test_commands": [["offline-test"]]}
        state = {"receipt_key": "ab" * 32}
        receipt = {"observation": self.observation, "receipt_version": 6, "contract_digest": runtime.digest_object(contract), "command": ["offline-test"], "exit_code": 0}
        receipt["signature"] = runtime.receipt_signature(receipt, state["receipt_key"])
        self.assertEqual(runtime.validate_receipt(receipt, contract, state), [])
        self.event("submit")
        self.errors()
        receipt["signature"] = runtime.receipt_signature(receipt, state["receipt_key"])
        self.assertTrue(any("submit count" in e for e in runtime.validate_receipt(receipt, contract, state)))


if __name__ == "__main__":
    unittest.main()
