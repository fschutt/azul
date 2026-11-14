//! `SignedKit`: a team log whose entries are made, signed, checked and applied through the production
//! functions (`verify_signatures`, `admit`, `apply`), with the fake signature scheme of `azlin_seams`
//! (`FakeScheme`: the "public key" of a key is the bytes of its id). The tests of the check build their
//! universes and their entries with it, so that every test enters through the function production calls.
//!
//! The convention that makes the keys cheap: device number `n` has the sign key `[n; 32]`, so a device
//! signs with `FakeScheme::sign(&KeyId::from_bytes([n; 32]), preimage)` and the log knows the same bytes
//! as its `sign_pub`. A delegate key signs with its own id; the payments key is `PAYMENTS_KEY`.

use azlin_policy::AdminQuorum;
use azlin_seams::{
    signing_input, AccountKind, ChainSeq, FakeScheme, Hash32, KeyId, MemberKind, Stamp,
};

use super::kit::{account, at, device, group, key, member};
use crate::admit::{admit, Refusal};
use crate::apply::{apply, Admitted};
use crate::devices::PublicKeys;
use crate::effects::Effects;
use crate::hash::entry_hash;
use crate::invite::invite_hash;
use crate::ops::{InviteSecret, Op};
use crate::proposal::{AssertionVerdict, RawEntry, Signer, TrustedKeys, TEAM_LOG_DOMAIN};
use crate::roles::needs_scope;
use crate::roles::Role;
use crate::state::TeamState;
use crate::verify::verify_signatures;

/// The payments service's public key in the tests.
pub(crate) const PAYMENTS_KEY: [u8; 32] = [0xFA; 32];

/// The keys of device number `n`: its sign key is `[n; 32]`.
pub(crate) fn dkeys(n: u8) -> PublicKeys {
    PublicKeys::classic([n; 32], [n.wrapping_add(100); 32])
}

/// The keys the check trusts from outside the log.
pub(crate) fn trusted() -> TrustedKeys {
    TrustedKeys::new(vec![PAYMENTS_KEY])
}

/// A signature of `preimage` by the key with these bytes.
pub(crate) fn sign_with(key_bytes: [u8; 32], preimage: &[u8]) -> Vec<u8> {
    FakeScheme::sign(&KeyId::from_bytes(key_bytes), preimage)
}

/// The `member_invite` of member `n` with the invite secret `[n; 32]`, into `groups`.
pub(crate) fn invite_op(n: u8, groups: &[u8]) -> Op {
    Op::MemberInvite {
        member: member(n),
        kind: MemberKind::Person,
        role: Role::Member,
        invite_hash: invite_hash(&InviteSecret([n; 32])),
        groups: groups.iter().map(|g| group(*g)).collect(),
        app: None,
    }
}

/// The `member_join` of member `n` with device `n` and the invite secret `[secret; 32]`.
pub(crate) fn join_op(n: u8, secret: u8) -> Op {
    Op::MemberJoin {
        member: member(n),
        device: device(n),
        member_keys: dkeys(n.wrapping_add(150)),
        device_keys: dkeys(n),
        secret: InviteSecret([secret; 32]),
    }
}

/// A team log under test.
#[derive(Clone)]
pub(crate) struct SignedKit {
    pub(crate) state: TeamState,
    /// The entries admitted so far, with the leader's stamp of each: what a device replays.
    pub(crate) log: Vec<(RawEntry, Stamp)>,
    now: u64,
}

impl SignedKit {
    /// An account of this kind whose log has no entry yet.
    pub(crate) fn empty(kind: AccountKind) -> SignedKit {
        SignedKit {
            state: TeamState::empty(account(), kind),
            log: Vec::new(),
            now: 1_000,
        }
    }

    /// An account of this kind with the genesis admitted: the owner is member 1 with device 1.
    pub(crate) fn new(kind: AccountKind) -> SignedKit {
        let mut kit = SignedKit::empty(kind);
        let genesis = Op::Genesis {
            owner: member(1),
            device: device(1),
            member_keys: dkeys(101),
            device_keys: dkeys(1),
        };
        let entry = kit.entry(Signer::Device(device(1)), genesis, &[], Vec::new(), None);
        kit.push(entry).expect("the genesis is admitted");
        kit
    }

    /// The leader's stamp of the next entry.
    pub(crate) fn stamp(&self) -> Stamp {
        at(self.now + 10)
    }

