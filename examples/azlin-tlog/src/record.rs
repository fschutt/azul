//! The release record: the leaf of the log (planning/architecture.md section 2.20: "every signed release (node
//! images, the Workers deployed onto customers' accounts, the `drive.azlin.io` bundle, the apps) is appended
//! to Azlin's public log"; point 3: "A release marked as a security fix waits 24 hours, never less").
//!
//! The plan does not say what a leaf is. This crate PROPOSES the record below (drift row X-DELTA-W1S45C-1: it
//! needs the owner's yes, because the waiting period of S48 reads `security_fix` from it). It is line-oriented
//! UTF-8, one canonical form, every line ends in `\n`, the keys come in exactly this order and there is no
//! other line (T21, T22):
//!
//! ```text
//! azlin-release-v1
//! kind=<node-image|worker|browser-bundle|desktop-app|phone-app>
//! name=<[a-z0-9._-]{1,64}>
//! version=<[0-9A-Za-z.+_-]{1,64}>
//! channel=release
//! arch=<x86_64|arm64|any>
//! digest=sha256:<64 lower-case hex digits>
//! signer=<16 lower-case hex digits>
//! security_fix=<0|1>
//! ```
//!
//! `channel=release` is the only channel: the public log holds release builds only, so a record of a `dev`
//! build is refused ([`RecordError::NotRelease`]). `digest` is the SHA-256 of the signed manifest or release
//! statement (for a node image `azlin_proto::image::ImageManifest::sha256_of(manifest_bytes)`); `signer` is
//! the first 8 bytes of the SHA-256 of the signing key. The leaf data is exactly these bytes; the leaf hash is
//! `azlin_merkle::leaf_hash` of them ([`ReleaseRecord::leaf_hash`]), so every field is bound by the tree
//! (T23).
//!
//! Seam: first callers are `log::Log::append` (S45-INT-1: AzCtl's `release sign|publish`), the node's check
//! 8 (S45-INT-2) and the waiting period of S48, which reads `security_fix`.

use std::fmt;

use azlin_merkle::Hash;
use sha2::{Digest as _, Sha256};

use crate::codec::{hex_decode, hex_encode};

/// The longest record in bytes (brief section 5; a valid record is under 300).
pub const MAX_RECORD_BYTES: usize = 4096;

/// The first line of every record.
pub const RECORD_HEADER: &str = "azlin-release-v1";

/// The longest name or version in bytes.
const MAX_FIELD_BYTES: usize = 64;

/// The line of a record a refusal is about.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    /// `kind=`.
    Kind,
    /// `name=`.
    Name,
    /// `version=`.
    Version,
    /// `channel=`.
    Channel,
    /// `arch=`.
    Arch,
    /// `digest=`.
    Digest,
    /// `signer=`.
    Signer,
    /// `security_fix=`.
    SecurityFix,
}

impl Field {
    /// The key of the line.
    pub const fn key(self) -> &'static str {
        match self {
            Field::Kind => "kind",
            Field::Name => "name",
            Field::Version => "version",
            Field::Channel => "channel",
            Field::Arch => "arch",
            Field::Digest => "digest",
            Field::Signer => "signer",
            Field::SecurityFix => "security_fix",
        }
    }
}

/// Why a record is refused. Never a string.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RecordError {
    /// The record is longer than [`MAX_RECORD_BYTES`].
    TooLarge,
    /// The record is not valid UTF-8.
    NotUtf8,
    /// The record does not end in a newline.
    NoFinalNewline,
    /// The first line is not `azlin-release-v1` (a carriage return before the newline lands here).
    Header,
    /// The line for this field is missing or has another key: an unknown key, a key out of order or a
    /// duplicate.
    Line(Field),
    /// The value of this field is not in its grammar.
    Value(Field),
    /// The channel is not `release`: a `dev` build is not logged.
    NotRelease,
    /// A line after the last field.
    Trailing,
}

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecordError::TooLarge => f.write_str("the record is longer than 4096 bytes"),
            RecordError::NotUtf8 => f.write_str("the record is not valid UTF-8"),
            RecordError::NoFinalNewline => f.write_str("the record does not end in a newline"),
            RecordError::Header => f.write_str("the first line is not azlin-release-v1"),
            RecordError::Line(field) => {
                write!(f, "the {} line is missing or out of order", field.key())
            }
            RecordError::Value(field) => write!(f, "the value of {} is refused", field.key()),
            RecordError::NotRelease => f.write_str("only release builds are logged"),
            RecordError::Trailing => f.write_str("a line after the last field"),
        }
    }
}

impl std::error::Error for RecordError {}

