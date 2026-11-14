//! The row of a link: `reg_link` (planning/architecture.md section 5.12): "`reg_link` holds `link_id`,
//! `slug`, `space_id`, `created_by`, `kind`, `manifest_object` (the AZS2 manifest), `grants` (the
//! objects and byte ranges it may read; nothing else in a pack is served), `follow_latest`, the
//! options above, `opaque_record`, `wraps` (for `people` and `approval`), `request_id` (the paired
//! request), `comment_log`, the counters (`downloads`, `bytes_served`, `devices_seen` as hashed
//! tokens, `first_open_at`), `paused`, `revoked_at` and `created_at`".
//!
//! [`LinkRecord`] is that row with its invariants. Its fields are private: a record is made by
//! [`LinkRecord::create`] from the witness of `apply_policy`, read back by [`LinkRecord::from_row`]
//! (which checks every invariant again), and changed only by the functions of `update` (each of them
//! checks the whole row again before it commits, so a refused change leaves the record as it was).
//! [`LinkRow`] is the plain data of the row, for the unit that stores it (S9) and for the golden
//! fixture (the row's wire form is S5's and INT-WIRE's: this crate has no codec).
//!
//! What the record derives instead of storing: `slug` and `kind` are the options' (one source), and
//! `follow_latest` is "the kind is `folder_live`" (a second stored fact could disagree with the kind).
//!
//! **No secret and no clear text is in a record**: the wraps and the OPAQUE record are opaque bytes
//! sealed by their makers, a title and a message are sealed, and the devices seen are HASHED tokens
//! ("`devices_seen` as hashed tokens": [`DeviceToken::seen_by`] hashes a device's token together
//! with the link's id, so the same device is two unrelated values on two links, and the crate has
//! no address type at all).
//!
//! Three times are kept: `created_at` (never changes: a policy's longest life counts from it),
//! `keyed_at` (the creation or the last rotation: the time the current id and key began; the idle
//! expiry counts from the last open or, for a link never opened, from it) and `revoked_at`.
//!
//! Implements planning/architecture.md section 5.12 (`reg_link`, "Nobody at Azlin can peek").

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use azlin_seams::{
    hash32, ByteCount, Entropy, EntropyError, Hash32, KeyId, MemberId, SpaceId, Stamp,
};

use crate::audience::Audience;
use crate::bounds::Bounded;
use crate::id::{PublicLinkId, Slug};
use crate::kind::LinkKind;
use crate::options::{LinkOptions, Sealed, SealedError};
use crate::validate::{rules, Facts, OptionError};

/// The most objects or byte ranges one link may read. The plan gives no limit (1.4 rule 4: everything
/// has a limit, stated up front): the grants are kept in the link's row, which a replicated state
/// machine holds, so they are bounded. An estimate (drift register of lane W1-S16c1).
///
/// Implements planning/architecture.md section 1.4 (rule 4, limits) and section 5.12 (`grants`).
///
/// Caller: `Grants::new`, `validate` (`TooManyGrants`).
pub const MAX_GRANTS: usize = 16_384;

/// The most wraps of the share key one link may hold: one for each named person, and one for each
/// approved requester of an `approval` link. An estimate (drift register of lane W1-S16c1).
///
/// Implements planning/architecture.md section 1.4 (rule 4, limits) and section 5.12 (`wraps`).
///
/// Caller: `validate` (`TooManyWraps`).
pub const MAX_WRAPS: usize = 1_000;

/// The longest sealed wrap of the share key (a hybrid envelope to one member key). An estimate
/// (drift register of lane W1-S16c1).
///
/// Implements planning/architecture.md section 1.4 (rule 4, limits) and section 5.12 (`wraps`).
///
/// Caller: `Wrap::new`.
pub const WRAP_MAX_LEN: usize = 4096;

/// The longest OPAQUE record of a password (a registration upload is about 200 bytes). An estimate
/// (drift register of lane W1-S16c1).
///
/// Implements planning/architecture.md section 1.4 (rule 4, limits) and section 5.12 ("Passwords
/// with OPAQUE").
///
/// Caller: `OpaqueRecord::new`.
pub const OPAQUE_RECORD_MAX_LEN: usize = 1024;

