//! A device's re-anchoring after an announced restore (planning/architecture.md section 2.17 "What
//! a restore re-anchors": "the metadata group signs a restore record that lists, per account, the
//! positions that survived ... Devices re-anchor every one of them and send again whatever they
//! hold beyond them"; section 5.13 "A head never goes backwards ... The only way back is a restore";
//! section 6.10 S15 and V3 "a restore epoch the metadata group signs"; section 12 E4 "devices
//! accept going back only through a restore signed by the map key"; build unit S17, lane
//! W1-S17a).
//!
//! The record lists the chain positions that survived: [`RestoreRecord`]. A device follows it only
//! when a [`RestoreTrust`] says its signatures are the right ones, and only when its restore epoch
//! is above the epoch the device has accepted (a replayed old record is refused). What happens then
//! is [`reanchor`]:
//!
//! - the pins the record lists are SET to the record's positions (the only way a pin goes back);
//! - the pin's FLOOR (its restore epoch) is raised to the record's epoch, so that the heads and
//!   entries of the lost timeline, signed before the restore in an older epoch, can no longer move a
//!   re-anchored chain forward again (X-DELTA-W1S17A-3, the coordinator's decision: a position alone
//!   cannot tell the restored timeline from the lost one, the epoch can);
//! - a chain whose pinned signed head was signed AFTER the record (`signed_at` above the record's
//!   `at`) is KEPT, not re-anchored: the device already follows the restored timeline there, and a
//!   record that arrives late must not take it back behind progress made after the restore
//!   ([`Reanchored::kept`]);
//! - the device learns which positions it holds beyond the survivors and must send again
//!   ([`Reanchored::resend_ranges`]);
//! - a chain the record does not list is never touched.
//!
//! WHO SIGNS A RESTORE is DRIFT 1 of the brief (X-D-53, answered provisionally by ARCH29 9.13): 2.17,
//! 5.13 and V3 say the metadata group signs it; E4 says the map key signs it and tests "an
//! announcement signed by the group alone" as the attack. Both readings are built behind
//! [`RestoreTrust`]: [`StrictTrust`] requires the map key's signature (the default: E4 is the test of
//! record), [`GroupAloneTrust`] accepts the group's. Which keys sign, and the bytes the signatures
//! cover (S43's record format), are the verifier's: a [`RestoreSigVerifier`] builds the preimage
//! with the engine's codec and `signing_input` and checks the signature with the key it knows.
//!
//! [`VerifiedRestore`] is the witness of a verified record: it has private fields and no `Clone`,
//! `Default` or `From`, so the only way to hold one is [`verify_record`] (or [`reanchor`], which
//! calls it). S16a's head guard takes one and nothing else.
//!
//! ```compile_fail
//! // A verified restore cannot be built by hand.
//! use azlin_chain::VerifiedRestore;
//! let _forged = VerifiedRestore {
//!     epoch: azlin_seams::RestoreEpoch::new(1),
//!     at: azlin_seams::UnixTime::from_secs(1),
//!     positions: Vec::new(),
//! };
//! ```
//!
//! ```compile_fail
//! // And it cannot be copied: one verification, one witness.
//! fn needs_clone<T: Clone>() {}
//! needs_clone::<azlin_chain::VerifiedRestore>();
//! ```

use std::collections::BTreeSet;

use azlin_seams::{mutant, ChainSeq, KeyId, RestoreEpoch, SigAlg, Stamp, VerifyError};

use crate::entry::{Alarm, ChainId, Head};
use crate::link::RefusalClass;
use crate::pin::{HeadPin, MAX_PINNED_CHAINS};

/// Which key signed a part of a restore record.
///
/// Implements planning/architecture.md section 12 E4 (the map key) and section 2.17 (the metadata
/// group): DRIFT 1 of the brief.
///
/// Caller: `RestoreSig`, `StrictTrust`, `GroupAloneTrust`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RestoreSigner {
    /// The metadata group's own key (the credential key, certified by the identity key).
    Group,
    /// The map key: the operator's Touch ID, as for `azctl meta recover` (planning/architecture.md
    /// section 2.4).
    MapKey,
}

