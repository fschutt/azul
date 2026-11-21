//! The proof bundle and its verifier: what travels with a release, and what every client checks before it
//! installs (planning/architecture.md section 2.20: "Nodes, AzWorkspace and the apps install a release only
//! with an inclusion proof under a witnessed checkpoint"; section 2.20 point 6: for a node the bundle sits in
//! `rel` beside the image, because a node reaches only `ctl` and `rel`, 4.1).
//!
//! The text form `azlin-proof-v1` is strict (T27) and at most [`MAX_BUNDLE_BYTES`] (64 KiB):
//!
//! ```text
//! azlin-proof-v1\n
//! leaf=<index>\n
//! size=<tree size>\n
//! path=<base64 hash>\n         zero to 64 lines, leaf side first
//! record=<base64 of the record bytes>\n
//! checkpoint=<base64 of the whole cosigned note>\n
//! ```
//!
//! [`verify_bundle`] checks every link and returns a [`Logged`] or one typed [`BundleError`] (T28): the
//! record parses in its one canonical form; the checkpoint note parses and names a checkpoint; the proof's
//! tree size IS the checkpoint's (the Merkle verifier does not bind it: `azlin-merkle`'s drift row
//! X-DELTA-W1S45B-3, coordinator's note); the inclusion proof leads from the record's leaf hash to the
//! checkpoint's root; the record is the release the caller expects ([`Expect`]); and the checkpoint passes
//! the [`Policy`](crate::Policy): the log's signature, the quorum of witnesses, no cosignature from the
//! future. The cheap checks come first, so a damaged bundle costs no signature check. A bundle carries its
//! OWN checkpoint, so one made at size 5 still verifies after the log grew to 50 (T29).
//!
//! Seam: first callers are S45-INT-2 (a node's check 8 in `azlin-proto`'s `image.rs`) and S45-INT-3
//! (AzWorkspace and the apps' updater); the page of `drive.azlin.io` (S48) runs it as wasm.

use std::fmt;

use azlin_merkle::{
    leaf_hash, verify_inclusion, InclusionProof, LeafIndex, ProofError, TreeSize, MAX_PROOF_HASHES,
};

use crate::checkpoint::{Checkpoint, CheckpointError};
use crate::codec::{b64_decode, b64_encode, b64_hash, parse_decimal, NumberError};
use crate::names::KeyName;
use crate::note::{Note, NoteError, MAX_NOTE_BYTES};
use crate::policy::{Policy, PolicyError};
use crate::record::{
    Arch, Digest, Name, RecordError, ReleaseRecord, SubjectKind, Version, MAX_RECORD_BYTES,
};
use crate::time::UnixTime;

/// The longest bundle text in bytes (brief section 5).
pub const MAX_BUNDLE_BYTES: usize = 64 * 1024;

/// The first line of a bundle.
pub const BUNDLE_HEADER: &str = "azlin-proof-v1";

/// A line of the bundle text a refusal is about.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BundleLine {
    /// The header.
    Header,
    /// `leaf=`.
    Leaf,
    /// `size=`.
    Size,
    /// `path=`.
    Path,
    /// `record=`.
    Record,
    /// `checkpoint=`.
    Checkpoint,
}

/// What a caller expects of the release it is about to install, compared with the record of the bundle.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExpectField {
    /// The kind of the release.
    Kind,
    /// The name.
    Name,
    /// The version.
    Version,
    /// The architecture.
    Arch,
    /// The digest.
    Digest,
}

