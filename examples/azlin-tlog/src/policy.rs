//! The witness rule: which log and which witnesses a verifier trusts, how many must have cosigned, and what
//! "in the log since" means (planning/architecture.md section 2.20: "Nodes, AzWorkspace and the apps install a
//! release only with an inclusion proof under a witnessed checkpoint"; point 3 there: the waiting period
//! of 72 hours, 24 for a security fix, counts from the time an entry is public).
//!
//! A [`Policy`] is DATA: the log's key, the witnesses' keys, a quorum `k` and the slack allowed for a witness
//! whose clock runs ahead. It is compiled into each verifier and rotated through the signed updater chain
//! (brief drift 4, row X-DELTA-W1S45C-4); this crate builds no witness network. It FAILS CLOSED: a policy with
//! no witness, with a quorum of zero or above the number of witnesses, with more than [`MAX_WITNESSES`]
//! witnesses, or whose witnesses are not independent of each other and of the log (a duplicated key counts
//! as two votes) cannot be built (T15). It is the flat `k`-of-`n` form of the group policies of C2SP
//! `tlog-policy` (row X-DELTA-W1S45C-12).
//!
//! [`Policy::check`] takes a note and the current time and returns a [`Witnessed`] checkpoint or a typed
//! error. The checks, in order: the text is a checkpoint (T7); its origin is the policy's (T14); the log's own
//! signature of type `0x01` is present and verifies (T14); every cosignature of a configured witness
//! verifies (a bad one is an error, not a missing vote: T11) and is not later than `now + max_future`
//! (T13); at least `quorum` different witnesses cosigned (T11); signatures of unknown keys are ignored (T5).
//!
//! `logged_since` is the `quorum`-th SMALLEST cosignature time (T12): fewer than `quorum` malicious witnesses
//! cannot move it earlier than the time an honest member of the first `quorum` cosigned, because at least one
//! of the `quorum` smallest times is an honest one, and an honest time is when a witness really saw the
//! checkpoint (row X-DELTA-W1S45C-3). The waiting period of S48 reads it.
//!
//! Seam: first callers are S45-INT-2 (a node's check 8 in `azlin-proto`'s `image.rs`), S45-INT-3
//! (AzWorkspace and the apps' updater) and S48 (the waiting period).

use std::fmt;

use crate::checkpoint::{Checkpoint, CheckpointError};
use crate::names::{KeyName, Origin, SigKind};
use crate::note::{Check, Known, Note, Verifier};
use crate::time::{Span, UnixTime};

/// The most witnesses of a policy (architecture 1.4 rule 4).
pub const MAX_WITNESSES: usize = 16;

/// Why a policy cannot be built, or why a note failed it. Never a string.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PolicyError {
    /// More than [`MAX_WITNESSES`] witnesses.
    TooMany,
    /// A quorum of zero: no cosignature would be needed (fail closed).
    QuorumZero,
    /// A quorum above the number of witnesses: no note could satisfy it.
    QuorumTooHigh,
    /// Two witnesses with the same name or the same public key: one key would count twice.
    DuplicateWitness,
    /// A witness whose public key is the log's: the log's operator would cosign alone.
    WitnessIsLog,
    /// A configured public key is not usable (not on the curve, or of small order).
    BadKey,
    /// A verifier of the wrong signature type: the log's must be type `0x01`, a witness's type `0x04`.
    WrongKind,
    /// The note text is not a checkpoint.
    Checkpoint(CheckpointError),
    /// The checkpoint is of another log.
    OriginMismatch,
    /// The log's own signature is missing.
    LogNotSigned,
    /// A signature of a configured key does not verify; the whole note is refused.
    BadSignature {
        /// The key whose signature failed.
        name: KeyName,
    },
    /// A cosignature is later than `now + max_future`.
    FutureCosignature {
        /// The witness whose cosignature is from the future.
        name: KeyName,
    },
    /// Fewer than `need` different witnesses cosigned.
    QuorumNotMet {
        /// How many different witnesses cosigned with a valid signature.
        have: usize,
        /// The quorum the policy asks for.
        need: usize,
    },
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PolicyError::TooMany => f.write_str("a policy has at most 16 witnesses"),
            PolicyError::QuorumZero => f.write_str("the quorum is zero"),
            PolicyError::QuorumTooHigh => {
                f.write_str("the quorum is above the number of witnesses")
            }
            PolicyError::DuplicateWitness => f.write_str("two witnesses share a name or a key"),
            PolicyError::WitnessIsLog => f.write_str("a witness has the log's key"),
            PolicyError::BadKey => f.write_str("a configured public key is not usable"),
            PolicyError::WrongKind => f.write_str("a verifier has the wrong signature type"),
            PolicyError::Checkpoint(error) => write!(f, "the note is no checkpoint: {error}"),
            PolicyError::OriginMismatch => f.write_str("the checkpoint is of another log"),
            PolicyError::LogNotSigned => f.write_str("the log's signature is missing"),
            PolicyError::BadSignature { name } => {
                write!(f, "the signature of {name} does not verify")
            }
            PolicyError::FutureCosignature { name } => {
                write!(f, "the cosignature of {name} is from the future")
            }
            PolicyError::QuorumNotMet { have, need } => {
                write!(f, "{have} witnesses cosigned, {need} are needed")
            }
        }
    }
}

impl std::error::Error for PolicyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PolicyError::Checkpoint(error) => Some(error),
            _ => None,
        }
    }
}

