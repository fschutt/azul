//! `restore_as_of`: the only way a head's CONTENT goes back (planning/architecture.md section 5.13 "A
//! head never goes backwards ... The only way back is a restore (`restore_as_of`), which the metadata
//! group announces signed", section 2.17 "What a restore re-anchors", section 6.10 V3 "a restore epoch
//! the metadata group signs", section 5.8 "Retention ... and `restore as of` exists"; build unit S16,
//! lane W1-S16a).
//!
//! THE VERSION NEVER GOES DOWN (DRIFT X-DELTA-W1S16A-1, the brief's drift 1: the plan does not say
//! whether a point-in-time restore moves the number back; the smallest answer is that it does not). A
//! restore writes a NEW record one version above the head, carrying the VALUE the register had at the
//! moment asked for and the restore's epoch, and marks it as a restore (`restored_from`). Only a
//! restore from the off-site copy, which replaces the whole register by an older snapshot, can make a
//! served version lower: the signed restore record excuses that, and the node then calls
//! `restore_as_of` once more on each restored register so that its head carries the record's epoch
//! (a restore "as of" the snapshot's time is a restore to the head itself: it is allowed, and it is
//! what stamps the epoch).
//!
//! THE EPOCH. The restore epoch of a record is the number the metadata group signs into its restore
//! record: one for the whole restore, above every epoch used before (planning/architecture.md
//! section 6.10 V3), so a signed record and a head are comparable. The register refuses an epoch that
//! is not strictly above its own head's, and every later `cas` carries the epoch on. The brief's
//! "`restore_epoch` + 1" is the case where the group's counter and the register's agree.
//!
//! THE WITNESS (DRIFT X-DELTA-W1S16A-13). A register is restored only with a [`RestoreAuth`]: a value that can be made only by
//! [`RestoreAuth::verify`], which asks a [`RestoreRequestVerifier`] (the signature of the operator or
//! of an owner, S9/S43) and returns who asked. It has private fields and no `Clone`, `Default` or
//! `From`.
//!
//! ```compile_fail,E0451
//! // A restore authorization cannot be built by hand.
//! use azlin_seams::{RestoreEpoch, SpaceId, UnixTime};
//! use azlin_sync::{RestoreAuth, Requester};
//! let _forged = RestoreAuth {
//!     space: SpaceId::from_bytes([1; 16]),
//!     as_of: UnixTime::from_secs(1),
//!     epoch: RestoreEpoch::new(1),
//!     requester: Requester::Operator,
//! };
//! ```
//!
//! ```compile_fail,E0277
//! // And it cannot be copied: one verified request, one authorization.
//! fn needs_clone<T: Clone>() {}
//! needs_clone::<azlin_sync::RestoreAuth>();
//! ```
//!
//! ```compile_fail,E0308
//! // A register cannot be restored without the witness.
//! use azlin_seams::{SpaceId, UnixTime};
//! use azlin_sync::HeadRegister;
//! let mut register = HeadRegister::new(SpaceId::from_bytes([1; 16]));
//! let _ = register.restore_as_of(UnixTime::from_secs(1), UnixTime::from_secs(2), ());
//! ```

use azlin_seams::{mutant, DeviceId, RestoreEpoch, SpaceId, Stamp, VerifyError};

use crate::cas::HeadRegister;
use crate::head::{HeadRecord, Writer};
use crate::version::Version;

/// A request to restore a space's head as of a moment, as it arrives: the space, the moment, and the
/// requester's signature. The bytes the signature covers and the keys that may sign are S43's and the
/// registry's: a [`RestoreRequestVerifier`] knows them.
///
/// Implements planning/architecture.md section 5.13 (`restore_as_of`) and section 2.17 (the operator's
/// or an owner's signed request).
///
/// Caller: SYNC-APPLY (the `restore_as_of` verb), S43 (the operator's restore).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreRequest {
    /// The space whose head is to be restored.
    pub space: SpaceId,
    /// The moment to restore to: the register's value at that moment becomes the new head.
    pub as_of: Stamp,
    /// The requester's signature.
    pub sig: Vec<u8>,
}

