//! When the vault locks by itself, and how long a wrong master password makes the user wait.
//!
//! - [`AutoLock`]: the vault locks after N minutes without input (0 = never); every key or click
//!   in the window restarts the count.
//! - [`Attempts`]: after the third wrong password the next try waits 30 s, after every further
//!   one twice as long, at most 5 minutes. (Argon2id already makes each try cost a quarter second; this is
//!   the visible back-off the plan asks for.)

/// The vault's idle lock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutoLock {
    /// Minutes without input before the vault locks (0 = never).
    pub idle_minutes: u64,
    last_activity: u64,
}

impl AutoLock {
    #[must_use]
    pub fn new(idle_minutes: u64, now: u64) -> AutoLock {
        AutoLock {
            idle_minutes,
            last_activity: now,
        }
    }

    /// Input at `now` (seconds).
    pub fn touch(&mut self, now: u64) {
        self.last_activity = self.last_activity.max(now);
    }

    /// Seconds until the vault locks; `None` when it never locks by itself.
    #[must_use]
    pub fn remaining(&self, now: u64) -> Option<u64> {
        if self.idle_minutes == 0 {
            return None;
        }
        Some((self.last_activity + self.idle_minutes * 60).saturating_sub(now))
    }

    /// Whether the vault locks now.
    #[must_use]
    pub fn due(&self, now: u64) -> bool {
        self.remaining(now) == Some(0)
    }
}

/// Wrong master passwords and the wait they cause.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Attempts {
    failures: u32,
    wait_until: u64,
}

impl Attempts {
    /// Wrong passwords without a wait.
    pub const FREE: u32 = 3;
    /// The first wait, in seconds; it doubles with every further wrong password.
    pub const WAIT_SECONDS: u64 = 30;
    /// The longest wait.
    pub const MAX_WAIT_SECONDS: u64 = 300;

    /// A wrong password at `now`.
    pub fn failed(&mut self, now: u64) {
        self.failures = self.failures.saturating_add(1);
        if self.failures >= Attempts::FREE {
            let doublings = (self.failures - Attempts::FREE).min(16);
            let wait = (Attempts::WAIT_SECONDS << doublings).min(Attempts::MAX_WAIT_SECONDS);
            self.wait_until = now + wait;
        }
    }

    /// The right password: the count starts over.
    pub fn succeeded(&mut self) {
        *self = Attempts::default();
    }

    /// Seconds before the next try is taken (0 = now).
    #[must_use]
    pub fn wait(&self, now: u64) -> u64 {
        self.wait_until.saturating_sub(now)
    }

    /// The line under the password field after a wrong password ("" when none was wrong).
    #[must_use]
    pub fn message(&self, now: u64) -> String {
        if self.failures == 0 {
            return String::new();
        }
        let wait = self.wait(now);
        if wait > 0 {
            return format!("Wrong password. Wait {wait} s before the next try.");
        }
        if self.failures < Attempts::FREE {
            let left = Attempts::FREE - self.failures;
            return format!(
                "Wrong password. {left} attempt{} left before a {} s wait.",
                if left == 1 { "" } else { "s" },
                Attempts::WAIT_SECONDS
            );
        }
        "Wrong password.".to_string()
    }
}

/// `m:ss` of a number of seconds (`4:32`), `h:mm:ss` from an hour on.
#[must_use]
pub fn clock(seconds: u64) -> String {
    let (h, m, s) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_wrong_passwords_are_free_then_the_waits_double_up_to_five_minutes() {
        let mut a = Attempts::default();
        assert_eq!(a.message(0), "");
        a.failed(100);
        assert_eq!(a.wait(100), 0);
        assert_eq!(
            a.message(100),
            "Wrong password. 2 attempts left before a 30 s wait."
        );
        a.failed(101);
        assert_eq!(
            a.message(101),
            "Wrong password. 1 attempt left before a 30 s wait."
        );
        a.failed(102);
        assert_eq!(a.wait(102), 30);
        assert_eq!(
            a.message(110),
            "Wrong password. Wait 22 s before the next try."
        );
        assert_eq!(a.wait(132), 0);
        a.failed(132);
        assert_eq!(a.wait(132), 60);
        for n in 0..10 {
            a.failed(1000 + n);
        }
        assert_eq!(a.wait(1009), Attempts::MAX_WAIT_SECONDS);
    }

    #[test]
    fn the_right_password_starts_the_count_over() {
        let mut a = Attempts::default();
        for n in 0..5 {
            a.failed(n);
        }
        a.succeeded();
        assert_eq!(a.wait(5), 0);
        assert_eq!(a.message(5), "");
    }

    #[test]
    fn the_vault_locks_after_its_idle_minutes_and_input_restarts_them() {
        let mut lock = AutoLock::new(5, 1000);
        assert_eq!(lock.remaining(1000), Some(300));
        assert!(!lock.due(1299));
        assert!(lock.due(1300));
        lock.touch(1200);
        assert_eq!(lock.remaining(1228), Some(272));
        assert!(!lock.due(1300));
        let never = AutoLock::new(0, 0);
        assert_eq!(never.remaining(1_000_000), None);
        assert!(!never.due(1_000_000));
    }

    #[test]
    fn clocks_read_minutes_and_seconds() {
        assert_eq!(clock(272), "4:32");
        assert_eq!(clock(9), "0:09");
        assert_eq!(clock(3_725), "1:02:05");
    }
}
