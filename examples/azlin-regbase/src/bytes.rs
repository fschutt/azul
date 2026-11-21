//! The bounded value types of the registry's rows (planning/architecture.md section 1.4 rule 4
//! "Everything has a limit, stated up front"; section 5.3 "sealed" columns, the entitlement,
//! `min_client`; section 5.11 `features`, `regions_allowed`; build unit S9, lane W1-S9a).
//!
//! No unbounded `String` or `Vec<u8>` is a column of the registry: every text, blob and set is one
//! of these types, each with a limit named in a constant, a constructor that REFUSES a value above
//! it with a [`BoundError`] (it never truncates), and a `Debug` that prints a length and never a
//! byte of a blob.
//!
//! | type | what | limit |
//! |---|---|---|
//! | [`SealedBlob`] | an opaque sealed envelope the server stores and never opens | [`SEALED_BLOB_MAX`] |
//! | [`SignedBytes`] | a signed structure kept verbatim (the entitlement, a certificate) | [`SIGNED_BYTES_MAX`] |
//! | [`SmallText`] | a word or a short printable ASCII text, no control byte | [`SMALL_TEXT_MAX`] |
//! | [`Features`] | a sorted set of feature words; unknown words are KEPT | [`FEATURES_MAX`] |
//! | [`RegionSet`] | a sorted set of region ids | [`REGIONS_MAX`] |
//! | [`UsedHashes`] | the rotated-away token hashes of a family, oldest first | [`USED_HASHES_KEPT`] |
//! | [`ClientVersion`] | `major.minor.patch`, ordered | - |
//!
//! `PublicKey32` and `Signature64` are in [`crate::sign`]: they are aliases of azlin-keys' types.

use core::fmt;

use azlin_seams::ByteCount;

use crate::error::{BoundError, VersionError};
use crate::ids::TokenHash;

/// The longest sealed blob: a sealed box of a key or a small secret, a device name or a push token
/// sealed to a key (planning/architecture.md section 5.3 `label_sealed`, `push_sealed`,
/// `opaque_setup`).
pub const SEALED_BLOB_MAX: ByteCount = ByteCount::new(SEALED_BLOB_MAX_LEN as u64);

/// [`SEALED_BLOB_MAX`] as a buffer length.
const SEALED_BLOB_MAX_LEN: usize = 512;

/// The longest signed structure kept verbatim: an entitlement or a certificate (planning/
/// architecture.md section 5.11).
pub const SIGNED_BYTES_MAX: ByteCount = ByteCount::new(SIGNED_BYTES_MAX_LEN as u64);

/// [`SIGNED_BYTES_MAX`] as a buffer length.
const SIGNED_BYTES_MAX_LEN: usize = 8 * 1024;

/// The longest [`SmallText`], in bytes.
pub const SMALL_TEXT_MAX: ByteCount = ByteCount::new(SMALL_TEXT_MAX_LEN as u64);

/// [`SMALL_TEXT_MAX`] as a buffer length.
const SMALL_TEXT_MAX_LEN: usize = 64;

/// The most feature words an account can have.
pub const FEATURES_MAX: usize = 32;

/// The most regions an account can be allowed.
pub const REGIONS_MAX: usize = 64;

/// How many rotated-away token hashes a family keeps, oldest first: a hash older than this is no
/// longer recognised, so a token leaked more than this many rotations ago reads as unknown and not
/// as a reuse (the old code's `USED_HASHES_KEPT`; planning/architecture.md section 5.3 `reg_family`,
/// DRIFT X-D-05: "the richer row").
pub const USED_HASHES_KEPT: usize = 20;

/// An opaque sealed envelope: the server stores it and never opens it (planning/architecture.md
/// section 5.3 `reg_wrap`, `label_sealed`, `push_sealed`; section 2.19 "secrets on a node only
/// sealed or in memory"). At least one byte and at most [`SEALED_BLOB_MAX`]. `Debug` prints the
/// length only.
///
/// Caller: azlin-registry's rows (`opaque_setup`, `label_sealed`, `push_sealed`, the sealed key
/// blobs), S9c (push), S9f (reseal).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SealedBlob(Vec<u8>);

