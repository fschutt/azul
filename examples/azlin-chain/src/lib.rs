//! Azlin chain: the generic signed, hash-chained log machinery of planning/architecture.md section 5.9
//! "The team log and the space chains", shared by the team log (S17c) and every space chain (S16b),
//! and the device's memory of heads (section 5.13 "A head never goes backwards", section 2.17 "What a
//! restore re-anchors", section 6.10 V3 "a restore epoch the metadata group signs"); build unit S17,
//! lane W1-S17a.
//!
//! No I/O, no transport, no database, no clock: every function takes the leader's stamp (`Stamp`) or
//! the facts it needs, and every signature check is a seam ([`HeadVerifier`], [`RestoreSigVerifier`],
//! `azlin_seams::SignatureVerifier`). The byte-exact encoding of an entry and the preimage of its
//! signature belong to azlin-cbor and azlin-contract; a later glue unit (CHAIN-GLUE) implements
//! [`ChainEntry`] for the real entries and runs the golden vectors.
//!
//! | what | module | the plan |
//! |---|---|---|
//! | [`ChainId`], [`ChainEntry`], [`Head`], [`SignedHead`], [`HeadVerifier`], [`Alarm`] | [`entry`] | 5.9, 5.13 |
//! | link checks: [`Tip`], [`Chain`], [`ChainRefusal`], [`RefusalClass`], [`verify_from_genesis`] | [`link`] | 5.9 |
//! | the head a device pins and never lets go backwards: [`HeadPin`], [`PinVerdict`], [`HeadRefusal`] | [`pin`] | 5.9, 5.13, 6.10 V3 |
//! | heads exchanged between devices: [`compare_peer`], [`PeerVerdict`] | [`peers`] | 5.9 |
//! | checkpoints: [`checkpoint_due`], [`Checkpoint`], [`verify_from_checkpoint`] | [`checkpoint`] | 5.9 |
//! | a device's re-anchoring after an announced restore: [`reanchor`], [`RestoreTrust`] | [`restore`] | 2.17, 5.13, 12 E4 |
//!
//! The rules, in one place: an entry is the NEXT one only if it belongs to the chain, its copy is
//! intact, its position is the head's plus one, its `prev` is the head's hash, its stamp does not
//! run backwards and its restore epoch is not below the chain's; the first entry is a genesis
//! (position 0, zero `prev`); the same position with another hash is a FORK (refused and alarmed),
//! the same entry again changes nothing; a corrupt copy is told apart from a fork (it is fetched
//! again from another voter). A device's pin never goes backwards except through a restore record
//! signed by the map key (strict trust, E4), whose epoch is above the one the device accepted.
//!
//! THE RESTORE EPOCH (the coordinator's decision on X-DELTA-W1S17A-3, 2026-11-14): a restore moves
//! positions back, so a position alone cannot tell the lost timeline from the restored one. Every
//! signed head and every entry carries the [`RestoreEpoch`] it was written in (covered by its
//! signature: the preimage the glue builds includes it), the pin's FLOOR is the newest restore epoch
//! the device accepted, and a head or an entry below the floor is refused and alarmed
//! ([`HeadRefusal::BelowRestoreEpoch`], [`ChainRefusal::BelowRestoreEpoch`]): the replay of the lost
//! timeline is no way back. A record that arrives late is not applied to a chain whose pinned
//! signed head was signed after the record. S5's signed-head format and the wording of 2.17, 5.13 and
//! E4 follow this decision (drift row X-DELTA-W1S17A-3, OPEN as the owner's fold-in item).
//!
//! Mutation guards (`azlin_seams::mutant!`, the feature `azlin-seams/mutants`) sit at each of these
//! decisions; the mutant hold of this crate turns each one off and watches a test fail.

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing
    )
)]
#![warn(missing_docs)]

pub mod checkpoint;
pub mod entry;
pub mod link;
pub mod peers;
pub mod pin;
pub mod restore;

// The restore epoch (defined once, in `azlin-seams`): re-exported because it is part of the signed
// head and of every entry.
pub use azlin_seams::RestoreEpoch;

pub use checkpoint::{
    checkpoint_due, verify_from_checkpoint, Checkpoint, CHECKPOINT_AGE, CHECKPOINT_ENTRIES,
};
pub use entry::{
    require_signed, Alarm, ChainEntry, ChainId, ChainKind, Head, HeadVerifier, SignedHead,
};
pub use link::{verify_from_genesis, Admitted, Chain, ChainRefusal, Placement, RefusalClass, Tip};
pub use peers::{compare_peer, PeerVerdict};
pub use pin::{HeadPin, HeadRefusal, PinVerdict, MAX_PINNED_CHAINS};
pub use restore::{
    reanchor, rollback_alarm, verify_record, GroupAloneTrust, Kept, Moved, Reanchored, ResendRange,
    RestoreRecord, RestoreRefusal, RestoreSig, RestoreSigVerifier, RestoreSigner, RestoreTrust,
    StrictTrust, VerifiedRestore,
};
