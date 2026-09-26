//! `ragtech-to-nut`: exports Ragtech Supervise telemetry to a NUT `dummy-ups` state file.
//!
//! Usage: `ragtech-to-nut [--once | --wait-for-valid]`
//!
//! Without arguments it publishes the latest sample every `POLL_INTERVAL`
//! seconds until stopped. `--once` publishes once and exits. `--wait-for-valid`
//! publishes until a sample is accepted as live telemetry, then exits 0.
//! Configuration comes from environment variables; see [`Config`].
//!
//! The process is synchronous: one blocking read, one file write and one sleep
//! per poll, with nothing to run concurrently. It is not PID 1 in the
//! container, so the default SIGTERM disposition is the shutdown path; the
//! state file is replaced atomically, so termination never leaves it partial.

use std::process::ExitCode;
use std::time::Instant;

use anyhow::Context as _;
use ragtech_nut_bridge::bridge::Bridge;
use ragtech_nut_bridge::config::Config;
use ragtech_nut_bridge::nut::{Telemetry, render, write_state_file};
use ragtech_nut_bridge::supervise::SuperviseDb;

/// Exit status when telemetry becomes invalid after live telemetry was served
/// (`EX_TEMPFAIL`); the container entrypoint stops serving the UPS on it.
const EXIT_TELEMETRY_LOST: u8 = 75;

/// Exit status for command-line usage errors (`EX_USAGE`).
const EXIT_USAGE: u8 = 64;

/// When the exporter stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Publish until stopped.
    Loop,
    /// Publish once.
    Once,
    /// Publish until live telemetry is accepted.
    WaitForValid,
}

fn main() -> ExitCode {
    let Some(mode) = parse_mode(std::env::args().skip(1)) else {
        eprintln!("usage: ragtech-to-nut [--once | --wait-for-valid]");
        return ExitCode::from(EXIT_USAGE);
    };
    match run(mode) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("[ragtech-to-nut] {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn parse_mode(mut args: impl Iterator<Item = String>) -> Option<Mode> {
    let mode = match args.next().as_deref() {
        None => Mode::Loop,
        Some("--once") => Mode::Once,
        Some("--wait-for-valid") => Mode::WaitForValid,
        Some(_) => return None,
    };
    args.next().is_none().then_some(mode)
}

fn run(mode: Mode) -> anyhow::Result<ExitCode> {
    let config = Config::from_lookup(|key| std::env::var(key).ok())?;
    let db = SuperviseDb::new(config.db_path());
    let mut bridge = Bridge::new(config.policy(), config.live_history());

    let mut last_logged = String::new();

    loop {
        let read = db.latest_reading();
        let cause = read.as_ref().err().map(|error| error_chain(error));
        let outcome = bridge.observe(Instant::now(), read);
        let state = match (&outcome.telemetry, cause) {
            (Telemetry::Live(_), _) => "publishing live telemetry".to_owned(),
            (Telemetry::Unavailable(reason), None) => format!("telemetry unavailable ({reason})"),
            (Telemetry::Unavailable(reason), Some(cause)) => {
                format!("telemetry unavailable ({reason}): {cause}")
            }
        };
        if state != last_logged {
            eprintln!("[ragtech-to-nut] {state}");
            last_logged = state;
        }

        let contents = render(&outcome.telemetry, config.battery_charge_low());
        write_state_file(config.dev_path(), &contents)
            .with_context(|| format!("failed to write {}", config.dev_path().display()))?;

        let live = matches!(outcome.telemetry, Telemetry::Live(_));
        if outcome.must_exit {
            return Ok(ExitCode::from(EXIT_TELEMETRY_LOST));
        }
        if mode == Mode::Once || (mode == Mode::WaitForValid && live) {
            return Ok(ExitCode::SUCCESS);
        }
        std::thread::sleep(config.poll_interval());
    }
}

/// Formats an error and its sources as `error: source: source`.
fn error_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut chain = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        chain.push_str(": ");
        chain.push_str(&cause.to_string());
        source = cause.source();
    }
    chain
}