impl SealedBlob {
    /// The blob with these bytes; refused when empty or longer than [`SEALED_BLOB_MAX`].
    pub fn new(bytes: Vec<u8>) -> Result<SealedBlob, BoundError> {
        check_bytes("SealedBlob", &bytes, SEALED_BLOB_MAX_LEN)?;
        Ok(SealedBlob(bytes))
    }

    /// The blob with a copy of these bytes; refused as [`SealedBlob::new`].
    pub fn from_slice(bytes: &[u8]) -> Result<SealedBlob, BoundError> {
        SealedBlob::new(bytes.to_vec())
    }

    /// The bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// The number of bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Never true: a blob has at least one byte.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The length only.
impl fmt::Debug for SealedBlob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SealedBlob(len {})", self.0.len())
    }
}

/// A signed structure kept verbatim, such as the latest entitlement (planning/architecture.md
/// section 5.3 `entitlement`: "the latest entitlement, signed by the payments key"). At least one
/// byte and at most [`SIGNED_BYTES_MAX`]. The registry never interprets it: azlin-creds verifies
/// and decodes it. `Debug` prints the length only.
///
/// Caller: `reg_account.entitlement`, `reg_key_epoch.cert` (azlin-registry), S9d, S9f.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SignedBytes(Vec<u8>);

impl SignedBytes {
    /// The bytes as a signed structure; refused when empty or longer than [`SIGNED_BYTES_MAX`].
    pub fn new(bytes: Vec<u8>) -> Result<SignedBytes, BoundError> {
        check_bytes("SignedBytes", &bytes, SIGNED_BYTES_MAX_LEN)?;
        Ok(SignedBytes(bytes))
    }

    /// The bytes with a copy of these; refused as [`SignedBytes::new`].
    pub fn from_slice(bytes: &[u8]) -> Result<SignedBytes, BoundError> {
        SignedBytes::new(bytes.to_vec())
    }

    /// The bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// The number of bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Never true: a signed structure has at least one byte.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The length only.
impl fmt::Debug for SignedBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SignedBytes(len {})", self.0.len())
    }
}

/// The length check of the byte blobs: not empty, not above `max`.
fn check_bytes(what: &'static str, bytes: &[u8], max: usize) -> Result<(), BoundError> {
    if bytes.is_empty() {
        return Err(BoundError::Empty { what });
    }
    if bytes.len() > max {
        return Err(BoundError::TooLong {
            what,
            len: bytes.len(),
            max,
        });
    }
    Ok(())
}

/// A short printable ASCII text: at most [`SMALL_TEXT_MAX`] bytes, each byte `0x20` to `0x7e` (no
/// control byte, nothing above ASCII). It may be empty. Closed sets of words (a log line's detail,
/// an app's name, a feature) are `SmallText`; free text is not a registry column
/// (planning/architecture.md section 5.15 R1 and R3).
///
/// Caller: `LogLine::detail`, `AppFacts::app_name`, [`Features`], S9c (the closed list of details).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SmallText(String);

impl SmallText {
    /// The text; refused above [`SMALL_TEXT_MAX`] or with a byte outside printable ASCII.
    pub fn new(text: &str) -> Result<SmallText, BoundError> {
        if text.len() > SMALL_TEXT_MAX_LEN {
            return Err(BoundError::TooLong {
                what: "SmallText",
                len: text.len(),
                max: SMALL_TEXT_MAX_LEN,
            });
        }
        if !text.bytes().all(|byte| (0x20..=0x7e).contains(&byte)) {
            return Err(BoundError::BadChar { what: "SmallText" });
        }
        Ok(SmallText(text.to_owned()))
    }

    /// The empty text.
    pub const fn empty() -> SmallText {
        SmallText(String::new())
    }

