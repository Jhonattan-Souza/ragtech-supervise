#!/usr/bin/env bats

load "../helpers/test_helper"

setup() {
  if [[ "${RAGTECH_TEST_CONTAINER:-0}" != "1" || "$(id -u)" != "0" ]]; then
    skip "entrypoint tests write container-local /etc/nut and require the test container"
  fi

  fake_bin="$BATS_TEST_TMPDIR/bin"
  log="$BATS_TEST_TMPDIR/commands.log"
  mkdir -p "$fake_bin"
  rm -rf /etc/nut /run/nut
}

install_entrypoint_fakes() {
  make_fake_command "$fake_bin" ragtech-to-nut 'echo "ragtech-to-nut $*" >>"$FAKE_COMMAND_LOG"; if [[ "${1:-}" != "--once" ]]; then sleep "${FAKE_EXPORTER_SLEEP:-1}"; fi'
  make_fake_command "$fake_bin" upsdrvctl 'echo "upsdrvctl $*" >>"$FAKE_COMMAND_LOG"'
  make_fake_command "$fake_bin" upsd 'echo "upsd $*" >>"$FAKE_COMMAND_LOG"; sleep "${FAKE_UPSD_SLEEP:-0.1}"'
}

run_entrypoint() {
  run env \
    PATH="$fake_bin:$PATH" \
    FAKE_COMMAND_LOG="$log" \
    NUT_MONITOR_PASSWORD="${NUT_MONITOR_PASSWORD-}" \
    NUT_MONITOR_USER="${NUT_MONITOR_USER:-monuser}" \
    UPS_NAME="${UPS_NAME:-ragtech}" \
    DEV_PATH="${DEV_PATH:-/run/nut/ragtech.dev}" \
    NUT_LISTEN_ADDRESS="${NUT_LISTEN_ADDRESS:-0.0.0.0}" \
    FAKE_EXPORTER_STATUS="${FAKE_EXPORTER_STATUS:-}" \
    bash "$REPO_ROOT/nut-bridge/entrypoint.sh"
}

@test "missing NUT_MONITOR_PASSWORD exits non-zero with expected error" {
  install_entrypoint_fakes

  run_entrypoint

  assert_failure
  assert_output_contains "NUT_MONITOR_PASSWORD must be set explicitly"
}

@test "invalid UPS_NAME and NUT_MONITOR_USER are rejected" {
  install_entrypoint_fakes

  NUT_MONITOR_PASSWORD=secret UPS_NAME="bad name" run_entrypoint
  assert_failure
  assert_output_contains "UPS_NAME must contain only letters"

  NUT_MONITOR_PASSWORD=secret UPS_NAME=default run_entrypoint
  assert_failure
  assert_output_contains "UPS_NAME must not be the reserved NUT name default"

  NUT_MONITOR_PASSWORD=secret UPS_NAME=ragtech NUT_MONITOR_USER="bad/user" run_entrypoint
  assert_failure
  assert_output_contains "NUT_MONITOR_USER must contain only letters"
}

@test "passwords containing newline or carriage return are rejected" {
  install_entrypoint_fakes

  NUT_MONITOR_PASSWORD=$'line\nbreak' run_entrypoint
  assert_failure
  assert_output_contains "NUT_MONITOR_PASSWORD must not contain newline characters"

  NUT_MONITOR_PASSWORD=$'line\rbreak' run_entrypoint
  assert_failure
  assert_output_contains "NUT_MONITOR_PASSWORD must not contain newline characters"
}

