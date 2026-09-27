//! Shared fixtures for the integration tests.

#![allow(
    dead_code,
    reason = "each test binary uses a different subset of the fixtures"
)]

pub mod db;

use ragtech_nut_bridge::supervise::{DeviceInfo, Flags, Reading, Sample, SampleSource};

/// A connected UPS on line power, matching the defaults of the SQLite fixture.
pub fn online_reading() -> Reading {
    Reading {
        sample: Sample {
            device_id: "ups-1".to_owned(),
            time: 1000,
            event: 1,
            source: SampleSource::EventLog,
            input_voltage: Some(127.2),
            output_voltage: Some(127.0),
            output_current: Some(1.0),
            output_power: Some(42.0),
            output_frequency: Some(60.0),
            battery_voltage: Some(13.5),
            battery_charge: Some(88.0),
            temperature: Some(29.2),
            nominal_input_voltage: Some(127.0),
            nominal_output_voltage: Some(127.0),
            nominal_output_power: Some(500.0),
            nominal_output_frequency: Some(60.0),
            nominal_battery_voltage: Some(12.0),
            flags: Flags {
                connected: true,
                ..Flags::default()
            },
        },
        device: DeviceInfo {
            model: Some("Ragtech Test UPS".to_owned()),
            firmware: Some("1.2.3".to_owned()),
        },
    }
}

/// Reads `key` from a dummy-ups state file the way NUT's parseconf sees it: a
/// double-quoted value is one word in which `\` takes the next character
/// literally, and `""` is the explicit empty value. Returns `None` when the
/// key is missing or parseconf would reject the line (an unescaped `#` inside
/// quotes).
pub fn nut_value(file: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    let rest = file.lines().find_map(|line| line.strip_prefix(&prefix))?;
    let value = rest.strip_prefix(' ').unwrap_or(rest);
    match value.strip_prefix('"') {
        Some(quoted) => unquote(quoted),
        None => Some(value.to_owned()),
    }
}

fn unquote(quoted: &str) -> Option<String> {
    let mut value = String::new();
    let mut chars = quoted.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => value.extend(chars.next()),
            '"' => return Some(value),
            '#' => return None,
            ch => value.push(ch),
        }
    }
    None
}

/// Returns the `ALARM` directive line, if any.
pub fn alarm_line(file: &str) -> Option<&str> {
    file.lines()
        .find(|line| *line == "ALARM" || line.starts_with("ALARM "))
}
