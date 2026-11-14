//! The keys a verifier of the metadata group trusts, with their time windows (planning/
//! architecture.md section 2.4 "Credentials are verifiable on every node", "the map names the
//! group's long-lived identity key", section 2.11 anchors; build unit S9, lane W1-S9a).
//!
//! DRIFT D-5 / X-D-08 (accepted by the coordinator): an apply that verifies a signature must reach
//! the same verdict on every voter, but voters hold different map epochs at the same log index, so
//! the keys an apply verifies with are REPLICATED STATE of the group, not a node's cached map: an
//! [`AnchorSet`] is installed by an entry that carries the signed map (S9i) and is read back
//! through `RegistryRead::anchors`.
//!
//! A [`KeyWindow`] is valid from `not_before` (inclusive) to `not_after` (exclusive; `None` never
//! ends), the convention of azlin-keycert. Windows may overlap (a rotation's overlap verifies both
//! keys) and a list may be empty: an empty set returns NO key, and a caller refuses when it gets
//! none: an empty anchor set is never "no check" (COMMON-r30 rule 18).

use crate::error::BoundError;
use crate::ids::MetaGroupId;
use crate::sign::PublicKey32;
use crate::time::LeaderTime;

/// The most windows of one list of an [`AnchorSet`] (a key rotates every 30 days and the previous
/// key stays valid one more period, so a handful is the normal count).
pub const ANCHOR_WINDOWS_MAX: usize = 64;

/// One trusted public key and when it is trusted: `not_before <= t < not_after`, with `None`
/// meaning "until it is replaced". Both times are exact leader stamps. A key that is RETIRED
/// (`not_after`) is not the same as a key that is REVOKED (`revoked_from`): what a retired key signed on
/// a day inside its window stays valid, what a revoked key signed from its revocation on does not
/// (decision D-W1S9E3, drift row X-DELTA-W1S9E-5).
///
/// Implements planning/architecture.md section 2.4 (the identity key certifies each credential key;
/// the previous key stays valid for one more period) and section 2.11 (anchors).
///
/// Caller: [`AnchorSet`], S9e (certificate and entitlement verification), S9f (the identity key
/// must be the one the map names), S42b (`anchors_from_map`), S9d and S9i (fill `revoked_from` from
/// their revocation rows).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyWindow {
    /// The public key.
    pub public: PublicKey32,
    /// The first second the key is trusted (inclusive).
    pub not_before: LeaderTime,
    /// The first second it is no longer trusted (exclusive); `None`: until replaced.
    pub not_after: Option<LeaderTime>,
    /// The first second from which what this key signs is NOT trusted any more, because the key is
    /// revoked (stolen or lost), from the registry's revocation row; `None`: not revoked. A verifier
    /// refuses what a revoked key signed when the time the signed claim carries (an entitlement's issue
    /// day) is at or after `revoked_from`, whatever `not_after` says: azlin-creds'
    /// `verify_entitlement` answers `Revoked`. A claim dated before `revoked_from` is out of this
    /// field's reach, so the registry sets it to the earliest second the key may have been in other
    /// hands (the window's `not_before` when that is unknown, which refuses every date of the window).
    /// [`KeyWindow::contains`] does not read it, and azlin-creds reads it for the payments key only.
    pub revoked_from: Option<LeaderTime>,
}

impl KeyWindow {
    /// Whether the key is trusted at `t`: `not_before <= t < not_after`.
    pub fn contains(&self, t: LeaderTime) -> bool {
        self.not_before <= t && self.not_after.is_none_or(|end| t < end)
    }
}

/// The keys the voters of one metadata group verify with: the payments key (entitlements), the
/// operator key (operator rows) and the identity key of each metadata group (certificates of
/// credential keys), each with its window, plus the epoch of the map they came from.
///
/// Implements planning/architecture.md section 2.4 and section 2.11; DRIFT D-5.
///
/// Caller: `RegistryRead::anchors` and `Change::PutAnchors` (azlin-registry), S9e, S9f, S9g, S9i
/// (`MapAnchors` glue), S42b (`anchors_from_map` builds one).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct AnchorSet {
    /// The payments key's windows (it signs entitlements).
    pub payments: Vec<KeyWindow>,
    /// The operator key's windows (it signs operator rows).
    pub operator: Vec<KeyWindow>,
    /// The identity keys' windows, per metadata group.
    pub identity: Vec<(MetaGroupId, KeyWindow)>,
    /// The epoch of the cluster map these anchors were taken from.
    pub map_epoch: u64,
}

impl AnchorSet {
    /// The set with no key at all: every lookup answers nothing, and a caller refuses.
    pub fn empty() -> AnchorSet {
        AnchorSet::default()
    }

    /// Whether no list holds a window.
    pub fn is_empty(&self) -> bool {
        self.payments.is_empty() && self.operator.is_empty() && self.identity.is_empty()
    }

    /// The identity-key windows of metadata group `g` that are valid at `t`, in list order.
    pub fn identity_at(&self, g: MetaGroupId, t: LeaderTime) -> Vec<&KeyWindow> {
        self.identity
            .iter()
            .filter(|(group, window)| *group == g && window.contains(t))
            .map(|(_, window)| window)
            .collect()
    }

    /// The payments-key windows valid at `t`, in list order.
    pub fn payments_at(&self, t: LeaderTime) -> Vec<&KeyWindow> {
        self.payments
            .iter()
            .filter(|window| window.contains(t))
            .collect()
    }

    /// The operator-key windows valid at `t`, in list order.
    pub fn operator_at(&self, t: LeaderTime) -> Vec<&KeyWindow> {
        self.operator
            .iter()
            .filter(|window| window.contains(t))
            .collect()
    }

    /// Whether every list is within [`ANCHOR_WINDOWS_MAX`] (refused, never truncated).
    pub fn check(&self) -> Result<(), BoundError> {
        for (what, count) in [
            ("AnchorSet.payments", self.payments.len()),
            ("AnchorSet.operator", self.operator.len()),
            ("AnchorSet.identity", self.identity.len()),
        ] {
            if count > ANCHOR_WINDOWS_MAX {
                return Err(BoundError::TooMany {
                    what,
                    count,
                    max: ANCHOR_WINDOWS_MAX,
                });
            }
        }
        Ok(())
    }
}
