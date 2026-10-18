#!/bin/sh
# Fake nf: logs its arguments.
printf '%s\n' "$*" >> "$FAKE_NF_LOG"
