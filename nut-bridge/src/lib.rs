//! Bridge from Ragtech Supervise telemetry to Network UPS Tools (NUT).
//!
//! Ragtech's Supervise daemon polls the UPS and stores samples in a SQLite
//! database. This crate reads the latest sample, decides whether it is live
//! telemetry, and renders the state file that NUT's `dummy-ups` driver serves.
//!
//! The entry points are [`supervise::SuperviseDb`] to read the latest sample,
//! [`bridge::Bridge`] to decide whether it is live, and [`nut::render`] with
//! [`nut::write_state_file`] to publish it. The `ragtech-to-nut` binary runs
//! that loop with settings from [`config::Config`].
//!
//! # Examples
//!
//! One poll, without a database: an empty read is published as unavailable.
//!
//! ```
//! use std::time::Instant;
//!
//! use ragtech_nut_bridge::bridge::{Bridge, LiveHistory, Policy};
//! use ragtech_nut_bridge::nut::{ChargePercent, render};
//!
//! let policy = Policy {
//!     require_fresh_sample: false,
//!     max_sample_age: None,
//!     exit_on_invalid_after_live: false,
//! };
//! let mut bridge = Bridge::new(policy, LiveHistory::NotServed);
//! let low = ChargePercent::new(20).expect("20 is a percentage");
//!
//! let outcome = bridge.observe(Instant::now(), Ok(None));
//! let file = render(&outcome.telemetry, low);
//!
//! assert!(file.contains("experimental.ragtech.bridge.reason: no-current-sample\n"));
//! assert!(file.contains("ALARM Ragtech telemetry unavailable: no-current-sample\n"));
//! ```
//!
//! Reading a live database:
//!
//! ```no_run
//! use ragtech_nut_bridge::supervise::SuperviseDb;
//!
//! let db = SuperviseDb::new("/data/monit.db");
//! if let Some(reading) = db.latest_reading()? {
//!     println!("{:?} at {}", reading.sample.source, reading.sample.time);
//! }
//! # Ok::<(), ragtech_nut_bridge::supervise::ReadError>(())
//! ```

pub mod bridge;
pub mod config;
pub mod nut;
pub mod supervise;
