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
  grist.py delete TABLE --id N
  grist.py column-update TABLE COLUMN --fields JSON   # e.g. {"formula": "..."}

Shortcuts:
  grist.py project RAW_ID                  # Table7 row
  grist.py task RAW_ID                     # Table26 row
  grist.py task-update RAW_ID --fields JSON
  grist.py task-add --fields JSON          # Table26 new row

Stage gate (the only writers of $K and of these $H values):
  grist.py gate --cwd DIR [--row N]        # task ids of the nearest AGENTS.md (or row N) and their $H
  grist.py task-goal RAW_ID --project DIR --goal-file F --summary TEXT
                                           # main, $H=초안: format check + jev scope → Input.md `### 목표 #N`, $K = TEXT
  grist.py task-red RAW_ID --project DIR --test CMD --tests T1,T2 [--paths P1,P2]
                                           # CMD must fail on an assertion → `## 빨강` record, $H=작업
  grist.py task-submit RAW_ID --project DIR
                                           # red record intact, test passes, scope/literal checks, jev match → $H=검증
  grist.py flow-wait --project DIR [--waiting TEXT]
                                           # main: all rows 검증 + reruns + hidden cases + `## 흐름 판정` → $H=대기
  grist.py task-complete RAW_ID --project DIR
                                           # main: `## 완료 판정 #N` for every goal line → $H=완료

Judgments (jev scope/match) come from the access-jev skill; when it is missing,
keyless or failing they are skipped (fail-open). Only JEV_MODE=enforce rejects.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timedelta, timezone
from pathlib import Path

SKILL_DIR = Path(__file__).resolve().parents[1]
PROJECT_TABLE = "Table7"
TASK_TABLE = "Table26"
# Table26 $H is a Choice column with exactly these Korean values.
TASK_STAGES = ("초안", "진단", "작업", "교정", "검증", "대기", "완료")
SUBMITTED_STAGE = "검증"
WAITING_STAGE = "대기"
DONE_STAGE = "완료"
WORKING_STAGES = ("작업", "교정", "진단")
# `- Table26 task Raw ID: 14` or, for a flow split into rows, `- Table26 task Raw ID: 14, 15, 16`.
TASK_ID_LINE = re.compile(r"^\s*-\s*Table26 task Raw ID:\s*(\d+(?:\s*,\s*\d+)*)", re.M)
CHECK_ITEM = re.compile(r"^\s*-\s*\[([ xX])\]\s*(.+?)\s*$")
ROW_TAG = re.compile(r"^#(\d+)\s+(.+)$")
SUBMIT_TEST = re.compile(r"^\s*-\s*테스트:\s*`(.+)`\s*$")
DETAIL_LINE = re.compile(r"^\s*-\s*상세:\s*`(\{.*\})`\s*$")
TEST_TIMEOUT = 900
OUTPUT_TAIL = 4000
DRAFT_STAGE = "초안"
# Table26 $K (label 목표) holds the row's one-line goal summary; the detailed goal lines live
# in the project's Input.md under `### 목표 #N`. Only `task-goal` writes either.
# Rows without that block are legacy rows whose multi-line goal is $K itself.
GOAL_FIELD = "K"
GOAL_HEADING = re.compile(r"^#+\s*목표 #(\d+)\s*$")
GOAL_LINE = re.compile(r"^\s*-\s*\[(값|상태|수치|불변|화면|사용자)\]\s*(.+?)\s*$")
VARIANT_LINE = re.compile(r"^\s*-\s*변형:\s*\S")
ARROW_TYPES = ("값", "상태", "수치")
MATCH_TYPES = ("값", "상태", "화면")
MISSING_SKELETON = re.compile(r"ModuleNotFoundError|ImportError|SyntaxError|command not found|No such file|is not defined|Cannot find module")
# Paths an agent may always change besides its editable paths and tests.
ALWAYS_EDITABLE = (".ai", "Input.md", "AGENTS.md", "test/tmp")
SESSIONS = Path.home() / ".codex" / "sessions"
AGENT_TASK = re.compile(r"(?:^|/)t26_(\d+)$")
JEV = Path.home() / ".codex" / "skills" / "access-jev" / "scripts" / "jev.py"
JEV_TIMEOUT = 40
# Build/test caches are not edits.
GENERATED = re.compile(r"(^|/)(__pycache__|\.pytest_cache|node_modules|\.mypy_cache)(/|$)|\.pyc$")
SEOUL = timezone(timedelta(hours=9))


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
    if stage == DONE_STAGE:
        _fail("validation", "Table26 $H=완료 is written only by `task-complete RAW_ID --project DIR`")
    if stage == SUBMITTED_STAGE:
        _fail("validation", "Table26 $H=검증 is written only by `task-submit RAW_ID --project DIR`")
    if GOAL_FIELD in fields:
        _fail("validation", "Table26 $K (목표) is written only by `task-goal RAW_ID --project DIR --goal-file F`")


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


