//! Rendering of the NUT `dummy-ups` state file.

use std::fmt;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::supervise::{Reading, Sample};

/// A battery charge percentage, from 0 to 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChargePercent(u8);

impl ChargePercent {
    /// Returns the percentage, or `None` when `value` is above 100.
    #[must_use]
    pub fn new(value: u8) -> Option<Self> {
        (value <= 100).then_some(Self(value))
    }
}

/// What the bridge publishes to NUT.
#[derive(Debug, Clone, PartialEq)]
#[expect(
    clippy::large_enum_variant,
    reason = "one value per poll, never stored in bulk; boxing buys nothing"
)]
pub enum Telemetry {
    /// A sample accepted as live telemetry.
    Live(Reading),
    /// No live telemetry, for the given reason.
    Unavailable(Unavailable),
}

/// Why no live telemetry is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    /// The Supervise database cannot be opened for reading.
    DatabaseUnreadable,
    /// The latest-sample query failed.
    QueryFailed,
    /// The database holds no sample for the current device.
    NoCurrentSample,
    /// The sample was already in the database when the bridge started.
    StaleStartupSample,
    /// The sample has not changed for longer than the maximum sample age.
    StaleSourceSample,
    /// Supervise reports that it lost contact with the UPS.
    UpsDisconnected,
}

impl Unavailable {
    /// The reason as published in `experimental.ragtech.bridge.reason`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DatabaseUnreadable => "database-unreadable",
            Self::QueryFailed => "query-failed",
            Self::NoCurrentSample => "no-current-sample",
            Self::StaleStartupSample => "stale-startup-sample",
            Self::StaleSourceSample => "stale-source-sample",
            Self::UpsDisconnected => "ups-disconnected",
        }
    }

    fn connection_status(self) -> &'static str {
        match self {
            Self::UpsDisconnected => "disconnected",
            _ => "unavailable",
        }
    }
}

impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Variables published only for live telemetry. They are removed explicitly
/// when telemetry becomes unavailable, because `dummy-ups` keeps variables
/// between reads of the file.
const LIVE_ONLY_VARIABLES: [&str; 20] = [
    "device.serial",
    "ups.serial",
    "ups.firmware",
    "ups.load",
    "ups.power.nominal",
    "battery.charge",
    "battery.charger.status",
    "battery.voltage",
    "battery.voltage.nominal",
    "input.voltage",
    "input.voltage.nominal",
    "output.voltage",
    "output.voltage.nominal",
    "output.current",
    "output.frequency",
    "output.frequency.nominal",
    "ups.temperature",
    "experimental.ragtech.event",
    "experimental.ragtech.sample.source",
    "experimental.ragtech.sample.time",
];