/// One signature of a restore record.
///
/// Implements planning/architecture.md section 2.17 (the metadata group signs a restore record) and
/// section 12 E4 (the map key signs it).
///
/// Caller: `RestoreRecord`, `RestoreSigVerifier`; S43's record format (a later unit).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreSig {
    /// Which key claims to have signed.
    pub by: RestoreSigner,
    /// The id of that key.
    pub key: KeyId,
    /// The algorithm of the signature.
    pub alg: SigAlg,
    /// The signature bytes.
    pub sig: Vec<u8>,
}

/// A restore record at the level of the decision: its epoch, its stamp, the chain positions that
/// survived the restore, and its signatures. The byte-exact signed format is S43's.
///
/// `epoch` is the restore epoch the restore starts: one above the last, counted from 1. `at` is the
/// leader's stamp of the record: a pinned head signed after it belongs to the restored timeline.
///
/// Implements planning/architecture.md section 2.17 (the positions that survived, per account) and
/// section 6.10 V3 (a restore epoch the metadata group signs).
///
/// Caller: the device's restore handler (CHAIN-GLUE), S47c's announcement builders, `reanchor`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreRecord {
    /// The restore epoch the record announces.
    pub epoch: RestoreEpoch,
    /// The leader's stamp of the record.
    pub at: Stamp,
    /// The chain positions that survived: team-log and space-chain heads.
    pub positions: Vec<Head>,
    /// The signatures.
    pub signatures: Vec<RestoreSig>,
}

/// Why a restore record is not followed.
///
/// Implements planning/architecture.md section 12 E4 (an announcement signed by the group alone, a
/// replayed old one, one older than a device's newest head are refused) and section 6.10 S15.
///
/// Caller: `verify_record`, `reanchor`, the `RestoreTrust` implementations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum RestoreRefusal {
    /// The record carries no signature at all.
    #[error("the restore record carries no signature")]
    Unsigned,
    /// The record has signatures but none by the map key (the strict trust: E4).
    #[error("the restore record is not signed by the map key")]
    MapKeySignatureMissing,
    /// A signature the trust requires does not verify (or cannot be checked).
    #[error("a signature of the restore record does not verify: {0}")]
    BadSignature(VerifyError),
    /// The record's epoch is not above the restore epoch the device has accepted: a replay, or the
    /// record is already applied.
    #[error("the restore epoch {record} is not above the accepted epoch {pinned}")]
    Replayed {
        /// The record's epoch.
        record: RestoreEpoch,
        /// The epoch the device has accepted.
        pinned: RestoreEpoch,
    },
    /// The record lists the same chain twice.
    #[error("the restore record lists the chain {0} twice")]
    DuplicatePosition(ChainId),
    /// The record lists more chains than a device follows.
    #[error("the restore record lists more chains than a device follows")]
    TooManyPositions,
}

impl RestoreRefusal {
    /// What to do about the refusal: a record that lacks the authority (no signature, no map key,
    /// a bad signature) or is older than what the device accepted is an attempt to move devices
    /// back and is an `Alarm`; the same epoch again (a record already applied) and the malformed
    /// ones are `Refuse`.
    pub fn class(&self) -> RefusalClass {
        match *self {
            RestoreRefusal::Unsigned
            | RestoreRefusal::MapKeySignatureMissing
            | RestoreRefusal::BadSignature(_) => RefusalClass::Alarm,
            RestoreRefusal::Replayed { record, pinned } if record < pinned => RefusalClass::Alarm,
            RestoreRefusal::Replayed { .. }
            | RestoreRefusal::DuplicatePosition(_)
            | RestoreRefusal::TooManyPositions => RefusalClass::Refuse,
        }
    }
}

/// The first position of `rec` that is older than what `pin` holds: the rollback an untrusted or
/// replayed record would have caused, as an alarm. `None` when the record moves no pinned chain
/// back.
///
/// Implements planning/architecture.md section 5.13 (a head never goes backwards) and section 6.10
/// S15.
///
/// Caller: the device's restore handler, when `reanchor` refuses a record whose class is `Alarm`.
pub fn rollback_alarm(pin: &HeadPin, rec: &RestoreRecord) -> Option<Alarm> {
    rec.positions.iter().find_map(|head| {
        let pinned = pin.seq(&head.chain)?;
        (head.seq < pinned).then_some(Alarm::Rollback {
            chain: head.chain,
            pinned,
            offered: head.seq,
        })
    })
}

