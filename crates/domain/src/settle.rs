//! Deciding when the guest has settled after a step.
//!
//! The guest is settled once nothing observed has changed for the settle window.
//! A step that has not settled when its limit is reached is unsettled.

use std::time::Duration;

/// Where a step stands with respect to settling, measured from the step's start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettleState {
    Waiting,
    Settled { at: Duration },
    Unsettled,
}

/// Tracks changes observed after a step and decides whether the guest has settled.
#[derive(Clone, Debug)]
pub struct SettleTracker {
    window: Duration,
    limit: Duration,
    last_change: Duration,
}

impl SettleTracker {
    /// Starts tracking a step at elapsed time zero.
    ///
    /// `window` is how long nothing may change; `limit` is how long the step may take to settle.
    pub fn new(window: Duration, limit: Duration) -> Self {
        Self {
            window,
            limit,
            last_change: Duration::ZERO,
        }
    }

    /// Records that an observed value changed at `at`, measured from the step's start.
    pub fn record_change(&mut self, at: Duration) {
        self.last_change = self.last_change.max(at);
    }

    /// Returns the state at `now`, measured from the step's start.
    pub fn state(&self, now: Duration) -> SettleState {
        let settles_at = self.last_change + self.window;
        if settles_at <= now && settles_at <= self.limit {
            SettleState::Settled { at: settles_at }
        } else if now >= self.limit {
            SettleState::Unsettled
        } else {
            SettleState::Waiting
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_secs(5);
    const LIMIT: Duration = Duration::from_secs(30);

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    #[test]
    fn settles_one_window_after_the_start_when_nothing_changes() {
        let tracker = SettleTracker::new(WINDOW, LIMIT);

        assert_eq!(tracker.state(secs(5)), SettleState::Settled { at: secs(5) });
    }

    #[test]
    fn a_change_restarts_the_window() {
        let mut tracker = SettleTracker::new(WINDOW, LIMIT);
        tracker.record_change(secs(3));

        assert_eq!(
            tracker.state(secs(8) - Duration::from_millis(1)),
            SettleState::Waiting
        );
        assert_eq!(tracker.state(secs(8)), SettleState::Settled { at: secs(8) });
    }

    #[test]
    fn is_unsettled_when_changes_continue_to_the_limit() {
        let mut tracker = SettleTracker::new(WINDOW, LIMIT);
        tracker.record_change(secs(28));

        assert_eq!(tracker.state(secs(30)), SettleState::Unsettled);
    }
}
