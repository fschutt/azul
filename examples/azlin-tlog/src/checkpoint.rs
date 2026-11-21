//! Checkpoints: the signed head of the log (C2SP `tlog-checkpoint`; planning/architecture.md section 2.20:
//! "a Merkle tree whose checkpoints independent witnesses cosign").
//!
//! The note text of a checkpoint is `<origin>\n<tree size>\n<base64 root>\n` and then zero or more extension
//! lines, each non-empty (C2SP: "at least three non-empty lines"; "extension lines, if any, MUST be
//! non-empty"). The tree size is an ASCII decimal without leading zeros; the root is the canonical base64
//! of the 32-byte RFC 9162 root; a tree of size zero has the root of the empty tree (C2SP `tlog-witness`).
//! This crate limits the extensions (the spec does not): at most [`MAX_EXTENSIONS`] lines of at most
//! [`MAX_EXTENSION_BYTES`] bytes and without a control character (T7), and the tree size: a checkpoint never
//! names more than [`MAX_LEAVES`] leaves (2^32, the limit of `azlin-merkle`'s tree, where the proofs have at
//! most 33 hashes; the coordinator's note on the Merkle drift rows X-DELTA-W1S45B-4 and -6). The log signs
//! the text with an Ed25519 signature of type `0x01` ([`sign_checkpoint`]); a witness cosigns the same
//! text.
//!
//! Seam: first callers are `log::Log::checkpoint` (S45-INT-1: AzCtl signs after each release),
//! `policy::Policy::check` and `bundle::verify_bundle` (S45-INT-2, S45-INT-3).

use std::fmt;

use azlin_merkle::{empty_root, Hash, TreeSize, MAX_LEAVES};

use crate::codec::{b64_decode, b64_encode, parse_decimal, NumberError};
use crate::names::{NameError, Origin};
use crate::note::{Note, NoteError, NoteSig, NoteSigner, MAX_TEXT_BYTES};

/// The most extension lines of a checkpoint.
pub const MAX_EXTENSIONS: usize = 8;

/// The longest extension line in bytes.
pub const MAX_EXTENSION_BYTES: usize = 256;

/// Why a checkpoint text is refused. Never a string.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CheckpointError {
    /// The text is longer than the note limit.
    TooLarge,
    /// The text does not end in a newline.
    NoFinalNewline,
    /// Fewer than three lines: an origin, a size and a root are all required.
    TooFewLines,
    /// The first line is no origin.
    BadOrigin(NameError),
    /// The size line is no valid decimal (see [`NumberError`]).
    Size(NumberError),
    /// The size is above [`MAX_LEAVES`], the most leaves a log of this crate holds.
    TooManyLeaves,
    /// The root line is not canonical base64.
    RootNotBase64,
    /// The root decodes to a length other than 32 bytes.
    RootLength,
    /// A tree of size zero must carry the root of the empty tree.
    EmptyTreeRoot,
    /// An extension line is empty.
    EmptyExtension,
    /// An extension line is longer than [`MAX_EXTENSION_BYTES`].
    ExtensionTooLong,
    /// More than [`MAX_EXTENSIONS`] extension lines.
    TooManyExtensions,
    /// An extension line has a control character below U+0020.
    ExtensionControl,
}

impl fmt::Display for CheckpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckpointError::TooLarge => f.write_str("the checkpoint text is too large"),
            CheckpointError::NoFinalNewline => {
                f.write_str("the checkpoint text does not end in a newline")
            }
            CheckpointError::TooFewLines => {
                f.write_str("a checkpoint has an origin, a size and a root")
            }
            CheckpointError::BadOrigin(error) => write!(f, "the origin is refused: {error}"),
            CheckpointError::Size(error) => write!(f, "the tree size is refused: {error}"),
            CheckpointError::TooManyLeaves => f.write_str("the tree size is above 2^32 leaves"),
            CheckpointError::RootNotBase64 => f.write_str("the root is not canonical base64"),
            CheckpointError::RootLength => f.write_str("the root is not 32 bytes"),
            CheckpointError::EmptyTreeRoot => {
                f.write_str("a tree of size zero has the root of the empty tree")
            }
            CheckpointError::EmptyExtension => f.write_str("an extension line is empty"),
            CheckpointError::ExtensionTooLong => {
                f.write_str("an extension line is longer than 256 bytes")
            }
            CheckpointError::TooManyExtensions => {
                f.write_str("a checkpoint has at most 8 extension lines")
            }
            CheckpointError::ExtensionControl => {
                f.write_str("an extension line has a control character")
            }
        }
    }
}

