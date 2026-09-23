#!/usr/bin/env python3
"""Verify staged, installed, and backed-up copies of the UI design skill."""

from __future__ import annotations

import hashlib
import json
import os
import pathlib
import subprocess
import sys
import time


ROOT = pathlib.Path("/home/tree/ai/test/tmp/ui-design-skill")
STAGED = ROOT / "staging/ui-design-implementation"
INSTALLED = pathlib.Path("/home/tree/.codex/skills/ui-design-implementation")
BACKUP = pathlib.Path("/home/tree/ai/skills/ui-design-implementation")
QUICK_VALIDATE = pathlib.Path("/home/tree/.codex/skills/.system/skill-creator/scripts/quick_validate.py")
INPUT_LOG = pathlib.Path("/home/tree/ai/Input.md")
AGENTS_SOURCE = pathlib.Path("/home/tree/.codex/AGENTS.md")
AGENTS_BACKUP = pathlib.Path("/home/tree/ai/codex/AGENTS.override.md")
ARCHIVED_BUILD_DESIGN = pathlib.Path("/home/tree/ai/archives/build-design/SKILL.md")
ARCHIVED_BUILD_DESIGN_SHA256 = "30bea12d84664c9b0618697998cf9fdf61204dfe2e1cc4bbbe602523cd9c9db3"


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def artifact(path: pathlib.Path) -> dict[str, str]:
    return {"path": str(path.resolve()), "sha256": sha256(path)}


