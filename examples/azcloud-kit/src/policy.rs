//! How a request to an Azlin service is asked again: one policy for the token server's calls,
//! a drive's requests to its nodes and the sync's retries, reading what an answer means from
//! [`crate::answer`].
//!
//! Only a transient answer is asked again, and always as the same request - the same drive
//! token, the same idempotency key, the same body: a retry never makes a new one. A request
//! that may have done something goes again only when it answers the same when it comes twice
//! ([`Repeat`]). The pause doubles from `base` up to `ceiling` with a random part, and is never
//! shorter than the `Retry-After` the service named; a longer `Retry-After` than `max_wait` is
//! the caller's to wait out (its error says how long), and `attempts` caps the tries.

use std::{sync::Arc, time::Duration};

use crate::answer::{Outcome, Spent};

/// How a pause is taken (the tests record it instead of sleeping).
pub type Sleep = Arc<dyn Fn(Duration) + Send + Sync>;

/// After which transient answers a request may be sent again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    /// Only after one that spent nothing: a drive token's refresh or a checkout, sent twice,
    /// could spend or make twice.
    NothingSpent,
    /// After any: the request answers the same when it comes twice (a read, a grant named by
    /// its idempotency key).
    Keyed,
}

/// How often and after which pauses one request is tried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    /// Tries in all, the first one included (at least one).
    pub attempts: u32,
    /// The pause before the first retry; it doubles every retry, up to `ceiling`.
    pub base: Duration,
    pub ceiling: Duration,
    /// The longest `Retry-After` waited out here; a longer one ends the tries.
    pub max_wait: Duration,
}

impl Policy {
    /// A call to the token server: three tries, from a second. A pause up to ten seconds is
    /// waited out in the call; its rate limit's minute is the caller's (a sync's pass waits for
    /// it, a window says when the next try is).
    pub const TOKEN: Policy = Policy {
        attempts: 3,
        base: Duration::from_secs(1),
        ceiling: Duration::from_secs(8),
        max_wait: Duration::from_secs(10),
    };

    /// One request of a drive to its nodes: every node three times, from 250 ms.
    pub const NODE: Policy = Policy {
        attempts: 3,
        base: Duration::from_millis(250),
        ceiling: Duration::from_secs(8),
        max_wait: Duration::from_secs(30),
    };

    /// A request of a sync's pass: four tries, 100 ms doubling up to 5 s.
    pub const REQUEST: Policy = Policy {
        attempts: 4,
        base: Duration::from_millis(100),
        ceiling: Duration::from_secs(5),
        max_wait: Duration::from_secs(30),
    };

    /// Every node once, no pause (a probe, a lane that falls back on its own).
    #[must_use]
    pub fn once() -> Policy {
        Policy {
            attempts: 1,
            ..Policy::NODE
        }
    }

    /// The pause before try `attempt + 1` (`attempt` from 1): `base * 2^(attempt-1)` capped at
    /// the ceiling, times `jitter` in [0, 1) - devices that failed together do not come back
    /// together.
    #[must_use]
    pub fn delay(&self, attempt: u32, jitter: f64) -> Duration {
        let exp = self
            .base
            .saturating_mul(1u32 << attempt.saturating_sub(1).min(20))
            .min(self.ceiling);
        exp.mul_f64(jitter.clamp(0.0, 1.0))
    }

    /// [`Policy::delay`] with a random half on top of a fixed half: it still grows with every
    /// try.
    #[must_use]
    pub fn pause_after(&self, attempt: u32) -> Duration {
        self.delay(attempt, 0.5 + random_jitter() / 2.0)
    }

    /// The pause after try `attempt` when the service asked for `asked`: never sooner than it
    /// asked, nor than the backoff; `None` when it asked for longer than this policy waits.
    #[must_use]
    pub fn pause(&self, asked: Option<Duration>, attempt: u32) -> Option<Duration> {
        if asked.is_some_and(|asked| asked > self.max_wait) {
            return None;
        }
        let backoff = self.pause_after(attempt);
        Some(asked.map_or(backoff, |asked| asked.max(backoff)))
    }

    /// Whether a request that got `outcome` on its try `attempt` goes again, and after which
    /// pause: only a transient answer, only one that spent nothing unless the request may come
    /// twice (`repeat`), only while tries are left and the pause asked for is one this policy
    /// waits out.
    #[must_use]
    pub fn again(&self, outcome: &Outcome, attempt: u32, repeat: Repeat) -> Option<Duration> {
        let Outcome::Transient {
            retry_after, spent, ..
        } = outcome
        else {
            return None;
        };
        if repeat == Repeat::NothingSpent && *spent == Spent::Maybe {
            return None;
        }
        if attempt >= self.attempts.max(1) {
            return None;
        }
        self.pause(*retry_after, attempt)
    }
}

/// A number in [0, 1) from the OS random source (the random part of every pause).
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn random_jitter() -> f64 {
    let mut bytes = [0u8; 8];
    if getrandom::getrandom(&mut bytes).is_err() {
        return 0.5;
    }
    // 53 random bits over 2^53: exactly representable, below one.
    (u64::from_le_bytes(bytes) >> 11) as f64 / (1u64 << 53) as f64
}
