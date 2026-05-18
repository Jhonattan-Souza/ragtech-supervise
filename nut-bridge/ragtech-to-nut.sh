#!/usr/bin/env bash
set -euo pipefail

DB_PATH="${DB_PATH:-/data/monit.db}"
DEV_PATH="${DEV_PATH:-/run/nut/ragtech.dev}"
POLL_INTERVAL="${POLL_INTERVAL:-2}"
BATTERY_CHARGE_LOW="${BATTERY_CHARGE_LOW:-20}"
UPS_NAME="${UPS_NAME:-ragtech}"
REQUIRE_FRESH_SAMPLE="${REQUIRE_FRESH_SAMPLE:-1}"
SQLITE_SEPARATOR=$'\x1f'
STARTUP_SAMPLE_SEEN=0
STARTUP_SAMPLE_TOKEN=""

clamp_charge() {
  awk -v value="${1:-0}" 'BEGIN {
    if (value < 0) value = 0;
    if (value > 100) value = 100;
    printf "%.0f", value;
  }'
}

format_number() {
  awk -v value="${1:-0}" 'BEGIN { printf "%.1f", value + 0 }'
}

is_number() {
  [[ "${1:-}" =~ ^-?[0-9]+([.][0-9]+)?$ ]]
}

write_empty_live_values() {
  local name

  for name in \
    device.serial \
    ups.serial \
    ups.firmware \
    ups.load \
    ups.power.nominal \
    battery.charge \
    battery.voltage \
    battery.voltage.nominal \
    input.voltage \
    input.voltage.nominal \
    output.voltage \
    output.voltage.nominal \
    output.current \
    output.frequency \
    output.frequency.nominal \
    ups.temperature \
    ragtech.event \
    ragtech.sample.source \
    ragtech.sample.time; do
    printf '%s: \n' "$name"
  done
}

has_fresh_start_sample() {
  local sample_token="${1:-}"

  if [[ "$REQUIRE_FRESH_SAMPLE" != "1" ]]; then
    return 0
  fi

  if [[ "$STARTUP_SAMPLE_SEEN" != "1" ]]; then
    STARTUP_SAMPLE_TOKEN="$sample_token"
    STARTUP_SAMPLE_SEEN=1
    return 1
  fi

  [[ "$sample_token" != "$STARTUP_SAMPLE_TOKEN" ]]
}

build_sample_token() {
  local token="" separator="" field

  for field in "$@"; do
    token+="$separator$field"
    separator="$SQLITE_SEPARATOR"
  done

  printf '%s' "$token"
}

output_load_percent() {
  awk \
    -v power="${1:-0}" \
    -v nominal="${2:-0}" \
    -v voltage="${3:-0}" \
    -v current="${4:-0}" 'BEGIN {
      if (nominal <= 0) {
        value = power;
      } else {
        power_pct = (power / nominal) * 100;
        apparent_pct = 0;
        if (voltage > 0 && current > 0) {
          apparent_pct = ((voltage * current) / nominal) * 100;
        }

        # Some Supervise versions appear to store percent load in var_pOutput
        # while others may store output power. Prefer the already-percent value
        # only when it agrees with the voltage/current-derived apparent load.
        if (power >= 0 && power <= 100 && apparent_pct > 0) {
          diff = power - apparent_pct;
          if (diff < 0) diff = -diff;
          tolerance = apparent_pct * 0.35;
          if (tolerance < 10) tolerance = 10;
          value = diff <= tolerance ? power : power_pct;
        } else if (power >= 0 && power <= 100 && apparent_pct == 0) {
          value = power;
        } else {
          value = power_pct;
        }
      }

      if (value < 0) value = 0;
      if (value > 100) value = 100;
      printf "%.0f", value;
    }'
}

write_unknown_state() {
  local reason="${1:-no-current-sample}"
  local tmp
  tmp="$(mktemp "${DEV_PATH}.XXXXXX")"
  {
    cat <<EOF
device.mfr: Ragtech
device.model: Supervise
device.type: ups
ups.mfr: Ragtech
ups.model: Supervise
ups.status: OL
battery.charge.low: $BATTERY_CHARGE_LOW
ragtech.sample.valid: 0
ragtech.connection.status: unavailable
ragtech.bridge.reason: $reason
EOF
    write_empty_live_values
    printf 'ALARM\n'
  } >"$tmp"
  chmod 0644 "$tmp"
  mv "$tmp" "$DEV_PATH"
}