def delete(table: str, row_id: int) -> object:
    _request("POST", f"/tables/{table}/data/delete", [row_id])
    return {"id": row_id, "deleted": not get(table, ids=[row_id])}


def column_update(table: str, column: str, fields: dict[str, object]) -> object:
    _request("PATCH", f"/tables/{table}/columns", {"columns": [{"id": column, "fields": fields}]})
    found = [c for c in _request("GET", f"/tables/{table}/columns")["columns"] if c["id"] == column]
    if not found:
        _fail("validation", f"{table} has no column {column}")
    return {"id": column, "fields": {k: found[0]["fields"].get(k) for k in fields}}


def _today_epoch() -> int:
    """Today's Asia/Seoul date as a Grist Date cell: UTC midnight in epoch seconds."""
    today = datetime.now(SEOUL).date()
    return int(datetime(today.year, today.month, today.day, tzinfo=timezone.utc).timestamp())


def _parse_ids(text: str) -> list[int]:
    match = TASK_ID_LINE.search(text)
    return [int(n) for n in re.split(r"\s*,\s*", match.group(1))] if match else []


def _find_task_agents(start: Path) -> tuple[list[int], Path] | None:
    for directory in (start, *start.parents):
        agents = directory / "AGENTS.md"
        if agents.is_file():
            ids = _parse_ids(agents.read_text(encoding="utf-8"))
            if ids:
                return ids, directory
    return None


def _project(project: str) -> tuple[Path, str, list[int]]:
    project_dir = Path(project).resolve()
    agents = project_dir / "AGENTS.md"
    if not agents.is_file():
        _fail("validation", f"{agents} does not exist")
    text = agents.read_text(encoding="utf-8")
    ids = _parse_ids(text)
    if not ids:
        _fail("validation", f"{agents} has no `- Table26 task Raw ID:` line")
    return project_dir, text, ids


def _owned(project: str, raw_id: int) -> tuple[Path, str, list[int]]:
    project_dir, text, ids = _project(project)
    if raw_id not in ids:
        _fail("validation", f"{project_dir / 'AGENTS.md'} does not name Table26 task Raw ID {raw_id}")
    return project_dir, text, ids


def _today() -> str:
    return datetime.now(SEOUL).strftime("%y%m%d")


def _next_number(cell: str | None) -> int:
    match = re.match(r"\s*(\d+)\.", cell or "")
    return int(match.group(1)) + 1 if match else 1


def _sha(text: str | bytes) -> str:
    return hashlib.sha256(text.encode("utf-8") if isinstance(text, str) else text).hexdigest()[:16]


def _changes_record(project_dir: Path) -> Path:
    """Today's change record; one file per day, created with `# jobs` when missing."""
    changes = project_dir / ".ai" / "changes"
    existing = sorted(changes.glob(f"{_today()}.*.md"))
    if existing:
        return existing[0]
    changes.mkdir(parents=True, exist_ok=True)
    record = changes / f"{_today()}.작업.md"
    record.write_text("# jobs\n", encoding="utf-8")
    return record


def _append(project_dir: Path, text: str) -> None:
    with _changes_record(project_dir).open("a", encoding="utf-8") as record:
        record.write(text)


def _section(text: str, heading: re.Pattern[str]) -> list[str]:
    """Lines under the first heading matching `heading`, up to the next heading of the same or higher level."""
    found = _sections_in(text, heading)
    return found[0] if found else []


def _sections_in(text: str, heading: re.Pattern[str]) -> list[list[str]]:
    lines = text.splitlines()
    found = []
    for index, line in enumerate(lines):
        if line.startswith("#") and heading.match(line):
            level = len(line) - len(line.lstrip("#"))
            body = []
            for rest in lines[index + 1:]:
                if rest.startswith("#") and len(rest) - len(rest.lstrip("#")) <= level:
                    break
                body.append(rest)
            found.append(body)
    return found


def _records(project_dir: Path, title: str, raw_id: int) -> list[list[str]]:
    """Every `## <title> #raw_id` section across the change records, oldest first."""
    heading = re.compile(rf"^#+\s*{title} #{raw_id}\s*$")
    found = []
    for record in sorted((project_dir / ".ai" / "changes").glob("*.md")):
        found += _sections_in(record.read_text(encoding="utf-8"), heading)
    return found


def _detail(section: list[str]) -> dict:
    for line in section:
        match = DETAIL_LINE.match(line)
        if match:
            return json.loads(match.group(1))
    return {}


# --- caller identity -------------------------------------------------------

