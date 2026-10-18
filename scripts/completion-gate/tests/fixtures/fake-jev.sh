#!/bin/sh
# Fake jev CLI: logs `<cmd> <flag>` and the message, then prints FAKE_JEV_JSON.
[ -n "$FAKE_JEV_LOG" ] && { printf '%s %s\n' "$1" "$2" >> "$FAKE_JEV_LOG"; cat "$3" >> "$FAKE_JEV_LOG"; printf '\n' >> "$FAKE_JEV_LOG"; }
printf '%s' "$FAKE_JEV_JSON"
exit "${FAKE_JEV_EXIT:-0}"
