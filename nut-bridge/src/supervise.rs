//! Samples stored by Ragtech Supervise.

use std::fmt;
use std::fs::File;
use std::path::PathBuf;
use std::time::Duration;

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags, OptionalExtension};

/// The Supervise table a sample was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleSource {
    /// `EVENTLOG`: rows written when the UPS reports an event.
    EventLog,
    /// `HISTLOGHOUR`: periodic history rows.
    HistLogHour,
}

impl fmt::Display for SampleSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EventLog => "EVENTLOG",
            Self::HistLogHour => "HISTLOGHOUR",
        })
    }
}

/// Status flags reported by Supervise for one sample.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flags {
    /// `flag_connected`: Supervise is talking to the UPS.
    pub connected: bool,
    /// `flag_opBattery`: the load is running from the battery.
    pub on_battery: bool,
    /// `flag_opWarning`: Supervise reports a warning condition.
    pub warning: bool,
    /// `flag_noVInput`: no input voltage.
    pub no_input_voltage: bool,
    /// `flag_loBattery`: the battery is low.
    pub low_battery: bool,
    /// `flag_hiPOutput`: output power is above the rated limit.
    pub high_output_power: bool,
    /// `flag_noBattery`: no battery detected.
    pub no_battery: bool,
    /// `fail_overload`: the UPS reports an overload fault.
    pub overload: bool,
    /// `fail_endBattery`: the battery is exhausted.
    pub end_of_battery: bool,
}

/// One telemetry row from `EVENTLOG` or `HISTLOGHOUR`.
///
/// Measurements are `None` when the column is NULL or not numeric.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    /// Supervise device id (`DEVICELIST.id`).
    pub device_id: String,
    /// Sample timestamp (`dt`), in Supervise's own units.
    pub time: i64,
    /// Event code (`event`).
    pub event: i64,
    /// Table the sample came from.
    pub source: SampleSource,
    /// Input voltage, in volts.
    pub input_voltage: Option<f64>,
    /// Output voltage, in volts.
    pub output_voltage: Option<f64>,
    /// Output current, in amperes.
    pub output_current: Option<f64>,
    /// `var_pOutput`: output power, or load percent on some Supervise versions.
    pub output_power: Option<f64>,
    /// Output frequency, in hertz.
    pub output_frequency: Option<f64>,
    /// Battery voltage, in volts.
    pub battery_voltage: Option<f64>,
    /// Battery charge, in percent.
    pub battery_charge: Option<f64>,
    /// UPS temperature, in degrees Celsius.
    pub temperature: Option<f64>,
    /// Nominal input voltage, in volts.
    pub nominal_input_voltage: Option<f64>,
    /// Nominal output voltage, in volts.
    pub nominal_output_voltage: Option<f64>,
    /// Nominal output power.
    pub nominal_output_power: Option<f64>,
    /// Nominal output frequency, in hertz.
    pub nominal_output_frequency: Option<f64>,
    /// Nominal battery voltage, in volts.
    pub nominal_battery_voltage: Option<f64>,
    /// Status flags.
    pub flags: Flags,
}

/// Device metadata from `DEVICELIST`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceInfo {
    /// `userProd`: product name, when Supervise knows it.
    pub model: Option<String>,
    /// `version`: firmware version, when Supervise knows it.
    pub firmware: Option<String>,
}

/// The latest sample together with its device metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    /// The telemetry row.
    pub sample: Sample,
    /// The device the row belongs to.
    pub device: DeviceInfo,
}

/// Why the latest sample could not be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReadError {
    /// The database file cannot be opened for reading.
    #[error("cannot read {}", path.display())]
    Unreadable {
        /// The database path.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// SQLite failed to open the database or run the query.
    #[error("failed to query {}", path.display())]
    Query {
        /// The database path.
        path: PathBuf,
        /// The underlying SQLite error.
        #[source]
        source: rusqlite::Error,
    },
}

/// The Supervise `monit.db` SQLite database, opened read-only on every read.
///
/// Opening per read keeps no file handle between polls, so a database that
/// Supervise recreates is picked up on the next read.
#[derive(Debug, Clone)]
pub struct SuperviseDb {
    path: PathBuf,
}