    /// Moves the leader's clock forward by `secs` (the next entry is stamped that much later).
    pub(crate) fn advance(&mut self, secs: u64) {
        self.now += secs;
    }

    /// The next entry of the log, signed: valid in every part. `cosigners` are device numbers.
    pub(crate) fn entry(
        &self,
        signer: Signer,
        op: Op,
        cosigners: &[u8],
        assertions: Vec<AssertionVerdict>,
        sealed: Option<Vec<u8>>,
    ) -> RawEntry {
        let (seq, prev) = match self.state.head() {
            Some(head) => (head.seq.next().expect("a short log"), head.hash),
            None => (ChainSeq::MIN, Hash32::ZERO),
        };
        let body = format!("{:?}|{seq}|{prev}|{op:?}", account());
        let preimage = signing_input(&TEAM_LOG_DOMAIN, body.as_bytes());
        let hash = entry_hash(&preimage);
        let sig = match signer {
            Signer::Device(device) => sign_with([device.as_bytes()[0]; 32], &preimage),
            Signer::DelegateKey(delegate) => FakeScheme::sign(&delegate, &preimage),
            Signer::PaymentsKey => sign_with(PAYMENTS_KEY, &preimage),
        };
        let cosigs = cosigners
            .iter()
            .map(|n| (device(*n), sign_with([*n; 32], &preimage)))
            .collect();
        RawEntry {
            account: account(),
            seq,
            prev,
            hash,
            at: self.stamp(),
            epoch: self.state.tip().epoch(),
            signer,
            op,
            sig,
            cosigs,
            assertions,
            preimage,
            sealed,
        }
    }

    /// An entry signed by device `device_n` with `cosigners`.
    pub(crate) fn entry_by(&self, device_n: u8, cosigners: &[u8], op: Op) -> RawEntry {
        self.entry(
            Signer::Device(device(device_n)),
            op,
            cosigners,
            Vec::new(),
            None,
        )
    }

    /// The production check: `verify_signatures` then `admit`, at the next stamp; nothing is applied.
    pub(crate) fn check(&self, entry: &RawEntry) -> Result<Admitted, Refusal> {
        let proposal = verify_signatures(&self.state, entry, &FakeScheme, &trusted())?;
        admit(&self.state, &proposal, self.stamp())
    }

    /// Whether the entry would be admitted, and why not.
    pub(crate) fn verdict(&self, entry: &RawEntry) -> Result<(), Refusal> {
        self.check(entry).map(|_| ())
    }

    /// The check, and the apply when it holds: what the metadata group does with an entry.
    pub(crate) fn push(&mut self, entry: RawEntry) -> Result<Effects, Refusal> {
        let admitted = self.check(&entry)?;
        let stamp = self.stamp();
        let effects = apply(&mut self.state, admitted);
        assert!(effects.applied, "an admitted entry applies");
        self.now += 10;
        self.log.push((entry, stamp));
        Ok(effects)
    }

    /// An entry signed by device `device_n`, pushed.
    pub(crate) fn send(&mut self, device_n: u8, op: Op) -> Result<Effects, Refusal> {
        self.send_with(device_n, &[], op)
    }

    /// An entry signed by device `device_n` with `cosigners`, pushed.
    pub(crate) fn send_with(
        &mut self,
        device_n: u8,
        cosigners: &[u8],
        op: Op,
    ) -> Result<Effects, Refusal> {
        let entry = self.entry_by(device_n, cosigners, op);
        self.push(entry)
    }

    /// `send`, and the entry must be admitted.
    pub(crate) fn must(&mut self, device_n: u8, op: Op) {
        if let Err(why) = self.send(device_n, op.clone()) {
            panic!("expected {op:?} to be admitted, refused: {why}");
        }
    }

    /// `send_with`, and the entry must be admitted.
    pub(crate) fn must_with(&mut self, device_n: u8, cosigners: &[u8], op: Op) {
        if let Err(why) = self.send_with(device_n, cosigners, op.clone()) {
            panic!("expected {op:?} to be admitted, refused: {why}");
        }
    }

    /// Invites member `n` by the owner (device 1) and joins it with device `n`, cosigned by the owner's
    /// device, into `groups`; then gives it `role` (with `scope` for a group admin or helpdesk member).
    /// The owner acts alone, so the account must have an admin quorum of 1 (a team account).
    pub(crate) fn enrol(&mut self, n: u8, role: Role, groups: &[u8], scope: Option<u8>) {
        self.must(1, invite_op(n, groups));
        let join = self.entry(
            Signer::Device(device(n)),
            join_op(n, n),
            &[1],
            Vec::new(),
            None,
        );
        self.push(join).expect("the join is admitted");
        if role != Role::Member {
            self.must(
                1,
                Op::MemberRole {
                    member: member(n),
                    role,
                    admin_scope: scope.map(group),
                },
            );
        }
    }

