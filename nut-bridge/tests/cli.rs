//! The `ragtech-to-nut` executable: configuration, modes and exit codes.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "a failed fixture or assertion should panic the test"
)]

mod common;

use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::time::{Duration, Instant};

use common::db::{Fixture, Row};
use common::nut_value;

/// Runs the exporter with only the given environment.
struct Exporter {
    command: Command,
    dev_path: PathBuf,
}

impl Exporter {
    fn new(fixture: &Fixture) -> Self {
        let dev_path = fixture.dir().join("ragtech.dev");
        let mut command = Command::new(env!("CARGO_BIN_EXE_ragtech-to-nut"));
        command
            .env_clear()
            .env("DB_PATH", fixture.path())
            .env("DEV_PATH", &dev_path)
            .env("REQUIRE_FRESH_SAMPLE", "0");
        Self { command, dev_path }
    }

    fn env(mut self, key: &str, value: &str) -> Self {
        self.command.env(key, value);
        self
    }

    fn spawn(mut self, args: &[&str]) -> Running {
        let child = self
            .command
            .args(args)
            .stderr(Stdio::piped())
            .spawn()
            .expect("start the exporter");
        Running {
            child: Some(child),
            dev_path: self.dev_path,
        }
    }

    fn once(mut self) -> (Output, String) {
        let output = self
            .command
            .arg("--once")
            .output()
            .expect("run the exporter");
        let file = std::fs::read_to_string(&self.dev_path).unwrap_or_default();
        (output, file)
    }
}

/// A running exporter, killed when dropped.
struct Running {
    child: Option<Child>,
    dev_path: PathBuf,
}