/// How long a read waits for a lock held by Supervise before failing.
const BUSY_TIMEOUT: Duration = Duration::from_secs(2);

/// Selects the newest sample of the most recently seen device from both
/// sample tables. Each branch can use the `(id, dt, event)` primary key.
const LATEST_SAMPLE_QUERY: &str = "
WITH device AS (
  SELECT id FROM DEVICELIST ORDER BY last DESC LIMIT 1
),
samples AS (
  SELECT * FROM (
    SELECT 'EVENTLOG' AS sample_source, id, dt, event,
      var_vInput, var_vOutput, var_iOutput, var_pOutput, var_fOutput,
      var_vBattery, var_cBattery, var_temperature,
      var_nominalVInput, var_nominalVOutput, var_nominalPOutput,
      var_nominalFOutput, var_nominalVBattery,
      flag_connected, flag_opBattery, flag_opWarning, flag_noVInput,
      flag_loBattery, flag_hiPOutput, flag_noBattery, fail_overload, fail_endBattery
    FROM EVENTLOG
    WHERE id = (SELECT id FROM device)
    ORDER BY dt DESC, event DESC
    LIMIT 1
  )
  UNION ALL
  SELECT * FROM (
    SELECT 'HISTLOGHOUR' AS sample_source, id, dt, event,
      var_vInput, var_vOutput, var_iOutput, var_pOutput, var_fOutput,
      var_vBattery, var_cBattery, var_temperature,
      var_nominalVInput, var_nominalVOutput, var_nominalPOutput,
      var_nominalFOutput, var_nominalVBattery,
      flag_connected, flag_opBattery, flag_opWarning, flag_noVInput,
      flag_loBattery, flag_hiPOutput, flag_noBattery, fail_overload, fail_endBattery
    FROM HISTLOGHOUR
    WHERE id = (SELECT id FROM device)
    ORDER BY dt DESC, event DESC
    LIMIT 1
  )
)
SELECT
  e.sample_source, e.id,
  CAST(COALESCE(e.dt, 0) AS INTEGER), CAST(COALESCE(e.event, 0) AS INTEGER),
  e.var_vInput, e.var_vOutput, e.var_iOutput, e.var_pOutput, e.var_fOutput,
  e.var_vBattery, e.var_cBattery, e.var_temperature,
  e.var_nominalVInput, e.var_nominalVOutput, e.var_nominalPOutput,
  e.var_nominalFOutput, e.var_nominalVBattery,
  e.flag_connected, e.flag_opBattery, e.flag_opWarning, e.flag_noVInput,
  e.flag_loBattery, e.flag_hiPOutput, e.flag_noBattery, e.fail_overload, e.fail_endBattery,
  d.userProd, d.version
FROM samples e
LEFT JOIN DEVICELIST d ON d.id = e.id
ORDER BY e.dt DESC, e.event DESC, e.sample_source ASC
LIMIT 1";

impl SuperviseDb {
    /// A handle on the database at `path`. Nothing is opened until a read.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Reads the newest sample of the most recently seen device.
    ///
    /// Returns `Ok(None)` when the device has no samples. The database is
    /// opened read-only and is never created. Each attempt waits up to 2 s
    /// for locks, and a failed attempt is retried once.
    ///
    /// # Errors
    ///
    /// [`ReadError::Unreadable`] when the file cannot be opened for reading,
    /// and [`ReadError::Query`] when SQLite cannot open it as a database or
    /// the query fails.
    pub fn latest_reading(&self) -> Result<Option<Reading>, ReadError> {
        File::open(&self.path).map_err(|source| ReadError::Unreadable {
            path: self.path.clone(),
            source,
        })?;
        // One retry rides out a lock held past the busy timeout.
        self.query()
            .or_else(|_| self.query())
            .map_err(|source| ReadError::Query {
                path: self.path.clone(),
                source,
            })
    }

    fn query(&self) -> rusqlite::Result<Option<Reading>> {
        let connection = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.busy_timeout(BUSY_TIMEOUT)?;
        connection
            .query_row(LATEST_SAMPLE_QUERY, [], reading_from_row)
            .optional()
    }
}

