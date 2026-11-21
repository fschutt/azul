//! The reference witness and its request (C2SP `tlog-witness`, `add-checkpoint`).
//! Implements planning/architecture.md section 2.20: "independent witnesses cosign" the checkpoints.
//!
//! A witness keeps, for every log it is configured with, the latest checkpoint it cosigned (size and root).
//! A log asks it to cosign a new checkpoint with an [`AddRequest`]: the size `old` of the checkpoint the
//! log believes the witness holds, a consistency proof from `old` to the new size and the new signed
//! checkpoint. The request has the wire form of the spec (T20):
//!
//! ```text
//! old <size>\n
//! <base64 hash>\n         zero to 63 lines
//! \n
//! <the checkpoint, as a signed note>
//! ```
//!
//! [`Witness::add_checkpoint`] checks, in the order of the spec, and answers with the cosignature line or
//! one [`WitnessRefusal`] (the spec's HTTP code in brackets):
//!
//! 1. the origin is configured (`UnknownLog`, 404) and the log's signature verifies (`BadLogSignature`, 403);
//! 2. `old` is not above the checkpoint's size (`Shrunk`, 400: a checkpoint smaller than the one the log
//!    names as the witness's is a shrinking log);
//! 3. `old` is the size the witness holds, or zero if it holds nothing (`Conflict { have }`, 409: the log
//!    learns the size and tries again, T16);
//! 4. the same size means the same root (`Fork`, 422: two checkpoints of one size with two roots are
//!    evidence of a split log, T17), and otherwise the consistency proof from the stored root to the new root
//!    verifies; for `old = 0` it is empty (`BadProof`, 422, T18, T19).
//!
//! The witness refuses whatever is not provably an extension of what it holds, so it never cosigns two
//! checkpoints that are inconsistent with each other (`tlog-checkpoint`: "A log or cosigner MUST NOT sign any
//! checkpoint which is inconsistent with any checkpoint it previously signed"). Only then does it update its
//! head and cosign, with the time `now` it is given: a clock is a parameter.
//!
//! THIS WITNESS KEEPS ITS HEADS IN MEMORY. The spec's "The witness MUST persist the new checkpoint before
//! responding" is the job of the unit that runs a witness as a service (the HTTP transport is later): it
//! reads [`Witness::head`] before it answers and restores the state at start. A witness per log operator in
//! the same process (the reference witnesses of the tests, T26) needs no persistence.
//!
//! Seam: first callers are S45-INT-1 (AzCtl's cosignature round, which runs in-process witnesses and
//! talks to remote ones) and the witness service of a later unit.

use std::fmt;

use azlin_merkle::{empty_root, verify_consistency, ConsistencyProof, Hash, TreeSize};

use crate::checkpoint::{Checkpoint, CheckpointError};
use crate::codec::{b64_encode, b64_hash, parse_decimal, NumberError};
use crate::cosign::Cosigner;
use crate::names::{KeyName, Origin, SigKind};
use crate::note::{Known, Note, NoteError, NoteSig, SignError, MAX_NOTE_BYTES};
use crate::policy::LogKey;
use crate::time::UnixTime;

/// The most consistency proof lines of a request (C2SP `tlog-witness`: "The client MUST NOT send more than
/// 63 consistency proof lines"; the brief says 64: drift row X-DELTA-W1S45C-9).
pub const MAX_PROOF_LINES: usize = 63;

/// The most logs a witness is configured with (architecture 1.4 rule 4).
pub const MAX_ORIGINS: usize = 64;

/// The longest request in bytes: `old ` and 20 digits, 63 proof lines of 44 characters, the blank line, a
/// note.
pub const MAX_REQUEST_BYTES: usize = 4 + 20 + 1 + MAX_PROOF_LINES * 45 + 1 + MAX_NOTE_BYTES;

/// Why a request is malformed. Never a string.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RequestError {
    /// The request is longer than [`MAX_REQUEST_BYTES`].
    TooLarge,
    /// More than [`MAX_PROOF_LINES`] proof lines.
    TooMany,
    /// The request is not valid UTF-8.
    NotUtf8,
    /// The first line is not `old <size>`.
    NoOldLine,
    /// The old size is no valid decimal.
    Old(NumberError),
    /// A proof line is not the canonical base64 of 32 bytes.
    ProofLine,
    /// The request ends before the empty line that follows the proof.
    NoBlankLine,
    /// The checkpoint after the empty line is no signed note.
    Note(NoteError),
}

impl fmt::Display for RequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RequestError::TooLarge => f.write_str("the request is too large"),
            RequestError::TooMany => f.write_str("more than 63 proof lines"),
            RequestError::NotUtf8 => f.write_str("the request is not valid UTF-8"),
            RequestError::NoOldLine => {
                f.write_str("the request does not start with an old size line")
            }
            RequestError::Old(error) => write!(f, "the old size is refused: {error}"),
            RequestError::ProofLine => f.write_str("a proof line is not the base64 of 32 bytes"),
            RequestError::NoBlankLine => f.write_str("no empty line after the proof"),
            RequestError::Note(error) => write!(f, "the checkpoint is no signed note: {error}"),
        }
    }
}