def _actor() -> tuple[str, int | None]:
    """("agent", row) for a Codex `t26_<row>` subagent, ("helper", None) for other subagents, else ("main", None)."""
    thread = os.environ.get("CODEX_THREAD_ID")
    if not thread:
        return "main", None
    for rollout in SESSIONS.glob(f"*/*/*/rollout-*-{thread}.jsonl"):
        try:
            with rollout.open(encoding="utf-8") as handle:
                meta = json.loads(handle.readline()).get("payload", {})
        except (OSError, ValueError):
            continue
        path = meta.get("agent_path") or ""
        match = AGENT_TASK.search(path)
        if match:
            return "agent", int(match.group(1))
        if path:
            return "helper", None
    return "main", None


def _require_main(command: str) -> None:
    role, row = _actor()
    if role != "main":
        _fail("actor", f"`{command}` is run by the main session only (caller: {role}{f' t26_{row}' if row else ''})")


def _require_row(command: str, raw_id: int) -> None:
    role, row = _actor()
    if role == "helper" or (role == "agent" and row != raw_id):
        _fail("actor", f"`{command} {raw_id}` is run by the main session or agent t26_{raw_id} only (caller: {role}{f' t26_{row}' if row else ''})")


# --- goal ($K) -------------------------------------------------------------

def _goal_lines(goal: str) -> list[tuple[str, str]]:
    return [(m.group(1), m.group(2)) for line in goal.splitlines() if (m := GOAL_LINE.match(line))]


def _goal_errors(goal: str) -> list[str]:
    errors = []
    typed = _goal_lines(goal)
    if not typed:
        errors.append("no `- [값|상태|수치|불변|화면|사용자] ...` line")
    for line in goal.splitlines():
        if line.strip() and not GOAL_LINE.match(line) and not VARIANT_LINE.match(line):
            errors.append(f"unknown line: {line.strip()}")
    for kind, text in typed:
        if kind in ARROW_TYPES and "→" not in text:
            errors.append(f"[{kind}] needs `입력 → 기대 출력`: {text}")
    if any(kind in ("값", "상태") for kind, _ in typed) and not any(VARIANT_LINE.match(l) for l in goal.splitlines()):
        errors.append("[값]/[상태] goals need a `- 변형: <how the output follows a changed input>` line")
    return errors


def _input_text(project_dir: Path) -> str:
    try:
        return (project_dir / "Input.md").read_text(encoding="utf-8")
    except OSError:
        return ""


def _goal_blocks(text: str) -> tuple[list[str], dict[int, str]]:
    """Input.md lines without its `### 목표 #N` blocks, and each block's goal text by row (last wins).

    A block is the heading plus the goal-format and blank lines right under it; any other line
    (a request appended later) ends the block and stays part of the request text."""
    rest, goals = [], {}
    lines = text.splitlines()
    index = 0
    while index < len(lines):
        heading = GOAL_HEADING.match(lines[index]) if lines[index].startswith("#") else None
        if not heading:
            rest.append(lines[index])
            index += 1
            continue
        index += 1
        body = []
        while index < len(lines) and (not lines[index].strip() or GOAL_LINE.match(lines[index]) or VARIANT_LINE.match(lines[index])):
            body.append(lines[index])
            index += 1
        goals[int(heading.group(1))] = "\n".join(body).strip()
    return rest, goals


def _write_input_goal(project_dir: Path, raw_id: int, goal: str) -> None:
    """Replace row raw_id's `### 목표 #N` block in Input.md, appended at the end."""
    kept, skip = [], False
    for line in _input_text(project_dir).splitlines():
        heading = GOAL_HEADING.match(line) if line.startswith("#") else None
        if heading:
            skip = int(heading.group(1)) == raw_id
            if skip:
                continue
        elif skip and (not line.strip() or GOAL_LINE.match(line) or VARIANT_LINE.match(line)):
            continue
        else:
            skip = False
        kept.append(line)
    body = "\n".join(kept).rstrip()
    (project_dir / "Input.md").write_text(f"{body}\n\n### 목표 #{raw_id}\n{goal}\n", encoding="utf-8")


def _goal(project_dir: Path, row: dict) -> str:
    """Detailed goal of a row: Input.md `### 목표 #N` checked against the `## 목표 #N` sha, else legacy $K."""
    goal = _goal_blocks(_input_text(project_dir))[1].get(row["id"])
    if goal is None:
        return row["K"]
    recorded = [line.split("sha256:", 1)[1].strip() for section in _records(project_dir, "목표", row["id"])
                for line in section if "sha256:" in line]
    if not recorded or recorded[-1] != _sha(goal):
        _fail("goal", f"Input.md `### 목표 #{row['id']}` differs from the latest `## 목표 #{row['id']}` record; "
                      "the main session rewrites it with task-goal while $H=초안")
    return goal


def _latest_request(project_dir: Path) -> str:
    """Body of the last heading section of Input.md (the newest request) without goal blocks, or its tail."""
    text = "\n".join(_goal_blocks(_input_text(project_dir))[0])
    if not text.strip():
        return ""
    sections = re.split(r"(?m)^(?=#)", text)
    for section in reversed(sections):
        if section.strip() and section.strip().count("\n") >= 1:
            return section.strip()[-6000:]
    return text[-6000:]