fn reading_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Reading> {
    let source = match row.get_ref(0)?.as_str()? {
        "EVENTLOG" => SampleSource::EventLog,
        _ => SampleSource::HistLogHour,
    };
    let measurement = |index| row.get_ref(index).map(measurement);
    let flag = |index| row.get_ref(index).map(flag);
    Ok(Reading {
        sample: Sample {
            device_id: text(row.get_ref(1)?).unwrap_or_default(),
            time: row.get(2)?,
            event: row.get(3)?,
            source,
            input_voltage: measurement(4)?,
            output_voltage: measurement(5)?,
            output_current: measurement(6)?,
            output_power: measurement(7)?,
            output_frequency: measurement(8)?,
            battery_voltage: measurement(9)?,
            battery_charge: measurement(10)?,
            temperature: measurement(11)?,
            nominal_input_voltage: measurement(12)?,
            nominal_output_voltage: measurement(13)?,
            nominal_output_power: measurement(14)?,
            nominal_output_frequency: measurement(15)?,
            nominal_battery_voltage: measurement(16)?,
            flags: Flags {
                connected: flag(17)?,
                on_battery: flag(18)?,
                warning: flag(19)?,
                no_input_voltage: flag(20)?,
                low_battery: flag(21)?,
                high_output_power: flag(22)?,
                no_battery: flag(23)?,
                overload: flag(24)?,
                end_of_battery: flag(25)?,
            },
        },
        device: DeviceInfo {
            model: text(row.get_ref(26)?),
            firmware: text(row.get_ref(27)?),
        },
    })
}

/// A numeric column value; NULL, blobs and non-numeric text are `None`.
fn measurement(value: ValueRef<'_>) -> Option<f64> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "Supervise measurements are far below 2^52"
    )]
    match value {
        ValueRef::Integer(value) => Some(value as f64),
        ValueRef::Real(value) => Some(value),
        ValueRef::Text(text) => std::str::from_utf8(text)
            .ok()
            .filter(|text| is_decimal(text))
            .and_then(|text| text.parse().ok()),
        ValueRef::Null | ValueRef::Blob(_) => None,
    }
}

/// Matches `-?[0-9]+(\.[0-9]+)?`, the numbers Supervise writes as text.
fn is_decimal(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, "1"));
    [whole, fraction]
        .iter()
        .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

/// A flag column is set when it holds the number 1.
fn flag(value: ValueRef<'_>) -> bool {
    #[expect(
        clippy::float_cmp,
        reason = "flags hold small integers, which are exact in f64"
    )]
    measurement(value).is_some_and(|value| value == 1.0)
}

/// A column as text; NULL and blobs are `None`.
fn text(value: ValueRef<'_>) -> Option<String> {
    match value {
        ValueRef::Text(text) => Some(String::from_utf8_lossy(text).into_owned()),
        ValueRef::Integer(value) => Some(value.to_string()),
        ValueRef::Real(value) => Some(value.to_string()),
        ValueRef::Null | ValueRef::Blob(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `EVENTLOG` and `HISTLOGHOUR` grow for as long as Supervise runs, so the
    /// latest-sample query must seek the `(id, dt, event)` primary key rather
    /// than scan either table.
    #[test]
    fn latest_sample_query_seeks_the_primary_key_of_both_sample_tables() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(include_str!("../tests/fixtures/supervise-8.9-schema.sql"))
            .unwrap();
        let mut statement = connection
            .prepare(&format!("EXPLAIN QUERY PLAN {LATEST_SAMPLE_QUERY}"))
            .unwrap();
        let plan: Vec<String> = statement
            .query_map([], |row| row.get::<_, String>(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let plan = plan.join("\n");

        for table in ["EVENTLOG", "HISTLOGHOUR"] {
            assert!(
                plan.contains(&format!(
                    "SEARCH {table} USING INDEX sqlite_autoindex_{table}_1"
                )),
                "{plan}"
            );
            assert!(!plan.contains(&format!("SCAN {table}")), "{plan}");
        }
    }
}
