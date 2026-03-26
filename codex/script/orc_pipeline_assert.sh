#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${1:-.}"
ROOT_DIR="${ROOT_DIR%/}"
TRACE_FILE="${ROOT_DIR}/.project/orc_gate_trace.log"
JOB_FILE="${ROOT_DIR}/job.md"

fail() {
  echo "[orc_pipeline_assert] FAIL: $1" >&2
  exit 1
}

[[ -f "${TRACE_FILE}" ]] || fail "missing trace file: ${TRACE_FILE}"
[[ -f "${JOB_FILE}" ]] || fail "missing job.md: ${JOB_FILE}"

required_tokens=(
  "stage_plan_done"
  "stage_drafts_done"
  "stage_impl_done"
  "stage_check_done"
)

for token in "${required_tokens[@]}"; do
  if ! rg -q --fixed-strings "${token}" "${TRACE_FILE}"; then
    fail "missing trace token: ${token}"
  fi
done

if ! rg -q "# task" "${JOB_FILE}"; then
  fail "job.md missing # task section"
fi

echo "[orc_pipeline_assert] OK: trace and job evidence verified"