/// The hash domain of a seen device: [`DeviceToken::seen_by`].
///
/// Implements planning/architecture.md section 5.12 (`devices_seen` as hashed tokens).
///
/// Caller: `DeviceToken::seen_by`.
pub const DEVICE_SEEN_DOMAIN: &str = "azlin-link-device-v1";

/// An id of 16 bytes that another unit owns and this crate only holds: the bytes, a hex `Debug`, no
/// arithmetic.
macro_rules! held_id16 {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; 16]);

        impl $name {
            /// The id with these bytes.
            pub const fn from_bytes(bytes: [u8; 16]) -> $name {
                $name(bytes)
            }

            /// The bytes of the id.
            pub const fn as_bytes(&self) -> &[u8; 16] {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "("))?;
                for byte in &self.0 {
                    write!(f, "{byte:02x}")?;
                }
                write!(f, ")")
            }
        }
    };
}

held_id16! {
    /// The id of the link's paired request (S16f's `reg_request`): the upload-only door that
    /// `allow_upload` and `approval` use. The same 16 bytes as S16f's id (the glue converts).
    ///
    /// Implements planning/architecture.md section 5.12 (`request_id`: "the paired request").
    ///
    /// Caller: `LinkRecord::request_id`, `NewLink`, `KeyBound`; Seam: S16c2 (`CredentialSpec`'s
    /// `request_upload` scope), S16f (`paired_with`, `end_with_link`).
    RequestId
}

held_id16! {
    /// The id of the link's comment log (S16a's append log, 5.13). The same 16 bytes as S16a's id
    /// (the glue converts).
    ///
    /// Implements planning/architecture.md section 5.12 (`comment_log`).
    ///
    /// Caller: `LinkRecord::comment_log`, `NewLink`; Seam: S16c2 (`CredentialSpec`'s `log_append`
    /// scope), S16a (the log itself).
    LogId
}

/// A reference to one stored object (a pack or the AZS2 manifest): the 32-byte hash that names it.
/// Opaque to this crate ("`ObjectRef` and the AZS2 manifest are opaque").
///
/// Implements planning/architecture.md section 5.12 (`manifest_object`, `grants`).
///
/// Caller: `LinkRecord::manifest_object`, `Grant`, `repoint`; Seam: S14 (the content crate names its
/// objects), S9 (the node serves what the grants name).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectRef(Hash32);

impl ObjectRef {
    /// The reference to the object with this hash.
    pub const fn from_hash(hash: Hash32) -> ObjectRef {
        ObjectRef(hash)
    }

    /// The hash that names the object.
    pub const fn as_hash(&self) -> &Hash32 {
        &self.0
    }
}

impl fmt::Debug for ObjectRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ObjectRef({})", self.0)
    }
}

/// Why a list of grants (or one range of it) is no grant.
///
/// Implements planning/architecture.md section 5.12 (`grants`) and section 1.4 (rule 4).
///
/// Caller: `ByteRange::new`, `Grants::new`; Seam: LINKS-NODE (`link_create` refuses with it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GrantError {
    /// More grants than [`MAX_GRANTS`].
    #[error("a link reads at most {} objects or ranges, not {count}", MAX_GRANTS)]
    TooMany {
        /// The number given.
        count: usize,
    },
    /// A byte range of no bytes.
    #[error("a byte range has at least one byte")]
    EmptyRange,
    /// A byte range whose end does not fit a size.
    #[error("a byte range ends beyond the largest size")]
    RangeOverflow,
}

/// A range of bytes of an object: `start` and `size`, at least one byte, ending within a size.
///
/// Implements planning/architecture.md section 5.12 (`grants`: "the objects and byte ranges it may
/// read").
///
/// Caller: `Grant`; Seam: S16c2 (the credential's `Read` scope), S9 (a ranged read).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ByteRange {
    start: ByteCount,
    size: ByteCount,
}

impl ByteRange {
    /// The range of `size` bytes from `start`; an error for no bytes and for an end beyond the
    /// largest size.
    pub fn new(start: ByteCount, size: ByteCount) -> Result<ByteRange, GrantError> {
        if size.is_zero() {
            return Err(GrantError::EmptyRange);
        }
        if start.checked_add(size).is_none() {
            return Err(GrantError::RangeOverflow);
        }
        Ok(ByteRange { start, size })
    }

    /// The first byte.
    pub const fn start(&self) -> ByteCount {
        self.start
    }