# --- jev (TypeSafe) judgments, fail-open ----------------------------------

def _jev(command: str, row: int | None = None, **texts: str) -> dict:
    """Run access-jev `jev.py`; any failure returns {"ok": False, "boundary": ...} and never blocks."""
    if not JEV.is_file():
        return {"ok": False, "boundary": "jev-missing"}
    with tempfile.TemporaryDirectory(prefix="jev-") as tmp:
        args = ["python3", str(JEV), command]
        for name, text in texts.items():
            path = Path(tmp) / f"{name}.txt"
            path.write_text(text, encoding="utf-8")
            args += [f"--{name.replace('_', '-')}-file", str(path)]
        if row is not None:
            args += ["--row", str(row)]
        try:
            proc = subprocess.run(args, capture_output=True, text=True, timeout=JEV_TIMEOUT)
        except (OSError, subprocess.TimeoutExpired):
            return {"ok": False, "boundary": "jev-timeout"}
    if proc.returncode != 0:
        try:
            return json.loads(proc.stderr.strip().splitlines()[-1])
        except (ValueError, IndexError):
            return {"ok": False, "boundary": "jev", "error": proc.stderr[-200:]}
    return json.loads(proc.stdout)


def _jev_blocks(result: dict) -> bool:
    return bool(result.get("ok")) and result.get("mode") == "enforce" and result.get("verdict") == "fail"


def _jev_failed(result: dict) -> list[str]:
    return [item.get("line") or item["name"] for item in result.get("items", []) if item.get("verdict") == "fail"]


def _jev_summary(result: dict) -> str:
    if not result.get("ok"):
        return f"jev: {result.get('boundary')} (판정 생략)"
    failed = _jev_failed(result)
    return f"jev {result['mode']} {result['verdict']}" + (f" — 불일치: {'; '.join(failed)}" if failed else "")


# --- git scope ------------------------------------------------------------

def _git(project_dir: Path, *args: str) -> str | None:
    proc = subprocess.run(["git", "-c", "core.quotepath=off", "-C", str(project_dir), *args], capture_output=True, text=True)
    return proc.stdout if proc.returncode == 0 else None


def _dirty(project_dir: Path, base: str) -> set[str]:
    # --relative: paths relative to (and limited to) the project dir inside a monorepo.
    tracked = _git(project_dir, "diff", "--name-only", "--relative", base) or ""
    untracked = _git(project_dir, "ls-files", "--others", "--exclude-standard") or ""
    return {line for line in (tracked + untracked).splitlines() if line and not GENERATED.search(line)}


def _file_sha(project_dir: Path, name: str) -> str | None:
    try:
        return _sha((project_dir / name).read_bytes())
    except OSError:
        return None


def _changed_since(project_dir: Path, base: str, baseline: dict[str, str | None]) -> set[str]:
    """Files differing from `base` now, minus those already dirty at task-red with identical content."""
    return {f for f in _dirty(project_dir, base) if f not in baseline or baseline[f] != _file_sha(project_dir, f)}


def _inside(path: str, prefixes: list[str]) -> bool:
    return any(path == p.rstrip("/") or path.startswith(p.rstrip("/") + "/") for p in prefixes)


def _added_text(project_dir: Path, base: str, files: list[str]) -> str:
    """Lines added since `base` in tracked files plus the content of untracked files."""
    added = []
    diff = (_git(project_dir, "diff", "--relative", base, "--", *files) or "") if files else ""
    added += [l[1:] for l in diff.splitlines() if l.startswith("+") and not l.startswith("+++")]
    untracked = set((_git(project_dir, "ls-files", "--others", "--exclude-standard") or "").splitlines())
    for name in files:
        if name in untracked:
            try:
                added.append((project_dir / name).read_text(encoding="utf-8"))
            except (OSError, UnicodeDecodeError):
                pass
    return "\n".join(added)


def _literals(goal: str) -> list[str]:
    """Expected-output literals of [값]/[상태] lines: quoted strings and numbers of 3+ digits after `→`."""
    found = []
    for kind, text in _goal_lines(goal):
        if kind in ("값", "상태") and "→" in text:
            expected = text.split("→", 1)[1]
            found += [a or b or c for a, b, c in re.findall(r'"([^"]+)"|\'([^\']+)\'|`([^`]+)`', expected)]
            found += re.findall(r"(?<![\d.])\d{3,}(?![\d.])", expected)
    return sorted({f for f in found if len(f) >= 2})


# --- rows and gates -------------------------------------------------------

def _rows(ids: list[int]) -> list[dict]:
    rows = {r["id"]: r["fields"] for r in get(TASK_TABLE, ids=ids, fields=["H", "D", "I", GOAL_FIELD])}
    missing = [i for i in ids if i not in rows]
    if missing:
        _fail("validation", f"Table26 rows {missing} do not exist")
    return [{"id": i, "H": rows[i]["H"], "D": rows[i]["D"], "I": rows[i]["I"], "K": rows[i][GOAL_FIELD] or ""} for i in ids]