/// What a release is: the five kinds of signed release architecture 2.20 names.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SubjectKind {
    /// A node image (`node-image`).
    NodeImage,
    /// A Worker AzWorkspace deploys onto a customer's account (`worker`).
    Worker,
    /// The browser bundle of `drive.azlin.io` (`browser-bundle`).
    BrowserBundle,
    /// A desktop app (`desktop-app`).
    DesktopApp,
    /// A phone app (`phone-app`).
    PhoneApp,
}

impl SubjectKind {
    /// The word of the record line.
    pub const fn as_str(self) -> &'static str {
        match self {
            SubjectKind::NodeImage => "node-image",
            SubjectKind::Worker => "worker",
            SubjectKind::BrowserBundle => "browser-bundle",
            SubjectKind::DesktopApp => "desktop-app",
            SubjectKind::PhoneApp => "phone-app",
        }
    }

    /// The kind a word names.
    pub fn from_text(text: &str) -> Option<SubjectKind> {
        match text {
            "node-image" => Some(SubjectKind::NodeImage),
            "worker" => Some(SubjectKind::Worker),
            "browser-bundle" => Some(SubjectKind::BrowserBundle),
            "desktop-app" => Some(SubjectKind::DesktopApp),
            "phone-app" => Some(SubjectKind::PhoneApp),
            _ => None,
        }
    }
}

/// The CPU architecture a release is for.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Arch {
    /// `x86_64`.
    X86_64,
    /// `arm64`.
    Arm64,
    /// `any`: not tied to an architecture (a bundle, a Worker).
    Any,
}

impl Arch {
    /// The word of the record line.
    pub const fn as_str(self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64",
            Arch::Arm64 => "arm64",
            Arch::Any => "any",
        }
    }

    /// The architecture a word names.
    pub fn from_text(text: &str) -> Option<Arch> {
        match text {
            "x86_64" => Some(Arch::X86_64),
            "arm64" => Some(Arch::Arm64),
            "any" => Some(Arch::Any),
            _ => None,
        }
    }
}

/// The name of a release: `[a-z0-9._-]{1,64}`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Name(String);

impl Name {
    /// A name, checked once.
    pub fn new(text: &str) -> Result<Name, RecordError> {
        let valid = !text.is_empty()
            && text.len() <= MAX_FIELD_BYTES
            && text.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
            });
        if !valid {
            return Err(RecordError::Value(Field::Name));
        }
        Ok(Name(text.to_string()))
    }

    /// The name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The version of a release: `[0-9A-Za-z.+_-]{1,64}`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Version(String);

impl Version {
    /// A version, checked once.
    pub fn new(text: &str) -> Result<Version, RecordError> {
        let valid = !text.is_empty()
            && text.len() <= MAX_FIELD_BYTES
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'_' | b'-'));
        if !valid {
            return Err(RecordError::Value(Field::Version));
        }
        Ok(Version(text.to_string()))
    }

    /// The version.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The SHA-256 of the signed manifest or release statement of a release; in the record as
/// `sha256:<64 lower-case hex digits>`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest([u8; 32]);

impl Digest {
    /// The SHA-256 of `bytes`. Seam: first callers are S45-INT-1 (AzCtl digests the signed manifest or release
    /// statement it logs) and the apps' start check of their own build (architecture 2.20 point 6).
    pub fn of(bytes: &[u8]) -> Digest {
        Digest(Sha256::digest(bytes).into())
    }

    /// A digest from its 32 bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Digest {
        Digest(bytes)
    }

    /// The 32 bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Reads the `sha256:<hex>` form: 64 lower-case hex digits.
    pub fn from_text(text: &str) -> Result<Digest, RecordError> {
        text.strip_prefix("sha256:")
            .and_then(hex_decode::<32>)
            .map(Digest)
            .ok_or(RecordError::Value(Field::Digest))
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sha256:{}", hex_encode(&self.0))
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Which key signed the release: the first 8 bytes of the SHA-256 of the signing key; in the record as 16
/// lower-case hex digits.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SignerId([u8; 8]);

impl SignerId {
    /// The id of the signing key `public` (its bytes as the key's own encoding gives them). Seam: first caller
    /// is S45-INT-1 (AzCtl names the release key that signed the release it logs).
    pub fn of_public_key(public: &[u8]) -> SignerId {
        let digest: [u8; 32] = Sha256::digest(public).into();
        let [a, b, c, d, e, f, g, h, ..] = digest;
        SignerId([a, b, c, d, e, f, g, h])
    }

    /// An id from its 8 bytes.
    pub const fn from_bytes(bytes: [u8; 8]) -> SignerId {
        SignerId(bytes)
    }

    /// The 8 bytes.
    pub const fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }

    /// Reads the 16 lower-case hex digits.
    pub fn from_text(text: &str) -> Result<SignerId, RecordError> {
        hex_decode::<8>(text)
            .map(SignerId)
            .ok_or(RecordError::Value(Field::Signer))
    }
}

