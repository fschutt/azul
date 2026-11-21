//! The block list of "Blocking members": who may not reach the blocker.
//!
//! Implements planning/architecture.md section 3.9: "Blocking members (Apple 1.2): a member blocked in AzChat or
//! AzDrive can no longer reach the blocker: no messages, shares or invites. The rule is synced in the blocker's
//! own space, and the server filters the blocker's inbox requests." The list is the rule: the apps keep it in
//! the blocker's space (the byte form below) and check every incoming message, share, invite and request
//! against it. That a SEALED rule is invisible to the server, so the server filter needs a list it can read, is
//! drift X-DELTA-W1S46A-5 (the owner chooses: a `reg_block` table of key hashes, or the client alone filters;
//! this crate serves both).
//!
//! The byte form is a deterministic CBOR map (section 5.1 principle 6): 1 version, 2 the blocked members'
//! public keys as an array of 32-byte strings in STRICTLY ASCENDING order, so the bytes are the same whatever
//! the order the members were blocked in. A reader ignores unknown keys that are not in `crit`, keeps them, and
//! writes them back (rule R18: a device that does not know a newer field must not lose it).
//!
//! Limits (section 1.4 rule 4): at most [`MAX_BLOCKED`] members; a longer list is refused when blocking and
//! when parsing.
//!
//! Caller: S46-INT-5 (the sync engine's block list: AzChat and AzDrive filter inbound contact with
//! `may_reach`, and keep the list in the blocker's space with `to_bytes` and `parse`).

use core::fmt;
use std::collections::BTreeSet;

use azlin_cbor::{
    decode, encode, DecodeError, EncodeError, IntMap, IntMapBuilder, Limits, MapError, ShapeError,
    Value,
};

use crate::body::{take_extras, Extras, ExtrasError};

/// The most members one list holds.
pub const MAX_BLOCKED: usize = 10_000;

/// The version of the byte form.
pub const BLOCK_VERSION: u64 = 1;

const K_VERSION: u64 = 1;
const K_MEMBERS: u64 = 2;
const KNOWN_KEYS: &[u64] = &[K_VERSION, K_MEMBERS];

/// 10,000 keys of 34 bytes is 340,000 bytes; the rest is room for the fields of a newer writer.
const BLOCK_LIMITS: Limits = Limits {
    max_input: 512 * 1024,
    max_depth: 6,
    max_items: 10_240,
    max_total_items: 10_496,
    max_text: 4 * 1024,
};

/// Why a block list was refused.
///
/// Caller: S46-INT-5.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlockError {
    /// The list already holds [`MAX_BLOCKED`] members and the member is not one of them.
    Full,
    /// The bytes are not the strict deterministic CBOR of `azlin-cbor`.
    Cbor(DecodeError),
    /// The map has the wrong shape (a missing key, a wrong type, an invalid or unknown `crit`).
    Shape(ShapeError),
    /// The list could not be written under its limits.
    Encode(EncodeError),
    /// The map could not be built.
    Map(MapError),
    /// The extras of the map cannot be written.
    Extras(ExtrasError),
    /// The version is not [`BLOCK_VERSION`].
    Version(u64),
    /// The list has more than [`MAX_BLOCKED`] members.
    TooMany,
    /// The member at this index is not a 32-byte string.
    MemberLength(usize),
    /// The members are not in strictly ascending order (a duplicate or an unsorted list).
    NotAscending,
}

impl fmt::Display for BlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockError::Full => write!(f, "the block list holds {MAX_BLOCKED} members already"),
            BlockError::Cbor(e) => write!(f, "not strict CBOR: {e}"),
            BlockError::Shape(e) => write!(f, "wrong shape: {e}"),
            BlockError::Encode(e) => write!(f, "cannot be written: {e}"),
            BlockError::Map(e) => write!(f, "cannot be built: {e}"),
            BlockError::Extras(e) => write!(f, "extras: {e}"),
            BlockError::Version(v) => write!(f, "version {v} is not understood"),
            BlockError::TooMany => write!(f, "more than {MAX_BLOCKED} members"),
            BlockError::MemberLength(i) => write!(f, "member {i} is not 32 bytes"),
            BlockError::NotAscending => f.write_str("members are not in strictly ascending order"),
        }
    }
}

impl std::error::Error for BlockError {}

impl From<DecodeError> for BlockError {
    fn from(e: DecodeError) -> BlockError {
        BlockError::Cbor(e)
    }
}

impl From<ShapeError> for BlockError {
    fn from(e: ShapeError) -> BlockError {
        BlockError::Shape(e)
    }
}

impl From<EncodeError> for BlockError {
    fn from(e: EncodeError) -> BlockError {
        BlockError::Encode(e)
    }
}

impl From<MapError> for BlockError {
    fn from(e: MapError) -> BlockError {
        BlockError::Map(e)
    }
}

