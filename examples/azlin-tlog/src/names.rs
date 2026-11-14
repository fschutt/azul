//! The names of the log's vocabulary: key names and log origins (C2SP `signed-note` and `tlog-cosignature`),
//! the 4-byte key hash and the two signature types (planning/architecture.md section 2.20).
//!
//! A name is checked once, when it is made, so the rest of the crate can trust it: non-empty, at most
//! [`MAX_NAME_BYTES`] bytes (the cosigner-name limit of `tlog-cosignature`), no Unicode space, no plus
//! sign, no control character below U+0020 (`signed-note`: key names "MUST NOT contain any Unicode spaces
//! or plus (U+002B) characters"; `tlog-cosignature`: "containing no Unicode spaces, plus characters
//! (U+002B), or control characters (those below U+0020)"). A log's origin is built by the same rule:
//! "Logs are also identified by strings following the above construction".

use std::fmt;

use sha2::{Digest, Sha256};

/// The longest key name or origin in bytes (C2SP `tlog-cosignature`: "at most 255 bytes long"; the brief's
/// 64 and 128 are drift row X-DELTA-W1S45C-9).
pub const MAX_NAME_BYTES: usize = 255;

/// The log origin of Azlin's release log (architecture 2.20), the default of this crate.
pub const DEFAULT_ORIGIN: &str = "azlin.io/release-log/v1";

/// Why a key name or an origin is refused. Never a string: a caller decides on the variant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NameError {
    /// The name is empty.
    Empty,
    /// The name is longer than [`MAX_NAME_BYTES`].
    TooLong,
    /// The name contains a Unicode space (a space would end the name in a signature line).
    Space,
    /// The name contains a plus sign (it separates the parts of a verifier key).
    Plus,
    /// The name contains a control character below U+0020.
    Control,
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            NameError::Empty => "the name is empty",
            NameError::TooLong => "the name is longer than 255 bytes",
            NameError::Space => "the name contains a space",
            NameError::Plus => "the name contains a plus sign",
            NameError::Control => "the name contains a control character",
        })
    }
}

impl std::error::Error for NameError {}

fn check_name(name: &str) -> Result<(), NameError> {
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(NameError::TooLong);
    }
    for c in name.chars() {
        if c == '+' {
            return Err(NameError::Plus);
        }
        if c.is_whitespace() {
            return Err(NameError::Space);
        }
        if u32::from(c) < 0x20 {
            return Err(NameError::Control);
        }
    }
    Ok(())
}

/// The name of a signing key as a signature line carries it: a schema-less URL such as
/// `witness.example/w1` is recommended (C2SP `signed-note`).
///
/// Caller: `note::Verifier`, `note::NoteSig`, `note::MemoryKey` and every signature line of a note.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct KeyName(String);

impl KeyName {
    /// A key name, checked once (see the module documentation).
    pub fn new(name: &str) -> Result<KeyName, NameError> {
        check_name(name)?;
        Ok(KeyName(name.to_string()))
    }

    /// The name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for KeyName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The identifier of a log: the first line of its checkpoints (C2SP `tlog-checkpoint`, as defined in
/// `tlog-cosignature`). Azlin's own log is [`Origin::azlin`].
///
/// Caller: `checkpoint::Checkpoint`, `policy::LogKey`, `witness::Witness` (one head per origin),
/// `log::Log`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Origin(String);

impl Origin {
    /// An origin, checked once (the same rule as a key name).
    pub fn new(origin: &str) -> Result<Origin, NameError> {
        check_name(origin)?;
        Ok(Origin(origin.to_string()))
    }

    /// Azlin's release log, [`DEFAULT_ORIGIN`] (`azlin.io/release-log/v1`).
    pub fn azlin() -> Origin {
        Origin(DEFAULT_ORIGIN.to_string())
    }

    /// The origin line.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The signature types of this crate, as the byte of the key hash names them (C2SP `signed-note`, signature
/// types): the log signs with [`SigKind::Ed25519`], a witness cosigns with [`SigKind::Cosignature`]. The
/// post-quantum type `0x06` of the current C2SP texts is not built (drift row X-DELTA-W1S45C-10).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SigKind {
    /// Type `0x01`: an Ed25519 signature over the note text (the log's own signature).
    Ed25519,
    /// Type `0x04`: a timestamped Ed25519 checkpoint cosignature (`tlog-cosignature`, "cosignature/v1").
    Cosignature,
}

impl SigKind {
    /// The signature type byte of the key hash.
    pub const fn type_byte(self) -> u8 {
        match self {
            SigKind::Ed25519 => 0x01,
            SigKind::Cosignature => 0x04,
        }
    }

    /// The kind a type byte names; `None` for any other type.
    pub const fn from_type_byte(byte: u8) -> Option<SigKind> {
        match byte {
            0x01 => Some(SigKind::Ed25519),
            0x04 => Some(SigKind::Cosignature),
            _ => None,
        }
    }
}

/// The 4-byte key identifier of a signature line: `SHA-256(name || 0x0A || type || public key)[..4]`
/// (C2SP `signed-note`). It is an identifier, not a strong hash: a verifier is configured with the key and
/// checks the signature (T3).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyHash([u8; 4]);

impl KeyHash {
    /// The key hash of a key `public` with the name `name` under the signature type `kind`.
    pub fn of(name: &KeyName, kind: SigKind, public: &[u8; 32]) -> KeyHash {
        let mut hasher = Sha256::new();
        hasher.update(name.as_str().as_bytes());
        hasher.update([0x0A, kind.type_byte()]);
        hasher.update(public);
        let digest: [u8; 32] = hasher.finalize().into();
        let [a, b, c, d, ..] = digest;
        KeyHash([a, b, c, d])
    }

    /// A key hash from its 4 bytes (as a signature line carries them).
    pub const fn from_bytes(bytes: [u8; 4]) -> KeyHash {
        KeyHash(bytes)
    }

    /// The 4 bytes.
    pub const fn as_bytes(&self) -> &[u8; 4] {
        &self.0
    }
}

impl fmt::Display for KeyHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for KeyHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