def gate(cwd: str, row: int | None = None) -> object:
    found = _find_task_agents(Path(cwd).resolve())
    strip = lambda rows: [{k: r[k] for k in ("id", "H", "D", "I")} for r in rows]
    if row is not None:
        return {"raw_ids": [row], "rows": strip(_rows([row])), "project_dir": str(found[1]) if found else None}
    if not found:
        return {"raw_ids": [], "rows": []}
    ids, project_dir = found
    return {"raw_ids": ids, "rows": strip(_rows(ids)), "project_dir": str(project_dir)}


def _conditions(agents_text: str, raw_id: int, ids: list[int]) -> list[str]:
    """Legacy checklist items of `## 완료 조건` owned by raw_id: `#N text` lines, or untagged lines when the header names one row."""
    owned = []
    for line in _section(agents_text, re.compile(r"^#+\s*완료 조건\s*$")):
        item = CHECK_ITEM.match(line)
        if not item:
            continue
        tagged = ROW_TAG.match(item.group(2))
        if tagged and int(tagged.group(1)) == raw_id:
            owned.append(tagged.group(2))
        elif not tagged and len(ids) == 1:
            owned.append(item.group(2))
    return owned


def _judged(project_dir: Path, heading: re.Pattern[str]) -> dict[str, str | None]:
    """`- [x] text` items under `heading` in today's change records, mapped to the following `근거:` line (or None)."""
    judged: dict[str, str | None] = {}
    for record in sorted((project_dir / ".ai" / "changes").glob(f"{_today()}.*.md")):
        lines = _section(record.read_text(encoding="utf-8"), heading)
        for index, line in enumerate(lines):
            item = CHECK_ITEM.match(line)
            if item and item.group(1) in "xX":
                following = lines[index + 1] if index + 1 < len(lines) else ""
                text = ROW_TAG.match(item.group(2))
                evidence = following.strip() if "근거:" in following and not CHECK_ITEM.match(following) else None
                judged[text.group(2) if text else item.group(2)] = evidence
    return judged


def _run_test(command: str, project_dir: Path) -> tuple[int, str]:
    try:
        proc = subprocess.run(command, shell=True, cwd=project_dir, capture_output=True, text=True, timeout=TEST_TIMEOUT)
    except subprocess.TimeoutExpired:
        return 124, f"timeout after {TEST_TIMEOUT}s"
    return proc.returncode, (proc.stdout + proc.stderr)[-OUTPUT_TAIL:]


def _gate_fail(payload: dict) -> None:
    print(json.dumps({"ok": False, **payload}, ensure_ascii=False, indent=1), file=sys.stderr)
    sys.exit(2)


def task_goal(raw_id: int, project: str, goal_file: str, summary: str) -> object:
    _require_main("task-goal")
    project_dir, _, _ = _owned(project, raw_id)
    row = _rows([raw_id])[0]
    if row["H"] != DRAFT_STAGE:
        _fail("goal", f"Table26 #{raw_id} $H={row['H']}; $K is written only while $H=초안")
    summary = summary.strip()
    if not summary or "\n" in summary:
        _fail("goal", "--summary is the one-line $K goal; the detailed lines go in --goal-file")
    goal = Path(goal_file).read_text(encoding="utf-8").strip()
    errors = _goal_errors(goal)
    if errors:
        _gate_fail({"boundary": "goal", "raw_id": raw_id, "errors": errors})
    judgment = _jev("scope", raw_id, request=_latest_request(project_dir), goal=goal)
    if _jev_blocks(judgment):
        _gate_fail({"boundary": "goal", "raw_id": raw_id, "reason": "goal not grounded in Input.md or incomplete",
                    "jev": judgment.get("items")})
    _write_input_goal(project_dir, raw_id, goal)
    _append(project_dir, f"\n## 목표 #{raw_id}\n- sha256: {_sha(goal)}\n- 판정: {_jev_summary(judgment)}\n")
    return update(TASK_TABLE, raw_id, {GOAL_FIELD: summary})


