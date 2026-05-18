#!/usr/bin/env bats

load "../helpers/test_helper"
load "../fixtures/ragtech_db"

setup() {
  db="$BATS_TEST_TMPDIR/monit.db"
  dev="$BATS_TEST_TMPDIR/ragtech.dev"
  exporter_pid=""
}

teardown() {
  if [[ -n "$exporter_pid" ]]; then
    kill "$exporter_pid" >/dev/null 2>&1 || true
    wait "$exporter_pid" >/dev/null 2>&1 || true
  fi
}

run_exporter_once() {
  run env \
    DB_PATH="$db" \
    DEV_PATH="$dev" \
    REQUIRE_FRESH_SAMPLE="${REQUIRE_FRESH_SAMPLE:-0}" \
    MAX_SAMPLE_AGE="${MAX_SAMPLE_AGE:-30}" \
    BATTERY_CHARGE_LOW="${BATTERY_CHARGE_LOW:-20}" \
    bash "$REPO_ROOT/nut-bridge/ragtech-to-nut.sh" --once
}

seed_live_sample() {
  create_ragtech_schema "$db"
  insert_device "$db"
  insert_sample "$db" EVENTLOG
}

@test "missing database reports database-unreadable" {
  run_exporter_once

  assert_success
  assert_nut_value "$dev" "experimental.ragtech.sample.valid" "0"
  assert_nut_value "$dev" "experimental.ragtech.bridge.reason" "database-unreadable"
  assert_nut_value "$dev" "ups.status" "ALARM"
}

@test "missing SQLite tables reports query-failed" {
  sqlite3 "$db" "CREATE TABLE unrelated (id INTEGER);"

  run_exporter_once

  assert_success
  assert_nut_value "$dev" "experimental.ragtech.sample.valid" "0"
  assert_nut_value "$dev" "experimental.ragtech.bridge.reason" "query-failed"
}

@test "valid schema with no samples reports no-current-sample" {
  create_ragtech_schema "$db"
  insert_device "$db"

  run_exporter_once

  assert_success
  assert_nut_value "$dev" "experimental.ragtech.sample.valid" "0"
  assert_nut_value "$dev" "experimental.ragtech.bridge.reason" "no-current-sample"
}

@test "fresh-sample mode rejects startup sample until the row changes" {
  seed_live_sample

  DB_PATH="$db" DEV_PATH="$dev" REQUIRE_FRESH_SAMPLE=1 MAX_SAMPLE_AGE=0 POLL_INTERVAL=1 \
    bash "$REPO_ROOT/nut-bridge/ragtech-to-nut.sh" &
  exporter_pid=$!

  wait_for_file_contains "$dev" "experimental.ragtech.bridge.reason: stale-startup-sample"

  SAMPLE_DT=1001 SAMPLE_EVENT=2 insert_sample "$db" EVENTLOG

  wait_for_file_contains "$dev" "experimental.ragtech.sample.valid: 1"
  assert_nut_value "$dev" "experimental.ragtech.bridge.reason" "live-sample"
  assert_nut_value "$dev" "experimental.ragtech.sample.time" "1001"
}

@test "invalid MAX_SAMPLE_AGE exits with a clear error" {
  seed_live_sample

  MAX_SAMPLE_AGE=abc run_exporter_once

  assert_failure
  assert_output_contains "MAX_SAMPLE_AGE must be a non-negative integer"
}

@test "MAX_SAMPLE_AGE=0 allows the current row to remain valid" {
  seed_live_sample

  MAX_SAMPLE_AGE=0 run_exporter_once

  assert_success
  assert_nut_value "$dev" "experimental.ragtech.sample.valid" "1"
  assert_nut_value "$dev" "experimental.ragtech.bridge.reason" "live-sample"
}

