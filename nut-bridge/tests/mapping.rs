//! Supervise sample to dummy-ups state file mapping, through the public API.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "a failed fixture or assertion should panic the test"
)]

mod common;

use common::{alarm_line, nut_value, online_reading};
use ragtech_nut_bridge::nut::{ChargePercent, Telemetry, Unavailable, render};

fn low() -> ChargePercent {
    ChargePercent::new(20).expect("20 is a valid percentage")
}

#[test]
fn online_sample_is_reported_on_line_as_a_valid_live_sample() {
    let file = render(&Telemetry::Live(online_reading()), low());

    assert_eq!(nut_value(&file, "ups.status").as_deref(), Some("OL"));
    assert_eq!(
        nut_value(&file, "experimental.ragtech.sample.valid").as_deref(),
        Some("1")
    );
    assert_eq!(
        nut_value(&file, "experimental.ragtech.bridge.reason").as_deref(),
        Some("live-sample")
    );
}

#[test]
fn battery_operation_or_missing_input_voltage_is_reported_as_discharging_on_battery() {
    let mut on_battery = online_reading();
    on_battery.sample.flags.on_battery = true;
    let mut no_input = online_reading();
    no_input.sample.flags.no_input_voltage = true;

    for reading in [on_battery, no_input] {
        let file = render(&Telemetry::Live(reading.clone()), low());

        assert_eq!(
            nut_value(&file, "ups.status").as_deref(),
            Some("OB DISCHRG")
        );
        assert_eq!(
            nut_value(&file, "battery.charger.status").as_deref(),
            Some("discharging")
        );
    }
}

#[test]
fn low_or_exhausted_battery_flags_add_low_battery_to_the_status() {
    let mut low_flag = online_reading();
    low_flag.sample.flags.on_battery = true;
    low_flag.sample.flags.low_battery = true;
    let mut end_flag = online_reading();
    end_flag.sample.flags.on_battery = true;
    end_flag.sample.flags.end_of_battery = true;

    for reading in [low_flag, end_flag] {
        let file = render(&Telemetry::Live(reading.clone()), low());

        assert_eq!(
            nut_value(&file, "ups.status").as_deref(),
            Some("OB DISCHRG LB")
        );
    }
}

#[test]
fn charge_at_or_below_the_configured_threshold_is_low_battery() {
    let mut at_threshold = online_reading();
    at_threshold.sample.battery_charge = Some(20.0);
    let mut above_threshold = online_reading();
    above_threshold.sample.battery_charge = Some(21.0);

    let at = render(&Telemetry::Live(at_threshold.clone()), low());
    let above = render(&Telemetry::Live(above_threshold.clone()), low());

    assert_eq!(nut_value(&at, "ups.status").as_deref(), Some("OL LB"));
    assert_eq!(nut_value(&above, "ups.status").as_deref(), Some("OL"));
}

#[test]
fn healthy_sample_clears_the_alarm() {
    let file = render(&Telemetry::Live(online_reading()), low());

    assert_eq!(alarm_line(&file), Some("ALARM"));
}

#[test]
fn overload_is_reported_in_status_and_alarm() {
    let mut high_power = online_reading();
    high_power.sample.flags.high_output_power = true;
    let mut overload_fault = online_reading();
    overload_fault.sample.flags.overload = true;

    for reading in [high_power, overload_fault] {
        let file = render(&Telemetry::Live(reading.clone()), low());

        assert_eq!(nut_value(&file, "ups.status").as_deref(), Some("OL OVER"));
        assert_eq!(alarm_line(&file), Some("ALARM UPS overload"));
    }
}

#[test]
fn missing_battery_is_reported_as_replace_battery() {
    let mut reading = online_reading();
    reading.sample.flags.no_battery = true;

    let file = render(&Telemetry::Live(reading.clone()), low());

    assert_eq!(nut_value(&file, "ups.status").as_deref(), Some("OL RB"));
    assert_eq!(alarm_line(&file), Some("ALARM Battery not detected"));
}

