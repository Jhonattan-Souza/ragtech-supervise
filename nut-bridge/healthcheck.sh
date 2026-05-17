#!/usr/bin/env bash
set -euo pipefail

dev_path="${DEV_PATH:-/run/nut/ragtech.dev}"
ups_name="${UPS_NAME:-ragtech}"
max_age="${HEALTHCHECK_MAX_DEV_AGE:-30}"

test -s "$dev_path"

age="$(($(date +%s) - $(stat -c %Y "$dev_path")))"
test "$age" -le "$max_age"

timeout 3 upsc "${ups_name}@127.0.0.1:3493" ups.status >/dev/null
