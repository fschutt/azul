//! `Version`: the position of a head in its register (planning/architecture.md section 5.13 "A head
//! never goes backwards", section 2.16 "`heads`: each space's CAS register and its history"; build
//! unit S16, lane W1-S16a).
//!
//! A version is a positive counter: the first write of a register is version 1 and every later write
//! is one above the head it replaced. Versions never go down, not even through a point-in-time
//! restore (a restore writes a NEW, higher version that carries the old value: [`crate::restore`]).
//! There is no version 0: "nothing yet" is `None` of an `Option<Version>`, so a head that claims
//! version 0 cannot be built (the decoder refuses it) and a counter never wraps (`next` is checked).

use core::fmt;
use core::num::NonZeroU64;

/// The version of a head: positive, one above the head it replaced.
///
/// Implements planning/architecture.md section 5.13 (the CAS register of a space: `head_cas` with the
/// expected version) and section 2.16 (`heads`).
///
/// Caller: `HeadRegister` (every record), `resolve_lost_cas`, `judge` (the device's memory of a
/// head); the verbs `head_get`, `head_cas` and `head_history` of the node (SYNC-APPLY) and of the
/// client (SYNC-CLIENT).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(NonZeroU64);

impl Version {
    /// The version of the first write of a register.
    pub const FIRST: Version = Version(NonZeroU64::MIN);

    /// The version with this number; `None` for 0 (there is no version 0).
    #[allow(clippy::manual_map)] // `Option::map` is not a `const fn`
    pub const fn new(number: u64) -> Option<Version> {
        match NonZeroU64::new(number) {
            Some(number) => Some(Version(number)),
            None => None,
        }
    }

    /// The number of the version.
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    /// The version after this one; `None` at the largest number (a counter never wraps).
    pub fn next(self) -> Option<Version> {
        self.0.get().checked_add(1).and_then(Version::new)
    }
}

/// Decimal.
impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Version({})", self.0)
    }
}
