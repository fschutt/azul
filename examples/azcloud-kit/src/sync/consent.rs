//! The user's answers to the questions the guards ask, bound to what was shown (RF46 / AR5a-7;
//! AZ-S8, AZ-S9).
//!
//! A guard that stops a plan (a mass delete, a burst of rewrites) asks the user and changes
//! nothing until the answer comes. The answer used to be a flag in the pairing's states -
//! "delete them too", "these changes are mine" - read by the NEXT pass whatever it planned: a
//! pass that failed (offline, refused) left the flag set, and the first pass that worked days
//! later skipped the guard for everything it found, new accidents included.
//!
//! A [`Consent`] is the answer to ONE question: it names the kind and a hash of the keys (or the
//! pause) the user was shown. A pass that plans another set asks again; a pass that reaches its
//! end consumes the consents it was given, whether it needed them or not.

use serde::{Deserialize, Serialize};

/// What was asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentKind {
    /// "Delete them too": the files a mass delete would take (here or on the drive).
    MassDelete,
    /// "These changes are mine": the uploads a burst of rewrites paused.
    Burst,
}

impl ConsentKind {
    fn word(self) -> &'static str {
        match self {
            ConsentKind::MassDelete => "mass-delete",
            ConsentKind::Burst => "burst",
        }
    }
}

/// An answer, for the set of keys it was given for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Consent {
    pub kind: ConsentKind,
    /// A hash of the keys shown (BLAKE3 of the sorted keys, 64 hex digits): the set the answer
    /// covers, and no other.
    pub keys_hash: String,
    /// When it was given (seconds on this device's clock): for the display only, nothing is
    /// decided by it.
    pub issued_at: i64,
}

/// The hash of a set of keys: the same whatever their order, a different set a different hash.
#[must_use]
pub fn keys_hash(kind: ConsentKind, keys: &[&str]) -> String {
    let mut sorted: Vec<&str> = keys.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"azcloud consent\n");
    hasher.update(kind.word().as_bytes());
    for key in sorted {
        hasher.update(b"\n");
        hasher.update(&(key.len() as u64).to_le_bytes());
        hasher.update(key.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

impl Consent {
    /// The answer to the question about `keys`, given at `issued_at`.
    #[must_use]
    pub fn new(kind: ConsentKind, keys: &[&str], issued_at: i64) -> Consent {
        Consent {
            kind,
            keys_hash: keys_hash(kind, keys),
            issued_at,
        }
    }

    /// Whether this is the answer to a question of `kind` about exactly `keys`.
    #[must_use]
    pub fn covers(&self, kind: ConsentKind, keys: &[&str]) -> bool {
        self.kind == kind && self.keys_hash == keys_hash(kind, keys)
    }
}
