#!/bin/sh
# Fake grist CLI: logs each call and answers sync/gate from FAKE_* variables.
[ -n "$FAKE_GRIST_LOG" ] && printf '%s\n' "$*" >> "$FAKE_GRIST_LOG"
case "$1" in
  sync)
    [ -n "$FAKE_SYNC_ERR" ] && printf '%s\n' "$FAKE_SYNC_ERR" >&2
    exit "${FAKE_SYNC_EXIT:-0}" ;;
  gate)
    [ -n "$FAKE_GATE_ERR" ] && printf '%s\n' "$FAKE_GATE_ERR" >&2
    printf '%s' "$FAKE_GATE_JSON"
    exit "${FAKE_GATE_EXIT:-0}" ;;
  *) exit 0 ;;
esac
