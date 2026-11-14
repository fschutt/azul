//! Heads exchanged between devices (planning/architecture.md section 5.9 "a server could still
//! withhold newer entries from one device, which would keep wrapping keys to a removed member. So
//! ... devices of one account exchange the heads they have seen whenever they meet peer to peer (LAN
//! sync, calls), so a hidden removal shows quickly"; section 6.10 V3 "a restore epoch the metadata
//! group signs"; build unit S17, lane W1-S17a).
//!
//! [`compare_peer`] reads a peer's signed head against this device's pin and says what it means; it
//! changes nothing. A peer head this device has not seen yet is the alarm: the server showed the
//! peer a newer head than it showed this device. After `PeerAhead` the device may offer the head to
//! its own pin (`HeadPin::offer`) and fetch the entries it lacks.
//!
//! The restore epoch comes first: heads of different restore epochs are not positions of one
//! timeline, so they are not compared by position. The epoch a device stands in is the newest
//! restore it accepted (or that of its pinned head, when that head was signed later). A peer on an
//! OLDER epoch holds a head of the lost timeline (it has not been announced the restore): ignored,
//! it is behind. A peer on a NEWER epoch has been announced a restore this device has not: it is
//! ahead, and the announcement is what this device was not shown.
//!
//! | the peer's head | verdict |
//! |---|---|
//! | its signature does not verify | `BadSignature` (ignored: a peer's junk is no alarm) |
//! | of a chain this device pins nothing of | `Unpinned` (nothing to compare) |
//! | of a restore epoch below the one this device stands in | `PeerBehind` (ignored: the lost timeline) |
//! | of a restore epoch above the one this device stands in | `PeerAhead(Withheld alarm naming both positions)` |
//! | below this device's pinned position | `PeerBehind` (ignored) |
//! | the pinned head | `Same` (quiet) |
//! | another hash at the pinned position | `Fork(alarm)` |
//! | above the pinned position | `PeerAhead(Withheld alarm naming both positions)` |

use azlin_seams::mutant;

use crate::entry::{Alarm, HeadVerifier, SignedHead};
use crate::pin::HeadPin;

/// What a peer's head means to this device.
///
/// Implements planning/architecture.md section 5.9 ("devices of one account exchange the heads they
/// have seen ... so a hidden removal shows quickly") and section 6.10 V3 (a restore epoch the
/// metadata group signs).
///
/// Caller: `compare_peer`; the LAN-sync and call clients (CHAIN-GLUE) react to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PeerVerdict {
    /// The peer holds the head this device holds: nothing to say.
    Same,
    /// The peer is behind this device, by position or by restore epoch: ignored.
    PeerBehind,
    /// The peer holds a newer head than this device has seen, or a head of a restore epoch this
    /// device has not been announced: the server may be withholding entries or the restore. The
    /// alarm names both positions.
    PeerAhead(Alarm),
    /// The peer holds another entry at the position this device holds: a fork.
    Fork(Alarm),
    /// The peer's head does not verify: ignored, no alarm (anybody can send junk).
    BadSignature,
    /// This device pins no head of the peer's chain: there is nothing to compare it with.
    Unpinned,
}

impl PeerVerdict {
    /// The alarm to raise, if the verdict is one.
    pub fn alarm(&self) -> Option<&Alarm> {
        match self {
            PeerVerdict::PeerAhead(alarm) | PeerVerdict::Fork(alarm) => Some(alarm),
            _ => None,
        }
    }
}

/// Compares the signed head of a peer with this device's pin. The peer's signature is checked first
/// (`v`): a head that does not verify is `BadSignature` and raises nothing.
///
/// Implements planning/architecture.md section 5.9 (a hidden removal shows quickly) and section 6.10
/// V3 (a restore epoch the metadata group signs).
///
/// Caller: the peer-to-peer head exchange of the device clients (CHAIN-GLUE: LAN sync, calls).
pub fn compare_peer(local: &HeadPin, peer: &SignedHead, v: &dyn HeadVerifier) -> PeerVerdict {
    if !mutant!("chain_peer_skips_signature_check") && v.verify_head(peer).is_err() {
        return PeerVerdict::BadSignature;
    }
    let chain = peer.head.chain;
    let Some(ours) = local.head(&chain) else {
        return PeerVerdict::Unpinned;
    };
    if !mutant!("chain_peer_epoch_ignored") {
        // Heads of different restore epochs are not positions of one timeline. The epoch this
        // device stands in is the newest restore it accepted, or that of its pinned head when that
        // head was signed later (a head of a restore it has not been announced yet).
        let standing = match local.signed_head(&chain) {
            Some(signed) => signed.epoch.max(local.restore_epoch()),
            None => local.restore_epoch(),
        };
        if peer.epoch < standing {
            return PeerVerdict::PeerBehind;
        }
        if peer.epoch > standing {
            return PeerVerdict::PeerAhead(Alarm::Withheld {
                chain,
                ours: ours.seq,
                peer: peer.head.seq,
            });
        }
    }
    if peer.head.seq < ours.seq {
        return PeerVerdict::PeerBehind;
    }
    if peer.head.seq > ours.seq {
        if mutant!("chain_peer_ahead_is_quiet") {
            return PeerVerdict::Same;
        }
        return PeerVerdict::PeerAhead(Alarm::Withheld {
            chain,
            ours: ours.seq,
            peer: peer.head.seq,
        });
    }
    if peer.head.hash == ours.hash || mutant!("chain_peer_fork_is_quiet") {
        return PeerVerdict::Same;
    }
    PeerVerdict::Fork(Alarm::Fork {
        chain,
        seq: ours.seq,
        have: ours.hash,
        got: peer.head.hash,
    })
}
