sql_literal() {
  local value="${1-}"

  if [[ "$value" == "__NULL__" ]]; then
    printf 'NULL'
    return
  fi

  value="${value//\'/\'\'}"
  printf "'%s'" "$value"
}

create_ragtech_schema() {
  local db="$1"

  sqlite3 "$db" <<'SQL'
CREATE TABLE DEVICELIST (
  id TEXT PRIMARY KEY,
  last INTEGER,
  userProd TEXT,
  version TEXT
);

CREATE TABLE EVENTLOG (
  id TEXT,
  dt INTEGER,
  event INTEGER,
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
  flag_connected INTEGER,
  flag_opBattery INTEGER,
  flag_opWarning INTEGER,
  flag_noVInput INTEGER,
  flag_loBattery INTEGER,
  flag_hiPOutput INTEGER,
  flag_noBattery INTEGER,
  fail_overload INTEGER,
  fail_endBattery INTEGER,
  PRIMARY KEY (id, dt, event)
);

CREATE TABLE HISTLOGHOUR (
  id TEXT,
  dt INTEGER,
  event INTEGER,
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
  flag_connected INTEGER,
  flag_opBattery INTEGER,
  flag_opWarning INTEGER,
  flag_noVInput INTEGER,
  flag_loBattery INTEGER,
  flag_hiPOutput INTEGER,
  flag_noBattery INTEGER,
  fail_overload INTEGER,
  fail_endBattery INTEGER,
  PRIMARY KEY (id, dt, event)
);
SQL
}

insert_device() {
  local db="$1"
  local id="${2:-ups-1}"
  local last="${3:-1000}"
  local model="${4:-Ragtech Test UPS}"
  local version="${5:-1.2.3}"

  sqlite3 "$db" "INSERT INTO DEVICELIST (id, last, userProd, version) VALUES ($(sql_literal "$id"), $last, $(sql_literal "$model"), $(sql_literal "$version"));"
}

insert_sample() {
  local db="$1"
  local table="${2:-EVENTLOG}"
  local id="${SAMPLE_ID:-ups-1}"

  sqlite3 "$db" <<SQL
INSERT INTO $table (
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
) VALUES (
  $(sql_literal "$id"),
  $(sql_literal "${SAMPLE_DT:-1000}"),
  $(sql_literal "${SAMPLE_EVENT:-1}"),
  $(sql_literal "${SAMPLE_V_INPUT:-127.2}"),
  $(sql_literal "${SAMPLE_V_OUTPUT:-127.0}"),
  $(sql_literal "${SAMPLE_I_OUTPUT:-1.0}"),
  $(sql_literal "${SAMPLE_P_OUTPUT:-42}"),
  $(sql_literal "${SAMPLE_F_OUTPUT:-60.0}"),
  $(sql_literal "${SAMPLE_V_BATTERY:-13.5}"),
  $(sql_literal "${SAMPLE_C_BATTERY:-88}"),
  $(sql_literal "${SAMPLE_TEMPERATURE:-29.2}"),
  $(sql_literal "${SAMPLE_NOMINAL_V_INPUT:-127}"),
  $(sql_literal "${SAMPLE_NOMINAL_V_OUTPUT:-127}"),
  $(sql_literal "${SAMPLE_NOMINAL_P_OUTPUT:-500}"),
  $(sql_literal "${SAMPLE_NOMINAL_F_OUTPUT:-60}"),
  $(sql_literal "${SAMPLE_NOMINAL_V_BATTERY:-12}"),
  $(sql_literal "${SAMPLE_CONNECTED:-1}"),
  $(sql_literal "${SAMPLE_OP_BATTERY:-0}"),
  $(sql_literal "${SAMPLE_OP_WARNING:-0}"),
  $(sql_literal "${SAMPLE_NO_V_INPUT:-0}"),
  $(sql_literal "${SAMPLE_LO_BATTERY:-0}"),
  $(sql_literal "${SAMPLE_HI_P_OUTPUT:-0}"),
  $(sql_literal "${SAMPLE_NO_BATTERY:-0}"),
  $(sql_literal "${SAMPLE_FAIL_OVERLOAD:-0}"),
  $(sql_literal "${SAMPLE_FAIL_END_BATTERY:-0}")
);
SQL
}
