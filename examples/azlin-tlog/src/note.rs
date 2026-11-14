//! Signed notes: the text format of every signature the log carries (C2SP `signed-note`;
//! planning/architecture.md section 2.20, "the C2SP witness protocol").
//!
//! A note is a text that ends in a newline, one blank line and one or more signature lines
//! `— <key name> <base64(key hash (4 bytes) || signature)>`. The parse is strict (T2): the text is valid
//! UTF-8 without a control character below U+0020 other than the newline, at most [`MAX_TEXT_BYTES`]; the
//! text is separated from the signatures by the LAST empty line; at most [`MAX_SIGNATURES`] signature
//! lines; canonical base64; a key name without a space or a plus sign; no two lines with the same name AND
//! key hash (C2SP `tlog-checkpoint`: "there MUST NOT be multiple signature lines with both the same key
//! name and the same key id"); nothing after the last signature line.
//!
//! Verification follows `signed-note` word for word (T5): a signature from an unknown key is ignored
//! "even if they share a name or ID (but not both) with a known key"; "if the key name and ID match a known
//! key, verifiers MUST verify the signature", and a known key whose signature fails rejects the whole note;
//! a note with no signature of a known key is rejected. The signature type is part of the key hash (T3,
//! T9): the log's type `0x01` and a witness's type `0x04` are two different keys even for one public key.
//!
//! Seam: the first callers are `policy::Policy::check` (nodes' check 8, S45-INT-2) and
//! `witness::Witness::add_checkpoint` (the operator's cosignature round, S45-INT-1). A key that holds a
//! secret is a [`NoteSigner`]: [`MemoryKey`] here, the Keychain-backed signer of AzCtl (Touch ID) in
//! S45-INT-1.

use std::fmt;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

use crate::codec::{b64_decode, b64_encode, hex_decode, hex_encode};
use crate::cosign::cosignature_message;
use crate::names::{KeyHash, KeyName, NameError, SigKind, MAX_NAME_BYTES};
use crate::time::UnixTime;

/// The longest note text in bytes (architecture 1.4 rule 4; brief section 5).
pub const MAX_TEXT_BYTES: usize = 16 * 1024;

/// The most signature lines of a note (C2SP `signed-note`: "MUST accept at least up to 16 signatures").
pub const MAX_SIGNATURES: usize = 16;

/// The longest signature payload in bytes (the bytes after the 4-byte key hash): room for the post-quantum
/// cosignatures of the current C2SP texts (about 2.4 kB), which this crate carries but does not verify.
pub const MAX_SIG_PAYLOAD: usize = 4096;

/// The longest signature line: the em dash and its space (4 bytes), a name, a space, base64 of the key hash
/// and the longest payload, the newline.
const MAX_SIG_LINE: usize = 4 + MAX_NAME_BYTES + 1 + (4 + MAX_SIG_PAYLOAD).div_ceil(3) * 4 + 1;

/// The longest note in bytes: the text, the blank line and [`MAX_SIGNATURES`] signature lines.
pub const MAX_NOTE_BYTES: usize = MAX_TEXT_BYTES + 1 + MAX_SIGNATURES * MAX_SIG_LINE;

/// The dash and the space that start a signature line.
const DASH: &str = "\u{2014} ";

/// Why a signer did not sign. Never a string.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SignError {
    /// The signer refused (a person declined the Touch ID prompt, a policy said no).
    Refused,
    /// The signer cannot be reached now (a locked keychain, a missing device).
    Unavailable,
    /// The text handed to a cosigner is not a checkpoint text.
    NotACheckpoint,
}

impl fmt::Display for SignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SignError::Refused => "the signer refused to sign",
            SignError::Unavailable => "the signer cannot be reached",
            SignError::NotACheckpoint => "the text to cosign is not a checkpoint text",
        })
    }
}

impl std::error::Error for SignError {}