def task_red(raw_id: int, project: str, command: str, paths: list[str], tests: list[str]) -> object:
    _require_row("task-red", raw_id)
    project_dir, _, _ = _owned(project, raw_id)
    row = _rows([raw_id])[0]
    if row["H"] not in (DRAFT_STAGE, *WORKING_STAGES):
        _fail("red", f"Table26 #{raw_id} $H={row['H']}; task-red needs 초안 or one of {', '.join(WORKING_STAGES)}")
    goal = _goal(project_dir, row)
    if not goal.strip():
        _fail("red", f"Table26 #{raw_id} has no goal; the main session writes it with task-goal first")
    hashes = {}
    for name in tests:
        try:
            hashes[name] = _sha((project_dir / name).read_bytes())
        except OSError:
            _fail("red", f"test file {name} does not exist under {project_dir}")
    code, output = _run_test(command, project_dir)
    if code == 0:
        _gate_fail({"boundary": "red", "raw_id": raw_id, "reason": "the test passes; red needs a failing assertion", "test": command})
    if MISSING_SKELETON.search(output):
        _gate_fail({"boundary": "red", "raw_id": raw_id, "reason": "the test fails before any assertion; write the function skeleton first",
                    "output_tail": output})
    base = (_git(project_dir, "rev-parse", "HEAD") or "").strip() or None
    detail = {"test": command, "tests": hashes, "paths": paths, "goal": _sha(goal), "base": base,
              "dirty": {f: _file_sha(project_dir, f) for f in sorted(_dirty(project_dir, base))} if base else {}}
    stamp = datetime.now(SEOUL).strftime("%H:%M")
    _append(project_dir, f"\n## 빨강 #{raw_id}\n- 테스트: `{command}`\n- 결과: exit {code} ({stamp})\n"
                         f"- 상세: `{json.dumps(detail, ensure_ascii=False)}`\n")
    return update(TASK_TABLE, raw_id, {"H": "작업"}) if row["H"] in (DRAFT_STAGE, "진단") else row


def task_submit(raw_id: int, project: str) -> object:
    _require_row("task-submit", raw_id)
    project_dir, _, ids = _owned(project, raw_id)
    row = _rows([raw_id])[0]
    if row["H"] not in WORKING_STAGES:
        _fail("validation", f"Table26 #{raw_id} $H={row['H']}; task-submit needs one of {', '.join(WORKING_STAGES)}")
    reds = _records(project_dir, "빨강", raw_id)
    red = _detail(reds[-1]) if reds else {}
    if not red:
        _gate_fail({"boundary": "submit", "raw_id": raw_id, "reason": "no `## 빨강` record; run task-red with a failing test first"})
    goal = _goal(project_dir, row)
    if _sha(goal) != red["goal"]:
        _gate_fail({"boundary": "submit", "raw_id": raw_id, "reason": "the goal changed after task-red"})
    changed_tests = [name for name, digest in red["tests"].items()
                     if not (project_dir / name).is_file() or _sha((project_dir / name).read_bytes()) != digest]
    if changed_tests:
        _gate_fail({"boundary": "submit", "raw_id": raw_id, "reason": "test files changed after task-red; rerun task-red while they fail",
                    "tests": changed_tests})

    code, output = _run_test(red["test"], project_dir)
    if code != 0:
        _gate_fail({"boundary": "submit", "raw_id": raw_id, "test": red["test"], "exit": code, "output_tail": output})

    if red.get("base"):
        allowed, test_files = list(ALWAYS_EDITABLE), []
        for other in ids:
            other_reds = _records(project_dir, "빨강", other)
            if other_reds:
                detail = _detail(other_reds[-1])
                allowed += detail.get("paths", [])
                test_files += list(detail.get("tests", {}))
        changed = _changed_since(project_dir, red["base"], red.get("dirty", {}))
        outside = sorted(f for f in changed if not _inside(f, allowed + test_files))
        if outside:
            _gate_fail({"boundary": "submit", "raw_id": raw_id, "reason": "files changed outside the editable paths", "files": outside})
        product = sorted(f for f in changed if f not in red["tests"] and not _inside(f, list(ALWAYS_EDITABLE)))
        added = _added_text(project_dir, red["base"], product)
        hardcoded = [literal for literal in _literals(goal) if literal in added]
        if hardcoded:
            _gate_fail({"boundary": "submit", "raw_id": raw_id, "reason": "expected-output literals of the goal appear in the implementation",
                        "literals": hardcoded})

    judgment = _jev("match", raw_id, goal=goal, output=output)
    if _jev_blocks(judgment):
        note = f"{_next_number(row['I'])}. Jev 불일치: {'; '.join(_jev_failed(judgment))[:200]}"
        update(TASK_TABLE, raw_id, {"H": "교정", "I": note})
        _gate_fail({"boundary": "submit", "raw_id": raw_id, "reason": "actual output does not meet the goal; row set to 교정",
                    "jev": judgment.get("items")})
    stamp = datetime.now(SEOUL).strftime("%H:%M")
    _append(project_dir, f"\n## 제출 #{raw_id}\n- 테스트: `{red['test']}`\n- 결과: exit 0 ({stamp})\n- 판정: {_jev_summary(judgment)}\n")
    return update(TASK_TABLE, raw_id, {"H": SUBMITTED_STAGE})


