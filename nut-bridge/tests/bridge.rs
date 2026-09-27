//! Deciding whether a read is published as live telemetry.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "a failed fixture or assertion should panic the test"
)]

mod common;

use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::online_reading;
use ragtech_nut_bridge::bridge::{Bridge, LiveHistory, Policy};
use ragtech_nut_bridge::config::Config;
use ragtech_nut_bridge::nut::{Telemetry, Unavailable};
use ragtech_nut_bridge::supervise::ReadError;

fn lenient() -> Policy {
    Policy {
        require_fresh_sample: false,
        max_sample_age: None,
        exit_on_invalid_after_live: false,
    }
}

fn unreadable() -> ReadError {
    ReadError::Unreadable {
        path: PathBuf::from("/data/monit.db"),
        source: io::Error::from(io::ErrorKind::NotFound),
    }
}

fn query_failed() -> ReadError {
    ReadError::Query {
        path: PathBuf::from("/data/monit.db"),
        source: rusqlite::Error::InvalidQuery,
    }
}

#[test]
fn read_failures_and_missing_samples_are_unavailable_telemetry() {
    let now = Instant::now();
    let mut bridge = Bridge::new(lenient(), LiveHistory::NotServed);

    assert_eq!(
        bridge.observe(now, Err(unreadable())).telemetry,
        Telemetry::Unavailable(Unavailable::DatabaseUnreadable)
    );
    assert_eq!(
        bridge.observe(now, Err(query_failed())).telemetry,
        Telemetry::Unavailable(Unavailable::QueryFailed)
    );
    assert_eq!(
        bridge.observe(now, Ok(None)).telemetry,
        Telemetry::Unavailable(Unavailable::NoCurrentSample)
    );
}

#[test]
fn connected_sample_is_live_telemetry() {
    let mut bridge = Bridge::new(lenient(), LiveHistory::NotServed);

    let outcome = bridge.observe(Instant::now(), Ok(Some(online_reading())));

    assert_eq!(outcome.telemetry, Telemetry::Live(online_reading()));
}

#[test]
fn sample_from_a_disconnected_ups_is_unavailable() {
    let mut reading = online_reading();
    reading.sample.flags.connected = false;
    let mut bridge = Bridge::new(lenient(), LiveHistory::NotServed);

    let outcome = bridge.observe(Instant::now(), Ok(Some(reading)));

    assert_eq!(
        outcome.telemetry,
        Telemetry::Unavailable(Unavailable::UpsDisconnected)
    );
}

/// An old `OB LB` row must not be replayed as live just because the bridge
/// restarted: the first sample seen is a baseline until Supervise writes anew.
#[test]
fn fresh_sample_mode_rejects_the_startup_sample_until_a_new_one_arrives() {
    let policy = Policy {
        require_fresh_sample: true,
        ..lenient()
    };
    let mut bridge = Bridge::new(policy, LiveHistory::NotServed);
    let now = Instant::now();
    let mut newer = online_reading();
    newer.sample.time = 1001;
    newer.sample.event = 2;

    let first = bridge.observe(now, Ok(Some(online_reading())));
    let unchanged = bridge.observe(now + Duration::from_secs(2), Ok(Some(online_reading())));
    let changed = bridge.observe(now + Duration::from_secs(4), Ok(Some(newer.clone())));

    let stale = Telemetry::Unavailable(Unavailable::StaleStartupSample);
    assert_eq!(first.telemetry, stale);
    assert_eq!(unchanged.telemetry, stale);
    assert_eq!(changed.telemetry, Telemetry::Live(newer));
}

#[test]
fn fresh_sample_mode_takes_the_first_sample_after_read_failures_as_the_baseline() {
    let policy = Policy {
        require_fresh_sample: true,
        ..lenient()
    };
    let mut bridge = Bridge::new(policy, LiveHistory::NotServed);
    let now = Instant::now();

    bridge.observe(now, Err(unreadable()));
    bridge.observe(now, Ok(None));
    let first_sample = bridge.observe(now, Ok(Some(online_reading())));

    assert_eq!(
        first_sample.telemetry,
        Telemetry::Unavailable(Unavailable::StaleStartupSample)
    );
}

