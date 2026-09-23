#!/usr/bin/env python3
"""Run and validate scoped, log-grounded real-runtime evidence."""

from __future__ import annotations

import copy
import fnmatch
import hashlib
import hmac
import json
import os
import pathlib
import secrets
import subprocess
import sys
import tempfile
import time
from typing import Any

from input_lineage import validate_scenario, validate_substitutions, validate_trace, read_artifact
from effect_safety import validate_observation as validate_effect_observation
from user_action_gate import validate_ledger, in_verification_workspace


RECEIPT_VERSION = 6
DEFAULT_IGNORES = (".git/**", "node_modules/**", "__pycache__/**", "*.pyc")
INPUT_KINDS = {"text", "textarea", "contenteditable", "select", "toggle", "file", "paste", "drop", "shortcut", "other"}
API_SCHEMA_TYPES = {"string", "number", "integer", "boolean", "object", "array", "null"}
SAFE_PROBE_METHODS = {"GET", "HEAD", "OPTIONS"}
SENSITIVE_API_KEYS = {
    "authorization", "proxy-authorization", "api_key", "apikey", "token", "access_token",
    "refresh_token", "password", "secret", "cookie", "set-cookie", "bearer",
}
FORBIDDEN_CAPTURE_KEYS = {"raw", "raw_body", "response_body", "request_body", "payload", "headers"}
BASE_INPUT_OBSERVATIONS = {
    "rendered_target", "hit_test", "focus", "first_input", "continuous_input",
    "event_trace", "event_value_snapshot", "value_after_input", "root_count_after_input",
    "uncaught_errors",
}


