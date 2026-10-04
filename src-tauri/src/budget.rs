//! Rolling request budget for the Toggl API.
//!
//! Free-plan accounts are limited to about 30 requests per hour. That is much
//! lower than the documented limit of about 1 request per second, so the app
//! cannot poll on a fixed timer. This module keeps a sliding 1-hour window of
//! request timestamps. It answers two questions for the sync loop:
//!   * `try_consume()` - May I spend one request right now?
//!   * `until_refill()` - How long until a slot frees up again?
//!
//! The window lives in memory only. On restart, the app assumes it is empty
//! and lets the first few requests populate it again. Toggl's own 429 +
//! Retry-After response is the hard backstop if the assumption is wrong.
//! A 402 or 429 response also records a server block: the app pauses every
//! request until the reset time Toggl reports. See [`Budget::block_for`].

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(3600);
/// The default for the free plan. Paid plans allow more. The user can change
/// this in Settings.
pub const DEFAULT_MAX_REQUESTS: usize = 30;
/// Keep some headroom, so a burst of user actions (start, stop, edit) never
/// gets blocked by background polling that ate the last slots.
const BACKGROUND_RESERVE: usize = 4;

#[derive(Default)]
struct Window {
    hits: VecDeque<Instant>,
}

pub struct Budget {
    inner: Mutex<Window>,
    max: AtomicUsize,
    /// The instant until which the server's quota limit blocks all requests.
    /// Set from a 402 or 429 response. Toggl's own window is the authority;
    /// ours is an in-memory guess that resets on restart.
    blocked_until: Mutex<Option<Instant>>,
}

impl Default for Budget {
    fn default() -> Self {
        Self::new()
    }
}

impl Budget {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Window::default()),
            max: AtomicUsize::new(DEFAULT_MAX_REQUESTS),
            blocked_until: Mutex::new(None),
        }
    }

    /// Remember that the server rejected a request for a quota reason. Keep
    /// the longest block seen. While a block is active, every headroom check
    /// below reports false, so the loop stops sending anything.
    pub fn block_for(&self, d: Duration) {
        if d.is_zero() {
            return;
        }
        let until = Instant::now() + d;
        let mut b = self.blocked_until.lock().unwrap();
        if b.map(|cur| until > cur).unwrap_or(true) {
            *b = Some(until);
        }
    }

    /// Time left on the server block. Zero when not blocked.
    pub fn until_blocked(&self) -> Duration {
        let b = self.blocked_until.lock().unwrap();
        match *b {
            Some(until) => until.saturating_duration_since(Instant::now()),
            None => Duration::ZERO,
        }
    }

    /// Forget the server block. A request that succeeded proves the quota
    /// window is open again, whatever the earlier response said.
    pub fn clear_block(&self) {
        *self.blocked_until.lock().unwrap() = None;
    }

    /// The hourly cap the user configures. The value is limited to a sane
    /// range.
    pub fn set_max(&self, max: usize) {
        self.max.store(max.clamp(5, 2000), Ordering::Relaxed);
    }

    pub fn max(&self) -> usize {
        self.max.load(Ordering::Relaxed)
    }

    /// Background polls stop before the last few slots. This always leaves
    /// room for interactive actions: start, stop, and edit pushes.
    fn background_budget(&self) -> usize {
        self.max().saturating_sub(BACKGROUND_RESERVE)
    }

    fn prune(&self, now: Instant) -> usize {
        let mut w = self.inner.lock().unwrap();
        while let Some(&front) = w.hits.front() {
            if now.duration_since(front) >= WINDOW {
                w.hits.pop_front();
            } else {
                break;
            }
        }
        w.hits.len()
    }

    pub fn used(&self) -> usize {
        self.prune(Instant::now())
    }

    pub fn remaining(&self) -> usize {
        self.max().saturating_sub(self.used())
    }

    /// Records a request that is about to be sent, interactive or background.
    pub fn record(&self) {
        let now = Instant::now();
        self.prune(now);
        self.inner.lock().unwrap().hits.push_back(now);
    }

    /// Spends a background slot when the count is under the reduced
    /// background budget. Returns false when the budget is exhausted. The
    /// caller should then defer polling.
    pub fn try_consume_background(&self) -> bool {
        if !self.until_blocked().is_zero() {
            return false;
        }
        let now = Instant::now();
        let used = self.prune(now);
        if used >= self.background_budget() {
            return false;
        }
        self.inner.lock().unwrap().hits.push_back(now);
        true
    }

    /// Checks without spending. Used to schedule the next wake-up.
    pub fn has_background_headroom(&self) -> bool {
        self.prune(Instant::now()) < self.background_budget() && self.until_blocked().is_zero()
    }

    /// True when any slot is left in the rolling hour. Interactive pushes
    /// use this. Background polls use `has_background_headroom`, which keeps
    /// a reserve. A server block clears this too.
    pub fn has_headroom(&self) -> bool {
        self.prune(Instant::now()) < self.max() && self.until_blocked().is_zero()
    }

    /// How long until the oldest request ages out and a background slot
    /// frees up. The server block can outlast our window guess, so the
    /// longer of the two wins.
    pub fn until_refill(&self) -> Duration {
        let now = Instant::now();
        let window = {
            let guard = self.inner.lock().unwrap();
            match guard.hits.front() {
                Some(&front) => WINDOW.saturating_sub(now.duration_since(front)),
                None => Duration::ZERO,
            }
        };
        window.max(self.until_blocked())
    }
}

/// Format a wait time for a message: "28m", "1h 05m". Rounds minutes up.
pub fn fmt_wait(d: Duration) -> String {
    let mins = (d.as_secs() + 59) / 60;
    if mins < 60 {
        format!("{mins}m")
    } else {
        format!("{}h {:02}m", mins / 60, mins % 60)
    }
}
