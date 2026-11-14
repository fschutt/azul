//! Helpers the tests of this crate share (a test-only file, declared under `#[cfg(test)]` in lib.rs):
//! ids and keys built from one byte, and `Kit`, a team log whose entries are applied WITHOUT the
//! permission check (the `Admitted` witness is made by the crate-private constructor), so that the
//! tests of the state, the hash and `apply` need no signatures. The tests of the check build their
//! entries through `admit` (see `admit_tests.rs`).

use azlin_policy::PolicyPatch;
use azlin_seams::{
    hash32, AccountId, AccountKind, ChainSeq, DeviceId, GroupId, Hash32, KeyId, MemberId,
    MemberKind, RestoreEpoch, Stamp, UnixTime,
};

use crate::apply::{apply, Actor, Admitted, EntryMeta};
use crate::devices::PublicKeys;
use crate::effects::Effects;
use crate::members::AppSpec;
use crate::ops::{
    AppName, EntitlementState, EntitlementVersion, EscrowReason, InviteSecret, Op, OpName,
    PolicyScope,
};
use crate::roles::Role;
use crate::state::TeamState;

/// A stamp.
pub(crate) fn at(secs: u64) -> Stamp {
    UnixTime::from_secs(secs)
}

/// The account of the tests.
pub(crate) fn account() -> AccountId {
    AccountId::from_bytes([0xAA; 16])
}

/// The member with this number.
pub(crate) fn member(n: u8) -> MemberId {
    MemberId::from_bytes([n; 16])
}

/// The device with this number.
pub(crate) fn device(n: u8) -> DeviceId {
    DeviceId::from_bytes([n; 16])
}

/// The group with this number.
pub(crate) fn group(n: u8) -> GroupId {
    GroupId::from_bytes([n; 16])
}

/// The delegate key with this number.
pub(crate) fn key(n: u8) -> KeyId {
    KeyId::from_bytes([n; 32])
}

/// Public keys made from one byte.
pub(crate) fn keys(n: u8) -> PublicKeys {
    PublicKeys::classic([n; 32], [n.wrapping_add(100); 32])
}

/// A hash made from one byte.
pub(crate) fn hash(n: u8) -> Hash32 {
    Hash32::from_bytes([n; 32])
}

/// The actor of a device of a member.
pub(crate) fn by(member_n: u8, device_n: u8) -> Actor {
    Actor::Device {
        device: device(device_n),
        member: member(member_n),
    }
}

/// A team log under test: the state, and the position, the hash and the clock of the next entry.
pub(crate) struct Kit {
    pub(crate) state: TeamState,
    seq: u64,
    prev: Hash32,
    now: u64,
}

impl Kit {
    /// An account of this kind with the genesis applied: the owner is `member(1)` with `device(1)`.
    pub(crate) fn new(kind: AccountKind) -> Kit {
        Kit::with_genesis(kind).0
    }

    /// An account of this kind with the genesis applied, and the effects of the genesis.
    pub(crate) fn with_genesis(kind: AccountKind) -> (Kit, Effects) {
        let mut kit = Kit {
            state: TeamState::empty(account(), kind),
            seq: 0,
            prev: Hash32::ZERO,
            now: 1_000,
        };
        let effects = kit.step_with(
            by(1, 1),
            Vec::new(),
            Op::Genesis {
                owner: member(1),
                device: device(1),
                member_keys: keys(1),
                device_keys: keys(101),
            },
            None,
        );
        assert!(effects.applied, "the genesis applies to an empty state");
        (kit, effects)
    }

    /// Sets the leader's clock: the next entry is stamped `secs + 10`.
    pub(crate) fn set_clock(&mut self, secs: u64) {
        self.now = secs;
    }

    /// The leader's stamp of the next entry.
    pub(crate) fn next_stamp(&self) -> Stamp {
        at(self.now + 10)
    }

    /// Applies `op` as the device `device_n` of the member `member_n`.
    pub(crate) fn step(&mut self, member_n: u8, device_n: u8, op: Op) -> Effects {
        self.step_with(by(member_n, device_n), Vec::new(), op, None)
    }

