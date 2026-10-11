#!/usr/bin/env python3
"""TypeSafe Jev judgments for the Table26 gates, without handling the API key.

Called by grist-access `grist.py` and the Codex completion hook, not by Codex
directly. The key is read from this skill's `.env` (or the environment) and is
never printed. All output is JSON on stdout.

Usage:
  jev.py scope --request-file F --goal-file G [--row N]   # J3: goal lines grounded in the request, request covered
  jev.py match --goal-file G --output-file O [--row N]    # J1: actual output satisfies each [값]/[상태]/[화면] line
  jev.py stop --message-file M                            # J2: classify the final assistant message

Verdicts: a Noul value (or Choice confidence) >= JEV_PASS is `pass`, < JEV_FLOOR
is `fail`, anything between is `uncertain`. JEV_MODE=shadow (default) tells the
caller to log only; `enforce` lets it reject on `fail`.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

SKILL_DIR = Path(__file__).resolve().parents[1]
API_URL = "https://api.typesafe.ai/v1/systemone"
LOG_FILE = Path.home() / ".codex/tmp/jev/judgments.jsonl"
STATE_LIMIT = 12000
TIMEOUT = 20
GOAL_LINE = re.compile(r"^\s*-\s*\[(값|상태|수치|불변|화면|사용자)\]\s*(.+?)\s*$")
MATCH_TYPES = ("값", "상태", "화면")
STOP_CATEGORIES = {
    "완료 보고": "The work is finished and the message reports the verified result.",
    "사용자 질문": "The message asks the user a question or asks the user to perform an action.",
    "실패를 결론으로 종료": "The message stops because a tool, browser, CDP or runtime step failed and presents that failure as the outcome.",
    "진행 보고": "The message reports progress while work is still continuing.",
}
SECRET = re.compile(r"(Bearer\s+\S+|\b[A-Z0-9_]*(?:KEY|TOKEN|SECRET)[A-Z0-9_]*\s*=\s*\S+)", re.I)


def _load_env() -> dict[str, str]:
    values: dict[str, str] = {}
    env_file = Path(os.environ.get("JEV_ENV_FILE", SKILL_DIR / ".env"))
    try:
        for line in env_file.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if line and not line.startswith("#") and "=" in line:
                key, value = line.split("=", 1)
                values[key.strip()] = value.strip().strip('"').strip("'")
    except FileNotFoundError:
        pass
    for key in ("TYPESAFE_API_KEY", "JEV_MODEL", "JEV_MODE", "JEV_PASS", "JEV_FLOOR"):
        if os.environ.get(key):
            values[key] = os.environ[key]
    return values


ENV = _load_env()
MODE = ENV.get("JEV_MODE", "shadow")
PASS = float(ENV.get("JEV_PASS", "0.9"))
FLOOR = float(ENV.get("JEV_FLOOR", "0.5"))


def _fail(boundary: str, message: str) -> None:
    # Never include the key or headers in error output.
    print(json.dumps({"ok": False, "boundary": boundary, "error": message, "mode": MODE}, ensure_ascii=False), file=sys.stderr)
    sys.exit(2)


def _clean(text: str) -> str:
    text = SECRET.sub("[masked]", text).replace(str(Path.home()), "~")
    return text[-STATE_LIMIT:]


def _call(state: str, questions: dict) -> dict:
    fake = os.environ.get("JEV_FAKE_RESPONSE")  # test hook: a JSON file shaped like the API response
    if fake:
        return json.loads(Path(fake).read_text(encoding="utf-8"))
    key = ENV.get("TYPESAFE_API_KEY", "")
    if not key:
        _fail("config", f"TYPESAFE_API_KEY is not set (expected in {SKILL_DIR / '.env'})")
    body = {"model": ENV.get("JEV_MODEL", "jev-latest"), "state": state, "questions": questions}
    request = urllib.request.Request(
        API_URL, data=json.dumps(body, ensure_ascii=False).encode("utf-8"), method="POST",
        headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=TIMEOUT) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        _fail(f"http_{exc.code}", exc.read().decode("utf-8", "replace")[:300])
    except (urllib.error.URLError, TimeoutError, OSError) as exc:
        _fail("connection", str(getattr(exc, "reason", exc))[:300])


def _grade(value: float | None) -> str:
    if value is None:
        return "uncertain"
    return "pass" if value >= PASS else "fail" if value < FLOOR else "uncertain"


def _overall(items: list[dict]) -> str:
    grades = {item["verdict"] for item in items}
    return "fail" if "fail" in grades else "uncertain" if "uncertain" in grades else "pass"


def _noul_items(response: dict, names: dict[str, str]) -> list[dict]:
    answers = response.get("answers", {})
    items = []
    for name, label in names.items():
        value = answers.get(name, {}).get("noul")
        items.append({"name": name, "line": label, "value": value, "verdict": _grade(value)})
    return items


def _log(command: str, row: int | None, state: str, result: dict) -> None:
    try:
        LOG_FILE.parent.mkdir(parents=True, exist_ok=True)
        entry = {"at": datetime.now(timezone.utc).isoformat(timespec="seconds"), "command": command, "row": row,
                 "state_sha256": hashlib.sha256(state.encode("utf-8")).hexdigest(), **result}
        with LOG_FILE.open("a", encoding="utf-8") as log:
            log.write(json.dumps(entry, ensure_ascii=False) + "\n")
    except OSError:
        pass  # logging never changes a judgment


def _goal_lines(goal: str, types: tuple[str, ...] | None = None) -> list[tuple[str, str]]:
    lines = [(m.group(1), m.group(2)) for line in goal.splitlines() if (m := GOAL_LINE.match(line))]
    return [line for line in lines if types is None or line[0] in types]


def scope(request: str, goal: str) -> dict:
    lines = _goal_lines(goal)
    names = {f"g{i}": f"[{kind}] {text}" for i, (kind, text) in enumerate(lines, 1)}
    questions = {name: {"type": "noul", "instructions": f"이 목표 조건은 위 사용자 요청에 명시되었거나 요청에서 직접 따라 나온다: {label}"}
                 for name, label in names.items()}
    questions["coverage"] = {"type": "noul", "instructions": "사용자 요청의 모든 요구가 아래 목표 조건들에 빠짐없이 반영되어 있다:\n" + goal}
    state = _clean(f"# 사용자 요청\n{request}\n\n# 목표\n{goal}")
    response = _call(state, questions)
    items = _noul_items(response, {**names, "coverage": "요청 전체 반영"})
    return {"command": "scope", "state": state, "items": items, "usage": response.get("usage")}


def match(goal: str, output: str) -> dict:
    lines = _goal_lines(goal, MATCH_TYPES)
    names = {f"g{i}": f"[{kind}] {text}" for i, (kind, text) in enumerate(lines, 1)}
    if not names:
        return {"command": "match", "state": "", "items": [], "usage": None}
    questions = {name: {"type": "noul", "instructions": f"위 실제 출력은 이 목표 조건을 충족한다: {label}"}
                 for name, label in names.items()}
    state = _clean(f"# 실제 출력\n{output}")
    response = _call(state, questions)
    return {"command": "match", "state": state, "items": _noul_items(response, names), "usage": response.get("usage")}


def stop(message: str) -> dict:
    questions = {
        "category": {"type": "choice", "instructions": "이 AI 작업 에이전트의 마지막 메시지는 어떤 종류인가?", "criteria": STOP_CATEGORIES},
        "allowed_action": {"type": "noul", "instructions": "메시지가 사용자에게 요청하는 동작은 로그인·인증, 결제·비용, 되돌릴 수 없는 외부 전송, 브라우저 밖 OS 앱 조작 중 하나다"},
    }
    state = _clean(f"# 마지막 메시지\n{message}")
    response = _call(state, questions)
    answer = response.get("answers", {}).get("category", {})
    items = [{"name": "category", "value": answer.get("choice"), "confidence": answer.get("confidence"),
              "verdict": _grade(answer.get("confidence"))}]
    if answer.get("choice") == "사용자 질문":  # allowed_action only means something when the user is asked
        items += _noul_items(response, {"allowed_action": "허용된 사용자 동작"})
    return {"command": "stop", "state": state, "items": items, "category": answer.get("choice"), "usage": response.get("usage")}


def main(argv: list[str]) -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("scope"); p.add_argument("--request-file", required=True); p.add_argument("--goal-file", required=True)
    p.add_argument("--row", type=int)
    p = sub.add_parser("match"); p.add_argument("--goal-file", required=True); p.add_argument("--output-file", required=True)
    p.add_argument("--row", type=int)
    p = sub.add_parser("stop"); p.add_argument("--message-file", required=True)
    args = parser.parse_args(argv)

    def read(path: str) -> str:
        try:
            return Path(path).read_text(encoding="utf-8")
        except OSError as exc:
            _fail("arguments", f"cannot read {path}: {exc.strerror}")

    if args.command == "scope":
        result = scope(read(args.request_file), read(args.goal_file))
    elif args.command == "match":
        result = match(read(args.goal_file), read(args.output_file))
    else:
        result = stop(read(args.message_file))
    state = result.pop("state")
    result = {"ok": True, "mode": MODE, "verdict": _overall(result["items"]) if result["items"] else "pass", **result}
    _log(args.command, getattr(args, "row", None), state, {k: result[k] for k in ("mode", "verdict", "items")})
    print(json.dumps(result, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main(sys.argv[1:])
