#!/usr/bin/env bash
set -euo pipefail

UPS_NAME="${UPS_NAME:-ragtech}"
DEV_PATH="${DEV_PATH:-/run/nut/ragtech.dev}"
NUT_MONITOR_USER="${NUT_MONITOR_USER:-monuser}"
NUT_MONITOR_PASSWORD="${NUT_MONITOR_PASSWORD:-}"
NUT_MONITOR_ROLE="${NUT_MONITOR_ROLE:-secondary}"
NUT_LISTEN_ADDRESS="${NUT_LISTEN_ADDRESS:-0.0.0.0}"

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

validate_ups_name() {
  validate_section_name "UPS_NAME" "$UPS_NAME"

  if [[ "${UPS_NAME,,}" == "default" ]]; then
    echo "[entrypoint] UPS_NAME must not be the reserved NUT name default" >&2
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

validate_config_token() {
  local name="$1"
  local value="$2"

  if [[ "$value" =~ [[:space:]] \
    || "$value" == *"#"* \
    || "$value" == *"["* \
    || "$value" == *"]"* \
    || "$value" == *"="* \
    || "$value" == *'"'* \
    || "$value" == *\\* ]]; then
    echo "[entrypoint] $name must not contain whitespace or NUT config metacharacters" >&2
    exit 1
  fi
}

validate_absolute_config_path() {
  local name="$1"
  local value="$2"

  if [[ "$value" != /* \
    || "$value" =~ [[:space:]] \
    || "$value" == *"#"* \
    || "$value" == *"["* \
    || "$value" == *"]"* \
    || "$value" == *"="* \
    || "$value" == *'"'* \
    || "$value" == *\\* ]]; then
    echo "[entrypoint] $name must be an absolute path without whitespace or NUT config metacharacters" >&2
    exit 1
  fi
}

validate_ups_name
validate_section_name "NUT_MONITOR_USER" "$NUT_MONITOR_USER"
if [[ "$NUT_MONITOR_ROLE" != "primary" && "$NUT_MONITOR_ROLE" != "secondary" ]]; then
  echo "[entrypoint] NUT_MONITOR_ROLE must be primary or secondary" >&2
  exit 1
fi
validate_no_newline "NUT_MONITOR_PASSWORD" "$NUT_MONITOR_PASSWORD"
validate_config_token "NUT_MONITOR_PASSWORD" "$NUT_MONITOR_PASSWORD"
validate_absolute_config_path "DEV_PATH" "$DEV_PATH"
validate_config_token "NUT_LISTEN_ADDRESS" "$NUT_LISTEN_ADDRESS"

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
LISTEN $NUT_LISTEN_ADDRESS 3493
EOF

cat >/etc/nut/upsd.users <<EOF
[$NUT_MONITOR_USER]
  password = $NUT_MONITOR_PASSWORD
  upsmon $NUT_MONITOR_ROLE
EOF

chmod 0640 /etc/nut/upsd.users
chown -R root:nut /etc/nut

echo "[entrypoint] waiting for valid Ragtech telemetry"
ragtech-to-nut --wait-for-valid

echo "[entrypoint] starting Ragtech SQLite exporter"
RAGTECH_NUT_INITIAL_LIVE_SAMPLE_SEEN=1 EXIT_ON_INVALID_AFTER_LIVE=1 REQUIRE_FRESH_SAMPLE=0 ragtech-to-nut &
exporter_pid=$!
upsd_pid=""

# shellcheck disable=SC2317,SC2329
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
completed_pid=""
wait -n -p completed_pid "$exporter_pid" "$upsd_pid"
exit_code=$?
set -e
if [[ "$completed_pid" == "$exporter_pid" ]]; then
  exporter_pid=""
  echo "[entrypoint] Ragtech SQLite exporter exited with status $exit_code" >&2
else
  upsd_pid=""
  echo "[entrypoint] upsd exited with status $exit_code" >&2
fi
exit "$exit_code"