#[test]
fn unchanged_sample_goes_stale_after_the_maximum_age_and_a_new_one_revives_it() {
    let policy = Policy {
        max_sample_age: Some(Duration::from_secs(30)),
        ..lenient()
    };
    let mut bridge = Bridge::new(policy, LiveHistory::NotServed);
    let start = Instant::now();
    let at = |secs| start + Duration::from_secs(secs);
    let mut newer = online_reading();
    newer.sample.time = 1030;

    let first = bridge.observe(at(0), Ok(Some(online_reading())));
    let at_limit = bridge.observe(at(30), Ok(Some(online_reading())));
    let past_limit = bridge.observe(at(31), Ok(Some(online_reading())));
    let new_sample = bridge.observe(at(32), Ok(Some(newer.clone())));
    let new_sample_later = bridge.observe(at(62), Ok(Some(newer.clone())));

    assert_eq!(first.telemetry, Telemetry::Live(online_reading()));
    assert_eq!(at_limit.telemetry, Telemetry::Live(online_reading()));
    assert_eq!(
        past_limit.telemetry,
        Telemetry::Unavailable(Unavailable::StaleSourceSample)
    );
    assert_eq!(new_sample.telemetry, Telemetry::Live(newer.clone()));
    assert_eq!(new_sample_later.telemetry, Telemetry::Live(newer));
}

#[test]
fn without_a_maximum_age_an_unchanged_sample_stays_live() {
    let mut bridge = Bridge::new(lenient(), LiveHistory::NotServed);
    let start = Instant::now();

    bridge.observe(start, Ok(Some(online_reading())));
    let much_later = bridge.observe(
        start + Duration::from_secs(86_400),
        Ok(Some(online_reading())),
    );

    assert_eq!(much_later.telemetry, Telemetry::Live(online_reading()));
}

#[test]
fn exit_policy_stops_the_bridge_only_after_live_telemetry_was_served() {
    let policy = Policy {
        exit_on_invalid_after_live: true,
        ..lenient()
    };
    let mut bridge = Bridge::new(policy, LiveHistory::NotServed);
    let now = Instant::now();

    let before_live = bridge.observe(now, Ok(None));
    let live = bridge.observe(now, Ok(Some(online_reading())));
    let after_live = bridge.observe(now, Err(query_failed()));

    assert!(!before_live.must_exit);
    assert!(!live.must_exit);
    assert!(after_live.must_exit);
    assert_eq!(
        after_live.telemetry,
        Telemetry::Unavailable(Unavailable::QueryFailed)
    );
}

#[test]
fn exit_policy_applies_immediately_when_live_telemetry_was_served_before_startup() {
    let policy = Policy {
        exit_on_invalid_after_live: true,
        ..lenient()
    };
    let mut bridge = Bridge::new(policy, LiveHistory::AlreadyServed);

    let outcome = bridge.observe(Instant::now(), Err(unreadable()));

    assert!(outcome.must_exit);
}

#[test]
fn without_the_exit_policy_the_bridge_keeps_publishing_unavailable_telemetry() {
    let mut bridge = Bridge::new(lenient(), LiveHistory::AlreadyServed);
    let now = Instant::now();

    bridge.observe(now, Ok(Some(online_reading())));
    let outcome = bridge.observe(now, Err(unreadable()));

    assert!(!outcome.must_exit);
}

/// Supervise 8.9 commits samples in batches roughly every 40 s (observed on a
/// live family-10 unit), so with the default configuration a sample that has
/// not changed for a minute is still live telemetry.
#[test]
fn default_configuration_tolerates_supervise_batched_commits() {
    let config = Config::from_lookup(|_| None).expect("the defaults are valid");
    let policy = Policy {
        require_fresh_sample: false,
        ..config.policy()
    };
    let mut bridge = Bridge::new(policy, LiveHistory::NotServed);
    let start = Instant::now();

    bridge.observe(start, Ok(Some(online_reading())));
    let a_minute_later =
        bridge.observe(start + Duration::from_secs(60), Ok(Some(online_reading())));

    assert_eq!(a_minute_later.telemetry, Telemetry::Live(online_reading()));
}

/// Freshness follows the sample row only, as the shell exporter's sample
/// token did: Supervise updating `DEVICELIST` metadata is not a new sample.
#[test]
fn device_metadata_changes_do_not_make_a_sample_fresh() {
    let policy = Policy {
        require_fresh_sample: true,
        max_sample_age: Some(Duration::from_secs(30)),
        ..lenient()
    };
    let mut bridge = Bridge::new(policy, LiveHistory::NotServed);
    let start = Instant::now();
    let mut new_firmware = online_reading();
    new_firmware.device.firmware = Some("8.9".to_owned());
    let mut newer = online_reading();
    newer.sample.time = 1001;
    let mut newer_renamed = newer.clone();
    newer_renamed.device.model = Some("Renamed UPS".to_owned());

    bridge.observe(start, Ok(Some(online_reading())));
    let metadata_only = bridge.observe(start, Ok(Some(new_firmware)));
    bridge.observe(start, Ok(Some(newer)));
    let renamed_later = bridge.observe(start + Duration::from_secs(31), Ok(Some(newer_renamed)));

    assert_eq!(
        metadata_only.telemetry,
        Telemetry::Unavailable(Unavailable::StaleStartupSample)
    );
    assert_eq!(
        renamed_later.telemetry,
        Telemetry::Unavailable(Unavailable::StaleSourceSample)
    );
}
