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

cleanup() {
  echo "[entrypoint] stopping services"
  upsdrvctl stop >/dev/null 2>&1 || true
  kill "$exporter_pid" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

echo "[entrypoint] starting NUT driver for $UPS_NAME"
upsdrvctl -u nut start

echo "[entrypoint] starting upsd on port 3493"
exec upsd -D
