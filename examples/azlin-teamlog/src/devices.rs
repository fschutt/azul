//! Devices of the team log: the keys, the state and the hold of a device (planning/architecture.md
//! section 5.3 `reg_device`, section 7.5 "Notice and hold"; build unit S17, lane W1-S17c1).
//!
//! A device is a row of the registry that the team log creates (`genesis`, `member_join`,
//! `device_add`) and revokes (`device_remove`, `member_remove`). Its `state` is `pending (in its
//! hold)`, `active` or `revoked`: a device added without the person's own device starts `Pending` and
//! its hold ends at `hold_until`, a time the metadata group computes from the leader's stamp and the
//! policy's `enroll_hold` ([`DeviceState::Pending`]). Nothing writes the end of a hold into the log, so
//! whether a device may sign is a function of its state AND of the leader's stamp:
//! [`DeviceState::is_active_at`].

use azlin_seams::{ChainSeq, DeviceId, MemberId, Stamp};

/// The public keys of a member or a device: Ed25519 to sign, X25519 for key exchange and, where the
/// account uses the hybrid envelope, the ML-KEM-768 encapsulation key (planning/architecture.md
/// section 5.3 `sign_pub`, `kx_pub`, `pq_pub`).
///
/// Implements planning/architecture.md section 5.3 (`reg_member` and `reg_device` key columns).
///
/// Caller: `Op::Genesis`, `Op::DeviceAdd`, `Op::MemberJoin` (the keys the entry registers), `Device`,
/// `Member` (the rows), `TeamState::device_key`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicKeys {
    /// The Ed25519 public key that verifies the signatures (`sign_pub`).
    pub sign: [u8; 32],
    /// The X25519 public key (`kx_pub`).
    pub kx: [u8; 32],
    /// The ML-KEM-768 encapsulation key, when the account uses the hybrid envelope (`pq_pub`).
    pub pq: Option<Vec<u8>>,
}

impl PublicKeys {
    /// The longest `pq_pub` an entry may carry: an ML-KEM-768 encapsulation key is 1,184 bytes, and the
    /// bound leaves room for the next parameter set (est.).
    ///
    /// Implements planning/architecture.md section 1.4 rule 4 (everything has a limit, stated up
    /// front) and section 5.3 (`pq_pub`).
    ///
    /// Caller: `admit.rs` (`check_keys`: a longer key is `Invalid(Args)`), `invite.rs` (`check_join`).
    pub const PQ_MAX_LEN: usize = 2048;

    /// Keys with an Ed25519 and an X25519 key and no post-quantum key.
    ///
    /// Implements planning/architecture.md section 5.3 (`reg_device` key columns).
    ///
    /// Caller: the glue that decodes a wire entry; the tests.
    pub const fn classic(sign: [u8; 32], kx: [u8; 32]) -> PublicKeys {
        PublicKeys { sign, kx, pq: None }
    }
}

/// The state of a device: in its hold, active, or revoked (planning/architecture.md section 5.3
/// `reg_device.state`: "pending (in its hold), active, revoked").
///
/// Implements planning/architecture.md section 5.3 and section 7.5 (the enrollment hold).
///
/// Caller: `Device`, `apply.rs` (the transitions), `admit.rs` (a pending device signs nothing).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeviceState {
    /// In its enrollment hold until `hold_until` (the leader's stamp): the device signs nothing and
    /// the group withholds every wrap for it until then (planning/architecture.md section 7.5).
    Pending {
        /// The exact time the hold ends.
        hold_until: Stamp,
    },
    /// An enrolled device.
    Active,
    /// A removed device: it signs nothing, for ever.
    Revoked,
}

impl DeviceState {
    /// Whether a device in this state may sign at the leader's stamp `now`: an `Active` one, and a
    /// `Pending` one whose hold has ended (`now` is at or after `hold_until`). A `Revoked` one never.
    ///
    /// Implements planning/architecture.md section 7.5 ("The group withholds the wrap until
    /// `hold_until`") and the authority table v0 ("a Pending device, one in its hold, signs nothing").
    ///
    /// Caller: `TeamState::device_is_active_at`, `admit.rs` (the signer and the cosigners).
    pub fn is_active_at(&self, now: Stamp) -> bool {
        match *self {
            DeviceState::Active => true,
            DeviceState::Pending { hold_until } => now >= hold_until,
            DeviceState::Revoked => false,
        }
    }

    /// The word of the state in the registry's column: `pending`, `active` or `revoked`.
    ///
    /// Implements planning/architecture.md section 5.3 (`reg_device.state`).
    ///
    /// Caller: the registry glue (TEAMLOG-APPLY); the tests.
    pub const fn word(&self) -> &'static str {
        match self {
            DeviceState::Pending { .. } => "pending",
            DeviceState::Active => "active",
            DeviceState::Revoked => "revoked",
        }
    }
}

/// How a device was enrolled (planning/architecture.md section 5.3 `reg_device.enrolled_by`: "own
/// device, quorum, recovery code"), plus the two ways a FIRST device comes into being.
///
/// Implements planning/architecture.md section 5.3 and section 7.5 (the paths of a new device).
///
/// Caller: `Device`, `apply.rs`, the registry glue (maps `Genesis` and `Invite` to the paths that
/// need no hold).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EnrolledBy {
    /// The account's first device, made by the `genesis` entry.
    Genesis,
    /// The first device of a member who joined by an invitation (`member_join`).
    Invite,
    /// Another device of the same member approved it: no hold.
    OwnDevice,
    /// Admins approved it by quorum: the hold applies.
    Quorum,
}

impl EnrolledBy {
    /// The word of the path.
    ///
    /// Implements planning/architecture.md section 5.3 (`reg_device.enrolled_by`).
    ///
    /// Caller: the registry glue (TEAMLOG-APPLY); the tests.
    pub const fn word(self) -> &'static str {
        match self {
            EnrolledBy::Genesis => "genesis",
            EnrolledBy::Invite => "invite",
            EnrolledBy::OwnDevice => "own_device",
            EnrolledBy::Quorum => "quorum",
        }
    }
}

/// One device of the account, as the team log defines it: the columns of `reg_device` that the log
/// writes. The columns the log does not write (`push_sealed`, `p2p_id`, `wipe_requested_at`,
/// `last_seen`, the evidence kind) belong to the registry's own commands.
///
/// Implements planning/architecture.md section 5.3 (`reg_device`: `device_id`, `account_id`,
/// `member_id`, the keys, `state`, `enrolled_by`, `hold_until`, `label_sealed`).
///
/// Caller: `TeamState::devices`, `RegistryDelta::Device`, `admit.rs` (the signer's device).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// The id of the device.
    pub id: DeviceId,
    /// The member the device belongs to.
    pub member: MemberId,
    /// The device's keys.
    pub keys: PublicKeys,
    /// Pending (in its hold), active or revoked.
    pub state: DeviceState,
    /// How the device was enrolled.
    pub enrolled_by: EnrolledBy,
    /// The device's name, sealed to the member (`label_sealed`): the `sealed` field of the entry that
    /// added the device.
    pub label_sealed: Option<Vec<u8>>,
    /// The leader's stamp of the entry that added the device.
    pub added_at: Stamp,
    /// The leader's stamp of the entry that revoked the device.
    pub revoked_at: Option<Stamp>,
    /// The position of the newest entry that touched the device.
    pub log_seq: ChainSeq,
}
