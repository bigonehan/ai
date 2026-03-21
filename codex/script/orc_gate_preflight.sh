#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${1:-.}"
ROOT_DIR="${ROOT_DIR%/}"
MODE="${2:-gate}"
GLOBAL_OVERRIDE_FILE="/home/tree/ai/codex/AGENTS.override.md"
SCRIPT_HOME="/home/tree/ai/codex/script"
VERIFY_SCRIPT="${SCRIPT_HOME}/verify_job_evidence.sh"

abs_path() {
  python3 - "$1" <<'PY'
import pathlib
import sys
print(pathlib.Path(sys.argv[1]).resolve())
PY
}

ROOT_DIR="$(abs_path "${ROOT_DIR}")"
JOB_FILE="${ROOT_DIR}/job.md"
TRACE_FILE="${ROOT_DIR}/.project/orc_gate_trace.log"
LOCAL_OVERRIDE_FILE="${ROOT_DIR}/AGENTS.override.md"
LOCAL_RULE_FILE="${LOCAL_OVERRIDE_FILE}"
DRAFTS_FILE="${ROOT_DIR}/.project/drafts.yaml"

fail() {
  echo "[orc_gate_preflight] FAIL: $1" >&2
  exit 1
}

fail_root_markers() {
  local reason="$1"
  echo "[orc_gate_preflight] FAIL: ${reason}" >&2
  echo "[orc_gate_preflight] HINT: run from repo root (expected markers: AGENTS.md or AGENTS.override.md, job.md, .project/orc_gate_trace.log)" >&2
  exit 1
}

require_file() {
  local file_path="$1"
  local label="$2"
  [[ -f "${file_path}" ]] || fail "${label} not found: ${file_path}"
}

require_dir() {
  local dir_path="$1"
  [[ -d "${dir_path}" ]] || fail_root_markers "root dir not found: ${dir_path}"
}

require_trace() {
  local token="$1"
  rg -n --fixed-strings "${token}" "${TRACE_FILE}" >/dev/null 2>&1 \
    || fail "missing trace token: ${token}"
}

task_bullet_count() {
  python3 - "$JOB_FILE" <<'PY'
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
section = re.search(r"(?ms)^# task\s*\n(.*?)(?:^# |\Z)", text)
if not section:
    print("0")
    raise SystemExit(0)
count = 0
for line in section.group(1).splitlines():
    if re.match(r"^\s*-\s+\S", line):
        count += 1
print(count)
PY
}

require_file "${GLOBAL_OVERRIDE_FILE}" "global AGENTS override"
require_dir "${ROOT_DIR}"
if [[ ! -f "${LOCAL_OVERRIDE_FILE}" && -f "${ROOT_DIR}/AGENTS.md" ]]; then
  LOCAL_RULE_FILE="${ROOT_DIR}/AGENTS.md"
fi
if [[ ! -f "${LOCAL_RULE_FILE}" ]]; then
  fail_root_markers "local AGENTS rule not found: ${LOCAL_RULE_FILE}"
fi
require_file "${JOB_FILE}" "job.md"
require_file "${TRACE_FILE}" "orc gate trace"
require_file "${VERIFY_SCRIPT}" "verify_job_evidence.sh"

[[ -r "${GLOBAL_OVERRIDE_FILE}" ]] || fail "global AGENTS override unreadable: ${GLOBAL_OVERRIDE_FILE}"
[[ -r "${LOCAL_RULE_FILE}" ]] || fail "local AGENTS rule unreadable: ${LOCAL_RULE_FILE}"

require_trace "global_override_read"
require_trace "orc_init_orc_job"
require_trace "job_task_locked"

"${VERIFY_SCRIPT}" "${ROOT_DIR}" >/dev/null

BULLET_COUNT="$(task_bullet_count)"
if [[ "${BULLET_COUNT}" -lt 1 ]]; then
  fail "job.md#task is not locked (no bullet item in task sections)"
fi

if [[ "${MODE}" == "pipeline" ]]; then
  require_trace "stage_plan_done"
  require_trace "stage_drafts_done"
  require_trace "stage_draft_item_done"
  require_trace "stage_impl_done"
  require_trace "stage_check_done"
  [[ -f "${DRAFTS_FILE}" ]] || fail "drafts file missing after pipeline: ${DRAFTS_FILE}"
fi

echo "[orc_gate_preflight] OK: root=${ROOT_DIR} mode=${MODE} bullets=${BULLET_COUNT}"
