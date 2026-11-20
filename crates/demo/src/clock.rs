//! Time, behind a boundary the tests replace.
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub trait Clock: Send {
    /// A moment to measure from.
    fn now(&self) -> Instant;
    /// Seconds since 1970 UTC, as a status value of type `time` says it.
    fn unix(&self) -> i64;
    /// Waits, as an action that takes long does.
    fn sleep(&self, period: Duration);
}

/// The system's clock.
pub struct System;

impl Clock for System {
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn unix(&self) -> i64 {
        unix_now()
    }
    fn sleep(&self, period: Duration) {
        std::thread::sleep(period);
    }
}

pub(crate) fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64)
}
