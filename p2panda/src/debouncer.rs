// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

use tokio::time::Instant;

/// Default quiet duration in milliseconds.
///
/// After at least one trigger arrived, this is the duration after which the task will be executed
/// if no further triggers arrive.
const DEFAULT_QUIET_PERIOD_MS: u64 = 200;

/// Default throttle duration in seconds.
///
/// In the case of events continuously arriving at a rate > quiet_period the task will still be
/// executed after throttle time has been reached.
const DEFAULT_MAX_WAIT_SECS: u64 = 1;

/// Debouncer with throttle for batching triggers which arrive in "bursts" up to a throttle
/// duration.
///
/// While triggers arrive at a frequency more often than the configured quiet period we want the
/// ultimate execution of a task to be repeatedly postponed. If no trigger arrives for the
/// configured `quiet_period` OR the `throttle` duration has been reached the task should be
/// executed.
///
/// Users should call `Debouncer::next_run_at` to receive an `Instant` at which the task should be
/// next run.
#[derive(Clone, Debug)]
pub struct Debouncer {
    quiet_period: Duration,
    throttle: Duration,
    pending: Option<Deadlines>,
}

#[derive(Clone, Copy, Debug)]
struct Deadlines {
    quiet_period_ends_at: Instant,
    throttle_ends_at: Instant,
}

impl Default for Debouncer {
    fn default() -> Self {
        Self {
            quiet_period: Duration::from_millis(DEFAULT_QUIET_PERIOD_MS),
            throttle: Duration::from_secs(DEFAULT_MAX_WAIT_SECS),
            pending: Default::default(),
        }
    }
}

impl Debouncer {
    /// Construct a new debouncer with `quiet_period` and `throttle` arguments.
    pub fn new(quiet_period: Duration, throttle: Duration) -> Self {
        Self {
            quiet_period,
            throttle,
            pending: None,
        }
    }

    /// Record a new trigger at `now` instant.
    pub fn record_trigger(&mut self, now: Instant) {
        let quiet_period_ends_at = now + self.quiet_period;

        match &mut self.pending {
            Some(pending) => {
                pending.quiet_period_ends_at =
                    pending.quiet_period_ends_at.max(quiet_period_ends_at);
            }
            None => {
                self.pending = Some(Deadlines {
                    quiet_period_ends_at,
                    throttle_ends_at: now + self.throttle,
                });
            }
        }
    }

    /// Get the instant at which the task should be executed if no new triggers arrive. The
    /// returned instant will either be the end of the current "quiet" period or the time when max
    /// wait expires, whichever is sooner.
    ///
    /// Returns None if there are no pending triggers.
    pub fn next_run_at(&self) -> Option<Instant> {
        let pending = self.pending?;
        Some(pending.quiet_period_ends_at.min(pending.throttle_ends_at))
    }

    /// Clear all pending triggers. Should be called when the task is actually executed.
    pub fn clear_pending(&mut self) {
        self.pending = None;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::Instant;

    use super::Debouncer;

    #[test]
    fn debounces_triggers_with_throttle() {
        let quiet_period = Duration::from_millis(100);
        let throttle = Duration::from_millis(300);
        let mut debouncer = Debouncer::new(quiet_period, throttle);
        let now = Instant::now();

        // No triggers yet.
        assert_eq!(debouncer.next_run_at(), None);

        // Record one trigger.
        debouncer.record_trigger(now);
        // Next run expected after quiet period expires.
        assert_eq!(debouncer.next_run_at(), Some(now + quiet_period));

        // Clearing for the run means nothing is due until the next trigger.
        debouncer.clear_pending();
        assert_eq!(debouncer.next_run_at(), None);

        // A burst of triggers repeatedly extends the quiet period.
        let trigger_1 = Duration::from_millis(200);
        let trigger_2 = Duration::from_millis(250);
        let trigger_3 = Duration::from_millis(300);
        debouncer.record_trigger(now + trigger_1);
        debouncer.record_trigger(now + trigger_2);
        debouncer.record_trigger(now + trigger_3);
        assert_eq!(
            debouncer.next_run_at(),
            Some(now + trigger_3 + quiet_period)
        );
        debouncer.clear_pending();

        // Continuous triggers every 50ms never leave a 100ms quiet period, so throttle caps them
        // from after the first one.
        for i in 0..10 {
            debouncer.record_trigger(now + Duration::from_millis(i * 50));
        }
        assert_eq!(debouncer.next_run_at(), Some(now + throttle));
        debouncer.clear_pending();
    }
}