/// Why a note was refused, did not verify, or could not be made. Never a string.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum NoteError {
    /// The text is longer than [`MAX_TEXT_BYTES`], a signature payload longer than [`MAX_SIG_PAYLOAD`], or
    /// the note longer than [`MAX_NOTE_BYTES`].
    TooLarge,
    /// More than [`MAX_SIGNATURES`] signature lines.
    TooMany,
    /// The bytes are not valid UTF-8.
    NotUtf8,
    /// A control character below U+0020 other than the newline (a carriage return included).
    ControlCharacter,
    /// The note does not end in a newline.
    NoFinalNewline,
    /// There is no empty line between the text and the signatures.
    NoBlankLine,
    /// The text holds nothing but newlines.
    EmptyText,
    /// The note has no signature line.
    NoSignature,
    /// A line after the blank line does not start with the em dash and a space.
    NotASignatureLine,
    /// A signature line has no space between the key name and the signature.
    MissingSpace,
    /// A key name that breaks the name rule.
    BadName(NameError),
    /// The signature is not canonical standard base64 with padding.
    BadBase64,
    /// The signature decodes to 4 bytes or fewer: a key hash and no signature.
    ShortPayload,
    /// Two signature lines with the same key name and the same key hash.
    DuplicateSignature,
    /// A signature of a known key does not verify (the whole note is refused).
    BadSignature {
        /// The key whose signature failed.
        name: KeyName,
    },
    /// No signature of a known key is present.
    NoKnownSignature,
    /// A configured public key is not a usable Ed25519 key (not on the curve, or of small order).
    BadKey,
    /// A verifier key text is malformed.
    BadVkey,
    /// A signer failed.
    Sign(SignError),
}

impl fmt::Display for NoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NoteError::TooLarge => f.write_str("the note or a part of it is too large"),
            NoteError::TooMany => f.write_str("the note has more than 16 signature lines"),
            NoteError::NotUtf8 => f.write_str("the note is not valid UTF-8"),
            NoteError::ControlCharacter => f.write_str("the note has a control character"),
            NoteError::NoFinalNewline => f.write_str("the note does not end in a newline"),
            NoteError::NoBlankLine => {
                f.write_str("no empty line between the text and the signatures")
            }
            NoteError::EmptyText => f.write_str("the note text is empty"),
            NoteError::NoSignature => f.write_str("the note has no signature"),
            NoteError::NotASignatureLine => {
                f.write_str("a line after the text is not a signature line")
            }
            NoteError::MissingSpace => {
                f.write_str("a signature line has no space before the signature")
            }
            NoteError::BadName(error) => write!(f, "a key name is refused: {error}"),
            NoteError::BadBase64 => f.write_str("a signature is not canonical base64"),
            NoteError::ShortPayload => f.write_str("a signature has no bytes after the key hash"),
            NoteError::DuplicateSignature => {
                f.write_str("two signature lines have the same key name and hash")
            }
            NoteError::BadSignature { name } => {
                write!(f, "the signature of {name} does not verify")
            }
            NoteError::NoKnownSignature => f.write_str("no signature of a known key"),
            NoteError::BadKey => f.write_str("a configured public key is not usable"),
            NoteError::BadVkey => f.write_str("a verifier key text is malformed"),
            NoteError::Sign(error) => write!(f, "signing failed: {error}"),
        }
    }
}

impl std::error::Error for NoteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            NoteError::BadName(error) => Some(error),
            NoteError::Sign(error) => Some(error),
            _ => None,
        }
    }
}

/// One signature line of a note: the key's name, its 4-byte key hash and the signature bytes (an Ed25519
/// signature of 64 bytes for type `0x01`, a time and a signature of 72 bytes for type `0x04`).
///
/// Seam: made by [`NoteSigner`]s and [`Cosigner`](crate::Cosigner)s; first callers outside the crate are
/// S45-INT-1 (AzCtl's cosignature round).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NoteSig {
    name: KeyName,
    hash: KeyHash,
    payload: Vec<u8>,
}

impl NoteSig {
    /// A signature line from its parts; the payload (the bytes after the key hash) is 1 to
    /// [`MAX_SIG_PAYLOAD`] bytes.
    pub fn new(name: KeyName, hash: KeyHash, payload: Vec<u8>) -> Result<NoteSig, NoteError> {
        if payload.is_empty() {
            return Err(NoteError::ShortPayload);
        }
        if payload.len() > MAX_SIG_PAYLOAD {
            return Err(NoteError::TooLarge);
        }
        Ok(NoteSig {
            name,
            hash,
            payload,
        })
    }

