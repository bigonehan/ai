#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${1:-.}"
TASK_NAME="${2:-recursive_orc_improve}"
PLAN_REQUEST="${3:-/plan rust-write 규칙 우선순위 준수 여부를 점검하고 게이트를 통과한 뒤 작업 계획을 제시해줘}"
MAX_RETRY="${ORC_RECURSIVE_MAX_RETRY:-0}" # 0 means infinite loop

ROOT_DIR="${ROOT_DIR%/}"
TRACE_DIR="${ROOT_DIR}/.project"
TRACE_FILE="${TRACE_DIR}/orc_gate_trace.log"
PIPELINE_LOG_FILE="${TRACE_DIR}/orc_pipeline.log"
CHECK_LOG_FILE="${ROOT_DIR}/job.md"
CHECK_LOG_SECTION="## orc_recursive_log"
GLOBAL_OVERRIDE_FILE="/home/tree/ai/codex/AGENTS.override.md"
SCRIPT_HOME="/home/tree/ai/codex/script"
PRECHECK_SCRIPT="${SCRIPT_HOME}/orc_gate_preflight.sh"
LOCK_SCRIPT="${SCRIPT_HOME}/orc_task_lock.py"
CODEX_CMD="${CODEX_CMD:-codex}"
CLIT_MODE="${ORC_CLIT_MODE:-gate_priority_recursive_improve}"
STEP_TIMEOUT_SEC="${ORC_STAGE_TIMEOUT_SEC:-180}"
STEP_RETRY_MAX="${ORC_STAGE_RETRY_MAX:-2}"
LOCAL_OVERRIDE_FILE="${ROOT_DIR}/AGENTS.override.md"
LOCAL_RULE_FILE="${ROOT_DIR}/AGENTS.md"
CONFIG_FILE="${ROOT_DIR}/configs/configs.yaml"

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

log_check() {
  local line="$1"
  mkdir -p "${TRACE_DIR}"
  if [[ ! -f "${CHECK_LOG_FILE}" ]]; then
    printf '# task\n\n# problems\n\n# check\n' > "${CHECK_LOG_FILE}"
  fi
  if ! rg -q --fixed-strings "${CHECK_LOG_SECTION}" "${CHECK_LOG_FILE}"; then
    printf '\n%s\n' "${CHECK_LOG_SECTION}" >> "${CHECK_LOG_FILE}"
  fi
  printf -- '- [%s] %s\n' "$(date '+%s')" "${line}" >> "${CHECK_LOG_FILE}"
}

lock_job_task() {
  python3 "${LOCK_SCRIPT}" "${ROOT_DIR}/job.md" "${TASK_NAME}" || fail "failed to lock job task"
  log_trace "job_task_locked"
}

load_gate_inputs() {
  cat "${GLOBAL_OVERRIDE_FILE}" >/dev/null || fail "failed to read global override: ${GLOBAL_OVERRIDE_FILE}"
  if [[ -f "${LOCAL_OVERRIDE_FILE}" ]]; then
    cat "${LOCAL_OVERRIDE_FILE}" >/dev/null || fail "failed to read local override: ${LOCAL_OVERRIDE_FILE}"
  else
    cat "${LOCAL_RULE_FILE}" >/dev/null || fail "failed to read local rule: ${LOCAL_RULE_FILE}"
  fi
  cat "${CONFIG_FILE}" >/dev/null || fail "failed to read config: ${CONFIG_FILE}"
  cat "${PRECHECK_SCRIPT}" >/dev/null || fail "failed to read precheck script: ${PRECHECK_SCRIPT}"
  log_trace "global_override_read"
}

run_preflight() {
  log_trace "run_preflight"
  "${PRECHECK_SCRIPT}" "${ROOT_DIR}" >/dev/null
}

