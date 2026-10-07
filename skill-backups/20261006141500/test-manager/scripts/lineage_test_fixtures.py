"""Synthetic input for validator unit tests ONLY. Never a runtime recording adapter."""
import hashlib
import json
from pathlib import Path


def artifact(path):
    path = Path(path)
    return {"path": str(path.resolve()), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def write(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False), encoding="utf-8")
    return artifact(path)


def scenario_spec(scenario, loader):
    action = scenario["user_action"].encode("utf-8")
    return {
        "original_input": {"source_kind": "user_action", "source_ref": "scenario:" + scenario["scenario_id"],
                           "value_type": "action", "byte_length": len(action), "sha256": hashlib.sha256(action).hexdigest()},
        "production_path": [{"id": role, "role": role, "interface": interface} for role, interface in
                            [("input", "pointer target"), ("loader", "production script list"),
                             ("handler", "delegated listener"), ("consumer", scenario["authoritative_consumer"])]],
        "loader_sources": [artifact(loader)],
        "artifact_order": [artifact(loader)],
        "expected_result": {"value_type": "action", "byte_length": len(action), "sha256": hashlib.sha256(action).hexdigest()},
    }


def attach_fixture(directory, scenario, observation):
    spec = scenario["input_lineage"]
    observation["original_input"] = spec["original_input"]
    observation["entry_stage"] = "original_input"
    observation["substitutions"] = ([{"component": "runtime", "stage": "handler", "reason": "unit fixture"}]
                                     if observation["phase"] == "unit" else [])
    if observation["phase"] == "unit":
        return
    directory = Path(directory) / observation["scenario_id"] / observation["phase"]
    trace = {key: observation[key] for key in ("scenario_id", "run_id", "phase", "outcome", "original_input", "substitutions", "loaded_artifacts")}
    # These records exercise the *shape* of runtime evidence, not real product behavior.
    trace.update(evidence_kind="runtime_capture", loader_sources=spec["loader_sources"], steps=[])
    trace["loader_capture"] = write(directory / "loader.json", {**{k: observation[k] for k in ("run_id", "scenario_id", "phase")},
                                                               "loader_sources": spec["loader_sources"], "artifact_order": spec["artifact_order"]})
    for index, declared in enumerate(spec["production_path"]):
        step = {**declared, "at_ns": observation["action_started_ns"], "substituted": False,
                "input_sha256": spec["original_input"]["sha256"], "output_sha256": spec["original_input"]["sha256"],
                "status": "failure" if index == len(spec["production_path"]) - 1 and observation["outcome"] == "failure" else "success"}
        capture = {key: observation[key] for key in ("scenario_id", "run_id", "phase")}
        capture.update({k: step[k] for k in ("at_ns", "input_sha256", "output_sha256", "status")})
        capture.update(step_id=step["id"], channel="browser-event", observation={"target": "fixture-control", "source_ref": spec["original_input"]["source_ref"],
                       "readback": spec["expected_result"], "consumer": scenario["authoritative_consumer"]})
        step["capture"] = write(directory / f"{index}.json", capture)
        trace["steps"].append(step)
    observation["input_lineage"] = {"start": "original_input", "trace": write(directory / "trace.json", trace)}
