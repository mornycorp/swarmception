//! Watching a live swarm (#54): on an interval, fetch the remote's `tracking` and `main` off the
//! UI thread (the source's deadlines), and read the branch again only when one of them moved. On
//! by default only when the latest event was under [`FRESH_SECONDS`] old at the first read; an
//! error is kept to show, and backs the next check off (doubling, up to [`MAX_BACKOFF`]), never
//! failing the session. The schedule runs on [`Instant`]s the session is handed, so a test can
//! drive it.

use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_model::Timestamp;

/// The interval when nobody chose one, in seconds.
pub const DEFAULT_INTERVAL: u64 = 30;
/// The shortest and longest interval `live.set` takes, in seconds.
pub const MIN_INTERVAL: u64 = 5;
pub const MAX_INTERVAL: u64 = 3_600;
/// The longest an error backs the next check off (or the interval, when that is longer).
pub const MAX_BACKOFF: Duration = Duration::from_secs(600);
/// Live is on by default when the latest event is younger than this at the first read.
pub const FRESH_SECONDS: i64 = 30 * 60;
/// The deadline of each of a live check's fetches: shorter than a Fetch and Reload's.
pub const FETCH_DEADLINE: Duration = Duration::from_secs(30);

/// What a live check cost, in milliseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LiveCost {
    /// Fetching `tracking` and `main`, and reading their heads.
    pub fetch_ms: u64,
    /// Reading the branch and main's merges again: none when neither moved.
    pub read_ms: Option<u64>,
    /// The activity, the rows and the timeline's data built from what was read.
    pub build_ms: Option<u64>,
    /// The events the reload brought (none when nothing moved).
    pub new_events: Option<usize>,
}

/// `live.get`'s answer: whether the session watches the swarm, and how it is going.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LiveStatus {
    pub on: bool,
    /// Nobody chose: on because the latest event was under 30 min old at the first read, or off.
    pub default: bool,
    pub interval_seconds: u64,
    /// A check is going on.
    pub checking: bool,
    /// Seconds until the next check (none while one is going, or when off).
    pub next_in_seconds: Option<u64>,
    /// When the last check ended, and the last that brought new events.
    pub last_check: Option<Timestamp>,
    pub last_new: Option<Timestamp>,
    /// Checks made, and how many of them read the branch again.
    pub checks: u64,
    pub reloads: u64,
    /// Checks that failed in a row, and why the last did: it backs the next off.
    pub failures: u32,
    pub error: Option<String>,
    /// What the last check that worked cost.
    pub cost: Option<LiveCost>,
}

/// The session's live watch.
#[derive(Debug, Clone)]
pub(crate) struct LiveState {
    pub(crate) on: bool,
    /// Someone chose `on` (`live.set`): the default no longer applies.
    pub(crate) chosen: bool,
    /// The default was applied, at the repository's first read.
    pub(crate) decided: bool,
    pub(crate) interval: Duration,
    /// When the next check is due; none: at once.
    pub(crate) due: Option<Instant>,
    pub(crate) failures: u32,
    pub(crate) error: Option<String>,
    pub(crate) last_check: Option<Timestamp>,
    pub(crate) last_new: Option<Timestamp>,
    pub(crate) checks: u64,
    pub(crate) reloads: u64,
    pub(crate) cost: Option<LiveCost>,
}

impl Default for LiveState {
    fn default() -> Self {
        Self {
            on: false,
            chosen: false,
            decided: false,
            interval: Duration::from_secs(DEFAULT_INTERVAL),
            due: None,
            failures: 0,
            error: None,
            last_check: None,
            last_new: None,
            checks: 0,
            reloads: 0,
            cost: None,
        }
    }
}

impl LiveState {
    /// A new repository's watch: what was chosen stays, the rest starts afresh.
    pub(crate) fn reopened(&self) -> Self {
        Self { on: self.on && self.chosen, chosen: self.chosen, interval: self.interval, ..Self::default() }
    }

    /// The wait after a check: the interval, doubled for each failure in a row, up to
    /// [`MAX_BACKOFF`] (or the interval, when that is longer).
    pub(crate) fn wait(&self) -> Duration {
        if self.failures == 0 {
            return self.interval;
        }
        let doubled = self.interval.saturating_mul(1u32.checked_shl(self.failures.min(16)).unwrap_or(u32::MAX));
        doubled.min(MAX_BACKOFF.max(self.interval))
    }

    /// Whether a check is due at `now`.
    pub(crate) fn is_due(&self, now: Instant) -> bool {
        self.on && self.due.is_none_or(|due| now >= due)
    }

    /// The default, at the first read: on when the latest event (`last`) is under
    /// [`FRESH_SECONDS`] old at `wall`; the first check one interval on.
    pub(crate) fn decide(&mut self, last: Option<Timestamp>, wall: Timestamp, now: Instant) {
        if self.chosen || self.decided {
            return;
        }
        self.decided = true;
        self.on = last.is_some_and(|last| wall.0.saturating_sub(last.0) < FRESH_SECONDS);
        self.due = self.on.then(|| now + self.interval);
    }