@test "status mapping covers line, battery, low battery, overload, replace battery, and disconnected states" {
  create_ragtech_schema "$db"
  insert_device "$db"

  SAMPLE_EVENT=1 insert_sample "$db" EVENTLOG
  run_exporter_once
  assert_nut_value "$dev" "ups.status" "OL"

  SAMPLE_DT=1001 SAMPLE_EVENT=2 SAMPLE_OP_BATTERY=1 insert_sample "$db" EVENTLOG
  run_exporter_once
  assert_nut_value "$dev" "ups.status" "OB DISCHRG"

  SAMPLE_DT=1002 SAMPLE_EVENT=3 SAMPLE_OP_BATTERY=1 SAMPLE_LO_BATTERY=1 insert_sample "$db" EVENTLOG
  run_exporter_once
  assert_nut_value "$dev" "ups.status" "OB DISCHRG LB"

  SAMPLE_DT=1003 SAMPLE_EVENT=4 SAMPLE_HI_P_OUTPUT=1 insert_sample "$db" EVENTLOG
  run_exporter_once
  assert_nut_value "$dev" "ups.status" "OL OVER"
  assert_file_contains "$dev" "ALARM [UPS overload]"

  SAMPLE_DT=1004 SAMPLE_EVENT=5 SAMPLE_NO_BATTERY=1 insert_sample "$db" EVENTLOG
  run_exporter_once
  assert_nut_value "$dev" "ups.status" "OL RB"
  assert_file_contains "$dev" "ALARM [Battery not detected]"

  SAMPLE_DT=1005 SAMPLE_EVENT=6 SAMPLE_CONNECTED=0 insert_sample "$db" EVENTLOG
  run_exporter_once
  assert_nut_value "$dev" "ups.status" "ALARM"
  assert_nut_value "$dev" "experimental.ragtech.connection.status" "disconnected"
  assert_file_contains "$dev" "Ragtech Supervise reports UPS disconnected"
}

@test "warning and fault alarms are combined in alarm text" {
  create_ragtech_schema "$db"
  insert_device "$db"
  SAMPLE_OP_WARNING=1 SAMPLE_FAIL_OVERLOAD=1 SAMPLE_NO_BATTERY=1 insert_sample "$db" EVENTLOG

  run_exporter_once

  assert_success
  assert_nut_value "$dev" "ups.status" "OL OVER RB"
  assert_file_contains "$dev" "ALARM [Ragtech Supervise reports warning; UPS overload; Battery not detected]"
}

@test "sample selection uses newest row and falls back to HISTLOGHOUR" {
  create_ragtech_schema "$db"
  insert_device "$db"
  SAMPLE_DT=900 SAMPLE_EVENT=1 SAMPLE_C_BATTERY=50 insert_sample "$db" HISTLOGHOUR

  run_exporter_once
  assert_success
  assert_nut_value "$dev" "experimental.ragtech.sample.source" "HISTLOGHOUR"
  assert_nut_value "$dev" "battery.charge" "50"

  SAMPLE_DT=1200 SAMPLE_EVENT=2 SAMPLE_C_BATTERY=77 insert_sample "$db" EVENTLOG
  run_exporter_once
  assert_nut_value "$dev" "experimental.ragtech.sample.source" "EVENTLOG"
  assert_nut_value "$dev" "battery.charge" "77"
}

@test "numeric formatting clamps charge and blanks invalid numbers" {
  create_ragtech_schema "$db"
  insert_device "$db"
  SAMPLE_C_BATTERY=150 \
    SAMPLE_V_BATTERY=__NULL__ \
    SAMPLE_NOMINAL_P_OUTPUT=__NULL__ \
    SAMPLE_P_OUTPUT=70 \
    insert_sample "$db" EVENTLOG

  run_exporter_once

  assert_success
  assert_nut_value "$dev" "battery.charge" "100"
  assert_nut_value "$dev" "battery.voltage" ""
  assert_nut_value "$dev" "ups.power.nominal" ""
  assert_nut_value "$dev" "ups.load" "70"
}

@test "load derivation prefers apparent output when percent-like power disagrees" {
  create_ragtech_schema "$db"
  insert_device "$db"
  SAMPLE_P_OUTPUT=90 SAMPLE_NOMINAL_P_OUTPUT=500 SAMPLE_V_OUTPUT=100 SAMPLE_I_OUTPUT=1 insert_sample "$db" EVENTLOG

  run_exporter_once

  assert_success
  assert_nut_value "$dev" "ups.load" "18"
}

@test "output file has expected mode and no live telemetry for rejected samples" {
  seed_live_sample

  REQUIRE_FRESH_SAMPLE=1 run_exporter_once

  assert_success
  [[ "$(stat -c %a "$dev")" == "644" ]]
  assert_nut_value "$dev" "experimental.ragtech.sample.valid" "0"
  assert_nut_value "$dev" "experimental.ragtech.bridge.reason" "stale-startup-sample"
  refute_file_contains "$dev" "experimental.ragtech.bridge.reason: live-sample"
}