    /// The text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the text is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether the text is a machine word: one to [`SMALL_TEXT_MAX`] lower-case letters, digits or
    /// underscores (the spelling of every closed set of the registry).
    pub fn is_word(&self) -> bool {
        !self.0.is_empty()
            && self
                .0
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    }
}

impl fmt::Display for SmallText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for SmallText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SmallText({:?})", self.0)
    }
}

/// The set of feature words an entitlement grants (planning/architecture.md section 5.11
/// `features`: team, sso, lock, audit, residency_choice, escrow, requests...). Sorted, without
/// duplicates, at most [`FEATURES_MAX`] words, each a machine word ([`SmallText::is_word`]).
/// A word this build does not know is KEPT ("unknown fields ignored", section 5.1 principle 6):
/// the set neither knows nor checks the names, so a newer entitlement's feature survives a round
/// trip through an older node.
///
/// Caller: `reg_account.features` (S9a), S9d (the entitlement path).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Features(Vec<SmallText>);

impl Features {
    /// The set of these words: sorted and deduplicated; refused when a name is not a machine word
    /// or when more than [`FEATURES_MAX`] distinct names remain.
    pub fn new(mut names: Vec<SmallText>) -> Result<Features, BoundError> {
        if names.iter().any(|name| !name.is_word()) {
            return Err(BoundError::BadChar { what: "Features" });
        }
        names.sort();
        names.dedup();
        if names.len() > FEATURES_MAX {
            return Err(BoundError::TooMany {
                what: "Features",
                count: names.len(),
                max: FEATURES_MAX,
            });
        }
        Ok(Features(names))
    }

    /// No features.
    pub const fn empty() -> Features {
        Features(Vec::new())
    }

    /// Whether the set has this word.
    pub fn contains(&self, name: &str) -> bool {
        self.0.iter().any(|word| word.as_str() == name)
    }

    /// The words in order.
    pub fn as_slice(&self) -> &[SmallText] {
        &self.0
    }

    /// The number of words.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A region (data residency, planning/architecture.md section 5.3 `region`): a number the
/// operator's region table assigns.
///
/// Caller: `reg_account.region`, [`RegionSet`], S9d.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegionId(u16);

impl RegionId {
    /// The region with this number.
    pub const fn new(number: u16) -> RegionId {
        RegionId(number)
    }

    /// The number of the region.
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl fmt::Display for RegionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for RegionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RegionId({})", self.0)
    }
}

/// The regions an entitlement allows an account to choose (planning/architecture.md section 5.11
/// `regions_allowed`). Sorted, without duplicates, at most [`REGIONS_MAX`].
///
/// Caller: `reg_account.regions_allowed` (S9a), S9d.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct RegionSet(Vec<RegionId>);

impl RegionSet {
    /// The set of these regions: sorted and deduplicated; refused above [`REGIONS_MAX`] distinct
    /// regions.
    pub fn new(mut regions: Vec<RegionId>) -> Result<RegionSet, BoundError> {
        regions.sort();
        regions.dedup();
        if regions.len() > REGIONS_MAX {
            return Err(BoundError::TooMany {
                what: "RegionSet",
                count: regions.len(),
                max: REGIONS_MAX,
            });
        }
        Ok(RegionSet(regions))
    }

    /// No regions.
    pub const fn empty() -> RegionSet {
        RegionSet(Vec::new())
    }

    /// Whether the set has this region.
    pub fn contains(&self, region: RegionId) -> bool {
        self.0.binary_search(&region).is_ok()
    }

    /// The regions in order.
    pub fn as_slice(&self) -> &[RegionId] {
        &self.0
    }

    /// The number of regions.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The rotated-away token hashes of a family, OLDEST FIRST, at most [`USED_HASHES_KEPT`]
/// (planning/architecture.md section 5.3 `reg_family`, DRIFT X-D-05). The newest one is the token
/// a lost refresh answer would have replaced. `Debug` prints the count only.
///
/// Caller: `FamilyRow::used` and azlin-registry's `family` rules (rotate, reissue).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct UsedHashes(Vec<TokenHash>);

impl UsedHashes {
    /// No hashes yet.
    pub const fn empty() -> UsedHashes {
        UsedHashes(Vec::new())
    }