/// The seam that checks one signature of a restore record: it knows the keys, builds the preimage
/// of the record with the engine's codec, and verifies `sig`.
///
/// Implements planning/architecture.md section 2.17 and section 12 E4.
///
/// Seam: S43's restore record and CHAIN-GLUE (over the map anchors and
/// `azlin_seams::SignatureVerifier`); the tests use a verifier over `FakeScheme`.
pub trait RestoreSigVerifier {
    /// `Ok(())` when `sig` is a valid signature of `rec` by the key it names, in the role it
    /// claims.
    fn verify_signature(&self, rec: &RestoreRecord, sig: &RestoreSig) -> Result<(), VerifyError>;
}

/// Which signatures make a restore record one a device follows (DRIFT 1 of the brief).
///
/// Implements planning/architecture.md section 12 E4 and section 2.17.
///
/// Caller: `verify_record`, `reanchor`; implemented by `StrictTrust` (the default) and
/// `GroupAloneTrust`.
pub trait RestoreTrust {
    /// `Ok(())` when the record's signatures are enough; otherwise why not.
    fn check(&self, rec: &RestoreRecord) -> Result<(), RestoreRefusal>;
}

/// The STRICT trust, the default: a record is followed only when a signature by the MAP KEY
/// verifies (planning/architecture.md section 12 E4: "devices accept going back only through a
/// restore signed by the map key"). A signature by the group alone is `MapKeySignatureMissing`.
///
/// Implements planning/architecture.md section 12 E4.
///
/// Caller: the production wiring of the device's restore handler (CHAIN-GLUE, S43).
#[derive(Clone, Copy)]
pub struct StrictTrust<'a> {
    verifier: &'a dyn RestoreSigVerifier,
}

impl<'a> StrictTrust<'a> {
    /// The strict trust over a signature verifier.
    pub fn new(verifier: &'a dyn RestoreSigVerifier) -> StrictTrust<'a> {
        StrictTrust { verifier }
    }
}

impl RestoreTrust for StrictTrust<'_> {
    fn check(&self, rec: &RestoreRecord) -> Result<(), RestoreRefusal> {
        if rec.signatures.is_empty() {
            return Err(RestoreRefusal::Unsigned);
        }
        let mut failure = None;
        for sig in &rec.signatures {
            if sig.by != RestoreSigner::MapKey && !mutant!("chain_restore_group_alone_accepted") {
                continue;
            }
            match self.verifier.verify_signature(rec, sig) {
                Ok(()) => return Ok(()),
                Err(error) => failure = Some(error),
            }
        }
        match failure {
            Some(error) => Err(RestoreRefusal::BadSignature(error)),
            None => Err(RestoreRefusal::MapKeySignatureMissing),
        }
    }
}

/// The other reading of the plan (sections 2.17, 5.13 and V3: "the metadata group signs a restore
/// record"): a record is followed when ANY signature, the group's or the map key's, verifies. It is
/// NOT the default and has no production caller until the owner decides DRIFT 1 (X-D-53).
///
/// Implements planning/architecture.md section 2.17 (the metadata group signs a restore record).
///
/// Seam: the owner's decision on X-D-53; S47c tests both variants.
#[derive(Clone, Copy)]
pub struct GroupAloneTrust<'a> {
    verifier: &'a dyn RestoreSigVerifier,
}

impl<'a> GroupAloneTrust<'a> {
    /// The group-alone trust over a signature verifier.
    pub fn new(verifier: &'a dyn RestoreSigVerifier) -> GroupAloneTrust<'a> {
        GroupAloneTrust { verifier }
    }
}

impl RestoreTrust for GroupAloneTrust<'_> {
    fn check(&self, rec: &RestoreRecord) -> Result<(), RestoreRefusal> {
        if rec.signatures.is_empty() {
            return Err(RestoreRefusal::Unsigned);
        }
        let mut failure = None;
        for sig in &rec.signatures {
            match self.verifier.verify_signature(rec, sig) {
                Ok(()) => return Ok(()),
                Err(error) => failure = Some(error),
            }
        }
        match failure {
            Some(error) => Err(RestoreRefusal::BadSignature(error)),
            None => Err(RestoreRefusal::Unsigned),
        }
    }
}