def canonical(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def digest_object(value: object) -> str:
    return hashlib.sha256(canonical(value)).hexdigest()


def digest_file(path: pathlib.Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def digest_text(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def read_json(path: pathlib.Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path: pathlib.Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def text(value: object) -> bool:
    return isinstance(value, str) and bool(value.strip())


def sha256_text(value: object) -> bool:
    return isinstance(value, str) and len(value) == 64 and all(character in "0123456789abcdef" for character in value)


def contains_raw_input(value: object) -> bool:
    if isinstance(value, dict):
        if any(str(key).lower() in {"text", "raw", "raw_text", "raw_value", "input_value"} for key in value):
            return True
        return any(contains_raw_input(item) for item in value.values())
    if isinstance(value, list):
        return any(contains_raw_input(item) for item in value)
    return False


def unsafe_api_capture_path(value: object, prefix: str = "$") -> str | None:
    if isinstance(value, dict):
        for key, item in value.items():
            normalized = str(key).strip().lower().replace("-", "_")
            if normalized in {key.replace("-", "_") for key in SENSITIVE_API_KEYS | FORBIDDEN_CAPTURE_KEYS}:
                return f"{prefix}.{key}"
            nested = unsafe_api_capture_path(item, f"{prefix}.{key}")
            if nested:
                return nested
    elif isinstance(value, list):
        for index, item in enumerate(value):
            nested = unsafe_api_capture_path(item, f"{prefix}[{index}]")
            if nested:
                return nested
    return None


def validate_api_schema(value: object, field: str, errors: list[str]) -> list[dict[str, Any]]:
    if not isinstance(value, list) or not value:
        errors.append(f"{field} must be a non-empty array")
        return []
    result: list[dict[str, Any]] = []
    paths: set[str] = set()
    for index, item in enumerate(value):
        item_field = f"{field}[{index}]"
        if not isinstance(item, dict):
            errors.append(f"{item_field} must be an object")
            continue
        path = item.get("path")
        if not text(path) or not str(path).startswith("$."):
            errors.append(f"{item_field}.path must be a JSON path beginning with $.")
        elif any(marker in str(path).lower().replace("-", "_") for marker in SENSITIVE_API_KEYS):
            errors.append(f"{item_field}.path must not identify credentials or secrets")
        elif path in paths:
            errors.append(f"{field} contains duplicate path: {path}")
        else:
            paths.add(path)
        if item.get("type") not in API_SCHEMA_TYPES:
            errors.append(f"{item_field}.type must be one of {sorted(API_SCHEMA_TYPES)}")
        if not isinstance(item.get("required"), bool):
            errors.append(f"{item_field}.required must be boolean")
        if set(item) - {"path", "type", "required", "safe_value"}:
            errors.append(f"{item_field} contains unsupported fields")
        safe_value = item.get("safe_value")
        if unsafe_api_capture_path(safe_value) or (isinstance(safe_value, str) and safe_value.lower().startswith("bearer ")):
            errors.append(f"{item_field}.safe_value contains sensitive material")
        result.append(item)
    return result


def checked_artifact(item: object, field: str, errors: list[str]) -> pathlib.Path | None:
    if not isinstance(item, dict):
        errors.append(f"{field} must be an object")
        return None
    path_value = item.get("path")
    checksum = item.get("sha256")
    if not text(path_value) or not pathlib.Path(path_value).is_absolute():
        errors.append(f"{field}.path must be an absolute path")
        return None
    path = pathlib.Path(path_value)
    if not path.is_file():
        errors.append(f"{field}.path must be an existing file")
        return None
    if not sha256_text(checksum) or digest_file(path) != checksum:
        errors.append(f"{field}.sha256 does not match the artifact")
    return path


def validate_external_api_contracts(
    contract: dict[str, Any], scenario_ids: set[str], scenario_api_links: dict[str, set[str]], errors: list[str]
) -> None:
    decision = contract.get("external_api_validation")
    if not isinstance(decision, dict):
        errors.append("external_api_validation must explicitly declare applicability")
        return
    applicable = decision.get("applicable")
    if not isinstance(applicable, bool):
        errors.append("external_api_validation.applicable must be boolean")
    if not text(decision.get("reason")):
        errors.append("external_api_validation.reason must be non-empty text")
    api_contracts = decision.get("contracts")
    if not isinstance(api_contracts, list):
        errors.append("external_api_validation.contracts must be an array")
        return
    if applicable is False and api_contracts:
        errors.append("external_api_validation.contracts must be empty when not applicable")
    if applicable is True and not api_contracts:
        errors.append("applicable external API validation requires at least one contract")

    declared_ids: set[str] = set()
    linked_by_contract: dict[str, set[str]] = {}
    environments: set[str] = set()
    for index, item in enumerate(api_contracts):
        field = f"external_api_validation.contracts[{index}]"
        if not isinstance(item, dict):
            errors.append(f"{field} must be an object")
            continue
        contract_id = item.get("contract_id")
        if not text(contract_id):
            errors.append(f"{field}.contract_id must be non-empty text")
            continue
        if contract_id in declared_ids:
            errors.append(f"duplicate external API contract id: {contract_id}")
        declared_ids.add(contract_id)
        for key in ("system", "operation", "endpoint_pattern", "authoritative_environment"):
            if not text(item.get(key)):
                errors.append(f"{field}.{key} must be non-empty text")
        endpoint = item.get("endpoint_pattern")
        if text(endpoint) and ("?" in endpoint or "@" in endpoint):
            errors.append(f"{field}.endpoint_pattern must omit credentials and query values")
        method = item.get("method")
        if not text(method) or method != str(method).upper():
            errors.append(f"{field}.method must be an uppercase HTTP method")
        environment = item.get("authoritative_environment")
        if text(environment):
            environments.add(environment)
        linked = item.get("scenario_ids")
        if not isinstance(linked, list) or not linked or not all(text(value) for value in linked):
            errors.append(f"{field}.scenario_ids must be a non-empty text array")
            linked_set: set[str] = set()
        else:
            linked_set = set(linked)
            if len(linked_set) != len(linked):
                errors.append(f"{field}.scenario_ids must not contain duplicates")
            if not linked_set.issubset(scenario_ids):
                errors.append(f"{field}.scenario_ids contains undeclared scenarios")
        linked_by_contract[contract_id] = linked_set

        schema = validate_api_schema(item.get("response_schema"), f"{field}.response_schema", errors)
        schema_paths = {entry.get("path") for entry in schema if isinstance(entry, dict) and text(entry.get("path"))}
        probe = item.get("safe_probe")
        if not isinstance(probe, dict):
            errors.append(f"{field}.safe_probe must be an object")
            continue
        available = probe.get("available")
        if not isinstance(available, bool):
            errors.append(f"{field}.safe_probe.available must be boolean")
        source_kind = probe.get("source_kind")
        allowed_sources = {"live_read_only"} if available is True else {"recorded_response", "authoritative_docs"}
        if source_kind not in allowed_sources:
            errors.append(f"{field}.safe_probe.source_kind is inconsistent with availability")
        if available is True and method not in SAFE_PROBE_METHODS:
            errors.append(f"{field}.safe_probe live read-only capture requires GET, HEAD, or OPTIONS")
        capture_path = checked_artifact(probe, f"{field}.safe_probe", errors)
        capture: object = None
        if capture_path:
            try:
                capture = read_json(capture_path)
            except (OSError, json.JSONDecodeError) as error:
                errors.append(f"{field}.safe_probe capture must be valid JSON: {error}")
            if isinstance(capture, dict):
                unsafe = unsafe_api_capture_path(capture)
                if unsafe:
                    errors.append(f"{field}.safe_probe capture exposes forbidden field at {unsafe}")
                if capture.get("contains_secrets") is not False:
                    errors.append(f"{field}.safe_probe capture must declare contains_secrets false")
                for key, expected in (
                    ("contract_id", contract_id), ("system", item.get("system")),
                    ("operation", item.get("operation")), ("method", method),
                    ("endpoint_pattern", endpoint), ("authoritative_environment", environment),
                    ("source_kind", source_kind), ("response_schema", schema),
                ):
                    if capture.get(key) != expected:
                        errors.append(f"{field}.safe_probe capture {key} does not match the contract")

        fixtures = item.get("fixtures")
        if not isinstance(fixtures, list) or not fixtures:
            errors.append(f"{field}.fixtures must be a non-empty array")
        else:
            for fixture_index, fixture in enumerate(fixtures):
                fixture_field = f"{field}.fixtures[{fixture_index}]"
                checked_artifact(fixture, fixture_field, errors)
                if not isinstance(fixture, dict) or fixture.get("source_capture_sha256") != probe.get("sha256"):
                    errors.append(f"{fixture_field}.source_capture_sha256 must match the safe probe capture")
                if isinstance(fixture, dict) and fixture.get("response_schema") != schema:
                    errors.append(f"{fixture_field}.response_schema must match the live capture schema")

        consumer = item.get("consumer")
        consumer_path = checked_artifact(consumer, f"{field}.consumer", errors)
        if isinstance(consumer, dict):
            if not text(consumer.get("expression")):
                errors.append(f"{field}.consumer.expression must be non-empty text")
            required_fields = consumer.get("required_fields")
            required_schema_paths = {entry.get("path") for entry in schema if entry.get("required") is True}
            if not isinstance(required_fields, list) or set(required_fields) != required_schema_paths or len(required_fields) != len(set(required_fields)):
                errors.append(f"{field}.consumer.required_fields must exactly match required response schema paths")
        if consumer_path and not schema_paths:
            errors.append(f"{field}.consumer has no validated response fields")

        mutations = item.get("mutation_tests")
        mutation_kinds: set[str] = set()
        schema_types = {entry.get("path"): entry.get("type") for entry in schema}
        if not isinstance(mutations, list) or not mutations:
            errors.append(f"{field}.mutation_tests must be a non-empty array")
        else:
            for mutation_index, mutation in enumerate(mutations):
                mutation_field = f"{field}.mutation_tests[{mutation_index}]"
                mutation_path = checked_artifact(mutation, mutation_field, errors)
                if not mutation_path:
                    continue
                try:
                    receipt = read_json(mutation_path)
                except (OSError, json.JSONDecodeError) as error:
                    errors.append(f"{mutation_field} receipt must be valid JSON: {error}")
                    continue
                kind = receipt.get("mutation_kind") if isinstance(receipt, dict) else None
                if kind in mutation_kinds:
                    errors.append(f"{field}.mutation_tests contains duplicate kind: {kind}")
                if text(kind):
                    mutation_kinds.add(kind)
                common_valid = (
                    isinstance(receipt, dict)
                    and receipt.get("contract_id") == contract_id
                    and receipt.get("rejected") is True
                    and receipt.get("production_consumer_executed") is True
                    and isinstance(receipt.get("exit_code"), int)
                    and receipt.get("exit_code") != 0
                )
                if kind == "field_name":
                    valid = (
                        common_valid
                        and receipt.get("from_path") in schema_paths
                        and text(receipt.get("to_path"))
                        and receipt.get("to_path") not in schema_paths
                    )
                elif kind == "field_type":
                    valid = (
                        common_valid
                        and receipt.get("path") in schema_paths
                        and receipt.get("from_type") == schema_types.get(receipt.get("path"))
                        and receipt.get("to_type") in API_SCHEMA_TYPES
                        and receipt.get("to_type") != receipt.get("from_type")
                    )
                else:
                    valid = False
                if not valid:
                    errors.append(f"{mutation_field} must prove its production-consumer mutant is rejected")
        if mutation_kinds != {"field_name", "field_type"}:
            errors.append(f"{field}.mutation_tests must include field_name and field_type mutants")

    linked_ids = set().union(*scenario_api_links.values()) if scenario_api_links else set()
    if linked_ids != declared_ids:
        errors.append("acceptance scenario external_api_contract_ids must cover every and only declared external API contract")
    for contract_id, linked_scenarios in linked_by_contract.items():
        reverse = {scenario_id for scenario_id, ids in scenario_api_links.items() if contract_id in ids}
        if reverse != linked_scenarios:
            errors.append(f"external API contract {contract_id} scenario links are not bidirectional")
    if applicable is False and linked_ids:
        errors.append("external API links must be empty when validation is not applicable")
    target = contract.get("runtime_target")
    if applicable is True and isinstance(target, dict):
        if set(target.get("external_api_environments", [])) != environments:
            errors.append("runtime_target.external_api_environments must exactly match external API contract environments")


def validate_external_api_observation(contract: dict[str, Any], observation: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    scenario = next(
        (item for item in contract.get("acceptance_scenarios", []) if item.get("scenario_id") == observation.get("scenario_id")),
        {},
    )
    linked_ids = scenario.get("external_api_contract_ids", [])
    if not linked_ids:
        if observation.get("external_api_validation") not in (None, []):
            errors.append("runtime observation contains undeclared external API evidence")
        return errors
    evidence = observation.get("external_api_validation")
    if not isinstance(evidence, list):
        return ["observation requires external_api_validation evidence"]
    evidence_by_id = {
        item.get("contract_id"): item for item in evidence
        if isinstance(item, dict) and text(item.get("contract_id"))
    }
    if len(evidence_by_id) != len(evidence) or set(evidence_by_id) != set(linked_ids):
        errors.append("observation external API evidence must exactly match scenario links")
        return errors
    contracts = {
        item["contract_id"]: item for item in contract["external_api_validation"]["contracts"]
        if item.get("contract_id") in linked_ids
    }
    is_unit = observation.get("phase") == "unit"
    successful_post = observation.get("phase") == "post_fix" and observation.get("outcome") == "success"
    for contract_id, item in evidence_by_id.items():
        spec = contracts[contract_id]
        field = f"external API observation {contract_id}"
        probe = spec["safe_probe"]
        if item.get("authoritative_environment") != spec.get("authoritative_environment"):
            errors.append(f"{field} environment does not match the contract")
        if item.get("capture_sha256") != probe.get("sha256"):
            errors.append(f"{field} capture_sha256 does not match the contract")
        if item.get("response_schema") != spec.get("response_schema"):
            errors.append(f"{field} response schema does not match the captured API contract")
        if item.get("production_consumer_executed") is not True:
            errors.append(f"{field} did not execute the production consumer")
        if unsafe_api_capture_path(item):
            errors.append(f"{field} exposes credentials or raw API material")
        if is_unit:
            if item.get("boundary_source") != "capture_fixture":
                errors.append(f"{field} unit evidence must use the capture-derived fixture")
            expected_fixtures = {(entry["path"], entry["sha256"]) for entry in spec.get("fixtures", [])}
            observed_fixtures = {
                (entry.get("path"), entry.get("sha256")) for entry in item.get("fixture_artifacts", [])
                if isinstance(entry, dict)
            }
            if observed_fixtures != expected_fixtures:
                errors.append(f"{field} fixture artifacts do not match the contract")
            if set(item.get("mutations_rejected", [])) != {"field_name", "field_type"}:
                errors.append(f"{field} unit evidence must reject field_name and field_type mutants")
        if successful_post:
            if item.get("boundary_source") != "live_response" or item.get("mocked") is not False:
                errors.append(f"{field} successful runtime evidence must come from a non-mocked live response")
            if item.get("consumer_readback") is not True:
                errors.append(f"{field} successful runtime evidence requires authoritative consumer readback")
    return errors


def absolute_paths(value: object, field: str, errors: list[str]) -> list[pathlib.Path]:
    if not isinstance(value, list) or not value:
        errors.append(f"{field} must be a non-empty array")
        return []
    result = []
    for index, item in enumerate(value):
        if not text(item) or not pathlib.Path(item).is_absolute():
            errors.append(f"{field}[{index}] must be an absolute path")
            continue
        result.append(pathlib.Path(item).resolve())
    return result


def validate_sourced_record(item: object, field: str, errors: list[str]) -> None:
    if not isinstance(item, dict):
        errors.append(f"{field} must be an object")
        return
    for key in ("source_path", "source_text", "source_sha256"):
        if not text(item.get(key)):
            errors.append(f"{field}.{key} must be non-empty text")
    source_path = pathlib.Path(str(item.get("source_path", "")))
    source_text = item.get("source_text")
    if not source_path.is_absolute() or not source_path.is_file():
        errors.append(f"{field}.source_path must be an existing absolute file")
    elif text(source_text) and source_text not in source_path.read_text(encoding="utf-8"):
        errors.append(f"{field}.source_text is not present in source_path")
    if text(source_text) and item.get("source_sha256") != digest_text(source_text):
        errors.append(f"{field}.source_sha256 does not match source_text")


def log_source_files(contract: dict[str, Any]) -> list[pathlib.Path]:
    result: list[pathlib.Path] = []
    for source_text in contract.get("runtime_log_sources", []):
        source = pathlib.Path(source_text).resolve()
        if source.is_file():
            result.append(source)
        elif source.is_dir():
            result.extend(path for path in source.rglob("*") if path.is_file())
    return sorted(set(result))


def log_source_inventory(contract: dict[str, Any]) -> dict[str, dict[str, int]]:
    return {
        str(path): {"mtime_ns": path.stat().st_mtime_ns, "size": path.stat().st_size}
        for path in log_source_files(contract)
    }


def changed_log_files(contract: dict[str, Any], state: dict[str, Any]) -> list[pathlib.Path]:
    before_logs = state.get("runtime_log_sources", {})
    return [
        path for path in log_source_files(contract)
        if str(path) not in before_logs
        or before_logs[str(path)].get("mtime_ns") != path.stat().st_mtime_ns
        or before_logs[str(path)].get("size") != path.stat().st_size
    ]


def validate_contract(contract: object) -> list[str]:
    if not isinstance(contract, dict):
        return ["contract root must be an object"]
    errors: list[str] = []
    for key in ("task_id", "workflow", "user_action", "expected_outcome", "unit_observation", "authoritative_consumer"):
        if not text(contract.get(key)):
            errors.append(f"{key} must be non-empty text")
    coverage = contract.get("requirement_coverage")
    requirement_ids: set[str] = set()
    if not isinstance(coverage, list) or not coverage:
        errors.append("requirement_coverage must be a non-empty array")
    else:
        for index, item in enumerate(coverage):
            validate_sourced_record(item, f"requirement_coverage[{index}]", errors)
            if not isinstance(item, dict) or not text(item.get("id")) or not text(item.get("incident_key")):
                errors.append(f"requirement_coverage[{index}] requires id and incident_key")
                continue
            if item["id"] in requirement_ids:
                errors.append(f"duplicate requirement id: {item['id']}")
            requirement_ids.add(item["id"])
    scenarios = contract.get("acceptance_scenarios")
    scenario_ids: set[str] = set()
    covered_requirement_ids: set[str] = set()
    scenario_origins: set[str] = set()
    scenario_api_links: dict[str, set[str]] = {}
    if not isinstance(scenarios, list) or not scenarios:
        errors.append("acceptance_scenarios must be a non-empty array")
    else:
        for index, scenario in enumerate(scenarios):
            field = f"acceptance_scenarios[{index}]"
            if not isinstance(scenario, dict):
                errors.append(f"{field} must be an object")
                continue
            for key in ("scenario_id", "user_action", "authoritative_consumer"):
                if not text(scenario.get(key)):
                    errors.append(f"{field}.{key} must be non-empty text")
            scenario_id = scenario.get("scenario_id")
            if text(scenario_id):
                if scenario_id in scenario_ids:
                    errors.append(f"duplicate scenario id: {scenario_id}")
                scenario_ids.add(scenario_id)
                api_links = scenario.get("external_api_contract_ids")
                if not isinstance(api_links, list) or not all(text(item) for item in api_links):
                    errors.append(f"{field}.external_api_contract_ids must be a text array")
                    api_links = []
                elif len(api_links) != len(set(api_links)):
                    errors.append(f"{field}.external_api_contract_ids must not contain duplicates")
                scenario_api_links[scenario_id] = set(api_links)
            linked = scenario.get("requirement_ids")
            if not isinstance(linked, list) or not linked or not all(text(item) for item in linked):
                errors.append(f"{field}.requirement_ids must be a non-empty text array")
            else:
                covered_requirement_ids.update(linked)
            origin = scenario.get("origin")
            if not isinstance(origin, str):
                errors.append(f"{field}.origin must be text")
            elif origin:
                scenario_origins.add(origin)
            count_value = scenario.get("expected_output_count")
            if not isinstance(count_value, int) or isinstance(count_value, bool) or count_value < 0:
                errors.append(f"{field}.expected_output_count must be a non-negative integer")
    if requirement_ids and covered_requirement_ids != requirement_ids:
        errors.append("acceptance_scenarios must cover every and only declared requirement id")
    user_execution = contract.get("user_execution")
    if user_execution is not None:
        if not isinstance(user_execution, dict):
            errors.append("user_execution must be an object when provided")
        else:
            manual_required = user_execution.get("manual_run_required")
            if not isinstance(manual_required, bool):
                errors.append("user_execution.manual_run_required must be boolean")
            if manual_required is True:
                if user_execution.get("max_user_runs") != 1:
                    errors.append("user_execution.max_user_runs must be exactly 1")
                linked = user_execution.get("scenario_ids")
                if not isinstance(linked, list) or not linked or not all(text(item) for item in linked):
                    errors.append("user_execution.scenario_ids must be a non-empty text array")
                    linked = []
                elif len(linked) != len(set(linked)) or set(linked) != scenario_ids:
                    errors.append("user_execution.scenario_ids must exactly cover every acceptance scenario once")
                linked_actions = {
                    scenario.get("user_action") for scenario in (scenarios if isinstance(scenarios, list) else [])
                    if isinstance(scenario, dict) and scenario.get("scenario_id") in linked
                }
                if len(linked_actions) != 1:
                    errors.append("one user execution requires the same initiating action for every linked scenario")
                if user_execution.get("production_path_complete") is not True:
                    errors.append("user_execution.production_path_complete must be true")
                if user_execution.get("continues_independent_targets") is not True:
                    errors.append("user_execution.continues_independent_targets must be true")
                if user_execution.get("micro_stage_reassignment") is not False:
                    errors.append("user_execution.micro_stage_reassignment must be false")
                if user_execution.get("post_run_debug_source") != "captured_logs_and_automation":
                    errors.append("user_execution.post_run_debug_source must be captured_logs_and_automation")
                ledger_path = user_execution.get("action_ledger_path")
                if ledger_path is not None:
                    if not text(ledger_path) or not pathlib.Path(ledger_path).is_absolute() or not in_verification_workspace(pathlib.Path(ledger_path)):
                        errors.append("user_execution.action_ledger_path must be an absolute project test/tmp path")
                    if user_execution.get("action_type") not in {"settings_test", "extension_reload", "generation", "other"}:
                        errors.append("user_execution.action_type must identify the initiating action")
    requires_preservation = contract.get("requires_behavior_preservation", False)
    if not isinstance(requires_preservation, bool):
        errors.append("requires_behavior_preservation must be boolean when provided")
    if requires_preservation:
        for index, scenario in enumerate(scenarios if isinstance(scenarios, list) else []):
            field = f"acceptance_scenarios[{index}]"
            if not isinstance(scenario, dict):
                continue
            if scenario.get("coverage_kind") not in {"target", "preservation"}:
                errors.append(f"{field}.coverage_kind must be target or preservation")
            lifecycle = scenario.get("lifecycle_conditions")
            if not isinstance(lifecycle, list) or not lifecycle or not all(text(item) for item in lifecycle):
                errors.append(f"{field}.lifecycle_conditions must be a non-empty text array")
        analysis = contract.get("behavior_change_analysis")
        if not isinstance(analysis, dict):
            errors.append("behavior_change_analysis is required for shared control-flow changes")
        else:
            if not text(analysis.get("baseline_ref")):
                errors.append("behavior_change_analysis.baseline_ref must be non-empty text")
            points = analysis.get("changed_contract_points")
            if not isinstance(points, list) or not points:
                errors.append("behavior_change_analysis.changed_contract_points must be a non-empty array")
            else:
                invariant_ids: set[str] = set()
                for index, point in enumerate(points):
                    field = f"behavior_change_analysis.changed_contract_points[{index}]"
                    if not isinstance(point, dict):
                        errors.append(f"{field} must be an object")
                        continue
                    for key in ("id", "kind", "before_contract", "after_contract"):
                        if not text(point.get(key)):
                            errors.append(f"{field}.{key} must be non-empty text")
                    point_id = point.get("id")
                    if text(point_id):
                        if point_id in invariant_ids:
                            errors.append(f"duplicate behavior contract point id: {point_id}")
                        invariant_ids.add(point_id)

                    def state_set(key: str, *, non_empty: bool = False) -> set[str]:
                        value = point.get(key)
                        if not isinstance(value, list) or (non_empty and not value) or not all(text(item) for item in value):
                            qualifier = "non-empty " if non_empty else ""
                            errors.append(f"{field}.{key} must be a {qualifier}text array")
                            return set()
                        if len(value) != len(set(value)):
                            errors.append(f"{field}.{key} must not contain duplicates")
                        return set(value)

                    previous_states = state_set("previously_accepted_states", non_empty=True)
                    preserved_states = state_set("preserved_states")
                    removed_states = state_set("intentionally_removed_states")
                    previous_branches = state_set("downstream_branches_before", non_empty=True)
                    preserved_branches = state_set("downstream_branches_preserved")
                    removed_branches = state_set("intentionally_removed_branches")
                    if preserved_states & removed_states or previous_states != preserved_states | removed_states:
                        errors.append(f"{field} must partition every previously accepted state into preserved or intentionally removed")
                    if preserved_branches & removed_branches or previous_branches != preserved_branches | removed_branches:
                        errors.append(f"{field} must partition every downstream branch into preserved or intentionally removed")
                    scenario_links = state_set("preservation_scenario_ids", non_empty=bool(preserved_states or preserved_branches))
                    if not scenario_links.issubset(scenario_ids):
                        errors.append(f"{field}.preservation_scenario_ids contains undeclared scenarios")
                    linked_scenarios = [
                        scenario for scenario in (scenarios if isinstance(scenarios, list) else [])
                        if isinstance(scenario, dict) and scenario.get("scenario_id") in scenario_links
                    ]
                    if any(scenario.get("coverage_kind") != "preservation" for scenario in linked_scenarios):
                        errors.append(f"{field}.preservation_scenario_ids must reference preservation scenarios")
                    removal_ids = state_set("removal_requirement_ids")
                    if removed_states or removed_branches:
                        if not removal_ids:
                            errors.append(f"{field} intentionally removed behavior requires removal_requirement_ids")
                        elif not removal_ids.issubset(requirement_ids):
                            errors.append(f"{field}.removal_requirement_ids contains undeclared requirements")
                    elif removal_ids:
                        errors.append(f"{field}.removal_requirement_ids must be empty when no behavior is removed")
    unit_commands = contract.get("unit_test_commands")
    if not isinstance(unit_commands, list) or not unit_commands:
        errors.append("unit_test_commands must be a non-empty array")
    elif not all(isinstance(command, list) and command and all(text(item) for item in command) for command in unit_commands):
        errors.append("every unit_test_commands entry must be a non-empty argv array")
    count = contract.get("repeated_report_count")
    if not isinstance(count, int) or isinstance(count, bool) or count < 0:
        errors.append("repeated_report_count must be a non-negative integer")
    history = contract.get("incident_history")
    if not isinstance(history, list):
        errors.append("incident_history must be an array")
    else:
        if isinstance(count, int) and not isinstance(count, bool) and len(history) != count:
            errors.append("incident_history length must equal repeated_report_count")
        for index, item in enumerate(history):
            validate_sourced_record(item, f"incident_history[{index}]", errors)
            if not isinstance(item, dict) or not text(item.get("incident_key")) or not text(item.get("prior_evidence_gap")):
                errors.append(f"incident_history[{index}] requires incident_key and prior_evidence_gap")
    log_sources_value = contract.get("runtime_log_sources")
    if not isinstance(log_sources_value, list) or not all(text(item) and pathlib.Path(item).is_absolute() and pathlib.Path(item).exists() for item in log_sources_value):
        errors.append("runtime_log_sources must be an array of existing absolute paths")
    allowed = absolute_paths(contract.get("allowed_roots"), "allowed_roots", errors)
    watched = absolute_paths(contract.get("watch_roots"), "watch_roots", errors)
    forbidden_value = contract.get("forbidden_roots", [])
    forbidden: list[pathlib.Path] = []
    if not isinstance(forbidden_value, list):
        errors.append("forbidden_roots must be an array")
    else:
        for index, item in enumerate(forbidden_value):
            if not text(item) or not pathlib.Path(item).is_absolute():
                errors.append(f"forbidden_roots[{index}] must be an absolute path")
            else:
                forbidden.append(pathlib.Path(item).resolve())
    for root in allowed + forbidden:
        if watched and not any(root == watch or root.is_relative_to(watch) for watch in watched):
            errors.append(f"scope root is outside watch_roots: {root}")
    adjacent = contract.get("adjacent_workflows")
    if not isinstance(adjacent, list) or not all(text(item) for item in adjacent):
        errors.append("adjacent_workflows must be an array of non-empty text")
    fidelity = contract.get("input_fidelity")
    if not isinstance(fidelity, dict):
        errors.append("input_fidelity must be an object")
    else:
        if not text(fidelity.get("production_boundary")):
            errors.append("input_fidelity.production_boundary must be non-empty text")
        required = fidelity.get("required_observations")
        if not isinstance(required, list) or not required or not all(text(item) for item in required):
            errors.append("input_fidelity.required_observations must be a non-empty text array")
        forbidden_shortcuts = fidelity.get("forbidden_shortcuts")
        if not isinstance(forbidden_shortcuts, list) or not forbidden_shortcuts or not all(text(item) for item in forbidden_shortcuts):
            errors.append("input_fidelity.forbidden_shortcuts must be a non-empty text array")
    input_validation = contract.get("input_validation")
    if not isinstance(input_validation, dict):
        errors.append("input_validation must be an object with an explicit applicable decision")
    else:
        applicable = input_validation.get("applicable")
        if not isinstance(applicable, bool):
            errors.append("input_validation.applicable must be boolean")
        if not text(input_validation.get("reason")):
            errors.append("input_validation.reason must be non-empty text")
        surfaces = input_validation.get("surfaces")
        if not isinstance(surfaces, list):
            errors.append("input_validation.surfaces must be an array")
            surfaces = []
        if applicable is False and surfaces:
            errors.append("input_validation.surfaces must be empty when input validation is not applicable")
        if applicable is True and not surfaces:
            errors.append("applicable input validation requires at least one surface")
        surface_ids: set[str] = set()
        for index, surface in enumerate(surfaces):
            field = f"input_validation.surfaces[{index}]"
            if not isinstance(surface, dict):
                errors.append(f"{field} must be an object")
                continue
            surface_id = surface.get("surface_id")
            if not text(surface_id):
                errors.append(f"{field}.surface_id must be non-empty text")
            elif surface_id in surface_ids:
                errors.append(f"duplicate input surface id: {surface_id}")
            else:
                surface_ids.add(surface_id)
            if surface.get("input_kind") not in INPUT_KINDS:
                errors.append(f"{field}.input_kind must be one of {sorted(INPUT_KINDS)}")
            linked_scenarios = surface.get("scenario_ids")
            if not isinstance(linked_scenarios, list) or not linked_scenarios or not all(text(item) for item in linked_scenarios):
                errors.append(f"{field}.scenario_ids must be a non-empty text array")
            elif len(linked_scenarios) != len(set(linked_scenarios)):
                errors.append(f"{field}.scenario_ids must not contain duplicates")
            elif not set(linked_scenarios).issubset(scenario_ids):
                errors.append(f"{field}.scenario_ids contains undeclared scenarios")
            for key in ("ime_applicable", "commit_required", "cancel_required", "persistence_required"):
                if not isinstance(surface.get(key), bool):
                    errors.append(f"{field}.{key} must be boolean")
            if not text(surface.get("authoritative_consumer")):
                errors.append(f"{field}.authoritative_consumer must be non-empty text")
            observations = surface.get("required_runtime_observations")
            if not isinstance(observations, list) or not observations or not all(text(item) for item in observations):
                errors.append(f"{field}.required_runtime_observations must be a non-empty text array")
                continue
            required_observations = set(BASE_INPUT_OBSERVATIONS)
            if surface.get("ime_applicable") is True:
                required_observations.add("ime_composition")
            if surface.get("commit_required") is True:
                required_observations.add("commit_readback")
            if surface.get("cancel_required") is True:
                required_observations.add("cancel_readback")
            if surface.get("persistence_required") is True:
                required_observations.add("reload_readback")
            missing_observations = required_observations - set(observations)
            if missing_observations:
                errors.append(f"{field}.required_runtime_observations omits {sorted(missing_observations)}")
    target = contract.get("runtime_target")
    if not isinstance(target, dict):
        errors.append("runtime_target must be an object")
    else:
        if not text(target.get("environment")):
            errors.append("runtime_target.environment must be non-empty text")
        origins = target.get("required_origins")
        if not isinstance(origins, list) or not all(text(item) for item in origins):
            errors.append("runtime_target.required_origins must be an array of non-empty text")
        elif set(origins) != scenario_origins:
            errors.append("runtime_target.required_origins must exactly equal acceptance scenario origins")
        if not isinstance(target.get("allow_fixture_origins"), bool):
            errors.append("runtime_target.allow_fixture_origins must be boolean")
        output_root = target.get("authoritative_output_root")
        if output_root is not None and (not text(output_root) or not pathlib.Path(output_root).is_absolute()):
            errors.append("runtime_target.authoritative_output_root must be null or an absolute path")
        if target.get("environment") == "windows_user_chrome":
            for key in ("extension_id", "extension_path", "windows_extension_path", "extension_version", "secure_preferences_path"):
                if not text(target.get(key)):
                    errors.append(f"runtime_target.{key} must be non-empty text for windows_user_chrome")
            extension_path = pathlib.Path(str(target.get("extension_path", "")))
            preferences_path = pathlib.Path(str(target.get("secure_preferences_path", "")))
            if not extension_path.is_absolute() or not extension_path.is_dir():
                errors.append("runtime_target.extension_path must be an existing absolute directory")
            if not preferences_path.is_absolute() or not preferences_path.is_file():
                errors.append("runtime_target.secure_preferences_path must be an existing absolute file")
    validate_external_api_contracts(contract, scenario_ids, scenario_api_links, errors)
    if count and count >= 2:
        diagnosis = contract.get("runtime_diagnosis")
        if not isinstance(diagnosis, dict):
            errors.append("repeated reports require runtime_diagnosis")
        else:
            if diagnosis.get("source") not in {"runtime_log", "runtime_trace", "runtime_stack"}:
                errors.append("runtime_diagnosis.source must be runtime_log, runtime_trace, or runtime_stack")
            if not text(diagnosis.get("failing_boundary")):
                errors.append("runtime_diagnosis.failing_boundary must be non-empty text")
            if not text(diagnosis.get("specific_failing_detail")):
                errors.append("repeated reports require runtime_diagnosis.specific_failing_detail before a functional fix")
            artifacts = diagnosis.get("log_artifacts")
            if not isinstance(artifacts, list) or not artifacts:
                errors.append("repeated reports require runtime log artifacts")
            elif not all(text(item) and pathlib.Path(item).is_file() for item in artifacts):
                errors.append("every runtime diagnosis artifact must be an existing file")
    delivered = contract.get("delivered_artifacts")
    if not isinstance(delivered, list) or not delivered:
        errors.append("delivered_artifacts must be a non-empty array")
    elif not all(text(item) and pathlib.Path(item).is_file() for item in delivered):
        errors.append("every delivered artifact must be an existing file")
    for scenario in contract.get("acceptance_scenarios", []):
        if isinstance(scenario, dict):
            errors.extend(validate_scenario(scenario))
    return errors


def ignored(relative: str, patterns: list[str]) -> bool:
    normalized = relative.replace(os.sep, "/")
    return any(fnmatch.fnmatch(normalized, pattern) or normalized.startswith(pattern.removesuffix("/**") + "/") for pattern in patterns)


def scan_files(contract: dict[str, Any]) -> dict[str, str]:
    patterns = [*DEFAULT_IGNORES, *contract.get("ignore_globs", [])]
    files: dict[str, str] = {}
    for root_text in contract["watch_roots"]:
        root = pathlib.Path(root_text).resolve()
        if root.is_file():
            files[str(root)] = digest_file(root)
            continue
        if not root.exists():
            continue
        for path in root.rglob("*"):
            if not path.is_file():
                continue
            relative = str(path.relative_to(root))
            if ignored(relative, patterns):
                continue
            files[str(path.resolve())] = digest_file(path)
    return files


def create_snapshot(contract: dict[str, Any]) -> dict[str, Any]:
    return {
        "version": RECEIPT_VERSION,
        "contract_digest": digest_object(contract),
        "created_ns": time.time_ns(),
        "receipt_key": secrets.token_hex(32),
        "files": scan_files(contract),
        "runtime_log_sources": log_source_inventory(contract),
    }


def changed_paths(before: dict[str, str], after: dict[str, str]) -> list[pathlib.Path]:
    return [pathlib.Path(path) for path in sorted(set(before) | set(after)) if before.get(path) != after.get(path)]


def in_roots(path: pathlib.Path, roots: list[pathlib.Path]) -> bool:
    resolved = path.resolve()
    return any(resolved == root or resolved.is_relative_to(root) for root in roots)


def scope_errors(contract: dict[str, Any], state: dict[str, Any]) -> list[str]:
    current = scan_files(contract)
    changes = changed_paths(state.get("files", {}), current)
    allowed = [pathlib.Path(item).resolve() for item in contract["allowed_roots"]]
    forbidden = [pathlib.Path(item).resolve() for item in contract.get("forbidden_roots", [])]
    errors = []
    for path in changes:
        if in_roots(path, forbidden):
            errors.append(f"forbidden scope changed: {path}")
        elif not in_roots(path, allowed):
            errors.append(f"unapproved scope changed: {path}")
    return errors


def receipt_signature(receipt: dict[str, Any], key: str) -> str:
    unsigned = {name: value for name, value in receipt.items() if name != "signature"}
    return hmac.new(bytes.fromhex(key), canonical(unsigned), hashlib.sha256).hexdigest()


def run_check(contract: dict[str, Any], state: dict[str, Any], evidence_path: pathlib.Path, command: list[str]) -> int:
    if not command:
        raise ValueError("runtime command is required after --")
    if state.get("contract_digest") != digest_object(contract):
        raise ValueError("snapshot does not belong to this contract")
    with tempfile.TemporaryDirectory(prefix="test-manager-observation-") as temp_dir:
        observation_path = pathlib.Path(temp_dir, "observation.json")
        environment = os.environ.copy()
        environment["TEST_MANAGER_OBSERVATION_PATH"] = str(observation_path)
        started_ns = time.time_ns()
        result = subprocess.run(command, capture_output=True, text=True, env=environment, check=False)
        finished_ns = time.time_ns()
        observation: object = None
        observation_error = None
        try:
            observation = read_json(observation_path)
        except (OSError, json.JSONDecodeError) as error:
            observation_error = str(error)
        receipt: dict[str, Any] = {
            "receipt_version": RECEIPT_VERSION,
            "contract_digest": digest_object(contract),
            "state_digest": digest_object({k: v for k, v in state.items() if k != "receipt_key"}),
            "command": command,
            "started_ns": started_ns,
            "finished_ns": finished_ns,
            "exit_code": result.returncode,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "observation": observation,
            "observation_error": observation_error,
        }
        receipt["signature"] = receipt_signature(receipt, state["receipt_key"])
        evidence = {"version": RECEIPT_VERSION, "receipts": []}
        if evidence_path.exists():
            existing = read_json(evidence_path)
            if isinstance(existing, dict) and isinstance(existing.get("receipts"), list):
                evidence = existing
        evidence["receipts"].append(receipt)
        write_json(evidence_path, evidence)
        if result.stdout:
            print(result.stdout, end="")
        if result.stderr:
            print(result.stderr, end="", file=sys.stderr)
        if observation_error:
            print(f"runtime observation missing or invalid: {observation_error}", file=sys.stderr)
            return 1 if result.returncode == 0 else result.returncode
        return result.returncode


def validate_receipt(receipt: object, contract: dict[str, Any], state: dict[str, Any]) -> list[str]:
    if not isinstance(receipt, dict):
        return ["receipt must be an object"]
    errors: list[str] = []
    signature = receipt.get("signature")
    if not text(signature) or not hmac.compare_digest(signature, receipt_signature(receipt, state["receipt_key"])):
        errors.append("receipt signature is missing or invalid; evidence must be produced by run")
    if receipt.get("receipt_version") != RECEIPT_VERSION:
        errors.append("receipt version is invalid; v6 requires recapture, never relabel archived v5 evidence")
    if receipt.get("contract_digest") != digest_object(contract):
        errors.append("receipt belongs to a different contract")
    if not isinstance(receipt.get("command"), list) or not receipt["command"] or not all(text(item) for item in receipt["command"]):
        errors.append("receipt command must be a non-empty argv array")
    if not isinstance(receipt.get("exit_code"), int):
        errors.append("receipt exit_code must be an integer")
    observation = receipt.get("observation")
    if not isinstance(observation, dict):
        errors.append("runtime command must emit an observation object")
        return errors
    for key in ("scenario_id", "runtime", "user_action", "observed_consumer"):
        if not text(observation.get(key)):
            errors.append(f"observation.{key} must be non-empty text")
    scenario = next(
        (item for item in contract["acceptance_scenarios"] if item["scenario_id"] == observation.get("scenario_id")),
        None,
    )
    if scenario is None:
        errors.append("observation scenario_id is not declared in acceptance_scenarios")
        scenario = {}
    if observation.get("requirement_ids") != scenario.get("requirement_ids"):
        errors.append("observation requirement_ids do not match the acceptance scenario")
    if contract.get("requires_behavior_preservation") is True:
        if observation.get("lifecycle_conditions") != scenario.get("lifecycle_conditions"):
            errors.append("observation lifecycle_conditions do not match the acceptance scenario")
    if observation.get("phase") not in {"unit", "pre_fix", "post_fix"}:
        errors.append("observation.phase must be unit, pre_fix, or post_fix")
    if observation.get("outcome") not in {"failure", "success"}:
        errors.append("observation.outcome must be failure or success")
    if observation.get("user_action") != scenario.get("user_action"):
        errors.append("observation user action does not match the acceptance scenario")
    errors.extend(validate_external_api_observation(contract, observation))
    errors.extend(validate_substitutions(observation))
    spec = scenario.get("input_lineage", {})
    if observation.get("original_input") != spec.get("original_input"):
        errors.append("observation original_input does not match its scenario")
    if not text(observation.get("entry_stage")):
        errors.append("observation.entry_stage must declare where execution actually started")
    if observation.get("phase") == "unit":
        errors.extend(validate_effect_observation(scenario, observation, read_artifact))
        if observation.get("observation_level") != "unit":
            errors.append("unit receipt must use observation_level: unit")
        if observation.get("observed_consumer") != contract["unit_observation"]:
            errors.append("unit observation does not match the contract")
        if receipt.get("command") not in contract["unit_test_commands"]:
            errors.append("unit receipt command was not declared in unit_test_commands")
        if receipt.get("exit_code") != 0 or observation.get("outcome") != "success":
            errors.append("unit receipt must exit zero with outcome success")
        return errors
    if observation.get("entry_stage") != "original_input":
        errors.append("runtime execution cannot begin at an intermediate stage")
    errors.extend(validate_trace(scenario, observation))
    if observation.get("observation_level") != "runtime":
        errors.append("pre_fix and post_fix receipts must use observation_level: runtime")
    if observation.get("observed_consumer") != scenario.get("authoritative_consumer"):
        errors.append("runtime observation consumer does not match the acceptance scenario")
    if observation.get("mocked") is not False:
        errors.append("runtime observation must explicitly be non-mocked")
    run_id = observation.get("run_id")
    action_started_ns = observation.get("action_started_ns")
    action_finished_ns = observation.get("action_finished_ns")
    if not text(run_id):
        errors.append("runtime observation requires a non-empty run_id")
    if not isinstance(action_started_ns, int) or isinstance(action_started_ns, bool) or action_started_ns <= 0:
        errors.append("runtime observation requires a positive action_started_ns")
        action_started_ns = 0
    if not isinstance(action_finished_ns, int) or isinstance(action_finished_ns, bool) or action_finished_ns < action_started_ns:
        errors.append("runtime observation requires action_finished_ns at or after action_started_ns")
    fidelity_contract = contract.get("input_fidelity", {})
    fidelity = observation.get("input_fidelity")
    if not isinstance(fidelity, dict):
        errors.append("runtime observation requires input_fidelity evidence")
    else:
        if fidelity.get("production_boundary") != fidelity_contract.get("production_boundary"):
            errors.append("runtime input boundary does not match the contract")
        if not text(fidelity.get("driver")):
            errors.append("runtime input_fidelity.driver must be non-empty text")
        if fidelity.get("matches_user_action") is not True:
            errors.append("runtime input must explicitly match the user action")
        if fidelity.get("bypassed_layers") != []:
            errors.append("runtime input evidence contains bypassed layers")
        if fidelity.get("synthetic_shortcuts") != []:
            errors.append("runtime input evidence contains synthetic shortcuts")
        observed = fidelity.get("observations")
        if not isinstance(observed, dict):
            errors.append("runtime input_fidelity.observations must be an object")
        else:
            for required in fidelity_contract.get("required_observations", []):
                if required not in observed or observed[required] is None:
                    errors.append(f"runtime input observation is missing: {required}")
    input_contract = contract.get("input_validation", {})
    if input_contract.get("applicable") is True:
        expected_surfaces = {
            surface["surface_id"]: surface
            for surface in input_contract.get("surfaces", [])
            if isinstance(surface, dict)
            and text(surface.get("surface_id"))
            and observation.get("scenario_id") in surface.get("scenario_ids", [])
        }
        input_evidence = observation.get("input_validation")
        if not isinstance(input_evidence, list):
            errors.append("runtime observation requires input_validation surface evidence")
            input_evidence = []
        evidence_by_id = {
            item.get("surface_id"): item
            for item in input_evidence
            if isinstance(item, dict) and text(item.get("surface_id"))
        }
        if len(evidence_by_id) != len(input_evidence):
            errors.append("runtime input_validation evidence contains duplicate or invalid surface ids")
        if set(evidence_by_id) != set(expected_surfaces):
            errors.append("runtime input_validation evidence must exactly match scenario-linked surfaces")
        for surface_id, surface in expected_surfaces.items():
            item = evidence_by_id.get(surface_id)
            field = f"runtime input surface {surface_id}"
            if not isinstance(item, dict):
                errors.append(f"{field} evidence is missing")
                continue
            if not text(item.get("input_driver")):
                errors.append(f"{field}.input_driver must be non-empty text")
            if item.get("synthetic_shortcuts") != []:
                errors.append(f"{field} contains synthetic shortcuts")
            observed_input = item.get("observations")
            if not isinstance(observed_input, dict):
                errors.append(f"{field}.observations must be an object")
                continue
            for required in surface.get("required_runtime_observations", []):
                if required not in observed_input or observed_input[required] is None:
                    errors.append(f"{field} is missing observation: {required}")
            if contains_raw_input(observed_input):
                errors.append(f"{field} exposes raw input text")
            value_after = observed_input.get("value_after_input")
            if not isinstance(value_after, dict) or not isinstance(value_after.get("length"), int) or isinstance(value_after.get("length"), bool) or value_after.get("length", -1) < 0 or not sha256_text(value_after.get("sha256")):
                errors.append(f"{field}.value_after_input must contain non-negative length and lowercase SHA-256")
            event_trace = observed_input.get("event_trace")
            if not isinstance(event_trace, list) or not event_trace or not all(text(event) for event in event_trace):
                errors.append(f"{field}.event_trace must be a non-empty text array")
            for step in ("first_input", "continuous_input"):
                step_value = observed_input.get(step)
                if not isinstance(step_value, dict) or not isinstance(step_value.get("length"), int) or isinstance(step_value.get("length"), bool) or step_value.get("length", 0) <= 0:
                    errors.append(f"{field}.{step} must record a positive resulting length")
            successful_post = observation.get("phase") == "post_fix" and observation.get("outcome") == "success"
            if successful_post:
                if observed_input.get("rendered_target") is not True:
                    errors.append(f"{field}.rendered_target must be true after the fix")
                if not text(observed_input.get("hit_test")) or not text(observed_input.get("focus")):
                    errors.append(f"{field} requires hit-test and focus evidence after the fix")
                root_count = observed_input.get("root_count_after_input")
                if not isinstance(root_count, int) or isinstance(root_count, bool) or root_count != 1:
                    errors.append(f"{field}.root_count_after_input must be exactly one after the fix")
                if observed_input.get("uncaught_errors") != []:
                    errors.append(f"{field}.uncaught_errors must be empty after the fix")
                if observed_input.get("event_value_snapshot") is not True:
                    errors.append(f"{field}.event_value_snapshot must prove synchronous capture before deferred work")
                if surface.get("ime_applicable") is True:
                    ime = observed_input.get("ime_composition")
                    if not isinstance(ime, dict) or ime.get("committed_once") is not True or not isinstance(ime.get("event_order"), list) or not ime.get("event_order"):
                        errors.append(f"{field}.ime_composition must prove one commit and event order")
                if surface.get("commit_required") is True:
                    commit = observed_input.get("commit_readback")
                    if not isinstance(commit, dict) or commit.get("matched") is not True:
                        errors.append(f"{field}.commit_readback must match the authoritative consumer")
                if surface.get("cancel_required") is True:
                    cancel = observed_input.get("cancel_readback")
                    if not isinstance(cancel, dict) or cancel.get("unchanged") is not True:
                        errors.append(f"{field}.cancel_readback must prove the consumer is unchanged")
                if surface.get("persistence_required") is True:
                    reload_readback = observed_input.get("reload_readback")
                    if not isinstance(reload_readback, dict) or reload_readback.get("matched") is not True:
                        errors.append(f"{field}.reload_readback must match after reload or restart")
    runtime = str(observation.get("runtime", "")).lower()
    if any(word in runtime for word in ("mock", "fake", "jsdom", "fixture-only", "production-shaped")):
        errors.append("mock or fake runtime is not authoritative runtime evidence")
    target = contract["runtime_target"]
    provenance = observation.get("runtime_provenance")
    if not isinstance(provenance, dict):
        errors.append("runtime observation requires runtime_provenance")
    else:
        if provenance.get("environment") != target.get("environment"):
            errors.append("runtime environment does not match the contract")
        if provenance.get("fixture") is not False:
            errors.append("runtime provenance must explicitly be non-fixture")
        origins = provenance.get("page_origins")
        if not isinstance(origins, list) or not all(text(item) for item in origins):
            errors.append("runtime_provenance.page_origins must be a text array")
            origins = []
        scenario_origin = scenario.get("origin")
        if scenario_origin and scenario_origin not in origins:
            errors.append("runtime origins do not contain the acceptance scenario origin")
        if not target.get("allow_fixture_origins"):
            fixture_prefixes = ("http://localhost", "https://localhost", "http://127.0.0.1", "https://127.0.0.1", "file:", "data:")
            if any(origin.lower().startswith(fixture_prefixes) for origin in origins):
                errors.append("localhost, loopback, file, or data origins cannot prove a live service")
        if target.get("environment") == "windows_user_chrome":
            if provenance.get("browser_name") != "Google Chrome" or provenance.get("os") != "Windows":
                errors.append("windows_user_chrome requires Windows Google Chrome")
            if provenance.get("profile_kind") != "actual-user" or provenance.get("headless") is not False:
                errors.append("windows_user_chrome requires the actual user profile in a headed browser")
            for key in ("extension_id", "extension_path", "extension_version"):
                if provenance.get(key) != target.get(key):
                    errors.append(f"runtime provenance {key} does not match the contract")
            try:
                preferences = read_json(pathlib.Path(target["secure_preferences_path"]))
                registration = preferences["extensions"]["settings"][target["extension_id"]]
            except (OSError, json.JSONDecodeError, KeyError, TypeError):
                errors.append("Chrome Secure Preferences does not contain the declared extension registration")
            else:
                registered_version = registration.get("service_worker_registration_info", {}).get("version")
                if registration.get("path") != target.get("windows_extension_path") or registered_version != target.get("extension_version"):
                    errors.append("Chrome registration path or service-worker version does not match the contract")
    output_root_value = target.get("authoritative_output_root")
    expected_output_count = scenario.get("expected_output_count", 0)
    outputs = observation.get("authoritative_outputs")
    if not isinstance(outputs, list):
        errors.append("runtime observation authoritative_outputs must be an array")
        outputs = []
    if observation.get("phase") == "post_fix" and len(outputs) != expected_output_count:
        errors.append("post_fix authoritative output count does not match the acceptance scenario")
    if output_root_value and outputs:
        output_root = pathlib.Path(output_root_value).resolve()
        for index, item in enumerate(outputs):
            if not isinstance(item, dict) or not text(item.get("path")) or not text(item.get("sha256")):
                errors.append(f"authoritative_outputs[{index}] must contain path and sha256")
                continue
            output_path = pathlib.Path(item["path"])
            if not output_path.is_absolute() or not in_roots(output_path, [output_root]):
                errors.append(f"authoritative_outputs[{index}] is outside authoritative_output_root")
                continue
            if not output_path.is_file():
                errors.append(f"authoritative_outputs[{index}] must still exist during validation")
                continue
            actual_size = output_path.stat().st_size
            if output_path.stat().st_mtime_ns < action_started_ns:
                errors.append(f"authoritative_outputs[{index}] predates the user action")
            if actual_size <= 0 or item.get("size") != actual_size or digest_file(output_path) != item["sha256"]:
                errors.append(f"authoritative_outputs[{index}] size or sha256 does not match actual bytes")
    loaded = observation.get("loaded_artifacts")
    if not isinstance(loaded, list) or not loaded:
        errors.append("observation.loaded_artifacts must be non-empty")
    else:
        for index, item in enumerate(loaded):
            if not isinstance(item, dict) or not text(item.get("path")) or not text(item.get("sha256")):
                errors.append(f"loaded_artifacts[{index}] must contain path and sha256")
                continue
            loaded_path = pathlib.Path(item["path"])
            if not loaded_path.is_file() or digest_file(loaded_path) != item["sha256"]:
                errors.append(f"loaded_artifacts[{index}] path or sha256 does not match actual bytes")
    logs = observation.get("runtime_logs")
    if not isinstance(logs, list) or not logs or not all(text(item) and pathlib.Path(item).is_file() for item in logs):
        errors.append("runtime observation requires existing runtime log files")
    log_sources = [pathlib.Path(item).resolve() for item in contract.get("runtime_log_sources", [])]
    if log_sources:
        records = observation.get("runtime_log_records")
        if not isinstance(records, list) or not records:
            errors.append("runtime observation requires runtime_log_records for declared log sources")
        else:
            recorded_paths: set[pathlib.Path] = set()
            for index, item in enumerate(records):
                if not isinstance(item, dict) or not text(item.get("path")):
                    errors.append(f"runtime_log_records[{index}] must contain path")
                    continue
                record_path = pathlib.Path(item["path"]).resolve()
                recorded_paths.add(record_path)
                if not record_path.is_file() or not in_roots(record_path, log_sources):
                    errors.append(f"runtime_log_records[{index}] is outside runtime_log_sources or missing")
                    continue
                actual_mtime = record_path.stat().st_mtime_ns
                if item.get("mtime_ns") != actual_mtime or actual_mtime < action_started_ns:
                    errors.append(f"runtime_log_records[{index}] is stale or has an invalid mtime")
                if item.get("run_id") != run_id:
                    errors.append(f"runtime_log_records[{index}] run_id does not match the runtime observation")
            changed_logs = changed_log_files(contract, state)
            if not changed_logs:
                errors.append("runtime log sources did not change after the snapshot")
            elif max(changed_logs, key=lambda path: path.stat().st_mtime_ns) not in recorded_paths:
                errors.append("runtime_log_records omit the newest rotated or changed log file")
    return errors


def validate_user_action_evidence(contract: dict[str, Any], post: list[tuple[dict[str, Any], dict[str, Any]]]) -> list[str]:
    execution = contract.get("user_execution")
    ledger_path = execution.get("action_ledger_path") if isinstance(execution, dict) else None
    if not ledger_path:
        return []  # Archived v6 contracts keep their original shape.
    try:
        ledger = read_json(pathlib.Path(ledger_path))
    except (OSError, json.JSONDecodeError) as error:
        return [f"user action ledger is missing or unreadable: {error}"]
    run_ids = {item.get("run_id") for _, item in post if text(item.get("run_id"))}
    errors = validate_ledger(ledger, contract, require_observed=True, runtime_run_ids=run_ids)
    if len(run_ids) != 1:
        errors.append("one user action must produce one correlated post_fix run_id across all scenarios")
    return errors


def validate_all(contract: object, state: object, evidence: object) -> list[str]:
    errors = validate_contract(contract)
    if errors or not isinstance(contract, dict):
        return errors
    if not isinstance(state, dict):
        return errors + ["snapshot root must be an object"]
    if state.get("contract_digest") != digest_object(contract) or not text(state.get("receipt_key")):
        errors.append("snapshot does not belong to this contract")
        return errors
    errors.extend(scope_errors(contract, state))
    if not isinstance(evidence, dict) or not isinstance(evidence.get("receipts"), list):
        return errors + ["evidence.receipts must be an array"]
    if evidence.get("version") != RECEIPT_VERSION:
        errors.append("evidence version must be 6; archived evidence requires recapture")
    receipts = evidence["receipts"]
    if not receipts:
        return errors + ["at least one runtime receipt is required"]
    observations = []
    for index, receipt in enumerate(receipts):
        for error in validate_receipt(receipt, contract, state):
            errors.append(f"receipts[{index}]: {error}")
        if isinstance(receipt, dict) and isinstance(receipt.get("observation"), dict):
            observations.append((receipt, receipt["observation"]))
    unit = [(receipt, item) for receipt, item in observations if item.get("phase") == "unit"]
    post = [(receipt, item) for receipt, item in observations if item.get("phase") == "post_fix"]
    pre = [(receipt, item) for receipt, item in observations if item.get("phase") == "pre_fix"]
    errors.extend(validate_user_action_evidence(contract, post))
    passed_unit_commands = {tuple(receipt.get("command", [])) for receipt, item in unit if receipt.get("exit_code") == 0 and item.get("outcome") == "success"}
    for command in contract["unit_test_commands"]:
        if tuple(command) not in passed_unit_commands:
            errors.append(f"declared unit command has no passing receipt: {command}")
    required_scenario_ids = {item["scenario_id"] for item in contract["acceptance_scenarios"]}
    unit_ids = {item.get("scenario_id") for receipt, item in unit if receipt.get("exit_code") == 0 and item.get("outcome") == "success"}
    successful_post_ids = {item.get("scenario_id") for receipt, item in post if receipt.get("exit_code") == 0 and item.get("outcome") == "success"}
    if not required_scenario_ids.issubset(unit_ids):
        errors.append("every acceptance scenario requires a passing unit receipt")
    if not required_scenario_ids.issubset(successful_post_ids):
        errors.append("every acceptance scenario requires a successful post_fix runtime receipt")
    if contract.get("requires_regression", True):
        if not pre:
            errors.append("a pre_fix runtime failure receipt is required")
        if not post:
            errors.append("a post_fix runtime success receipt is required")
        pre_ids = {item.get("scenario_id") for _, item in pre if item.get("outcome") == "failure"}
        post_ids = {item.get("scenario_id") for receipt, item in post if item.get("outcome") == "success" and receipt.get("exit_code") == 0}
        if not required_scenario_ids.issubset(pre_ids.intersection(post_ids)):
            errors.append("every acceptance scenario must fail before the fix and succeed after it")
    elif not any(item.get("outcome") == "success" and receipt.get("exit_code") == 0 for receipt, item in post):
        errors.append("a successful post_fix runtime receipt is required")
    delivered_by_path = {str(pathlib.Path(item)): digest_file(pathlib.Path(item)) for item in contract["delivered_artifacts"]}
    for _, observation in post:
        loaded = {item.get("path"): item.get("sha256") for item in observation.get("loaded_artifacts", []) if isinstance(item, dict)}
        if any(loaded.get(path) != checksum for path, checksum in delivered_by_path.items()):
            errors.append(f"scenario {observation.get('scenario_id')} does not cover every delivered artifact by path and hash")
    delivered_hashes = {digest_file(pathlib.Path(item)) for item in contract["delivered_artifacts"]}
    loaded_hashes = {
        artifact.get("sha256")
        for _, item in post
        for artifact in item.get("loaded_artifacts", [])
        if isinstance(artifact, dict)
    }
    if not delivered_hashes.issubset(loaded_hashes):
        errors.append("runtime-loaded artifact hashes do not cover every delivered artifact")
    if any(item.get("uncaught_errors") for _, item in post):
        errors.append("post_fix runtime observation contains uncaught errors")
    if any(item.get("unobserved_layers") for _, item in post):
        errors.append("post_fix runtime observation contains unobserved layers")
    return errors


def self_test() -> int:
    results: list[dict[str, object]] = []
    workspace = pathlib.Path(__file__).resolve().parents[1] / "test" / "tmp"
    workspace.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="test-manager-self-test-", dir=workspace) as temp_text:
        root = pathlib.Path(temp_text)
        allowed = root / "allowed"
        forbidden = root / "forbidden"
        allowed.mkdir()
        forbidden.mkdir()
        artifact = allowed / "artifact.bin"
        artifact.write_bytes(b"delivered")
        runtime_log = root / "runtime.log"
        runtime_log.write_text("real runtime boundary\n", encoding="utf-8")
        requirement_log = root / "Input.md"
        requirement_one = "- restore ChatGPT download"
        requirement_two = "- restore Gemini download"
        requirement_log.write_text(f"{requirement_one}\n{requirement_two}\n", encoding="utf-8")
        output_root = root / "downloads"
        output_root.mkdir()
        downloaded = output_root / "result.bin"
        downloaded.write_bytes(b"downloaded")
        extension_id = "abcdefghijklmnopabcdefghijklmnop"
        secure_preferences = root / "Secure Preferences"
        write_json(secure_preferences, {"extensions": {"settings": {extension_id: {
            "path": "D:\\code\\extension",
            "service_worker_registration_info": {"version": "1.2.3"},
        }}}})
        runner = allowed / "runtime_runner.py"
        contract = {
            "task_id": "incident-regression",
            "workflow": "extension popup",
            "user_action": "click gallery button",
            "expected_outcome": "gallery opens",
            "unit_observation": "loader contract state",
            "unit_test_commands": [
                [sys.executable, str(runner), "unit", "success", "chatgpt-download"],
                [sys.executable, str(runner), "unit", "success", "gemini-download"],
            ],
            "authoritative_consumer": "browser DOM",
            "requirement_coverage": [
                {"id": "req-chatgpt", "incident_key": "ai-download", "source_path": str(requirement_log), "source_text": requirement_one, "source_sha256": digest_text(requirement_one)},
                {"id": "req-gemini", "incident_key": "ai-download", "source_path": str(requirement_log), "source_text": requirement_two, "source_sha256": digest_text(requirement_two)},
            ],
            "acceptance_scenarios": [
                {"scenario_id": "chatgpt-download", "requirement_ids": ["req-chatgpt"], "external_api_contract_ids": [], "origin": "https://chatgpt.com", "user_action": "click gallery button", "authoritative_consumer": "browser DOM", "expected_output_count": 1, "coverage_kind": "preservation", "lifecycle_conditions": ["external_bridge", "tab_inactive", "document_hidden"]},
                {"scenario_id": "gemini-download", "requirement_ids": ["req-gemini"], "external_api_contract_ids": [], "origin": "https://gemini.google.com", "user_action": "click gallery button", "authoritative_consumer": "browser DOM", "expected_output_count": 1, "coverage_kind": "preservation", "lifecycle_conditions": ["external_bridge", "tab_inactive", "document_hidden"]},
            ],
            "requires_behavior_preservation": True,
            "behavior_change_analysis": {
                "baseline_ref": "git:buggy-readiness-baseline",
                "changed_contract_points": [{
                    "id": "generated-image-readiness",
                    "kind": "readiness_guard",
                    "before_contract": "authoritative generated URLs can reach loaded and deferred download branches",
                    "after_contract": "loaded images and authorized deferred URLs remain reachable",
                    "previously_accepted_states": ["active-loaded-image", "inactive-deferred-url"],
                    "preserved_states": ["active-loaded-image", "inactive-deferred-url"],
                    "intentionally_removed_states": [],
                    "downstream_branches_before": ["loaded-image-download", "deferred-url-fetch"],
                    "downstream_branches_preserved": ["loaded-image-download", "deferred-url-fetch"],
                    "intentionally_removed_branches": [],
                    "preservation_scenario_ids": ["chatgpt-download", "gemini-download"],
                    "removal_requirement_ids": []
                }]
            },
            "incident_history": [
                {"incident_key": "ai-download", "source_path": str(requirement_log), "source_text": requirement_one, "source_sha256": digest_text(requirement_one), "prior_evidence_gap": "fixture only"},
                {"incident_key": "ai-download", "source_path": str(requirement_log), "source_text": requirement_two, "source_sha256": digest_text(requirement_two), "prior_evidence_gap": "one target omitted"},
            ],
            "runtime_log_sources": [],
            "runtime_target": {
                "environment": "windows_user_chrome",
                "required_origins": ["https://chatgpt.com", "https://gemini.google.com"],
                "allow_fixture_origins": False,
                "authoritative_output_root": str(output_root),
                "extension_id": extension_id,
                "extension_path": str(allowed),
                "windows_extension_path": "D:\\code\\extension",
                "extension_version": "1.2.3",
                "secure_preferences_path": str(secure_preferences),
            },
            "input_fidelity": {
                "production_boundary": "rendered pointer target then keyboard channel",
                "required_observations": ["hit_test", "focus", "event_trace"],
                "forbidden_shortcuts": ["property assignment", "dispatchEvent", "direct handler call"],
            },
            "input_validation": {
                "applicable": True,
                "reason": "The acceptance scenarios type into an editable browser surface.",
                "surfaces": [{
                    "surface_id": "gallery-prompt",
                    "input_kind": "textarea",
                    "scenario_ids": ["chatgpt-download", "gemini-download"],
                    "ime_applicable": True,
                    "commit_required": True,
                    "cancel_required": True,
                    "persistence_required": True,
                    "authoritative_consumer": "browser DOM",
                    "required_runtime_observations": [
                        "rendered_target", "hit_test", "focus", "first_input", "continuous_input",
                        "event_trace", "event_value_snapshot", "value_after_input", "root_count_after_input", "uncaught_errors",
                        "ime_composition", "commit_readback", "cancel_readback", "reload_readback",
                    ],
                }],
            },
            "external_api_validation": {
                "applicable": False,
                "reason": "These validator fixtures exercise browser download boundaries without an external HTTP API.",
                "contracts": [],
            },
            "repeated_report_count": 2,
            "runtime_diagnosis": {"source": "runtime_log", "failing_boundary": "content loader", "specific_failing_detail": "receiver missing after reload", "log_artifacts": [str(runtime_log)]},
            "allowed_roots": [str(allowed)],
            "watch_roots": [str(root)],
            "forbidden_roots": [str(forbidden)],
            "adjacent_workflows": ["unrelated bridge"],
            "delivered_artifacts": [str(artifact)],
            "requires_regression": True,
            "user_execution": {
                "manual_run_required": True,
                "max_user_runs": 1,
                "scenario_ids": ["chatgpt-download", "gemini-download"],
                "production_path_complete": True,
                "continues_independent_targets": True,
                "micro_stage_reassignment": False,
                "post_run_debug_source": "captured_logs_and_automation",
            },
        }
        from lineage_test_fixtures import scenario_spec, attach_fixture
        for scenario in contract["acceptance_scenarios"]:
            scenario["input_lineage"] = scenario_spec(scenario, pathlib.Path(__file__).resolve())
            scenario["input_lineage"]["artifact_order"] = [{"path": str(artifact), "sha256": digest_file(artifact)}]
        state = create_snapshot(contract)
        key = state["receipt_key"]

        def receipt(phase: str, outcome: str, exit_code: int, scenario_id: str) -> dict[str, Any]:
            is_unit = phase == "unit"
            scenario = next(item for item in contract["acceptance_scenarios"] if item["scenario_id"] == scenario_id)
            action_started_ns = min(downloaded.stat().st_mtime_ns, runtime_log.stat().st_mtime_ns) - 1
            value: dict[str, Any] = {
                "receipt_version": RECEIPT_VERSION,
                "contract_digest": digest_object(contract),
                "state_digest": digest_object({k: v for k, v in state.items() if k != "receipt_key"}),
                "command": [sys.executable, str(runner), "unit", "success", scenario_id] if is_unit else ["runtime-check", scenario_id],
                "started_ns": 1,
                "finished_ns": 2,
                "exit_code": exit_code,
                "stdout": "",
                "stderr": "",
                "observation_error": None,
                "observation": {
                    "scenario_id": scenario_id,
                    "requirement_ids": scenario["requirement_ids"],
                    "lifecycle_conditions": scenario["lifecycle_conditions"],
                    "phase": phase,
                    "outcome": outcome,
                    "observation_level": "unit" if is_unit else "runtime",
                    "runtime": "Google Chrome extension runtime",
                    "user_action": scenario["user_action"],
                    "observed_consumer": contract["unit_observation"] if is_unit else scenario["authoritative_consumer"],
                    "mocked": is_unit,
                    "run_id": None if is_unit else f"run-{scenario_id}",
                    "action_started_ns": None if is_unit else action_started_ns,
                    "action_finished_ns": None if is_unit else action_started_ns + 1,
                    "runtime_provenance": None if is_unit else {
                        "environment": "windows_user_chrome", "fixture": False,
                        "page_origins": [scenario["origin"]], "browser_name": "Google Chrome",
                        "os": "Windows", "profile_kind": "actual-user", "headless": False,
                        "extension_id": extension_id, "extension_path": str(allowed), "extension_version": "1.2.3",
                    },
                    "authoritative_outputs": [] if is_unit else [{"path": str(downloaded), "size": downloaded.stat().st_size, "sha256": digest_file(downloaded)}],
                    "input_fidelity": None if is_unit else {
                        "production_boundary": contract["input_fidelity"]["production_boundary"],
                        "driver": "browser pointer and keyboard automation",
                        "matches_user_action": True,
                        "bypassed_layers": [],
                        "synthetic_shortcuts": [],
                        "observations": {"hit_test": "target", "focus": "target", "event_trace": ["capture", "target", "bubble"]},
                    },
                    "input_validation": None if is_unit else [{
                        "surface_id": "gallery-prompt",
                        "input_driver": "browser keyboard input",
                        "synthetic_shortcuts": [],
                        "observations": {
                            "rendered_target": True,
                            "hit_test": "textarea",
                            "focus": "textarea",
                            "first_input": {"length": 1},
                            "continuous_input": {"length": 4},
                            "event_trace": ["keydown", "beforeinput", "input", "keyup"],
                            "event_value_snapshot": True,
                            "value_after_input": {"length": 4, "sha256": digest_text("test")},
                            "root_count_after_input": 1,
                            "uncaught_errors": [],
                            "ime_composition": {"committed_once": True, "event_order": ["compositionstart", "compositionupdate", "input", "compositionend"]},
                            "commit_readback": {"matched": True},
                            "cancel_readback": {"unchanged": True},
                            "reload_readback": {"matched": True},
                        },
                    }],
                    "loaded_artifacts": [{"path": str(artifact), "sha256": digest_file(artifact)}],
                    "runtime_logs": [str(runtime_log)],
                    "uncaught_errors": [],
                    "unobserved_layers": [],
                },
            }
            attach_fixture(allowed / "test" / "tmp", scenario, value["observation"])
            value["signature"] = receipt_signature(value, key)
            return value

        good = {"version": RECEIPT_VERSION, "receipts": [
            receipt("unit", "success", 0, "chatgpt-download"),
            receipt("pre_fix", "failure", 1, "chatgpt-download"),
            receipt("post_fix", "success", 0, "chatgpt-download"),
            receipt("unit", "success", 0, "gemini-download"),
            receipt("pre_fix", "failure", 1, "gemini-download"),
            receipt("post_fix", "success", 0, "gemini-download"),
        ]}

        def case(name: str, accepted: bool, candidate_contract: object = contract, candidate_state: object = state, candidate_evidence: object = good) -> None:
            errors = validate_all(candidate_contract, candidate_state, candidate_evidence)
            actual = not errors
            results.append({"name": name, "passed": actual is accepted, **({"errors": errors} if actual is not accepted else {})})

        def contract_case(name: str, accepted: bool, candidate_contract: object) -> None:
            actual = not validate_contract(candidate_contract)
            results.append({"name": name, "passed": actual is accepted})

        case("synthetic v6 schema fixtures accepted (validator unit test only)", True)
        missing_api_decision = copy.deepcopy(contract)
        missing_api_decision.pop("external_api_validation")
        contract_case("missing external API applicability decision rejected", False, missing_api_decision)

        api_schema = [
            {"path": "$.status", "type": "string", "required": True, "safe_value": "OK"},
            {"path": "$.authenticated", "type": "boolean", "required": True, "safe_value": True},
        ]
        api_capture = allowed / "obsidian-status-capture.json"
        write_json(api_capture, {
            "contract_id": "obsidian-status",
            "system": "Obsidian Local REST API",
            "operation": "read server status",
            "method": "GET",
            "endpoint_pattern": "https://127.0.0.1:27124/",
            "authoritative_environment": "windows_host",
            "source_kind": "live_read_only",
            "response_schema": api_schema,
            "contains_secrets": False,
        })
        api_fixture = allowed / "obsidian-status-fixture.json"
        write_json(api_fixture, {"status": "OK", "authenticated": True})
        api_consumer = allowed / "obsidian-consumer.js"
        api_consumer.write_text("const connected = body.status === 'OK' && body.authenticated === true;\n", encoding="utf-8")
        mutant_receipt = allowed / "obsidian-status-mutant.json"
        write_json(mutant_receipt, {
            "contract_id": "obsidian-status", "mutation_kind": "field_name",
            "from_path": "$.status", "to_path": "$.ok",
            "rejected": True, "production_consumer_executed": True, "exit_code": 1,
        })
        type_mutant_receipt = allowed / "obsidian-type-mutant.json"
        write_json(type_mutant_receipt, {
            "contract_id": "obsidian-status", "mutation_kind": "field_type",
            "path": "$.authenticated", "from_type": "boolean", "to_type": "string",
            "rejected": True, "production_consumer_executed": True, "exit_code": 1,
        })
        api_contract = copy.deepcopy(contract)
        api_contract["acceptance_scenarios"][0]["external_api_contract_ids"] = ["obsidian-status"]
        api_contract["runtime_target"]["external_api_environments"] = ["windows_host"]
        api_contract["external_api_validation"] = {
            "applicable": True,
            "reason": "The connection check consumes the live Obsidian status response.",
            "contracts": [{
                "contract_id": "obsidian-status",
                "scenario_ids": ["chatgpt-download"],
                "system": "Obsidian Local REST API",
                "operation": "read server status",
                "method": "GET",
                "endpoint_pattern": "https://127.0.0.1:27124/",
                "authoritative_environment": "windows_host",
                "response_schema": api_schema,
                "safe_probe": {
                    "available": True, "source_kind": "live_read_only",
                    "path": str(api_capture), "sha256": digest_file(api_capture),
                },
                "fixtures": [{
                    "path": str(api_fixture), "sha256": digest_file(api_fixture),
                    "source_capture_sha256": digest_file(api_capture), "response_schema": api_schema,
                }],
                "consumer": {
                    "path": str(api_consumer), "sha256": digest_file(api_consumer),
                    "expression": "body.status === 'OK' && body.authenticated === true",
                    "required_fields": ["$.status", "$.authenticated"],
                },
                "mutation_tests": [
                    {"path": str(mutant_receipt), "sha256": digest_file(mutant_receipt)},
                    {"path": str(type_mutant_receipt), "sha256": digest_file(type_mutant_receipt)},
                ],
            }],
        }
        contract_case("capture-derived external API consumer contract accepted", True, api_contract)
        wrong_fixture_schema = copy.deepcopy(api_contract)
        wrong_fixture_schema["external_api_validation"]["contracts"][0]["fixtures"][0]["response_schema"][0]["path"] = "$.ok"
        contract_case("status versus ok fixture drift rejected", False, wrong_fixture_schema)
        wrong_environment = copy.deepcopy(api_contract)
        wrong_environment["runtime_target"]["external_api_environments"] = ["wsl"]
        contract_case("WSL versus Windows API environment mismatch rejected", False, wrong_environment)
        wrong_type = copy.deepcopy(api_contract)
        wrong_type["external_api_validation"]["contracts"][0]["fixtures"][0]["response_schema"][1]["type"] = "string"
        contract_case("external API fixture type mismatch rejected", False, wrong_type)
        surviving_mutant = allowed / "surviving-api-mutant.json"
        write_json(surviving_mutant, {
            "contract_id": "obsidian-status", "mutation_kind": "field_name",
            "from_path": "$.status", "to_path": "$.ok",
            "rejected": False, "production_consumer_executed": True, "exit_code": 0,
        })
        surviving_mutant_contract = copy.deepcopy(api_contract)
        surviving_mutant_contract["external_api_validation"]["contracts"][0]["mutation_tests"][0] = {
            "path": str(surviving_mutant), "sha256": digest_file(surviving_mutant),
        }
        contract_case("surviving external API field mutant rejected", False, surviving_mutant_contract)
        secret_capture = allowed / "secret-api-capture.json"
        secret_capture_value = read_json(api_capture)
        secret_capture_value["authorization"] = "Bearer secret"
        write_json(secret_capture, secret_capture_value)
        secret_capture_contract = copy.deepcopy(api_contract)
        secret_capture_contract["external_api_validation"]["contracts"][0]["safe_probe"].update({
            "path": str(secret_capture), "sha256": digest_file(secret_capture),
        })
        secret_capture_contract["external_api_validation"]["contracts"][0]["fixtures"][0]["source_capture_sha256"] = digest_file(secret_capture)
        contract_case("credential-bearing API capture rejected", False, secret_capture_contract)

        api_unit_observation = {
            "scenario_id": "chatgpt-download", "phase": "unit", "outcome": "success",
            "external_api_validation": [{
                "contract_id": "obsidian-status", "authoritative_environment": "windows_host",
                "capture_sha256": digest_file(api_capture), "response_schema": api_schema,
                "production_consumer_executed": True, "boundary_source": "capture_fixture",
                "fixture_artifacts": [{"path": str(api_fixture), "sha256": digest_file(api_fixture)}],
                "mutations_rejected": ["field_name", "field_type"],
            }],
        }
        results.append({"name": "capture fixture executes production API consumer", "passed": not validate_external_api_observation(api_contract, api_unit_observation)})
        mock_only_observation = copy.deepcopy(api_unit_observation)
        mock_only_observation["external_api_validation"][0]["production_consumer_executed"] = False
        results.append({"name": "mock-only API test rejected", "passed": bool(validate_external_api_observation(api_contract, mock_only_observation))})
        api_runtime_observation = {
            "scenario_id": "chatgpt-download", "phase": "post_fix", "outcome": "success",
            "external_api_validation": [{
                "contract_id": "obsidian-status", "authoritative_environment": "windows_host",
                "capture_sha256": digest_file(api_capture), "response_schema": api_schema,
                "production_consumer_executed": True, "boundary_source": "live_response",
                "mocked": False, "consumer_readback": True,
            }],
        }
        results.append({"name": "live API response reaches authoritative consumer", "passed": not validate_external_api_observation(api_contract, api_runtime_observation)})
        unobserved_consumer = copy.deepcopy(api_runtime_observation)
        unobserved_consumer["external_api_validation"][0]["consumer_readback"] = False
        results.append({"name": "unobserved final API consumer rejected", "passed": bool(validate_external_api_observation(api_contract, unobserved_consumer))})
        # Re-sign mutations so lineage rejection, not the receipt signature, is tested.
        def lineage_case(name, mutate, expected_error):
            candidate = copy.deepcopy(good)
            item = candidate["receipts"][2]
            obs = item["observation"]
            trace = read_json(pathlib.Path(obs["input_lineage"]["trace"]["path"]))
            mutate(obs, trace)
            trace_path = allowed / "test" / "tmp" / "lineage-negatives" / (name + ".json")
            trace_path.parent.mkdir(exist_ok=True)
            write_json(trace_path, trace)
            obs["input_lineage"]["trace"] = {"path": str(trace_path), "sha256": digest_file(trace_path)}
            item["signature"] = receipt_signature(item, key)
            errors = validate_all(contract, state, candidate)
            results.append({"name": name, "passed": any(expected_error in error for error in errors) and not any("signature" in error or "scope changed" in error for error in errors)})

        lineage_case("middle-entry", lambda o, t: o.update(entry_stage="handler"), "intermediate stage")
        lineage_case("missing-original", lambda o, t: t.pop("original_input"), "original input")
        lineage_case("injected-namespace", lambda o, t: t.update(substitutions=[{"component": "namespace", "stage": "handler", "reason": "fake"}]), "substitutions")
        lineage_case("skipped-handler", lambda o, t: t["steps"].pop(2), "production path")
        lineage_case("wrong-run", lambda o, t: t.update(run_id="another-run"), "run_id")
        lineage_case("missing-loader-capture", lambda o, t: t.pop("loader_capture"), "loader_capture")
        lineage_case("missing-boundary-capture", lambda o, t: t["steps"][0].pop("capture"), "capture")
        lineage_case("broken-input-chain", lambda o, t: t["steps"][1].update(input_sha256="0" * 64), "transformation chain")
        def inject_module(obs, trace):
            trace["loaded_artifacts"].append({"path": str(pathlib.Path(__file__).resolve()), "sha256": digest_file(pathlib.Path(__file__))})
            obs["loaded_artifacts"] = trace["loaded_artifacts"]
        lineage_case("test-added-module", inject_module, "frozen production loader order")
        def force_success(obs, trace):
            capture = read_json(pathlib.Path(trace["steps"][-1]["capture"]["path"]))
            capture["observation"] = {"success": True}
            path = allowed / "test" / "tmp" / "forced-success.json"
            write_json(path, capture)
            trace["steps"][-1]["capture"] = {"path": str(path), "sha256": digest_file(path)}
        lineage_case("forced-success-flag", force_success, "boundary value fingerprint")
        bad = copy.deepcopy(good)
        bad["version"] = 5
        case("v5 requires recapture", False, candidate_evidence=bad)
        bad = copy.deepcopy(good)
        bad["receipts"][0]["observation"]["substitutions"] = []
        bad["receipts"][0]["signature"] = receipt_signature(bad["receipts"][0], key)
        case("unit mock inventory required", False, candidate_evidence=bad)
        bad_contract = copy.deepcopy(contract)
        bad_contract["acceptance_scenarios"][0]["input_lineage"]["original_input"]["source_kind"] = []
        contract_case("malformed source fails closed", False, bad_contract)
        runner.write_text(
            "import hashlib, json, os, pathlib, sys\n"
            f"artifact = pathlib.Path({str(artifact)!r})\n"
            f"runtime_log = pathlib.Path({str(runtime_log)!r})\n"
            f"downloaded = pathlib.Path({str(downloaded)!r})\n"
            "phase, outcome, scenario_id = sys.argv[1:4]\n"
            "is_unit = phase == 'unit'\n"
            "origin = 'https://chatgpt.com' if scenario_id == 'chatgpt-download' else 'https://gemini.google.com'\n"
            "requirement_id = 'req-chatgpt' if scenario_id == 'chatgpt-download' else 'req-gemini'\n"
            "action_started_ns = min(downloaded.stat().st_mtime_ns, runtime_log.stat().st_mtime_ns) - 1\n"
            "observation = {\n"
            "  'scenario_id': scenario_id, 'requirement_ids': [requirement_id], 'lifecycle_conditions': ['external_bridge', 'tab_inactive', 'document_hidden'], 'phase': phase, 'outcome': outcome,\n"
            "  'observation_level': 'unit' if is_unit else 'runtime',\n"
            "  'runtime': 'Google Chrome extension runtime', 'user_action': 'click gallery button',\n"
            "  'observed_consumer': 'loader contract state' if is_unit else 'browser DOM', 'mocked': is_unit,\n"
            "  'run_id': None if is_unit else 'run-' + scenario_id, 'action_started_ns': None if is_unit else action_started_ns, 'action_finished_ns': None if is_unit else action_started_ns + 1,\n"
            f"  'runtime_provenance': None if is_unit else {{'environment': 'windows_user_chrome', 'fixture': False, 'page_origins': [origin], 'browser_name': 'Google Chrome', 'os': 'Windows', 'profile_kind': 'actual-user', 'headless': False, 'extension_id': {extension_id!r}, 'extension_path': {str(allowed)!r}, 'extension_version': '1.2.3'}},\n"
            "  'authoritative_outputs': [] if is_unit else [{'path': str(downloaded), 'size': downloaded.stat().st_size, 'sha256': hashlib.sha256(downloaded.read_bytes()).hexdigest()}],\n"
            "  'input_fidelity': None if is_unit else {'production_boundary': 'rendered pointer target then keyboard channel', 'driver': 'browser pointer and keyboard automation', 'matches_user_action': True, 'bypassed_layers': [], 'synthetic_shortcuts': [], 'observations': {'hit_test': 'target', 'focus': 'target', 'event_trace': ['capture', 'target', 'bubble']}},\n"
            "  'input_validation': None if is_unit else [{'surface_id': 'gallery-prompt', 'input_driver': 'browser keyboard input', 'synthetic_shortcuts': [], 'observations': {'rendered_target': True, 'hit_test': 'textarea', 'focus': 'textarea', 'first_input': {'length': 1}, 'continuous_input': {'length': 4}, 'event_trace': ['keydown', 'beforeinput', 'input', 'keyup'], 'event_value_snapshot': True, 'value_after_input': {'length': 4, 'sha256': hashlib.sha256(b'test').hexdigest()}, 'root_count_after_input': 1, 'uncaught_errors': [], 'ime_composition': {'committed_once': True, 'event_order': ['compositionstart', 'compositionupdate', 'input', 'compositionend']}, 'commit_readback': {'matched': True}, 'cancel_readback': {'unchanged': True}, 'reload_readback': {'matched': True}}}],\n"
            "  'loaded_artifacts': [{'path': str(artifact), 'sha256': hashlib.sha256(artifact.read_bytes()).hexdigest()}],\n"
            "  'runtime_logs': [str(runtime_log)], 'uncaught_errors': [], 'unobserved_layers': []}\n"
            f"sys.path.insert(0, {str(pathlib.Path(__file__).resolve().parent)!r})\n"
            "from lineage_test_fixtures import attach_fixture\n"
            f"scenarios = {contract['acceptance_scenarios']!r}\n"
            f"attach_fixture({str(allowed / 'test' / 'tmp')!r}, next(s for s in scenarios if s['scenario_id'] == scenario_id), observation)\n"
            "pathlib.Path(os.environ['TEST_MANAGER_OBSERVATION_PATH']).write_text(json.dumps(observation), encoding='utf-8')\n"
            "raise SystemExit(1 if phase == 'pre_fix' else 0)\n",
            encoding="utf-8",
        )
        runtime_evidence_path = allowed / "runner-evidence.json"
        for scenario_id in ("chatgpt-download", "gemini-download"):
            run_check(contract, state, runtime_evidence_path, [sys.executable, str(runner), "unit", "success", scenario_id])
            run_check(contract, state, runtime_evidence_path, [sys.executable, str(runner), "pre_fix", "failure", scenario_id])
            run_check(contract, state, runtime_evidence_path, [sys.executable, str(runner), "post_fix", "success", scenario_id])
        case("validator executes and signs fixture commands (not product evidence)", True, candidate_evidence=read_json(runtime_evidence_path))
        missing_input_contract = dict(contract)
        missing_input_contract.pop("input_validation")
        contract_case("missing input applicability decision rejected", False, missing_input_contract)
        empty_input_contract = json.loads(json.dumps(contract))
        empty_input_contract["input_validation"]["surfaces"] = []
        contract_case("applicable input contract without surfaces rejected", False, empty_input_contract)
        missing_input_observation_contract = json.loads(json.dumps(contract))
        missing_input_observation_contract["input_validation"]["surfaces"][0]["required_runtime_observations"].remove("event_value_snapshot")
        contract_case("input surface omitting event snapshot evidence rejected", False, missing_input_observation_contract)

        def mutated_post_input(name: str, mutate: Any) -> None:
            candidate_evidence = json.loads(json.dumps(good))
            post = candidate_evidence["receipts"][2]
            observations = post["observation"]["input_validation"][0]["observations"]
            mutate(observations)
            post["signature"] = receipt_signature(post, key)
            case(name, False, candidate_evidence=candidate_evidence)

        mutated_post_input("raw input text in evidence rejected", lambda observations: observations.update({"raw_value": "secret input"}))
        mutated_post_input("first input root disappearance rejected", lambda observations: observations.update({"root_count_after_input": 0}))
        mutated_post_input("uncaught input exception rejected", lambda observations: observations.update({"uncaught_errors": ["released event target"]}))
        mutated_post_input("deferred event read without synchronous snapshot rejected", lambda observations: observations.update({"event_value_snapshot": False}))
        mutated_post_input("IME duplicate or missing commit rejected", lambda observations: observations.update({"ime_composition": {"committed_once": False, "event_order": ["compositionstart", "compositionend"]}}))
        mutated_post_input("commit consumer mismatch rejected", lambda observations: observations.update({"commit_readback": {"matched": False}}))
        mutated_post_input("cancel mutation rejected", lambda observations: observations.update({"cancel_readback": {"unchanged": False}}))
        mutated_post_input("persistence readback mismatch rejected", lambda observations: observations.update({"reload_readback": {"matched": False}}))

        released_event: dict[str, Any] = {"current_target": {"value": "first character"}}
        delayed_read = lambda: released_event["current_target"]["value"]
        released_event["current_target"] = None
        try:
            delayed_read()
            released_target_failed = False
        except (TypeError, KeyError):
            released_target_failed = True
        results.append({"name": "released event target mutant fails before deferred updater", "passed": released_target_failed})
        case("one of two required targets rejected", False, candidate_evidence={"version": RECEIPT_VERSION, "receipts": good["receipts"][:3]})
        omitted_requirement_contract = {
            **contract,
            "acceptance_scenarios": contract["acceptance_scenarios"][:1],
            "runtime_target": {**contract["runtime_target"], "required_origins": ["https://chatgpt.com"]},
        }
        contract_case("unmapped requirement rejected", False, omitted_requirement_contract)
        origin_mismatch_contract = {**contract, "runtime_target": {**contract["runtime_target"], "required_origins": ["https://chatgpt.com"]}}
        contract_case("underdeclared origin set rejected", False, origin_mismatch_contract)
        repeated_user_runs_contract = copy.deepcopy(contract)
        repeated_user_runs_contract["user_execution"]["max_user_runs"] = 2
        contract_case("more than one user runtime execution rejected", False, repeated_user_runs_contract)
        micro_stage_contract = copy.deepcopy(contract)
        micro_stage_contract["user_execution"]["micro_stage_reassignment"] = True
        contract_case("micro-stage Test reassignment rejected", False, micro_stage_contract)
        fail_fast_contract = copy.deepcopy(contract)
        fail_fast_contract["user_execution"]["continues_independent_targets"] = False
        contract_case("fail-fast across independent targets rejected", False, fail_fast_contract)
        split_action_contract = copy.deepcopy(contract)
        split_action_contract["acceptance_scenarios"][1]["user_action"] = "click a second site Test"
        contract_case("different user actions across one-run scenarios rejected", False, split_action_contract)
        missing_behavior_analysis = dict(contract)
        missing_behavior_analysis.pop("behavior_change_analysis")
        contract_case("shared guard change without behavior analysis rejected", False, missing_behavior_analysis)
        unreachable_fallback_contract = json.loads(json.dumps(contract))
        unreachable_point = unreachable_fallback_contract["behavior_change_analysis"]["changed_contract_points"][0]
        unreachable_point["downstream_branches_preserved"] = ["loaded-image-download"]
        contract_case("unreachable preserved fallback rejected", False, unreachable_fallback_contract)
        unapproved_removal_contract = json.loads(json.dumps(contract))
        removal_point = unapproved_removal_contract["behavior_change_analysis"]["changed_contract_points"][0]
        removal_point["preserved_states"] = ["active-loaded-image"]
        removal_point["intentionally_removed_states"] = ["inactive-deferred-url"]
        removal_point["downstream_branches_preserved"] = ["loaded-image-download"]
        removal_point["intentionally_removed_branches"] = ["deferred-url-fetch"]
        contract_case("behavior removal without requirement authorization rejected", False, unapproved_removal_contract)
        synthetic_post = {**good["receipts"][2], "observation": {**good["receipts"][2]["observation"], "input_fidelity": {**good["receipts"][2]["observation"]["input_fidelity"], "synthetic_shortcuts": ["input.value assignment"]}}}
        synthetic_post["signature"] = receipt_signature(synthetic_post, key)
        case("synthetic user action rejected", False, candidate_evidence={"version": RECEIPT_VERSION, "receipts": [good["receipts"][0], good["receipts"][1], synthetic_post]})
        wrong_lifecycle_post = {**good["receipts"][2], "observation": {**good["receipts"][2]["observation"], "lifecycle_conditions": ["external_bridge", "tab_active", "document_visible"]}}
        wrong_lifecycle_post["signature"] = receipt_signature(wrong_lifecycle_post, key)
        case("active lifecycle cannot satisfy inactive scenario", False, candidate_evidence={"version": RECEIPT_VERSION, "receipts": [good["receipts"][0], good["receipts"][1], wrong_lifecycle_post]})
        localhost_post = {**good["receipts"][2], "observation": {**good["receipts"][2]["observation"], "runtime_provenance": {**good["receipts"][2]["observation"]["runtime_provenance"], "page_origins": ["http://localhost:8000"]}}}
        localhost_post["signature"] = receipt_signature(localhost_post, key)
        case("localhost fixture rejected as live site", False, candidate_evidence={"version": RECEIPT_VERSION, "receipts": [good["receipts"][0], good["receipts"][1], localhost_post]})
        headless_post = {**good["receipts"][2], "observation": {**good["receipts"][2]["observation"], "runtime_provenance": {**good["receipts"][2]["observation"]["runtime_provenance"], "profile_kind": "isolated-test", "headless": True}}}
        headless_post["signature"] = receipt_signature(headless_post, key)
        case("isolated headless browser rejected as user Chrome", False, candidate_evidence={"version": RECEIPT_VERSION, "receipts": [good["receipts"][0], good["receipts"][1], headless_post]})
        wrong_registration_post = {**good["receipts"][2], "observation": {**good["receipts"][2]["observation"], "runtime_provenance": {**good["receipts"][2]["observation"]["runtime_provenance"], "extension_version": "9.9.9"}}}
        wrong_registration_post["signature"] = receipt_signature(wrong_registration_post, key)
        case("wrong loaded extension version rejected", False, candidate_evidence={"version": RECEIPT_VERSION, "receipts": [*good["receipts"][:2], wrong_registration_post, *good["receipts"][3:]]})
        missing_output = {**good["receipts"][2], "observation": {**good["receipts"][2]["observation"], "authoritative_outputs": [{"path": str(output_root / "missing.bin"), "size": 1, "sha256": "0" * 64}]}}
        missing_output["signature"] = receipt_signature(missing_output, key)
        case("missing final download rejected", False, candidate_evidence={"version": RECEIPT_VERSION, "receipts": [good["receipts"][0], good["receipts"][1], missing_output]})
        fabricated = {"version": RECEIPT_VERSION, "receipts": [{**good["receipts"][1], "command": ["never-executed"]}]}
        case("fabricated command rejected", False, candidate_evidence=fabricated)
        no_logs_contract = {**contract, "runtime_diagnosis": {"source": "static_analysis", "failing_boundary": "guess", "log_artifacts": []}}
        case("repeated report without runtime logs rejected", False, candidate_contract=no_logs_contract)
        vague_diagnosis_contract = {**contract, "runtime_diagnosis": {"source": "runtime_log", "failing_boundary": "candidate classifier", "specific_failing_detail": "", "log_artifacts": [str(runtime_log)]}}
        contract_case("vague diagnosis rejected before functional fix", False, vague_diagnosis_contract)
        case("missing unit receipt rejected", False, candidate_evidence={"version": RECEIPT_VERSION, "receipts": good["receipts"][1:]})
        mismatched_post = {**good["receipts"][2], "observation": {**good["receipts"][2]["observation"], "scenario_id": "different-scenario"}}
        mismatched_post["signature"] = receipt_signature(mismatched_post, key)
        case("unit and runtime scenario mismatch rejected", False, candidate_evidence={"version": RECEIPT_VERSION, "receipts": [good["receipts"][0], good["receipts"][1], mismatched_post]})
        case("empty evidence rejected", False, candidate_evidence={"version": RECEIPT_VERSION, "receipts": []})
        forbidden_file = forbidden / "changed.js"
        forbidden_file.write_text("changed\n", encoding="utf-8")
        case("forbidden adjacent scope rejected", False)
        forbidden_file.unlink()
        unsigned = {"version": RECEIPT_VERSION, "receipts": [{**good["receipts"][1], "signature": "0" * 64}]}
        case("unsigned self-attestation rejected", False, candidate_evidence=unsigned)
        rotation_root = root / "rotating-logs"
        rotation_root.mkdir()
        old_log = rotation_root / "000004.log"
        old_log.write_text("old\n", encoding="utf-8")
        rotation_contract = {**contract, "runtime_log_sources": [str(rotation_root)]}
        rotation_state = create_snapshot(rotation_contract)
        new_log = rotation_root / "000007.log"
        new_log.write_text("new run-id\n", encoding="utf-8")
        changed = changed_log_files(rotation_contract, rotation_state)
        results.append({"name": "rotated newest log discovered", "passed": changed == [new_log]})
    ok = all(item["passed"] for item in results)
    print(json.dumps({"ok": ok, "checks": results}, ensure_ascii=False, indent=2))
    observation_path = os.environ.get("TEST_MANAGER_OBSERVATION_PATH")
    if observation_path:
        scenario_id = os.environ.get("TEST_MANAGER_SCENARIO_ID", "test-manager-self-test")
        phase = os.environ.get("TEST_MANAGER_PHASE", "unit")
        requirement_ids = [item for item in os.environ.get("TEST_MANAGER_REQUIREMENT_IDS", "req-self-test").split(",") if item]
        lifecycle_conditions = [item for item in os.environ.get("TEST_MANAGER_LIFECYCLE", "local_cli").split(",") if item]
        observation = {
            "scenario_id": scenario_id, "requirement_ids": requirement_ids,
            "lifecycle_conditions": lifecycle_conditions, "phase": "unit",
            "outcome": "success" if ok else "failure", "observation_level": "unit",
            "runtime": "validator self-test fixtures", "mocked": True,
            "substitutions": [{"component": "runtime recorder", "stage": "all", "reason": "validator schema self-test only"}],
            "entry_stage": "validator", "evidence_kind": "validator_self_test",
            "observed_consumer": "validator self-test result", "user_action": "run self-test",
        }
        write_json(pathlib.Path(observation_path), observation)
    return 0 if ok else 1


def usage() -> int:
    print(
        "usage:\n"
        "  verify_runtime_evidence.py self-test\n"
        "  verify_runtime_evidence.py snapshot <contract.json> <state.json>\n"
        "  verify_runtime_evidence.py run <contract.json> <state.json> <evidence.json> -- <command...>\n"
        "  verify_runtime_evidence.py validate <contract.json> <state.json> <evidence.json>",
        file=sys.stderr,
    )
    return 64


def main(argv: list[str]) -> int:
    if argv == ["self-test"]:
        return self_test()
    if len(argv) == 3 and argv[0] == "snapshot":
        contract = read_json(pathlib.Path(argv[1]))
        errors = validate_contract(contract)
        if errors:
            print(json.dumps({"ok": False, "errors": errors}, ensure_ascii=False, indent=2))
            return 1
        write_json(pathlib.Path(argv[2]), create_snapshot(contract))
        print(json.dumps({"ok": True, "state": argv[2]}, ensure_ascii=False))
        return 0
    if len(argv) >= 6 and argv[0] == "run" and argv[4] == "--":
        contract = read_json(pathlib.Path(argv[1]))
        state = read_json(pathlib.Path(argv[2]))
        errors = validate_contract(contract)
        if errors:
            print(json.dumps({"ok": False, "errors": errors}, ensure_ascii=False, indent=2))
            return 1
        return run_check(contract, state, pathlib.Path(argv[3]), argv[5:])
    if len(argv) == 4 and argv[0] == "validate":
        contract = read_json(pathlib.Path(argv[1]))
        state = read_json(pathlib.Path(argv[2]))
        evidence = read_json(pathlib.Path(argv[3]))
        errors = validate_all(contract, state, evidence)
        print(json.dumps({"ok": not errors, "status": "complete" if not errors else "runtime-unverified", "errors": errors}, ensure_ascii=False, indent=2))
        return 0 if not errors else 2
    return usage()


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