    /// These hashes, oldest first; refused above [`USED_HASHES_KEPT`] (never truncated).
    pub fn new(hashes: Vec<TokenHash>) -> Result<UsedHashes, BoundError> {
        if hashes.len() > USED_HASHES_KEPT {
            return Err(BoundError::TooMany {
                what: "UsedHashes",
                count: hashes.len(),
                max: USED_HASHES_KEPT,
            });
        }
        Ok(UsedHashes(hashes))
    }

    /// The hashes, oldest first.
    pub fn as_slice(&self) -> &[TokenHash] {
        &self.0
    }

    /// The number of hashes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The newest hash: the one rotated away last.
    pub fn newest(&self) -> Option<&TokenHash> {
        self.0.last()
    }

    /// These hashes with `hash` as the newest; the oldest ones beyond [`USED_HASHES_KEPT`] fall
    /// away (the rule of a rotation: a hash 21 rotations back is forgotten, not remembered as a
    /// reuse).
    pub fn pushed(&self, hash: TokenHash) -> UsedHashes {
        let mut hashes = self.0.clone();
        hashes.push(hash);
        UsedHashes(drop_oldest(hashes))
    }

    /// These hashes with `hash` just BEFORE the newest one (or alone when there is none): the
    /// lost-answer swap, after which the caller's token stays the newest. The oldest beyond
    /// [`USED_HASHES_KEPT`] fall away.
    pub fn pushed_before_newest(&self, hash: TokenHash) -> UsedHashes {
        let mut hashes = self.0.clone();
        match hashes.pop() {
            Some(newest) => {
                hashes.push(hash);
                hashes.push(newest);
            }
            None => hashes.push(hash),
        }
        UsedHashes(drop_oldest(hashes))
    }
}

/// The oldest hashes beyond [`USED_HASHES_KEPT`] dropped.
fn drop_oldest(mut hashes: Vec<TokenHash>) -> Vec<TokenHash> {
    let excess = hashes.len().saturating_sub(USED_HASHES_KEPT);
    hashes.drain(..excess);
    hashes
}

/// The count only: never a hash.
impl fmt::Debug for UsedHashes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UsedHashes(len {})", self.0.len())
    }
}

/// An app version `major.minor.patch`, ordered by major, then minor, then patch (planning/
/// architecture.md section 5.3 `min_client`: "a workspace minimum app version on top of the global
/// one (`client_too_old`)").
///
/// Caller: `reg_account.min_client`, S9b (`client_too_old`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ClientVersion {
    major: u16,
    minor: u16,
    patch: u16,
}

impl ClientVersion {
    /// The version with these parts.
    pub const fn new(major: u16, minor: u16, patch: u16) -> ClientVersion {
        ClientVersion {
            major,
            minor,
            patch,
        }
    }

    /// The major part.
    pub const fn major(self) -> u16 {
        self.major
    }

    /// The minor part.
    pub const fn minor(self) -> u16 {
        self.minor
    }

    /// The patch part.
    pub const fn patch(self) -> u16 {
        self.patch
    }

    /// The version of this text. Strict: three plain decimal numbers (digits only, no sign, no
    /// leading zero, each below 65,536) separated by single dots.
    pub fn parse(text: &str) -> Result<ClientVersion, VersionError> {
        let mut parts = text.split('.');
        let (Some(major), Some(minor), Some(patch), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(VersionError::Shape);
        };
        Ok(ClientVersion {
            major: part(major)?,
            minor: part(minor)?,
            patch: part(patch)?,
        })
    }
}

/// One strict decimal part of a version.
fn part(text: &str) -> Result<u16, VersionError> {
    let bytes = text.as_bytes();
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(VersionError::Number);
    }
    if bytes.len() > 1 && bytes.first() == Some(&b'0') {
        return Err(VersionError::Number);
    }
    text.parse::<u16>().map_err(|_| VersionError::Number)
}

impl fmt::Display for ClientVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}
