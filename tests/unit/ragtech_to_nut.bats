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
    EXIT_ON_INVALID_AFTER_LIVE="${EXIT_ON_INVALID_AFTER_LIVE:-0}" \
    RAGTECH_NUT_INITIAL_LIVE_SAMPLE_SEEN="${RAGTECH_NUT_INITIAL_LIVE_SAMPLE_SEEN:-0}" \
    bash "$REPO_ROOT/nut-bridge/ragtech-to-nut.sh" --once
}

fake_live_sqlite_row_body='attempt=0
has_timeout=0
while (($#)); do
  if [[ "$1" == "-cmd" && "${2:-}" == ".timeout 2000" ]]; then
    has_timeout=1
  fi
  shift
done
if [[ "$has_timeout" != "1" ]]; then
  echo "missing sqlite busy timeout" >&2
  exit 9
fi
if [[ -f "${FAKE_SQLITE_ATTEMPTS:-}" ]]; then
  attempt="$(<"$FAKE_SQLITE_ATTEMPTS")"
fi
attempt=$((attempt + 1))
printf "%s\n" "$attempt" >"$FAKE_SQLITE_ATTEMPTS"
if [[ "$attempt" == "1" ]]; then
  echo "database is locked" >&2
  exit 5
fi
sep=$'\''\x1f'\''
fields=(
  ups-1 1000 1 EVENTLOG
  127.2 127.0 1.0 42 60.0 13.5 88 29.2
  127 127 500 60 12
  1 0 0 0 0 0 0 0 0
  "Ragtech Test UPS" "1.2.3"
)
(IFS="$sep"; printf "%s\n" "${fields[*]}")'

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
  assert_nut_value "$dev" "ups.status" ""
  assert_nut_value "$dev" "ups.alarm" "Ragtech telemetry unavailable: database-unreadable"
  assert_file_contains "$dev" "ALARM [Ragtech telemetry unavailable: database-unreadable]"
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

@test "wait-for-valid mode exits only after a fresh sample is accepted" {
  seed_live_sample

  DB_PATH="$db" DEV_PATH="$dev" REQUIRE_FRESH_SAMPLE=1 MAX_SAMPLE_AGE=0 POLL_INTERVAL=1 \
    bash "$REPO_ROOT/nut-bridge/ragtech-to-nut.sh" --wait-for-valid &
  exporter_pid=$!

  wait_for_file_contains "$dev" "experimental.ragtech.bridge.reason: stale-startup-sample"
  kill -0 "$exporter_pid"

  SAMPLE_DT=1001 SAMPLE_EVENT=2 insert_sample "$db" EVENTLOG

  for _ in $(seq 1 30); do
    if ! kill -0 "$exporter_pid" >/dev/null 2>&1; then
      wait "$exporter_pid"
      exporter_pid=""
      assert_nut_value "$dev" "experimental.ragtech.sample.valid" "1"
      assert_nut_value "$dev" "experimental.ragtech.sample.time" "1001"
      return 0
    fi
    sleep 0.2
  done

  return 1
}

@test "invalid MAX_SAMPLE_AGE exits with a clear error" {
  seed_live_sample

  MAX_SAMPLE_AGE=abc run_exporter_once

  assert_failure
  assert_output_contains "MAX_SAMPLE_AGE must be a non-negative integer"
}

@test "invalid POLL_INTERVAL and BATTERY_CHARGE_LOW exit with clear errors" {
  seed_live_sample

  POLL_INTERVAL=0 run_exporter_once
  assert_failure
  assert_output_contains "POLL_INTERVAL must be a positive number"

  BATTERY_CHARGE_LOW=abc run_exporter_once
  assert_failure
  assert_output_contains "BATTERY_CHARGE_LOW must be an integer from 0 to 100"

  BATTERY_CHARGE_LOW=101 run_exporter_once
  assert_failure
  assert_output_contains "BATTERY_CHARGE_LOW must be an integer from 0 to 100"
}

@test "invalid REQUIRE_FRESH_SAMPLE exits with a clear error" {
  seed_live_sample

  REQUIRE_FRESH_SAMPLE=maybe run_exporter_once

  assert_failure
  assert_output_contains "REQUIRE_FRESH_SAMPLE must be 0 or 1"
}

@test "invalid initial live sample flag exits with a clear error" {
  seed_live_sample

  RAGTECH_NUT_INITIAL_LIVE_SAMPLE_SEEN=maybe run_exporter_once

  assert_failure
  assert_output_contains "RAGTECH_NUT_INITIAL_LIVE_SAMPLE_SEEN must be 0 or 1"
}

@test "SQLite read uses a busy timeout and retries a transient failure" {
  fake_bin="$BATS_TEST_TMPDIR/bin"
  attempts="$BATS_TEST_TMPDIR/sqlite-attempts"
  mkdir -p "$fake_bin"
  : >"$db"
  make_fake_command "$fake_bin" sqlite3 "$fake_live_sqlite_row_body"

  PATH="$fake_bin:$PATH" FAKE_SQLITE_ATTEMPTS="$attempts" run_exporter_once

  assert_success
  [[ "$(<"$attempts")" == "2" ]]
  assert_nut_value "$dev" "experimental.ragtech.sample.valid" "1"
  assert_nut_value "$dev" "ups.status" "OL"
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
  assert_nut_value "$dev" "battery.charger.status" "discharging"

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
  assert_nut_value "$dev" "ups.alarm" "Ragtech telemetry unavailable: ups-disconnected"
  assert_nut_value "$dev" "experimental.ragtech.connection.status" "disconnected"
  assert_nut_value "$dev" "experimental.ragtech.bridge.reason" "ups-disconnected"
  assert_nut_value "$dev" "ups.status" ""
}

@test "long-running exporter exits after live telemetry becomes invalid" {
  seed_live_sample

  DB_PATH="$db" DEV_PATH="$dev" REQUIRE_FRESH_SAMPLE=0 MAX_SAMPLE_AGE=1 POLL_INTERVAL=1 EXIT_ON_INVALID_AFTER_LIVE=1 \
    bash "$REPO_ROOT/nut-bridge/ragtech-to-nut.sh" &
  exporter_pid=$!

  wait_for_file_contains "$dev" "experimental.ragtech.sample.valid: 1"
  wait_for_file_contains "$dev" "experimental.ragtech.bridge.reason: stale-source-sample" 20

  set +e
  wait "$exporter_pid"
  status=$?
  set -e
  exporter_pid=""
  [[ "$status" -eq 75 ]]
}

@test "exporter exits on first invalid sample when startup already served live telemetry" {
  EXIT_ON_INVALID_AFTER_LIVE=1 RAGTECH_NUT_INITIAL_LIVE_SAMPLE_SEEN=1 run_exporter_once

  assert_failure
  [[ "$status" -eq 75 ]]
  assert_nut_value "$dev" "experimental.ragtech.sample.valid" "0"
  assert_nut_value "$dev" "experimental.ragtech.bridge.reason" "database-unreadable"
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

@test "latest-sample query shape can use representative dt indexes" {
  create_ragtech_schema "$db"

  plan="$(sqlite3 "$db" <<'SQL'
EXPLAIN QUERY PLAN
SELECT * FROM (
  SELECT 'EVENTLOG' AS sample_source, id, dt, event
  FROM EVENTLOG
  WHERE id = 'ups-1'
  ORDER BY dt DESC, event DESC
  LIMIT 1
)
UNION ALL
SELECT * FROM (
  SELECT 'HISTLOGHOUR' AS sample_source, id, dt, event
  FROM HISTLOGHOUR
  WHERE id = 'ups-1'
  ORDER BY dt DESC, event DESC
  LIMIT 1
);
SQL
)"

  [[ "$plan" == *"SEARCH EVENTLOG USING COVERING INDEX sqlite_autoindex_EVENTLOG_1"* ]]
  [[ "$plan" == *"SEARCH HISTLOGHOUR USING COVERING INDEX sqlite_autoindex_HISTLOGHOUR_1"* ]]
  [[ "$plan" != *"USE TEMP B-TREE"* ]]
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

@test "numeric awk formatting is forced to C locale" {
  create_ragtech_schema "$db"
  insert_device "$db"
  insert_sample "$db" EVENTLOG
  fake_bin="$BATS_TEST_TMPDIR/bin"
  make_fake_command "$fake_bin" awk '
if [[ "${LC_ALL:-}" != "C" ]]; then
  echo "awk called without LC_ALL=C" >&2
  exit 17
fi
exec /usr/bin/awk "$@"
'

  PATH="$fake_bin:$PATH" run_exporter_once

  assert_success
  assert_nut_value "$dev" "ups.load" "8"
}

@test "SQLite string values are sanitized before writing dummy-ups state" {
  create_ragtech_schema "$db"
  insert_device "$db" $'ups\nINJECT: bad' 1000 $'Model\rups.status: OB LB' $'1.2.3\nALARM [bad]'
  SAMPLE_ID=$'ups\nINJECT: bad' SAMPLE_DT=$'1000\nups.status: OB' SAMPLE_EVENT=$'8\nALARM [bad]' insert_sample "$db" EVENTLOG

  run_exporter_once

  assert_success
  assert_nut_value "$dev" "device.serial" "upsINJECT: bad"
  assert_nut_value "$dev" "device.model" "Modelups.status: OB LB"
  assert_nut_value "$dev" "ups.firmware" "1.2.3ALARM [bad]"
  assert_nut_value "$dev" "experimental.ragtech.sample.time" "1000"
  assert_nut_value "$dev" "experimental.ragtech.event" "8"
  ! grep -Fxq "ups.status: OB LB" "$dev"
  ! grep -Fxq "ups.status: OB" "$dev"
  ! grep -Fxq "ALARM [bad]" "$dev"
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