impl std::error::Error for RequestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RequestError::Old(error) => Some(error),
            RequestError::Note(error) => Some(error),
            _ => None,
        }
    }
}

/// A request to cosign a checkpoint: the size the log believes the witness holds, the consistency proof from
/// that size, and the checkpoint note the log signed.
///
/// Seam: made by `log::Log::add_request`; first caller outside the crate is S45-INT-1.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AddRequest {
    old: TreeSize,
    proof: Vec<Hash>,
    checkpoint: Note,
}

impl AddRequest {
    /// A request; the proof has at most [`MAX_PROOF_LINES`] hashes.
    pub fn new(
        old: TreeSize,
        proof: Vec<Hash>,
        checkpoint: Note,
    ) -> Result<AddRequest, RequestError> {
        if proof.len() > MAX_PROOF_LINES {
            return Err(RequestError::TooMany);
        }
        Ok(AddRequest {
            old,
            proof,
            checkpoint,
        })
    }

    /// The size of the checkpoint the log believes the witness holds.
    pub fn old(&self) -> TreeSize {
        self.old
    }

    /// The consistency proof from `old` to the checkpoint's size.
    pub fn proof(&self) -> &[Hash] {
        &self.proof
    }

    /// The checkpoint, as the signed note the log made.
    pub fn checkpoint(&self) -> &Note {
        &self.checkpoint
    }

    /// The wire form of the spec (see the module documentation).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = format!("old {}\n", self.old.0);
        for hash in &self.proof {
            out.push_str(&b64_encode(hash.as_bytes()));
            out.push('\n');
        }
        out.push('\n');
        let mut bytes = out.into_bytes();
        bytes.extend_from_slice(&self.checkpoint.to_bytes());
        bytes
    }

    /// Parses the wire form strictly (T20); `parse(to_bytes(r)) == r`.
    pub fn parse(bytes: &[u8]) -> Result<AddRequest, RequestError> {
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(RequestError::TooLarge);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| RequestError::NotUtf8)?;
        let (first, mut rest) = text.split_once('\n').ok_or(RequestError::NoOldLine)?;
        let old = first.strip_prefix("old ").ok_or(RequestError::NoOldLine)?;
        let old = parse_decimal(old).map_err(RequestError::Old)?;
        let mut proof = Vec::new();
        // Every pass consumes one line of the bounded input; more than 63 proof lines end it.
        loop {
            let (line, tail) = rest.split_once('\n').ok_or(RequestError::NoBlankLine)?;
            rest = tail;
            if line.is_empty() {
                break;
            }
            if proof.len() == MAX_PROOF_LINES {
                return Err(RequestError::TooMany);
            }
            proof.push(b64_hash(line).ok_or(RequestError::ProofLine)?);
        }
        let checkpoint = Note::parse(rest.as_bytes()).map_err(RequestError::Note)?;
        AddRequest::new(TreeSize(old), proof, checkpoint)
    }
}

/// Why a witness did not cosign. Never a string; the spec's HTTP code is in the documentation of each case.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum WitnessRefusal {
    /// The note text is no checkpoint (400).
    BadCheckpoint(CheckpointError),
    /// The origin is not one this witness is configured with (404).
    UnknownLog,
    /// No signature of the log's key, or one that does not verify (403).
    BadLogSignature,
    /// The old size is above the checkpoint's size (400).
    Shrunk,
    /// The old size is not the size the witness holds: `have` is (409; the log tries again with `have`).
    Conflict {
        /// The size of the latest checkpoint the witness cosigned for this log (zero if none).
        have: TreeSize,
    },
    /// The consistency proof does not verify, or is not empty where it must be (422).
    BadProof,
    /// The checkpoint has the size the witness holds and another root: the log signed two checkpoints of one
    /// size (422). Evidence of misbehaviour: the witness cosigns nothing.
    Fork,
    /// The witness's own key failed to sign.
    Sign(SignError),
}

impl fmt::Display for WitnessRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WitnessRefusal::BadCheckpoint(error) => write!(f, "the note is no checkpoint: {error}"),
            WitnessRefusal::UnknownLog => f.write_str("the log is unknown to this witness"),
            WitnessRefusal::BadLogSignature => {
                f.write_str("the log's signature is missing or does not verify")
            }
            WitnessRefusal::Shrunk => f.write_str("the old size is above the checkpoint's size"),
            WitnessRefusal::Conflict { have } => write!(f, "the witness holds size {}", have.0),
            WitnessRefusal::BadProof => f.write_str("the consistency proof does not verify"),
            WitnessRefusal::Fork => f.write_str("two checkpoints of one size with two roots"),
            WitnessRefusal::Sign(error) => write!(f, "the witness could not sign: {error}"),
        }
    }
}

impl std::error::Error for WitnessRefusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WitnessRefusal::BadCheckpoint(error) => Some(error),
            WitnessRefusal::Sign(error) => Some(error),
            _ => None,
        }
    }
}