/// Why a bundle is malformed, or did not verify. Never a string.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum BundleError {
    /// The text is longer than [`MAX_BUNDLE_BYTES`], or a part is longer than its limit.
    TooLarge,
    /// The text is not valid UTF-8.
    NotUtf8,
    /// The text does not end in a newline.
    NoFinalNewline,
    /// This line is missing, has another key or is out of order.
    Line(BundleLine),
    /// `leaf=` or `size=` is no valid decimal.
    Number {
        /// The line.
        line: BundleLine,
        /// Why.
        error: NumberError,
    },
    /// `record=` or `checkpoint=` is not canonical base64.
    Base64(BundleLine),
    /// A `path=` line is not the canonical base64 of 32 bytes.
    BadHash,
    /// More than [`MAX_PROOF_HASHES`] `path=` lines.
    TooManyPath,
    /// A line after `checkpoint=`.
    Trailing,
    /// The record is not a release record.
    Record(RecordError),
    /// The checkpoint is no signed note.
    Note(NoteError),
    /// The note text is no checkpoint.
    Checkpoint(CheckpointError),
    /// The proof was made for another tree size than the checkpoint's.
    SizeMismatch {
        /// The size the proof names.
        proof: TreeSize,
        /// The size of the checkpoint.
        checkpoint: TreeSize,
    },
    /// The inclusion proof does not lead from the record to the checkpoint's root.
    ProofFailed(ProofError),
    /// The record is a release other than the one expected.
    ExpectMismatch(ExpectField),
    /// The checkpoint does not pass the policy: not signed by the log, not cosigned by a quorum, a bad or
    /// future cosignature.
    NotWitnessed(PolicyError),
}

impl fmt::Display for BundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BundleError::TooLarge => f.write_str("the bundle is too large"),
            BundleError::NotUtf8 => f.write_str("the bundle is not valid UTF-8"),
            BundleError::NoFinalNewline => f.write_str("the bundle does not end in a newline"),
            BundleError::Line(line) => write!(f, "the {line:?} line is missing or out of order"),
            BundleError::Number { line, error } => {
                write!(f, "the {line:?} number is refused: {error}")
            }
            BundleError::Base64(line) => write!(f, "the {line:?} line is not canonical base64"),
            BundleError::BadHash => f.write_str("a path line is not the base64 of 32 bytes"),
            BundleError::TooManyPath => f.write_str("more than 64 path lines"),
            BundleError::Trailing => f.write_str("a line after the checkpoint"),
            BundleError::Record(error) => write!(f, "the record is refused: {error}"),
            BundleError::Note(error) => write!(f, "the checkpoint note is refused: {error}"),
            BundleError::Checkpoint(error) => write!(f, "the note is no checkpoint: {error}"),
            BundleError::SizeMismatch { proof, checkpoint } => write!(
                f,
                "the proof is for size {} and the checkpoint has size {}",
                proof.0, checkpoint.0
            ),
            BundleError::ProofFailed(error) => write!(f, "the inclusion proof fails: {error}"),
            BundleError::ExpectMismatch(field) => {
                write!(f, "the record is not the expected release ({field:?})")
            }
            BundleError::NotWitnessed(error) => {
                write!(f, "the checkpoint is not witnessed: {error}")
            }
        }
    }
}

impl std::error::Error for BundleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BundleError::Number { error, .. } => Some(error),
            BundleError::Record(error) => Some(error),
            BundleError::Note(error) => Some(error),
            BundleError::Checkpoint(error) => Some(error),
            BundleError::ProofFailed(error) => Some(error),
            BundleError::NotWitnessed(error) => Some(error),
            _ => None,
        }
    }
}

/// The proof that travels with a release: the record, the inclusion proof of its leaf and the cosigned
/// checkpoint the proof leads to, as bytes. Plain data: [`verify_bundle`] is what makes it trusted.
///
/// Seam: made by `log::Log::bundle` (S45-INT-1); read by S45-INT-2 and S45-INT-3.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ProofBundle {
    /// The bytes of the release record (the leaf data).
    pub record: Vec<u8>,
    /// The inclusion proof of the leaf: its index, the size of the tree and the sibling hashes.
    pub proof: InclusionProof,
    /// The whole cosigned checkpoint note, as bytes.
    pub checkpoint: Vec<u8>,
}

