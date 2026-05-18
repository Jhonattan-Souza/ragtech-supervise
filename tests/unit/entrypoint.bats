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
  make_fake_command "$fake_bin" ragtech-to-nut 'echo "ragtech-to-nut $*" >>"$FAKE_COMMAND_LOG"'
  make_fake_command "$fake_bin" upsdrvctl 'echo "upsdrvctl $*" >>"$FAKE_COMMAND_LOG"'
  make_fake_command "$fake_bin" upsd 'echo "upsd $*" >>"$FAKE_COMMAND_LOG"'
}

run_entrypoint() {
  run env \
    PATH="$fake_bin:$PATH" \
    FAKE_COMMAND_LOG="$log" \
    NUT_MONITOR_PASSWORD="${NUT_MONITOR_PASSWORD-}" \
    NUT_MONITOR_USER="${NUT_MONITOR_USER:-monuser}" \
    UPS_NAME="${UPS_NAME:-ragtech}" \
    DEV_PATH="${DEV_PATH:-/run/nut/ragtech.dev}" \
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

@test "valid names and password generate NUT config files with expected permissions" {
  install_entrypoint_fakes

  UPS_NAME=ragtech_lab NUT_MONITOR_USER=monitor.user NUT_MONITOR_PASSWORD="safe punctuation !@# with spaces" DEV_PATH=/tmp/ragtech.dev run_entrypoint

  assert_success
  assert_file_contains /etc/nut/nut.conf "MODE=netserver"
  assert_file_contains /etc/nut/ups.conf "[ragtech_lab]"
  assert_file_contains /etc/nut/ups.conf "port = /tmp/ragtech.dev"
  assert_file_contains /etc/nut/upsd.conf "LISTEN 0.0.0.0 3493"
  assert_file_contains /etc/nut/upsd.users "[monitor.user]"
  assert_file_contains /etc/nut/upsd.users "password = safe punctuation !@# with spaces"
  assert_file_contains /etc/nut/upsd.users "upsmon primary"
  [[ "$(stat -c %a /etc/nut/upsd.users)" == "640" ]]
}

@test "fake command log verifies startup order without real daemons" {
  install_entrypoint_fakes

  NUT_MONITOR_PASSWORD=secret run_entrypoint

  assert_success
  mapfile -t commands <"$log"
  [[ "${commands[0]}" == "ragtech-to-nut --once" ]]
  [[ "${commands[1]}" == "ragtech-to-nut " ]]
  [[ "${commands[2]}" == "upsdrvctl -u nut start" ]]
  [[ "${commands[3]}" == "upsd -D" ]]
}