    /// The number of bytes.
    pub const fn size(&self) -> ByteCount {
        self.size
    }

    /// The byte after the last (never beyond the largest size: `new` checked it).
    pub fn end(&self) -> ByteCount {
        self.start.saturating_add(self.size)
    }
}

/// One thing a link may read: a whole object (`range` is `None`) or a range of it.
///
/// Implements planning/architecture.md section 5.12 (`grants`: "the objects and byte ranges it may
/// read; nothing else in a pack is served").
///
/// Caller: `Grants`; Seam: S16c2 (the credential is "limited to the link's grants"), S9 (the node
/// serves nothing else).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Grant {
    /// The object.
    pub object: ObjectRef,
    /// The range of the object, or the whole of it.
    pub range: Option<ByteRange>,
}

/// What a link may read: at most [`MAX_GRANTS`] grants, in the order given.
///
/// Implements planning/architecture.md section 5.12 (`grants`).
///
/// Caller: `LinkRecord::grants`, `NewLink`, `repoint`, `KeyBound`; Seam: S16c2 (`CredentialSpec`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grants(Vec<Grant>);

impl Grants {
    /// The grants; an error when there are more than [`MAX_GRANTS`].
    pub fn new(grants: Vec<Grant>) -> Result<Grants, GrantError> {
        if grants.len() > MAX_GRANTS {
            return Err(GrantError::TooMany {
                count: grants.len(),
            });
        }
        Ok(Grants(grants))
    }

    /// The grants.
    pub fn as_slice(&self) -> &[Grant] {
        &self.0
    }

    /// The number of grants.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no grants.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Why bytes are no OPAQUE record.
///
/// Implements planning/architecture.md section 5.12 ("Passwords with OPAQUE").
///
/// Caller: `OpaqueRecord::new`; Seam: LINKS-NODE (`link_create` refuses with it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OpaqueRecordError {
    /// No bytes.
    #[error("an OPAQUE record has bytes")]
    Empty,
    /// More than [`OPAQUE_RECORD_MAX_LEN`] bytes.
    #[error(
        "an OPAQUE record is at most {} bytes, not {len}",
        OPAQUE_RECORD_MAX_LEN
    )]
    TooLong {
        /// The length given.
        len: usize,
    },
}

/// The OPAQUE record of a link's password (the registration the creating device made under the
/// account's OPAQUE setup): opaque bytes of S16e's, 1 to [`OPAQUE_RECORD_MAX_LEN`] of them. It
/// prints as its length only.
///
/// Implements planning/architecture.md section 5.12 ("the metadata group keeps only the OPAQUE
/// record, under the account's own OPAQUE server setup").
///
/// Caller: `LinkRecord::opaque_record`, `NewLink`, `KeyBound`, `Change::Password`; Seam: S16e
/// (`link_login` reads it).
#[derive(Clone, PartialEq, Eq)]
pub struct OpaqueRecord(Vec<u8>);

impl OpaqueRecord {
    /// The record with these bytes.
    pub fn new(bytes: Vec<u8>) -> Result<OpaqueRecord, OpaqueRecordError> {
        if bytes.is_empty() {
            return Err(OpaqueRecordError::Empty);
        }
        if bytes.len() > OPAQUE_RECORD_MAX_LEN {
            return Err(OpaqueRecordError::TooLong { len: bytes.len() });
        }
        Ok(OpaqueRecord(bytes))
    }

    /// The bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for OpaqueRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OpaqueRecord({} bytes)", self.0.len())
    }
}

/// The share key wrapped to one member's key: the member, the id of the key it is wrapped to (the
/// `rcpt` of the envelope) and the sealed bytes (opaque here, at most [`WRAP_MAX_LEN`]).
///
/// Implements planning/architecture.md section 5.12 (`wraps`: "for `people` and `approval`").
///
/// Caller: `LinkRecord::wraps`, `NewLink`, `Change::AddWrap`, `KeyBound`; Seam: S15b (the envelope),
/// S16c2 (an `approval` caller is "approved" when its member holds a wrap).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wrap {
    member: MemberId,
    key: KeyId,
    sealed: Sealed,
}

