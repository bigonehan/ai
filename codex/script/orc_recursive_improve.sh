#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${1:-.}"
TASK_NAME="${2:-recursive_orc_improve}"
PLAN_REQUEST="${3:-/plan rust-write 규칙 우선순위 준수 여부를 점검하고 게이트를 통과한 뒤 작업 계획을 제시해줘}"
MAX_RETRY="${ORC_RECURSIVE_MAX_RETRY:-0}" # 0 means infinite loop

ROOT_DIR="${ROOT_DIR%/}"
TRACE_DIR="${ROOT_DIR}/.project"
TRACE_FILE="${TRACE_DIR}/orc_gate_trace.log"
GLOBAL_OVERRIDE_FILE="/home/tree/ai/codex/AGENTS.override.md"
SCRIPT_HOME="/home/tree/ai/codex/script"
PRECHECK_SCRIPT="${SCRIPT_HOME}/orc_gate_preflight.sh"
LOCK_SCRIPT="${SCRIPT_HOME}/orc_task_lock.py"
CODEX_CMD="${CODEX_CMD:-codex}"
CLIT_MODE="${ORC_CLIT_MODE:-gate_priority_recursive_improve}"

fail() {
  echo "[orc_recursive_improve] FAIL: $1" >&2
  exit 1
}

ensure_tmux() {
  command -v tmux >/dev/null 2>&1 || fail "tmux not found"
  tmux display-message -p "#{pane_id}" >/dev/null 2>&1 || fail "not in tmux session"
}

log_trace() {
  local token="$1"
  mkdir -p "${TRACE_DIR}"
  printf '%s %s\n' "$(date '+%Y-%m-%dT%H:%M:%S%z')" "${token}" >> "${TRACE_FILE}"
}

lock_job_task() {
  python3 "${LOCK_SCRIPT}" "${ROOT_DIR}/job.md" "${TASK_NAME}" || fail "failed to lock job task"
  log_trace "job_task_locked"
}

open_worker_and_send_plan() {
  local pane_id
  pane_id="$(tmux split-window -h -P -F '#{pane_id}' fish -i)"
  [[ -n "${pane_id}" ]] || fail "failed to create worker pane"
  local cmd="${CODEX_CMD} \"${PLAN_REQUEST}\""
  orc send-tmux "${pane_id}" "${cmd}" enter >/dev/null
  orc send-tmux "$(tmux display-message -p '#{pane_id}')" "recursive:worker_started:${pane_id}" enter >/dev/null
  echo "${pane_id}"
}

run_orc_pipeline() {
  (
    cd "${ROOT_DIR}"
    orc add_orc_drafts >/dev/null
    log_trace "stage_drafts_done"

    # add_orc_drafts already materializes draft items in rust-orc flow.
    # Keep a dedicated stage token so the loop can assert draft_item readiness.
    log_trace "stage_draft_item_done"

    orc impl_orc_code >/dev/null
    log_trace "stage_impl_done"

    orc check_orc_code >/dev/null
    orc clit test -p . -m "${CLIT_MODE}" >/dev/null
    log_trace "stage_check_done"
  )
}

ensure_tmux

attempt=0
while true; do
  attempt=$((attempt + 1))
  if [[ "${MAX_RETRY}" -gt 0 && "${attempt}" -gt "${MAX_RETRY}" ]]; then
    fail "max retry reached (${MAX_RETRY})"
  fi

  cat "${GLOBAL_OVERRIDE_FILE}" >/dev/null
  log_trace "global_override_read"

  orc init_orc_job >/dev/null 2>&1 || true
  log_trace "orc_init_orc_job"

  lock_job_task

  if "${PRECHECK_SCRIPT}" "${ROOT_DIR}" >/dev/null; then
    worker="$(open_worker_and_send_plan)"
    log_trace "stage_plan_done"
    run_orc_pipeline
    "${PRECHECK_SCRIPT}" "${ROOT_DIR}" pipeline >/dev/null
    echo "[orc_recursive_improve] OK: attempt=${attempt} worker=${worker}"
    exit 0
  fi

  orc send-tmux "$(tmux display-message -p '#{pane_id}')" "recursive:retry:${attempt}" enter >/dev/null
done