/// Who asked for a restore.
///
/// Implements planning/architecture.md section 2.17 (the operator's or an owner's signed request).
///
/// Caller: `RestoreRequestVerifier::verify_request`, `RestoreAuth::requester`, `restore_as_of` (the
/// writer of the restore record).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Requester {
    /// The operator, through `ctl` (no device).
    Operator,
    /// An owner of the space's account, through this device.
    Owner(DeviceId),
}

/// The seam that checks a restore request: it knows the keys, builds the signed bytes of the request
/// with the engine's codec and `signing_input`, verifies the signature, checks that the signer is the
/// operator or an owner of the space's account, and says which of the two it was.
///
/// Implements planning/architecture.md section 2.17 and section 5.8 (an owner or the operator asks).
///
/// Seam: S9 (the registry: who is an owner) and S43 (the operator's key); the tests use a verifier
/// over `FakeScheme`.
pub trait RestoreRequestVerifier {
    /// Who asked, when the request is authentic and the asker may restore this space; otherwise why
    /// not.
    fn verify_request(&self, req: &RestoreRequest) -> Result<Requester, VerifyError>;
}

/// The witness of a verified restore request: it proves that the operator or an owner asked for THIS
/// restore of THIS space, and carries the restore epoch the metadata group assigned to it. It has
/// private fields and no `Clone`, `Default` or `From`: the only maker is [`RestoreAuth::verify`].
///
/// Implements planning/architecture.md section 2.17 (a restore is the operator's or an owner's signed
/// request) and section 6.10 V3 (a restore epoch the metadata group signs).
///
/// Caller: `HeadRegister::restore_as_of`; made by SYNC-APPLY / S43 from a request the leader has
/// verified.
#[derive(Debug)]
pub struct RestoreAuth {
    space: SpaceId,
    as_of: Stamp,
    epoch: RestoreEpoch,
    requester: Requester,
}

impl RestoreAuth {
    /// Checks `req` with `verifier` and, when it is authentic, makes the witness for the restore
    /// that carries `epoch` (the epoch the metadata group assigned: above every epoch it used
    /// before). The verifier's refusal is passed on; there is no witness for a request it refused.
    pub fn verify(
        req: &RestoreRequest,
        epoch: RestoreEpoch,
        verifier: &dyn RestoreRequestVerifier,
    ) -> Result<RestoreAuth, VerifyError> {
        let requester = match verifier.verify_request(req) {
            Ok(requester) => requester,
            Err(error) => {
                if mutant!("sync_restore_auth_failure_ignored") {
                    Requester::Operator
                } else {
                    return Err(error);
                }
            }
        };
        Ok(RestoreAuth {
            space: req.space,
            as_of: req.as_of,
            epoch,
            requester,
        })
    }

    /// The space the restore is for.
    pub fn space(&self) -> SpaceId {
        self.space
    }

    /// The moment the restore goes back to.
    pub fn as_of(&self) -> Stamp {
        self.as_of
    }

    /// The restore epoch of this restore.
    pub fn epoch(&self) -> RestoreEpoch {
        self.epoch
    }

    /// Who asked.
    pub fn requester(&self) -> Requester {
        self.requester
    }
}

/// What a restore did: the new head (a restore record), the version whose value it carries, and the
/// version it replaced as the head.
///
/// Implements planning/architecture.md section 2.17 (a restore re-anchors the space heads) and section
/// 5.13 (the metadata group announces the restore signed).
///
/// Caller: `HeadRegister::restore_as_of`; S43 puts the new head into the signed restore record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoredHead {
    /// The new head: a restore record one version above the old head.
    pub record: HeadRecord,
    /// The version whose value the new head carries.
    pub restored_from: Version,
    /// The head that was current before the restore.
    pub replaced: Version,
}

