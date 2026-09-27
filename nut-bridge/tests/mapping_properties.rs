//! Invariants of the state file for arbitrary Supervise samples.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "a failed fixture or assertion should panic the test"
)]

mod common;

use common::{nut_value, online_reading};
use proptest::prelude::*;
use ragtech_nut_bridge::nut::{ChargePercent, Telemetry, render};
use ragtech_nut_bridge::supervise::{Flags, Reading};

fn measurement() -> impl Strategy<Value = Option<f64>> {
    prop_oneof![
        Just(None),
        (-1.0e6..1.0e6f64).prop_map(Some),
        prop::num::f64::NORMAL.prop_map(Some),
        Just(Some(0.0)),
        Just(Some(100.0)),
    ]
}

fn flags() -> impl Strategy<Value = Flags> {
    prop::array::uniform9(any::<bool>()).prop_map(|bits| Flags {
        connected: bits[0],
        on_battery: bits[1],
        warning: bits[2],
        no_input_voltage: bits[3],
        low_battery: bits[4],
        high_output_power: bits[5],
        no_battery: bits[6],
        overload: bits[7],
        end_of_battery: bits[8],
    })
}

prop_compose! {
    fn reading()(
        flags in flags(),
        values in prop::collection::vec(measurement(), 13),
        device_id in any::<String>(),
        model in any::<Option<String>>(),
        firmware in any::<Option<String>>(),
        time in any::<i64>(),
        event in any::<i64>(),
    ) -> Reading {
        let mut reading = online_reading();
        let sample = &mut reading.sample;
        sample.flags = flags;
        sample.device_id = device_id;
        sample.time = time;
        sample.event = event;
        [
            sample.input_voltage,
            sample.output_voltage,
            sample.output_current,
            sample.output_power,
            sample.output_frequency,
            sample.battery_voltage,
            sample.battery_charge,
            sample.temperature,
            sample.nominal_input_voltage,
            sample.nominal_output_voltage,
            sample.nominal_output_power,
            sample.nominal_output_frequency,
            sample.nominal_battery_voltage,
        ] = values.try_into().expect("the strategy yields 13 measurements");
        reading.device.model = model;
        reading.device.firmware = firmware;
        reading
    }
}

fn is_percentage_or_empty(value: &str) -> bool {
    value.is_empty() || value.parse::<u8>().is_ok_and(|percent| percent <= 100)
}

proptest! {
    #[test]
    fn status_uses_only_known_tokens_and_never_forces_shutdown(
        reading in reading(),
        low in 0..=100u8,
    ) {
        let low = ChargePercent::new(low).expect("0..=100 is a valid percentage");
        let file = render(&Telemetry::Live(reading.clone()), low);
        let status = nut_value(&file, "ups.status").expect("live files carry a status");
        let tokens: Vec<&str> = status.split(' ').collect();

        prop_assert!(tokens.iter().all(|t| ["OL", "OB", "DISCHRG", "LB", "OVER", "RB"].contains(t)));
        prop_assert!(!tokens.contains(&"FSD"));
        prop_assert_eq!(tokens.iter().filter(|t| ["OL", "OB"].contains(t)).count(), 1);
    }

    #[test]
    fn database_values_cannot_add_or_break_lines(reading in reading()) {
        let low = ChargePercent::new(20).expect("20 is a valid percentage");
        let file = render(&Telemetry::Live(reading.clone()), low);
        let reference = render(&Telemetry::Live(online_reading()), low);

        let keys = |file: &str| -> Vec<String> {
            file.lines()
                .map(|line| match line.split_once(':') {
                    _ if line.starts_with("ALARM") => "ALARM".to_owned(),
                    Some((key, _)) => key.to_owned(),
                    None => line.to_owned(),
                })
                .collect()
        };
        prop_assert_eq!(keys(&file), keys(&reference));
        prop_assert!(file.lines().last().is_some_and(|line| line.starts_with("ALARM")));
        prop_assert!(file.chars().all(|ch| ch == '\n' || matches!(ch, ' '..='~')));
        for key in ["device.model", "device.serial", "ups.firmware"] {
            prop_assert!(nut_value(&file, key).is_some(), "parseconf rejects {}:\n{}", key, file);
        }
    }

    #[test]
    fn charge_and_load_are_percentages(reading in reading()) {
        let low = ChargePercent::new(20).expect("20 is a valid percentage");
        let file = render(&Telemetry::Live(reading.clone()), low);

        let charge = nut_value(&file, "battery.charge").expect("always published");
        let load = nut_value(&file, "ups.load").expect("always published");
        prop_assert!(is_percentage_or_empty(&charge), "battery.charge={}", charge);
        prop_assert!(is_percentage_or_empty(&load), "ups.load={}", load);
    }

    /// dummy-ups copies values into a 256-byte buffer, so a measurement must
    /// never render longer than a plausible number; implausible magnitudes
    /// are removed like non-numeric values.
    #[test]
    fn measurements_render_as_short_numbers(reading in reading()) {
        let low = ChargePercent::new(20).expect("20 is a valid percentage");
        let file = render(&Telemetry::Live(reading), low);

        for key in MEASUREMENT_KEYS {
            let value = nut_value(&file, key).expect("always published");
            prop_assert!(value.len() <= 24, "{}={}", key, value);
        }
    }
}

const MEASUREMENT_KEYS: [&str; 11] = [
    "ups.power.nominal",
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
];
