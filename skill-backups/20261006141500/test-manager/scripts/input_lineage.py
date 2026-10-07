"""Check evidence consistency, not the honesty of an arbitrary runtime recorder.

The recording adapter and source captures still require independent review.
"""
import hashlib
import json
from effect_safety import validate_contract as validate_effect_contract, validate_observation as validate_effect_observation
from pathlib import Path


def fingerprint(value):
    return isinstance(value, str) and len(value) == 64 and all(c in "0123456789abcdef" for c in value)


def nonempty(value):
    return isinstance(value, str) and bool(value.strip())


def read_artifact(ref, errors, field, parse=False):
    if not isinstance(ref, dict) or not nonempty(ref.get("path")) or not fingerprint(ref.get("sha256")):
        errors.append(f"{field} requires an absolute path and SHA-256")
        return None
    try:
        path = Path(ref["path"])
        if not path.is_absolute():
            raise ValueError("path must be absolute")
        data = path.read_bytes()
        if not data or hashlib.sha256(data).hexdigest() != ref["sha256"]:
            raise ValueError("empty or changed artifact")
        return json.loads(data) if parse else data
    except (OSError, ValueError) as error:
        errors.append(f"{field}: {error}")
        return None


def validate_original(value, errors, field):
    if not isinstance(value, dict):
        errors.append(f"{field} must describe the original input")
        return
    if set(value) != {"source_kind", "source_ref", "value_type", "byte_length", "sha256"}:
        errors.append(f"{field} requires only source_kind/source_ref/value_type/byte_length/sha256; no raw values")
    if value.get("source_kind") not in ("user_action", "user_value", "user_file", "authorized_fixture"):
        errors.append(f"{field} must originate at the user boundary, not intermediate data")
    if not nonempty(value.get("source_ref")) or not nonempty(value.get("value_type")):
        errors.append(f"{field} requires source and type")
    size = value.get("byte_length")
    if type(size) is not int or size < 0 or not fingerprint(value.get("sha256")):
        errors.append(f"{field} requires byte length and SHA-256")


def validate_scenario(scenario):
    errors = validate_effect_contract(scenario)
    if not isinstance(scenario, dict):
        return ["scenario must be an object"]
    spec = scenario.get("input_lineage")
    if not isinstance(spec, dict):
        return ["scenario.input_lineage is required for v6 evidence"]
    validate_original(spec.get("original_input"), errors, "original_input")
    if isinstance(spec.get("original_input"), dict) and spec["original_input"].get("source_kind") == "authorized_fixture" and not nonempty(spec.get("fixture_authorization")):
        errors.append("fixture input requires explicit authorization and cannot substitute an incident's original input")
    steps = spec.get("production_path")
    if not isinstance(steps, list) or len(steps) < 3 or not all(isinstance(s, dict) for s in steps):
        errors.append("production_path requires input, handler and consumer steps")
    else:
        ids = [step.get("id") for step in steps]
        if not all(nonempty(i) for i in ids) or len(set(ids)) != len(ids):
            errors.append("production_path step IDs must be unique text")
        roles = [step.get("role") for step in steps]
        if roles[0] != "input" or roles[-1] != "consumer" or "handler" not in roles:
            errors.append("production_path must start at input and reach the consumer through its handler")
        for step in steps:
            if step.get("role") not in ("input", "loader", "handler", "transform", "consumer") or not nonempty(step.get("interface")):
                errors.append("each production step requires a role and actual interface")
    expected_result = spec.get("expected_result")
    if not isinstance(expected_result, dict) or set(expected_result) != {"value_type", "byte_length", "sha256"} or not nonempty(expected_result.get("value_type")) or type(expected_result.get("byte_length")) is not int or expected_result["byte_length"] < 0 or not fingerprint(expected_result.get("sha256")):
        errors.append("expected_result requires the type/byte_length/SHA-256 of the requested consumer projection")
    order = spec.get("artifact_order")
    if not isinstance(order, list) or not order:
        errors.append("artifact_order requires the frozen production loader's ordered runtime artifacts")
    else:
        for artifact in order:
            read_artifact(artifact, errors, "artifact_order")
    sources = spec.get("loader_sources")
    if not isinstance(sources, list) or not sources:
        errors.append("loader_sources must identify the production-owned loader or CLI entrypoint")
    else:
        for source in sources:
            read_artifact(source, errors, "loader_sources")
    return errors


def validate_substitutions(observation):
    items = observation.get("substitutions")
    if not isinstance(items, list):
        return ["observation.substitutions must explicitly inventory replaced components and stages"]
    errors = []
    for item in items:
        if not isinstance(item, dict) or not all(nonempty(item.get(k)) for k in ("component", "stage", "reason")):
            errors.append("each substitution requires component, stage and reason")
    if items and observation.get("mocked") is not True:
        errors.append("substitutions cannot be labelled non-mocked")
    if items and observation.get("phase") != "unit":
        errors.append("substituted executions cannot serve as runtime completion evidence")
    if observation.get("phase") == "unit" and observation.get("mocked") is True and not items:
        errors.append("mocked unit observations must identify their substitutions")
    return errors