/// Renders the `dummy-ups` state file for `telemetry`.
///
/// Live telemetry maps Supervise flags to `ups.status` (`OL` or `OB DISCHRG`,
/// plus `LB`, `OVER` and `RB`) and never emits `FSD`, which upsd owns. Alarms
/// go through the `ALARM` directive rather than `ups.alarm`. Unavailable
/// telemetry clears the status and removes every live variable explicitly,
/// because `dummy-ups` keeps variables between reads of the file. Text from
/// the database is reduced to printable ASCII and quoted.
///
/// # Examples
///
/// ```
/// use ragtech_nut_bridge::nut::{ChargePercent, Telemetry, Unavailable, render};
///
/// let low = ChargePercent::new(20).expect("20 is a percentage");
/// let file = render(&Telemetry::Unavailable(Unavailable::UpsDisconnected), low);
///
/// assert!(file.contains("\nups.status:\n"));
/// assert!(file.contains("experimental.ragtech.connection.status: disconnected\n"));
/// ```
#[must_use]
pub fn render(telemetry: &Telemetry, battery_charge_low: ChargePercent) -> String {
    let mut file = StateFile::default();
    match telemetry {
        Telemetry::Unavailable(reason) => {
            file.value("device.mfr", "Ragtech");
            file.value("device.model", "Supervise");
            file.value("device.type", "ups");
            file.value("ups.mfr", "Ragtech");
            file.value("ups.model", "Supervise");
            // dummy-ups resets the status for a bare `ups.status:`; the alarm
            // below then makes it `ALARM`.
            file.bare("ups.status");
            file.value("battery.charge.low", &battery_charge_low.0.to_string());
            file.value("experimental.ragtech.sample.valid", "0");
            file.value(
                "experimental.ragtech.connection.status",
                reason.connection_status(),
            );
            file.value("experimental.ragtech.bridge.reason", reason.as_str());
            for name in LIVE_ONLY_VARIABLES {
                file.optional(name, None);
            }
            file.alarm(&format!("Ragtech telemetry unavailable: {reason}"));
        }
        Telemetry::Live(reading) => {
            let sample = &reading.sample;
            let flags = sample.flags;
            let charge = reading.sample.battery_charge.map(charge_percent);
            let on_battery = flags.on_battery || flags.no_input_voltage;
            let (mut status, charger) = if on_battery {
                (vec!["OB", "DISCHRG"], "discharging")
            } else {
                (vec!["OL"], "unknown")
            };
            if flags.low_battery
                || flags.end_of_battery
                || charge.is_some_and(|charge| charge <= battery_charge_low.0)
            {
                status.push("LB");
            }
            let mut alarms = Vec::new();
            if flags.warning {
                alarms.push("Ragtech Supervise reports warning");
            }
            if flags.high_output_power || flags.overload {
                status.push("OVER");
                alarms.push("UPS overload");
            }
            if flags.no_battery {
                status.push("RB");
                alarms.push("Battery not detected");
            }
            let model =
                non_empty_text(reading.device.model.as_deref()).unwrap_or("Supervise".into());
            let firmware =
                non_empty_text(reading.device.firmware.as_deref()).unwrap_or("unknown".into());
            let serial = non_empty_text(Some(&sample.device_id));
            file.value("device.mfr", "Ragtech");
            file.text("device.model", Some(&model));
            file.text("device.serial", serial.as_deref());
            file.value("device.type", "ups");
            file.value("ups.mfr", "Ragtech");
            file.text("ups.model", Some(&model));
            file.text("ups.serial", serial.as_deref());
            file.text("ups.firmware", Some(&firmware));
            file.value("ups.status", &status.join(" "));
            file.optional(
                "ups.load",
                load_percent(sample).map(|load| load.to_string()),
            );
            file.optional(
                "ups.power.nominal",
                sample.nominal_output_power.and_then(decimal),
            );
            file.optional("battery.charge", charge.map(|charge| charge.to_string()));
            file.value("battery.charge.low", &battery_charge_low.0.to_string());
            file.value("battery.charger.status", charger);
            for (name, value) in [
                ("battery.voltage", sample.battery_voltage),
                ("battery.voltage.nominal", sample.nominal_battery_voltage),
                ("input.voltage", sample.input_voltage),
                ("input.voltage.nominal", sample.nominal_input_voltage),
                ("output.voltage", sample.output_voltage),
                ("output.voltage.nominal", sample.nominal_output_voltage),
                ("output.current", sample.output_current),
                ("output.frequency", sample.output_frequency),
                ("output.frequency.nominal", sample.nominal_output_frequency),
                ("ups.temperature", sample.temperature),
            ] {
                file.optional(name, value.and_then(decimal));
            }
            file.value("experimental.ragtech.event", &sample.event.to_string());
            file.value(
                "experimental.ragtech.sample.source",
                &sample.source.to_string(),
            );
            file.value("experimental.ragtech.sample.time", &sample.time.to_string());
            file.value("experimental.ragtech.sample.valid", "1");
            file.value("experimental.ragtech.connection.status", "connected");
            file.value("experimental.ragtech.bridge.reason", "live-sample");
            file.alarm(&alarms.join("; "));
        }
    }
    file.finish()
}

/// Clamps a Supervise charge reading to 0..=100 and rounds it half to even,
/// like the `%.0f` formatting the shell exporter used.
fn charge_percent(value: f64) -> u8 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the value is clamped to 0..=100 and rounded before the cast"
    )]
    let percent = value.clamp(0.0, 100.0).round_ties_even() as u8;
    percent
}

/// Derives the output load percentage.
///
/// Some Supervise versions store percent load in `var_pOutput` and others
/// store output power. A value in 0..=100 is taken as a percentage when it
/// agrees with the apparent load (voltage x current / nominal power) within
/// 35% of the apparent load, or 10 points, whichever is larger; otherwise it
/// is taken as power and divided by the nominal power.
fn load_percent(sample: &Sample) -> Option<u8> {
    let power = sample.output_power;
    let load = match sample.nominal_output_power.filter(|nominal| *nominal > 0.0) {
        None => power?,
        Some(nominal) => {
            let apparent = match (sample.output_voltage, sample.output_current) {
                (Some(voltage), Some(current)) if voltage > 0.0 && current > 0.0 => {
                    Some(voltage * current / nominal * 100.0)
                }
                _ => None,
            };
            match (power, apparent) {
                (None, apparent) => apparent?,
                (Some(power), apparent) if (0.0..=100.0).contains(&power) => match apparent {
                    None => power,
                    Some(apparent) => {
                        let tolerance = (apparent * 0.35).max(10.0);
                        if (power - apparent).abs() <= tolerance {
                            power
                        } else {
                            power / nominal * 100.0
                        }
                    }
                },
                (Some(power), _) => power / nominal * 100.0,
            }
        }
    };
    Some(charge_percent(load))
}