impl Wrap {
    /// The wrap of the share key to `member`'s key `key`; an error when the sealed bytes are longer
    /// than [`WRAP_MAX_LEN`].
    pub fn new(member: MemberId, key: KeyId, sealed: Sealed) -> Result<Wrap, SealedError> {
        if sealed.len() > WRAP_MAX_LEN {
            return Err(SealedError::TooLong { len: sealed.len() });
        }
        Ok(Wrap {
            member,
            key,
            sealed,
        })
    }

    /// The member the key is wrapped to.
    pub fn member(&self) -> MemberId {
        self.member
    }

    /// The id of the member key the share key is wrapped to.
    pub fn key(&self) -> KeyId {
        self.key
    }

    /// The sealed bytes.
    pub fn sealed(&self) -> &Sealed {
        &self.sealed
    }
}

/// A random token each device or browser keeps, to be counted by `max_devices`: 16 random bytes,
/// "counted by a random token each keeps (never an address)". It prints as nothing.
///
/// Implements planning/architecture.md section 5.12 (`max_devices`).
///
/// Caller: `DeviceToken::seen_by`; Seam: S16c2 (`ResolveCtx::device`), the apps and the browser
/// (they make and keep it with [`DeviceToken::generate`]).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DeviceToken([u8; 16]);

impl DeviceToken {
    /// The token with these bytes (a client reads back the token it kept).
    pub const fn from_bytes(bytes: [u8; 16]) -> DeviceToken {
        DeviceToken(bytes)
    }

    /// The bytes of the token.
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// A fresh token: 16 bytes from the entropy seam. On an entropy failure there is no token.
    pub fn generate(entropy: &mut dyn Entropy) -> Result<DeviceToken, EntropyError> {
        let mut bytes = [0u8; 16];
        entropy.fill(&mut bytes)?;
        Ok(DeviceToken(bytes))
    }

    /// What the link's row keeps of this token: a hash of the token under [`DEVICE_SEEN_DOMAIN`] and
    /// the link's id, cut to 16 bytes. The same device is two unrelated values on two links.
    pub fn seen_by(&self, link: &PublicLinkId) -> SeenDevice {
        let digest = hash32(
            DEVICE_SEEN_DOMAIN,
            &[link.as_bytes().as_slice(), self.0.as_slice()],
        );
        let mut out = [0u8; 16];
        for (dst, src) in out.iter_mut().zip(digest.as_bytes()) {
            *dst = *src;
        }
        SeenDevice(out)
    }
}

impl fmt::Debug for DeviceToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceToken(..)")
    }
}

/// A device the link has seen: the hash of its token ([`DeviceToken::seen_by`]), 16 bytes.
///
/// Implements planning/architecture.md section 5.12 (`devices_seen` "as hashed tokens").
///
/// Caller: `Counters::devices_seen`; Seam: S16c2 (`count_open`, `max_devices`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SeenDevice([u8; 16]);

impl SeenDevice {
    /// The bytes of the hash.
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// What a link counts: "the counters (`downloads`, `bytes_served`, `devices_seen` as hashed tokens,
/// `first_open_at`)", and the time of the last open (for `idle_expiry`). Counted, never logged
/// (M7). The functions that change them are S16c2's (`count_open`, `count_download_complete`); the
/// record gives them in-crate access only.
///
/// Implements planning/architecture.md section 5.12 (the counters) and section 5.15 (M7: "link
/// counters and caps only").
///
/// Caller: `LinkRecord::counters`, `rotate` (a rotation starts the counters again);
/// Seam: S16c2 (`resolve`, `count_open`, `admit_bytes`, `count_download_complete`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Counters {
    /// The complete downloads so far.
    pub downloads: u64,
    /// The bytes served so far.
    pub bytes_served: ByteCount,
    /// The devices that opened the link, as hashed tokens (kept only for a link with `max_devices`).
    pub devices_seen: BTreeSet<SeenDevice>,
    /// The time of the first open (the human step).
    pub first_open_at: Option<Stamp>,
    /// The time of the last open.
    pub last_open_at: Option<Stamp>,
}

impl Counters {
    /// The counters of a link nobody has opened.
    pub fn zero() -> Counters {
        Counters {
            downloads: 0,
            bytes_served: ByteCount::ZERO,
            devices_seen: BTreeSet::new(),
            first_open_at: None,
            last_open_at: None,
        }
    }
}