#[test]
fn warning_and_fault_alarms_are_combined_in_one_alarm() {
    let mut reading = online_reading();
    reading.sample.flags.warning = true;
    reading.sample.flags.overload = true;
    reading.sample.flags.no_battery = true;

    let file = render(&Telemetry::Live(reading.clone()), low());

    assert_eq!(
        nut_value(&file, "ups.status").as_deref(),
        Some("OL OVER RB")
    );
    assert_eq!(
        alarm_line(&file),
        Some("ALARM Ragtech Supervise reports warning; UPS overload; Battery not detected")
    );
}

#[test]
fn measurements_are_published_with_one_decimal() {
    let file = render(&Telemetry::Live(online_reading()), low());

    for (key, expected) in [
        ("battery.charge", "88"),
        ("battery.charge.low", "20"),
        ("battery.voltage", "13.5"),
        ("battery.voltage.nominal", "12.0"),
        ("input.voltage", "127.2"),
        ("input.voltage.nominal", "127.0"),
        ("output.voltage", "127.0"),
        ("output.voltage.nominal", "127.0"),
        ("output.current", "1.0"),
        ("output.frequency", "60.0"),
        ("output.frequency.nominal", "60.0"),
        ("ups.power.nominal", "500.0"),
        ("ups.temperature", "29.2"),
    ] {
        assert_eq!(nut_value(&file, key).as_deref(), Some(expected), "{key}");
    }
}

#[test]
fn charge_is_clamped_to_a_percentage() {
    let mut over = online_reading();
    over.sample.battery_charge = Some(150.0);
    let mut under = online_reading();
    under.sample.battery_charge = Some(-3.0);
    let mut real_world = online_reading();
    real_world.sample.battery_charge = Some(100.214_996_337_891);

    let charge = |reading| nut_value(&render(&Telemetry::Live(reading), low()), "battery.charge");

    assert_eq!(charge(over).as_deref(), Some("100"));
    assert_eq!(charge(under).as_deref(), Some("0"));
    assert_eq!(charge(real_world).as_deref(), Some("100"));
}

#[test]
fn missing_measurements_are_removed_with_an_explicit_empty_token() {
    let mut reading = online_reading();
    reading.sample.battery_voltage = None;
    reading.sample.battery_charge = None;
    reading.sample.nominal_output_power = None;

    let file = render(&Telemetry::Live(reading.clone()), low());

    // A bare `name:` line makes dummy-ups reuse the previous line's value.
    assert!(file.contains("battery.voltage: \"\"\n"), "{file}");
    assert!(file.contains("battery.charge: \"\"\n"), "{file}");
    assert!(file.contains("ups.power.nominal: \"\"\n"), "{file}");
}

