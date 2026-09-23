#!/usr/bin/env python3
"""Fail-closed budget for requests that ask a user to operate a product."""

from __future__ import annotations

import argparse
import datetime as dt
import fcntl
import json
import os
import pathlib
import sys
import uuid
from typing import Any


ACTION_TYPES = {"settings_test", "extension_reload", "generation", "other"}
EVENT_KINDS = {"requested", "observed"}


def is_text(value: object) -> bool:
    return isinstance(value, str) and bool(value.strip())


def in_verification_workspace(path: pathlib.Path) -> bool:
    parts = path.resolve().parts
    return any(parts[index:index + 2] == ("test", "tmp") for index in range(len(parts) - 1))


def read_ledger(path: pathlib.Path) -> object:
    return json.loads(path.read_text(encoding="utf-8"))


def write_ledger(path: pathlib.Path, ledger: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    try:
        temporary.write_text(json.dumps(ledger, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def validate_ledger(
    ledger: object,
    contract: object = None,
    *,
    require_observed: bool = False,
    runtime_run_ids: set[str] | None = None,
) -> list[str]:
    if not isinstance(ledger, dict):
        return ["action ledger root must be an object"]
    errors: list[str] = []
    if ledger.get("version") != 1:
        errors.append("action ledger version must be 1")
    if not is_text(ledger.get("task_id")):
        errors.append("action ledger task_id must be non-empty text")
    events = ledger.get("events")
    if not isinstance(events, list):
        return errors + ["action ledger events must be an array"]
    ids: set[str] = set()
    requested: list[dict[str, Any]] = []
    observed: list[dict[str, Any]] = []
    for index, event in enumerate(events):
        field = f"events[{index}]"
        if not isinstance(event, dict):
            errors.append(f"{field} must be an object")
            continue
        for key in ("event_id", "at_utc", "source", "scope"):
            if not is_text(event.get(key)):
                errors.append(f"{field}.{key} must be non-empty text")
        if is_text(event.get("event_id")):
            if event["event_id"] in ids:
                errors.append(f"{field}.event_id is duplicated")
            ids.add(event["event_id"])
        if event.get("kind") not in EVENT_KINDS:
            errors.append(f"{field}.kind must be requested or observed")
        if event.get("action_type") not in ACTION_TYPES:
            errors.append(f"{field}.action_type is invalid")
        if event.get("kind") == "requested":
            requested.append(event)
        elif event.get("kind") == "observed":
            observed.append(event)
            if not is_text(event.get("run_id")):
                errors.append(f"{field}.run_id must identify the observed run")
    if len(requested) > 1:
        errors.append("more than one user action was requested")
    if len(observed) > 1:
        errors.append("more than one user action was observed")
    if requested and observed:
        if requested[0].get("action_type") != observed[0].get("action_type") or requested[0].get("scope") != observed[0].get("scope"):
            errors.append("requested and observed actions differ; another user action was requested")
        if events.index(observed[0]) < events.index(requested[0]):
            errors.append("observed action predates request; these are separate user actions")
    if require_observed and not observed:
        errors.append("the one user action has no observed run")
    if runtime_run_ids is not None and observed and observed[0].get("run_id") not in runtime_run_ids:
        errors.append("observed user run_id is absent from runtime receipts")
    if contract is not None:
        if not isinstance(contract, dict):
            errors.append("contract root must be an object")
        else:
            if ledger.get("task_id") != contract.get("task_id"):
                errors.append("action ledger task_id does not match contract")
            execution = contract.get("user_execution")
            if not isinstance(execution, dict) or execution.get("manual_run_required") is not True:
                errors.append("contract must declare a required single user run")
            else:
                expected_type = execution.get("action_type")
                if expected_type not in ACTION_TYPES:
                    errors.append("user_execution.action_type must identify the initiating action")
                for event in requested + observed:
                    if event.get("action_type") != expected_type:
                        errors.append("action ledger contains an action outside the contract entrypoint")
                        break
    return errors


def result(errors: list[str], *, success_status: str = "valid", **extra: object) -> int:
    print(json.dumps({"ok": not errors, "status": success_status if not errors else "denied", "errors": errors, **extra}, ensure_ascii=False))
    return 0 if not errors else 2


def add_event(ledger: dict[str, Any], kind: str, action_type: str, scope: str, source: str, run_id: str | None) -> None:
    event = {
        "event_id": str(uuid.uuid4()),
        "at_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "kind": kind,
        "action_type": action_type,
        "scope": scope,
        "source": source,
    }
    if run_id:
        event["run_id"] = run_id
    ledger["events"].append(event)


def reservation_errors(ledger: object, contract: object, ledger_path: pathlib.Path, action_type: str) -> list[str]:
    errors = validate_ledger(ledger, contract)
    execution = contract.get("user_execution", {}) if isinstance(contract, dict) else {}
    if not isinstance(execution, dict) or execution.get("action_ledger_path") != str(ledger_path.resolve()):
        errors.append("contract must bind this exact action ledger path")
    if isinstance(execution, dict) and action_type != execution.get("action_type"):
        errors.append("requested action does not match the single contract entrypoint")
    if isinstance(ledger, dict) and ledger.get("events"):
        errors.append("user action budget is already consumed; use captured evidence or report runtime-unverified")
    return errors


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    init = sub.add_parser("init", help="Create an empty task ledger without overwriting history")
    init.add_argument("ledger", type=pathlib.Path)
    init.add_argument("task_id")
    reserve = sub.add_parser("reserve", help="Reserve the sole user-facing request before sending it")
    reserve.add_argument("contract", type=pathlib.Path)
    reserve.add_argument("ledger", type=pathlib.Path)
    reserve.add_argument("action_type", choices=sorted(ACTION_TYPES))
    reserve.add_argument("scope")
    reserve.add_argument("source", help="Conversation message or task reference, never prompt text")
    observed = sub.add_parser("observe", help="Record the run produced by the reserved request")
    observed.add_argument("ledger", type=pathlib.Path)
    observed.add_argument("run_id")
    observed.add_argument("source")
    existing = sub.add_parser("record-existing", help="Backfill historical requests or runs without granting more budget")
    existing.add_argument("ledger", type=pathlib.Path)
    existing.add_argument("kind", choices=sorted(EVENT_KINDS))
    existing.add_argument("action_type", choices=sorted(ACTION_TYPES))
    existing.add_argument("scope")
    existing.add_argument("source")
    existing.add_argument("--run-id")
    audit = sub.add_parser("audit", help="Validate actual request/run history")
    audit.add_argument("ledger", type=pathlib.Path)
    audit.add_argument("--contract", type=pathlib.Path)
    audit.add_argument("--require-observed", action="store_true")
    audit.add_argument("--run-id", action="append")
    args = parser.parse_args(argv)
    if not in_verification_workspace(args.ledger) or args.ledger.suffix != ".json":
        return result(["action ledger must be a JSON file under a project test/tmp directory"])
    if args.command in {"init", "reserve", "observe", "record-existing"}:
        lock_path = args.ledger.with_name(args.ledger.name + ".lock")
        lock_path.parent.mkdir(parents=True, exist_ok=True)
        with lock_path.open("a+", encoding="utf-8") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            return execute(args)
    return execute(args)


def execute(args: argparse.Namespace) -> int:
    if args.command == "init":
        if not is_text(args.task_id):
            return result(["task_id must be non-empty text"])
        args.ledger.parent.mkdir(parents=True, exist_ok=True)
        try:
            with args.ledger.open("x", encoding="utf-8") as handle:
                json.dump({"version": 1, "task_id": args.task_id, "events": []}, handle, ensure_ascii=False, indent=2)
                handle.write("\n")
        except FileExistsError:
            return result(["action ledger already exists; history cannot be reset"])
        return result([], success_status="initialized")
    try:
        ledger = read_ledger(args.ledger)
    except (OSError, json.JSONDecodeError) as error:
        return result([f"action ledger is missing or unreadable: {error}"])
    if args.command == "audit":
        try:
            contract = json.loads(args.contract.read_text(encoding="utf-8")) if args.contract else None
        except (OSError, json.JSONDecodeError) as error:
            return result([f"contract is unreadable: {error}"])
        return result(validate_ledger(ledger, contract, require_observed=args.require_observed,
                                      runtime_run_ids=set(args.run_id) if args.run_id else None))
    if not isinstance(ledger, dict):
        return result(["action ledger root must be an object"])
    if args.command == "reserve":
        try:
            contract = json.loads(args.contract.read_text(encoding="utf-8"))
            from verify_runtime_evidence import validate_contract
            errors = validate_contract(contract)
        except (OSError, json.JSONDecodeError) as error:
            return result([f"contract is unreadable: {error}"])
        errors.extend(reservation_errors(ledger, contract, args.ledger, args.action_type))
        if not all(is_text(value) for value in (args.scope, args.source)):
            errors.append("scope and source must be non-empty")
        if errors:
            return result(errors)
        add_event(ledger, "requested", args.action_type, args.scope, args.source, None)
    elif args.command == "observe":
        errors = validate_ledger(ledger)
        requests = [event for event in ledger.get("events", []) if isinstance(event, dict) and event.get("kind") == "requested"]
        if len(requests) != 1 or any(event.get("kind") == "observed" for event in ledger.get("events", [])):
            errors.append("observe requires exactly one unobserved reserved request")
        if not is_text(args.run_id) or not is_text(args.source):
            errors.append("run_id and source must be non-empty")
        if errors:
            return result(errors)
        add_event(ledger, "observed", requests[0]["action_type"], requests[0]["scope"], args.source, args.run_id)
    else:
        errors = validate_ledger(ledger)
        if errors and not ledger.get("events"):
            return result(errors)
        if not all(is_text(value) for value in (args.scope, args.source)):
            return result(["scope and source must be non-empty"])
        if args.kind == "observed" and not is_text(args.run_id):
            return result(["historical observed action requires run_id"])
        add_event(ledger, args.kind, args.action_type, args.scope, args.source, args.run_id)
    write_ledger(args.ledger, ledger)
    return result([], success_status="reserved" if args.command == "reserve" else "recorded",
                  event_count=len(ledger["events"]))


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