    /// `live.set`'s choice: on or off, and the interval; turned on, the first check is at once.
    pub(crate) fn choose(&mut self, on: Option<bool>, interval: Option<Duration>, now: Instant) {
        if let Some(interval) = interval {
            self.interval = interval;
            if self.on {
                self.due = Some(now + self.wait());
            }
        }
        if let Some(on) = on {
            if on && !self.on {
                self.due = None;
            }
            self.on = on;
            self.chosen = true;
            self.failures = if on { self.failures } else { 0 };
        }
    }

    /// A check ended at `now` (`wall` on the clock): what it cost, or why it failed. The next is
    /// due one wait on.
    pub(crate) fn checked(&mut self, now: Instant, wall: Timestamp, result: Result<LiveCost, String>) {
        self.checks = self.checks.saturating_add(1);
        self.last_check = Some(wall);
        match result {
            Ok(cost) => {
                if cost.read_ms.is_some() {
                    self.reloads = self.reloads.saturating_add(1);
                }
                if cost.new_events.is_some_and(|n| n > 0) {
                    self.last_new = Some(wall);
                }
                self.cost = Some(cost);
                self.failures = 0;
                self.error = None;
            }
            Err(error) => {
                self.failures = self.failures.saturating_add(1);
                self.error = Some(error);
            }
        }
        self.due = Some(now + self.wait());
    }

    /// What `live.get` says at `now`, `checking` when a check is going on.
    pub(crate) fn status(&self, now: Instant, checking: bool) -> LiveStatus {
        let next = (self.on && !checking).then(|| self.due.map_or(0, |due| due.saturating_duration_since(now).as_secs()));
        LiveStatus {
            on: self.on,
            default: !self.chosen,
            interval_seconds: self.interval.as_secs(),
            checking,
            next_in_seconds: next,
            last_check: self.last_check,
            last_new: self.last_new,
            checks: self.checks,
            reloads: self.reloads,
            failures: self.failures,
            error: self.error.clone(),
            cost: self.cost,
        }
    }
}

/// Milliseconds, for [`LiveCost`].
pub(crate) fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn on_by_default_only_for_a_fresh_swarm() {
        let now = Instant::now();
        let wall = Timestamp(1_000_000);
        let mut fresh = LiveState::default();
        fresh.decide(Some(Timestamp(wall.0 - 29 * 60)), wall, now);
        assert!(fresh.on && fresh.decided);
        assert_eq!(fresh.due, Some(now + secs(DEFAULT_INTERVAL)), "the first check an interval after the read");
        let mut stale = LiveState::default();
        stale.decide(Some(Timestamp(wall.0 - 31 * 60)), wall, now);
        assert!(!stale.on);
        let mut empty = LiveState::default();
        empty.decide(None, wall, now);
        assert!(!empty.on);
        // Decided once: a later read doesn't turn it back on.
        stale.decide(Some(wall), wall, now);
        assert!(!stale.on);
        // A choice stands over the default.
        let mut chosen = LiveState::default();
        chosen.choose(Some(false), None, now);
        chosen.decide(Some(wall), wall, now);
        assert!(!chosen.on);
        assert!(!chosen.status(now, false).default);
    }

    #[test]
    fn an_error_backs_off_and_a_success_resets() {
        let now = Instant::now();
        let mut live = LiveState { on: true, ..LiveState::default() };
        assert!(live.is_due(now), "on with nothing due: at once");
        live.checked(now, Timestamp(5), Err("no network".into()));
        assert_eq!((live.failures, live.wait()), (1, secs(60)));
        assert!(!live.is_due(now + secs(59)));
        assert!(live.is_due(now + secs(60)));
        for _ in 0..10 {
            live.checked(now, Timestamp(5), Err("no network".into()));
        }
        assert_eq!(live.wait(), MAX_BACKOFF, "capped");
        assert_eq!(live.status(now, false).error.as_deref(), Some("no network"));
        live.checked(now, Timestamp(6), Ok(LiveCost { fetch_ms: 3, read_ms: Some(4), build_ms: Some(5), new_events: Some(2) }));
        assert_eq!((live.failures, live.wait(), live.error.clone()), (0, secs(30), None));
        assert_eq!((live.checks, live.reloads, live.last_new), (12, 1, Some(Timestamp(6))));
        // A long interval backs off no further than itself.
        let mut slow = LiveState { on: true, interval: secs(3_600), failures: 3, ..LiveState::default() };
        assert_eq!(slow.wait(), secs(3_600));
        slow.failures = u32::MAX;
        assert_eq!(slow.wait(), secs(3_600));
    }

    #[test]
    fn choosing_on_checks_at_once_and_off_stops() {
        let now = Instant::now();
        let mut live = LiveState::default();
        assert!(!live.is_due(now));
        live.choose(Some(true), Some(secs(10)), now);
        assert!(live.is_due(now));
        live.checked(now, Timestamp(1), Ok(LiveCost::default()));
        assert_eq!(live.status(now, false).next_in_seconds, Some(10));
        assert_eq!(live.status(now, true).next_in_seconds, None, "none while checking");
        live.choose(None, Some(secs(60)), now);
        assert_eq!(live.status(now, false).next_in_seconds, Some(60), "a new interval reschedules");
        live.choose(Some(false), None, now);
        assert!(!live.is_due(now + secs(1_000)));
        assert_eq!(live.status(now, false).next_in_seconds, None);
        let reopened = live.reopened();
        assert!(!reopened.on && reopened.chosen && reopened.interval == secs(60));
    }
}
