#!/usr/bin/env bash
set -euo pipefail

dev_path="${DEV_PATH:-/run/nut/ragtech.dev}"
ups_name="${UPS_NAME:-ragtech}"
max_age="${HEALTHCHECK_MAX_DEV_AGE:-30}"
require_valid_sample="${HEALTHCHECK_REQUIRE_VALID_SAMPLE:-1}"

test -s "$dev_path"

age="$(($(date +%s) - $(stat -c %Y "$dev_path")))"
test "$age" -le "$max_age"

upsc_value() {
  timeout 3 upsc "${ups_name}@127.0.0.1:3493" "$1"
}

upsc_value ups.status >/dev/null

if [[ "$require_valid_sample" == "1" ]]; then
  sample_valid="$(upsc_value ragtech.sample.valid)"
  test "$sample_valid" = "1"
fi
