//! Who may say "the user confirmed this delete" (SYNC-1, ARCH29 C8): one pure judge.
//!
//! A tombstone carries `confirmed: true` when the user answered "delete them too" to a mass-delete
//! question on the device that wrote it. Another device that meets the tombstone may skip its own
//! question - but only if the word is the user's. It is written by whoever can write the drive's
//! index: with an encrypted drive every member that holds the drive key (the account's own
//! devices and every person the drive was shared with), with a plain bucket everyone who can write
//! the bucket. The tombstone's `device` is a string its writer chooses, so comparing it with the
//! sync's device name (a hostname, or the literal "this device" when the system has none) or with
//! the token server's member ids is no check at all: the two name spaces never meet in the
//! product, which made the feature a no-op, and a stranger who names itself like a device of the
//! user's passes it (S7, X-DELTA-AZDRIVE-SYNC-1).
//!
//! What would make a confirmation the user's is an identity the drive AUTHENTICATES: a signature
//! of the writer over the tombstone, checked against a key the token server vouches for as one
//! of the account's own devices. This build's drives authenticate none (the drive key is the one
//! secret every member shares, and the roles of a policy are advisory), so [`verified_writer`]
//! answers `None` for every tombstone and [`trust_tombstone`] answers [`Trust::Ask`]: the
//! breaker counts every delete the drive makes for this folder, and the user's own second computer
//! asks its own user once more. That is safe and a little annoying; the day an index reader can
//! verify a writer it hands the judge an [`Authenticated`] and nothing else here changes.

use super::remote::Tombstone;

/// A member of the drive as the token server names it (`owner`, letters only): the identity a
/// verified writer is known by.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MemberId(String);

impl MemberId {
    /// The member named `id`.
    #[must_use]
    pub fn new(id: &str) -> MemberId {
        MemberId(id.to_string())
    }

    /// The name as the token server writes it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A writer the index reader has VERIFIED - a signature checked against a key the token server
/// vouches for - and not one a string in the index claims. Nothing but the reader makes one (no
/// public constructor, no `Clone`, no `Default`, no `Deserialize`): a JSON string cannot become
/// one, so the tombstone's own `device` can never be the witness.
#[derive(Debug, PartialEq, Eq)]
pub struct Authenticated(MemberId);

impl Authenticated {
    /// The member the signature belongs to.
    #[must_use]
    pub fn member(&self) -> &MemberId {
        &self.0
    }

    /// A writer as a reader that verified one would hand it over; tests only, until a reader does.
    #[cfg(test)]
    pub(crate) fn verified(member: MemberId) -> Authenticated {
        Authenticated(member)
    }
}

/// The judge's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trust {
    /// The user confirmed this delete on a device of theirs: the breaker does not count it.
    Trusted,
    /// Count it: the user is asked.
    Ask,
}

/// The writer of `tombstone` as the index reader verified it. The index of this build carries no
/// signature to verify (module doc), so there is none; this is the one place a reader that can
/// verify a writer plugs in.
#[must_use]
pub fn verified_writer(tombstone: &Tombstone) -> Option<Authenticated> {
    let _ = tombstone;
    None
}

/// Whether the word `confirmed` of `tombstone` is the user's: only when it is set, its writer was
/// verified ([`Authenticated`]) and that member is one of `own`, the account's own devices. Every
/// other cell - an unconfirmed tombstone, an unverified writer (`None`: whatever name the
/// tombstone carries), a verified stranger - is [`Trust::Ask`]. The tombstone's `device` string
/// is never read.
#[must_use]
pub fn trust_tombstone(
    tombstone: &Tombstone,
    writer: Option<Authenticated>,
    own: &[MemberId],
) -> Trust {
    match writer {
        Some(writer) if tombstone.confirmed && own.contains(writer.member()) => Trust::Trusted,
        Some(_) | None => Trust::Ask,
    }
}
