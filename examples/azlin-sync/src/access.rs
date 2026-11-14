//! What the state machines ask the registry: may a device read a space's head, may it append to a
//! space's logs, and which key does it sign with (planning/architecture.md section 5.13 "The home node
//! checks each subscribed space against the registry, so events never tell a device more than
//! `head_get` would", section 2.4 "a replica ... `head_get`", section 5.11 "Access: credentials,
//! assertions"; build unit S16, lane W1-S16a).
//!
//! The answers are facts of the registry at the entry being applied (the node applies the same entry
//! to the same registry state on every voter), so the same call gives the same answer on every
//! replica. The trait is defined here and implemented only by the node's glue (SYNC-APPLY, which reads
//! the grants and the devices of `meta.db`): an azul crate names no registry type (COMMON-W1R R5).

use azlin_seams::{DeviceId, SpaceId, VerifyKey};

/// The registry's view of one device and one space.
///
/// `may_head_get` is the rule for everything a device may LEARN about a space (`head_get`, a `watch`
/// subscription): it answers `false` both for a space that does not exist and for a space the device
/// has no access to, so that nothing a caller does with the answer can tell the two apart.
/// `may_append` is the write rule of the space's append logs. `author_key` is the key a device's
/// record signatures are checked with: the device's own signing key in the registry, `None` for a
/// device the registry does not know or that has no usable key (DRIFT X-DELTA-W1S16A-11: the brief's
/// trait has the first two methods only, and a signature cannot be checked without a key).
///
/// Implements planning/architecture.md section 5.13 (the registry check of a `watch` subscription and
/// the author of a log record) and section 5.11 (access: grants and devices).
///
/// Seam: SYNC-APPLY (the node's glue over the registry: `reg_grant`, `reg_device`) implements it; the
/// tests use a table.
pub trait SpaceAccess {
    /// Whether `device` may read the head of `space` (and so be told of its changes).
    fn may_head_get(&self, device: DeviceId, space: SpaceId) -> bool;

    /// Whether `device` may append to the logs of `space`.
    fn may_append(&self, device: DeviceId, space: SpaceId) -> bool;

    /// The key the signatures of `device` are verified with; `None` when it has none.
    fn author_key(&self, device: DeviceId) -> Option<VerifyKey>;
}
