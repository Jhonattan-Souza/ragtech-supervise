#!/usr/bin/env bash
set -euo pipefail

DB_PATH="${DB_PATH:-/data/monit.db}"
DEV_PATH="${DEV_PATH:-/run/nut/ragtech.dev}"
POLL_INTERVAL="${POLL_INTERVAL:-2}"
BATTERY_CHARGE_LOW="${BATTERY_CHARGE_LOW:-20}"
UPS_NAME="${UPS_NAME:-ragtech}"
SQLITE_SEPARATOR=$'\x1f'

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
  local tmp
  tmp="$(mktemp "${DEV_PATH}.XXXXXX")"
  cat >"$tmp" <<EOF
device.mfr: Ragtech
device.model: Supervise
device.type: ups
ups.mfr: Ragtech
ups.model: Supervise
ups.status: OFF
ALARM [Ragtech Supervise SQLite database has no current UPS sample]
battery.charge.low: $BATTERY_CHARGE_LOW
EOF
  chmod 0644 "$tmp"
  mv "$tmp" "$DEV_PATH"
}

write_state() {
  if [[ ! -r "$DB_PATH" ]]; then
    write_unknown_state
    return
  fi

  local query row
  query="
SELECT
  COALESCE(e.id, ''),
  COALESCE(e.dt, 0),
  COALESCE(e.event, 0),
  COALESCE(e.var_vInput, 0),
  COALESCE(e.var_vOutput, 0),
  COALESCE(e.var_iOutput, 0),
  COALESCE(e.var_pOutput, 0),
  COALESCE(e.var_fOutput, 0),
  COALESCE(e.var_vBattery, 0),
  COALESCE(e.var_cBattery, 0),
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
FROM EVENTLOG e
LEFT JOIN DEVICELIST d ON d.id = e.id
ORDER BY e.dt DESC, e.event DESC
LIMIT 1;"

  local query_error
  query_error="$(mktemp)"
  if ! row="$(sqlite3 -batch -noheader -separator "$SQLITE_SEPARATOR" "$DB_PATH" "$query" 2>"$query_error")"; then
    echo "[ragtech-to-nut] failed to read $DB_PATH: $(tr '\n' ' ' <"$query_error")" >&2
    rm -f "$query_error"
    write_unknown_state
    return
  fi
  rm -f "$query_error"

  if [[ -z "$row" ]]; then
    write_unknown_state
    return
  fi

  local id dt event v_input v_output i_output p_output f_output v_battery c_battery temperature
  local nominal_v_input nominal_v_output nominal_p_output nominal_f_output nominal_v_battery
  local flag_connected flag_op_battery flag_op_warning flag_no_v_input flag_lo_battery
  local flag_hi_p_output flag_no_battery fail_overload fail_end_battery model version

  IFS="$SQLITE_SEPARATOR" read -r \
    id dt event v_input v_output i_output p_output f_output v_battery c_battery temperature \
    nominal_v_input nominal_v_output nominal_p_output nominal_f_output nominal_v_battery \
    flag_connected flag_op_battery flag_op_warning flag_no_v_input flag_lo_battery \
    flag_hi_p_output flag_no_battery fail_overload fail_end_battery model version <<<"$row"

  local charge status alarm tmp
  charge="$(clamp_charge "$c_battery")"

  if [[ "${flag_connected:-0}" != "1" ]]; then
    status="OFF"
    alarm="Ragtech Supervise reports UPS disconnected"
  elif [[ "${flag_op_battery:-0}" == "1" || "${flag_no_v_input:-0}" == "1" ]]; then
    status="OB DISCHRG"
    alarm=""
  else
    status="OL"
    alarm=""
  fi

  if [[ "${flag_lo_battery:-0}" == "1" || "${fail_end_battery:-0}" == "1" || "$charge" -le "$BATTERY_CHARGE_LOW" ]]; then
    status="$status LB"
  fi

  if [[ "${flag_op_warning:-0}" == "1" ]]; then
    alarm="${alarm:+$alarm; }Ragtech Supervise reports warning"
  fi
  if [[ "${flag_hi_p_output:-0}" == "1" || "${fail_overload:-0}" == "1" ]]; then
    alarm="${alarm:+$alarm; }UPS overload"
  fi
  if [[ "${flag_no_battery:-0}" == "1" ]]; then
    alarm="${alarm:+$alarm; }Battery not detected"
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
    printf 'ragtech.sample.time: %s\n' "$dt"
    if [[ -n "$alarm" ]]; then
      printf 'ALARM [%s]\n' "$alarm"
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