/// Why a restore is refused. A refusal changes nothing.
///
/// Implements planning/architecture.md section 5.13 (`restore_as_of`) and section 2.5 (the statuses:
/// the first two are a bug of the caller, `bad_request`; `Empty`, `BeforeFirstVersion` and
/// `BeyondRetention` are `not_found`; `EpochNotAbove` is `bad_request`; `HistoryFull` is
/// `retry_after`).
///
/// Caller: `HeadRegister::restore_as_of`; SYNC-APPLY maps it to a status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RestoreAsOfRefusal {
    /// The authorization is for another space.
    #[error("the restore authorization is for another space")]
    WrongSpace,
    /// The moment asked for is not the one the authorization was made for.
    #[error("the restore authorization is for another moment")]
    RequestMismatch,
    /// Nobody has written the register: there is nothing to restore.
    #[error("the register is empty")]
    Empty,
    /// The register holds no version at or before that moment, and it never held an older one.
    #[error("the register has no version at or before that moment")]
    BeforeFirstVersion,
    /// The register holds no version at or before that moment because the older ones are pruned.
    #[error("the versions at that moment are past the retention")]
    BeyondRetention,
    /// The epoch of the restore is not above the epoch of the head.
    #[error("the restore epoch {got} is not above the head's epoch {current}")]
    EpochNotAbove {
        /// The epoch of the register's head.
        current: RestoreEpoch,
        /// The epoch the restore carries.
        got: RestoreEpoch,
    },
    /// The register keeps as many versions or bytes as its limits allow.
    #[error("the register is at its limits")]
    HistoryFull,
    /// The register has used every version number.
    #[error("the register has used every version number")]
    VersionExhausted,
}

impl HeadRegister {
    /// Restores the head as of `as_of`: writes a NEW record one version above the head, with the value
    /// the register had at that moment (the newest version stamped at or before `as_of`), the epoch
    /// of the restore, the stamp `at`, and the lock stamp of the head it replaces. A restore to a
    /// moment at or after the head's own stamp restores the head to itself (a new record that carries
    /// the epoch): that is how the node re-stamps a register after the off-site copy replaced it.
    ///
    /// The register must be the authorization's space, `as_of` the authorization's moment, and the
    /// authorization's epoch strictly above the head's. A refusal changes nothing.
    pub fn restore_as_of(
        &mut self,
        as_of: Stamp,
        at: Stamp,
        auth: RestoreAuth,
    ) -> Result<RestoredHead, RestoreAsOfRefusal> {
        if auth.space != self.space && !mutant!("sync_restore_wrong_space_allowed") {
            return Err(RestoreAsOfRefusal::WrongSpace);
        }
        if auth.as_of != as_of && !mutant!("sync_restore_moment_unchecked") {
            return Err(RestoreAsOfRefusal::RequestMismatch);
        }
        let (current_version, current_epoch, retain_until) = match self.get() {
            Some(current) => (current.version, current.restore_epoch, current.retain_until),
            None => return Err(RestoreAsOfRefusal::Empty),
        };
        let found = self
            .records
            .values()
            .rev()
            .find(|record| record.at <= as_of);
        let target = match found {
            Some(target) => target,
            None if mutant!("sync_restore_before_first_allowed") => {
                match self.records.values().next() {
                    Some(oldest) => oldest,
                    None => return Err(RestoreAsOfRefusal::Empty),
                }
            }
            None => {
                let first = self.records.keys().next().copied();
                return Err(if first == Some(Version::FIRST) {
                    RestoreAsOfRefusal::BeforeFirstVersion
                } else {
                    RestoreAsOfRefusal::BeyondRetention
                });
            }
        };
        let (restored_from, value) = (target.version, target.value.clone());
        if auth.epoch <= current_epoch && !mutant!("sync_restore_epoch_not_above_allowed") {
            return Err(RestoreAsOfRefusal::EpochNotAbove {
                current: current_epoch,
                got: auth.epoch,
            });
        }
        let version = if mutant!("sync_restore_version_regresses") {
            restored_from
        } else {
            match current_version.next() {
                Some(next) => next,
                None => return Err(RestoreAsOfRefusal::VersionExhausted),
            }
        };
        let record = HeadRecord {
            version,
            value,
            at,
            writer: match auth.requester {
                Requester::Operator => Writer::Operator,
                Requester::Owner(device) => Writer::Device(device),
            },
            restore_epoch: if mutant!("sync_restore_epoch_not_raised") {
                current_epoch
            } else {
                auth.epoch
            },
            retain_until,
            restored_from: Some(restored_from),
        };
        if !self.has_room_for(&record) {
            return Err(RestoreAsOfRefusal::HistoryFull);
        }
        self.insert(record.clone());
        Ok(RestoredHead {
            record,
            restored_from,
            replaced: current_version,
        })
    }
}
