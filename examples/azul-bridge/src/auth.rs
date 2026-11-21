//! Who may use the bridge: the one user name (the account's address) and the per-install
//! password the bridge made at `init` - random, 125 bits, printed once and kept in the secret
//! store ([`crate::secrets`]). Every server checks it here: the password in constant time,
//! wrong attempts counted per connection and across the bridge ([`FailureGate`]).

use std::{
    collections::VecDeque,
    fmt,
    sync::Mutex,
    time::{Duration, Instant},
};

use base64::Engine;

/// The characters of a password: Crockford's base32 in lower case (no `i l o u`), so it can be
/// read out and typed without confusing `1` and `l`.
const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// A new password: 16 bytes of the OS's randomness as 25 characters (125 bits) in five groups,
/// `k7m2p-9qxat-...`.
///
/// # Errors
///
/// When the OS gives no randomness.
pub fn new_password() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|e| format!("no randomness from the OS: {e}"))?;
    Ok(password_from(&bytes))
}

/// The password of 16 random bytes (deterministic: the tests' half of [`new_password`]).
#[must_use]
pub fn password_from(bytes: &[u8; 16]) -> String {
    let mut chars = Vec::with_capacity(25);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for byte in bytes {
        acc = (acc << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 && chars.len() < 25 {
            bits -= 5;
            chars.push(ALPHABET[((acc >> bits) & 31) as usize]);
        }
        acc &= (1 << bits) - 1;
    }
    let mut out = String::with_capacity(29);
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && i % 5 == 0 {
            out.push('-');
        }
        out.push(char::from(*c));
    }
    out
}

/// Whether `a` and `b` are the same bytes, in a time that depends on their lengths only, never
/// on where they differ.
#[must_use]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = (a.len() ^ b.len()) as u64;
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= u64::from(x ^ y);
    }
    std::hint::black_box(diff) == 0
}

/// The one user of the bridge. `Debug` shows no password.
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    /// The account's address: what a mail program signs in with.
    pub user: String,
    password: String,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("user", &self.user)
            .field("password", &"<hidden>")
            .finish()
    }
}

impl Credentials {
    #[must_use]
    pub fn new(user: &str, password: &str) -> Credentials {
        Credentials {
            user: user.trim().to_string(),
            password: password.to_string(),
        }
    }

    /// Whether `user` (any case) and `password` (exactly; dashes and spaces a person typed
    /// between the groups do not count) are this user's. The password is compared in constant
    /// time, also when the user name is wrong.
    #[must_use]
    pub fn check(&self, user: &[u8], password: &[u8]) -> bool {
        let typed: Vec<u8> = password
            .iter()
            .copied()
            .filter(|b| *b != b'-' && *b != b' ')
            .collect();
        let ours: Vec<u8> = self
            .password
            .bytes()
            .filter(|b| *b != b'-' && *b != b' ')
            .collect();
        let password_ok = constant_time_eq(&typed, &ours);
        let user_ok = std::str::from_utf8(user)
            .map(|u| u.trim().eq_ignore_ascii_case(&self.user))
            .unwrap_or(false);
        password_ok & user_ok
    }

    /// Whether `given` is what `compute` makes of this user's password (HTTP Digest: the
    /// response is a hash of it), for the user `user`: the password as it was printed and
    /// without the dashes between its groups (a person may have typed either), compared in
    /// constant time.
    #[must_use]
    pub fn check_derived(&self, user: &str, given: &str, compute: impl Fn(&str) -> String) -> bool {
        let plain: String = self.password.chars().filter(|c| *c != '-' && *c != ' ').collect();
        let as_printed = constant_time_eq(compute(&self.password).as_bytes(), given.as_bytes());
        let without_dashes = constant_time_eq(compute(&plain).as_bytes(), given.as_bytes());
        let user_ok = user.trim().eq_ignore_ascii_case(&self.user);
        (as_printed | without_dashes) & user_ok
    }
}

/// SASL PLAIN's message (RFC 4616), base64: `authzid NUL authcid NUL passwd`. The user name
/// and the password; `None` for anything that is not one (or names another identity to act
/// as).
#[must_use]
pub fn decode_plain(base64_text: &str) -> Option<(Vec<u8>, Vec<u8>)> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(base64_text.trim())
        .ok()?;
    let mut parts = raw.split(|b| *b == 0);
    let authzid = parts.next()?;
    let user = parts.next()?;
    let password = parts.next()?;
    if parts.next().is_some() || user.is_empty() {
        return None;
    }
    if !authzid.is_empty() && authzid != user {
        return None;
    }
    Some((user.to_vec(), password.to_vec()))
}

/// Base64 of one SASL LOGIN answer (the user name or the password).
#[must_use]
pub fn decode_base64(text: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(text.trim())
        .ok()
}