/// `var_pOutput` holds output power on some Supervise versions and percent load
/// on others; the load is cross-checked against voltage x current / nominal.
#[test]
fn load_is_derived_from_power_or_apparent_output() {
    struct Case {
        power: Option<f64>,
        nominal: Option<f64>,
        voltage: Option<f64>,
        current: Option<f64>,
        load: &'static str,
    }
    let cases = [
        // Percent-like power disagrees with the 25.4% apparent load: power/nominal.
        Case {
            power: Some(42.0),
            nominal: Some(500.0),
            voltage: Some(127.0),
            current: Some(1.0),
            load: "8",
        },
        Case {
            power: Some(90.0),
            nominal: Some(500.0),
            voltage: Some(100.0),
            current: Some(1.0),
            load: "18",
        },
        // Percent-like power agrees with the apparent load: already a percentage.
        Case {
            power: Some(25.0),
            nominal: Some(500.0),
            voltage: Some(127.0),
            current: Some(1.0),
            load: "25",
        },
        // A real Ragtech family-10 row.
        Case {
            power: Some(15.0),
            nominal: Some(1200.0),
            voltage: Some(112.665),
            current: Some(1.727),
            load: "15",
        },
        // No apparent load to compare against: trust the percent-like value.
        Case {
            power: Some(42.0),
            nominal: Some(500.0),
            voltage: Some(127.0),
            current: Some(0.0),
            load: "42",
        },
        // Power outside 0..=100 is watts.
        Case {
            power: Some(600.0),
            nominal: Some(1200.0),
            voltage: None,
            current: None,
            load: "50",
        },
        // No nominal power: the value is taken as a percentage.
        Case {
            power: Some(70.0),
            nominal: None,
            voltage: None,
            current: None,
            load: "70",
        },
        // No power: fall back to the apparent load.
        Case {
            power: None,
            nominal: Some(500.0),
            voltage: Some(100.0),
            current: Some(1.0),
            load: "20",
        },
        // Results are clamped to a percentage.
        Case {
            power: Some(900.0),
            nominal: Some(500.0),
            voltage: None,
            current: None,
            load: "100",
        },
        Case {
            power: None,
            nominal: None,
            voltage: Some(127.0),
            current: Some(1.0),
            load: "",
        },
        Case {
            power: None,
            nominal: Some(500.0),
            voltage: None,
            current: Some(1.0),
            load: "",
        },
    ];

    for case in cases {
        let mut reading = online_reading();
        reading.sample.output_power = case.power;
        reading.sample.nominal_output_power = case.nominal;
        reading.sample.output_voltage = case.voltage;
        reading.sample.output_current = case.current;

        let file = render(&Telemetry::Live(reading.clone()), low());

        assert_eq!(
            nut_value(&file, "ups.load").as_deref(),
            Some(case.load),
            "power={:?} nominal={:?} voltage={:?} current={:?}",
            case.power,
            case.nominal,
            case.voltage,
            case.current
        );
    }
}

#[test]
fn device_identity_and_sample_metadata_are_published() {
    let file = render(&Telemetry::Live(online_reading()), low());

    for (key, expected) in [
        ("device.mfr", "Ragtech"),
        ("device.model", "Ragtech Test UPS"),
        ("device.serial", "ups-1"),
        ("device.type", "ups"),
        ("ups.mfr", "Ragtech"),
        ("ups.model", "Ragtech Test UPS"),
        ("ups.serial", "ups-1"),
        ("ups.firmware", "1.2.3"),
        ("experimental.ragtech.event", "1"),
        ("experimental.ragtech.sample.source", "EVENTLOG"),
        ("experimental.ragtech.sample.time", "1000"),
        ("experimental.ragtech.connection.status", "connected"),
    ] {
        assert_eq!(nut_value(&file, key).as_deref(), Some(expected), "{key}");
    }
}

#[test]
fn unknown_device_metadata_falls_back_to_placeholders() {
    let mut reading = online_reading();
    reading.device.model = None;
    reading.device.firmware = Some(String::new());
    reading.sample.device_id = String::new();

    let file = render(&Telemetry::Live(reading.clone()), low());

    assert_eq!(
        nut_value(&file, "device.model").as_deref(),
        Some("Supervise")
    );
    assert_eq!(nut_value(&file, "ups.firmware").as_deref(), Some("unknown"));
    assert_eq!(nut_value(&file, "device.serial").as_deref(), Some(""));
}

#[test]
fn database_text_cannot_inject_lines_or_parser_syntax() {
    let mut reading = online_reading();
    reading.sample.device_id = "ups\nINJECT: bad".to_owned();
    reading.device.model = Some("Model\rups.status: OB LB\u{1}".to_owned());
    reading.device.firmware = Some("1.2.3\tALARM [bad] # \"quoted\" \\".to_owned());

    let file = render(&Telemetry::Live(reading.clone()), low());

    assert_eq!(
        nut_value(&file, "device.serial").as_deref(),
        Some("upsINJECT: bad")
    );
    assert_eq!(
        nut_value(&file, "device.model").as_deref(),
        Some("Modelups.status: OB LB")
    );
    assert_eq!(
        nut_value(&file, "ups.firmware").as_deref(),
        Some("1.2.3 ALARM [bad] # \"quoted\" \\")
    );
    // NUT's parseconf rejects the whole line for an unescaped `#` inside
    // quotes, and takes any character after `\` literally.
    assert!(
        file.contains("ups.firmware: \"1.2.3 ALARM [bad] \\# \\\"quoted\\\" \\\\\"\n"),
        "{file}"
    );
    assert_eq!(nut_value(&file, "ups.status").as_deref(), Some("OL"));
    assert!(
        !file.lines().any(|line| line.starts_with("INJECT")),
        "{file}"
    );
    assert_eq!(common::alarm_line(&file), Some("ALARM"));
}

