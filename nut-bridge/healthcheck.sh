#!/usr/bin/env bash
set -euo pipefail

dev_path="${DEV_PATH:-/run/nut/ragtech.dev}"
ups_name="${UPS_NAME:-ragtech}"
max_age="${HEALTHCHECK_MAX_DEV_AGE:-30}"
require_valid_sample="${HEALTHCHECK_REQUIRE_VALID_SAMPLE:-1}"
listen_address="${NUT_LISTEN_ADDRESS:-0.0.0.0}"

if [[ ! "$max_age" =~ ^[0-9]+$ ]]; then
  echo "[healthcheck] HEALTHCHECK_MAX_DEV_AGE must be a non-negative integer" >&2
  exit 1
fi

if [[ ! "$require_valid_sample" =~ ^[01]$ ]]; then
  echo "[healthcheck] HEALTHCHECK_REQUIRE_VALID_SAMPLE must be 0 or 1" >&2
  exit 1
fi

test -s "$dev_path"

age="$(($(date +%s) - $(stat -c %Y "$dev_path")))"
test "$age" -le "$max_age"

healthcheck_address="$listen_address"
case "$healthcheck_address" in
  0.0.0.0|\*)
    healthcheck_address="127.0.0.1"
    ;;
  ::)
    healthcheck_address="::1"
    ;;
esac

if [[ "$healthcheck_address" == *:* ]]; then
  upsc_target="${ups_name}@[$healthcheck_address]:3493"
else
  upsc_target="${ups_name}@$healthcheck_address:3493"
fi

upsc_value() {
  timeout 3 upsc "$upsc_target" "$1"
}

upsc_value ups.status >/dev/null

if [[ "$require_valid_sample" == "1" ]]; then
  sample_valid="$(upsc_value experimental.ragtech.sample.valid)"
  test "$sample_valid" = "1"
fi