    /// A signature line whose payload is known to be in bounds (a signature this crate just made: 64 or 72
    /// bytes).
    pub(crate) fn raw(name: KeyName, hash: KeyHash, payload: Vec<u8>) -> NoteSig {
        NoteSig {
            name,
            hash,
            payload,
        }
    }

    /// The key's name.
    pub fn name(&self) -> &KeyName {
        &self.name
    }

    /// The key hash of the line.
    pub fn key_hash(&self) -> KeyHash {
        self.hash
    }

    /// The signature bytes after the key hash.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

/// A signed note: a text and its signature lines. Made only by [`Note::new`], [`Note::parse`] and
/// [`Note::with_sig`], so every note holds the invariants of the module documentation.
///
/// Seam: the first callers are `checkpoint::sign_checkpoint` and `policy::Policy::check` (S45-INT-1, S45-INT-2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Note {
    text: String,
    sigs: Vec<NoteSig>,
}

impl Note {
    /// A note from a text and its signatures: the text ends in a newline, is not empty and has no control
    /// character below U+0020 other than the newline, at most [`MAX_TEXT_BYTES`]; one to
    /// [`MAX_SIGNATURES`] signatures without a duplicate name-and-hash pair.
    pub fn new(text: String, sigs: Vec<NoteSig>) -> Result<Note, NoteError> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(NoteError::TooLarge);
        }
        if text.bytes().any(|b| b < 0x20 && b != b'\n') {
            return Err(NoteError::ControlCharacter);
        }
        if !text.ends_with('\n') {
            return Err(NoteError::NoFinalNewline);
        }
        if text.bytes().all(|b| b == b'\n') {
            return Err(NoteError::EmptyText);
        }
        if sigs.is_empty() {
            return Err(NoteError::NoSignature);
        }
        if sigs.len() > MAX_SIGNATURES {
            return Err(NoteError::TooMany);
        }
        for (i, sig) in sigs.iter().enumerate() {
            let duplicate = sigs
                .iter()
                .skip(i + 1)
                .any(|other| other.name == sig.name && other.hash == sig.hash);
            if duplicate {
                return Err(NoteError::DuplicateSignature);
            }
        }
        Ok(Note { text, sigs })
    }

    /// The same note with one more signature line (a cosignature a witness returned).
    pub fn with_sig(&self, sig: NoteSig) -> Result<Note, NoteError> {
        let mut sigs = self.sigs.clone();
        sigs.push(sig);
        Note::new(self.text.clone(), sigs)
    }

    /// The text, with its last newline (the bytes every signature covers).
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The signature lines, in the order of the note.
    pub fn sigs(&self) -> &[NoteSig] {
        &self.sigs
    }

    /// The note as bytes: the text, the empty line and one line per signature.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = String::with_capacity(self.text.len() + 1 + self.sigs.len() * 200);
        out.push_str(&self.text);
        out.push('\n');
        for sig in &self.sigs {
            out.push_str(DASH);
            out.push_str(sig.name.as_str());
            out.push(' ');
            let mut raw = Vec::with_capacity(4 + sig.payload.len());
            raw.extend_from_slice(sig.hash.as_bytes());
            raw.extend_from_slice(&sig.payload);
            out.push_str(&b64_encode(&raw));
            out.push('\n');
        }
        out.into_bytes()
    }

    /// Parses a note strictly (see the module documentation); `parse(to_bytes(n)) == n`.
    pub fn parse(bytes: &[u8]) -> Result<Note, NoteError> {
        if bytes.len() > MAX_NOTE_BYTES {
            return Err(NoteError::TooLarge);
        }
        let all = std::str::from_utf8(bytes).map_err(|_| NoteError::NotUtf8)?;
        if all.bytes().any(|b| b < 0x20 && b != b'\n') {
            return Err(NoteError::ControlCharacter);
        }
        if !all.ends_with('\n') {
            return Err(NoteError::NoFinalNewline);
        }
        let split = all.rfind("\n\n").ok_or(NoteError::NoBlankLine)?;
        let (text, rest) = all
            .split_at_checked(split + 1)
            .ok_or(NoteError::NoBlankLine)?;
        let signatures = rest.strip_prefix('\n').ok_or(NoteError::NoBlankLine)?;
        if text.len() > MAX_TEXT_BYTES {
            return Err(NoteError::TooLarge);
        }
        if signatures.is_empty() {
            return Err(NoteError::NoSignature);
        }
        let signatures = signatures
            .strip_suffix('\n')
            .ok_or(NoteError::NoFinalNewline)?;
        let mut sigs = Vec::new();
        for line in signatures.split('\n') {
            if sigs.len() == MAX_SIGNATURES {
                return Err(NoteError::TooMany);
            }
            sigs.push(parse_signature_line(line)?);
        }
        Note::new(text.to_string(), sigs)
    }

    /// Verifies the note against `known` keys, as C2SP `signed-note` says: signatures of unknown keys are
    /// ignored; a signature whose key name and key hash match a known key MUST verify, else the whole note
    /// is refused ([`NoteError::BadSignature`]); with no signature of a known key the note is refused
    /// ([`NoteError::NoKnownSignature`]). Returns one [`Verified`] per known key that signed, in the order
    /// of `known`. A key of kind [`SigKind::Cosignature`] verifies the signed time as well.
    ///
    /// Seam: first caller is S45-INT-1 (AzCtl checks its own note before it publishes).
    pub fn verify(&self, known: &[Verifier]) -> Result<Vec<Verified>, NoteError> {
        let mut out = Vec::new();
        for verifier in known {
            let key = Known::new(verifier).map_err(|_| NoteError::BadKey)?;
            let Some(line) = key.find(self) else {
                continue;
            };
            match key.check(&self.text, line) {
                Some(Check::Log) => out.push(Verified {
                    name: verifier.name.clone(),
                    kind: verifier.kind,
                    time: None,
                }),
                Some(Check::Cosigned(time)) => out.push(Verified {
                    name: verifier.name.clone(),
                    kind: verifier.kind,
                    time: Some(time),
                }),
                None => {
                    return Err(NoteError::BadSignature {
                        name: verifier.name.clone(),
                    })
                }
            }
        }
        if out.is_empty() {
            return Err(NoteError::NoKnownSignature);
        }
        Ok(out)
    }
}