@test "unsafe password, dev path, and listen address values are rejected before config generation" {
  install_entrypoint_fakes

  NUT_MONITOR_PASSWORD="space secret" run_entrypoint
  assert_failure
  assert_output_contains "NUT_MONITOR_PASSWORD must not contain whitespace or NUT config metacharacters"

  NUT_MONITOR_PASSWORD='hash#secret' run_entrypoint
  assert_failure
  assert_output_contains "NUT_MONITOR_PASSWORD must not contain whitespace or NUT config metacharacters"

  NUT_MONITOR_PASSWORD='quoted"secret' run_entrypoint
  assert_failure
  assert_output_contains "NUT_MONITOR_PASSWORD must not contain whitespace or NUT config metacharacters"

  NUT_MONITOR_PASSWORD='slash\secret' run_entrypoint
  assert_failure
  assert_output_contains "NUT_MONITOR_PASSWORD must not contain whitespace or NUT config metacharacters"

  NUT_MONITOR_PASSWORD=secret DEV_PATH=$'/tmp/ragtech.dev\nLISTEN 0.0.0.0 3493' run_entrypoint
  assert_failure
  assert_output_contains "DEV_PATH must be an absolute path without whitespace or NUT config metacharacters"

  NUT_MONITOR_PASSWORD=secret NUT_LISTEN_ADDRESS=$'127.0.0.1\nLISTEN 0.0.0.0 3493' run_entrypoint
  assert_failure
  assert_output_contains "NUT_LISTEN_ADDRESS must not contain whitespace or NUT config metacharacters"
}

@test "valid names and password generate NUT config files with expected permissions" {
  install_entrypoint_fakes

  UPS_NAME=ragtech_lab NUT_MONITOR_USER=monitor.user NUT_MONITOR_PASSWORD="safe-._:@%+=,secret" DEV_PATH=/tmp/ragtech.dev NUT_LISTEN_ADDRESS=127.0.0.1 run_entrypoint

  assert_success
  assert_file_contains /etc/nut/nut.conf "MODE=netserver"
  assert_file_contains /etc/nut/ups.conf "[ragtech_lab]"
  assert_file_contains /etc/nut/ups.conf "port = /tmp/ragtech.dev"
  assert_file_contains /etc/nut/upsd.conf "LISTEN 127.0.0.1 3493"
  assert_file_contains /etc/nut/upsd.users "[monitor.user]"
  assert_file_contains /etc/nut/upsd.users "password = safe-._:@%+=,secret"
  assert_file_contains /etc/nut/upsd.users "upsmon primary"
  [[ "$(stat -c %a /etc/nut/upsd.users)" == "640" ]]
}

@test "fake command log verifies startup order without real daemons" {
  install_entrypoint_fakes

  NUT_MONITOR_PASSWORD=secret run_entrypoint

  assert_success
  assert_file_contains "$log" "ragtech-to-nut --once"
  assert_file_contains "$log" "ragtech-to-nut "
  assert_file_contains "$log" "upsdrvctl -u nut start"
  assert_file_contains "$log" "upsd -D"

  once_line="$(grep -nFx "ragtech-to-nut --once" "$log" | cut -d: -f1)"
  driver_line="$(grep -nFx "upsdrvctl -u nut start" "$log" | cut -d: -f1)"
  upsd_line="$(grep -nFx "upsd -D" "$log" | cut -d: -f1)"
  [[ "$once_line" -lt "$driver_line" ]]
  [[ "$driver_line" -lt "$upsd_line" ]]
}

@test "entrypoint exits when the SQLite exporter exits after startup" {
  make_fake_command "$fake_bin" ragtech-to-nut 'echo "ragtech-to-nut $*" >>"$FAKE_COMMAND_LOG"; if [[ "${1:-}" == "--once" ]]; then exit 0; fi; exit "${FAKE_EXPORTER_STATUS:-42}"'
  make_fake_command "$fake_bin" upsdrvctl 'echo "upsdrvctl $*" >>"$FAKE_COMMAND_LOG"'
  make_fake_command "$fake_bin" upsd 'echo "upsd $*" >>"$FAKE_COMMAND_LOG"; sleep 30'

  NUT_MONITOR_PASSWORD=secret FAKE_EXPORTER_STATUS=42 run_entrypoint

  assert_failure
  [[ "$status" -eq 42 ]]
  assert_output_contains "Ragtech SQLite exporter exited with status 42"
}