#[test]
fn unavailable_telemetry_publishes_the_reason_and_clears_the_status() {
    let cases = [
        (
            Unavailable::DatabaseUnreadable,
            "database-unreadable",
            "unavailable",
        ),
        (Unavailable::QueryFailed, "query-failed", "unavailable"),
        (
            Unavailable::NoCurrentSample,
            "no-current-sample",
            "unavailable",
        ),
        (
            Unavailable::StaleStartupSample,
            "stale-startup-sample",
            "unavailable",
        ),
        (
            Unavailable::StaleSourceSample,
            "stale-source-sample",
            "unavailable",
        ),
        (
            Unavailable::UpsDisconnected,
            "ups-disconnected",
            "disconnected",
        ),
    ];

    for (reason, reason_text, connection) in cases {
        let file = render(&Telemetry::Unavailable(reason), low());

        assert!(file.contains("\nups.status:\n"), "{file}");
        assert_eq!(
            nut_value(&file, "experimental.ragtech.sample.valid").as_deref(),
            Some("0")
        );
        assert_eq!(
            nut_value(&file, "experimental.ragtech.bridge.reason").as_deref(),
            Some(reason_text)
        );
        assert_eq!(
            nut_value(&file, "experimental.ragtech.connection.status").as_deref(),
            Some(connection)
        );
        assert_eq!(
            nut_value(&file, "battery.charge.low").as_deref(),
            Some("20")
        );
        assert_eq!(
            alarm_line(&file),
            Some(format!("ALARM Ragtech telemetry unavailable: {reason_text}").as_str())
        );
    }
}

/// dummy-ups keeps variables between reads of the file, so every live value
/// has to be removed explicitly or NUT keeps serving it.
#[test]
fn unavailable_telemetry_removes_every_live_measurement() {
    let live = render(&Telemetry::Live(online_reading()), low());
    let unavailable = render(&Telemetry::Unavailable(Unavailable::QueryFailed), low());

    for line in live.lines().filter(|line| !line.starts_with("ALARM")) {
        let (key, _) = line.split_once(':').expect("state lines are name: value");
        assert!(
            nut_value(&unavailable, key).is_some(),
            "{key} is neither republished nor removed:\n{unavailable}"
        );
    }
    assert_eq!(
        nut_value(&unavailable, "battery.charge").as_deref(),
        Some("")
    );
    assert!(
        unavailable.contains("input.voltage: \"\"\n"),
        "{unavailable}"
    );
}

/// The shell exporter saw values of 1e15 and above in exponent form and
/// dropped them as non-numeric; dummy-ups would otherwise receive a
/// 300-digit value for its 256-byte buffer.
#[test]
fn implausibly_large_measurements_are_removed() {
    let mut reading = online_reading();
    reading.sample.input_voltage = Some(1e300);
    reading.sample.output_voltage = Some(-1e15);
    reading.sample.battery_voltage = Some(999_999_999_999_999.0);

    let file = render(&Telemetry::Live(reading), low());

    assert_eq!(nut_value(&file, "input.voltage").as_deref(), Some(""));
    assert_eq!(nut_value(&file, "output.voltage").as_deref(), Some(""));
    assert_eq!(
        nut_value(&file, "battery.voltage").as_deref(),
        Some("999999999999999.0")
    );
}
