#!/usr/bin/env bash
set -euo pipefail

UPS_NAME="${UPS_NAME:-ragtech}"
DEV_PATH="${DEV_PATH:-/run/nut/ragtech.dev}"
NUT_MONITOR_USER="${NUT_MONITOR_USER:-monuser}"
NUT_MONITOR_PASSWORD="${NUT_MONITOR_PASSWORD:-}"

if [[ -z "$NUT_MONITOR_PASSWORD" ]]; then
  echo "[entrypoint] NUT_MONITOR_PASSWORD must be set explicitly" >&2
  exit 1
fi

validate_section_name() {
  local name="$1"
  local value="$2"

  if [[ ! "$value" =~ ^[A-Za-z0-9_.-]+$ ]]; then
    echo "[entrypoint] $name must contain only letters, numbers, dot, underscore, or dash" >&2
    exit 1
  fi
}

validate_no_newline() {
  local name="$1"
  local value="$2"

  if [[ "$value" == *$'\n'* || "$value" == *$'\r'* ]]; then
    echo "[entrypoint] $name must not contain newline characters" >&2
    exit 1
  fi
}

validate_section_name "UPS_NAME" "$UPS_NAME"
validate_section_name "NUT_MONITOR_USER" "$NUT_MONITOR_USER"
validate_no_newline "NUT_MONITOR_PASSWORD" "$NUT_MONITOR_PASSWORD"

mkdir -p /etc/nut /run/nut
chown -R nut:nut /run/nut

cat >/etc/nut/nut.conf <<EOF
MODE=netserver
EOF

cat >/etc/nut/ups.conf <<EOF
[$UPS_NAME]
  driver = dummy-ups
  port = $DEV_PATH
  mode = dummy-loop
  desc = "Ragtech UPS via Supervise SQLite"
EOF

cat >/etc/nut/upsd.conf <<EOF
LISTEN 0.0.0.0 3493
EOF

cat >/etc/nut/upsd.users <<EOF
[$NUT_MONITOR_USER]
  password = $NUT_MONITOR_PASSWORD
  upsmon primary
EOF

chmod 0640 /etc/nut/upsd.users
chown -R root:nut /etc/nut

echo "[entrypoint] generating initial dummy-ups file at $DEV_PATH"
ragtech-to-nut --once

echo "[entrypoint] starting Ragtech SQLite exporter"
ragtech-to-nut &
exporter_pid=$!
upsd_pid=""

# shellcheck disable=SC2329
cleanup() {
  trap - EXIT INT TERM
  echo "[entrypoint] stopping services"
  if [[ -n "$upsd_pid" ]]; then
    kill "$upsd_pid" >/dev/null 2>&1 || true
    wait "$upsd_pid" >/dev/null 2>&1 || true
  fi
  upsdrvctl stop >/dev/null 2>&1 || true
  kill "$exporter_pid" >/dev/null 2>&1 || true
  wait "$exporter_pid" >/dev/null 2>&1 || true
}
trap cleanup EXIT
trap 'cleanup; exit 143' INT TERM

echo "[entrypoint] starting NUT driver for $UPS_NAME"
upsdrvctl -u nut start

echo "[entrypoint] starting upsd on port 3493"
upsd -D &
upsd_pid=$!

set +e
wait "$upsd_pid"
exit_code=$?
upsd_pid=""
set -e
exit "$exit_code"
