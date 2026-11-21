//! Grants: who holds which role in a space, whose quota pays and up to how many bytes, and what made
//! the grant (planning/architecture.md section 5.4 `reg_grant`, section 5.12 "External members",
//! section 5.15 M6 "capability grants"; build unit S16, lane W1-S16b).
//!
//! A [`Grant`] is one `reg_grant` row as the space chain made it: the grantee (a member of the
//! account, a member of ANOTHER account, a group, or a bearer capability), the role, the per-share cap
//! and the payer (S14: a participant's writes count against the participant's own quota, with a cap per
//! share set by the owner), the source (the chain position that made it) and its state. A grant is
//! created by a signed `space_grant` entry and ended by a signed `space_revoke` entry; the registry
//! keeps ended grants as rows with their `removed_at`.
//!
//! A CAPABILITY grantee is stored only as the BLAKE3 hash of the bearer secret ([`capability_hash`]),
//! never the secret: the space chain, the state, the effects and every `Debug` output hold the hash
//! only, and a presented secret is hashed and compared in constant time (`access`).

use azlin_seams::{AccountId, ByteCount, ChainSeq, GroupId, Hash32, MemberId, Secret, Stamp};

use crate::role::{role_caps, Caps, SpaceRole};

/// Who a grant is for.
///
/// Implements planning/architecture.md section 5.4 (`reg_grant`: grantee is "member, possibly of another
/// account, or group; for a capability, the hash of its secret"), section 5.12 (an external member is a
/// `reg_grant` row with a member of another account) and section 5.15 M6 (capability grants).
///
/// Caller: `Grant::grantee`, `SpaceOp::Grant`, `SpaceOp::Revoke`, `WrapNeed::to`, `SpaceState` (the
/// grants are keyed by it); S9 (`reg_grant`, through the SPACES-APPLY glue).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Grantee {
    /// A member of the space's own account.
    Member(MemberId),
    /// A member of ANOTHER account (an external member): both the account and the member id are part
    /// of the identity, so an external member whose id equals an internal member's gets nothing of
    /// the internal member's grants.
    External {
        /// The member's own account.
        account: AccountId,
        /// The member in that account.
        member: MemberId,
    },
    /// A group of the space's own account (nesting is resolved by the caller: the principal lists
    /// every group the member is in).
    Group(GroupId),
    /// A bearer capability: the BLAKE3 hash of its secret ([`capability_hash`]), never the secret.
    /// Declared last, so the capabilities are contiguous at the end of an ordered map.
    Capability(Hash32),
}

/// Whose quota a grantee's writes are counted against.
///
/// Implements planning/architecture.md section 5.4 (`reg_grant.payer`: "the grantee's writes count
/// against the grantee's own quota, capped per share by the owner") and section 6.10 S14 (quota
/// exhaustion through shares).
///
/// Caller: `Grant::payer`, `SpaceOp::Grant`, `charge`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Payer {
    /// The space's owner pays: the writes count against the owning account's quota.
    Owner,
    /// The grantee pays: the writes count against the grantee's own quota.
    Grantee,
}

words!(Payer, "payer", [Owner => "owner", Grantee => "grantee"]);

/// The id of a share of the old per-drive sharing path (`azlin-token` `shares.rs`), which a grant can
/// carry as its source while the old path runs on.
///
/// Implements planning/architecture.md section 5.4 (`reg_grant.source`: "chain seq or share id").
///
/// Seam: the migration of old shares into grants (INT unit of the old sharing path); nothing in this
/// crate makes one.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShareId([u8; 16]);

impl ShareId {
    /// The share id with these bytes.
    pub const fn from_bytes(bytes: [u8; 16]) -> ShareId {
        ShareId(bytes)
    }

    /// The bytes of the share id.
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl core::fmt::Debug for ShareId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ShareId(")?;
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        f.write_str(")")
    }
}