impl From<ExtrasError> for BlockError {
    fn from(e: ExtrasError) -> BlockError {
        BlockError::Extras(e)
    }
}

/// How a blocked member would reach the blocker.
///
/// Caller: S46-INT-5 (the sync engine names the kind of every inbound item).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Contact {
    /// A chat message.
    Message,
    /// A share (a file or a folder).
    Share,
    /// An invite (to a space, a team or a chat).
    Invite,
    /// A request dropped into the blocker's inbox through a request (AZD2).
    Request,
}

/// The members a person has blocked, identified by their public keys.
///
/// Caller: S46-INT-5.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct BlockList {
    members: BTreeSet<[u8; 32]>,
    extras: Extras,
}

impl fmt::Debug for BlockList {
    /// The number of members only: who a person blocked is theirs.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlockList {{ members: {} }}", self.members.len())
    }
}

impl BlockList {
    /// An empty list.
    ///
    /// Caller: S46-INT-5.
    pub fn new() -> BlockList {
        BlockList::default()
    }

    /// Blocks `who`. Blocking a member that is already blocked is fine; a new member beyond
    /// [`MAX_BLOCKED`] is refused with `Full`.
    ///
    /// Caller: S46-INT-5 (the "Block" action of a member's menu).
    pub fn block(&mut self, who: [u8; 32]) -> Result<(), BlockError> {
        if self.members.contains(&who) {
            return Ok(());
        }
        if self.members.len() >= MAX_BLOCKED {
            return Err(BlockError::Full);
        }
        self.members.insert(who);
        Ok(())
    }

    /// Unblocks `who` (nothing happens when they are not blocked).
    ///
    /// Caller: S46-INT-5.
    pub fn unblock(&mut self, who: &[u8; 32]) {
        self.members.remove(who);
    }

    /// Whether a contact of this kind from `sender` may reach the blocker: false for a blocked member, whatever
    /// the kind (no messages, no shares, no invites, no requests).
    ///
    /// Caller: S46-INT-5 (every inbound message, share, invite and inbox request is checked).
    pub fn may_reach(&self, sender: &[u8; 32], kind: Contact) -> bool {
        match kind {
            Contact::Message | Contact::Share | Contact::Invite | Contact::Request => {
                !self.members.contains(sender)
            }
        }
    }

    /// The number of blocked members.
    ///
    /// Caller: S46-INT-5 (the settings page shows the count).
    pub fn len(&self) -> usize {
        self.members.len()
    }

    /// Whether nobody is blocked.
    ///
    /// Caller: S46-INT-5.
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// The blocked members in ascending key order.
    ///
    /// Caller: S46-INT-5 (the settings page lists them).
    pub fn members(&self) -> impl Iterator<Item = &[u8; 32]> {
        self.members.iter()
    }

    /// The byte form: the same bytes whatever the order the members were blocked in, and the unknown keys of a
    /// newer writer kept (rule R18).
    ///
    /// Caller: S46-INT-5 (the apps write it into the blocker's space).
    pub fn to_bytes(&self) -> Result<Vec<u8>, BlockError> {
        self.extras.check(KNOWN_KEYS)?;
        let members = self
            .members
            .iter()
            .map(|m| Value::Bytes(m.to_vec()))
            .collect();
        let builder = IntMapBuilder::new()
            .put(K_VERSION, Value::Uint(BLOCK_VERSION))
            .put(K_MEMBERS, Value::Array(members));
        let value = self.extras.put_into(builder).build()?;
        Ok(encode(&value, &BLOCK_LIMITS)?)
    }

    /// The list of `b`, which must be the strict byte form: version 1, at most [`MAX_BLOCKED`] members of 32
    /// bytes in strictly ascending order, nothing after it. Unknown keys that are not in `crit` are kept.
    ///
    /// Caller: S46-INT-5 (the apps read it from the blocker's space).
    pub fn parse(b: &[u8]) -> Result<BlockList, BlockError> {
        let value = decode(b, &BLOCK_LIMITS)?;
        let mut map = IntMap::new(&value)?;
        let version = map.req_u64(K_VERSION)?;
        if version != BLOCK_VERSION {
            return Err(BlockError::Version(version));
        }
        let items = map.req_array(K_MEMBERS)?;
        if items.len() > MAX_BLOCKED {
            return Err(BlockError::TooMany);
        }
        let mut members = BTreeSet::new();
        let mut previous: Option<[u8; 32]> = None;
        for (index, item) in items.iter().enumerate() {
            let bytes = item.as_bytes().ok_or(BlockError::MemberLength(index))?;
            let member =
                <[u8; 32]>::try_from(bytes).map_err(|_| BlockError::MemberLength(index))?;
            if previous.is_some_and(|p| p >= member) {
                return Err(BlockError::NotAscending);
            }
            previous = Some(member);
            members.insert(member);
        }
        let extras = take_extras(map)?;
        Ok(BlockList { members, extras })
    }
}
