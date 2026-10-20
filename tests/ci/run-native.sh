#!/usr/bin/env bash
# Python can print unraisable finalizer warnings while returning success.
set -uo pipefail
label=$1
shift
mkdir -p artifacts/native-command-logs
log=$(mktemp "artifacts/native-command-logs/${label}.XXXXXX.log")
"$@" 2>&1 | tee "$log"
statuses=("${PIPESTATUS[@]}")
if [[ ${statuses[0]} != 0 ]]; then exit "${statuses[0]}"; fi
if [[ ${statuses[1]} != 0 ]]; then exit "${statuses[1]}"; fi
if grep -Eq '(^|[[:space:]])ResourceWarning:|^Exception ignored (in|while finalizing)' "$log"; then
  echo "Native test emitted an unraisable warning; see $log" >&2
  exit 1
fi
