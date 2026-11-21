//! The public name of a link: its id (60 random bits, 12 base32 characters) and its slug (a
//! readable name instead of the id), planning/architecture.md section 5.12: "The URL:
//! `https://drive.azlin.io/l/<link id or slug>#<share key>`. The link id has ~60 random bits" and
//! "`slug`: a readable name instead of the random id; only with a password or a named audience,
//! because a name can be guessed".
//!
//! - [`PublicLinkId`] is 60 bits kept in 8 bytes (big endian, the top four bits zero). Its text is
//!   12 characters of the lower-case RFC 4648 base32 alphabet (`a` to `z`, `2` to `7`), five bits
//!   each, the most significant first. The plan does not name the alphabet (drift register of lane
//!   W1-S16c1, X-D-62): this is the reading the client side (S16d) parses. [`FromStr`] is strict:
//!   exactly 12 characters of the alphabet, no upper case, no padding.
//! - [`new_id`] draws 8 bytes from the entropy seam and keeps 60 bits of them. A failed draw is an
//!   error, never an id of zeroes. Two ids can collide with a probability of about 2^-61 per pair, so
//!   the node that inserts a link refuses an id it already holds (S9's unique index); this crate
//!   only makes ids.
//! - [`Slug`] is 3 to 64 characters of `a-z`, `0-9` and `-`, with no hyphen at either end and no two
//!   hyphens in a row, and it NEVER has the shape of a link id (12 characters of the id alphabet),
//!   so that `/l/<x>` is an id or a slug and never both. Slugs are unique per cluster; the index that
//!   enforces it is the node's (drift register of lane W1-S16c1).
//!
//! Implements planning/architecture.md section 5.12 (the URL, `slug`).

use core::fmt;
use core::str::FromStr;

use azlin_seams::{Entropy, EntropyError, IdError};

/// The number of characters of a link id's text.
///
/// Implements planning/architecture.md section 5.12 ("The link id has ~60 random bits": 12
/// characters of five bits).
///
/// Caller: `PublicLinkId`, `has_link_id_shape`; Seam: S16d (`parse_link` reads an id of this
/// length).
pub const ID_CHARS: usize = 12;

/// The number of random bits of a link id.
///
/// Implements planning/architecture.md section 5.12 ("The link id has ~60 random bits").
///
/// Caller: `new_id`, `PublicLinkId::from_bytes`.
pub const ID_BITS: u32 = 60;

/// The mask of the 60 bits.
const ID_MASK: u64 = (1u64 << ID_BITS) - 1;

/// The shortest slug.
///
/// Implements planning/architecture.md section 5.12 (`slug`; the plan gives no length: drift
/// register of lane W1-S16c1).
///
/// Caller: `Slug::parse`.
pub const SLUG_MIN_LEN: usize = 3;

/// The longest slug (the length of a DNS label).
///
/// Implements planning/architecture.md section 1.4 (rule 4, limits) and section 5.12 (`slug`).
///
/// Caller: `Slug::parse`.
pub const SLUG_MAX_LEN: usize = 64;

/// The public id of a link: 60 random bits, 12 base32 characters in the URL.
///
/// Implements planning/architecture.md section 5.12 ("The link id has ~60 random bits").
///
/// Caller: `LinkRecord`, `rotate`; Seam: S16d (`LinkUrl`), S16e (`link_login`'s guess ledger is
/// keyed by it), S16f (`paired_with`), S9 (the node's unique index of ids).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PublicLinkId([u8; 8]);

impl PublicLinkId {
    /// The number of bytes that hold the 60 bits.
    pub const LEN: usize = 8;

    /// The id with these bytes: a 60-bit number, big endian; `IdError::OutOfRange` when any of the
    /// top four bits is set (there is one spelling of every id).
    pub fn from_bytes(bytes: [u8; 8]) -> Result<PublicLinkId, IdError> {
        if u64::from_be_bytes(bytes) > ID_MASK {
            return Err(IdError::OutOfRange);
        }
        Ok(PublicLinkId(bytes))
    }

    /// The bytes of the id (a 60-bit number, big endian).
    pub const fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }
}

/// The base32 character of a five-bit value: `a` to `z` for 0 to 25, `2` to `7` for 26 to 31.
fn base32_char(value: u8) -> char {
    match value & 31 {
        letter @ 0..=25 => char::from(b'a' + letter),
        digit => char::from(b'2' + (digit - 26)),
    }
}

/// The five-bit value of a base32 character; `None` for anything else (upper case included).
fn base32_value(byte: u8) -> Option<u8> {
    match byte {
        b'a'..=b'z' => Some(byte - b'a'),
        b'2'..=b'7' => Some(byte - b'2' + 26),
        _ => None,
    }
}

/// 12 lower-case base32 characters.
impl fmt::Display for PublicLinkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = u64::from_be_bytes(self.0);
        for position in 0..ID_CHARS {
            let shift = 5 * (ID_CHARS - 1 - position);
            write!(f, "{}", base32_char(((value >> shift) & 31) as u8))?;
        }
        Ok(())
    }
}