def _ordered(project_dir: Path, raw_id: int) -> list[tuple[str, list[str]]]:
    """(`제출`|`숨긴 사례`, section) for raw_id in file order across the change records."""
    heading = re.compile(rf"^#+\s*(제출|숨긴 사례) #{raw_id}\s*$")
    found = []
    for record in sorted((project_dir / ".ai" / "changes").glob("*.md")):
        lines = record.read_text(encoding="utf-8").splitlines()
        for index, line in enumerate(lines):
            match = heading.match(line) if line.startswith("#") else None
            if match:
                body = []
                for rest in lines[index + 1:]:
                    if rest.startswith("#"):
                        break
                    body.append(rest)
                found.append((match.group(1), body))
    return found


def _test_line(section: list[str]) -> str | None:
    for line in section:
        match = SUBMIT_TEST.match(line)
        if match:
            return match.group(1)
    return None


def _flow_judged(project_dir: Path, ids: list[int]) -> dict[str, str | None]:
    """`- [x]` items of `## 흐름 판정` sections written after the latest `## 제출` of any row in ids."""
    submit = re.compile(rf"^#+\s*제출 #({'|'.join(map(str, ids))})\s*$")
    flow = re.compile(r"^#+\s*흐름 판정\s*$")
    blocks: list[list[str]] = []
    for record in sorted((project_dir / ".ai" / "changes").glob("*.md")):
        lines = record.read_text(encoding="utf-8").splitlines()
        for index, line in enumerate(lines):
            if not line.startswith("#"):
                continue
            if submit.match(line):
                blocks = []
            elif flow.match(line):
                body = []
                for rest in lines[index + 1:]:
                    if rest.startswith("#"):
                        break
                    body.append(rest)
                blocks.append(body)
    judged: dict[str, str | None] = {}
    for lines in blocks:
        for index, line in enumerate(lines):
            item = CHECK_ITEM.match(line)
            if item and item.group(1) in "xX":
                following = lines[index + 1] if index + 1 < len(lines) else ""
                judged[item.group(2)] = following.strip() if "근거:" in following and not CHECK_ITEM.match(following) else None
    return judged


def flow_wait(project: str, waiting: str) -> object:
    _require_main("flow-wait")
    project_dir, _, ids = _project(project)
    rows = _rows(ids)
    open_rows = [{"id": r["id"], "H": r["H"]} for r in rows if r["H"] not in (SUBMITTED_STAGE, WAITING_STAGE, DONE_STAGE)]
    if open_rows:
        _gate_fail({"boundary": "flow", "reason": "rows not submitted", "rows": open_rows})

    failed, missing_hidden = [], []
    for row in rows:
        if row["H"] != SUBMITTED_STAGE:
            continue
        sections = _ordered(project_dir, row["id"])
        submits = [i for i, (kind, _) in enumerate(sections) if kind == "제출"]
        submitted = _test_line(sections[submits[-1]][1]) if submits else None
        hidden = [_test_line(body) for i, (kind, body) in enumerate(sections) if kind == "숨긴 사례" and submits and i > submits[-1]]
        hidden = [cmd for cmd in hidden if cmd and cmd != submitted]
        if not hidden:
            missing_hidden.append(row["id"])
            continue
        for command in [submitted, hidden[-1]]:
            code, output = _run_test(command, project_dir) if command else (None, "no `## 제출` record")
            if code != 0:
                note = f"{_next_number(row['I'])}. 메인 재실행 실패: {command or '제출 기록 없음'} exit {code}"
                update(TASK_TABLE, row["id"], {"H": "교정", "I": note})
                failed.append({"id": row["id"], "test": command, "exit": code, "output_tail": output})
                break
    if missing_hidden:
        _gate_fail({"boundary": "flow", "reason": "each 검증 row needs a `## 숨긴 사례 #N` (`- 테스트: `CMD``, different from the submitted test) written after its `## 제출`",
                    "rows": missing_hidden})
    if failed:
        _gate_fail({"boundary": "flow", "reason": "submitted or hidden tests failed; rows set to 교정", "rows": failed})

    flow = _flow_judged(project_dir, ids)
    if not flow or not all(flow.values()):
        _gate_fail({"boundary": "flow", "reason": "a `## 흐름 판정` written after the latest `## 제출` needs `- [x] <flow check>` items each followed by `근거:`",
                    "missing_evidence": [k for k, v in flow.items() if not v]})
    goals = {row["id"]: _goal(project_dir, row) for row in rows}
    screen_goal = "\n".join(f"- [화면] {text}" for row in rows for kind, text in _goal_lines(goals[row["id"]]) if kind == "화면")
    if screen_goal:
        judgment = _jev("match", None, goal=screen_goal, output="\n".join(f"{k}\n{v}" for k, v in flow.items()))
        if _jev_blocks(judgment):
            _gate_fail({"boundary": "flow", "reason": "`## 흐름 판정` does not show the [화면] goals", "jev": judgment.get("items")})

    result = []
    for row in rows:
        if row["H"] == SUBMITTED_STAGE:
            users = [text for kind, text in _goal_lines(goals[row["id"]]) if kind == "사용자"]
            note = f"{_next_number(row['D'])}. 흐름 검증 통과, 사용자 확인 대기: {'; '.join(users) or waiting}"
            result.append(update(TASK_TABLE, row["id"], {"H": WAITING_STAGE, "D": note[:300]}))
    return result


