#!/usr/bin/env python3
"""Validate typed values, rendered templates, decoding, and cross-OS paths."""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import tempfile
from pathlib import Path, PureWindowsPath
from typing import Any
from urllib.parse import quote, unquote

PLACEHOLDER_RE = re.compile(r"\$\{[^{}]+\}|\{\{[^{}]+\}\}|(?<!\{)\{[A-Za-z_][^{}]*\}(?!\})")
PERCENT_RE = re.compile(r"%[0-9A-Fa-f]{2}")
BAD_PERCENT_RE = re.compile(r"%(?![0-9A-Fa-f]{2})")
WINDOWS_RE = re.compile(r"^([A-Za-z]):[\\/](.*)$")
WSL_RE = re.compile(r"^/mnt/([A-Za-z])(?:/(.*))?$")
ERROR_TAGS = {"E", "ERROR", "ERR", "P", "PENDING"}


def masked(value: str, show: bool) -> str:
    if show:
        return value
    if not value:
        return ""
    name = re.split(r"[\\/]", value)[-1]
    return f"<redacted>/{name}" if name else "<redacted>"


def is_error_shape(value: Any) -> bool:
    if isinstance(value, (list, tuple)) and value:
        return isinstance(value[0], str) and value[0].upper() in ERROR_TAGS
    if isinstance(value, dict):
        return any(key in value for key in ("error", "errors", "exception"))
    return False


def validate_value(value: Any, expected_type: str) -> tuple[bool, list[str]]:
    issues: list[str] = []
    if is_error_shape(value):
        issues.append("structured error value reached the consumer")

    valid = {
        "integer": type(value) is int,
        "opaque-string": isinstance(value, str),
        "path": isinstance(value, str) and bool(value),
        "object": isinstance(value, dict),
        "array": isinstance(value, list),
    }[expected_type]
    if not valid:
        issues.append(f"expected {expected_type}, received {type(value).__name__}")
    if expected_type == "integer" and isinstance(value, str) and value.isdigit():
        issues.append("numeric string was not implicitly coerced")
    return not issues, issues


def inspect_template(source: str, rendered: str, variables: dict[str, Any] | None) -> dict[str, Any]:
    placeholders = PLACEHOLDER_RE.findall(source)
    unresolved = PLACEHOLDER_RE.findall(rendered)
    issues: list[str] = []
    if placeholders and source == rendered:
        issues.append("template output is unchanged")
    if unresolved:
        issues.append("rendered output contains unresolved placeholders")

    missing_variables: list[str] = []
    error_variables: list[str] = []
    for key, value in (variables or {}).items():
        if is_error_shape(value):
            error_variables.append(key)
            continue
        text = str(value)
        candidates = {text, quote(text, safe=""), quote(text, safe="/\\:")}
        if text and not any(candidate in rendered for candidate in candidates):
            missing_variables.append(key)
    if missing_variables:
        issues.append("expected variable values are absent from rendered output")
    if error_variables:
        issues.append("structured error values were supplied as template variables")
    applied = bool(placeholders) and source != rendered and not unresolved and not issues
    return {
        "ok": not issues,
        "applied": applied,
        "placeholder_count": len(placeholders),
        "unresolved": unresolved,
        "missing_variables": missing_variables,
        "error_variables": error_variables,
        "issues": issues,
    }


def decode_once(value: str, mode: str) -> tuple[str, bool, bool, list[str]]:
    issues: list[str] = []
    if BAD_PERCENT_RE.search(value):
        return value, False, False, ["malformed percent escape"]
    encoded = bool(PERCENT_RE.search(value))
    should_decode = mode == "once" or (mode == "auto" and encoded)
    decoded = unquote(value) if should_decode else value
    possible_double = should_decode and bool(PERCENT_RE.search(decoded))
    if possible_double:
        issues.append("percent escapes remain after one decode; possible double encoding")
    return decoded, should_decode, possible_double, issues


