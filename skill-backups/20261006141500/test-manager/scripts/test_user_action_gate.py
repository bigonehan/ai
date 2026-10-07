#!/usr/bin/env python3
"""Offline regression tests for the one-user-action budget."""

from __future__ import annotations

import contextlib
import io
import json
import pathlib
import tempfile
import unittest
from unittest import mock

from user_action_gate import add_event, main, reservation_errors, validate_ledger
from verify_runtime_evidence import validate_user_action_evidence


class UserActionGateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.workspace = pathlib.Path(__file__).resolve().parents[1] / "test" / "tmp"
        self.workspace.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="user-action-gate-", dir=self.workspace)
        self.addCleanup(self.temporary.cleanup)
        self.ledger_path = pathlib.Path(self.temporary.name) / "action-ledger.json"
        self.ledger = {"version": 1, "task_id": "incident-background", "events": []}
        self.contract = {"task_id": "incident-background", "user_execution": {
            "manual_run_required": True,
            "action_type": "settings_test",
            "action_ledger_path": str(self.ledger_path.resolve()),
        }}

    def request(self, action_type: str = "settings_test", scope: str = "all-sites") -> None:
        add_event(self.ledger, "requested", action_type, scope, "conversation:request-1", None)

    def observe(self, action_type: str = "settings_test", run_id: str = "run-1") -> None:
        add_event(self.ledger, "observed", action_type, "all-sites", "runtime:run-1", run_id)

    def run_cli(self, *argv: str) -> tuple[int, dict]:
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            code = main(list(argv))
        return code, json.loads(output.getvalue())

    def test_one_full_user_action_and_matching_runtime_run(self) -> None:
        self.assertEqual(reservation_errors(self.ledger, self.contract, self.ledger_path, "settings_test"), [])
        self.request()
        self.observe()
        self.assertEqual(validate_ledger(self.ledger, self.contract, require_observed=True,
                                         runtime_run_ids={"run-1"}), [])

    def test_second_test_and_stage_retry_are_denied_before_request(self) -> None:
        self.request()
        self.assertIn("budget is already consumed", " ".join(reservation_errors(
            self.ledger, self.contract, self.ledger_path, "settings_test")))
        self.assertIn("budget is already consumed", " ".join(reservation_errors(
            self.ledger, self.contract, self.ledger_path, "settings_test")))
        self.request(scope="failed-stage-only")
        self.assertIn("more than one user action", " ".join(validate_ledger(self.ledger)))

    def test_reload_after_test_is_rejected_even_if_contract_still_says_one(self) -> None:
        self.request()
        self.observe()
        self.request("extension_reload", "loaded-worker")
        errors = validate_ledger(self.ledger, self.contract)
        self.assertIn("more than one user action was requested", errors)
        self.assertIn("action ledger contains an action outside the contract entrypoint", errors)

    def test_prior_observed_run_exhausts_budget_without_prior_request_record(self) -> None:
        self.observe()
        self.assertIn("budget is already consumed", " ".join(reservation_errors(
            self.ledger, self.contract, self.ledger_path, "settings_test")))

    def test_prior_run_and_later_same_type_request_are_distinct_actions(self) -> None:
        self.observe()
        self.request()
        self.assertIn("observed action predates request", " ".join(validate_ledger(self.ledger)))

    def test_missing_ledger_and_wrong_run_id_fail_closed(self) -> None:
        code, output = self.run_cli("audit", str(self.ledger_path))
        self.assertEqual(code, 2)
        self.assertFalse(output["ok"])
        self.request()
        self.observe()
        self.assertIn("absent from runtime receipts", " ".join(validate_ledger(
            self.ledger, self.contract, require_observed=True, runtime_run_ids={"other-run"})))

    def test_cli_init_preserves_history_and_historical_mismatch_is_detected(self) -> None:
        self.assertEqual(self.run_cli("init", str(self.ledger_path), "incident-background")[0], 0)
        self.assertEqual(self.run_cli("init", str(self.ledger_path), "incident-background")[0], 2)
        self.assertEqual(self.run_cli("record-existing", str(self.ledger_path), "observed", "generation",
                                      "all-items", "runtime:old-batch", "--run-id", "old-batch")[0], 0)
        self.assertEqual(self.run_cli("record-existing", str(self.ledger_path), "requested", "extension_reload",
                                      "loaded-worker", "conversation:old-request")[0], 0)
        code, output = self.run_cli("audit", str(self.ledger_path))
        self.assertEqual(code, 2)
        self.assertIn("requested and observed actions differ", " ".join(output["errors"]))

    def test_runtime_validator_requires_one_correlated_run_across_scenarios(self) -> None:
        self.request()
        self.observe()
        self.ledger_path.write_text(json.dumps(self.ledger), encoding="utf-8")
        post = [({}, {"run_id": "run-1"}), ({}, {"run_id": "run-1"})]
        self.assertEqual(validate_user_action_evidence(self.contract, post), [])
        post[1][1]["run_id"] = "run-2"
        self.assertIn("one correlated post_fix run_id", " ".join(validate_user_action_evidence(self.contract, post)))
        self.ledger_path.unlink()
        self.assertIn("missing or unreadable", " ".join(validate_user_action_evidence(self.contract, post)))

    def test_cli_reserve_is_atomic_budget_consumption(self) -> None:
        contract_path = pathlib.Path(self.temporary.name) / "contract.json"
        contract_path.write_text(json.dumps(self.contract), encoding="utf-8")
        self.assertEqual(self.run_cli("init", str(self.ledger_path), "incident-background")[0], 0)
        with mock.patch("verify_runtime_evidence.validate_contract", return_value=[]):
            first, _ = self.run_cli("reserve", str(contract_path), str(self.ledger_path),
                                    "settings_test", "all-sites", "conversation:request-1")
            second, second_result = self.run_cli("reserve", str(contract_path), str(self.ledger_path),
                                                 "settings_test", "failed-site-only", "conversation:request-2")
        self.assertEqual(first, 0)
        self.assertEqual(second, 2)
        self.assertIn("budget is already consumed", " ".join(second_result["errors"]))
        self.assertEqual(self.run_cli("observe", str(self.ledger_path), "run-1", "runtime:run-1")[0], 0)
        self.assertEqual(self.run_cli("audit", str(self.ledger_path), "--contract", str(contract_path),
                                      "--require-observed", "--run-id", "run-1")[0], 0)


if __name__ == "__main__":
    unittest.main()