fn parse_signature_line(line: &str) -> Result<NoteSig, NoteError> {
    let rest = line
        .strip_prefix(DASH)
        .ok_or(NoteError::NotASignatureLine)?;
    let (name, encoded) = rest.rsplit_once(' ').ok_or(NoteError::MissingSpace)?;
    let name = KeyName::new(name).map_err(NoteError::BadName)?;
    // Bound the decode before it runs: 4 bytes of key hash and the longest payload.
    if encoded.len() > (4 + MAX_SIG_PAYLOAD).div_ceil(3) * 4 {
        return Err(NoteError::TooLarge);
    }
    let raw = b64_decode(encoded).ok_or(NoteError::BadBase64)?;
    let (hash, payload) = raw.split_at_checked(4).ok_or(NoteError::ShortPayload)?;
    let hash: [u8; 4] = hash.try_into().map_err(|_| NoteError::ShortPayload)?;
    NoteSig::new(name, KeyHash::from_bytes(hash), payload.to_vec())
}

/// What a verifier key checks: a name, a signature type and the 32-byte Ed25519 public key (a "vkey" of C2SP
/// `signed-note`). It is data, compiled into every verifier through the signed updater chain (drift row
/// X-DELTA-W1S45C-4); [`Policy::new`](crate::Policy::new) and [`Witness::new`](crate::Witness::new) check
/// that the key is usable.
///
/// Seam: first callers are the policy data of S45-INT-2 and S45-INT-3.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Verifier {
    /// The key's name.
    pub name: KeyName,
    /// The signature type: [`SigKind::Ed25519`] for the log, [`SigKind::Cosignature`] for a witness.
    pub kind: SigKind,
    /// The Ed25519 public key.
    pub public: [u8; 32],
}

