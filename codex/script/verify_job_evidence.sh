#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${1:-.}"
JOB_FILE="${ROOT_DIR%/}/job.md"

fail() {
  echo "[verify_job_evidence] FAIL: $1" >&2
  exit 1
}

require_heading() {
  local heading="$1"
  if ! rg -n "^${heading}$" "${JOB_FILE}" >/dev/null 2>&1; then
    fail "missing heading: ${heading}"
  fi
}

if [[ ! -f "${JOB_FILE}" ]]; then
  fail "job.md not found at ${JOB_FILE}"
fi

require_heading "# requirement"
require_heading "# task"
require_heading "# problems"
require_heading "## planned"
require_heading "## work"
require_heading "## check"
require_heading "## completed"
require_heading "## fail"

task_bullets="$(
  python3 - "${JOB_FILE}" <<'PY'
import re
import sys
from pathlib import Path

text = Path(sys.argv[1]).read_text(encoding="utf-8")
section = re.search(r"(?ms)^# task\s*\n(.*?)(?:^# |\Z)", text)
if not section:
    print(0)
    raise SystemExit(0)
count = 0
for line in section.group(1).splitlines():
    if re.match(r"^\s*-\s+\S", line):
        count += 1
print(count)
PY
)"

if [[ "${task_bullets}" -lt 1 ]]; then
  fail "task section has no locked item"
fi

echo "[verify_job_evidence] OK: ${JOB_FILE} (task_bullets=${task_bullets})"