impl Running {
    /// Waits up to 10 s for the state file to contain `needle`.
    fn wait_for(&self, needle: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let file = std::fs::read_to_string(&self.dev_path).unwrap_or_default();
            if file.contains(needle) {
                return file;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {needle:?} in:\n{file}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Waits up to 10 s for the exporter to exit.
    fn wait_for_exit(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child().try_wait().expect("poll the exporter") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for the exporter to exit"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Stops the exporter and returns what it wrote to stderr.
    fn stop(mut self) -> Output {
        let mut child = self.child.take().expect("the exporter is still owned");
        let _ = child.kill();
        child
            .wait_with_output()
            .expect("collect the exporter output")
    }

    fn is_running(&mut self) -> bool {
        self.child()
            .try_wait()
            .expect("poll the exporter")
            .is_none()
    }

    fn child(&mut self) -> &mut Child {
        self.child.as_mut().expect("the exporter is still owned")
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn once_mode_publishes_the_latest_sample_and_exits() {
    let fixture = Fixture::with_device();
    fixture.insert("EVENTLOG", &Row::default());

    let (output, file) = Exporter::new(&fixture).once();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(nut_value(&file, "ups.status").as_deref(), Some("OL"));
    assert_eq!(nut_value(&file, "battery.charge").as_deref(), Some("88"));
    assert_eq!(
        nut_value(&file, "experimental.ragtech.sample.valid").as_deref(),
        Some("1")
    );
}

#[test]
fn invalid_configuration_is_rejected_before_anything_is_published() {
    let cases = [
        (
            "MAX_SAMPLE_AGE",
            "abc",
            "MAX_SAMPLE_AGE must be a non-negative integer",
        ),
        (
            "MAX_SAMPLE_AGE",
            "-1",
            "MAX_SAMPLE_AGE must be a non-negative integer",
        ),
        (
            "POLL_INTERVAL",
            "0",
            "POLL_INTERVAL must be a positive number",
        ),
        (
            "POLL_INTERVAL",
            "1e3",
            "POLL_INTERVAL must be a positive number",
        ),
        (
            "BATTERY_CHARGE_LOW",
            "abc",
            "BATTERY_CHARGE_LOW must be an integer from 0 to 100",
        ),
        (
            "BATTERY_CHARGE_LOW",
            "101",
            "BATTERY_CHARGE_LOW must be an integer from 0 to 100",
        ),
        (
            "REQUIRE_FRESH_SAMPLE",
            "maybe",
            "REQUIRE_FRESH_SAMPLE must be 0 or 1",
        ),
        (
            "EXIT_ON_INVALID_AFTER_LIVE",
            "yes",
            "EXIT_ON_INVALID_AFTER_LIVE must be 0 or 1",
        ),
        (
            "RAGTECH_NUT_INITIAL_LIVE_SAMPLE_SEEN",
            "maybe",
            "RAGTECH_NUT_INITIAL_LIVE_SAMPLE_SEEN must be 0 or 1",
        ),
    ];

    for (key, value, message) in cases {
        let fixture = Fixture::with_device();
        fixture.insert("EVENTLOG", &Row::default());

        let (output, file) = Exporter::new(&fixture).env(key, value).once();

        assert_eq!(output.status.code(), Some(1), "{key}={value}");
        assert!(
            stderr(&output).contains(message),
            "{key}={value}: {}",
            stderr(&output)
        );
        assert!(file.is_empty(), "{key}={value} published:\n{file}");
    }
}

#[test]
fn valid_configuration_values_are_accepted() {
    let fixture = Fixture::with_device();
    fixture.insert("EVENTLOG", &Row::default());

    let (output, file) = Exporter::new(&fixture)
        .env("MAX_SAMPLE_AGE", "0")
        .env("POLL_INTERVAL", "0.5")
        .env("BATTERY_CHARGE_LOW", "90")
        .env("EXIT_ON_INVALID_AFTER_LIVE", "1")
        .once();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(nut_value(&file, "ups.status").as_deref(), Some("OL LB"));
    assert_eq!(
        nut_value(&file, "battery.charge.low").as_deref(),
        Some("90")
    );
}

#[test]
fn invalid_telemetry_after_live_exits_with_status_75_after_publishing_it() {
    let fixture = Fixture::empty();

    let (output, file) = Exporter::new(&fixture)
        .env("EXIT_ON_INVALID_AFTER_LIVE", "1")
        .env("RAGTECH_NUT_INITIAL_LIVE_SAMPLE_SEEN", "1")
        .once();

    assert_eq!(output.status.code(), Some(75), "{}", stderr(&output));
    assert_eq!(
        nut_value(&file, "experimental.ragtech.sample.valid").as_deref(),
        Some("0")
    );
    assert_eq!(
        nut_value(&file, "experimental.ragtech.bridge.reason").as_deref(),
        Some("database-unreadable")
    );
}

#[test]
fn missing_database_is_published_as_unavailable_without_failing() {
    let fixture = Fixture::empty();

    let (output, file) = Exporter::new(&fixture).once();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        nut_value(&file, "experimental.ragtech.bridge.reason").as_deref(),
        Some("database-unreadable")
    );
    assert_eq!(
        common::alarm_line(&file),
        Some("ALARM Ragtech telemetry unavailable: database-unreadable")
    );
    assert!(
        !fixture.path().exists(),
        "the exporter must not create the database"
    );
}

#[test]
fn loop_mode_keeps_publishing_and_exits_75_when_the_live_sample_goes_stale() {
    let fixture = Fixture::with_device();
    fixture.insert("EVENTLOG", &Row::default());

    let mut exporter = Exporter::new(&fixture)
        .env("MAX_SAMPLE_AGE", "1")
        .env("POLL_INTERVAL", "0.2")
        .env("EXIT_ON_INVALID_AFTER_LIVE", "1")
        .spawn(&[]);

    exporter.wait_for("experimental.ragtech.sample.valid: 1");
    exporter.wait_for("experimental.ragtech.bridge.reason: stale-source-sample");
    assert_eq!(exporter.wait_for_exit().code(), Some(75));
}

#[test]
fn wait_for_valid_mode_exits_only_after_a_fresh_sample_is_accepted() {
    let fixture = Fixture::with_device();
    fixture.insert("EVENTLOG", &Row::default());

    let mut exporter = Exporter::new(&fixture)
        .env("REQUIRE_FRESH_SAMPLE", "1")
        .env("MAX_SAMPLE_AGE", "0")
        .env("POLL_INTERVAL", "0.1")
        .spawn(&["--wait-for-valid"]);

    exporter.wait_for("experimental.ragtech.bridge.reason: stale-startup-sample");
    assert!(exporter.is_running());
    fixture.insert("EVENTLOG", &Row::at(1001, 2));

    assert!(exporter.wait_for_exit().success());
    let file = exporter.wait_for("experimental.ragtech.sample.valid: 1");
    assert_eq!(
        nut_value(&file, "experimental.ragtech.sample.time").as_deref(),
        Some("1001")
    );
}

#[test]
fn state_file_is_world_readable_and_replaced_without_leftovers() {
    use std::os::unix::fs::PermissionsExt as _;

    let fixture = Fixture::with_device();
    fixture.insert("EVENTLOG", &Row::default());

    let (output, _) = Exporter::new(&fixture).once();
    let (again, _) = Exporter::new(&fixture).once();

    assert!(output.status.success() && again.status.success());
    let dev_path = fixture.dir().join("ragtech.dev");
    let mode = std::fs::metadata(&dev_path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o644);
    let mut names: Vec<String> = std::fs::read_dir(fixture.dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["monit.db", "ragtech.dev"]);
}

#[test]
fn unknown_arguments_are_a_usage_error() {
    let fixture = Fixture::with_device();

    for args in [&["--bogus"][..], &["--once", "--once"]] {
        let output = Exporter::new(&fixture).command.args(args).output().unwrap();

        assert_eq!(output.status.code(), Some(64), "{args:?}");
        assert!(
            stderr(&output).contains("usage: ragtech-to-nut"),
            "{args:?}"
        );
    }
}

#[test]
fn read_failures_are_logged_with_their_cause() {
    let fixture = Fixture::empty();
    fixture
        .connect()
        .execute_batch("CREATE TABLE unrelated (id INTEGER);")
        .unwrap();

    let (output, file) = Exporter::new(&fixture).once();

    assert!(output.status.success());
    assert_eq!(
        nut_value(&file, "experimental.ragtech.bridge.reason").as_deref(),
        Some("query-failed")
    );
    let log = stderr(&output);
    assert!(
        log.contains("[ragtech-to-nut] telemetry unavailable (query-failed)"),
        "{log}"
    );
    assert!(log.contains("no such table"), "{log}");
}

#[test]
fn state_changes_are_logged_once_rather_than_on_every_poll() {
    let fixture = Fixture::empty();

    let exporter = Exporter::new(&fixture)
        .env("POLL_INTERVAL", "0.05")
        .spawn(&[]);
    exporter.wait_for("database-unreadable");
    std::thread::sleep(Duration::from_millis(500));
    let output = exporter.stop();

    let log = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        log.matches("telemetry unavailable (database-unreadable)")
            .count(),
        1,
        "{log}"
    );
}

/// Like the shell exporter's `${VAR:-default}`, a variable set to the empty
/// string takes its default, so `KEY=${KEY:-}` in a compose file keeps working.
#[test]
fn empty_variables_take_their_defaults() {
    let fixture = Fixture::with_device();
    fixture.insert("EVENTLOG", &Row::default());

    let mut exporter = Exporter::new(&fixture);
    for key in [
        "POLL_INTERVAL",
        "MAX_SAMPLE_AGE",
        "BATTERY_CHARGE_LOW",
        "EXIT_ON_INVALID_AFTER_LIVE",
        "RAGTECH_NUT_INITIAL_LIVE_SAMPLE_SEEN",
    ] {
        exporter = exporter.env(key, "");
    }
    let (output, file) = exporter.once();

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        nut_value(&file, "battery.charge.low").as_deref(),
        Some("20")
    );
    assert_eq!(
        nut_value(&file, "experimental.ragtech.sample.valid").as_deref(),
        Some("1")
    );
}