impl ProofBundle {
    /// The text form (see the module documentation); refused when a part or the whole is over its limit.
    pub fn to_text(&self) -> Result<String, BundleError> {
        if self.record.len() > MAX_RECORD_BYTES || self.checkpoint.len() > MAX_NOTE_BYTES {
            return Err(BundleError::TooLarge);
        }
        if self.proof.path.len() > MAX_PROOF_HASHES {
            return Err(BundleError::TooManyPath);
        }
        let mut text = format!(
            "{BUNDLE_HEADER}\nleaf={}\nsize={}\n",
            self.proof.index.0, self.proof.size.0
        );
        for hash in &self.proof.path {
            text.push_str("path=");
            text.push_str(&b64_encode(hash.as_bytes()));
            text.push('\n');
        }
        text.push_str("record=");
        text.push_str(&b64_encode(&self.record));
        text.push_str("\ncheckpoint=");
        text.push_str(&b64_encode(&self.checkpoint));
        text.push('\n');
        if text.len() > MAX_BUNDLE_BYTES {
            return Err(BundleError::TooLarge);
        }
        Ok(text)
    }

    /// Parses the text form strictly (T27): this form and no other. `parse(to_text(b)) == b`.
    pub fn parse(bytes: &[u8]) -> Result<ProofBundle, BundleError> {
        if bytes.len() > MAX_BUNDLE_BYTES {
            return Err(BundleError::TooLarge);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| BundleError::NotUtf8)?;
        let body = text.strip_suffix('\n').ok_or(BundleError::NoFinalNewline)?;
        let mut lines = body.split('\n');
        if lines.next() != Some(BUNDLE_HEADER) {
            return Err(BundleError::Line(BundleLine::Header));
        }
        let index = number(lines.next(), "leaf=", BundleLine::Leaf)?;
        let size = number(lines.next(), "size=", BundleLine::Size)?;
        let mut path = Vec::new();
        let record = loop {
            let line = lines.next().ok_or(BundleError::Line(BundleLine::Record))?;
            if let Some(hash) = line.strip_prefix("path=") {
                if path.len() == MAX_PROOF_HASHES {
                    return Err(BundleError::TooManyPath);
                }
                path.push(b64_hash(hash).ok_or(BundleError::BadHash)?);
                continue;
            }
            break line
                .strip_prefix("record=")
                .ok_or(BundleError::Line(BundleLine::Record))?;
        };
        // Bound each decode before it runs: base64 is 4 characters for 3 bytes.
        if record.len() > MAX_RECORD_BYTES.div_ceil(3) * 4 {
            return Err(BundleError::TooLarge);
        }
        let record = b64_decode(record).ok_or(BundleError::Base64(BundleLine::Record))?;
        if record.len() > MAX_RECORD_BYTES {
            return Err(BundleError::TooLarge);
        }
        let checkpoint = lines
            .next()
            .and_then(|line| line.strip_prefix("checkpoint="))
            .ok_or(BundleError::Line(BundleLine::Checkpoint))?;
        if checkpoint.len() > MAX_NOTE_BYTES.div_ceil(3) * 4 {
            return Err(BundleError::TooLarge);
        }
        let checkpoint =
            b64_decode(checkpoint).ok_or(BundleError::Base64(BundleLine::Checkpoint))?;
        if checkpoint.len() > MAX_NOTE_BYTES {
            return Err(BundleError::TooLarge);
        }
        if lines.next().is_some() {
            return Err(BundleError::Trailing);
        }
        Ok(ProofBundle {
            record,
            proof: InclusionProof {
                index: LeafIndex(index),
                size: TreeSize(size),
                path,
            },
            checkpoint,
        })
    }
}

/// The number of a `key=<decimal>` line.
fn number(line: Option<&str>, key: &str, which: BundleLine) -> Result<u64, BundleError> {
    let value = line
        .and_then(|line| line.strip_prefix(key))
        .ok_or(BundleError::Line(which))?;
    parse_decimal(value).map_err(|error| BundleError::Number { line: which, error })
}