    /// Applies `op` with a sealed field.
    pub(crate) fn step_sealed(
        &mut self,
        member_n: u8,
        device_n: u8,
        op: Op,
        sealed: Vec<u8>,
    ) -> Effects {
        self.step_with(by(member_n, device_n), Vec::new(), op, Some(sealed))
    }

    /// Applies `op` with every parameter of the witness.
    pub(crate) fn step_with(
        &mut self,
        actor: Actor,
        cosigners: Vec<MemberId>,
        op: Op,
        sealed: Option<Vec<u8>>,
    ) -> Effects {
        let entry_hash = hash32(
            "azlin-teamlog-test-entry",
            &[&self.seq.to_be_bytes(), format!("{op:?}").as_bytes()],
        );
        let meta = EntryMeta {
            account: account(),
            seq: ChainSeq::new(self.seq),
            prev: self.prev,
            hash: entry_hash,
            epoch: RestoreEpoch::ZERO,
        };
        self.now += 10;
        let admitted = Admitted::new(meta, at(self.now), actor, cosigners, op, sealed);
        let effects = apply(&mut self.state, admitted);
        if effects.applied {
            self.prev = entry_hash;
            self.seq += 1;
        }
        effects
    }

    /// The next entry's meta, for a test that makes an `Admitted` by hand.
    pub(crate) fn next_meta(&self) -> EntryMeta {
        EntryMeta {
            account: account(),
            seq: ChainSeq::new(self.seq),
            prev: self.prev,
            hash: hash32(
                "azlin-teamlog-test-entry",
                &[&self.seq.to_be_bytes(), b"by hand"],
            ),
            epoch: RestoreEpoch::ZERO,
        }
    }

    /// Invites `member(n)` with `role`, into `groups`, as the owner.
    pub(crate) fn invite(&mut self, n: u8, role: Role, groups: &[u8]) -> Effects {
        self.step(
            1,
            1,
            Op::MemberInvite {
                member: member(n),
                kind: MemberKind::Person,
                role,
                invite_hash: hash(n),
                groups: groups.iter().map(|g| group(*g)).collect(),
                app: None,
            },
        )
    }

    /// Joins `member(n)` with `device(n)`.
    pub(crate) fn join(&mut self, n: u8) -> Effects {
        self.step(
            n,
            n,
            Op::MemberJoin {
                member: member(n),
                device: device(n),
                member_keys: keys(n),
                device_keys: keys(n.wrapping_add(50)),
                secret: InviteSecret([n; 32]),
            },
        )
    }

    /// Invites `member(n)` and joins it: an Active member with one Active device.
    pub(crate) fn enrol(&mut self, n: u8, role: Role, groups: &[u8]) {
        assert!(self.invite(n, role, groups).applied);
        assert!(self.join(n).applied);
    }

    /// Creates `group(n)` under `parent`, as the owner.
    pub(crate) fn make_group(&mut self, n: u8, parent: Option<u8>) -> Effects {
        self.step(
            1,
            1,
            Op::GroupCreate {
                group: group(n),
                parent: parent.map(group),
            },
        )
    }
}