/// Keeps the printable ASCII of a database string, with tabs turned into
/// spaces, and returns `None` when nothing is left.
fn non_empty_text(value: Option<&str>) -> Option<String> {
    let text: String = value?
        .chars()
        .map(|ch| if ch == '\t' { ' ' } else { ch })
        .filter(|ch| matches!(ch, ' '..='~'))
        .collect();
    (!text.is_empty()).then_some(text)
}

/// Measurements at or above this magnitude are treated as missing. The shell
/// exporter saw them in exponent form and dropped them as non-numeric, and
/// written out in full they would overflow dummy-ups' 256-byte value buffer.
const MAX_MEASUREMENT: f64 = 1e15;

/// Formats a measurement with one decimal place, or `None` when it is not a
/// plausible reading.
fn decimal(value: f64) -> Option<String> {
    (value.abs() < MAX_MEASUREMENT).then(|| format!("{value:.1}"))
}

/// Accumulates `name: value` lines in `dummy-ups` definition file syntax.
#[derive(Default)]
struct StateFile {
    text: String,
}

impl StateFile {
    fn value(&mut self, name: &str, value: &str) {
        self.text.push_str(name);
        self.text.push_str(": ");
        self.text.push_str(value);
        self.text.push('\n');
    }

    /// Writes `name:` with no value.
    fn bare(&mut self, name: &str) {
        self.text.push_str(name);
        self.text.push_str(":\n");
    }

    /// Writes `value`, or the explicit empty token that makes `dummy-ups`
    /// remove the variable. A bare `name:` line would instead make the
    /// driver reuse the previous line's value.
    fn optional(&mut self, name: &str, value: Option<String>) {
        match value {
            Some(value) => self.value(name, &value),
            None => self.value(name, "\"\""),
        }
    }

    /// Writes a text value from the database as one double-quoted word, with
    /// `"`, `\` and `#` escaped, so the driver's parser keeps them and runs of
    /// spaces literally (NUT's parseconf rejects the whole line for an
    /// unescaped `#` inside quotes). `text` must already be single-line
    /// printable text.
    fn text(&mut self, name: &str, text: Option<&str>) {
        let mut quoted = String::from("\"");
        for ch in text.unwrap_or_default().chars() {
            if matches!(ch, '"' | '\\' | '#') {
                quoted.push('\\');
            }
            quoted.push(ch);
        }
        quoted.push('"');
        self.value(name, &quoted);
    }

    /// Writes the `ALARM` directive; an empty message clears the alarm.
    fn alarm(&mut self, message: &str) {
        self.text.push_str("ALARM");
        if !message.is_empty() {
            self.text.push(' ');
            self.text.push_str(message);
        }
        self.text.push('\n');
    }

    fn finish(self) -> String {
        self.text
    }
}

/// Atomically replaces the state file at `path` with `contents`.
///
/// The file is written to a temporary file in the same directory, made
/// world-readable (0644) for the `nut` user, and renamed over `path`, so the
/// driver never reads a partial file.
///
/// # Errors
///
/// Any I/O error from creating, writing, or renaming the temporary file.
///
/// # Examples
///
/// ```
/// # fn main() -> std::io::Result<()> {
/// use ragtech_nut_bridge::nut::write_state_file;
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("ragtech.dev");
/// write_state_file(&path, "ups.status: OL\n")?;
/// assert_eq!(std::fs::read_to_string(&path)?, "ups.status: OL\n");
/// # Ok(())
/// # }
/// ```
pub fn write_state_file(path: &Path, contents: &str) -> io::Result<()> {
    let dir = path.parent().filter(|dir| !dir.as_os_str().is_empty());
    let mut file = tempfile::Builder::new()
        .prefix(".ragtech-nut-")
        .tempfile_in(dir.unwrap_or_else(|| Path::new(".")))?;
    file.write_all(contents.as_bytes())?;
    file.as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o644))?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}