/// A restore record whose signatures a `RestoreTrust` accepted, and whose structure is sound (no
/// chain twice, no more chains than a device follows): the witness S16a's head guard takes. It has
/// private fields and no `Clone`, `Default` or `From`: the only maker is `verify_record`.
///
/// Implements planning/architecture.md section 2.17 and section 6.10 V3 (a restore epoch the
/// metadata group signs; an unannounced older head is still refused).
///
/// Caller: S16a (`judge`, `baseline`), `reanchor`.
#[derive(Debug)]
pub struct VerifiedRestore {
    epoch: RestoreEpoch,
    at: Stamp,
    positions: Vec<Head>,
}

impl VerifiedRestore {
    /// The restore epoch the record announced.
    pub fn epoch(&self) -> RestoreEpoch {
        self.epoch
    }

    /// The leader's stamp of the record.
    pub fn at(&self) -> Stamp {
        self.at
    }

    /// The positions that survived, as listed.
    pub fn positions(&self) -> &[Head] {
        &self.positions
    }

    /// The surviving position of `chain`; `None` when the record does not list it.
    pub fn position(&self, chain: &ChainId) -> Option<&Head> {
        self.positions.iter().find(|head| head.chain == *chain)
    }

    /// Whether the record lists `chain`: a restore that covers it.
    pub fn covers(&self, chain: &ChainId) -> bool {
        self.position(chain).is_some()
    }

    /// Re-anchors `pin` to this record: every listed chain is SET to its surviving position (the
    /// only way a pin goes back) unless its pinned signed head was signed after the record (then it
    /// is kept, see [`Reanchored::kept`]); the pin's floor becomes the record's epoch; and the
    /// returned [`Reanchored`] says which positions the device holds beyond the survivors. A record
    /// whose epoch is not above the pin's is `Replayed`; a refusal leaves the pin as it was.
    pub fn apply(&self, pin: &mut HeadPin) -> Result<Reanchored, RestoreRefusal> {
        let accepted = pin.restore_epoch();
        if self.epoch <= accepted && !mutant!("chain_restore_replay_check_off") {
            return Err(RestoreRefusal::Replayed {
                record: self.epoch,
                pinned: accepted,
            });
        }
        let fresh = self
            .positions
            .iter()
            .filter(|head| pin.head(&head.chain).is_none())
            .count();
        if pin.len().saturating_add(fresh) > pin.limit() {
            return Err(RestoreRefusal::TooManyPositions);
        }
        let mut moved = Vec::with_capacity(self.positions.len());
        let mut kept = Vec::new();
        for head in &self.positions {
            // A pin whose signed head was signed AFTER this record already follows the restored
            // timeline: a record that arrives late does not take it back behind that progress.
            if let Some(signed) = pin.signed_head(&head.chain) {
                if signed.signed_at > self.at && !mutant!("chain_restore_late_record_check_off") {
                    kept.push(Kept {
                        chain: head.chain,
                        pinned: signed.head,
                        listed: *head,
                    });
                    continue;
                }
            }
            moved.push(Moved {
                chain: head.chain,
                before: pin.head(&head.chain),
                after: *head,
            });
            pin.anchor(*head);
        }
        pin.set_restore_epoch(self.epoch);
        Ok(Reanchored {
            epoch: self.epoch,
            moved,
            kept,
        })
    }
}

/// Verifies a restore record: its structure, then its signatures through `trust`. Nothing is
/// changed; the witness it returns is what `VerifiedRestore::apply` and S16a's guard take.
///
/// Implements planning/architecture.md section 12 E4 and section 2.17.
///
/// Caller: `reanchor`; S16a's guard (CHAIN-GLUE) when it judges a drive against a notice.
pub fn verify_record(
    rec: &RestoreRecord,
    trust: &dyn RestoreTrust,
) -> Result<VerifiedRestore, RestoreRefusal> {
    if rec.positions.len() > MAX_PINNED_CHAINS {
        return Err(RestoreRefusal::TooManyPositions);
    }
    let mut seen = BTreeSet::new();
    for head in &rec.positions {
        if !seen.insert(head.chain) {
            return Err(RestoreRefusal::DuplicatePosition(head.chain));
        }
    }
    if !mutant!("chain_restore_trust_skipped") {
        trust.check(rec)?;
    }
    Ok(VerifiedRestore {
        epoch: rec.epoch,
        at: rec.at,
        positions: rec.positions.clone(),
    })
}