/// Base64 (for a server's challenge).
#[must_use]
pub fn encode_base64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Wrong passwords across the bridge: after `free` of them within `window`, every further
/// sign-in waits `delay` first, so guessing from many connections at once is as slow as from
/// one.
#[derive(Debug)]
pub struct FailureGate {
    recent: Mutex<VecDeque<Instant>>,
    free: u32,
    window: Duration,
    delay: Duration,
}

impl FailureGate {
    #[must_use]
    pub fn new(free: u32, window: Duration, delay: Duration) -> FailureGate {
        FailureGate {
            recent: Mutex::new(VecDeque::new()),
            free,
            window,
            delay,
        }
    }

    fn recent_failures(&self, now: Instant) -> usize {
        let mut recent = self
            .recent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while recent
            .front()
            .is_some_and(|at| now.duration_since(*at) > self.window)
        {
            recent.pop_front();
        }
        recent.len()
    }

    /// How long a sign-in must wait now before its password is checked.
    #[must_use]
    pub fn wait_before_attempt(&self) -> Duration {
        if self.recent_failures(Instant::now()) >= self.free as usize {
            self.delay
        } else {
            Duration::ZERO
        }
    }

    /// Waits as long as [`FailureGate::wait_before_attempt`] says.
    pub fn before_attempt(&self) {
        let wait = self.wait_before_attempt();
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }

    /// One more wrong password.
    pub fn failed(&self) {
        let now = Instant::now();
        let _ = self.recent_failures(now);
        let mut recent = self
            .recent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        recent.push_back(now);
        // Old ones are dropped above; a flood keeps at most this many.
        while recent.len() > 10_000 {
            recent.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_is_25_characters_of_crockford_base32_in_five_groups() {
        let password = password_from(&[0u8; 16]);
        assert_eq!(password, "00000-00000-00000-00000-00000");
        let password = password_from(&[0xff; 16]);
        assert_eq!(password, "zzzzz-zzzzz-zzzzz-zzzzz-zzzzz");
        let password = password_from(&[
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x10, 0x32, 0x54, 0x76, 0x98, 0xba,
            0xdc, 0xfe,
        ]);
        assert_eq!(password.len(), 29);
        assert!(password
            .bytes()
            .all(|b| b == b'-' || ALPHABET.contains(&b)));
        let random = new_password().unwrap();
        assert_eq!(random.len(), 29);
        assert_ne!(random, new_password().unwrap());
    }

    #[test]
    fn equal_bytes_compare_equal_and_any_difference_or_length_does_not() {
        assert!(constant_time_eq(b"", b""));
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secreT"));
        assert!(!constant_time_eq(b"secret", b"secret!"));
        assert!(!constant_time_eq(b"secret", b""));
        assert!(!constant_time_eq(b"\0", b""));
    }

    #[test]
    fn the_user_is_any_case_and_the_password_exact_but_for_the_group_dashes() {
        let creds = Credentials::new("Ada@Example.org", "abcde-fghjk");
        assert!(creds.check(b"ada@example.org", b"abcde-fghjk"));
        assert!(creds.check(b"ADA@EXAMPLE.ORG", b"abcdefghjk"));
        assert!(creds.check(b"ada@example.org", b"abcde fghjk"));
        assert!(!creds.check(b"ada@example.org", b"ABCDE-FGHJK"));
        assert!(!creds.check(b"ben@example.org", b"abcde-fghjk"));
        assert!(!creds.check(b"ada@example.org", b""));
        assert!(!creds.check(b"\xff", b"abcde-fghjk"));
        assert!(!format!("{creds:?}").contains("abcde"));
    }

    #[test]
    fn sasl_plain_gives_the_user_and_password_and_refuses_acting_as_someone_else() {
        let encode = |raw: &[u8]| encode_base64(raw);
        assert_eq!(
            decode_plain(&encode(b"\0ada@example.org\0pw")),
            Some((b"ada@example.org".to_vec(), b"pw".to_vec()))
        );
        assert_eq!(
            decode_plain(&encode(b"ada@example.org\0ada@example.org\0pw")),
            Some((b"ada@example.org".to_vec(), b"pw".to_vec()))
        );
        assert_eq!(decode_plain(&encode(b"ben@example.org\0ada@example.org\0pw")), None);
        assert_eq!(decode_plain(&encode(b"\0\0pw")), None);
        assert_eq!(decode_plain(&encode(b"no separators")), None);
        assert_eq!(decode_plain("not base64!"), None);
    }

    #[test]
    fn after_the_free_failures_every_attempt_waits() {
        let gate = FailureGate::new(2, Duration::from_secs(60), Duration::from_millis(5));
        assert_eq!(gate.wait_before_attempt(), Duration::ZERO);
        gate.failed();
        assert_eq!(gate.wait_before_attempt(), Duration::ZERO);
        gate.failed();
        assert_eq!(gate.wait_before_attempt(), Duration::from_millis(5));
        let quick = FailureGate::new(1, Duration::from_millis(1), Duration::from_secs(9));
        quick.failed();
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(quick.wait_before_attempt(), Duration::ZERO, "the window has passed");
    }
}
