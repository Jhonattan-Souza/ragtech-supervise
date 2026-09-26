//! Decides whether a read from Supervise is published as live telemetry.

use std::time::{Duration, Instant};

use crate::nut::{Telemetry, Unavailable};
use crate::supervise::{ReadError, Reading, Sample};

/// When a sample counts as live telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// Treat the sample found at startup as stale until Supervise writes a new one.
    pub require_fresh_sample: bool,
    /// Treat a sample as stale once it has not changed for this long.
    pub max_sample_age: Option<Duration>,
    /// Stop the bridge when telemetry becomes unavailable after being live.
    pub exit_on_invalid_after_live: bool,
}

/// Whether live telemetry was served before this bridge started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveHistory {
    /// No live telemetry has been served yet.
    NotServed,
    /// An earlier process already served live telemetry, for example the
    /// `--wait-for-valid` run during container startup.
    AlreadyServed,
}

/// The result of observing one read.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// What to publish.
    pub telemetry: Telemetry,
    /// The bridge must stop after publishing, per [`Policy::exit_on_invalid_after_live`].
    pub must_exit: bool,
}

/// Tracks sample freshness across reads.
#[derive(Debug)]
pub struct Bridge {
    policy: Policy,
    /// The sample found at startup, in fresh-sample mode.
    startup: Startup,
    /// The latest distinct sample and when it was first read.
    current: Option<(Sample, Instant)>,
    live_seen: bool,
}

#[derive(Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "a single instance per bridge; boxing buys nothing"
)]
enum Startup {
    /// No sample read yet; the first one becomes the baseline.
    AwaitingBaseline,
    /// Samples equal to this one are stale.
    Baseline(Sample),
    /// Fresh-sample mode is off.
    NotRequired,
}

impl Bridge {
    /// A bridge applying `policy`, given whether live telemetry was already
    /// served before it started.
    #[must_use]
    pub fn new(policy: Policy, history: LiveHistory) -> Self {
        let startup = if policy.require_fresh_sample {
            Startup::AwaitingBaseline
        } else {
            Startup::NotRequired
        };
        Self {
            policy,
            startup,
            current: None,
            live_seen: history == LiveHistory::AlreadyServed,
        }
    }

    /// Decides what to publish for a read made at `now`.
    pub fn observe(&mut self, now: Instant, read: Result<Option<Reading>, ReadError>) -> Outcome {
        let telemetry = match read {
            Err(ReadError::Unreadable { .. }) => {
                Telemetry::Unavailable(Unavailable::DatabaseUnreadable)
            }
            Err(_) => Telemetry::Unavailable(Unavailable::QueryFailed),
            Ok(None) => Telemetry::Unavailable(Unavailable::NoCurrentSample),
            Ok(Some(reading)) => match self.staleness(now, &reading) {
                Some(reason) => Telemetry::Unavailable(reason),
                None if !reading.sample.flags.connected => {
                    Telemetry::Unavailable(Unavailable::UpsDisconnected)
                }
                None => Telemetry::Live(reading),
            },
        };
        let must_exit = match telemetry {
            Telemetry::Live(_) => {
                self.live_seen = true;
                false
            }
            Telemetry::Unavailable(_) => self.policy.exit_on_invalid_after_live && self.live_seen,
        };
        Outcome {
            telemetry,
            must_exit,
        }
    }

    /// Why `reading` is stale, if it is.
    fn staleness(&mut self, now: Instant, reading: &Reading) -> Option<Unavailable> {
        match &self.startup {
            Startup::AwaitingBaseline => {
                self.startup = Startup::Baseline(reading.sample.clone());
                return Some(Unavailable::StaleStartupSample);
            }
            Startup::Baseline(baseline) if *baseline == reading.sample => {
                return Some(Unavailable::StaleStartupSample);
            }
            Startup::Baseline(_) | Startup::NotRequired => {}
        }
        let first_read = match &self.current {
            Some((current, first_read)) if *current == reading.sample => *first_read,
            _ => {
                self.current = Some((reading.sample.clone(), now));
                now
            }
        };
        self.policy
            .max_sample_age
            .filter(|max_age| now.saturating_duration_since(first_read) > *max_age)
            .map(|_| Unavailable::StaleSourceSample)
    }
}