/// A business account with every kind of row in it: groups (three, nested), members of every state,
/// devices of every state, a group patch, a delegation, the escrow, the audit key, an entitlement and
/// a waiting escrow opening. The base of the state-hash tests.
pub(crate) fn rich_business() -> Kit {
    let mut kit = Kit::new(AccountKind::Business);
    assert!(kit.make_group(11, None).applied);
    assert!(
        kit.step_sealed(
            1,
            1,
            Op::GroupCreate {
                group: group(12),
                parent: Some(group(11)),
            },
            vec![1, 2, 3],
        )
        .applied
    );
    assert!(kit.make_group(13, Some(11)).applied);
    // m2: an admin; m3: a group admin; m4: invited only; m5: suspended; m6: removed.
    kit.enrol(2, Role::Member, &[12]);
    assert!(
        kit.step(
            1,
            1,
            Op::MemberRole {
                member: member(2),
                role: Role::Admin,
                admin_scope: None,
            },
        )
        .applied
    );
    kit.enrol(3, Role::Member, &[13]);
    assert!(
        kit.step(
            1,
            1,
            Op::MemberRole {
                member: member(3),
                role: Role::GroupAdmin,
                admin_scope: Some(group(11)),
            },
        )
        .applied
    );
    assert!(kit.invite(4, Role::Guest, &[]).applied);
    // m9: an app principal, invited (it has an expiry and a scope).
    let mut scopes = std::collections::BTreeSet::new();
    scopes.insert(AppName::new("notes").expect("a valid app name"));
    assert!(
        kit.step(
            1,
            1,
            Op::MemberInvite {
                member: member(9),
                kind: MemberKind::App,
                role: Role::Member,
                invite_hash: hash(9),
                groups: std::collections::BTreeSet::new(),
                app: Some(AppSpec {
                    scopes,
                    expires_at: Some(at(7_000_000)),
                }),
            },
        )
        .applied
    );
    kit.enrol(5, Role::Member, &[]);
    assert!(
        kit.step(1, 1, Op::MemberSuspend { member: member(5) })
            .applied
    );
    kit.enrol(6, Role::Member, &[13]);
    assert!(
        kit.step(1, 1, Op::MemberRemove { member: member(6) })
            .applied
    );
    // m2 adds its own device (Active at once); the owner adds one for m3 by quorum (Pending).
    assert!(
        kit.step(
            2,
            2,
            Op::DeviceAdd {
                member: member(2),
                device: device(7),
                keys: keys(7),
            },
        )
        .applied
    );
    assert!(
        kit.step_with(
            by(1, 1),
            vec![member(2)],
            Op::DeviceAdd {
                member: member(3),
                device: device(8),
                keys: keys(8),
            },
            Some(vec![9, 9]),
        )
        .applied
    );
    // A device of m3 is removed.
    assert!(
        kit.step(1, 1, Op::DeviceRemove { device: device(3) })
            .applied
    );
    // A patch of a group's policy, and a membership.
    let patch = PolicyPatch {
        credential_ttl: Some(azlin_seams::Span::minutes(30)),
        ..PolicyPatch::none()
    };
    assert!(
        kit.step(
            1,
            1,
            Op::PolicySet {
                scope: PolicyScope::Group(group(12)),
                patch,
            },
        )
        .applied
    );
    assert!(
        kit.step(
            1,
            1,
            Op::GroupMember {
                group: group(13),
                member: member(4),
                present: true,
            },
        )
        .applied
    );
    assert!(
        kit.step(
            1,
            1,
            Op::MemberManaged {
                member: member(2),
                managed: false,
            },
        )
        .applied
    );
    let mut ops = std::collections::BTreeSet::new();
    ops.insert(OpName::MemberSuspend);
    assert!(
        kit.step(
            1,
            1,
            Op::Delegate {
                key: key(40),
                ops,
                expires: Some(at(9_000_000)),
            },
        )
        .applied
    );
    assert!(
        kit.step(
            1,
            1,
            Op::EscrowSet {
                escrow_pub: [0xE5; 32],
                holders: [member(1), member(2)].into_iter().collect(),
            },
        )
        .applied
    );
    assert!(
        kit.step(
            1,
            1,
            Op::AuditKeySet {
                audit_pub: [0xA7; 32]
            }
        )
        .applied
    );
    assert!(
        kit.step_with(
            Actor::Payments,
            Vec::new(),
            Op::Entitlement {
                version: EntitlementVersion(3),
                state: EntitlementState::Active,
                quota: azlin_seams::ByteCount::gib(100),
                seats: 25,
            },
            None,
        )
        .applied
    );
    assert!(
        kit.step_with(
            by(1, 1),
            vec![member(2)],
            Op::EscrowOpen {
                member: member(2),
                reason: EscrowReason::LegalRequest,
            },
            None,
        )
        .applied
    );
    assert!(
        kit.step(
            2,
            2,
            Op::App {
                name: AppName::new("notes").expect("a valid app name"),
                body: vec![1, 2, 3],
            },
        )
        .applied
    );
    kit
}