impl Verifier {
    /// The key hash of this key: `SHA-256(name || 0x0A || type || public key)[..4]`.
    pub fn key_hash(&self) -> KeyHash {
        KeyHash::of(&self.name, self.kind, &self.public)
    }

    /// The verifier key text of C2SP `signed-note`: `<name>+<hex key hash>+<base64(type || public key)>`.
    /// Seam: first callers are the policy data of S45-INT-2 and S45-INT-3 (one vkey line per key).
    pub fn vkey(&self) -> String {
        let mut material = Vec::with_capacity(33);
        material.push(self.kind.type_byte());
        material.extend_from_slice(&self.public);
        format!(
            "{}+{}+{}",
            self.name,
            hex_encode(self.key_hash().as_bytes()),
            b64_encode(&material)
        )
    }

    /// Reads a verifier key text; the hex key hash in the text must be the one computed from the name, the
    /// type and the key, and the type must be `0x01` or `0x04`. Seam: first callers are the policy data of
    /// S45-INT-2 and S45-INT-3.
    pub fn from_vkey(text: &str) -> Result<Verifier, NoteError> {
        let mut parts = text.splitn(3, '+');
        let (Some(name), Some(id), Some(material)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(NoteError::BadVkey);
        };
        let name = KeyName::new(name).map_err(NoteError::BadName)?;
        let id: [u8; 4] = hex_decode(id).ok_or(NoteError::BadVkey)?;
        let material = b64_decode(material).ok_or(NoteError::BadVkey)?;
        let (kind, public) = material.split_first().ok_or(NoteError::BadVkey)?;
        let kind = SigKind::from_type_byte(*kind).ok_or(NoteError::BadVkey)?;
        let public: [u8; 32] = public.try_into().map_err(|_| NoteError::BadVkey)?;
        let verifier = Verifier { name, kind, public };
        if verifier.key_hash() != KeyHash::from_bytes(id) {
            return Err(NoteError::BadVkey);
        }
        Ok(verifier)
    }
}

/// A key that verified on a note: which key, and for a cosignature the time it signed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Verified {
    /// The key's name.
    pub name: KeyName,
    /// The signature type that verified.
    pub kind: SigKind,
    /// The time a cosignature carries; `None` for a log signature.
    pub time: Option<UnixTime>,
}

/// A key that signs a note's text with an Ed25519 signature (type `0x01`): the log's key. The caller that
/// holds the key in the Keychain implements this trait (S45-INT-1); [`MemoryKey`] is the one that holds a
/// seed in memory.
///
/// Seam: first caller is `checkpoint::sign_checkpoint`, for AzCtl's release flow (S45-INT-1).
pub trait NoteSigner {
    /// The key's name (the log's origin is recommended: C2SP `tlog-checkpoint`).
    fn name(&self) -> &KeyName;
    /// The key hash of this key for type `0x01`.
    fn key_hash(&self) -> KeyHash;
    /// The Ed25519 signature over `text` (RFC 8032: deterministic, no randomness).
    fn sign_text(&self, text: &[u8]) -> Result<[u8; 64], SignError>;
}

/// A signing key held in memory, made from a 32-byte seed: for tests and for tools that hold a key of their
/// own. It implements [`NoteSigner`] (type `0x01`) and [`Cosigner`](crate::Cosigner) (type `0x04`); one
/// seed under one name has two key hashes (T9). The seed lives in the Ed25519 key, which wipes itself on
/// drop; `Debug` prints the name and nothing secret; the type is not `Clone`, has no `Default` and no `From`
/// (planning/architecture.md section 1.4: secrets only in witness types).
///
/// Seam: first non-test callers are the tools of S45-INT-1; AzCtl's own signer is Keychain-backed.
pub struct MemoryKey {
    name: KeyName,
    key: SigningKey,
}

impl MemoryKey {
    /// A key from its seed. The caller's copy of the seed stays the caller's to wipe.
    pub fn from_seed(name: KeyName, seed: [u8; 32]) -> MemoryKey {
        MemoryKey {
            name,
            key: SigningKey::from_bytes(&seed),
        }
    }

