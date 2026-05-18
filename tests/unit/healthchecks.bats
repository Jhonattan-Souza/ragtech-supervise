#!/usr/bin/env bats

load "../helpers/test_helper"

setup() {
  fake_bin="$BATS_TEST_TMPDIR/bin"
  mkdir -p "$fake_bin"
}

@test "main healthcheck succeeds only when supsvc and web probe succeed" {
  make_fake_command "$fake_bin" pgrep '[[ "$1" == "-x" && "$2" == "supsvc" ]]; exit "${FAKE_PGREP_STATUS:-0}"'
  make_fake_command "$fake_bin" timeout '[[ "$1" == "3" ]]; shift; exit "${FAKE_TIMEOUT_STATUS:-0}"'

  run env PATH="$fake_bin:$PATH" bash "$REPO_ROOT/healthcheck.sh"
  assert_success

  run env PATH="$fake_bin:$PATH" FAKE_PGREP_STATUS=1 bash "$REPO_ROOT/healthcheck.sh"
  assert_failure

  run env PATH="$fake_bin:$PATH" FAKE_TIMEOUT_STATUS=1 bash "$REPO_ROOT/healthcheck.sh"
  assert_failure
}

@test "NUT bridge healthcheck fails for missing, empty, and stale dev files" {
  dev="$BATS_TEST_TMPDIR/ragtech.dev"
  make_fake_command "$fake_bin" upsc 'echo 1'

  run env DEV_PATH="$dev" PATH="$fake_bin:$PATH" bash "$REPO_ROOT/nut-bridge/healthcheck.sh"
  assert_failure

  : >"$dev"
  run env DEV_PATH="$dev" PATH="$fake_bin:$PATH" bash "$REPO_ROOT/nut-bridge/healthcheck.sh"
  assert_failure

  printf 'ups.status: OL\n' >"$dev"
  touch -d @1 "$dev"
  run env DEV_PATH="$dev" HEALTHCHECK_MAX_DEV_AGE=1 PATH="$fake_bin:$PATH" bash "$REPO_ROOT/nut-bridge/healthcheck.sh"
  assert_failure
}

@test "NUT bridge healthcheck checks ups.status through upsc" {
  dev="$BATS_TEST_TMPDIR/ragtech.dev"
  printf 'ups.status: OL\n' >"$dev"
  make_fake_command "$fake_bin" upsc '[[ "${*: -1}" == "ups.status" ]] || exit 1; exit "${FAKE_UPSC_STATUS:-0}"'

  run env DEV_PATH="$dev" PATH="$fake_bin:$PATH" HEALTHCHECK_REQUIRE_VALID_SAMPLE=0 bash "$REPO_ROOT/nut-bridge/healthcheck.sh"
  assert_success

  run env DEV_PATH="$dev" PATH="$fake_bin:$PATH" HEALTHCHECK_REQUIRE_VALID_SAMPLE=0 FAKE_UPSC_STATUS=1 bash "$REPO_ROOT/nut-bridge/healthcheck.sh"
  assert_failure
}

@test "NUT bridge healthcheck follows the configured listen address" {
  dev="$BATS_TEST_TMPDIR/ragtech.dev"
  log="$BATS_TEST_TMPDIR/upsc.log"
  printf 'ups.status: OL\n' >"$dev"
  make_fake_command "$fake_bin" upsc 'echo "$1" >>"$FAKE_UPSC_LOG"; echo 1'

  run env DEV_PATH="$dev" PATH="$fake_bin:$PATH" FAKE_UPSC_LOG="$log" HEALTHCHECK_REQUIRE_VALID_SAMPLE=0 NUT_LISTEN_ADDRESS=192.0.2.10 bash "$REPO_ROOT/nut-bridge/healthcheck.sh"
  assert_success
  assert_file_contains "$log" "ragtech@192.0.2.10:3493"

  : >"$log"
  run env DEV_PATH="$dev" PATH="$fake_bin:$PATH" FAKE_UPSC_LOG="$log" HEALTHCHECK_REQUIRE_VALID_SAMPLE=0 NUT_LISTEN_ADDRESS=0.0.0.0 bash "$REPO_ROOT/nut-bridge/healthcheck.sh"
  assert_success
  assert_file_contains "$log" "ragtech@127.0.0.1:3493"

  : >"$log"
  run env DEV_PATH="$dev" PATH="$fake_bin:$PATH" FAKE_UPSC_LOG="$log" HEALTHCHECK_REQUIRE_VALID_SAMPLE=0 NUT_LISTEN_ADDRESS="::" bash "$REPO_ROOT/nut-bridge/healthcheck.sh"
  assert_success
  assert_file_contains "$log" "ragtech@[::1]:3493"
}

@test "sample-valid enforcement can be required or skipped" {
  dev="$BATS_TEST_TMPDIR/ragtech.dev"
  printf 'ups.status: OL\n' >"$dev"
  make_fake_command "$fake_bin" upsc 'case "${*: -1}" in ups.status) echo OL ;; experimental.ragtech.sample.valid) echo "${FAKE_SAMPLE_VALID:-1}" ;; *) exit 1 ;; esac'

  run env DEV_PATH="$dev" PATH="$fake_bin:$PATH" HEALTHCHECK_REQUIRE_VALID_SAMPLE=1 FAKE_SAMPLE_VALID=1 bash "$REPO_ROOT/nut-bridge/healthcheck.sh"
  assert_success

  run env DEV_PATH="$dev" PATH="$fake_bin:$PATH" HEALTHCHECK_REQUIRE_VALID_SAMPLE=1 FAKE_SAMPLE_VALID=0 bash "$REPO_ROOT/nut-bridge/healthcheck.sh"
  assert_failure

  run env DEV_PATH="$dev" PATH="$fake_bin:$PATH" HEALTHCHECK_REQUIRE_VALID_SAMPLE=0 FAKE_SAMPLE_VALID=0 bash "$REPO_ROOT/nut-bridge/healthcheck.sh"
  assert_success
}

@test "NUT bridge healthcheck rejects invalid numeric configuration" {
  dev="$BATS_TEST_TMPDIR/ragtech.dev"
  printf 'ups.status: OL\n' >"$dev"
  make_fake_command "$fake_bin" upsc 'echo 1'

  run env DEV_PATH="$dev" PATH="$fake_bin:$PATH" HEALTHCHECK_MAX_DEV_AGE=abc bash "$REPO_ROOT/nut-bridge/healthcheck.sh"
  assert_failure
  assert_output_contains "HEALTHCHECK_MAX_DEV_AGE must be a non-negative integer"

  run env DEV_PATH="$dev" PATH="$fake_bin:$PATH" HEALTHCHECK_REQUIRE_VALID_SAMPLE=maybe bash "$REPO_ROOT/nut-bridge/healthcheck.sh"
  assert_failure
  assert_output_contains "HEALTHCHECK_REQUIRE_VALID_SAMPLE must be 0 or 1"
}