/// Who paused a link: the owner (the creator or a manager), the reports (three devices reported the
/// link) or the operator. Declared in the order of strength.
///
/// Implements planning/architecture.md section 5.12 ("`paused` stops and resumes a link without
/// changing it") and section 3.9 ("a link reported from three different devices pauses until
/// decided").
///
/// Caller: `Pauses`, `UpdateRefusal::Frozen`; Seam: S46 (reports), AzCtl (the operator).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PausedBy {
    /// The creator or a manager of the space.
    Owner,
    /// The reports counting of 3.9.
    Reports,
    /// The operator.
    Operator,
}

words!(
    PausedBy,
    "pause source",
    [Owner => "owner", Reports => "reports", Operator => "operator"]
);

/// Which sources have paused a link: each source pauses and lifts ITS pause, independently, so a
/// pause is never lost when another source pauses or lifts. The link is paused while any is set.
///
/// Implements planning/architecture.md section 5.12 ("`paused`") and section 3.9.
///
/// Caller: `LinkRecord::pauses`, `pause`, `unpause`; Seam: S16c2 (`resolve` refuses a paused link).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pauses {
    owner: bool,
    reports: bool,
    operator: bool,
}

impl Pauses {
    /// No pause.
    pub const NONE: Pauses = Pauses {
        owner: false,
        reports: false,
        operator: false,
    };

    /// Whether any source has paused the link.
    pub const fn is_paused(self) -> bool {
        self.owner || self.reports || self.operator
    }

    /// Whether `by` has paused the link.
    pub const fn has(self, by: PausedBy) -> bool {
        match by {
            PausedBy::Owner => self.owner,
            PausedBy::Reports => self.reports,
            PausedBy::Operator => self.operator,
        }
    }

    /// The sources that have paused the link, in the order of strength.
    pub fn sources(self) -> Vec<PausedBy> {
        PausedBy::ALL
            .iter()
            .copied()
            .filter(|by| self.has(*by))
            .collect()
    }

    /// The same pauses with the pause of `by` set.
    #[must_use]
    pub const fn with(self, by: PausedBy) -> Pauses {
        match by {
            PausedBy::Owner => Pauses {
                owner: true,
                ..self
            },
            PausedBy::Reports => Pauses {
                reports: true,
                ..self
            },
            PausedBy::Operator => Pauses {
                operator: true,
                ..self
            },
        }
    }

    /// The same pauses with the pause of `by` lifted.
    #[must_use]
    pub const fn without(self, by: PausedBy) -> Pauses {
        match by {
            PausedBy::Owner => Pauses {
                owner: false,
                ..self
            },
            PausedBy::Reports => Pauses {
                reports: false,
                ..self
            },
            PausedBy::Operator => Pauses {
                operator: false,
                ..self
            },
        }
    }

    /// The source that has frozen the link, if any: while the operator or the reports hold a pause,
    /// the owner changes nothing (the operator decides about the link as it is). The operator first.
    pub const fn frozen_by(self) -> Option<PausedBy> {
        if self.operator {
            Some(PausedBy::Operator)
        } else if self.reports {
            Some(PausedBy::Reports)
        } else {
            None
        }
    }
}

/// Why a link ended, for the paired request that ends with it ("a paired request ends with its
/// link: revoked, expired or rotated", S16f).
///
/// Implements planning/architecture.md section 5.12 (`link_revoke`, `expires_at`, "rotating").
///
/// Caller: `Rotated`; Seam: S16f (`end_with_link`), S16c2 (the resolver says `Expired`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LinkEnded {
    /// The owner or the operator revoked it.
    Revoked,
    /// It reached `expires_at` (or another expiry).
    Expired,
    /// A rotation gave it a new id and key: the old URL stops.
    Rotated,
}

words!(
    LinkEnded,
    "link end",
    [Revoked => "revoked", Expired => "expired", Rotated => "rotated"]
);