def task_complete(raw_id: int, project: str) -> object:
    _require_main("task-complete")
    project_dir, agents_text, ids = _owned(project, raw_id)
    row = _rows([raw_id])[0]
    goal_lines = _goal_lines(_goal(project_dir, row))
    if goal_lines:
        conditions = [f"[{kind}] {text}" for kind, text in goal_lines]
    else:
        conditions = _conditions(agents_text, raw_id, ids)
    if not conditions:
        _fail("validation", f"Table26 #{raw_id} has no $K goal and `## 완료 조건` has no items for it")
    judged = _judged(project_dir, re.compile(rf"^#+\s*완료 판정 #{raw_id}\s*$"))
    missing = [c for c in conditions if c not in judged]
    no_evidence = [c for c in conditions if c in judged and not judged[c]]
    if not missing and not no_evidence and goal_lines:
        checked = [c for c in conditions if c[1:].split("]", 1)[0] in MATCH_TYPES]
        if checked:
            judgment = _jev("match", raw_id, goal="\n".join(f"- {c}" for c in checked),
                            output="\n".join(f"{c}\n{judged[c]}" for c in checked))
            if _jev_blocks(judgment):
                no_evidence = [c for c in checked if c in _jev_failed(judgment)]
    if missing or no_evidence:
        _gate_fail({"boundary": "completion", "raw_id": raw_id,
                    "record": f".ai/changes/{_today()}.*.md ## 완료 판정 #{raw_id}",
                    "unchecked": missing, "missing_evidence": no_evidence})
    return update(TASK_TABLE, raw_id, {"H": DONE_STAGE, "F": _today_epoch()})


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
    p = sub.add_parser("delete"); p.add_argument("table"); p.add_argument("--id", type=int, required=True)
    p = sub.add_parser("column-update"); p.add_argument("table"); p.add_argument("column"); p.add_argument("--fields", required=True)
    p = sub.add_parser("project"); p.add_argument("row_id", type=int)
    p = sub.add_parser("task"); p.add_argument("row_id", type=int)
    p = sub.add_parser("task-update"); p.add_argument("row_id", type=int); p.add_argument("--fields", required=True)
    p = sub.add_parser("task-add"); p.add_argument("--fields", required=True)
    p = sub.add_parser("gate"); p.add_argument("--cwd", default=os.getcwd()); p.add_argument("--row", type=int)
    p = sub.add_parser("task-goal"); p.add_argument("row_id", type=int); p.add_argument("--project", required=True)
    p.add_argument("--goal-file", required=True); p.add_argument("--summary", required=True)
    p = sub.add_parser("task-red"); p.add_argument("row_id", type=int); p.add_argument("--project", required=True)
    p.add_argument("--test", required=True); p.add_argument("--tests", required=True); p.add_argument("--paths", default="")
    p = sub.add_parser("task-submit"); p.add_argument("row_id", type=int); p.add_argument("--project", required=True)
    p = sub.add_parser("flow-wait"); p.add_argument("--project", required=True); p.add_argument("--waiting", default="사용자 확인")
    p = sub.add_parser("task-complete"); p.add_argument("row_id", type=int); p.add_argument("--project", required=True)
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
    elif args.command == "delete":
        result = delete(args.table, args.id)
    elif args.command == "column-update":
        result = column_update(args.table, args.column, _parse_json(args.fields, "--fields"))
    elif args.command == "project":
        result = get(PROJECT_TABLE, ids=[args.row_id])
    elif args.command == "task":
        result = get(TASK_TABLE, ids=[args.row_id])
    elif args.command == "task-update":
        fields = _parse_json(args.fields, "--fields")
        _validate_task_fields(fields)
        result = update(TASK_TABLE, args.row_id, fields)
    elif args.command == "gate":
        result = gate(args.cwd, args.row)
    elif args.command == "task-submit":
        result = task_submit(args.row_id, args.project)
    elif args.command == "task-goal":
        result = task_goal(args.row_id, args.project, args.goal_file, args.summary)
    elif args.command == "task-red":
        split = lambda value: [v.strip() for v in value.split(",") if v.strip()]
        result = task_red(args.row_id, args.project, args.test, split(args.paths), split(args.tests))
    elif args.command == "flow-wait":
        result = flow_wait(args.project, args.waiting)
    elif args.command == "task-complete":
        result = task_complete(args.row_id, args.project)
    else:
        fields = _parse_json(args.fields, "--fields")
        _validate_task_fields(fields)
        result = add(TASK_TABLE, fields)
    print(json.dumps(result, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main(sys.argv[1:])