/// A log a verifier or a witness trusts: its origin and the key that signs its checkpoints (type `0x01`).
/// C2SP `tlog-checkpoint` recommends that the key name is the origin (drift row X-DELTA-W1S45C-9).
///
/// Seam: first callers are the policy data of S45-INT-2 and the witnesses of S45-INT-1.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LogKey {
    /// The log's origin.
    pub origin: Origin,
    /// The key of type [`SigKind::Ed25519`] that signs the log's checkpoints.
    pub verifier: Verifier,
}

/// A checkpoint that passed a [`Policy`]: which witnesses cosigned and since when the checkpoint is public.
/// Made only by [`Policy::check`].
///
/// Seam: first caller is `bundle::verify_bundle`; S48 reads [`Witnessed::logged_since`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Witnessed {
    checkpoint: Checkpoint,
    cosigners: Vec<KeyName>,
    logged_since: UnixTime,
}

impl Witnessed {
    /// The checkpoint the log signed.
    pub fn checkpoint(&self) -> &Checkpoint {
        &self.checkpoint
    }

    /// The witnesses whose valid cosignature the note carries, in the order of the policy.
    pub fn cosigners(&self) -> &[KeyName] {
        &self.cosigners
    }

    /// The `quorum`-th smallest cosignature time (T12).
    pub fn logged_since(&self) -> UnixTime {
        self.logged_since
    }
}

/// The witness rule (see the module documentation). Built by [`Policy::new`]; fields are private so that a
/// policy always satisfies its bounds.
///
/// Seam: first callers are S45-INT-2, S45-INT-3 and the monitor of S45-INT-4.
#[derive(Clone, Debug)]
pub struct Policy {
    log: LogKey,
    log_key: Known,
    witnesses: Vec<Verifier>,
    known: Vec<Known>,
    quorum: u8,
    max_future: Span,
}

impl Policy {
    /// A policy: the log, the witnesses (type `0x04`, at most [`MAX_WITNESSES`], each name and each public
    /// key once, none the log's key), the quorum (`1 <= quorum <= witnesses.len()`) and the slack for a
    /// witness clock that runs ahead (T15).
    pub fn new(
        log: LogKey,
        witnesses: Vec<Verifier>,
        quorum: u8,
        max_future: Span,
    ) -> Result<Policy, PolicyError> {
        if witnesses.len() > MAX_WITNESSES {
            return Err(PolicyError::TooMany);
        }
        if quorum == 0 {
            return Err(PolicyError::QuorumZero);
        }
        if usize::from(quorum) > witnesses.len() {
            return Err(PolicyError::QuorumTooHigh);
        }
        if log.verifier.kind != SigKind::Ed25519 {
            return Err(PolicyError::WrongKind);
        }
        let log_key = Known::new(&log.verifier).map_err(|_| PolicyError::BadKey)?;
        let mut known: Vec<Known> = Vec::new();
        for witness in &witnesses {
            if witness.kind != SigKind::Cosignature {
                return Err(PolicyError::WrongKind);
            }
            let key = Known::new(witness).map_err(|_| PolicyError::BadKey)?;
            if witness.public == log.verifier.public {
                return Err(PolicyError::WitnessIsLog);
            }
            let duplicate = known.iter().any(|other| {
                other.name() == &witness.name || other.verifier().public == witness.public
            });
            if duplicate {
                return Err(PolicyError::DuplicateWitness);
            }
            known.push(key);
        }
        Ok(Policy {
            log,
            log_key,
            witnesses,
            known,
            quorum,
            max_future,
        })
    }

    /// The log this policy trusts.
    pub fn log(&self) -> &LogKey {
        &self.log
    }

    /// The witnesses, in the order they were given.
    pub fn witnesses(&self) -> &[Verifier] {
        &self.witnesses
    }

    /// How many different witnesses must have cosigned.
    pub fn quorum(&self) -> u8 {
        self.quorum
    }

    /// How far ahead of `now` a cosignature time may be.
    pub fn max_future(&self) -> Span {
        self.max_future
    }

    /// Checks `note` against the policy at the time `now` (see the module documentation).
    pub fn check(&self, note: &Note, now: UnixTime) -> Result<Witnessed, PolicyError> {
        let checkpoint = Checkpoint::parse_text(note.text()).map_err(PolicyError::Checkpoint)?;
        if checkpoint.origin() != &self.log.origin {
            return Err(PolicyError::OriginMismatch);
        }
        let line = self.log_key.find(note).ok_or(PolicyError::LogNotSigned)?;
        if self.log_key.check(note.text(), line).is_none() {
            return Err(PolicyError::BadSignature {
                name: self.log.verifier.name.clone(),
            });
        }
        let mut cosigners: Vec<KeyName> = Vec::new();
        let mut times: Vec<UnixTime> = Vec::new();
        for witness in &self.known {
            let Some(line) = witness.find(note) else {
                continue;
            };
            let Some(Check::Cosigned(time)) = witness.check(note.text(), line) else {
                return Err(PolicyError::BadSignature {
                    name: witness.name().clone(),
                });
            };
            let ahead = time.since(now);
            if ahead.is_some_and(|ahead| ahead > self.max_future) {
                return Err(PolicyError::FutureCosignature {
                    name: witness.name().clone(),
                });
            }
            cosigners.push(witness.name().clone());
            times.push(time);
        }
        let need = usize::from(self.quorum);
        if times.len() < need {
            return Err(PolicyError::QuorumNotMet {
                have: times.len(),
                need,
            });
        }
        times.sort_unstable();
        let logged_since = times
            .get(need - 1)
            .copied()
            .ok_or(PolicyError::QuorumNotMet {
                have: times.len(),
                need,
            })?;
        Ok(Witnessed {
            checkpoint,
            cosigners,
            logged_since,
        })
    }
}