run_with_retry() {
  local step="$1"
  shift
  local attempt=1
  while [[ "${attempt}" -le "${STEP_RETRY_MAX}" ]]; do
    if timeout "${STEP_TIMEOUT_SEC}" "$@" >>"${PIPELINE_LOG_FILE}" 2>&1; then
      return 0
    fi
    log_check "${step} failed attempt ${attempt}/${STEP_RETRY_MAX}: $*"
    if [[ -f "${PIPELINE_LOG_FILE}" ]]; then
      log_check "${step} tail: $(tail -n 20 "${PIPELINE_LOG_FILE}" | tr '\n' ' ' | cut -c1-500)"
    fi
    attempt=$((attempt + 1))
  done
  return 1
}

open_worker_and_send_plan() {
  local pane_id
  local signal
  pane_id="$(tmux split-window -h -P -F '#{pane_id}' fish -i)"
  [[ -n "${pane_id}" ]] || fail "failed to create worker pane"
  log_trace "tmux_pane_created:${pane_id}"

  signal="orc_plan_done_${TASK_NAME}_$$_$(date +%s)"
  signal="${signal//[^a-zA-Z0-9_]/_}"

  local cmd
  cmd="${CODEX_CMD} --ask-for-approval never \"${PLAN_REQUEST}\"; tmux wait-for -S ${signal}"
  orc send-tmux "${pane_id}" "${cmd}" enter >/dev/null
  log_trace "orc_send_tmux_plan:${pane_id}"
  orc send-tmux "$(tmux display-message -p '#{pane_id}')" "echo recursive:worker_started:${pane_id}" enter >/dev/null

  if ! timeout "${STEP_TIMEOUT_SEC}" tmux wait-for "${signal}"; then
    log_check "plan wait timeout: pane=${pane_id} signal=${signal}"
    return 1
  fi
  echo "${pane_id}"
}

run_orc_pipeline() {
  (
    cd "${ROOT_DIR}"

    run_with_retry "add_orc_drafts" orc add_orc_drafts >/dev/null || return 1
    log_trace "stage_drafts_done"

    log_trace "stage_draft_item_done"

    run_with_retry "impl_orc_code" orc impl_orc_code >/dev/null || return 1
    log_trace "stage_impl_done"

    run_with_retry "check_orc_code" orc check_orc_code >/dev/null || return 1
    run_with_retry "clit_test" orc clit clit test -p . -m "${CLIT_MODE}" >/dev/null || return 1
    log_trace "stage_check_done"
  )
}

ensure_tmux
mkdir -p "${TRACE_DIR}"
: > "${TRACE_FILE}"
: > "${PIPELINE_LOG_FILE}"
log_trace "session_start:$$"

attempt=0
while true; do
  attempt=$((attempt + 1))
  if [[ "${MAX_RETRY}" -gt 0 && "${attempt}" -gt "${MAX_RETRY}" ]]; then
    fail "max retry reached (${MAX_RETRY})"
  fi

  load_gate_inputs

  orc init_orc_job >/dev/null 2>&1 || true
  log_trace "orc_init_orc_job"

  lock_job_task

  if ! run_preflight; then
    log_check "preflight gate failed at attempt ${attempt}: implementation blocked"
    log_trace "preflight_failed_blocked"
    continue
  fi

  if ! worker="$(open_worker_and_send_plan)"; then
    log_check "plan stage failed at attempt ${attempt}"
    continue
  fi
  log_trace "stage_plan_done"

  if ! run_orc_pipeline; then
    log_check "pipeline stage failed at attempt ${attempt}"
    orc send-tmux "${worker}" "recursive:retry:${attempt}" enter >/dev/null || true
    continue
  fi

  if ! "${PRECHECK_SCRIPT}" "${ROOT_DIR}" pipeline >/dev/null; then
    log_check "pipeline preflight failed at attempt ${attempt}"
    orc send-tmux "${worker}" "recursive:retry:${attempt}" enter >/dev/null || true
    continue
  fi

  log_check "recursive success at attempt ${attempt}: job.md exists | #task locked | preflight pass"
  echo "[orc_recursive_improve] OK: attempt=${attempt} worker=${worker}"
  exit 0
done