/// What a link is made of besides its options: the parts a client brings with `link_create`.
///
/// Implements planning/architecture.md section 5.12 (`reg_link`: `space_id`, `created_by`,
/// `manifest_object`, `grants`, `opaque_record`, `wraps`, `request_id`, `comment_log`).
///
/// Caller: `LinkRecord::create`; Seam: LINKS-NODE (`link_create` builds it from the command).
#[derive(Clone, Debug)]
pub struct NewLink {
    /// The space the link reads from.
    pub space: SpaceId,
    /// The member who made the link.
    pub created_by: MemberId,
    /// The AZS2 manifest object.
    pub manifest_object: ObjectRef,
    /// What the link may read.
    pub grants: Grants,
    /// The OPAQUE record of the password; `Some` exactly when the options have a password.
    pub opaque_record: Option<OpaqueRecord>,
    /// The wraps of the share key (one for each person of a `people` audience).
    pub wraps: Vec<Wrap>,
    /// The paired request, for uploads and approvals.
    pub request_id: Option<RequestId>,
    /// The comment log, for comments.
    pub comment_log: Option<LogId>,
}

/// The plain data of `reg_link`: every column, as the record holds it. Anyone may build one; only
/// [`LinkRecord::from_row`] turns it into a record, and only after every invariant holds.
///
/// Implements planning/architecture.md section 5.12 (`reg_link`).
///
/// Caller: `LinkRecord::row`, `LinkRecord::from_row`; Seam: S9 (it stores the row), INT-WIRE (the
/// row's wire form and the golden fixture `reg_link.json`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkRow {
    /// `link_id`: the public id.
    pub id: PublicLinkId,
    /// `space_id`.
    pub space: SpaceId,
    /// `created_by`.
    pub created_by: MemberId,
    /// The options (`kind` and `slug` are among them).
    pub options: LinkOptions,
    /// `manifest_object`.
    pub manifest_object: ObjectRef,
    /// `grants`.
    pub grants: Grants,
    /// `opaque_record`.
    pub opaque_record: Option<OpaqueRecord>,
    /// `wraps`, by member (one for each).
    pub wraps: BTreeMap<MemberId, Wrap>,
    /// `request_id`: the paired request.
    pub request_id: Option<RequestId>,
    /// `comment_log`.
    pub comment_log: Option<LogId>,
    /// The counters.
    pub counters: Counters,
    /// `paused`: who has paused the link.
    pub pauses: Pauses,
    /// `revoked_at`.
    pub revoked_at: Option<Stamp>,
    /// `created_at`: never changes.
    pub created_at: Stamp,
    /// When the current id and key began: the creation or the last rotation.
    pub keyed_at: Stamp,
}

/// Why a record cannot be made or kept.
///
/// Implements planning/architecture.md section 5.12 (`reg_link` and the rules between options).
///
/// Caller: `LinkRecord::create`, `LinkRecord::from_row`, `LinkRecord::check`, `UpdateRefusal::Invalid`;
/// Seam: LINKS-NODE (it answers `bad_request`), S9 (a stored row that breaks it is corrupt).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RecordError {
    /// The options break rules between options.
    #[error("the options break {} rule(s) between options", .0.len())]
    Options(Vec<OptionError>),
    /// The parts given are not the ones the options were checked against.
    #[error("the parts of the link are not the ones its options were checked against")]
    FactsChanged,
    /// Two wraps for one member.
    #[error("two wraps for the member {0}")]
    DuplicateWrap(MemberId),
    /// A wrap kept under another member's name.
    #[error("the wrap under the member {0} is a wrap of another member")]
    WrapKeyMismatch(MemberId),
    /// The wraps of a `people` audience are not exactly the named people.
    #[error("the wraps of a `people` audience are the named people, no more and no fewer")]
    WrapsAreNotThePeople,
    /// `keyed_at` before `created_at`.
    #[error("a link cannot be keyed before it was created")]
    KeyedBeforeCreated,
}

/// The facts a row states about itself (its time is the creation).
fn facts_of(row: &LinkRow) -> Facts {
    Facts {
        paired_request: row.request_id.is_some(),
        comment_log: row.comment_log.is_some(),
        opaque_record: row.opaque_record.is_some(),
        wraps: row.wraps.len(),
        grants: row.grants.len(),
        now: row.created_at,
    }
}