/// What made a grant: the position of the space-chain entry, or a share of the old path.
///
/// Implements planning/architecture.md section 5.4 (`reg_grant.source`: "chain seq or share id") and
/// section 2.4 ("the space grant backed by a signed entry").
///
/// Caller: `Grant::source` (`apply` writes `Source::Chain`); S9 (`reg_grant.source`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    /// The signed space-chain entry at this position.
    Chain(ChainSeq),
    /// A share of the old sharing path.
    Share(ShareId),
}

/// Whether a grant is in force.
///
/// Implements planning/architecture.md section 5.4 (`reg_grant.state`, `removed_at`).
///
/// Caller: `Grant::state`, `access`, `charge`, `apply` (a revoke ends a grant at once).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GrantState {
    /// The grant is in force.
    Active,
    /// The grant was ended by a `space_revoke` entry: it gives nothing, from that entry on.
    Revoked,
}

words!(
    GrantState,
    "grant state",
    [Active => "active", Revoked => "revoked"]
);

/// One `reg_grant` row of a space.
///
/// `cap_bytes` is the cap the owner set per share (what this grantee may write into the space in all);
/// `payer` says whose quota those writes count against. `added_at` and `removed_at` are the leader's
/// stamps of the entries that made and ended the grant (planning/architecture.md section 2.4).
///
/// Implements planning/architecture.md section 5.4 (`reg_grant`: `space_id`, `kind`, grantee, `role`,
/// `cap_bytes`, `payer`, `source`, `state`, `added_at`, `removed_at`).
///
/// Caller: `SpaceState::grant`, `SpaceDelta::Grant` (the registry glue writes each as a row), `access`,
/// `charge`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grant {
    /// Who the grant is for.
    pub grantee: Grantee,
    /// The role.
    pub role: SpaceRole,
    /// The cap the owner set for this grantee's writes into the space; `None`: no cap.
    pub cap_bytes: Option<ByteCount>,
    /// Whose quota the grantee's writes count against.
    pub payer: Payer,
    /// What made the grant.
    pub source: Source,
    /// Whether the grant is in force.
    pub state: GrantState,
    /// The leader's stamp of the entry that made the grant.
    pub added_at: Stamp,
    /// The leader's stamp of the entry that ended it; `None` while it is in force.
    pub removed_at: Option<Stamp>,
}

impl Grant {
    /// Whether the grant is in force.
    pub fn is_active(&self) -> bool {
        self.state == GrantState::Active
    }

    /// What the grant gives now: the capabilities of its role while it is in force, nothing after a
    /// revoke.
    pub fn caps(&self) -> Caps {
        if self.is_active() {
            role_caps(self.role)
        } else {
            Caps::NONE
        }
    }

    /// Whether the grant, while in force, lets its grantee read (and so holds the space key through a
    /// wrap).
    pub fn reads(&self) -> bool {
        role_caps(self.role).can_read()
    }
}

/// The context string of [`capability_hash`] (a BLAKE3 `derive_key` context, never edited: a changed
/// purpose is a new context).
///
/// Implements planning/architecture.md section 5.15 M6 (a capability grant is a bearer secret) and
/// section 5.4 ("for a capability, the hash of its secret").
///
/// Caller: `capability_hash`, the test that re-derives it.
pub const CAPABILITY_HASH_DOMAIN: &str = "azlin-spaces-capability-v1";

/// The hash a capability grant stores in place of the bearer secret: BLAKE3 `derive_key` of the secret
/// under [`CAPABILITY_HASH_DOMAIN`]. The device that makes the grant computes it from the secret it
/// generated; a node computes it from the secret a connection presents. The secret itself is never
/// stored, signed into the chain or printed.
///
/// A stack copy of the secret inside BLAKE3's one-shot hasher is not wiped (the crate has no `zeroize`
/// of its own; drift row X-DELTA-W1S16B-14).
///
/// Implements planning/architecture.md section 5.15 M6 and section 5.4 (`reg_grant`: "for a
/// capability, the hash of its secret").
///
/// Caller: the granting device (building `Grantee::Capability`), `access` and `charge` (hashing the
/// presented secret).
pub fn capability_hash(secret: &Secret<32>) -> Hash32 {
    Hash32::from_bytes(blake3::derive_key(CAPABILITY_HASH_DOMAIN, secret.expose()))
}
