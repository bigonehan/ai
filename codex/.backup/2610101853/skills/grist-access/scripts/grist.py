#!/usr/bin/env python3
"""Grist access for the extensions workflow without handling the API key.

The key is read from this skill's `.env` (or GRIST_API_KEY in the
environment) and is never printed. All output is JSON on stdout.

Usage:
  grist.py tables
  grist.py columns TABLE
  grist.py get TABLE [--id N ...] [--filter JSON] [--fields A,B]
  grist.py add TABLE --fields JSON
  grist.py update TABLE --id N --fields JSON

Shortcuts:
  grist.py project RAW_ID                  # Table7 row
  grist.py task RAW_ID                     # Table26 row
  grist.py task-update RAW_ID --fields JSON
  grist.py task-add --fields JSON          # Table26 new row
"""
from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

SKILL_DIR = Path(__file__).resolve().parents[1]
PROJECT_TABLE = "Table7"
TASK_TABLE = "Table26"
# Table26 $H is a Choice column with exactly these Korean values.
TASK_STAGES = ("초안", "진단", "교정", "검증", "대기", "완료")


def _load_env() -> dict[str, str]:
    values: dict[str, str] = {}
    env_file = Path(os.environ.get("GRIST_ENV_FILE", SKILL_DIR / ".env"))
    try:
        for line in env_file.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if line and not line.startswith("#") and "=" in line:
                key, value = line.split("=", 1)
                values[key.strip()] = value.strip().strip('"').strip("'")
    except FileNotFoundError:
        pass
    for key in ("GRIST_API_KEY", "GRIST_BASE_URL", "GRIST_DOC_ID"):
        if os.environ.get(key):
            values[key] = os.environ[key]
    return values


def _config() -> tuple[str, str, str]:
    env = _load_env()
    key = env.get("GRIST_API_KEY", "")
    if not key:
        _fail("config", f"GRIST_API_KEY is not set (expected in {SKILL_DIR / '.env'})")
    return key, env.get("GRIST_BASE_URL", "http://localhost:8484").rstrip("/"), env.get("GRIST_DOC_ID", "au1EbKLtZbrW")


def _fail(boundary: str, message: str) -> None:
    # Never include the key or headers in error output.
    print(json.dumps({"ok": False, "boundary": boundary, "error": message}, ensure_ascii=False), file=sys.stderr)
    sys.exit(2)


def _request(method: str, path: str, body: object | None = None) -> object:
    key, base, doc = _config()
    url = f"{base}/api/docs/{doc}{path}"
    data = json.dumps(body).encode("utf-8") if body is not None else None
    request = urllib.request.Request(
        url,
        data=data,
        method=method,
        headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            raw = response.read()
    except urllib.error.HTTPError as exc:
        detail = exc.read().decode("utf-8", "replace")[:300]
        _fail(f"http_{method.lower()}", f"HTTP {exc.code}: {detail}")
    except urllib.error.URLError as exc:
        _fail("connection", f"cannot reach Grist at {base}: {exc.reason}")
    return json.loads(raw) if raw else {}


def _parse_json(text: str, name: str) -> object:
    try:
        return json.loads(text)
    except json.JSONDecodeError as exc:
        _fail("arguments", f"{name} is not valid JSON: {exc}")


def _validate_task_fields(fields: dict[str, object]) -> None:
    stage = fields.get("H")
    if stage is not None and stage not in TASK_STAGES:
        _fail("validation", f"Table26 $H must be one of {', '.join(TASK_STAGES)}")


def tables() -> object:
    return [table["id"] for table in _request("GET", "/tables")["tables"]]


def columns(table: str) -> object:
    result = []
    for column in _request("GET", f"/tables/{table}/columns")["columns"]:
        fields = column["fields"]
        entry = {"id": column["id"], "label": fields.get("label"), "type": fields.get("type")}
        try:
            options = json.loads(fields.get("widgetOptions") or "{}")
        except json.JSONDecodeError:
            options = {}
        # Choice values and date formats tell which cell values are valid.
        for key in ("choices", "dateFormat"):
            if options.get(key):
                entry[key] = options[key]
        result.append(entry)
    return result


def get(table: str, ids: list[int] | None = None, filter_json: dict | None = None, fields: list[str] | None = None) -> object:
    query_filter = dict(filter_json or {})
    if ids:
        query_filter["id"] = ids
    path = f"/tables/{table}/records"
    if query_filter:
        path += "?filter=" + urllib.parse.quote(json.dumps(query_filter, ensure_ascii=False))
    records = _request("GET", path)["records"]
    if fields:
        records = [{"id": r["id"], "fields": {k: r["fields"].get(k) for k in fields}} for r in records]
    return records


def add(table: str, fields: dict[str, object]) -> object:
    result = _request("POST", f"/tables/{table}/records", {"records": [{"fields": fields}]})
    return {"id": result["records"][0]["id"]}


def update(table: str, row_id: int, fields: dict[str, object]) -> object:
    _request("PATCH", f"/tables/{table}/records", {"records": [{"id": row_id, "fields": fields}]})
    return get(table, ids=[row_id])[0]


def main(argv: list[str]) -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("tables")
    p = sub.add_parser("columns"); p.add_argument("table")
    p = sub.add_parser("get"); p.add_argument("table"); p.add_argument("--id", type=int, action="append")
    p.add_argument("--filter"); p.add_argument("--fields")
    p = sub.add_parser("add"); p.add_argument("table"); p.add_argument("--fields", required=True)
    p = sub.add_parser("update"); p.add_argument("table"); p.add_argument("--id", type=int, required=True)
    p.add_argument("--fields", required=True)
    p = sub.add_parser("project"); p.add_argument("row_id", type=int)
    p = sub.add_parser("task"); p.add_argument("row_id", type=int)
    p = sub.add_parser("task-update"); p.add_argument("row_id", type=int); p.add_argument("--fields", required=True)
    p = sub.add_parser("task-add"); p.add_argument("--fields", required=True)
    args = parser.parse_args(argv)

    if args.command == "tables":
        result = tables()
    elif args.command == "columns":
        result = columns(args.table)
    elif args.command == "get":
        result = get(
            args.table,
            ids=args.id,
            filter_json=_parse_json(args.filter, "--filter") if args.filter else None,
            fields=args.fields.split(",") if args.fields else None,
        )
    elif args.command == "add":
        fields = _parse_json(args.fields, "--fields")
        if args.table == TASK_TABLE:
            _validate_task_fields(fields)
        result = add(args.table, fields)
    elif args.command == "update":
        fields = _parse_json(args.fields, "--fields")
        if args.table == TASK_TABLE:
            _validate_task_fields(fields)
        result = update(args.table, args.id, fields)
    elif args.command == "project":
        result = get(PROJECT_TABLE, ids=[args.row_id])
    elif args.command == "task":
        result = get(TASK_TABLE, ids=[args.row_id])
    elif args.command == "task-update":
        fields = _parse_json(args.fields, "--fields")
        _validate_task_fields(fields)
        result = update(TASK_TABLE, args.row_id, fields)
    else:
        fields = _parse_json(args.fields, "--fields")
        _validate_task_fields(fields)
        result = add(TASK_TABLE, fields)
    print(json.dumps(result, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main(sys.argv[1:])