impl fmt::Debug for PublicLinkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PublicLinkId({self})")
    }
}

/// Strict: exactly 12 characters of `a` to `z` and `2` to `7`. `IdError::Length` for any other
/// length, `IdError::BadDigit` for any other character.
impl FromStr for PublicLinkId {
    type Err = IdError;

    fn from_str(text: &str) -> Result<PublicLinkId, IdError> {
        let characters = text.as_bytes();
        if characters.len() != ID_CHARS {
            return Err(IdError::Length);
        }
        let mut value: u64 = 0;
        for character in characters {
            match base32_value(*character) {
                Some(bits) => value = (value << 5) | u64::from(bits),
                None => return Err(IdError::BadDigit),
            }
        }
        Ok(PublicLinkId(value.to_be_bytes()))
    }
}

/// A fresh link id: 8 bytes from the entropy seam, 60 bits of them kept (uniform over 2^60). On an
/// entropy failure there is no id.
///
/// Implements planning/architecture.md section 5.12 ("The link id has ~60 random bits") and section
/// 1.4 rule 1 (randomness is a parameter).
///
/// Caller: `rotate`; Seam: LINKS-NODE (`link_create`: the node makes the id, checks its unique
/// index and draws again on a clash).
pub fn new_id(entropy: &mut dyn Entropy) -> Result<PublicLinkId, EntropyError> {
    let mut bytes = [0u8; 8];
    entropy.fill(&mut bytes)?;
    Ok(PublicLinkId(
        (u64::from_be_bytes(bytes) & ID_MASK).to_be_bytes(),
    ))
}

/// Why a text is no slug.
///
/// Implements planning/architecture.md section 5.12 (`slug`: a readable name; never an id).
///
/// Caller: `Slug::parse`; Seam: S16d (`parse_link` refuses a slug of an id's shape), the share
/// dialog (it shows the reason).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SlugError {
    /// Fewer than [`SLUG_MIN_LEN`] or more than [`SLUG_MAX_LEN`] bytes.
    #[error("a slug is {} to {} characters", SLUG_MIN_LEN, SLUG_MAX_LEN)]
    Length,
    /// A character other than `a` to `z`, `0` to `9` and `-`.
    #[error("a slug has lower-case letters, digits and hyphens only")]
    BadCharacter,
    /// A hyphen first or last.
    #[error("a slug neither starts nor ends with a hyphen")]
    HyphenAtEdge,
    /// Two hyphens in a row.
    #[error("a slug has no two hyphens in a row")]
    DoubleHyphen,
    /// The text has the shape of a link id: 12 characters of `a` to `z` and `2` to `7`.
    #[error("a slug never has the shape of a link id (12 characters of a-z and 2-7)")]
    LooksLikeLinkId,
}

/// Whether `text` has the shape of a link id: exactly 12 characters of the id alphabet. A slug never
/// does; the URL parser of S16d uses the same question to tell the two apart.
///
/// Implements planning/architecture.md section 5.12 (`/l/<link id or slug>`).
///
/// Caller: `Slug::parse`; Seam: S16d (`parse_link`).
pub fn has_link_id_shape(text: &str) -> bool {
    text.len() == ID_CHARS && text.bytes().all(|byte| base32_value(byte).is_some())
}

/// A readable name for a link instead of its random id: 3 to 64 characters of `a-z`, `0-9` and `-`
/// (no hyphen at either end, no two in a row), never of a link id's shape. A name can be guessed, so
/// a link may have one only with a password or a named audience (`validate`).
///
/// Implements planning/architecture.md section 5.12 (`slug`).
///
/// Caller: `LinkOptions::slug`, `validate`; Seam: S16d (`IdOrSlug::Slug`), S9 (slugs are unique per
/// cluster: the node's `SlugIndex`).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slug(String);

impl Slug {
    /// The slug of `text`, or why it is none. Exact: no case folding, no trimming.
    pub fn parse(text: &str) -> Result<Slug, SlugError> {
        if text.len() < SLUG_MIN_LEN || text.len() > SLUG_MAX_LEN {
            return Err(SlugError::Length);
        }
        let allowed = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-';
        if !text.bytes().all(allowed) {
            return Err(SlugError::BadCharacter);
        }
        if text.starts_with('-') || text.ends_with('-') {
            return Err(SlugError::HyphenAtEdge);
        }
        if text.contains("--") {
            return Err(SlugError::DoubleHyphen);
        }
        if has_link_id_shape(text) && !azlin_seams::mutant!("links_slug_may_look_like_an_id") {
            return Err(SlugError::LooksLikeLinkId);
        }
        Ok(Slug(text.to_string()))
    }

    /// The slug as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Slug({:?})", self.0)
    }
}

/// Strict: the same rules as [`Slug::parse`].
impl FromStr for Slug {
    type Err = SlugError;

    fn from_str(text: &str) -> Result<Slug, SlugError> {
        Slug::parse(text)
    }
}