    /// Creates group `n` under group `parent` (or at the top) as the owner.
    pub(crate) fn make_group(&mut self, n: u8, parent: Option<u8>) {
        self.must(
            1,
            Op::GroupCreate {
                group: group(n),
                parent: parent.map(group),
            },
        );
    }

    /// The delegate key `n` signs: its id is its key.
    pub(crate) fn delegate_signer(n: u8) -> Signer {
        Signer::DelegateKey(key(n))
    }
}

/// The six roles, from the highest to the lowest.
pub(crate) const ROLES: [Role; 6] = [
    Role::Owner,
    Role::Admin,
    Role::GroupAdmin,
    Role::Helpdesk,
    Role::Member,
    Role::Guest,
];

/// The member number of the actor with this role in `universe`: owner 1, admin 3, group admin 5,
/// helpdesk 6, member 7, guest 8 (its device has the same number).
pub(crate) fn actor_of(role: Role) -> u8 {
    match role {
        Role::Owner => 1,
        Role::Admin => 3,
        Role::GroupAdmin => 5,
        Role::Helpdesk => 6,
        Role::Member => 7,
        Role::Guest => 8,
    }
}

/// The member number of the subject with this role: in the scope of the group admin and of helpdesk
/// (in group 3, below group 2) when `in_scope`, outside it (in group 4) otherwise.
pub(crate) fn subject_of(role: Role, in_scope: bool) -> u8 {
    let index = ROLES.iter().position(|r| *r == role).expect("a role") as u8;
    if in_scope {
        20 + index
    } else {
        30 + index
    }
}

/// The universe of the authority matrix: a team account (so the owner can build it alone) whose admin
/// quorum is then raised to 2, with
///
/// - groups 1 (top) > 2 > 3, and 4 beside 2 under 1;
/// - the actors: owner 1 (devices 1 and 11), owner 2, admins 3 and 4, group admin 5 and helpdesk 6 (scope
///   group 2, members of it), member 7 and guest 8 (in group 3);
/// - a subject of every role in the scope of the group admin (numbers 20 to 25, in group 3) and one
///   outside it (30 to 35, in group 4), group admins and helpdesk with the scope of their own group;
/// - member 40, who belongs to no group;
/// - every member managed (its own device confirmed).
pub(crate) fn universe() -> SignedKit {
    let mut kit = SignedKit::new(AccountKind::Team);
    kit.make_group(1, None);
    kit.make_group(2, Some(1));
    kit.make_group(3, Some(2));
    kit.make_group(4, Some(1));
    kit.must(
        1,
        Op::DeviceAdd {
            member: member(1),
            device: device(11),
            keys: dkeys(11),
        },
    );
    kit.enrol(2, Role::Owner, &[], None);
    kit.enrol(3, Role::Admin, &[], None);
    kit.enrol(4, Role::Admin, &[], None);
    kit.enrol(5, Role::GroupAdmin, &[2], Some(2));
    kit.enrol(6, Role::Helpdesk, &[2], Some(2));
    kit.enrol(7, Role::Member, &[3], None);
    kit.enrol(8, Role::Guest, &[3], None);
    for role in ROLES {
        let scope_in = if needs_scope(role) { Some(3) } else { None };
        let scope_out = if needs_scope(role) { Some(4) } else { None };
        kit.enrol(subject_of(role, true), role, &[3], scope_in);
        kit.enrol(subject_of(role, false), role, &[4], scope_out);
    }
    kit.enrol(40, Role::Member, &[], None);
    // Every member is managed: the member's own device confirms (the owner's second device for the owner).
    let members: Vec<u8> = (1..=8u8)
        .chain(20..=25)
        .chain(30..=35)
        .chain([40])
        .collect();
    for n in members {
        let confirm = if n == 1 { 11 } else { n };
        kit.must_with(
            1,
            &[confirm],
            Op::MemberManaged {
                member: member(n),
                managed: true,
            },
        );
    }
    kit.must(
        1,
        Op::AdminQuorumSet {
            quorum: AdminQuorum::TWO,
        },
    );
    kit
}