/// Why a witness cannot be built. Never a string.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WitnessError {
    /// More than [`MAX_ORIGINS`] logs.
    TooMany,
    /// Two logs with the same origin.
    DuplicateOrigin,
    /// A log key that is not of type `0x01`.
    WrongKind,
    /// A log key that is not a usable public key.
    BadKey,
}

impl fmt::Display for WitnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            WitnessError::TooMany => "a witness is configured with at most 64 logs",
            WitnessError::DuplicateOrigin => "two logs have the same origin",
            WitnessError::WrongKind => "a log key is not an Ed25519 log key",
            WitnessError::BadKey => "a log key is not a usable public key",
        })
    }
}

impl std::error::Error for WitnessError {}

/// What a witness holds of one log: the key it trusts and the latest checkpoint it cosigned.
struct Held {
    origin: Origin,
    key: Known,
    size: TreeSize,
    root: Hash,
}

/// A witness: a cosigning key, the logs it is configured with and, per log, the latest checkpoint it
/// cosigned. Not `Clone`, no `Default`, no `From`: the head is state that only [`Witness::add_checkpoint`]
/// moves forward.
///
/// Seam: first callers are S45-INT-1 and the witness service of a later unit.
pub struct Witness {
    cosigner: Box<dyn Cosigner>,
    logs: Vec<Held>,
}

impl fmt::Debug for Witness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Witness")
            .field("cosigner", self.cosigner.name())
            .field("logs", &self.logs.len())
            .finish_non_exhaustive()
    }
}

impl Witness {
    /// A witness for the logs `logs` (at most [`MAX_ORIGINS`], each origin once, each key of type `0x01`),
    /// holding nothing yet: for a log it has never cosigned the head is size zero and the root of the empty
    /// tree. (The brief's `new` returns the witness itself: the limit needs a refusal, drift row
    /// X-DELTA-W1S45C-11.)
    pub fn new(cosigner: Box<dyn Cosigner>, logs: Vec<LogKey>) -> Result<Witness, WitnessError> {
        if logs.len() > MAX_ORIGINS {
            return Err(WitnessError::TooMany);
        }
        let mut held: Vec<Held> = Vec::new();
        for log in logs {
            if log.verifier.kind != SigKind::Ed25519 {
                return Err(WitnessError::WrongKind);
            }
            let key = Known::new(&log.verifier).map_err(|_| WitnessError::BadKey)?;
            if held.iter().any(|other| other.origin == log.origin) {
                return Err(WitnessError::DuplicateOrigin);
            }
            held.push(Held {
                origin: log.origin,
                key,
                size: TreeSize(0),
                root: empty_root(),
            });
        }
        Ok(Witness {
            cosigner,
            logs: held,
        })
    }

    /// The witness's own name.
    pub fn name(&self) -> &KeyName {
        self.cosigner.name()
    }

    /// The latest checkpoint this witness cosigned for `origin` (size and root); `None` when the origin is
    /// not configured. A service persists this before it answers. Seam: first caller is the witness service
    /// of a later unit (drift row X-DELTA-W1S45C-18).
    pub fn head(&self, origin: &Origin) -> Option<(TreeSize, Hash)> {
        self.logs
            .iter()
            .find(|held| &held.origin == origin)
            .map(|held| (held.size, held.root))
    }

    /// Checks the request and, if every check passes, moves the head forward and returns the cosignature
    /// line for the checkpoint, signed with the time `now` (see the module documentation).
    pub fn add_checkpoint(
        &mut self,
        req: &AddRequest,
        now: UnixTime,
    ) -> Result<NoteSig, WitnessRefusal> {
        let note = req.checkpoint();
        let checkpoint =
            Checkpoint::parse_text(note.text()).map_err(WitnessRefusal::BadCheckpoint)?;
        let held = self
            .logs
            .iter_mut()
            .find(|held| &held.origin == checkpoint.origin())
            .ok_or(WitnessRefusal::UnknownLog)?;
        let line = held.key.find(note).ok_or(WitnessRefusal::BadLogSignature)?;
        if held.key.check(note.text(), line).is_none() {
            return Err(WitnessRefusal::BadLogSignature);
        }
        if req.old() > checkpoint.size() {
            return Err(WitnessRefusal::Shrunk);
        }
        if req.old() != held.size {
            return Err(WitnessRefusal::Conflict { have: held.size });
        }
        if checkpoint.size() == held.size && checkpoint.root() != held.root {
            return Err(WitnessRefusal::Fork);
        }
        let proof = ConsistencyProof {
            old: req.old(),
            new: checkpoint.size(),
            path: req.proof().to_vec(),
        };
        verify_consistency(&proof, &held.root, &checkpoint.root())
            .map_err(|_| WitnessRefusal::BadProof)?;
        let cosignature = self
            .cosigner
            .cosign(note.text(), now)
            .map_err(WitnessRefusal::Sign)?;
        held.size = checkpoint.size();
        held.root = checkpoint.root();
        Ok(cosignature)
    }
}