/// One chain that a restore record moved.
///
/// Implements planning/architecture.md section 2.17 (devices re-anchor every position and send
/// again whatever they hold beyond them).
///
/// Caller: `Reanchored`, the device's restore handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Moved {
    /// The chain.
    pub chain: ChainId,
    /// The head the device had pinned before; `None` if it pinned nothing of the chain.
    pub before: Option<Head>,
    /// The surviving position the pin was set to.
    pub after: Head,
}

/// One chain that a restore record lists and that was NOT re-anchored: its pinned signed head was
/// signed after the record, so the device already follows the restored timeline there.
///
/// Implements planning/architecture.md section 5.13 (a head never goes backwards) and section 6.10
/// V3 (a restore epoch the metadata group signs).
///
/// Caller: `Reanchored::kept`, the device's restore handler (it keeps the chain's local state).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Kept {
    /// The chain.
    pub chain: ChainId,
    /// The head the device has pinned, which was signed after the record.
    pub pinned: Head,
    /// The surviving position the record lists for the chain.
    pub listed: Head,
}

/// The positions of a chain a device holds beyond the survivors and must send again (V3: chat never
/// reuses a sequence number and no fork appears).
///
/// Implements planning/architecture.md section 2.17 and section 6.10 V3.
///
/// Caller: `Moved::resend`, `Reanchored::resend_ranges`; the device's resend queue (CHAIN-GLUE).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResendRange {
    /// The chain.
    pub chain: ChainId,
    /// The first position to send again: the one after the survivor.
    pub from: ChainSeq,
    /// The last position the device had pinned. What it actually holds of this range is the
    /// caller's to look up.
    pub through: ChainSeq,
}

impl Moved {
    /// The range the device must send again: the positions after the survivor up to what it had
    /// pinned. `None` when the device was not ahead of the survivor.
    pub fn resend(&self) -> Option<ResendRange> {
        if mutant!("chain_restore_resend_off") {
            return None;
        }
        let before = self.before?;
        if before.seq <= self.after.seq {
            return None;
        }
        Some(ResendRange {
            chain: self.chain,
            from: self.after.seq.next()?,
            through: before.seq,
        })
    }
}

/// What a re-anchoring did.
///
/// Implements planning/architecture.md section 2.17 (devices re-anchor every position and send
/// again whatever they hold beyond them) and section 6.10 V3.
///
/// Caller: `VerifiedRestore::apply`, `reanchor`; the device's restore handler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reanchored {
    /// The restore epoch the pin's floor now stands at.
    pub epoch: RestoreEpoch,
    /// The chains that were set, in the order of the record.
    pub moved: Vec<Moved>,
    /// The chains the record lists that were not set because their pinned signed head is newer
    /// than the record, in the order of the record.
    pub kept: Vec<Kept>,
}

impl Reanchored {
    /// The ranges to send again, one per chain the device was ahead on.
    pub fn resend_ranges(&self) -> Vec<ResendRange> {
        self.moved.iter().filter_map(Moved::resend).collect()
    }
}

/// Follows a restore record: verifies it (`verify_record`), then re-anchors `pin` (`apply`): the
/// listed pins are set to the survivors, the floor (the pin's restore epoch) is raised to the
/// record's epoch, a chain whose pinned head is newer than the record is kept. The witness and the
/// account of what moved; on any refusal the pin is unchanged.
///
/// After it the device raises the floor of every chain object it keeps (`Tip::raise_floor`,
/// `Chain::raise_floor`) and rebuilds the ones it moved at the survivor (`Chain::resume`), so that
/// the entries of the lost timeline are refused there too.
///
/// Implements planning/architecture.md section 2.17, section 5.13, section 6.10 V3 and section 12
/// E4.
///
/// Caller: the device's restore handler (CHAIN-GLUE, S43), S47c's E4 story.
pub fn reanchor(
    pin: &mut HeadPin,
    rec: &RestoreRecord,
    trust: &dyn RestoreTrust,
) -> Result<(VerifiedRestore, Reanchored), RestoreRefusal> {
    let verified = verify_record(rec, trust)?;
    let reanchored = verified.apply(pin)?;
    Ok((verified, reanchored))
}