def map_path(value: str, target: str) -> tuple[str, list[str]]:
    issues: list[str] = []
    if value.startswith("file://"):
        value = value[7:]
        if re.match(r"^/[A-Za-z]:", value):
            value = value[1:]

    windows = WINDOWS_RE.match(value)
    wsl = WSL_RE.match(value)
    if target == "auto":
        target = "wsl" if os.name != "nt" and windows else "native"
    if target in {"native", "linux"}:
        return value, issues
    if target == "wsl":
        if windows:
            drive, rest = windows.groups()
            return f"/mnt/{drive.lower()}/{rest.replace(chr(92), '/')}", issues
        return value, issues
    if target == "windows":
        if wsl:
            drive, rest = wsl.groups()
            suffix = (rest or "").replace("/", "\\")
            return str(PureWindowsPath(f"{drive.upper()}:\\{suffix}")), issues
        return value, issues
    issues.append(f"unsupported target: {target}")
    return value, issues


def inspect_path(value: str, decode: str, target: str, require_exists: bool,
                 require_readable: bool, read_bytes: int, show: bool) -> dict[str, Any]:
    decoded, decoded_once, possible_double, issues = decode_once(value, decode)
    mapped, mapping_issues = map_path(decoded, target)
    issues.extend(mapping_issues)
    exists = os.path.exists(mapped)
    readable = os.path.isfile(mapped) and os.access(mapped, os.R_OK)
    bytes_read = 0
    if require_exists and not exists:
        issues.append("mapped path does not exist")
    if require_readable and not readable:
        issues.append("mapped path is not a readable file")
    if read_bytes:
        if readable:
            try:
                with open(mapped, "rb") as handle:
                    bytes_read = len(handle.read(read_bytes))
                if bytes_read == 0:
                    issues.append("file read returned zero bytes")
            except OSError as exc:
                issues.append(f"file read failed: {exc.__class__.__name__}")
        else:
            issues.append("cannot read bytes from an unreadable path")
    return {
        "ok": not issues,
        "raw": masked(value, show),
        "decoded": masked(decoded, show),
        "mapped": masked(mapped, show),
        "decoded_once": decoded_once,
        "possible_double_encoding": possible_double,
        "exists": exists,
        "readable": readable,
        "bytes_read": bytes_read,
        "issues": issues,
    }


def parse_json(text: str, label: str) -> Any:
    try:
        return json.loads(text)
    except json.JSONDecodeError as exc:
        raise ValueError(f"invalid {label} JSON: {exc.msg}") from exc


def emit(result: dict[str, Any]) -> int:
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
    return 0 if result.get("ok") else 1


def command_value(args: argparse.Namespace) -> int:
    value = parse_json(args.value_json, "value")
    valid, issues = validate_value(value, args.expected_type)
    expectation_met = valid == (args.expect == "valid")
    return emit({
        "ok": expectation_met,
        "actual_valid": valid,
        "expected": args.expect,
        "received_type": type(value).__name__,
        "issues": issues,
    })


def command_template(args: argparse.Namespace) -> int:
    variables = parse_json(args.variables_json, "variables") if args.variables_json else None
    if variables is not None and not isinstance(variables, dict):
        raise ValueError("variables JSON must be an object")
    result = inspect_template(args.source, args.rendered, variables)
    expected = args.expect_applied == "yes"
    result["actual_applied"] = result["applied"]
    result["expected_applied"] = expected
    result["diagnostic_ok"] = result["ok"]
    result["ok"] = result["applied"] == expected
    return emit(result)


def command_path(args: argparse.Namespace) -> int:
    return emit(inspect_path(args.value, args.decode, args.target, args.require_exists,
                             args.require_readable, args.read_bytes, args.show_value))


def command_pipeline(args: argparse.Namespace) -> int:
    variables = parse_json(args.variables_json, "variables") if args.variables_json else None
    if variables is not None and not isinstance(variables, dict):
        raise ValueError("variables JSON must be an object")
    template = inspect_template(args.template, args.rendered, variables)
    path = inspect_path(args.rendered, args.decode, args.target, args.require_exists,
                        args.require_readable, args.read_bytes, args.show_value)
    first_failure = None
    if not template["ok"]:
        first_failure = "template"
    elif not path["ok"]:
        first_failure = "path"
    return emit({"ok": first_failure is None, "first_failure": first_failure,
                 "template": template, "path": path})