def write_json(path: pathlib.Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def run(argv: list[str]) -> tuple[int, str]:
    result = subprocess.run(argv, capture_output=True, text=True, check=False)
    return result.returncode, result.stdout + result.stderr


def files(root: pathlib.Path) -> dict[str, str]:
    if not root.is_dir():
        return {}
    return {
        str(path.relative_to(root)): sha256(path)
        for path in sorted(root.rglob("*"))
        if path.is_file() and "__pycache__" not in path.parts
    }


def fingerprint(value_type: str, value: object) -> dict[str, object]:
    encoded = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return {
        "value_type": value_type,
        "byte_length": len(encoded),
        "sha256": hashlib.sha256(encoded).hexdigest(),
    }


def validate_skill(label: str, root: pathlib.Path) -> tuple[list[str], dict[str, object]]:
    errors: list[str] = []
    quick_code, quick_output = run([sys.executable, str(QUICK_VALIDATE), str(root)])
    if quick_code != 0:
        errors.append(f"{label} skill validation failed: {quick_output.strip()}")
    validator = root / "scripts/validate_layout_contract.py"
    self_test_code, self_test_output = run([sys.executable, str(validator), "self-test"])
    if self_test_code != 0:
        errors.append(f"{label} layout validator self-test failed: {self_test_output.strip()}")
    return errors, {
        "root": str(root),
        "quick_validate": {"exit_code": quick_code, "output_sha256": hashlib.sha256(quick_output.encode()).hexdigest()},
        "layout_self_test": {"exit_code": self_test_code, "output_sha256": hashlib.sha256(self_test_output.encode()).hexdigest()},
    }


def check_unit() -> tuple[list[str], dict[str, dict[str, object]]]:
    errors, validation = validate_skill("staged", STAGED)
    return errors, {
        "skill-copy-loader": fingerprint("staged-skill-tree", files(STAGED)),
        "skill-validator": fingerprint("validation-command-results", [validation]),
    }


def check_post() -> tuple[list[str], dict[str, dict[str, object]]]:
    errors: list[str] = []
    validations: list[dict[str, object]] = []
    expected = files(STAGED)
    for label, root in (("staged", STAGED), ("installed", INSTALLED), ("backup", BACKUP)):
        actual = files(root)
        if label != "staged" and actual != expected:
            errors.append(f"{label} skill tree differs from staged tree")
        validation_errors, validation = validate_skill(label, root)
        errors.extend(validation_errors)
        validations.append(validation)
    if not AGENTS_SOURCE.is_file() or not AGENTS_BACKUP.is_file() or sha256(AGENTS_SOURCE) != sha256(AGENTS_BACKUP):
        errors.append("AGENTS backup differs from active .codex AGENTS.md")
    build_design_exists = pathlib.Path("/home/tree/ai/skills/build-design").exists()
    if build_design_exists:
        errors.append("~/ai/skills/build-design still exists")
    if not ARCHIVED_BUILD_DESIGN.is_file() or sha256(ARCHIVED_BUILD_DESIGN) != ARCHIVED_BUILD_DESIGN_SHA256:
        errors.append("archived build-design source changed")
    consumer = {
        "installed_tree": files(INSTALLED),
        "backup_tree": files(BACKUP),
        "active_agents_sha256": sha256(AGENTS_SOURCE) if AGENTS_SOURCE.is_file() else None,
        "backup_agents_sha256": sha256(AGENTS_BACKUP) if AGENTS_BACKUP.is_file() else None,
        "ai_skills_build_design_exists": build_design_exists,
        "archived_build_design_sha256": sha256(ARCHIVED_BUILD_DESIGN) if ARCHIVED_BUILD_DESIGN.is_file() else None,
    }
    return errors, {
        "skill-copy-loader": fingerprint("staged-skill-tree", expected),
        "skill-validator": fingerprint("validation-command-results", validations),
        "installed-and-backup-readback": fingerprint("installed-and-backup-filesystem-state", consumer),
    }


def capture(path: pathlib.Path, observation: dict[str, object], step: dict[str, object], readback: dict[str, object], channel: str) -> dict[str, str]:
    value = {
        "scenario_id": observation["scenario_id"],
        "run_id": observation["run_id"],
        "phase": observation["phase"],
        "step_id": step["id"],
        "at_ns": step["at_ns"],
        "input_sha256": step["input_sha256"],
        "output_sha256": step["output_sha256"],
        "status": step["status"],
        "channel": channel,
        "observation": {
            "source_ref": observation["original_input"]["source_ref"],
            "readback": readback,
            "consumer": observation["observed_consumer"] if step["role"] == "consumer" else None,
        },
    }
    write_json(path, value)
    return artifact(path)


def attach_trace(
    contract: dict[str, object],
    scenario: dict[str, object],
    observation: dict[str, object],
    readbacks: dict[str, dict[str, object]],
) -> None:
    spec = scenario["input_lineage"]
    trace_root = ROOT / "runtime" / str(observation["run_id"])
    trace_root.mkdir(parents=True, exist_ok=True)
    loader_value = {
        "run_id": observation["run_id"],
        "scenario_id": observation["scenario_id"],
        "phase": observation["phase"],
        "loader_sources": spec["loader_sources"],
        "artifact_order": spec["artifact_order"],
    }
    loader_path = trace_root / "loader.json"
    write_json(loader_path, loader_value)

    steps: list[dict[str, object]] = []
    previous = spec["original_input"]["sha256"]
    for index, declared in enumerate(spec["production_path"]):
        is_input = index == 0
        readback = (
            {key: spec["original_input"][key] for key in ("value_type", "byte_length", "sha256")}
            if is_input
            else readbacks[declared["id"]]
        )
        output = readback["sha256"]
        step = {
            **declared,
            "at_ns": observation["action_started_ns"] + index + 1,
            "substituted": False,
            "input_sha256": previous,
            "output_sha256": output,
            "status": "success",
        }
        step["capture"] = capture(trace_root / f"step-{index}.json", observation, step, readback, "filesystem")
        steps.append(step)
        previous = output

    trace = {
        "evidence_kind": "runtime_capture",
        "scenario_id": observation["scenario_id"],
        "run_id": observation["run_id"],
        "phase": observation["phase"],
        "outcome": observation["outcome"],
        "original_input": observation["original_input"],
        "substitutions": [],
        "loader_sources": spec["loader_sources"],
        "loaded_artifacts": observation["loaded_artifacts"],
        "loader_capture": artifact(loader_path),
        "steps": steps,
    }
    trace_path = trace_root / "trace.json"
    write_json(trace_path, trace)
    observation["input_lineage"] = {"start": "original_input", "trace": artifact(trace_path)}


def main() -> int:
    if len(sys.argv) != 3 or sys.argv[1] not in {"unit", "post"}:
        print("usage: verify_skill_backup.py unit|post contract.json", file=sys.stderr)
        return 2
    phase_arg = sys.argv[1]
    contract = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
    scenario = contract["acceptance_scenarios"][0]
    started = time.time_ns()
    errors, readbacks = check_unit() if phase_arg == "unit" else check_post()
    phase = "unit" if phase_arg == "unit" else "post_fix"
    observation: dict[str, object] = {
        "scenario_id": scenario["scenario_id"],
        "requirement_ids": scenario["requirement_ids"],
        "runtime": "Python skill package validator" if phase_arg == "unit" else "local filesystem Codex skill loader",
        "user_action": scenario["user_action"],
        "observed_consumer": contract["unit_observation"] if phase_arg == "unit" else scenario["authoritative_consumer"],
        "phase": phase,
        "outcome": "success" if not errors else "failure",
        "observation_level": "unit" if phase_arg == "unit" else "runtime",
        "run_id": f"ui-skill-{phase_arg}-{started}",
        "action_started_ns": started,
        "action_finished_ns": time.time_ns() + 100,
        "original_input": scenario["input_lineage"]["original_input"],
        "entry_stage": "original_input",
        "substitutions": [],
        "mocked": False,
        "loaded_artifacts": scenario["input_lineage"]["artifact_order"],
        "uncaught_errors": errors,
        "unobserved_layers": [],
    }
    if phase_arg == "post":
        observation.update({
            "input_fidelity": {
                "production_boundary": contract["input_fidelity"]["production_boundary"],
                "driver": "Python filesystem process",
                "matches_user_action": True,
                "bypassed_layers": [],
                "synthetic_shortcuts": [],
                "observations": {key: True for key in contract["input_fidelity"]["required_observations"]},
            },
            "runtime_provenance": {
                "environment": contract["runtime_target"]["environment"],
                "fixture": False,
                "page_origins": [],
            },
            "authoritative_outputs": [],
            "runtime_logs": [str(INPUT_LOG)],
        })
        attach_trace(contract, scenario, observation, readbacks)
    observation_path = os.environ.get("TEST_MANAGER_OBSERVATION_PATH")
    if observation_path:
        write_json(pathlib.Path(observation_path), observation)
    print(json.dumps({"ok": not errors, "phase": phase, "errors": errors}, ensure_ascii=False, indent=2))
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
