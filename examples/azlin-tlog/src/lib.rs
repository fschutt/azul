//! The Azlin transparency log: C2SP signed notes and checkpoints, witness cosignatures and the witness
//! rule, the release record, proof bundles and a monitor (planning/architecture.md section 2.20: "every signed
//! release ... is appended to Azlin's public log, a Merkle tree whose checkpoints independent witnesses
//! cosign"; "Nodes, AzWorkspace and the apps install a release only with an inclusion proof under a
//! witnessed checkpoint"; section 3.9: "Logging one account's addresses would take a new signed release,
//! which the transparency log shows to everyone"; build unit S45, lane W1-S45c).
//!
//! A pure, strict library, used by the OPERATOR side (append a release, sign a checkpoint, collect witness
//! cosignatures, make the proof bundle that travels with the release) and by every VERIFIER (a node's check
//! 8, AzWorkspace, the apps' updater, the browser page). No I/O, no clock, no randomness: every time is a
//! parameter ([`UnixTime`]), Ed25519 signatures are deterministic, and the stores are traits
//! (architecture 1.4 rules 1 and 3). The Merkle tree is [`azlin_merkle`]'s. The formats are those of the
//! C2SP texts `signed-note`, `tlog-checkpoint`, `tlog-cosignature` and `tlog-witness` as published today (the
//! lane read them; where they differ from the brief the spec won: the drift register
//! `delta/W1S45C.tsv`, rows X-DELTA-W1S45C-9 and -10).
//!
//! The loop, each step a module:
//!
//! | step | module | what |
//! |---|---|---|
//! | release record | [`ReleaseRecord`] | the leaf: kind, name, version, arch, digest, signer, `security_fix` |
//! | append | [`Log`] | a Merkle tree plus the records; the same release twice with other bytes is an equivocation |
//! | checkpoint | [`Checkpoint`], [`sign_checkpoint`], [`NoteSigner`] | the log's signature over origin, size and root |
//! | witness | [`Witness`], [`AddRequest`], [`Cosigner`] | consistency proof in, a timestamped cosignature out |
//! | the rule | [`Policy`] | log key, witnesses, quorum `k`: `logged_since` is the `k`-th smallest cosignature time |
//! | bundle | [`ProofBundle`], [`verify_bundle`] | record, inclusion proof and the cosigned checkpoint; every link checked |
//! | monitor | [`Monitor`] | forks, shrinking, bad consistency, and the digests the log holds |
//!
//! ```
//! use azlin_tlog::*;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // The operator: a log with one release, signed by the log key.
//! let origin = Origin::azlin();
//! let log_key = MemoryKey::from_seed(KeyName::new("azlin.io/release-log/v1")?, [1; 32]);
//! let mut log = Log::open(origin.clone(), MemStore::new(), MemRecords::new())?;
//! let record = ReleaseRecord {
//!     kind: SubjectKind::DesktopApp,
//!     name: Name::new("azdrive")?,
//!     version: Version::new("0.3.0")?,
//!     arch: Arch::Any,
//!     digest: Digest::of(b"the signed release statement"),
//!     signer: SignerId::of_public_key(&[7; 32]),
//!     security_fix: false,
//! };
//! let index = log.append(&record)?;
//! let note = log.checkpoint(&log_key, vec![])?;
//!
//! // Two independent witnesses cosign it; the log asks with a consistency proof from what they hold (nothing).
//! let log_vkey = LogKey { origin, verifier: log_key.verifier(SigKind::Ed25519) };
//! let w1 = MemoryKey::from_seed(KeyName::new("witness.example/w1")?, [2; 32]);
//! let w2 = MemoryKey::from_seed(KeyName::new("witness.example/w2")?, [3; 32]);
//! let witnesses = vec![w1.verifier(SigKind::Cosignature), w2.verifier(SigKind::Cosignature)];
//! let mut first = Witness::new(Box::new(w1), vec![log_vkey.clone()])?;
//! let mut second = Witness::new(Box::new(w2), vec![log_vkey.clone()])?;
//! let request = log.add_request(TreeSize(0), &note)?;
//! let cosigned = note
//!     .with_sig(first.add_checkpoint(&request, UnixTime::from_secs(1_000))?)?
//!     .with_sig(second.add_checkpoint(&request, UnixTime::from_secs(1_010))?)?;
//! let bundle = log.bundle(index, &cosigned)?;
//!
//! // A verifier: the policy is data (the log, the witnesses, 2 of 2); the bundle is the proof.
//! let policy = Policy::new(log_vkey, witnesses, 2, Span::from_secs(300))?;
//! let expect = Expect {
//!     kind: SubjectKind::DesktopApp,
//!     name: Name::new("azdrive")?,
//!     version: Version::new("0.3.0")?,
//!     arch: Arch::Any,
//!     digest: Some(record.digest),
//! };
//! let logged = verify_bundle(&bundle, &policy, UnixTime::from_secs(2_000), &expect)?;
//! assert_eq!(logged.logged_since(), UnixTime::from_secs(1_010));
//! assert_eq!(logged.index(), index);
//! # Ok(())
//! # }
//! ```

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

mod bundle;
mod checkpoint;
mod codec;
mod cosign;
mod log;
mod monitor;
mod names;
mod note;
mod policy;
mod record;
mod time;
mod witness;

pub use azlin_merkle::{
    ConsistencyProof, Hash, InclusionProof, LeafIndex, LeafStore, MemStore, ProofError, StoreError,
    TreeError, TreeSize,
};

pub use bundle::{
    verify_bundle, BundleError, BundleLine, Expect, ExpectField, Logged, ProofBundle,
    BUNDLE_HEADER, MAX_BUNDLE_BYTES,
};
pub use checkpoint::{
    sign_checkpoint, Checkpoint, CheckpointError, MAX_EXTENSIONS, MAX_EXTENSION_BYTES,
};
pub use codec::NumberError;
pub use cosign::Cosigner;
pub use log::{Log, LogError, MemRecords, RecordStore};
pub use monitor::{Alarm, Monitor, Observed, MAX_INDEX_RECORDS};
pub use names::{KeyHash, KeyName, NameError, Origin, SigKind, DEFAULT_ORIGIN, MAX_NAME_BYTES};
pub use note::{
    MemoryKey, Note, NoteError, NoteSig, NoteSigner, SignError, Verified, Verifier, MAX_NOTE_BYTES,
    MAX_SIGNATURES, MAX_SIG_PAYLOAD, MAX_TEXT_BYTES,
};
pub use policy::{LogKey, Policy, PolicyError, Witnessed, MAX_WITNESSES};
pub use record::{
    Arch, Digest, Field, Name, RecordError, ReleaseKey, ReleaseRecord, SignerId, SubjectKind,
    Version, MAX_RECORD_BYTES, RECORD_HEADER,
};
pub use time::{Span, UnixTime};
pub use witness::{
    AddRequest, RequestError, Witness, WitnessError, WitnessRefusal, MAX_ORIGINS, MAX_PROOF_LINES,
    MAX_REQUEST_BYTES,
};