def command_self_test(_: argparse.Namespace) -> int:
    checks: list[tuple[str, bool]] = []
    checks.append(("integer", validate_value(17, "integer")[0]))
    checks.append(("numeric string rejected", not validate_value("17", "integer")[0]))
    checks.append(("error tuple rejected", not validate_value(["E", "failure"], "array")[0]))
    opaque = "a34721a6-a405-42d9-9380-cb5bcf05bbfc"
    checks.append(("opaque string preserved", validate_value(opaque, "opaque-string")[0]))
    checks.append(("template applied", inspect_template("결과/{{name}}.png", "결과/장면 1.png", {"name": "장면 1"})["ok"]))
    checks.append(("template unresolved", not inspect_template("결과/{{name}}.png", "결과/{{name}}.png", {"name": "장면 1"})["ok"]))
    checks.append(("template partial", not inspect_template("{{dir}}/{{name}}.png", "결과/{{name}}.png", {"dir": "결과", "name": "장면 1"})["ok"]))
    checks.append(("malformed percent rejected", not inspect_path("C:%ZZ\\x", "auto", "wsl", False, False, 0, True)["ok"]))
    double = inspect_path("C:%252Ftemp", "once", "wsl", False, False, 0, True)
    checks.append(("double encoding reported", double["possible_double_encoding"] and not double["ok"]))
    plus, _, _, _ = decode_once("장면+1%20완료", "once")
    checks.append(("plus preserved", plus == "장면+1 완료"))
    mapped_wsl, _ = map_path("D:\\한글 폴더\\장면 1.png", "wsl")
    checks.append(("windows to WSL", mapped_wsl == "/mnt/d/한글 폴더/장면 1.png"))
    mapped_windows, _ = map_path("/mnt/d/한글 폴더/장면 1.png", "windows")
    checks.append(("WSL to windows", mapped_windows == "D:\\한글 폴더\\장면 1.png"))

    with tempfile.TemporaryDirectory(prefix="check-correct-path-") as temp_dir:
        file_path = Path(temp_dir) / "한글 장면.png"
        file_path.write_bytes(b"image-bytes")
        encoded = quote(str(file_path), safe="/")
        report = inspect_path(encoded, "once", "linux", True, True, 1, False)
        checks.append(("unicode file byte read", report["ok"] and report["bytes_read"] == 1))

    failures = [name for name, passed in checks if not passed]
    return emit({"ok": not failures, "checks": [{"name": name, "passed": passed} for name, passed in checks], "failures": failures})


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    self_test = sub.add_parser("self-test", help="run deterministic regression checks")
    self_test.set_defaults(func=command_self_test)

    value = sub.add_parser("value", help="validate a JSON value without coercion")
    value.add_argument("--value-json", required=True)
    value.add_argument("--expected-type", choices=["integer", "opaque-string", "path", "object", "array"], required=True)
    value.add_argument("--expect", choices=["valid", "invalid"], required=True)
    value.set_defaults(func=command_value)

    template = sub.add_parser("template", help="inspect output from a project template renderer")
    template.add_argument("--source", required=True)
    template.add_argument("--rendered", required=True)
    template.add_argument("--variables-json")
    template.add_argument("--expect-applied", choices=["yes", "no"], required=True)
    template.set_defaults(func=command_template)

    def add_path_arguments(target: argparse.ArgumentParser) -> None:
        target.add_argument("--decode", choices=["auto", "never", "once"], default="auto")
        target.add_argument("--target", choices=["auto", "native", "linux", "windows", "wsl"], default="auto")
        target.add_argument("--require-exists", action="store_true")
        target.add_argument("--require-readable", action="store_true")
        target.add_argument("--read-bytes", type=int, default=0)
        target.add_argument("--show-value", action="store_true")

    path = sub.add_parser("path", help="inspect decoding, path mapping, and file access")
    path.add_argument("--value", required=True)
    add_path_arguments(path)
    path.set_defaults(func=command_path)

    pipeline = sub.add_parser("pipeline", help="inspect template output and its resulting path")
    pipeline.add_argument("--template", required=True)
    pipeline.add_argument("--rendered", required=True)
    pipeline.add_argument("--variables-json")
    add_path_arguments(pipeline)
    pipeline.set_defaults(func=command_pipeline)
    return parser


def main() -> int:
    parser = build_parser()
    try:
        args = parser.parse_args()
        if getattr(args, "read_bytes", 0) < 0:
            raise ValueError("read-bytes must be non-negative")
        return args.func(args)
    except ValueError as exc:
        print(json.dumps({"ok": False, "usage_error": str(exc)}, ensure_ascii=False), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