impl std::error::Error for CheckpointError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CheckpointError::BadOrigin(error) => Some(error),
            CheckpointError::Size(error) => Some(error),
            _ => None,
        }
    }
}

/// The head of a log: its origin, the number of leaves and the root over them, and the extension lines the
/// log chose to add. Made only by [`Checkpoint::new`] and [`Checkpoint::parse_text`].
///
/// Seam: first callers are `log::Log::checkpoint` and the verifiers of S45-INT-2 and S45-INT-3.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Checkpoint {
    origin: Origin,
    size: TreeSize,
    root: Hash,
    extensions: Vec<String>,
}

impl Checkpoint {
    /// A checkpoint; the extension lines are checked as the module documentation says, the size is at most
    /// [`MAX_LEAVES`], and a tree of size zero must have the root of the empty tree.
    pub fn new(
        origin: Origin,
        size: TreeSize,
        root: Hash,
        extensions: Vec<String>,
    ) -> Result<Checkpoint, CheckpointError> {
        if extensions.len() > MAX_EXTENSIONS {
            return Err(CheckpointError::TooManyExtensions);
        }
        for line in &extensions {
            if line.is_empty() {
                return Err(CheckpointError::EmptyExtension);
            }
            if line.len() > MAX_EXTENSION_BYTES {
                return Err(CheckpointError::ExtensionTooLong);
            }
            if line.bytes().any(|b| b < 0x20) {
                return Err(CheckpointError::ExtensionControl);
            }
        }
        if size.0 > MAX_LEAVES.0 {
            return Err(CheckpointError::TooManyLeaves);
        }
        if size.0 == 0 && root != empty_root() {
            return Err(CheckpointError::EmptyTreeRoot);
        }
        Ok(Checkpoint {
            origin,
            size,
            root,
            extensions,
        })
    }

    /// The log's origin.
    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// The number of leaves.
    pub fn size(&self) -> TreeSize {
        self.size
    }

    /// The root over the leaves.
    pub fn root(&self) -> Hash {
        self.root
    }

    /// The extension lines, in order.
    pub fn extensions(&self) -> &[String] {
        &self.extensions
    }

    /// The note text: `<origin>\n<size>\n<base64 root>\n` and one line per extension, each with its newline.
    pub fn text(&self) -> String {
        let mut text = format!(
            "{}\n{}\n{}\n",
            self.origin,
            self.size.0,
            b64_encode(self.root.as_bytes())
        );
        for line in &self.extensions {
            text.push_str(line);
            text.push('\n');
        }
        text
    }

    /// Parses the note text of a checkpoint strictly; `parse_text(cp.text()) == cp` (T6, T7).
    pub fn parse_text(text: &str) -> Result<Checkpoint, CheckpointError> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(CheckpointError::TooLarge);
        }
        let body = text
            .strip_suffix('\n')
            .ok_or(CheckpointError::NoFinalNewline)?;
        let mut lines = body.split('\n');
        let (Some(origin), Some(size), Some(root)) = (lines.next(), lines.next(), lines.next())
        else {
            return Err(CheckpointError::TooFewLines);
        };
        let origin = Origin::new(origin).map_err(CheckpointError::BadOrigin)?;
        let size = parse_decimal(size).map_err(CheckpointError::Size)?;
        let root = b64_decode(root).ok_or(CheckpointError::RootNotBase64)?;
        let root: [u8; 32] = root.try_into().map_err(|_| CheckpointError::RootLength)?;
        let mut extensions = Vec::new();
        for line in lines {
            if extensions.len() == MAX_EXTENSIONS {
                return Err(CheckpointError::TooManyExtensions);
            }
            extensions.push(line.to_string());
        }
        Checkpoint::new(origin, TreeSize(size), Hash(root), extensions)
    }
}

/// Signs a checkpoint as the log: one signature line of type `0x01` over the checkpoint text (T1, T4). The
/// result is a note with the log's signature only; witnesses add theirs ([`Note::with_sig`]).
///
/// Seam: first caller is `log::Log::checkpoint` for AzCtl's release flow (S45-INT-1), which holds the log
/// key in the Keychain behind Touch ID.
pub fn sign_checkpoint(cp: &Checkpoint, signer: &dyn NoteSigner) -> Result<Note, NoteError> {
    let text = cp.text();
    let signature = signer.sign_text(text.as_bytes()).map_err(NoteError::Sign)?;
    let sig = NoteSig::raw(signer.name().clone(), signer.key_hash(), signature.to_vec());
    Note::new(text, vec![sig])
}
