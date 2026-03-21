#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${1:-.}"
ROOT_DIR="${ROOT_DIR%/}"

SCRIPT_HOME="/home/tree/ai/codex/script"
PIPELINE_PREFLIGHT="${SCRIPT_HOME}/orc_gate_preflight.sh"
DRAFTS_FILE="${ROOT_DIR}/.project/drafts.yaml"

fail() {
  echo "[orc_pipeline_assert] FAIL: $1" >&2
  exit 1
}

[[ -x "${PIPELINE_PREFLIGHT}" ]] || fail "missing preflight script: ${PIPELINE_PREFLIGHT}"
"${PIPELINE_PREFLIGHT}" "${ROOT_DIR}" pipeline >/dev/null
[[ -f "${DRAFTS_FILE}" ]] || fail "missing drafts file: ${DRAFTS_FILE}"

echo "[orc_pipeline_assert] OK: root=${ROOT_DIR}"
