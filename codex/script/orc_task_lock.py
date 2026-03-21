#!/usr/bin/env python3
import re
import sys
from pathlib import Path


def replace_task_sections(text: str, task_name: str) -> str:
    pattern = re.compile(r"(?ms)^# task\s*\n(.*?)(?=^# |\Z)")
    match = pattern.search(text)
    if not match:
        raise ValueError("missing # task section")

    section = "\n".join(
        [
            "# task",
            "## planned",
            "",
            "## work",
            f"- {task_name}",
            "",
            "## check",
            "",
            "## completed",
            "",
            "## fail",
            "",
        ]
    )

    start, end = match.span()
    suffix = text[end:]
    return text[:start] + section + suffix


def main() -> int:
    if len(sys.argv) < 3:
        print("usage: orc_task_lock.py <job.md path> <task name>", file=sys.stderr)
        return 2

    path = Path(sys.argv[1])
    task_name = sys.argv[2].strip()
    if not path.exists():
        print(f"job.md not found: {path}", file=sys.stderr)
        return 2
    if not task_name:
        print("task name is empty", file=sys.stderr)
        return 2

    body = path.read_text(encoding="utf-8")
    updated = replace_task_sections(body, task_name)
    path.write_text(updated, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
