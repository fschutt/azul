//! Checkpoints of a chain (planning/architecture.md section 5.9 "Checkpoints: every 1,000 entries,
//! and monthly, the group records a checkpoint (the log's head and a hash of the registry state it
//! produced). A new device verifies from the latest checkpoint whose entries it can check, not
//! from the account's first entry"; build unit S17, lane W1-S17a).
//!
//! - [`checkpoint_due`] is the rule of when the group records one: 1,000 entries, or 30 days on the
//!   leader's stamp (DRIFT 4 of the brief: the plan says "monthly"; a calendar month and the clock
//!   that would measure it are open, so a month is 30 days of the leader's stamp, section 2.4).
//! - [`Checkpoint`] is the head the group signed plus the two claims that travel with it: the state
//!   hash the entries up to it produced (a device compares it with its own replay) and the stamp
//!   of the entry at the head (so the stamp rule holds across the boundary). Both are inside the
//!   checkpoint entry the head's hash commits to; this crate verifies the head's signature and
//!   hands the claims to the caller.
//! - [`verify_from_checkpoint`] verifies the entries AFTER a checkpoint: the checkpoint's head is
//!   checked, then every entry must be the next one, starting from the head, and of the restore
//!   epoch the head was signed in or later. Entries before the checkpoint are not needed. Its answer
//!   is the answer of [`crate::verify_from_genesis`] over the whole chain whenever the entries up to
//!   the checkpoint are sound (and the head's epoch is the epoch of its entry).

use azlin_seams::{mutant, Hash32, RestoreEpoch, Span, Stamp};

use crate::entry::{ChainEntry, Head, HeadVerifier, SignedHead};
use crate::link::{run, ChainRefusal, Tip};

/// A checkpoint is due after this many entries since the last one.
///
/// Implements planning/architecture.md section 5.9 (a checkpoint every 1,000 entries).
///
/// Caller: `checkpoint_due`; the metadata group's checkpoint writer (S17c integration).
pub const CHECKPOINT_ENTRIES: u64 = 1_000;

/// A checkpoint is due after this long since the last one, on the leader's stamp: "monthly" read
/// as 30 days (DRIFT 4 of the brief).
///
/// Implements planning/architecture.md section 5.9 (a checkpoint monthly) and section 2.4 (time is
/// the leader's stamp).
///
/// Caller: `checkpoint_due`.
pub const CHECKPOINT_AGE: Span = Span::days(30);

/// Whether the group records a checkpoint now: `entries_since` entries have been appended since the
/// last one (at `last`), and it is `now` (both stamps are the leader's). Due at 1,000 entries, or
/// when 30 days have passed. A `now` earlier than `last` is not a reason (the time clause is false;
/// the entry clause still applies): the stamps never run backwards, and an earlier `now` is
/// refused by the stamp check upstream.
///
/// Implements planning/architecture.md section 5.9 (a checkpoint every 1,000 entries, and monthly).
///
/// Caller: the metadata group's checkpoint writer (S17c integration, CHAIN-GLUE).
pub fn checkpoint_due(entries_since: u64, last: Stamp, now: Stamp) -> bool {
    if entries_since >= CHECKPOINT_ENTRIES && !mutant!("chain_checkpoint_entries_off") {
        return true;
    }
    if mutant!("chain_checkpoint_age_off") {
        return false;
    }
    match now.elapsed_since(last) {
        Some(age) => age >= CHECKPOINT_AGE,
        None => false,
    }
}

/// A checkpoint of a chain: the head the group signed, the stamp of the entry at the head, and the
/// hash of the state the entries up to it produced.
///
/// Implements planning/architecture.md section 5.9 (the log's head and a hash of the registry state
/// it produced).
///
/// Caller: the metadata group (builds it), a new device (`verify_from_checkpoint`, then compares
/// `state_hash` with its own replay; S17c `replay_from_checkpoint_state_equals_from_genesis`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    head: SignedHead,
    at: Stamp,
    state: Hash32,
}

impl Checkpoint {
    /// A checkpoint of `head`, whose entry has the stamp `at`, with the state hash `state`.
    pub fn new(head: SignedHead, at: Stamp, state: Hash32) -> Checkpoint {
        Checkpoint { head, at, state }
    }

    /// The signed head the checkpoint stands at.
    pub fn signed_head(&self) -> &SignedHead {
        &self.head
    }

    /// The head the checkpoint stands at.
    pub fn head(&self) -> Head {
        self.head.head
    }

    /// The restore epoch the group signed the checkpoint's head in: the entries after the
    /// checkpoint must be of this epoch or later.
    pub fn epoch(&self) -> RestoreEpoch {
        self.head.epoch
    }

    /// The stamp of the entry at the head.
    pub fn at(&self) -> Stamp {
        self.at
    }

    /// The hash of the state the entries up to the head produced.
    pub fn state_hash(&self) -> Hash32 {
        self.state
    }
}

/// Verifies the entries after a checkpoint: the checkpoint's head signature is checked by `v` (a
/// head that does not verify is a corrupt checkpoint: `Corrupt`, fetch another), then `rest` must
/// be a strictly sequential run that continues the head: the first entry names the head's hash as
/// its `prev`, every entry is the next one. The head after the last entry (the checkpoint's own
/// head when `rest` is empty).
///
/// Implements planning/architecture.md section 5.9 (a new device verifies from the latest
/// checkpoint whose entries it can check, not from the account's first entry).
///
/// Caller: a new device (CHAIN-GLUE) and S17c's `replay` from a checkpoint.
pub fn verify_from_checkpoint<E: ChainEntry>(
    cp: &Checkpoint,
    v: &dyn HeadVerifier,
    rest: &[E],
) -> Result<Head, ChainRefusal> {
    if !mutant!("chain_checkpoint_head_unverified") && v.verify_head(&cp.head).is_err() {
        return Err(ChainRefusal::Corrupt);
    }
    // The entries after the checkpoint must be of the restore epoch the group signed the head in, or
    // later: the lost timeline of a restore that the checkpoint already stands behind is refused.
    let mut tip = Tip::resume(cp.head.head, cp.at, cp.head.epoch);
    Ok(run(&mut tip, rest)?.unwrap_or(cp.head.head))
}