def validate_trace(scenario, observation):
    errors = validate_scenario(scenario)
    if errors:
        return errors
    if not isinstance(observation, dict):
        return ["observation must be an object"]
    errors.extend(validate_effect_observation(scenario, observation, read_artifact))
    spec = scenario.get("input_lineage")
    if not isinstance(spec, dict):
        return ["scenario input lineage is missing"]
    lineage = observation.get("input_lineage")
    if not isinstance(lineage, dict) or lineage.get("start") != "original_input":
        return ["runtime lineage must start at original_input"]
    trace = read_artifact(lineage.get("trace"), errors, "input_lineage.trace", parse=True)
    if not isinstance(trace, dict):
        return errors + ["input lineage trace must be a recorded JSON object"]
    if trace.get("evidence_kind") != "runtime_capture":
        errors.append("validator fixtures are not product runtime evidence")
    for key in ("scenario_id", "run_id", "phase", "outcome"):
        if trace.get(key) != observation.get(key):
            errors.append(f"trace {key} does not match the receipt")
    if trace.get("original_input") != spec.get("original_input"):
        errors.append("trace original input does not match the scenario source fingerprint")
    validate_original(trace.get("original_input"), errors, "trace.original_input")
    if trace.get("substitutions") != []:
        errors.append("runtime trace contains substitutions or lacks their explicit inventory")
    if trace.get("loader_sources") != spec.get("loader_sources"):
        errors.append("trace loader sources differ from the actual production loader")
    for source in spec.get("loader_sources", []):
        read_artifact(source, errors, "trace.loader_sources")
    loaded = trace.get("loaded_artifacts")
    if loaded != spec.get("artifact_order"):
        errors.append("loaded artifacts do not match the frozen production loader order; test-added modules are forbidden")
    loader_capture = read_artifact(trace.get("loader_capture"), errors, "trace.loader_capture", parse=True)
    if not isinstance(loader_capture, dict):
        errors.append("runtime trace must capture the production loader's actual ordered modules")
    else:
        expected_loader = {"run_id": observation.get("run_id"), "scenario_id": observation.get("scenario_id"), "phase": observation.get("phase"),
                           "loader_sources": spec.get("loader_sources"), "artifact_order": spec.get("artifact_order")}
        if any(loader_capture.get(k) != v for k, v in expected_loader.items()):
            errors.append("production loader capture is not correlated with the frozen source/order/run")
    if not isinstance(loaded, list) or not loaded:
        errors.append("trace must record actually loaded artifacts")
    else:
        for artifact in loaded:
            read_artifact(artifact, errors, "trace.loaded_artifacts")
        if loaded != observation.get("loaded_artifacts"):
            errors.append("trace loaded artifacts differ from the receipt")
    steps = trace.get("steps")
    expected = spec.get("production_path", [])
    if not isinstance(steps, list) or not steps or not all(isinstance(s, dict) for s in steps):
        return errors + ["trace requires actual ordered execution steps"]
    identities = lambda values: [{k: s.get(k) for k in ("id", "role", "interface")} for s in values]
    # A pre-fix failure may stop at the broken boundary; never fabricate downstream success.
    if identities(steps) != identities(expected[:len(steps)]) or len(steps) > len(expected):
        errors.append("trace bypasses or replaces the declared production path")
    if observation.get("outcome") == "success" and len(steps) != len(expected):
        errors.append("successful trace does not reach the authoritative consumer")
    previous = spec.get("original_input", {}).get("sha256")
    timestamp = observation.get("action_started_ns")
    end = observation.get("action_finished_ns")
    for index, step in enumerate(steps):
        field = f"trace.steps[{index}]"
        if step.get("input_sha256") != previous or not fingerprint(step.get("output_sha256")):
            errors.append(f"{field} breaks the original-input transformation chain")
        previous = step.get("output_sha256")
        at = step.get("at_ns")
        if type(at) is not int or type(timestamp) is not int or type(end) is not int or not timestamp <= at <= end:
            errors.append(f"{field} is outside the ordered action interval")
        timestamp = at
        if step.get("substituted") is not False:
            errors.append(f"{field} is injected or lacks explicit substitution status")
        status = "failure" if index == len(steps) - 1 and observation.get("outcome") == "failure" else "success"
        if step.get("status") != status:
            errors.append(f"{field} has an inconsistent outcome")
        capture = read_artifact(step.get("capture"), errors, f"{field}.capture", parse=True)
        if not isinstance(capture, dict):
            errors.append(f"{field} lacks a captured boundary observation")
            continue
        expected_capture = {"scenario_id": observation.get("scenario_id"), "run_id": observation.get("run_id"),
                            "phase": observation.get("phase"), "step_id": step.get("id"),
                            **{k: step.get(k) for k in ("at_ns", "input_sha256", "output_sha256", "status")}}
        if any(capture.get(k) != v for k, v in expected_capture.items()):
            errors.append(f"{field} capture is not correlated with this input/action/result")
        if capture.get("channel") not in ("browser-event", "browser-protocol", "process-io", "filesystem", "user-recording"):
            errors.append(f"{field} capture must come from an observed runtime channel")
        captured = capture.get("observation")
        if isinstance(captured, dict):
            readback = captured.get("readback")
            if not isinstance(readback, dict) or set(readback) != {"value_type", "byte_length", "sha256"} or not nonempty(readback.get("value_type")) or type(readback.get("byte_length")) is not int or readback["byte_length"] < 0 or readback.get("sha256") != step.get("output_sha256"):
                errors.append(f"{field} requires an independently read boundary value fingerprint")
            if index == 0:
                original = spec.get("original_input", {})
                if captured.get("source_ref") != original.get("source_ref") or readback != {k: original.get(k) for k in ("value_type", "byte_length", "sha256")}:
                    errors.append("input capture does not observe the original user source and value")
            if step.get("role") == "consumer":
                if captured.get("consumer") != scenario.get("authoritative_consumer"):
                    errors.append("final readback does not identify the authoritative consumer")
                if observation.get("outcome") == "success" and readback != spec.get("expected_result"):
                    errors.append("declared success is not supported by the requested consumer readback")
        if not isinstance(captured, dict) or not captured:
            errors.append(f"{field} capture must contain observable boundary data, not a success flag")
    return errors