/// Every invariant of a row: the rules between options over what the row holds, the wraps (under
/// their member, and exactly the people of a `people` audience), and the order of the times. The
/// functions that make or change a record run it before they commit.
pub(crate) fn check_row(row: &LinkRow) -> Result<(), RecordError> {
    let errors = rules(&row.options, &facts_of(row));
    if !errors.is_empty() {
        return Err(RecordError::Options(errors));
    }
    for (member, wrap) in &row.wraps {
        if *member != wrap.member() {
            return Err(RecordError::WrapKeyMismatch(*member));
        }
    }
    if let Audience::People { people } = &row.options.audience {
        let wrapped: BTreeSet<MemberId> = row.wraps.keys().copied().collect();
        if wrapped != *people && !azlin_seams::mutant!("links_wraps_may_name_strangers") {
            return Err(RecordError::WrapsAreNotThePeople);
        }
    }
    if row.keyed_at < row.created_at {
        return Err(RecordError::KeyedBeforeCreated);
    }
    Ok(())
}

/// A link: the row of `reg_link` with its invariants. Private fields; made by [`LinkRecord::create`]
/// or [`LinkRecord::from_row`], changed by the functions of `update` and (the counters) by S16c2.
///
/// Implements planning/architecture.md section 5.12 (`reg_link`, "Changing a link").
///
/// Caller: `update`, `rotate`, `pause`, `unpause`, `revoke`, `repoint`; Seam: S16c2 (`resolve`,
/// `count_open`, `count_download_complete`), LINKS-NODE (`link_*` verbs), S9 (stores `row()`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkRecord(LinkRow);

impl LinkRecord {
    /// Makes a link from options that passed `validate` and `apply_policy` and the parts that go
    /// with them. The parts must be the ones the options were checked against (the number of wraps,
    /// of grants, the paired request, the comment log, the OPAQUE record), and for a `people`
    /// audience the wraps must be one for each named person. The counters start at zero, nothing is
    /// paused, `created_at` and `keyed_at` are `now`.
    ///
    /// Implements planning/architecture.md section 5.12 (`reg_link`).
    ///
    /// Caller: LINKS-NODE (`link_create`), the tests of this crate.
    pub fn create(
        bounded: Bounded,
        new: NewLink,
        id: PublicLinkId,
        now: Stamp,
    ) -> Result<LinkRecord, RecordError> {
        let (options, facts, _forced) = bounded.into_parts();
        let mut wraps = BTreeMap::new();
        for wrap in new.wraps {
            let member = wrap.member();
            if wraps.insert(member, wrap).is_some() {
                return Err(RecordError::DuplicateWrap(member));
            }
        }
        let row = LinkRow {
            id,
            space: new.space,
            created_by: new.created_by,
            options,
            manifest_object: new.manifest_object,
            grants: new.grants,
            opaque_record: new.opaque_record,
            wraps,
            request_id: new.request_id,
            comment_log: new.comment_log,
            counters: Counters::zero(),
            pauses: Pauses::NONE,
            revoked_at: None,
            created_at: now,
            keyed_at: now,
        };
        // The parts must be the ones the options were checked against (their time is not a part).
        let found = Facts {
            now: facts.now,
            ..facts_of(&row)
        };
        if found != facts {
            return Err(RecordError::FactsChanged);
        }
        check_row(&row)?;
        Ok(LinkRecord(row))
    }

    /// Reads a stored row back: every invariant of a row is checked again. This is for a row that
    /// was stored (and for the tests of the resolver): it does not know what the row was before, so
    /// it cannot judge a CHANGE. A change of a link goes through `update`, `rotate`, `pause`,
    /// `unpause`, `revoke` or `repoint`, which hold the rules of changing a link.
    ///
    /// Implements planning/architecture.md section 5.12 (`reg_link`).
    ///
    /// Seam: S9 (loading `reg_link`), S16c2's tests.
    pub fn from_row(row: LinkRow) -> Result<LinkRecord, RecordError> {
        check_row(&row)?;
        Ok(LinkRecord(row))
    }

    /// Checks every invariant of the record again.
    ///
    /// Implements planning/architecture.md section 5.12 (`reg_link` and the rules between options).
    ///
    /// Caller: the tests of this crate; Seam: S9 (after a restore).
    pub fn check(&self) -> Result<(), RecordError> {
        check_row(&self.0)
    }

    /// The plain data of the row.
    pub fn row(&self) -> &LinkRow {
        &self.0
    }

    /// The plain data of the row, by value.
    pub fn into_row(self) -> LinkRow {
        self.0
    }

    /// `link_id`: the id the link answers to now.
    pub fn id(&self) -> &PublicLinkId {
        &self.0.id
    }