/// The release a caller is about to install: the record must be this release. The digest may be left open
/// (`None`) by a caller that learns it from the bundle.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Expect {
    /// The kind of the release.
    pub kind: SubjectKind,
    /// The name.
    pub name: Name,
    /// The version.
    pub version: Version,
    /// The architecture.
    pub arch: Arch,
    /// The digest of the signed manifest or statement, if the caller knows it.
    pub digest: Option<Digest>,
}

/// A release that is in the log under a witnessed checkpoint. Made only by [`verify_bundle`].
///
/// Seam: first callers are S45-INT-2, S45-INT-3 and S48, which reads [`Logged::logged_since`] and
/// `record().security_fix` for the waiting period.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Logged {
    record: ReleaseRecord,
    index: LeafIndex,
    logged_since: UnixTime,
    checkpoint: Checkpoint,
    cosigners: Vec<KeyName>,
}

impl Logged {
    /// The release record, as the log holds it.
    pub fn record(&self) -> &ReleaseRecord {
        &self.record
    }

    /// The leaf of the record.
    pub fn index(&self) -> LeafIndex {
        self.index
    }

    /// The `quorum`-th smallest cosignature time of the checkpoint the proof leads to (T12): since then the
    /// release is public.
    pub fn logged_since(&self) -> UnixTime {
        self.logged_since
    }

    /// The checkpoint the proof leads to.
    pub fn checkpoint(&self) -> &Checkpoint {
        &self.checkpoint
    }

    /// The witnesses whose valid cosignature the checkpoint carries.
    pub fn cosigners(&self) -> &[KeyName] {
        &self.cosigners
    }
}

/// Verifies a bundle (see the module documentation): the record is in the log under a checkpoint that
/// `policy` accepts at `now`, and is the release `expect`.
///
/// Seam: first callers are S45-INT-2 (a node's check 8) and S45-INT-3 (AzWorkspace, the apps' updater).
pub fn verify_bundle(
    bundle: &ProofBundle,
    policy: &Policy,
    now: UnixTime,
    expect: &Expect,
) -> Result<Logged, BundleError> {
    let record = ReleaseRecord::parse(&bundle.record).map_err(BundleError::Record)?;
    let note = Note::parse(&bundle.checkpoint).map_err(BundleError::Note)?;
    let checkpoint = Checkpoint::parse_text(note.text()).map_err(BundleError::Checkpoint)?;
    // `verify_inclusion` reads the size only through the shape of the proof, so it is compared here.
    if bundle.proof.size != checkpoint.size() {
        return Err(BundleError::SizeMismatch {
            proof: bundle.proof.size,
            checkpoint: checkpoint.size(),
        });
    }
    // The leaf hash is of the bytes the bundle carries, the bytes the log committed to; `ReleaseRecord::parse`
    // accepts one canonical form only, so they are the record's own bytes.
    verify_inclusion(
        &leaf_hash(&bundle.record),
        &bundle.proof,
        &checkpoint.root(),
    )
    .map_err(BundleError::ProofFailed)?;
    if let Some(field) = mismatch(&record, expect) {
        return Err(BundleError::ExpectMismatch(field));
    }
    let witnessed = policy
        .check(&note, now)
        .map_err(BundleError::NotWitnessed)?;
    Ok(Logged {
        record,
        index: bundle.proof.index,
        logged_since: witnessed.logged_since(),
        checkpoint,
        cosigners: witnessed.cosigners().to_vec(),
    })
}

/// The first field of `record` that is not what `expect` names.
fn mismatch(record: &ReleaseRecord, expect: &Expect) -> Option<ExpectField> {
    if record.kind != expect.kind {
        return Some(ExpectField::Kind);
    }
    if record.name != expect.name {
        return Some(ExpectField::Name);
    }
    if record.version != expect.version {
        return Some(ExpectField::Version);
    }
    if record.arch != expect.arch {
        return Some(ExpectField::Arch);
    }
    match &expect.digest {
        Some(digest) if *digest != record.digest => Some(ExpectField::Digest),
        _ => None,
    }
}
