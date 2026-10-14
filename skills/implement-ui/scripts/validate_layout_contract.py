#!/usr/bin/env python3
"""Validate measured UI sizes against an explicit semantic layout contract."""

from __future__ import annotations

import argparse
import json
import math
import pathlib


KINDS = {"column", "cell", "content", "thumbnail", "row", "container", "gap"}
AXES = {"width": ("width",), "height": ("height",), "both": ("width", "height")}
METHODS = {"getBoundingClientRect", "computedStyle"}


def finite_number(value: object) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def nonempty_text(value: object) -> bool:
    return isinstance(value, str) and bool(value.strip())


def validate(contract: object) -> list[str]:
    if not isinstance(contract, dict):
        return ["contract root must be an object"]
    errors: list[str] = []
    if contract.get("version") != 1:
        errors.append("version must be 1")
    targets = contract.get("targets")
    if not isinstance(targets, list) or not targets:
        return errors + ["targets must be a non-empty array"]

    names: set[str] = set()
    for index, target in enumerate(targets):
        field = f"targets[{index}]"
        if not isinstance(target, dict):
            errors.append(f"{field} must be an object")
            continue
        name = target.get("name")
        if not nonempty_text(name):
            errors.append(f"{field}.name must be non-empty text")
        elif name in names:
            errors.append(f"duplicate target name: {name}")
        else:
            names.add(name)

        requested_kind = target.get("requested_kind")
        measured_kind = target.get("measured_kind")
        if requested_kind not in KINDS:
            errors.append(f"{field}.requested_kind must be one of {sorted(KINDS)}")
        if measured_kind not in KINDS:
            errors.append(f"{field}.measured_kind must be one of {sorted(KINDS)}")
        elif requested_kind in KINDS and measured_kind != requested_kind:
            errors.append(f"{field} measures {measured_kind}, not requested {requested_kind}")

        for key in ("selector", "layout_owner", "runtime_context"):
            if not nonempty_text(target.get(key)):
                errors.append(f"{field}.{key} must be non-empty text")

        constraints = target.get("conflicting_constraints")
        if not isinstance(constraints, list) or not all(nonempty_text(item) for item in constraints):
            errors.append(f"{field}.conflicting_constraints must be a text array")

        method = target.get("measurement_method")
        if method not in METHODS:
            errors.append(f"{field}.measurement_method must be one of {sorted(METHODS)}")
        axis = target.get("axis")
        if axis not in AXES:
            errors.append(f"{field}.axis must be one of {sorted(AXES)}")
            continue
        tolerance = target.get("tolerance_px", 1)
        if not finite_number(tolerance) or tolerance < 0:
            errors.append(f"{field}.tolerance_px must be a non-negative finite number")
            continue

        before = target.get("before_px")
        goal = target.get("target_px")
        measured = target.get("runtime_px")
        for label, value in (("before_px", before), ("target_px", goal), ("runtime_px", measured)):
            if not isinstance(value, dict):
                errors.append(f"{field}.{label} must be an object")
                continue
            for dimension in AXES[axis]:
                if not finite_number(value.get(dimension)) or value[dimension] < 0:
                    errors.append(f"{field}.{label}.{dimension} must be a non-negative finite number")

        if all(isinstance(value, dict) for value in (goal, measured)):
            for dimension in AXES[axis]:
                if finite_number(goal.get(dimension)) and finite_number(measured.get(dimension)):
                    difference = abs(measured[dimension] - goal[dimension])
                    if difference > tolerance:
                        errors.append(
                            f"{field}.{dimension} differs by {difference:.3f}px "
                            f"(target {goal[dimension]}px, measured {measured[dimension]}px, tolerance {tolerance}px)"
                        )
    return errors


def load(path: pathlib.Path) -> object:
    return json.loads(path.read_text(encoding="utf-8"))


def self_test() -> int:
    valid = {
        "version": 1,
        "targets": [{
            "name": "primary-column",
            "requested_kind": "column",
            "measured_kind": "column",
            "selector": ".primary",
            "axis": "width",
            "layout_owner": "table > colgroup",
            "conflicting_constraints": ["table-layout:auto"],
            "before_px": {"width": 160},
            "target_px": {"width": 96},
            "runtime_px": {"width": 96.5},
            "tolerance_px": 1,
            "measurement_method": "getBoundingClientRect",
            "runtime_context": "headed browser at 1280x720",
        }],
    }
    invalid_kind = json.loads(json.dumps(valid))
    invalid_kind["targets"][0]["measured_kind"] = "thumbnail"
    invalid_size = json.loads(json.dumps(valid))
    invalid_size["targets"][0]["runtime_px"]["width"] = 99

    checks = [
        ("valid contract", not validate(valid)),
        ("semantic mismatch rejected", any("measures thumbnail" in item for item in validate(invalid_kind))),
        ("out-of-tolerance size rejected", any("differs by" in item for item in validate(invalid_size))),
    ]
    failures = [name for name, passed in checks if not passed]
    print(json.dumps({"ok": not failures, "checks": [{"name": n, "passed": p} for n, p in checks]}, indent=2))
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("contract", help="layout contract JSON path or 'self-test'")
    args = parser.parse_args()
    if args.contract == "self-test":
        return self_test()
    path = pathlib.Path(args.contract)
    try:
        errors = validate(load(path))
    except (OSError, json.JSONDecodeError) as error:
        print(json.dumps({"ok": False, "errors": [str(error)]}, ensure_ascii=False, indent=2))
        return 2
    print(json.dumps({"ok": not errors, "errors": errors}, ensure_ascii=False, indent=2))
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