    /// `space_id`.
    pub fn space(&self) -> &SpaceId {
        &self.0.space
    }

    /// `created_by`.
    pub fn created_by(&self) -> &MemberId {
        &self.0.created_by
    }

    /// The options.
    pub fn options(&self) -> &LinkOptions {
        &self.0.options
    }

    /// `kind` (the options').
    pub fn kind(&self) -> LinkKind {
        self.0.options.kind
    }

    /// `slug` (the options').
    pub fn slug(&self) -> Option<&Slug> {
        self.0.options.slug.as_ref()
    }

    /// `follow_latest`: the link follows the latest version of its folder: the kind is
    /// `folder_live`.
    pub fn follow_latest(&self) -> bool {
        self.0.options.kind.follows_latest()
    }

    /// `manifest_object`.
    pub fn manifest_object(&self) -> &ObjectRef {
        &self.0.manifest_object
    }

    /// `grants`.
    pub fn grants(&self) -> &Grants {
        &self.0.grants
    }

    /// `opaque_record`.
    pub fn opaque_record(&self) -> Option<&OpaqueRecord> {
        self.0.opaque_record.as_ref()
    }

    /// The wraps of the share key, by member.
    pub fn wraps(&self) -> impl Iterator<Item = &Wrap> {
        self.0.wraps.values()
    }

    /// The wrap of `member`, if the link holds one: for an `approval` link, the member is approved.
    pub fn wrap_for(&self, member: &MemberId) -> Option<&Wrap> {
        self.0.wraps.get(member)
    }

    /// `request_id`: the paired request.
    pub fn request_id(&self) -> Option<&RequestId> {
        self.0.request_id.as_ref()
    }

    /// `comment_log`.
    pub fn comment_log(&self) -> Option<&LogId> {
        self.0.comment_log.as_ref()
    }

    /// The counters.
    pub fn counters(&self) -> &Counters {
        &self.0.counters
    }

    /// Which sources have paused the link.
    pub fn pauses(&self) -> Pauses {
        self.0.pauses
    }

    /// Whether any source has paused the link.
    pub fn is_paused(&self) -> bool {
        self.0.pauses.is_paused()
    }

    /// `revoked_at`.
    pub fn revoked_at(&self) -> Option<Stamp> {
        self.0.revoked_at
    }

    /// Whether the link is revoked (for good).
    pub fn is_revoked(&self) -> bool {
        self.0.revoked_at.is_some()
    }

    /// `created_at`: never changes.
    pub fn created_at(&self) -> Stamp {
        self.0.created_at
    }

    /// When the current id and key began: the creation or the last rotation.
    pub fn keyed_at(&self) -> Stamp {
        self.0.keyed_at
    }

    /// Whether `id` is the id the link answers to NOW: after a rotation the old id is not.
    pub fn answers_to(&self, id: &PublicLinkId) -> bool {
        self.0.id == *id
    }

    /// Whether `slug` is the link's slug.
    pub fn answers_to_slug(&self, slug: &Slug) -> bool {
        self.0.options.slug.as_ref() == Some(slug)
    }

    /// Replaces the row with `next` once every invariant holds for it; on an error the record is as
    /// it was. The functions of `update` end with it.
    pub(crate) fn commit(&mut self, next: LinkRow) -> Result<(), RecordError> {
        check_row(&next)?;
        self.0 = next;
        Ok(())
    }

    /// Sets which sources have paused the link (a pause has no invariant but its type).
    pub(crate) fn set_pauses(&mut self, pauses: Pauses) {
        self.0.pauses = pauses;
    }

    /// The counters, for the functions of S16c2 that count (`count_open`,
    /// `count_download_complete`): the one in-crate way to change them.
    #[allow(dead_code)] // Seam: lane W1-S16c2 (counters.rs) is the caller.
    pub(crate) fn counters_mut(&mut self) -> &mut Counters {
        &mut self.0.counters
    }

    /// Revokes the link at `now` unless it is revoked already (the first time stands): `revoke`
    /// ends a link this way, and so does a link that is "used up" (`one_time`, S16c2's
    /// `count_download_complete`).
    pub(crate) fn mark_revoked(&mut self, now: Stamp) {
        if self.0.revoked_at.is_none() {
            self.0.revoked_at = Some(now);
        }
    }
}
