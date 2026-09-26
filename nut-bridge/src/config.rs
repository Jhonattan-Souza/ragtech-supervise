//! Configuration from environment variables.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::bridge::{LiveHistory, Policy};
use crate::nut::ChargePercent;

/// Validated exporter settings; see [`Config::from_lookup`] for the variables.
///
/// The fields are private so every `Config` has passed validation (for
/// example, the poll interval is always positive).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    db_path: PathBuf,
    dev_path: PathBuf,
    poll_interval: Duration,
    battery_charge_low: ChargePercent,
    policy: Policy,
    live_history: LiveHistory,
}

/// Default `MAX_SAMPLE_AGE`, in seconds. Supervise 8.9 commits samples in
/// batches about every 40 s, so the newest sample normally stays unchanged
/// that long; three batches without a new sample means Supervise stopped.
const DEFAULT_MAX_SAMPLE_AGE: &str = "120";

/// An invalid configuration value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// `MAX_SAMPLE_AGE` is not a non-negative integer.
    #[error("MAX_SAMPLE_AGE must be a non-negative integer")]
    MaxSampleAge,
    /// `POLL_INTERVAL` is not a positive decimal number.
    #[error("POLL_INTERVAL must be a positive number")]
    PollInterval,
    /// `BATTERY_CHARGE_LOW` is not an integer from 0 to 100.
    #[error("BATTERY_CHARGE_LOW must be an integer from 0 to 100")]
    BatteryChargeLow,
    /// A 0/1 switch holds another value.
    #[error("{0} must be 0 or 1")]
    Switch(&'static str),
}

impl Config {
    /// Reads and validates the configuration through `lookup`, typically
    /// `|key| std::env::var(key).ok()`. Unset or empty variables take their
    /// defaults:
    ///
    /// | Variable | Meaning | Default |
    /// | --- | --- | --- |
    /// | `DB_PATH` | Supervise database | `/data/monit.db` |
    /// | `DEV_PATH` | `dummy-ups` state file | `/run/nut/ragtech.dev` |
    /// | `POLL_INTERVAL` | seconds between reads, positive decimal | `2` |
    /// | `BATTERY_CHARGE_LOW` | low-battery charge, 0 to 100 | `20` |
    /// | `REQUIRE_FRESH_SAMPLE` | startup sample is stale, 0 or 1 | `1` |
    /// | `MAX_SAMPLE_AGE` | seconds a sample may stay unchanged, 0 disables | `120` |
    /// | `EXIT_ON_INVALID_AFTER_LIVE` | exit 75 when live telemetry is lost, 0 or 1 | `0` |
    /// | `RAGTECH_NUT_INITIAL_LIVE_SAMPLE_SEEN` | live telemetry already served, 0 or 1 | `0` |
    ///
    /// # Errors
    ///
    /// A [`ConfigError`] naming the first invalid variable.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use std::time::Duration;
    ///
    /// use ragtech_nut_bridge::config::{Config, ConfigError};
    ///
    /// let env = HashMap::from([("POLL_INTERVAL", "0.5"), ("MAX_SAMPLE_AGE", "0")]);
    /// let config = Config::from_lookup(|key| env.get(key).map(|v| v.to_string()))?;
    /// assert_eq!(config.poll_interval(), Duration::from_millis(500));
    /// assert_eq!(config.policy().max_sample_age, None);
    ///
    /// let invalid = Config::from_lookup(|key| (key == "BATTERY_CHARGE_LOW").then(|| "101".into()));
    /// assert_eq!(invalid, Err(ConfigError::BatteryChargeLow));
    /// # Ok::<(), ConfigError>(())
    /// ```
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        // Like `${VAR:-default}`: set-but-empty takes the default too.
        let var = |key: &str, default: &str| {
            lookup(key)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| default.to_owned())
        };
        let switch = |key: &'static str, default: &str| match var(key, default).as_str() {
            "0" => Ok(false),
            "1" => Ok(true),
            _ => Err(ConfigError::Switch(key)),
        };

        let max_sample_age = parse_integer(&var("MAX_SAMPLE_AGE", DEFAULT_MAX_SAMPLE_AGE))
            .ok_or(ConfigError::MaxSampleAge)?;
        let poll_interval = parse_decimal(&var("POLL_INTERVAL", "2"))
            .filter(|seconds| *seconds > 0.0)
            .and_then(|seconds| Duration::try_from_secs_f64(seconds).ok())
            .ok_or(ConfigError::PollInterval)?;
        let battery_charge_low = parse_integer(&var("BATTERY_CHARGE_LOW", "20"))
            .and_then(|percent| u8::try_from(percent).ok())
            .and_then(ChargePercent::new)
            .ok_or(ConfigError::BatteryChargeLow)?;
        let require_fresh_sample = switch("REQUIRE_FRESH_SAMPLE", "1")?;
        let exit_on_invalid_after_live = switch("EXIT_ON_INVALID_AFTER_LIVE", "0")?;
        let initial_live_sample_seen = switch("RAGTECH_NUT_INITIAL_LIVE_SAMPLE_SEEN", "0")?;

        Ok(Self {
            db_path: var("DB_PATH", "/data/monit.db").into(),
            dev_path: var("DEV_PATH", "/run/nut/ragtech.dev").into(),
            poll_interval,
            battery_charge_low,
            policy: Policy {
                require_fresh_sample,
                max_sample_age: (max_sample_age > 0).then(|| Duration::from_secs(max_sample_age)),
                exit_on_invalid_after_live,
            },
            live_history: if initial_live_sample_seen {
                LiveHistory::AlreadyServed
            } else {
                LiveHistory::NotServed
            },
        })
    }

    /// The Supervise database path.
    #[must_use]
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// The `dummy-ups` state file path.
    #[must_use]
    pub fn dev_path(&self) -> &Path {
        &self.dev_path
    }

    /// The delay between reads; always positive.
    #[must_use]
    pub fn poll_interval(&self) -> Duration {
        self.poll_interval
    }

    /// The charge at or below which the battery is low.
    #[must_use]
    pub fn battery_charge_low(&self) -> ChargePercent {
        self.battery_charge_low
    }

    /// The freshness and exit policy.
    #[must_use]
    pub fn policy(&self) -> Policy {
        self.policy
    }

    /// Whether live telemetry was served before this process started.
    #[must_use]
    pub fn live_history(&self) -> LiveHistory {
        self.live_history
    }
}

/// Parses `[0-9]+`.
fn parse_integer(text: &str) -> Option<u64> {
    is_digits(text).then(|| text.parse().ok()).flatten()
}

/// Parses `[0-9]+(\.[0-9]+)?`.
fn parse_decimal(text: &str) -> Option<f64> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, "0"));
    (is_digits(whole) && is_digits(fraction))
        .then(|| text.parse().ok())
        .flatten()
}

fn is_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}