write_state() {
  if [[ ! -r "$DB_PATH" ]]; then
    write_unknown_state "database-unreadable"
    return
  fi

  local query row
  query="
WITH device AS (
  SELECT id FROM DEVICELIST ORDER BY last DESC LIMIT 1
),
samples AS (
  SELECT * FROM (
    SELECT
      'EVENTLOG' AS sample_source,
      id,
      dt,
      event,
      var_vInput,
      var_vOutput,
      var_iOutput,
      var_pOutput,
      var_fOutput,
      var_vBattery,
      var_cBattery,
      var_temperature,
      var_nominalVInput,
      var_nominalVOutput,
      var_nominalPOutput,
      var_nominalFOutput,
      var_nominalVBattery,
      flag_connected,
      flag_opBattery,
      flag_opWarning,
      flag_noVInput,
      flag_loBattery,
      flag_hiPOutput,
      flag_noBattery,
      fail_overload,
      fail_endBattery
    FROM EVENTLOG
    WHERE id = (SELECT id FROM device)
    ORDER BY dt DESC, event DESC
    LIMIT 1
  )
  UNION ALL
  SELECT * FROM (
    SELECT
      'HISTLOGHOUR' AS sample_source,
      id,
      dt,
      event,
      var_vInput,
      var_vOutput,
      var_iOutput,
      var_pOutput,
      var_fOutput,
      var_vBattery,
      var_cBattery,
      var_temperature,
      var_nominalVInput,
      var_nominalVOutput,
      var_nominalPOutput,
      var_nominalFOutput,
      var_nominalVBattery,
      flag_connected,
      flag_opBattery,
      flag_opWarning,
      flag_noVInput,
      flag_loBattery,
      flag_hiPOutput,
      flag_noBattery,
      fail_overload,
      fail_endBattery
    FROM HISTLOGHOUR
    WHERE id = (SELECT id FROM device)
    ORDER BY dt DESC, event DESC
    LIMIT 1
  )
)
SELECT
  COALESCE(e.id, ''),
  COALESCE(e.dt, 0),
  COALESCE(e.event, 0),
  COALESCE(e.sample_source, ''),
  COALESCE(e.var_vInput, 0),
  COALESCE(e.var_vOutput, 0),
  COALESCE(e.var_iOutput, 0),
  COALESCE(e.var_pOutput, 0),
  COALESCE(e.var_fOutput, 0),
  COALESCE(e.var_vBattery, 0),
  COALESCE(e.var_cBattery, ''),
  COALESCE(e.var_temperature, 0),
  COALESCE(e.var_nominalVInput, 0),
  COALESCE(e.var_nominalVOutput, 0),
  COALESCE(e.var_nominalPOutput, 0),
  COALESCE(e.var_nominalFOutput, 0),
  COALESCE(e.var_nominalVBattery, 0),
  COALESCE(e.flag_connected, 0),
  COALESCE(e.flag_opBattery, 0),
  COALESCE(e.flag_opWarning, 0),
  COALESCE(e.flag_noVInput, 0),
  COALESCE(e.flag_loBattery, 0),
  COALESCE(e.flag_hiPOutput, 0),
  COALESCE(e.flag_noBattery, 0),
  COALESCE(e.fail_overload, 0),
  COALESCE(e.fail_endBattery, 0),
  COALESCE(NULLIF(d.userProd, ''), 'Supervise') AS model,
  COALESCE(NULLIF(d.version, ''), '') AS version
FROM samples e
LEFT JOIN DEVICELIST d ON d.id = e.id
ORDER BY e.dt DESC, e.event DESC, e.sample_source ASC
LIMIT 1;"

  local query_error
  query_error="$(mktemp)"
  if ! row="$(sqlite3 -batch -noheader -separator "$SQLITE_SEPARATOR" "$DB_PATH" "$query" 2>"$query_error")"; then
    echo "[ragtech-to-nut] failed to read $DB_PATH: $(tr '\n' ' ' <"$query_error")" >&2
    rm -f "$query_error"
    write_unknown_state "query-failed"
    return
  fi
  rm -f "$query_error"

  if [[ -z "$row" ]]; then
    write_unknown_state "no-current-sample"
    return
  fi

  local id dt event sample_source v_input v_output i_output p_output f_output v_battery c_battery temperature
  local nominal_v_input nominal_v_output nominal_p_output nominal_f_output nominal_v_battery
  local flag_connected flag_op_battery flag_op_warning flag_no_v_input flag_lo_battery
  local flag_hi_p_output flag_no_battery fail_overload fail_end_battery model version

  IFS="$SQLITE_SEPARATOR" read -r \
    id dt event sample_source v_input v_output i_output p_output f_output v_battery c_battery temperature \
    nominal_v_input nominal_v_output nominal_p_output nominal_f_output nominal_v_battery \
    flag_connected flag_op_battery flag_op_warning flag_no_v_input flag_lo_battery \
    flag_hi_p_output flag_no_battery fail_overload fail_end_battery model version <<<"$row"

  local charge status alarm tmp is_connected connection_status sample_token

  sample_token="$(build_sample_token \
    "$id" "$dt" "$event" "$sample_source" "$v_input" "$v_output" "$i_output" \
    "$p_output" "$f_output" "$v_battery" "$c_battery" "$temperature" \
    "$nominal_v_input" "$nominal_v_output" "$nominal_p_output" "$nominal_f_output" \
    "$nominal_v_battery" "$flag_connected" "$flag_op_battery" "$flag_op_warning" \
    "$flag_no_v_input" "$flag_lo_battery" "$flag_hi_p_output" "$flag_no_battery" \
    "$fail_overload" "$fail_end_battery")"
  if ! has_fresh_start_sample "$sample_token"; then
    write_unknown_state "stale-startup-sample"
    return
  fi

  charge="$(clamp_charge "$c_battery")"
  is_connected=0
  connection_status="disconnected"

  if [[ "${flag_connected:-0}" != "1" ]]; then
    status="OL"
    alarm=""
  elif [[ "${flag_op_battery:-0}" == "1" || "${flag_no_v_input:-0}" == "1" ]]; then
    status="OB DISCHRG"
    alarm=""
    is_connected=1
    connection_status="connected"
  else
    status="OL"
    alarm=""
    is_connected=1
    connection_status="connected"
  fi

  if [[ "$is_connected" == "1" ]]; then
    if [[ "${flag_lo_battery:-0}" == "1" || "${fail_end_battery:-0}" == "1" ]]; then
      status="$status LB"
    elif is_number "$c_battery" && [[ "$charge" -le "$BATTERY_CHARGE_LOW" ]]; then
      status="$status LB"
    fi
  fi

  if [[ "$is_connected" == "1" ]]; then
    if [[ "${flag_op_warning:-0}" == "1" ]]; then
      alarm="${alarm:+$alarm; }Ragtech Supervise reports warning"
    fi
    if [[ "${flag_hi_p_output:-0}" == "1" || "${fail_overload:-0}" == "1" ]]; then
      status="$status OVER"
      alarm="${alarm:+$alarm; }UPS overload"
    fi
    if [[ "${flag_no_battery:-0}" == "1" ]]; then
      status="$status RB"
      alarm="${alarm:+$alarm; }Battery not detected"
    fi
  fi

  tmp="$(mktemp "${DEV_PATH}.XXXXXX")"
  {
    printf 'device.mfr: Ragtech\n'
    printf 'device.model: %s\n' "${model:-Supervise}"
    printf 'device.serial: %s\n' "$id"
    printf 'device.type: ups\n'
    printf 'ups.mfr: Ragtech\n'
    printf 'ups.model: %s\n' "${model:-Supervise}"
    printf 'ups.serial: %s\n' "$id"
    printf 'ups.firmware: %s\n' "${version:-unknown}"
    printf 'ups.status: %s\n' "$status"
    printf 'ups.load: %s\n' "$(output_load_percent "$p_output" "$nominal_p_output" "$v_output" "$i_output")"
    printf 'ups.power.nominal: %s\n' "$(format_number "$nominal_p_output")"
    printf 'battery.charge: %s\n' "$charge"
    printf 'battery.charge.low: %s\n' "$BATTERY_CHARGE_LOW"
    printf 'battery.voltage: %s\n' "$(format_number "$v_battery")"
    printf 'battery.voltage.nominal: %s\n' "$(format_number "$nominal_v_battery")"
    printf 'input.voltage: %s\n' "$(format_number "$v_input")"
    printf 'input.voltage.nominal: %s\n' "$(format_number "$nominal_v_input")"
    printf 'output.voltage: %s\n' "$(format_number "$v_output")"
    printf 'output.voltage.nominal: %s\n' "$(format_number "$nominal_v_output")"
    printf 'output.current: %s\n' "$(format_number "$i_output")"
    printf 'output.frequency: %s\n' "$(format_number "$f_output")"
    printf 'output.frequency.nominal: %s\n' "$(format_number "$nominal_f_output")"
    printf 'ups.temperature: %s\n' "$(format_number "$temperature")"
    printf 'ragtech.event: %s\n' "$event"
    printf 'ragtech.sample.source: %s\n' "$sample_source"
    printf 'ragtech.sample.time: %s\n' "$dt"
    printf 'ragtech.sample.valid: 1\n'
    printf 'ragtech.connection.status: %s\n' "$connection_status"
    printf 'ragtech.bridge.reason: live-sample\n'
    if [[ -n "$alarm" ]]; then
      printf 'ALARM [%s]\n' "$alarm"
    else
      printf 'ALARM\n'
    fi
  } >"$tmp"
  chmod 0644 "$tmp"
  mv "$tmp" "$DEV_PATH"
}

if [[ "${1:-}" == "--once" ]]; then
  write_state
  exit 0
fi

while true; do
  write_state
  sleep "$POLL_INTERVAL"
done
