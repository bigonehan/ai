#!/usr/bin/env python3
import argparse
import hashlib
import json
import pathlib
import sys


def is_text(value):
    return isinstance(value, str) and bool(value.strip())


def sha256_file(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def validate(contract, receipt):
    errors = []
    if not isinstance(contract, dict):
        return ["contract root must be an object"]
    if not isinstance(receipt, dict):
        return ["receipt root must be an object"]
    independent = contract.get("independent_verification")
    if not isinstance(independent, dict):
        return ["contract.independent_verification must be an object"]
    for key in ("author_id", "verifier_id", "contract_digest"):
        if not is_text(receipt.get(key)):
            errors.append(f"{key} must be non-empty text")
    if receipt.get("author_id") != independent.get("author_id"):
        errors.append("author_id does not match contract")
    if receipt.get("author_id") == receipt.get("verifier_id"):
        errors.append("verifier_id must differ from author_id")
    expected_digest = hashlib.sha256(json.dumps(contract, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    if receipt.get("contract_digest") != expected_digest:
        errors.append("contract_digest does not match contract content")
    scenarios = receipt.get("scenario_ids")
    if not isinstance(scenarios, list) or not scenarios or not all(is_text(item) for item in scenarios):
        errors.append("scenario_ids must be a non-empty text array")
    expected_scenarios = independent.get("scenario_ids")
    if scenarios != expected_scenarios:
        errors.append("scenario_ids do not match contract independent verification scope")
    command_results = receipt.get("command_results")
    if not isinstance(command_results, list) or not command_results:
        errors.append("command_results must be a non-empty array")
    else:
        for index, result in enumerate(command_results):
            if not isinstance(result, dict) or not isinstance(result.get("argv"), list) or not result["argv"] or not all(is_text(arg) for arg in result["argv"]):
                errors.append(f"command_results[{index}].argv must be a non-empty text array")
            if not isinstance(result.get("exit_code"), int) or isinstance(result.get("exit_code"), bool):
                errors.append(f"command_results[{index}].exit_code must be an integer")
            if not is_text(result.get("output_sha256")) or len(result["output_sha256"]) != 64:
                errors.append(f"command_results[{index}].output_sha256 must be SHA-256 text")
        standard = independent.get("standard_suite_command")
        if not any(result.get("argv") == standard and result.get("exit_code") == 0 for result in command_results if isinstance(result, dict)):
            errors.append("standard suite has no passing command result")
    for key in ("mutant_rejected", "standard_suite_registered", "stale_fixture_check", "negative_scope_checked"):
        if receipt.get(key) is not True:
            errors.append(f"{key} must be true")
    artifacts = receipt.get("reviewed_artifacts")
    if not isinstance(artifacts, list) or not artifacts:
        errors.append("reviewed_artifacts must be a non-empty array")
    else:
        for index, item in enumerate(artifacts):
            field = f"reviewed_artifacts[{index}]"
            if not isinstance(item, dict) or not is_text(item.get("path")) or not is_text(item.get("sha256")):
                errors.append(f"{field} requires path and sha256")
                continue
            path = pathlib.Path(item["path"])
            if not path.is_file():
                errors.append(f"{field}.path is not a file")
            elif sha256_file(path) != item["sha256"]:
                errors.append(f"{field}.sha256 does not match current artifact")
        expected_paths = independent.get("reviewed_artifacts")
        if not isinstance(expected_paths, list) or {item.get("path") for item in artifacts if isinstance(item, dict)} != set(expected_paths):
            errors.append("reviewed_artifacts do not exactly match contract scope")
    findings = receipt.get("findings")
    if not isinstance(findings, list):
        errors.append("findings must be an array")
    else:
        for index, finding in enumerate(findings):
            if not isinstance(finding, dict) or not is_text(finding.get("summary")) or finding.get("disposition") not in {"fixed", "dropped-with-basis"}:
                errors.append(f"findings[{index}] requires summary and a closed disposition")
            elif finding.get("disposition") == "dropped-with-basis" and not is_text(finding.get("basis")):
                errors.append(f"findings[{index}].basis is required when dropped")
    return errors


def self_test():
    with pathlib.Path(__file__).open("rb") as handle:
        own_hash = hashlib.sha256(handle.read()).hexdigest()
    contract = {
        "independent_verification": {
            "author_id": "implementation-agent",
            "scenario_ids": ["scenario-1"],
            "standard_suite_command": ["node", "scripts/check.js"],
            "reviewed_artifacts": [__file__],
        }
    }
    contract_digest = hashlib.sha256(json.dumps(contract, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    base = {
        "author_id": "implementation-agent",
        "verifier_id": "independent-agent",
        "contract_digest": contract_digest,
        "scenario_ids": ["scenario-1"],
        "command_results": [{"argv": ["node", "scripts/check.js"], "exit_code": 0, "output_sha256": "a" * 64}],
        "mutant_rejected": True,
        "standard_suite_registered": True,
        "stale_fixture_check": True,
        "negative_scope_checked": True,
        "reviewed_artifacts": [{"path": __file__, "sha256": own_hash}],
        "findings": [{"summary": "binding checked", "disposition": "fixed"}],
    }
    if validate(contract, base):
        return 1
    mutations = [
        ("same identity", lambda item: item.update(verifier_id=item["author_id"])),
        ("mutant survived", lambda item: item.update(mutant_rejected=False)),
        ("suite unregistered", lambda item: item.update(standard_suite_registered=False)),
        ("stale fixture", lambda item: item.update(stale_fixture_check=False)),
        ("open finding", lambda item: item.update(findings=[{"summary": "open", "disposition": "open"}])),
        ("artifact changed", lambda item: item["reviewed_artifacts"][0].update(sha256="0" * 64)),
    ]
    for label, mutate in mutations:
        candidate = json.loads(json.dumps(base))
        mutate(candidate)
        if not validate(contract, candidate):
            print(f"self-test failed to reject: {label}", file=sys.stderr)
            return 1
    print("Independent verifier receipt self-test passed")
    return 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("validate", "self-test"))
    parser.add_argument("contract", nargs="?")
    parser.add_argument("receipt", nargs="?")
    args = parser.parse_args()
    if args.command == "self-test":
        return self_test()
    if not args.contract or not args.receipt:
        parser.error("validate requires contract and receipt paths")
    contract = json.loads(pathlib.Path(args.contract).read_text(encoding="utf-8"))
    errors = validate(contract, json.loads(pathlib.Path(args.receipt).read_text(encoding="utf-8")))
    if errors:
        print(json.dumps({"ok": False, "errors": errors}, ensure_ascii=False, indent=2))
        return 1
    print(json.dumps({"ok": True, "receipt": args.receipt}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
