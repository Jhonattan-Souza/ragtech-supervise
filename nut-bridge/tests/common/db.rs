//! Builds Supervise databases with the schema captured from a live install.

use std::path::{Path, PathBuf};

use rusqlite::types::Value;
use rusqlite::{Connection, params};

const SCHEMA: &str = include_str!("../fixtures/supervise-8.9-schema.sql");

/// A Supervise `monit.db` in a temporary directory.
pub struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    /// A temporary directory with no database in it yet.
    pub fn empty() -> Self {
        Self {
            dir: tempfile::tempdir().expect("create a temporary directory"),
        }
    }

    /// A database with the Supervise schema and one device, `ups-1`.
    pub fn with_device() -> Self {
        let fixture = Self::with_schema();
        fixture.insert_device("ups-1", 1000, Some("Ragtech Test UPS"), Some("1.2.3"));
        fixture
    }

    /// A database with the Supervise schema and no rows.
    pub fn with_schema() -> Self {
        let fixture = Self::empty();
        fixture
            .connect()
            .execute_batch(SCHEMA)
            .expect("create the Supervise schema");
        fixture
    }

    pub fn path(&self) -> PathBuf {
        self.dir.path().join("monit.db")
    }

    pub fn dir(&self) -> &Path {
        self.dir.path()
    }

    pub fn connect(&self) -> Connection {
        Connection::open(self.path()).expect("open the fixture database")
    }

    pub fn insert_device(&self, id: &str, last: i64, model: Option<&str>, version: Option<&str>) {
        self.connect()
            .execute(
                "INSERT INTO DEVICELIST (id, family, model, userProd, version, last)
                 VALUES (?1, 10, 6, ?2, ?3, ?4)",
                params![id, model, version, last],
            )
            .expect("insert a device");
    }

    pub fn insert(&self, table: &str, row: &Row) {
        let sql = format!(
            "INSERT INTO {table} (
                id, dt, event,
                var_vInput, var_vOutput, var_iOutput, var_pOutput, var_fOutput,
                var_vBattery, var_cBattery, var_temperature,
                var_nominalVInput, var_nominalVOutput, var_nominalPOutput,
                var_nominalFOutput, var_nominalVBattery,
                flag_connected, flag_opBattery, flag_opWarning, flag_noVInput,
                flag_loBattery, flag_hiPOutput, flag_noBattery,
                fail_overload, fail_endBattery
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                       ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)"
        );
        self.connect()
            .execute(
                &sql,
                params![
                    row.id,
                    row.dt,
                    row.event,
                    row.input_voltage,
                    row.output_voltage,
                    row.output_current,
                    row.output_power,
                    row.output_frequency,
                    row.battery_voltage,
                    row.battery_charge,
                    row.temperature,
                    row.nominal_input_voltage,
                    row.nominal_output_voltage,
                    row.nominal_output_power,
                    row.nominal_output_frequency,
                    row.nominal_battery_voltage,
                    row.connected,
                    row.on_battery,
                    row.warning,
                    row.no_input_voltage,
                    row.low_battery,
                    row.high_output_power,
                    row.no_battery,
                    row.overload,
                    row.end_of_battery,
                ],
            )
            .expect("insert a sample row");
    }
}

/// One `EVENTLOG`/`HISTLOGHOUR` row; the defaults describe a healthy UPS on line.
pub struct Row {
    pub id: Value,
    pub dt: Value,
    pub event: Value,
    pub input_voltage: Value,
    pub output_voltage: Value,
    pub output_current: Value,
    pub output_power: Value,
    pub output_frequency: Value,
    pub battery_voltage: Value,
    pub battery_charge: Value,
    pub temperature: Value,
    pub nominal_input_voltage: Value,
    pub nominal_output_voltage: Value,
    pub nominal_output_power: Value,
    pub nominal_output_frequency: Value,
    pub nominal_battery_voltage: Value,
    pub connected: Value,
    pub on_battery: Value,
    pub warning: Value,
    pub no_input_voltage: Value,
    pub low_battery: Value,
    pub high_output_power: Value,
    pub no_battery: Value,
    pub overload: Value,
    pub end_of_battery: Value,
}

impl Default for Row {
    fn default() -> Self {
        Self {
            id: Value::Text("ups-1".to_owned()),
            dt: Value::Integer(1000),
            event: Value::Integer(1),
            input_voltage: Value::Real(127.2),
            output_voltage: Value::Real(127.0),
            output_current: Value::Real(1.0),
            output_power: Value::Real(42.0),
            output_frequency: Value::Real(60.0),
            battery_voltage: Value::Real(13.5),
            battery_charge: Value::Real(88.0),
            temperature: Value::Real(29.2),
            nominal_input_voltage: Value::Real(127.0),
            nominal_output_voltage: Value::Real(127.0),
            nominal_output_power: Value::Real(500.0),
            nominal_output_frequency: Value::Real(60.0),
            nominal_battery_voltage: Value::Real(12.0),
            connected: Value::Integer(1),
            on_battery: Value::Integer(0),
            warning: Value::Integer(0),
            no_input_voltage: Value::Integer(0),
            low_battery: Value::Integer(0),
            high_output_power: Value::Integer(0),
            no_battery: Value::Integer(0),
            overload: Value::Integer(0),
            end_of_battery: Value::Integer(0),
        }
    }
}

impl Row {
    /// The default row at time `dt` with event code `event`.
    pub fn at(dt: i64, event: i64) -> Self {
        Self {
            dt: Value::Integer(dt),
            event: Value::Integer(event),
            ..Self::default()
        }
    }
}