impl fmt::Display for SignerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex_encode(&self.0))
    }
}

impl fmt::Debug for SignerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// What makes a release one release: the log holds one record for each (kind, name, version, arch) and
/// refuses a second with other bytes (T25).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ReleaseKey {
    /// The kind of the release.
    pub kind: SubjectKind,
    /// The name.
    pub name: Name,
    /// The version.
    pub version: Version,
    /// The architecture.
    pub arch: Arch,
}

/// A release as the log records it (see the module documentation).
///
/// Seam: first callers are `log::Log::append` (S45-INT-1) and `bundle::verify_bundle` (S45-INT-2,
/// S45-INT-3).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ReleaseRecord {
    /// The kind of the release.
    pub kind: SubjectKind,
    /// The name.
    pub name: Name,
    /// The version.
    pub version: Version,
    /// The architecture.
    pub arch: Arch,
    /// The digest of the signed manifest or release statement.
    pub digest: Digest,
    /// The key that signed the release.
    pub signer: SignerId,
    /// A security fix waits 24 hours, any other release 72 hours (architecture 2.20 point 3).
    pub security_fix: bool,
}

impl ReleaseRecord {
    /// The canonical bytes: the leaf data of the log (T21).
    pub fn to_bytes(&self) -> Vec<u8> {
        format!(
            "{RECORD_HEADER}\nkind={}\nname={}\nversion={}\nchannel=release\narch={}\ndigest={}\nsigner={}\nsecurity_fix={}\n",
            self.kind.as_str(),
            self.name,
            self.version,
            self.arch.as_str(),
            self.digest,
            self.signer,
            u8::from(self.security_fix),
        )
        .into_bytes()
    }

    /// Parses the canonical bytes strictly (T22): this form and no other. `parse(to_bytes(r)) == r`.
    pub fn parse(bytes: &[u8]) -> Result<ReleaseRecord, RecordError> {
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(RecordError::TooLarge);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| RecordError::NotUtf8)?;
        let body = text.strip_suffix('\n').ok_or(RecordError::NoFinalNewline)?;
        let mut lines = body.split('\n');
        if lines.next() != Some(RECORD_HEADER) {
            return Err(RecordError::Header);
        }
        let kind = SubjectKind::from_text(next_value(&mut lines, Field::Kind)?)
            .ok_or(RecordError::Value(Field::Kind))?;
        let name = Name::new(next_value(&mut lines, Field::Name)?)?;
        let version = Version::new(next_value(&mut lines, Field::Version)?)?;
        if next_value(&mut lines, Field::Channel)? != "release" {
            return Err(RecordError::NotRelease);
        }
        let arch = Arch::from_text(next_value(&mut lines, Field::Arch)?)
            .ok_or(RecordError::Value(Field::Arch))?;
        let digest = Digest::from_text(next_value(&mut lines, Field::Digest)?)?;
        let signer = SignerId::from_text(next_value(&mut lines, Field::Signer)?)?;
        let security_fix = match next_value(&mut lines, Field::SecurityFix)? {
            "0" => false,
            "1" => true,
            _ => return Err(RecordError::Value(Field::SecurityFix)),
        };
        if lines.next().is_some() {
            return Err(RecordError::Trailing);
        }
        Ok(ReleaseRecord {
            kind,
            name,
            version,
            arch,
            digest,
            signer,
            security_fix,
        })
    }

    /// The leaf hash of the record: `azlin_merkle::leaf_hash` of [`ReleaseRecord::to_bytes`], so the tree
    /// binds every field (T23).
    pub fn leaf_hash(&self) -> Hash {
        azlin_merkle::leaf_hash(&self.to_bytes())
    }

    /// What makes this release one release.
    pub fn key(&self) -> ReleaseKey {
        ReleaseKey {
            kind: self.kind,
            name: self.name.clone(),
            version: self.version.clone(),
            arch: self.arch,
        }
    }
}

/// The value of the next line, which must be `<key>=<value>` for `field`.
fn next_value<'a>(
    lines: &mut std::str::Split<'a, char>,
    field: Field,
) -> Result<&'a str, RecordError> {
    let line = lines.next().ok_or(RecordError::Line(field))?;
    line.strip_prefix(field.key())
        .and_then(|rest| rest.strip_prefix('='))
        .ok_or(RecordError::Line(field))
}
