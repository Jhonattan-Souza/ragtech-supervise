//! Reading the latest sample from a Supervise database.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "a failed fixture or assertion should panic the test"
)]

mod common;

use common::db::{Fixture, Row};
use common::online_reading;
use ragtech_nut_bridge::supervise::{ReadError, SampleSource, SuperviseDb};
use rusqlite::types::Value;

#[test]
fn latest_event_log_row_is_read_with_its_device_metadata() {
    let fixture = Fixture::with_device();
    fixture.insert("EVENTLOG", &Row::default());

    let reading = SuperviseDb::new(fixture.path())
        .latest_reading()
        .expect("readable database");

    assert_eq!(reading, Some(online_reading()));
}

#[test]
fn missing_database_is_unreadable_and_is_not_created() {
    let fixture = Fixture::empty();

    let result = SuperviseDb::new(fixture.path()).latest_reading();

    assert!(
        matches!(result, Err(ReadError::Unreadable { .. })),
        "{result:?}"
    );
    assert!(!fixture.path().exists());
}

#[test]
fn database_without_supervise_tables_is_a_query_failure() {
    let fixture = Fixture::empty();
    fixture
        .connect()
        .execute_batch("CREATE TABLE unrelated (id INTEGER);")
        .unwrap();

    let result = SuperviseDb::new(fixture.path()).latest_reading();

    assert!(matches!(result, Err(ReadError::Query { .. })), "{result:?}");
}

#[test]
fn file_that_is_not_a_database_is_a_query_failure() {
    let fixture = Fixture::empty();
    std::fs::write(fixture.path(), "not a database, just text\n".repeat(200)).unwrap();

    let result = SuperviseDb::new(fixture.path()).latest_reading();

    assert!(matches!(result, Err(ReadError::Query { .. })), "{result:?}");
}

#[test]
fn device_without_samples_has_no_reading() {
    let fixture = Fixture::with_device();

    let reading = SuperviseDb::new(fixture.path()).latest_reading().unwrap();

    assert_eq!(reading, None);
}

#[test]
fn newest_sample_wins_across_event_log_and_hourly_history() {
    let fixture = Fixture::with_device();
    fixture.insert("HISTLOGHOUR", &Row::at(900, 1));
    let db = SuperviseDb::new(fixture.path());

    let history_only = db.latest_reading().unwrap().unwrap().sample;
    fixture.insert("EVENTLOG", &Row::at(1200, 2));
    let newer_event = db.latest_reading().unwrap().unwrap().sample;
    fixture.insert("HISTLOGHOUR", &Row::at(1300, -1));
    let newer_history = db.latest_reading().unwrap().unwrap().sample;

    assert_eq!(
        (history_only.source, history_only.time),
        (SampleSource::HistLogHour, 900)
    );
    assert_eq!(
        (newer_event.source, newer_event.time),
        (SampleSource::EventLog, 1200)
    );
    assert_eq!(
        (newer_history.source, newer_history.time),
        (SampleSource::HistLogHour, 1300)
    );
}

#[test]
fn samples_come_from_the_most_recently_seen_device() {
    let fixture = Fixture::with_device();
    fixture.insert_device("ups-2", 2000, Some("Second UPS"), None);
    fixture.insert("EVENTLOG", &Row::at(5000, 1));
    fixture.insert(
        "EVENTLOG",
        &Row {
            id: Value::Text("ups-2".to_owned()),
            ..Row::at(1500, 1)
        },
    );

    let reading = SuperviseDb::new(fixture.path())
        .latest_reading()
        .unwrap()
        .unwrap();

    assert_eq!(reading.sample.device_id, "ups-2");
    assert_eq!(reading.sample.time, 1500);
    assert_eq!(reading.device.model.as_deref(), Some("Second UPS"));
    assert_eq!(reading.device.firmware, None);
}

#[test]
fn only_numeric_values_are_measurements() {
    let fixture = Fixture::with_device();
    fixture.insert(
        "EVENTLOG",
        &Row {
            input_voltage: Value::Null,
            output_voltage: Value::Text("127.5".to_owned()),
            output_current: Value::Text("12 V".to_owned()),
            output_power: Value::Text("abc".to_owned()),
            battery_charge: Value::Integer(97),
            temperature: Value::Text("-3".to_owned()),
            nominal_battery_voltage: Value::Text("inf".to_owned()),
            ..Row::default()
        },
    );

    let sample = SuperviseDb::new(fixture.path())
        .latest_reading()
        .unwrap()
        .unwrap()
        .sample;

    assert_eq!(sample.input_voltage, None);
    assert_eq!(sample.output_voltage, Some(127.5));
    assert_eq!(sample.output_current, None);
    assert_eq!(sample.output_power, None);
    assert_eq!(sample.battery_charge, Some(97.0));
    assert_eq!(sample.temperature, Some(-3.0));
    assert_eq!(sample.nominal_battery_voltage, None);
}

#[test]
fn flags_are_set_only_by_the_value_one() {
    let fixture = Fixture::with_device();
    fixture.insert(
        "EVENTLOG",
        &Row {
            connected: Value::Text("1".to_owned()),
            on_battery: Value::Null,
            warning: Value::Integer(2),
            low_battery: Value::Real(1.0),
            ..Row::default()
        },
    );

    let flags = SuperviseDb::new(fixture.path())
        .latest_reading()
        .unwrap()
        .unwrap()
        .sample
        .flags;

    assert!(flags.connected);
    assert!(!flags.on_battery);
    assert!(!flags.warning);
    assert!(flags.low_battery);
}

#[test]
fn text_in_integer_columns_is_read_as_its_leading_integer() {
    let fixture = Fixture::with_device();
    fixture.insert(
        "EVENTLOG",
        &Row {
            dt: Value::Text("1000\nups.status: OB".to_owned()),
            event: Value::Text("8\nALARM [bad]".to_owned()),
            ..Row::default()
        },
    );

    let sample = SuperviseDb::new(fixture.path())
        .latest_reading()
        .unwrap()
        .unwrap()
        .sample;

    assert_eq!((sample.time, sample.event), (1000, 8));
}

/// Supervise keeps writing while the bridge reads; in WAL mode a reader sees
/// the last committed sample instead of waiting for the writer.
#[test]
fn reads_do_not_wait_for_an_open_write_transaction_in_wal_mode() {
    let fixture = Fixture::with_device();
    fixture
        .connect()
        .pragma_update(None, "journal_mode", "WAL")
        .unwrap();
    fixture.insert("HISTLOGHOUR", &Row::at(1000, -1));
    let writer = fixture.connect();
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();
    writer
        .execute(
            "UPDATE HISTLOGHOUR SET var_cBattery = 5 WHERE dt = 1000",
            [],
        )
        .unwrap();

    let started = std::time::Instant::now();
    let reading = SuperviseDb::new(fixture.path())
        .latest_reading()
        .unwrap()
        .unwrap();

    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    assert_eq!(reading.sample.battery_charge, Some(88.0));
    writer.execute_batch("ROLLBACK").unwrap();
}

/// A lock held past the 2 s busy timeout (a long Supervise write in rollback
/// journal mode) fails one attempt; the read is retried once before failing.
#[test]
fn a_lock_held_past_the_busy_timeout_is_retried_once() {
    let fixture = Fixture::with_device();
    fixture.insert("EVENTLOG", &Row::default());
    let writer = fixture.connect();
    writer.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(3));
        writer.execute_batch("COMMIT").unwrap();
    });

    let result = SuperviseDb::new(fixture.path()).latest_reading();

    release.join().unwrap();
    assert!(result.is_ok_and(|reading| reading.is_some()));
}