    /// The key's name.
    pub fn name(&self) -> &KeyName {
        &self.name
    }

    /// The Ed25519 public key.
    pub fn public(&self) -> [u8; 32] {
        self.key.verifying_key().to_bytes()
    }

    /// The key hash under the signature type `kind`.
    pub fn hash_for(&self, kind: SigKind) -> KeyHash {
        KeyHash::of(&self.name, kind, &self.public())
    }

    /// What verifies this key's signatures of type `kind`.
    pub fn verifier(&self, kind: SigKind) -> Verifier {
        Verifier {
            name: self.name.clone(),
            kind,
            public: self.public(),
        }
    }

    /// The raw Ed25519 signature over `message`.
    pub(crate) fn sign_bytes(&self, message: &[u8]) -> [u8; 64] {
        self.key.sign(message).to_bytes()
    }
}

impl fmt::Debug for MemoryKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryKey")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl NoteSigner for MemoryKey {
    fn name(&self) -> &KeyName {
        &self.name
    }

    fn key_hash(&self) -> KeyHash {
        self.hash_for(SigKind::Ed25519)
    }

    fn sign_text(&self, text: &[u8]) -> Result<[u8; 64], SignError> {
        Ok(self.sign_bytes(text))
    }
}

/// What a successful signature check found.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Check {
    /// The log's own signature over the text verified.
    Log,
    /// A cosignature verified, signed at this time.
    Cosigned(UnixTime),
}

/// A verifier whose public key was parsed once: the form `Policy` and `Witness` keep, so a key that is
/// not on the curve or is of small order is refused when the policy is built, not at every check.
#[derive(Clone, Debug)]
pub(crate) struct Known {
    verifier: Verifier,
    hash: KeyHash,
    key: VerifyingKey,
}

impl Known {
    /// Parses the key of `verifier`; a point that does not decompress or is of small order is refused.
    pub(crate) fn new(verifier: &Verifier) -> Result<Known, NoteError> {
        let key = VerifyingKey::from_bytes(&verifier.public).map_err(|_| NoteError::BadKey)?;
        if key.is_weak() {
            return Err(NoteError::BadKey);
        }
        Ok(Known {
            verifier: verifier.clone(),
            hash: verifier.key_hash(),
            key,
        })
    }

    pub(crate) fn verifier(&self) -> &Verifier {
        &self.verifier
    }

    pub(crate) fn name(&self) -> &KeyName {
        &self.verifier.name
    }

    /// The signature line of this key in `note`: the one whose name AND key hash match.
    pub(crate) fn find<'n>(&self, note: &'n Note) -> Option<&'n NoteSig> {
        note.sigs
            .iter()
            .find(|sig| sig.name == self.verifier.name && sig.hash == self.hash)
    }

    /// Checks the signature line `sig` of this key over the note text: a type `0x01` key checks 64 bytes
    /// over the text; a type `0x04` key checks 72 bytes (a time and a signature) over the cosignature
    /// message of that time (T8, T10). `None` when the signature does not verify.
    pub(crate) fn check(&self, text: &str, sig: &NoteSig) -> Option<Check> {
        match self.verifier.kind {
            SigKind::Ed25519 => {
                let raw: [u8; 64] = sig.payload.as_slice().try_into().ok()?;
                self.verify(text.as_bytes(), &raw)?;
                Some(Check::Log)
            }
            SigKind::Cosignature => {
                let (time, raw) = sig.payload.split_at_checked(8)?;
                let time: [u8; 8] = time.try_into().ok()?;
                let raw: [u8; 64] = raw.try_into().ok()?;
                let time = UnixTime::from_secs(u64::from_be_bytes(time));
                self.verify(&cosignature_message(text, time), &raw)?;
                Some(Check::Cosigned(time))
            }
        }
    }

    /// RFC 8032 verification that also refuses a signature made with a small-order component (the strict
    /// form of `ed25519-dalek`).
    fn verify(&self, message: &[u8], raw: &[u8; 64]) -> Option<()> {
        self.key
            .verify_strict(message, &Signature::from_bytes(raw))
            .ok()
    }
}
