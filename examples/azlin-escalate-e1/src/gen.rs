//! The seeded generator of entries: the setup of a universe, valid entries, entries that are wrong in
//! exactly one thing (near misses), and random ones (planning/architecture.md section 12 E1: "random
//! sequences of ops, group moves and delegations"; lane W1-S47a).
//!
//! The generator reads the MODEL's state (never the production's) and uses the model as its oracle: a "valid"
//! entry is one the model admits, found by sampling and by adding the cosignatures the model says the op
//! needs; a "near miss" is a valid entry changed in one place, kept only when the model refuses it with the
//! intended class and nothing else (or nothing but a named companion class). The differential then asks the
//! production check the same question, so a rule that the model and the production disagree on shows up
//! whichever of them is wrong. It imports nothing from `azlin-teamlog`.

use std::collections::BTreeSet;

use azlin_seams::sim::Rng;
use azlin_seams::{Span, Stamp};

use crate::model::{
    model_admit, model_violations, plus, quorum_requirement, AccountSpec, AcctKind, AppId, Class,
    CosigClaim, DState, Dev, DevApproval, EpochSpec, ExtSharing, Grp, Key, MApp, MAssertion, MKind,
    MOp, MPatch, MSigner, MState, Mem, ModelEntry, ModelState, OpKind, PField, PScope, PrevSpec,
    Reason, Role, SeqSpec, SigClaim, Verdict,
};
use crate::universe::{between, pct, pick, Universe, UNKNOWN_DEV, UNKNOWN_GRP, UNKNOWN_MEM};

/// One entry and the leader's stamp it is offered at.
///
/// Implements planning/architecture.md section 2.4 (time is the leader's stamp); Caller: `compare.rs`,
/// `stories.rs`, the tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// The entry.
    pub entry: ModelEntry,
    /// The leader's stamp.
    pub now: Stamp,
}

/// The kinds of near miss: a valid entry changed in one place. The first eighteen are the brief's check 3;
/// the rest cover the guards and classes the brief's list leaves out (ARCH29 9.13 X-D-54: the stories and
/// near misses attack the table itself).
#[allow(missing_docs)] // the variants are named by the brief's words; `name()` and `expected()` say the rest
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NearMiss {
    SignerOneRoleTooLow,
    SignerScopeOffByOneGroup,
    QuorumShortByOnePerson,
    QuorumMetByTwoDevicesOfOnePerson,
    CosigByASuspendedAdmin,
    CosigOverAnotherEntryHash,
    WrongPrevOrSeq,
    DelegationExpiredByOneSecond,
    DelegateOpOutsideTheNamedSet,
    DelegateNamesARoleOrWrapOp,
    GroupMoveIntoOwnSubtree,
    LastOwnerDemotedRemovedOrSuspended,
    SelfRoleChange,
    HelpdeskAddsADeviceAlone,
    EntitlementSignedByADevice,
    AppOpByAGuest,
    PendingDeviceSigns,
    PolicySetThatLoosensAGroup,
    SuspendedMemberSigns,
    RevokedDeviceSigns,
    ForgedSignature,
    CorruptCopy,
    StampBackwards,
    EpochBelowChain,
    WrongAccount,
    UnknownWireOp,
    EscrowOpenWithoutAssertion,
    JoinWithWrongSecret,
    PrivateToManagedWithoutConsent,
    DeviceAddQuorumUnderSelfPolicy,
    GroupAdminPullsAnAdminIntoScope,
    AdminEnrollsADeviceForTheOwner,
    DelegateSuspendsAnOwner,
    EntitlementNotNewer,
    ReusedId,
    RootGroupMoves,
}

impl NearMiss {
    /// Every kind.
    pub const ALL: [NearMiss; 36] = [
        NearMiss::SignerOneRoleTooLow,
        NearMiss::SignerScopeOffByOneGroup,
        NearMiss::QuorumShortByOnePerson,
        NearMiss::QuorumMetByTwoDevicesOfOnePerson,
        NearMiss::CosigByASuspendedAdmin,
        NearMiss::CosigOverAnotherEntryHash,
        NearMiss::WrongPrevOrSeq,
        NearMiss::DelegationExpiredByOneSecond,
        NearMiss::DelegateOpOutsideTheNamedSet,
        NearMiss::DelegateNamesARoleOrWrapOp,
        NearMiss::GroupMoveIntoOwnSubtree,
        NearMiss::LastOwnerDemotedRemovedOrSuspended,
        NearMiss::SelfRoleChange,
        NearMiss::HelpdeskAddsADeviceAlone,
        NearMiss::EntitlementSignedByADevice,
        NearMiss::AppOpByAGuest,
        NearMiss::PendingDeviceSigns,
        NearMiss::PolicySetThatLoosensAGroup,
        NearMiss::SuspendedMemberSigns,
        NearMiss::RevokedDeviceSigns,
        NearMiss::ForgedSignature,
        NearMiss::CorruptCopy,
        NearMiss::StampBackwards,
        NearMiss::EpochBelowChain,
        NearMiss::WrongAccount,
        NearMiss::UnknownWireOp,
        NearMiss::EscrowOpenWithoutAssertion,
        NearMiss::JoinWithWrongSecret,
        NearMiss::PrivateToManagedWithoutConsent,
        NearMiss::DeviceAddQuorumUnderSelfPolicy,
        NearMiss::GroupAdminPullsAnAdminIntoScope,
        NearMiss::AdminEnrollsADeviceForTheOwner,
        NearMiss::DelegateSuspendsAnOwner,
        NearMiss::EntitlementNotNewer,
        NearMiss::ReusedId,
        NearMiss::RootGroupMoves,
    ];

    /// The brief's name of the kind (check 3), snake case.
    pub const fn name(self) -> &'static str {
        match self {
            NearMiss::SignerOneRoleTooLow => "signer_one_role_too_low",
            NearMiss::SignerScopeOffByOneGroup => "signer_scope_off_by_one_group",
            NearMiss::QuorumShortByOnePerson => "quorum_short_by_one_person",
            NearMiss::QuorumMetByTwoDevicesOfOnePerson => "quorum_met_by_two_devices_of_one_person",
            NearMiss::CosigByASuspendedAdmin => "cosig_by_a_suspended_admin",
            NearMiss::CosigOverAnotherEntryHash => "cosig_over_another_entry_hash",
            NearMiss::WrongPrevOrSeq => "wrong_prev_or_seq",
            NearMiss::DelegationExpiredByOneSecond => "delegation_expired_by_one_second",
            NearMiss::DelegateOpOutsideTheNamedSet => "delegate_op_outside_the_named_set",
            NearMiss::DelegateNamesARoleOrWrapOp => "delegate_names_a_role_or_wrap_op",
            NearMiss::GroupMoveIntoOwnSubtree => "group_move_into_own_subtree",
            NearMiss::LastOwnerDemotedRemovedOrSuspended => {
                "last_owner_demoted_removed_or_suspended"
            }
            NearMiss::SelfRoleChange => "self_role_change",
            NearMiss::HelpdeskAddsADeviceAlone => "helpdesk_adds_a_device_alone",
            NearMiss::EntitlementSignedByADevice => "entitlement_signed_by_a_device",
            NearMiss::AppOpByAGuest => "app_op_by_a_guest",
            NearMiss::PendingDeviceSigns => "pending_device_signs",
            NearMiss::PolicySetThatLoosensAGroup => "policy_set_that_loosens_a_group",
            NearMiss::SuspendedMemberSigns => "suspended_member_signs",
            NearMiss::RevokedDeviceSigns => "revoked_device_signs",
            NearMiss::ForgedSignature => "forged_signature",
            NearMiss::CorruptCopy => "corrupt_copy",
            NearMiss::StampBackwards => "stamp_backwards",
            NearMiss::EpochBelowChain => "epoch_below_chain",
            NearMiss::WrongAccount => "wrong_account",
            NearMiss::UnknownWireOp => "unknown_wire_op",
            NearMiss::EscrowOpenWithoutAssertion => "escrow_open_without_assertion",
            NearMiss::JoinWithWrongSecret => "join_with_wrong_secret",
            NearMiss::PrivateToManagedWithoutConsent => "private_to_managed_without_consent",
            NearMiss::DeviceAddQuorumUnderSelfPolicy => "device_add_quorum_under_self_policy",
            NearMiss::GroupAdminPullsAnAdminIntoScope => "group_admin_pulls_an_admin_into_scope",
            NearMiss::AdminEnrollsADeviceForTheOwner => "admin_enrolls_a_device_for_the_owner",
            NearMiss::DelegateSuspendsAnOwner => "delegate_suspends_an_owner",
            NearMiss::EntitlementNotNewer => "entitlement_not_newer",
            NearMiss::ReusedId => "reused_id",
            NearMiss::RootGroupMoves => "root_group_moves",
        }
    }

    /// The class the near miss aims at; the verdict of both sides must be a refusal with it.
    pub const fn expected(self) -> Class {
        match self {
            NearMiss::SignerOneRoleTooLow
            | NearMiss::SignerScopeOffByOneGroup
            | NearMiss::SelfRoleChange
            | NearMiss::HelpdeskAddsADeviceAlone
            | NearMiss::EntitlementSignedByADevice
            | NearMiss::AppOpByAGuest
            | NearMiss::GroupAdminPullsAnAdminIntoScope
            | NearMiss::AdminEnrollsADeviceForTheOwner
            | NearMiss::DelegateSuspendsAnOwner => Class::RoleOrScope,
            NearMiss::QuorumShortByOnePerson
            | NearMiss::QuorumMetByTwoDevicesOfOnePerson
            | NearMiss::CosigByASuspendedAdmin
            | NearMiss::CosigOverAnotherEntryHash => Class::QuorumShort,
            NearMiss::WrongPrevOrSeq
            | NearMiss::StampBackwards
            | NearMiss::EpochBelowChain
            | NearMiss::WrongAccount => Class::Chain,
            NearMiss::DelegationExpiredByOneSecond => Class::DelegationExpired,
            NearMiss::DelegateOpOutsideTheNamedSet | NearMiss::DelegateNamesARoleOrWrapOp => {
                Class::DelegationDoesNotCover
            }
            NearMiss::GroupMoveIntoOwnSubtree => Class::GroupCycle,
            NearMiss::LastOwnerDemotedRemovedOrSuspended => Class::LastOwner,
            NearMiss::PendingDeviceSigns
            | NearMiss::SuspendedMemberSigns
            | NearMiss::RevokedDeviceSigns => Class::SignerNotActive,
            NearMiss::PolicySetThatLoosensAGroup => Class::PolicyLoosens,
            NearMiss::ForgedSignature | NearMiss::CorruptCopy => Class::Signature,
            NearMiss::UnknownWireOp => Class::UnknownOp,
            NearMiss::EscrowOpenWithoutAssertion => Class::AssertionMissing,
            NearMiss::JoinWithWrongSecret => Class::InviteInvalid,
            NearMiss::PrivateToManagedWithoutConsent => Class::ConsentMissing,
            NearMiss::DeviceAddQuorumUnderSelfPolicy | NearMiss::EntitlementNotNewer => {
                Class::PolicyForbids
            }
            NearMiss::ReusedId | NearMiss::RootGroupMoves => Class::Invalid,
        }
    }

    /// The classes besides the expected one that the model may also report for the entry (the same
    /// mistake breaks a second rule: a self role change by the last owner is also "the last owner").
    pub const fn companions(self) -> &'static [Class] {
        match self {
            NearMiss::LastOwnerDemotedRemovedOrSuspended => {
                &[Class::RoleOrScope, Class::QuorumShort]
            }
            NearMiss::SelfRoleChange => &[Class::QuorumShort, Class::LastOwner],
            NearMiss::HelpdeskAddsADeviceAlone => &[
                Class::QuorumShort,
                Class::PolicyForbids,
                Class::SubjectUnknown,
                Class::Invalid,
            ],
            NearMiss::QuorumShortByOnePerson
            | NearMiss::CosigByASuspendedAdmin
            | NearMiss::CosigOverAnotherEntryHash
            | NearMiss::QuorumMetByTwoDevicesOfOnePerson => &[Class::AssertionMissing],
            NearMiss::DelegateNamesARoleOrWrapOp | NearMiss::DelegateOpOutsideTheNamedSet => {
                &[Class::RoleOrScope]
            }
            NearMiss::GroupMoveIntoOwnSubtree => &[Class::Invalid],
            NearMiss::DelegateSuspendsAnOwner => &[Class::QuorumShort, Class::LastOwner],
            NearMiss::AdminEnrollsADeviceForTheOwner => &[Class::QuorumShort],
            NearMiss::DeviceAddQuorumUnderSelfPolicy => &[Class::QuorumShort],
            _ => &[],
        }
    }
}

/// What kind of entry to generate.
///
/// Implements planning/architecture.md section 12 E1; Caller: `compare.rs`, the tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// An entry the model admits.
    Valid,
    /// An entry wrong in one place (see [`NearMiss`]).
    NearMiss(NearMiss),
    /// Whatever: a candidate, perhaps perturbed.
    Random,
}

/// The state the generator draws from.
struct G<'a> {
    rng: &'a mut Rng,
    u: &'a Universe,
    s: &'a ModelState,
    now: Stamp,
    eid: &'a mut u64,
}

/// The op kinds a valid entry is drawn from and their weights.
const VALID_WEIGHTS: [(OpKind, u32); 19] = [
    (OpKind::App, 12),
    (OpKind::MemberInvite, 8),
    (OpKind::MemberJoin, 9),
    (OpKind::DeviceAdd, 8),
    (OpKind::DeviceRemove, 4),
    (OpKind::MemberRole, 10),
    (OpKind::MemberSuspend, 3),
    (OpKind::MemberRemove, 2),
    (OpKind::MemberManaged, 5),
    (OpKind::GroupCreate, 4),
    (OpKind::GroupMember, 8),
    (OpKind::GroupMove, 4),
    (OpKind::PolicySet, 9),
    (OpKind::EscrowSet, 2),
    (OpKind::AuditKeySet, 1),
    (OpKind::AdminQuorumSet, 3),
    (OpKind::EscrowOpen, 5),
    (OpKind::Delegate, 5),
    (OpKind::Entitlement, 2),
];

/// The roles that act on the account: owner, admin and group admin.
const ADMINISH: [Role; 3] = [Role::Owner, Role::Admin, Role::GroupAdmin];
/// The roles of the whole account.
const OWNER_ADMIN: [Role; 2] = [Role::Owner, Role::Admin];

impl G<'_> {
    fn pct(&mut self, p: u32) -> bool {
        pct(self.rng, p)
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> Option<T> {
        pick(self.rng, items).copied()
    }

    fn new_eid(&mut self) -> u64 {
        *self.eid = self.eid.saturating_add(1);
        *self.eid
    }

    /// A fresh entry signed by a device, every claim valid.
    fn entry(&mut self, signer: Dev, op: MOp) -> ModelEntry {
        let id = self.new_eid();
        ModelEntry::new(id, MSigner::Device(signer), op)
    }

    fn step(&self, entry: ModelEntry) -> Step {
        Step {
            entry,
            now: self.now,
        }
    }

    /// The devices that may sign now, with their members, whose member has one of the roles.
    fn actors(&self, roles: &[Role]) -> Vec<(Dev, Mem)> {
        self.s
            .active_devices(self.now)
            .into_iter()
            .filter(|(_, m)| {
                self.s
                    .members
                    .get(m)
                    .is_some_and(|member| roles.contains(&member.role))
            })
            .collect()
    }

    fn actor(&mut self, roles: &[Role]) -> Option<(Dev, Mem)> {
        let list = self.actors(roles);
        self.pick(&list)
    }

    /// A random device of a member that may sign now.
    fn device_of(&mut self, m: Mem) -> Option<Dev> {
        let list: Vec<Dev> = self
            .s
            .devices
            .values()
            .filter(|d| d.member == m && d.state.is_active_at(self.now))
            .map(|d| d.id)
            .collect();
        self.pick(&list)
    }

    /// A member that satisfies the predicate (now and then a member that does not exist: a subject the
    /// check must find unknown).
    fn member_where(&mut self, pred: impl Fn(&crate::model::MMember) -> bool) -> Option<Mem> {
        if self.pct(2) {
            return Some(UNKNOWN_MEM);
        }
        let list: Vec<Mem> = self
            .s
            .members
            .values()
            .filter(|m| pred(m))
            .map(|m| m.id)
            .collect();
        self.pick(&list)
    }

    fn any_group(&mut self) -> Option<Grp> {
        if self.pct(3) {
            return Some(UNKNOWN_GRP);
        }
        let list: Vec<Grp> = self.s.groups.keys().copied().collect();
        self.pick(&list)
    }

    fn spare_device(&mut self) -> Option<Dev> {
        let s = self.s;
        let list: Vec<Dev> = self
            .u
            .spare_devices
            .iter()
            .copied()
            .filter(|d| !s.devices.contains_key(d))
            .collect();
        self.pick(&list)
    }

    /// Adds what a valid entry needs besides its signer: the cosignatures of a quorum (distinct people who
    /// are Active owners or admins), the passkey assertions of an `escrow_open`, the member's own device
    /// for `private -> managed`. The model says what is needed.
    fn finish(&mut self, mut e: ModelEntry) -> ModelEntry {
        let s = self.s;
        let now = self.now;
        if let Some(need) = quorum_requirement(s, &e, now) {
            let mut have: BTreeSet<Mem> = BTreeSet::new();
            if let MSigner::Device(d) = e.signer {
                if let Some(dv) = s.devices.get(&d) {
                    have.insert(dv.member);
                }
            }
            let mut tries = 0;
            while have.len() < need && tries < 12 {
                tries += 1;
                let candidates: Vec<(Dev, Mem)> = s
                    .active_devices(now)
                    .into_iter()
                    .filter(|(_, m)| {
                        !have.contains(m)
                            && s.members.get(m).is_some_and(|mm| {
                                mm.kind == MKind::Person && mm.role.is_admin_class()
                            })
                    })
                    .collect();
                match self.pick(&candidates) {
                    Some((d, m)) => {
                        e.cosigs.push((d, CosigClaim::Valid));
                        have.insert(m);
                    }
                    None => break,
                }
            }
        }
        if matches!(e.op, MOp::EscrowOpen { .. }) {
            let mut devices: Vec<Dev> = e.cosigs.iter().map(|(d, _)| *d).collect();
            if let MSigner::Device(d) = e.signer {
                devices.push(d);
            }
            for dev in devices {
                e.assertions.push(MAssertion {
                    dev,
                    user_verified: true,
                    challenge_ok: true,
                    rp_ok: true,
                });
            }
        }
        if let MOp::MemberManaged {
            member,
            managed: true,
        } = e.op
        {
            // private -> managed needs a cosignature of the member's own device, also when the signer is
            // that device (reading R-consent)
            let private = s.members.get(&member).is_some_and(|m| !m.managed);
            if private {
                if let Some(d) = self.device_of(member) {
                    e.cosigs.push((d, CosigClaim::Valid));
                }
            }
        }
        e
    }

    /// A random patch of the tracked policy fields: `fields` of them, each set to a value drawn from a
    /// small table that holds tighter and looser values than the defaults.
    fn random_patch(&mut self, fields: usize, group_only: bool) -> MPatch {
        let mut patch = MPatch::default();
        for _ in 0..fields {
            let mut choices: Vec<PField> = PField::ALL.to_vec();
            if group_only {
                choices.retain(|f| {
                    !matches!(
                        f,
                        PField::ManagedDefault | PField::Updates | PField::Padding
                    )
                });
            }
            let Some(field) = self.pick(&choices) else {
                break;
            };
            match field {
                PField::CredentialTtl => {
                    let v = self.pick(&[1u32, 6, 12, 24]).unwrap_or(12);
                    patch.credential_ttl = Some(Span::hours(v));
                }
                PField::DeviceApproval => {
                    let v = if self.pct(50) {
                        DevApproval::SelfOnly
                    } else {
                        DevApproval::SelfOrQuorum
                    };
                    patch.device_approval = Some(v);
                }
                PField::MaxDevices => {
                    patch.max_devices = self.pick(&[3u8, 5, 10, 20]);
                }
                PField::ManagedDefault => patch.managed_default = Some(self.pct(50)),
                PField::SsoRequired => patch.sso_required = Some(self.pct(50)),
                PField::EnrollHold => {
                    let v = self.pick(&[1u32, 24, 48, 96]).unwrap_or(48);
                    patch.enroll_hold = Some(Span::hours(v));
                }
                PField::PasskeyRecovery => patch.passkey_recovery = Some(self.pct(50)),
                PField::EscrowOpenWait => {
                    let v = self.pick(&[1u32, 24, 72, 120]).unwrap_or(72);
                    patch.escrow_open_wait = Some(Span::hours(v));
                }
                PField::AuditLevel => patch.audit_level = self.pick(&[0u8, 1, 2, 3]),
                PField::AdminQuorum => patch.admin_quorum = self.pick(&[1u8, 2, 3]),
                PField::BrowserDevices => patch.browser_devices = Some(self.pct(50)),
                PField::ExternalSharing => {
                    let v = if self.pct(50) {
                        ExtSharing::Allowed
                    } else {
                        ExtSharing::SameWorkspaceOnly
                    };
                    patch.external_sharing = Some(v);
                }
                PField::Updates => patch.updates = self.pick(&[0u8, 1, 2]),
                PField::Padding => patch.padding = self.pick(&[0u8, 1, 2]),
            }
        }
        patch
    }
}

// ---------------------------------------------------------------------------------------------------
// The setup of a universe: a plan of valid entries that makes the planned world.
// ---------------------------------------------------------------------------------------------------

/// One thing the setup does.
#[derive(Clone, Debug)]
enum Action {
    Genesis,
    CreateGroup(Grp, Option<Grp>),
    Invite(Mem),
    Join(Mem),
    AddDevice(Mem, Dev),
    SetRole(Mem),
    PendingDevice(Mem, Dev),
    Delegate(usize),
    MakeManaged(Mem),
    EscrowSet,
    Entitlement,
    TightenAccount,
    TightenGroup(Grp),
    Suspend(Mem),
    Quorum(u8),
}

/// The setup of a universe: the genesis, the groups, the invitations and joins, extra devices, the roles,
/// devices enrolled by quorum, delegations, managed members, the escrow, an entitlement, a few policies, the
/// suspension and the quorum the universe plans.
///
/// Implements planning/architecture.md section 12 E1 (a universe of accounts, members and roles); Caller:
/// `compare.rs` (`differential`).
#[derive(Clone, Debug)]
pub struct Setup {
    actions: Vec<Action>,
    next: usize,
}

impl Setup {
    /// The plan of a universe.
    ///
    /// Implements planning/architecture.md section 12 E1; Caller: `compare.rs`.
    pub fn new(u: &Universe) -> Setup {
        let mut actions = vec![Action::Genesis];
        for g in &u.groups {
            actions.push(Action::CreateGroup(g.grp, g.parent));
        }
        for m in u.members.iter().skip(1) {
            actions.push(Action::Invite(m.mem));
        }
        for m in u.members.iter().skip(1) {
            actions.push(Action::Join(m.mem));
        }
        for m in &u.members {
            for d in m.devices.iter().skip(1) {
                actions.push(Action::AddDevice(m.mem, *d));
            }
        }
        for m in u.members.iter().skip(1) {
            if !matches!(m.role, Role::Member) {
                actions.push(Action::SetRole(m.mem));
            }
        }
        for m in &u.members {
            if let Some(d) = m.pending_device {
                actions.push(Action::PendingDevice(m.mem, d));
            }
        }
        for (i, _) in u.delegations.iter().enumerate() {
            actions.push(Action::Delegate(i));
        }
        for m in &u.members {
            if m.make_managed {
                actions.push(Action::MakeManaged(m.mem));
            }
        }
        actions.push(Action::EscrowSet);
        actions.push(Action::Entitlement);
        actions.push(Action::TightenAccount);
        if let Some(g) = u.groups.first() {
            actions.push(Action::TightenGroup(g.grp));
        }
        for m in &u.members {
            if m.suspended {
                actions.push(Action::Suspend(m.mem));
            }
        }
        actions.push(Action::Quorum(u.quorum_target));
        Setup { actions, next: 0 }
    }

    /// The next entry of the setup, built from the model's current state; `None` when the plan is done.
    /// An action that cannot be built now (nobody to sign it) is skipped.
    ///
    /// Implements planning/architecture.md section 12 E1; Caller: `compare.rs`.
    pub fn next_step(
        &mut self,
        rng: &mut Rng,
        u: &Universe,
        s: &ModelState,
        now: Stamp,
        eid: &mut u64,
    ) -> Option<Step> {
        while let Some(action) = self.actions.get(self.next).cloned() {
            self.next += 1;
            let mut g = G {
                rng: &mut *rng,
                u,
                s,
                now,
                eid: &mut *eid,
            };
            if let Some(entry) = build_action(&mut g, &action) {
                return Some(g.step(entry));
            }
        }
        None
    }

    /// Whether the plan has been run to its end.
    ///
    /// Implements planning/architecture.md section 12 E1; Caller: `compare.rs`.
    pub fn is_done(&self) -> bool {
        self.next >= self.actions.len()
    }
}

fn build_action(g: &mut G, action: &Action) -> Option<ModelEntry> {
    let s = g.s;
    let u = g.u;
    match action {
        Action::Genesis => {
            let first = u.members.first()?;
            let dev = *first.devices.first()?;
            let op = MOp::Genesis {
                owner: first.mem,
                device: dev,
            };
            Some(g.entry(dev, op))
        }
        Action::CreateGroup(group, parent) => {
            let (signer, _) = g.actor(&OWNER_ADMIN)?;
            let e = g.entry(
                signer,
                MOp::GroupCreate {
                    group: *group,
                    parent: *parent,
                },
            );
            Some(g.finish(e))
        }
        Action::Invite(m) => {
            let planned = u.planned(*m)?;
            let (signer, _) = g.actor(&OWNER_ADMIN)?;
            let secret = (g.new_eid() as u32).wrapping_mul(7919).wrapping_add(13);
            let groups: BTreeSet<Grp> = planned
                .groups
                .iter()
                .copied()
                .filter(|x| s.groups.contains_key(x))
                .collect();
            let role = if planned.role == Role::Guest {
                Role::Guest
            } else {
                Role::Member
            };
            let app = if planned.kind == MKind::App {
                Some(MApp {
                    scopes: planned.app_scopes.iter().copied().collect(),
                    expires: None,
                })
            } else {
                None
            };
            let e = g.entry(
                signer,
                MOp::MemberInvite {
                    member: *m,
                    kind: planned.kind,
                    role,
                    secret,
                    groups,
                    app,
                },
            );
            Some(g.finish(e))
        }
        Action::Join(m) => {
            let planned = u.planned(*m)?;
            let device = *planned.devices.first()?;
            let member = s.members.get(m)?;
            let invite = member.invite.as_ref()?;
            let confirmer = match invite.inviter {
                Some(inviter) => g.device_of(inviter)?,
                None => g.actor(&OWNER_ADMIN)?.0,
            };
            let mut e = g.entry(
                device,
                MOp::MemberJoin {
                    member: *m,
                    device,
                    secret: invite.secret,
                },
            );
            e.cosigs.push((confirmer, CosigClaim::Valid));
            Some(e)
        }
        Action::AddDevice(m, d) => {
            let signer = g.device_of(*m)?;
            let e = g.entry(
                signer,
                MOp::DeviceAdd {
                    member: *m,
                    device: *d,
                },
            );
            Some(g.finish(e))
        }
        Action::SetRole(m) => {
            let planned = u.planned(*m)?;
            let (signer, _) = g.actor(&OWNER_ADMIN)?;
            let e = g.entry(
                signer,
                MOp::MemberRole {
                    member: *m,
                    role: planned.role,
                    scope: planned.scope,
                },
            );
            Some(g.finish(e))
        }
        Action::PendingDevice(m, d) => {
            let candidates: Vec<(Dev, Mem)> = g
                .actors(&OWNER_ADMIN)
                .into_iter()
                .filter(|(_, mem)| mem != m)
                .collect();
            let (signer, _) = g.pick(&candidates)?;
            let e = g.entry(
                signer,
                MOp::DeviceAdd {
                    member: *m,
                    device: *d,
                },
            );
            Some(g.finish(e))
        }
        Action::Delegate(i) => {
            let planned = u.delegations.get(*i)?;
            let (signer, _) = g.actor(&OWNER_ADMIN)?;
            let expires = planned.lives.map(|span| plus(g.now, span));
            let e = g.entry(
                signer,
                MOp::Delegate {
                    key: planned.key,
                    ops: planned.ops.iter().copied().collect(),
                    expires,
                },
            );
            Some(g.finish(e))
        }
        Action::MakeManaged(m) => {
            let (signer, _) = g.actor(&OWNER_ADMIN)?;
            let e = g.entry(
                signer,
                MOp::MemberManaged {
                    member: *m,
                    managed: true,
                },
            );
            Some(g.finish(e))
        }
        Action::EscrowSet => {
            let (signer, _) = g.actor(&OWNER_ADMIN)?;
            let holders: BTreeSet<Mem> = s
                .members
                .values()
                .filter(|m| m.role.is_admin_class() && m.state == MState::Active)
                .map(|m| m.id)
                .take(3)
                .collect();
            let e = g.entry(signer, MOp::EscrowSet { key: 1, holders });
            Some(g.finish(e))
        }
        Action::Entitlement => {
            let id = g.new_eid();
            Some(ModelEntry::new(
                id,
                MSigner::Payments,
                MOp::Entitlement {
                    version: 1,
                    active: true,
                    quota: 1 << 30,
                    seats: 10,
                },
            ))
        }
        Action::TightenAccount => {
            let (signer, _) = g.actor(&OWNER_ADMIN)?;
            let patch = MPatch {
                audit_level: Some(2),
                browser_devices: Some(true),
                ..MPatch::default()
            };
            let e = g.entry(
                signer,
                MOp::PolicySet {
                    scope: PScope::Account,
                    patch,
                },
            );
            Some(g.finish(e))
        }
        Action::TightenGroup(group) => {
            let (signer, _) = g.actor(&OWNER_ADMIN)?;
            let patch = MPatch {
                credential_ttl: Some(Span::hours(1)),
                external_sharing: Some(ExtSharing::SameWorkspaceOnly),
                ..MPatch::default()
            };
            let e = g.entry(
                signer,
                MOp::PolicySet {
                    scope: PScope::Group(*group),
                    patch,
                },
            );
            Some(g.finish(e))
        }
        Action::Suspend(m) => {
            let (signer, _) = g.actor(&OWNER_ADMIN)?;
            let e = g.entry(signer, MOp::MemberSuspend { member: *m });
            Some(g.finish(e))
        }
        Action::Quorum(q) => {
            if s.policy.admin_quorum == *q {
                return None;
            }
            let (signer, _) = g.actor(&OWNER_ADMIN)?;
            let e = g.entry(signer, MOp::AdminQuorumSet { quorum: *q });
            Some(g.finish(e))
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// Candidates: plausible entries of each op, drawn from the model's state.
// ---------------------------------------------------------------------------------------------------

impl G<'_> {
    /// A live delegate key whose delegation names the op.
    fn live_delegate_for(&mut self, op: OpKind) -> Option<Key> {
        let now = self.now;
        let keys: Vec<Key> = self
            .s
            .delegations
            .iter()
            .filter(|(_, d)| d.ops.contains(&op) && d.expires.is_none_or(|t| now < t))
            .map(|(k, _)| *k)
            .collect();
        self.pick(&keys)
    }

    fn weighted_op(&mut self) -> Option<OpKind> {
        let total: u32 = VALID_WEIGHTS.iter().map(|(_, w)| *w).sum();
        let mut roll = between(self.rng, 0, total.saturating_sub(1) as usize) as u32;
        for (op, weight) in VALID_WEIGHTS {
            if roll < weight {
                return Some(op);
            }
            roll -= weight;
        }
        None
    }

    /// A plausible entry of the op: fields drawn from the state, the signer a role the table lets sign
    /// it most of the time, the cosignatures the model asks for added.
    fn cand(&mut self, op: OpKind) -> Option<ModelEntry> {
        let e = match op {
            OpKind::Genesis => return None,
            OpKind::DeviceAdd => self.c_device_add()?,
            OpKind::DeviceRemove => self.c_device_remove()?,
            OpKind::MemberInvite => self.c_member_invite()?,
            OpKind::MemberJoin => self.c_member_join()?,
            OpKind::MemberRole => self.c_member_role()?,
            OpKind::MemberSuspend => self.c_member_suspend()?,
            OpKind::MemberRemove => self.c_member_remove()?,
            OpKind::MemberManaged => self.c_member_managed()?,
            OpKind::GroupCreate => self.c_group_create()?,
            OpKind::GroupMember => self.c_group_member()?,
            OpKind::GroupMove => self.c_group_move()?,
            OpKind::PolicySet => self.c_policy_set()?,
            OpKind::EscrowSet => self.c_escrow_set()?,
            OpKind::AuditKeySet => self.c_audit_key_set()?,
            OpKind::AdminQuorumSet => self.c_admin_quorum_set()?,
            OpKind::EscrowOpen => self.c_escrow_open()?,
            OpKind::Delegate => self.c_delegate()?,
            OpKind::Entitlement => self.c_entitlement()?,
            OpKind::App => self.c_app()?,
        };
        Some(self.finish(e))
    }

    fn c_device_add(&mut self) -> Option<ModelEntry> {
        let member = self.member_where(|m| m.kind == MKind::Person && m.state == MState::Active)?;
        let device = self.spare_device()?;
        let signer = if self.pct(45) {
            self.device_of(member)?
        } else {
            self.actor(&OWNER_ADMIN)?.0
        };
        Some(self.entry(signer, MOp::DeviceAdd { member, device }))
    }

    fn c_device_remove(&mut self) -> Option<ModelEntry> {
        let s = self.s;
        let list: Vec<(Dev, Mem)> = s
            .devices
            .values()
            .filter(|d| d.state != DState::Revoked)
            .map(|d| (d.id, d.member))
            .collect();
        let (mut device, owner_of) = self.pick(&list)?;
        let signer = if self.pct(50) {
            self.device_of(owner_of)?
        } else {
            self.actor(&OWNER_ADMIN)?.0
        };
        if self.pct(2) {
            // a device that does not exist
            device = UNKNOWN_DEV;
        }
        Some(self.entry(signer, MOp::DeviceRemove { device }))
    }

    fn c_member_invite(&mut self) -> Option<ModelEntry> {
        let s = self.s;
        let spare: Vec<Mem> = self
            .u
            .spare_members
            .iter()
            .copied()
            .filter(|m| !s.members.contains_key(m))
            .collect();
        let member = self.pick(&spare)?;
        let kind = if self.pct(20) {
            MKind::App
        } else {
            MKind::Person
        };
        let role = if self.pct(25) {
            Role::Guest
        } else {
            Role::Member
        };
        let delegate = if self.pct(25) {
            self.live_delegate_for(OpKind::MemberInvite)
        } else {
            None
        };
        let (signer, scope) = match delegate {
            Some(key) => (MSigner::Delegate(key), None),
            None => {
                let (dev, mem) = self.actor(&ADMINISH)?;
                let scope = s
                    .members
                    .get(&mem)
                    .filter(|m| m.role == Role::GroupAdmin)
                    .and_then(|m| m.scope);
                (MSigner::Device(dev), scope)
            }
        };
        let pool: Vec<Grp> = match scope {
            Some(root) => s.subtree(root).into_iter().collect(),
            None => s.groups.keys().copied().collect(),
        };
        let wanted = if scope.is_some() {
            between(self.rng, 1, 2)
        } else {
            between(self.rng, 0, 2)
        };
        let mut groups: BTreeSet<Grp> = BTreeSet::new();
        for _ in 0..wanted {
            if let Some(g) = self.pick(&pool) {
                groups.insert(g);
            }
        }
        let app = if kind == MKind::App {
            let expires = if self.pct(30) {
                let hours = between(self.rng, 1, 72) as u32;
                Some(plus(self.now, Span::hours(hours)))
            } else {
                None
            };
            let mut scopes = BTreeSet::new();
            let apps = self.u.apps.clone();
            for _ in 0..between(self.rng, 1, 2) {
                if let Some(a) = self.pick(&apps) {
                    scopes.insert(a);
                }
            }
            Some(MApp { scopes, expires })
        } else {
            None
        };
        let secret = (self.new_eid() as u32).wrapping_mul(7919).wrapping_add(13);
        let id = self.new_eid();
        Some(ModelEntry::new(
            id,
            signer,
            MOp::MemberInvite {
                member,
                kind,
                role,
                secret,
                groups,
                app,
            },
        ))
    }

    fn c_member_join(&mut self) -> Option<ModelEntry> {
        let s = self.s;
        let invited: Vec<Mem> = s
            .members
            .values()
            .filter(|m| m.state == MState::Invited && m.invite.is_some())
            .map(|m| m.id)
            .collect();
        let member = self.pick(&invited)?;
        let invite = s.members.get(&member)?.invite.clone()?;
        let device = self.spare_device()?;
        let confirmer = match invite.inviter {
            Some(inviter) => self.device_of(inviter),
            None => self.actor(&OWNER_ADMIN).map(|(d, _)| d),
        };
        let mut e = self.entry(
            device,
            MOp::MemberJoin {
                member,
                device,
                secret: invite.secret,
            },
        );
        if let Some(c) = confirmer {
            e.cosigs.push((c, CosigClaim::Valid));
        }
        Some(e)
    }

    fn c_member_role(&mut self) -> Option<ModelEntry> {
        let member = self.member_where(|m| m.kind == MKind::Person && m.state == MState::Active)?;
        let roles = [
            Role::Member,
            Role::Member,
            Role::Guest,
            Role::Helpdesk,
            Role::Helpdesk,
            Role::GroupAdmin,
            Role::GroupAdmin,
            Role::Admin,
            Role::Admin,
            Role::Owner,
        ];
        let role = self.pick(&roles)?;
        let scope = if role.needs_scope() {
            self.any_group()
        } else {
            None
        };
        let signer = self.actor(&ADMINISH)?.0;
        Some(self.entry(
            signer,
            MOp::MemberRole {
                member,
                role,
                scope,
            },
        ))
    }

    fn c_member_suspend(&mut self) -> Option<ModelEntry> {
        let member = self.member_where(|m| m.kind == MKind::Person && m.state == MState::Active)?;
        let delegate = if self.pct(30) {
            self.live_delegate_for(OpKind::MemberSuspend)
        } else {
            None
        };
        let op = MOp::MemberSuspend { member };
        match delegate {
            Some(key) => {
                let id = self.new_eid();
                Some(ModelEntry::new(id, MSigner::Delegate(key), op))
            }
            None => {
                let signer = self
                    .actor(&[Role::Owner, Role::Admin, Role::GroupAdmin, Role::Helpdesk])?
                    .0;
                Some(self.entry(signer, op))
            }
        }
    }

    fn c_member_remove(&mut self) -> Option<ModelEntry> {
        let member =
            self.member_where(|m| m.kind == MKind::Person && m.state != MState::Removed)?;
        let signer = self.actor(&ADMINISH)?.0;
        Some(self.entry(signer, MOp::MemberRemove { member }))
    }

    fn c_member_managed(&mut self) -> Option<ModelEntry> {
        let member = self.member_where(|m| m.kind == MKind::Person && m.state == MState::Active)?;
        let managed = self.pct(55);
        let signer = self.actor(&OWNER_ADMIN)?.0;
        Some(self.entry(signer, MOp::MemberManaged { member, managed }))
    }

    fn c_group_create(&mut self) -> Option<ModelEntry> {
        let s = self.s;
        let spare: Vec<Grp> = self
            .u
            .spare_groups
            .iter()
            .copied()
            .filter(|g| !s.groups.contains_key(g))
            .collect();
        let group = self.pick(&spare)?;
        let parent = if self.pct(80) { self.any_group() } else { None };
        let signer = self.actor(&ADMINISH)?.0;
        Some(self.entry(signer, MOp::GroupCreate { group, parent }))
    }

    fn c_group_member(&mut self) -> Option<ModelEntry> {
        let group = self.any_group()?;
        let member = self.member_where(|m| m.state != MState::Removed)?;
        let present = self.pct(70);
        let signer = self.actor(&ADMINISH)?.0;
        Some(self.entry(
            signer,
            MOp::GroupMember {
                group,
                member,
                present,
            },
        ))
    }

    fn c_group_move(&mut self) -> Option<ModelEntry> {
        let s = self.s;
        let movable: Vec<Grp> = s
            .groups
            .iter()
            .filter(|(_, p)| p.is_some())
            .map(|(g, _)| *g)
            .collect();
        let group = self.pick(&movable)?;
        let new_parent = self.any_group()?;
        let signer = self.actor(&ADMINISH)?.0;
        Some(self.entry(signer, MOp::GroupMove { group, new_parent }))
    }

    /// A patch that is at least as tight as `inherited` in every field it sets.
    fn tight_patch(&mut self, inherited: &crate::model::MPolicy) -> MPatch {
        let mut patch = MPatch::default();
        for _ in 0..between(self.rng, 1, 2) {
            let step = self.pick(&[0u64, 1800, 3600, 86_400]).unwrap_or(0);
            match self
                .pick(&[0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10])
                .unwrap_or(0)
            {
                0 => {
                    let secs = inherited
                        .credential_ttl
                        .as_secs()
                        .saturating_sub(step)
                        .max(60);
                    patch.credential_ttl = Some(Span::secs(secs));
                }
                1 => {
                    let secs = inherited.enroll_hold.as_secs().saturating_add(step);
                    patch.enroll_hold = Some(Span::secs(secs));
                }
                2 => {
                    let secs = inherited.escrow_open_wait.as_secs().saturating_add(step);
                    patch.escrow_open_wait = Some(Span::secs(secs));
                }
                3 => patch.max_devices = Some(inherited.max_devices.saturating_sub(1).max(1)),
                4 => patch.audit_level = Some(inherited.audit_level.saturating_add(1).min(3)),
                5 => patch.admin_quorum = Some(inherited.admin_quorum.saturating_add(1).min(8)),
                6 => patch.sso_required = Some(true),
                7 => patch.passkey_recovery = Some(false),
                8 => patch.browser_devices = Some(false),
                9 => patch.external_sharing = Some(ExtSharing::SameWorkspaceOnly),
                _ => patch.device_approval = Some(DevApproval::SelfOnly),
            }
        }
        patch
    }

    fn c_policy_set(&mut self) -> Option<ModelEntry> {
        let s = self.s;
        let signer = self.actor(&ADMINISH)?.0;
        let (scope, patch) = if self.pct(35) {
            let fields = between(self.rng, 1, 2);
            (PScope::Account, self.random_patch(fields, false))
        } else {
            let group = self.any_group()?;
            let patch = match s.inherited_policy(group) {
                Some(inherited) if self.pct(65) => self.tight_patch(&inherited),
                _ => {
                    let fields = between(self.rng, 1, 2);
                    self.random_patch(fields, true)
                }
            };
            (PScope::Group(group), patch)
        };
        Some(self.entry(signer, MOp::PolicySet { scope, patch }))
    }

    fn c_escrow_set(&mut self) -> Option<ModelEntry> {
        let signer = self.actor(&OWNER_ADMIN)?.0;
        let mut holders = BTreeSet::new();
        for _ in 0..between(self.rng, 0, 3) {
            if let Some(m) = self.member_where(|m| m.state == MState::Active) {
                holders.insert(m);
            }
        }
        let key = between(self.rng, 1, 9) as u16;
        Some(self.entry(signer, MOp::EscrowSet { key, holders }))
    }

    fn c_audit_key_set(&mut self) -> Option<ModelEntry> {
        let signer = self.actor(&OWNER_ADMIN)?.0;
        let key = between(self.rng, 1, 9) as u16;
        Some(self.entry(signer, MOp::AuditKeySet { key }))
    }

    fn c_admin_quorum_set(&mut self) -> Option<ModelEntry> {
        let signer = self.actor(&OWNER_ADMIN)?.0;
        let quorum = self.pick(&[1u8, 1, 2, 2, 3])?;
        Some(self.entry(signer, MOp::AdminQuorumSet { quorum }))
    }

    fn c_escrow_open(&mut self) -> Option<ModelEntry> {
        let member = self.member_where(|m| {
            m.kind == MKind::Person
                && m.managed
                && matches!(
                    m.state,
                    MState::Active | MState::Suspended | MState::Removed
                )
        })?;
        let reason = self.pick(&[Reason::LeaverHandover, Reason::LegalRequest, Reason::Other])?;
        let signer = self.actor(&OWNER_ADMIN)?.0;
        Some(self.entry(signer, MOp::EscrowOpen { member, reason }))
    }

    fn c_delegate(&mut self) -> Option<ModelEntry> {
        let keys = self.u.delegate_keys.clone();
        let key = self.pick(&keys)?;
        let mut ops: BTreeSet<OpKind> = BTreeSet::new();
        let roll = between(self.rng, 0, 9);
        if roll < 7 {
            ops.insert(OpKind::MemberSuspend);
            if self.pct(60) {
                ops.insert(OpKind::MemberInvite);
            }
        } else if roll >= 9 {
            let others = [
                OpKind::MemberRole,
                OpKind::DeviceAdd,
                OpKind::Delegate,
                OpKind::GroupMember,
            ];
            ops.insert(self.pick(&others)?);
        }
        let expires = if self.pct(50) {
            None
        } else {
            let minutes = between(self.rng, 5, 600) as u32;
            Some(plus(self.now, Span::minutes(minutes)))
        };
        let signer = self.actor(&OWNER_ADMIN)?.0;
        Some(self.entry(signer, MOp::Delegate { key, ops, expires }))
    }

    fn c_entitlement(&mut self) -> Option<ModelEntry> {
        let held = self.s.entitlement.as_ref().map(|e| e.version);
        let version = match held {
            Some(v) if self.pct(5) => v,
            Some(v) => v.saturating_add(1),
            None => 1,
        };
        let active = self.pct(80);
        let quota = (between(self.rng, 1, 64) as u64) << 30;
        let seats = between(self.rng, 1, 500) as u32;
        let id = self.new_eid();
        Some(ModelEntry::new(
            id,
            MSigner::Payments,
            MOp::Entitlement {
                version,
                active,
                quota,
                seats,
            },
        ))
    }

    fn c_app(&mut self) -> Option<ModelEntry> {
        let s = self.s;
        let want_app = self.pct(20);
        let signers: Vec<(Dev, Mem)> = s
            .active_devices(self.now)
            .into_iter()
            .filter(|(_, m)| {
                s.members.get(m).is_some_and(|mm| {
                    if want_app {
                        mm.kind == MKind::App
                    } else {
                        mm.kind == MKind::Person
                    }
                })
            })
            .collect();
        let (signer, _) = self.pick(&signers)?;
        let apps = self.u.apps.clone();
        let name: AppId = self.pick(&apps)?;
        let body_len = between(self.rng, 0, 300);
        Some(self.entry(signer, MOp::App { name, body_len }))
    }

    /// An entry the model admits: sampled from the candidates of the weighted ops.
    fn valid(&mut self) -> Option<ModelEntry> {
        for _ in 0..16 {
            let op = self.weighted_op()?;
            if let Some(e) = self.cand(op) {
                if model_violations(self.s, &e, self.now).is_empty() {
                    return Some(e);
                }
            }
        }
        None
    }

    /// A valid entry of one of the ops.
    fn base(&mut self, ops: &[OpKind]) -> Option<ModelEntry> {
        for _ in 0..14 {
            let op = self.pick(ops)?;
            if let Some(e) = self.cand(op) {
                if model_violations(self.s, &e, self.now).is_empty() {
                    return Some(e);
                }
            }
        }
        None
    }

    /// Touches an entry in a way that may or may not change its verdict.
    fn perturb(&mut self, e: &mut ModelEntry) {
        match between(self.rng, 0, 9) {
            0 => e.cosigs.clear(),
            1 => {
                let list = self.s.active_devices(self.now);
                if let Some((d, _)) = self.pick(&list) {
                    e.signer = MSigner::Device(d);
                }
            }
            2 => {
                let list: Vec<Dev> = self.s.devices.keys().copied().collect();
                if let Some(d) = self.pick(&list) {
                    e.signer = MSigner::Device(d);
                }
            }
            3 => e.sig = SigClaim::Invalid,
            4 => {
                if let Some(first) = e.cosigs.first_mut() {
                    first.1 = CosigClaim::OverOtherEntry;
                }
            }
            5 => e.assertions.clear(),
            _ => {}
        }
    }

    /// Any entry: a candidate of a random op, perturbed more often than not.
    fn random(&mut self) -> Option<ModelEntry> {
        let ops: Vec<OpKind> = OpKind::ALL
            .iter()
            .copied()
            .filter(|k| *k != OpKind::Genesis)
            .collect();
        for _ in 0..8 {
            let op = self.pick(&ops)?;
            if let Some(mut e) = self.cand(op) {
                if self.pct(70) {
                    self.perturb(&mut e);
                }
                return Some(e);
            }
        }
        None
    }

    /// Finishes a valid entry: a claimed time that is never read, now and then a restore.
    fn dress(&mut self, e: &mut ModelEntry) {
        if self.pct(50) {
            e.at = Stamp::from_secs(self.rng.next_u64() % 10_000_000);
        }
        if self.pct(2) {
            e.epoch = EpochSpec::Bump;
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// Near misses: a valid entry changed in one place.
// ---------------------------------------------------------------------------------------------------

/// What `gen_entry` made: the step, and the near miss it is one of, if any.
///
/// Implements planning/architecture.md section 12 E1; Caller: `compare.rs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generated {
    /// The entry and its stamp.
    pub step: Step,
    /// The kind of near miss the entry is, `None` for a valid, random or preparatory entry.
    pub near_miss: Option<NearMiss>,
}

impl G<'_> {
    fn role_of_signer(&self, e: &ModelEntry) -> Option<Role> {
        match e.signer {
            MSigner::Device(d) => {
                let dv = self.s.devices.get(&d)?;
                self.s.members.get(&dv.member).map(|m| m.role)
            }
            _ => None,
        }
    }

    fn member_of_signer(&self, e: &ModelEntry) -> Option<Mem> {
        match e.signer {
            MSigner::Device(d) => self.s.devices.get(&d).map(|dv| dv.member),
            _ => None,
        }
    }

    /// Whether the model refuses the entry with the near miss's class and nothing but its companions.
    fn nm_ok(&self, step: &Step, nm: NearMiss) -> bool {
        let set = model_violations(self.s, &step.entry, step.now);
        let want = nm.expected();
        set.contains(&want)
            && set
                .iter()
                .all(|c| *c == want || nm.companions().contains(c))
    }

    fn app_entry(&mut self, signer: Dev) -> ModelEntry {
        let apps = self.u.apps.clone();
        let name = self.pick(&apps).unwrap_or(AppId(1));
        self.entry(signer, MOp::App { name, body_len: 10 })
    }

    /// A valid entry that makes the state the near miss needs, when the state lacks it.
    fn prepare(&mut self, nm: NearMiss) -> Option<ModelEntry> {
        let s = self.s;
        match nm {
            NearMiss::JoinWithWrongSecret => {
                let spare: Vec<Mem> = self
                    .u
                    .spare_members
                    .iter()
                    .copied()
                    .filter(|m| !s.members.contains_key(m))
                    .collect();
                let member = self.pick(&spare)?;
                let signer = self.actor(&OWNER_ADMIN)?.0;
                let secret = (self.new_eid() as u32).wrapping_mul(31).wrapping_add(5);
                let e = self.entry(
                    signer,
                    MOp::MemberInvite {
                        member,
                        kind: MKind::Person,
                        role: Role::Member,
                        secret,
                        groups: BTreeSet::new(),
                        app: None,
                    },
                );
                Some(self.finish(e))
            }
            NearMiss::DelegationExpiredByOneSecond => {
                let has_expiring = s
                    .delegations
                    .values()
                    .any(|d| match (d.expires, s.last_at) {
                        (Some(t), Some(last)) => t.as_secs().saturating_add(1) >= last.as_secs(),
                        (Some(_), None) => true,
                        (None, _) => false,
                    });
                if has_expiring {
                    return None;
                }
                let signer = self.actor(&OWNER_ADMIN)?.0;
                let minutes = between(self.rng, 3, 30) as u32;
                let mut ops = BTreeSet::new();
                ops.insert(OpKind::MemberSuspend);
                ops.insert(OpKind::MemberInvite);
                let e = self.entry(
                    signer,
                    MOp::Delegate {
                        key: Key(3),
                        ops,
                        expires: Some(plus(self.now, Span::minutes(minutes))),
                    },
                );
                Some(self.finish(e))
            }
            NearMiss::SuspendedMemberSigns | NearMiss::CosigByASuspendedAdmin => {
                let victims: Vec<Mem> = s
                    .members
                    .values()
                    .filter(|m| {
                        m.kind == MKind::Person
                            && m.state == MState::Active
                            && m.role != Role::Owner
                            && (nm == NearMiss::SuspendedMemberSigns
                                || m.role == Role::Admin
                                || m.role == Role::GroupAdmin)
                    })
                    .map(|m| m.id)
                    .collect();
                let member = self.pick(&victims)?;
                let signer = self.actor(&OWNER_ADMIN)?.0;
                let e = self.entry(signer, MOp::MemberSuspend { member });
                Some(self.finish(e))
            }
            NearMiss::PendingDeviceSigns => {
                let subjects: Vec<Mem> = s
                    .members
                    .values()
                    .filter(|m| m.kind == MKind::Person && m.state == MState::Active)
                    .map(|m| m.id)
                    .collect();
                let member = self.pick(&subjects)?;
                let device = self.spare_device()?;
                let candidates: Vec<(Dev, Mem)> = self
                    .actors(&OWNER_ADMIN)
                    .into_iter()
                    .filter(|(_, m)| *m != member)
                    .collect();
                let (signer, _) = self.pick(&candidates)?;
                let e = self.entry(signer, MOp::DeviceAdd { member, device });
                Some(self.finish(e))
            }
            NearMiss::EpochBelowChain => {
                let mut e = self.valid()?;
                e.epoch = EpochSpec::Bump;
                Some(e)
            }
            _ => None,
        }
    }

    fn near_miss(&mut self, nm: NearMiss) -> Option<Generated> {
        for _ in 0..24 {
            if let Some(step) = self.try_nm(nm) {
                if self.nm_ok(&step, nm) {
                    return Some(Generated {
                        step,
                        near_miss: Some(nm),
                    });
                }
            }
        }
        let prepared = self.prepare(nm)?;
        if model_violations(self.s, &prepared, self.now).is_empty() {
            Some(Generated {
                step: self.step(prepared),
                near_miss: None,
            })
        } else {
            None
        }
    }

    fn try_nm(&mut self, nm: NearMiss) -> Option<Step> {
        let s = self.s;
        let now = self.now;
        match nm {
            NearMiss::SignerOneRoleTooLow => {
                let ops = [
                    OpKind::MemberRole,
                    OpKind::MemberInvite,
                    OpKind::GroupCreate,
                    OpKind::GroupMove,
                    OpKind::PolicySet,
                    OpKind::MemberRemove,
                    OpKind::MemberManaged,
                    OpKind::EscrowSet,
                    OpKind::AuditKeySet,
                    OpKind::AdminQuorumSet,
                    OpKind::Delegate,
                    OpKind::DeviceAdd,
                    OpKind::MemberSuspend,
                ];
                let mut e = self.base(&ops)?;
                let lower = match self.role_of_signer(&e)? {
                    Role::Owner => Role::Admin,
                    Role::Admin => Role::GroupAdmin,
                    Role::GroupAdmin => Role::Helpdesk,
                    Role::Helpdesk => Role::Member,
                    Role::Member => Role::Guest,
                    Role::Guest => return None,
                };
                let (dev, _) = self.actor(&[lower])?;
                e.signer = MSigner::Device(dev);
                Some(self.step(e))
            }
            NearMiss::SignerScopeOffByOneGroup => {
                let (dev, mem) = self.actor(&[Role::GroupAdmin, Role::Helpdesk])?;
                let member = s.members.get(&mem)?;
                let scope = member.scope?;
                let role = member.role;
                let inside = s.subtree(scope);
                let outside: Vec<Grp> = s
                    .groups
                    .keys()
                    .copied()
                    .filter(|g| !inside.contains(g))
                    .collect();
                let parent = match s.groups.get(&scope) {
                    Some(Some(p)) => Some(*p),
                    _ => None,
                };
                let target = match parent {
                    Some(p) if self.pct(60) => p,
                    _ => self.pick(&outside)?,
                };
                let op = if role == Role::GroupAdmin {
                    match between(self.rng, 0, 3) {
                        0 => {
                            let m = self.member_where(|m| m.state != MState::Removed)?;
                            MOp::GroupMember {
                                group: target,
                                member: m,
                                present: true,
                            }
                        }
                        1 => {
                            let spare: Vec<Grp> = self
                                .u
                                .spare_groups
                                .iter()
                                .copied()
                                .filter(|g| !s.groups.contains_key(g))
                                .collect();
                            MOp::GroupCreate {
                                group: self.pick(&spare)?,
                                parent: Some(target),
                            }
                        }
                        2 => {
                            let inherited = s.inherited_policy(target)?;
                            MOp::PolicySet {
                                scope: PScope::Group(target),
                                patch: self.tight_patch(&inherited),
                            }
                        }
                        _ => {
                            let inner: Vec<Grp> = inside
                                .iter()
                                .copied()
                                .filter(|g| s.groups.get(g).is_some_and(|p| p.is_some()))
                                .collect();
                            MOp::GroupMove {
                                group: self.pick(&inner)?,
                                new_parent: target,
                            }
                        }
                    }
                } else {
                    let subject = self.member_where(|m| {
                        m.state == MState::Active
                            && m.kind == MKind::Person
                            && m.role.rank() <= Role::Helpdesk.rank()
                            && !s.member_in_scope(scope, m.id)
                    })?;
                    MOp::MemberSuspend { member: subject }
                };
                let e = self.entry(dev, op);
                let e = self.finish(e);
                Some(self.step(e))
            }
            NearMiss::QuorumShortByOnePerson => {
                let mut e = self.quorum_base()?;
                let i = between(self.rng, 0, e.cosigs.len().saturating_sub(1));
                let (removed, _) = *e.cosigs.get(i)?;
                e.cosigs.remove(i);
                e.assertions.retain(|a| a.dev != removed);
                Some(self.step(e))
            }
            NearMiss::QuorumMetByTwoDevicesOfOnePerson => {
                let mut e = self.quorum_base()?;
                let i = between(self.rng, 0, e.cosigs.len().saturating_sub(1));
                let signer_dev = match e.signer {
                    MSigner::Device(d) => d,
                    _ => return None,
                };
                let signer_member = self.member_of_signer(&e)?;
                let others: Vec<Dev> = s
                    .devices
                    .values()
                    .filter(|d| {
                        d.member == signer_member && d.id != signer_dev && d.state.is_active_at(now)
                    })
                    .map(|d| d.id)
                    .collect();
                let replacement = match self.pick(&others) {
                    Some(d) if self.pct(70) => d,
                    _ => signer_dev,
                };
                let old = e.cosigs.get(i)?.0;
                e.assertions.retain(|a| a.dev != old);
                *e.cosigs.get_mut(i)? = (replacement, CosigClaim::Valid);
                if matches!(e.op, MOp::EscrowOpen { .. }) {
                    e.assertions.push(MAssertion {
                        dev: replacement,
                        user_verified: true,
                        challenge_ok: true,
                        rp_ok: true,
                    });
                }
                Some(self.step(e))
            }
            NearMiss::CosigByASuspendedAdmin => {
                let mut e = self.quorum_base()?;
                let i = between(self.rng, 0, e.cosigs.len().saturating_sub(1));
                let bad: Vec<Dev> = s
                    .devices
                    .values()
                    .filter(|d| {
                        let Some(m) = s.members.get(&d.member) else {
                            return false;
                        };
                        m.kind == MKind::Person
                            && m.role.is_admin_class()
                            && (m.state == MState::Suspended
                                || m.state == MState::Removed
                                || d.state == DState::Revoked
                                || !d.state.is_active_at(now))
                    })
                    .map(|d| d.id)
                    .collect();
                let replacement = self.pick(&bad)?;
                let old = e.cosigs.get(i)?.0;
                e.assertions.retain(|a| a.dev != old);
                *e.cosigs.get_mut(i)? = (replacement, CosigClaim::Valid);
                Some(self.step(e))
            }
            NearMiss::CosigOverAnotherEntryHash => {
                let mut e = self.quorum_base()?;
                let i = between(self.rng, 0, e.cosigs.len().saturating_sub(1));
                let claim = if self.pct(80) {
                    CosigClaim::OverOtherEntry
                } else {
                    CosigClaim::Garbage
                };
                e.cosigs.get_mut(i)?.1 = claim;
                Some(self.step(e))
            }
            NearMiss::WrongPrevOrSeq => {
                let mut e = self.valid()?;
                match between(self.rng, 0, 7) {
                    0 | 1 => e.prev = PrevSpec::Garbage((1 << 40) + (e.eid & 0xffff)),
                    2 => e.prev = PrevSpec::Stale(1),
                    3 => e.prev = PrevSpec::Zero,
                    4 => e.seq = SeqSpec::Off(1),
                    5 => e.seq = SeqSpec::Off(-1),
                    6 => e.seq = SeqSpec::Off(5),
                    _ => {
                        e.seq = SeqSpec::Off(-3);
                        e.prev = PrevSpec::Garbage((1 << 41) + 7);
                    }
                }
                Some(self.step(e))
            }
            NearMiss::DelegationExpiredByOneSecond => {
                let list: Vec<(Key, Stamp, BTreeSet<OpKind>)> = s
                    .delegations
                    .iter()
                    .filter_map(|(k, d)| d.expires.map(|t| (*k, t, d.ops.clone())))
                    .collect();
                let (key, expires, ops) = self.pick_clone(&list)?;
                let late = if self.pct(25) { 0 } else { 1 };
                let when = Stamp::from_secs(expires.as_secs().saturating_add(late));
                if let Some(last) = s.last_at {
                    if when < last {
                        return None;
                    }
                }
                let ops: Vec<OpKind> = ops.into_iter().collect();
                let kind = self.pick(&ops)?;
                let op = match kind {
                    OpKind::MemberSuspend => {
                        let m = self.member_where(|m| {
                            m.kind == MKind::Person
                                && m.state == MState::Active
                                && !m.role.is_admin_class()
                        })?;
                        MOp::MemberSuspend { member: m }
                    }
                    OpKind::MemberInvite => {
                        let spare: Vec<Mem> = self
                            .u
                            .spare_members
                            .iter()
                            .copied()
                            .filter(|m| !s.members.contains_key(m))
                            .collect();
                        MOp::MemberInvite {
                            member: self.pick(&spare)?,
                            kind: MKind::Person,
                            role: Role::Member,
                            secret: 99,
                            groups: BTreeSet::new(),
                            app: None,
                        }
                    }
                    _ => return None,
                };
                let id = self.new_eid();
                let e = ModelEntry::new(id, MSigner::Delegate(key), op);
                Some(Step {
                    entry: e,
                    now: when,
                })
            }
            NearMiss::DelegateOpOutsideTheNamedSet => {
                let live: Vec<(Key, BTreeSet<OpKind>)> = s
                    .delegations
                    .iter()
                    .filter(|(_, d)| d.expires.is_none_or(|t| now < t))
                    .map(|(k, d)| (*k, d.ops.clone()))
                    .collect();
                let (key, ops) = self.pick_clone(&live)?;
                let kinds = [
                    OpKind::MemberSuspend,
                    OpKind::MemberInvite,
                    OpKind::GroupCreate,
                    OpKind::MemberRole,
                    OpKind::DeviceRemove,
                    OpKind::GroupMember,
                    OpKind::App,
                ];
                let outside: Vec<OpKind> =
                    kinds.iter().copied().filter(|k| !ops.contains(k)).collect();
                let kind = self.pick(&outside)?;
                let mut e = self.cand(kind)?;
                e.signer = MSigner::Delegate(key);
                Some(self.step(e))
            }
            NearMiss::DelegateNamesARoleOrWrapOp => {
                let mut e = self.cand(OpKind::Delegate)?;
                let forbidden = [
                    OpKind::MemberRole,
                    OpKind::DeviceAdd,
                    OpKind::GroupMember,
                    OpKind::Delegate,
                    OpKind::MemberRemove,
                    OpKind::PolicySet,
                    OpKind::EscrowOpen,
                    OpKind::EscrowSet,
                ];
                let bad = self.pick(&forbidden)?;
                if let MOp::Delegate { ops, .. } = &mut e.op {
                    if self.rng.chance(1, 2) {
                        ops.insert(OpKind::MemberSuspend);
                    }
                    ops.insert(bad);
                }
                Some(self.step(e))
            }
            NearMiss::GroupMoveIntoOwnSubtree => {
                let mut e = self.base(&[OpKind::GroupMove])?;
                if let MOp::GroupMove { group, new_parent } = &mut e.op {
                    let inside: Vec<Grp> = s.subtree(*group).into_iter().collect();
                    *new_parent = self.pick(&inside)?;
                }
                Some(self.step(e))
            }
            NearMiss::LastOwnerDemotedRemovedOrSuspended => {
                if s.active_owner_count(None) != 1 {
                    return None;
                }
                let (dev, mem) = self.actor(&[Role::Owner])?;
                // the owner removes himself most of the time: that is the one self-action where the
                // plan's "at least one Active owner remains" is the only rule broken (a demotion or a
                // suspension of oneself also breaks "never one's own role" and "another owner")
                let op = match between(self.rng, 0, 9) {
                    0..=6 => MOp::MemberRemove { member: mem },
                    7 | 8 => MOp::MemberRole {
                        member: mem,
                        role: Role::Admin,
                        scope: None,
                    },
                    _ => MOp::MemberSuspend { member: mem },
                };
                let e = self.entry(dev, op);
                let e = self.finish(e);
                Some(self.step(e))
            }
            NearMiss::SelfRoleChange => {
                let (dev, mem) = self.actor(&ADMINISH)?;
                let current = s.members.get(&mem)?.role;
                let candidates: Vec<Role> = Role::ALL
                    .iter()
                    .copied()
                    .filter(|r| *r != current && r.rank() >= current.rank())
                    .collect();
                let role = self.pick(&candidates)?;
                let scope = if role.needs_scope() {
                    self.any_group()
                } else {
                    None
                };
                let e = self.entry(
                    dev,
                    MOp::MemberRole {
                        member: mem,
                        role,
                        scope,
                    },
                );
                let e = self.finish(e);
                Some(self.step(e))
            }
            NearMiss::HelpdeskAddsADeviceAlone => {
                let (dev, mem) = self.actor(&[Role::Helpdesk])?;
                let scope = s.members.get(&mem)?.scope;
                let subjects: Vec<Mem> = s
                    .members
                    .values()
                    .filter(|m| {
                        m.id != mem
                            && m.state == MState::Active
                            && m.kind == MKind::Person
                            && scope.is_some_and(|g| s.member_in_scope(g, m.id))
                    })
                    .map(|m| m.id)
                    .collect();
                let member = self.pick(&subjects)?;
                let device = self.spare_device()?;
                let e = self.entry(dev, MOp::DeviceAdd { member, device });
                let e = if self.pct(60) { self.finish(e) } else { e };
                Some(self.step(e))
            }
            NearMiss::EntitlementSignedByADevice => {
                let (dev, _) = self.actor(&[Role::Owner])?;
                let held = s.entitlement.as_ref().map(|e| e.version).unwrap_or(0);
                let e = self.entry(
                    dev,
                    MOp::Entitlement {
                        version: held.saturating_add(1),
                        active: true,
                        quota: 1 << 30,
                        seats: 5,
                    },
                );
                Some(self.step(e))
            }
            NearMiss::AppOpByAGuest => {
                let (dev, _) = self.actor(&[Role::Guest])?;
                let e = self.app_entry(dev);
                Some(self.step(e))
            }
            NearMiss::PendingDeviceSigns => {
                let pending: Vec<(Dev, Stamp)> = s
                    .devices
                    .values()
                    .filter_map(|d| match d.state {
                        DState::Pending(until) if now < until => Some((d.id, until)),
                        _ => None,
                    })
                    .filter(|(d, _)| {
                        s.devices
                            .get(d)
                            .and_then(|dv| s.members.get(&dv.member))
                            .is_some_and(|m| {
                                m.state == MState::Active
                                    && m.kind == MKind::Person
                                    && m.role != Role::Guest
                            })
                    })
                    .collect();
                let (dev, until) = self.pick(&pending)?;
                let e = self.app_entry(dev);
                // one second before the hold is over, or now
                let when = if self.pct(40) && until.as_secs() > now.as_secs().saturating_add(1) {
                    Stamp::from_secs(until.as_secs() - 1)
                } else {
                    now
                };
                Some(Step {
                    entry: e,
                    now: when,
                })
            }
            NearMiss::PolicySetThatLoosensAGroup => {
                let group = self.any_group()?;
                let inherited = s.inherited_policy(group)?;
                let mut patch = MPatch::default();
                let mut fields: Vec<PField> = PField::ALL.to_vec();
                self.rng.shuffle(&mut fields);
                for f in fields {
                    if self.loosen(&mut patch, &inherited, f) {
                        break;
                    }
                }
                if patch.is_empty() {
                    return None;
                }
                let signer = self.actor(&ADMINISH)?.0;
                let e = self.entry(
                    signer,
                    MOp::PolicySet {
                        scope: PScope::Group(group),
                        patch,
                    },
                );
                let e = self.finish(e);
                Some(self.step(e))
            }
            NearMiss::SuspendedMemberSigns => {
                let devs: Vec<Dev> = s
                    .devices
                    .values()
                    .filter(|d| {
                        d.state.is_active_at(now)
                            && s.members.get(&d.member).is_some_and(|m| {
                                m.state == MState::Suspended
                                    && m.kind == MKind::Person
                                    && m.role != Role::Guest
                            })
                    })
                    .map(|d| d.id)
                    .collect();
                let dev = self.pick(&devs)?;
                let e = self.app_entry(dev);
                Some(self.step(e))
            }
            NearMiss::RevokedDeviceSigns => {
                let devs: Vec<Dev> = s
                    .devices
                    .values()
                    .filter(|d| {
                        d.state == DState::Revoked
                            && s.members.get(&d.member).is_some_and(|m| {
                                m.state == MState::Active
                                    && m.kind == MKind::Person
                                    && m.role != Role::Guest
                            })
                    })
                    .map(|d| d.id)
                    .collect();
                let dev = self.pick(&devs)?;
                let e = self.app_entry(dev);
                Some(self.step(e))
            }
            NearMiss::ForgedSignature => {
                let mut e = self.valid()?;
                e.sig = SigClaim::Invalid;
                Some(self.step(e))
            }
            NearMiss::CorruptCopy => {
                let mut e = self.valid()?;
                e.corrupt = true;
                Some(self.step(e))
            }
            NearMiss::StampBackwards => {
                let last = s.last_at?;
                if last.as_secs() == 0 {
                    return None;
                }
                let e = self.valid()?;
                Some(Step {
                    entry: e,
                    now: Stamp::from_secs(last.as_secs() - 1),
                })
            }
            NearMiss::EpochBelowChain => {
                if s.epoch == 0 {
                    return None;
                }
                let mut e = self.valid()?;
                e.epoch = EpochSpec::Below;
                Some(self.step(e))
            }
            NearMiss::WrongAccount => {
                let mut e = self.valid()?;
                e.account = AccountSpec::Other;
                Some(self.step(e))
            }
            NearMiss::UnknownWireOp => {
                let list = s.active_devices(now);
                let (dev, _) = self.pick(&list)?;
                let word = between(self.rng, 0, 200) as u8;
                let e = self.entry(dev, MOp::Unknown { word });
                Some(self.step(e))
            }
            NearMiss::EscrowOpenWithoutAssertion => {
                let mut e = self.base(&[OpKind::EscrowOpen])?;
                // an assertion that does not count: none at all, one whose user was not verified, one
                // over another entry's hash, one for another relying party, or one made by a device
                // that did not sign (the signers' assertions are what the quorum asks for)
                let outsiders: Vec<Dev> = s
                    .active_devices(now)
                    .into_iter()
                    .map(|(d, _)| d)
                    .filter(|d| {
                        !e.cosigs.iter().any(|(c, _)| c == d) && e.signer != MSigner::Device(*d)
                    })
                    .collect();
                match between(self.rng, 0, 4) {
                    0 => e.assertions.clear(),
                    1..=3 => {
                        let i = between(self.rng, 0, e.assertions.len().saturating_sub(1));
                        let which = between(self.rng, 1, 3);
                        let a = e.assertions.get_mut(i)?;
                        match which {
                            1 => a.user_verified = false,
                            2 => a.challenge_ok = false,
                            _ => a.rp_ok = false,
                        }
                    }
                    _ => {
                        let other = self.pick(&outsiders)?;
                        let i = between(self.rng, 0, e.assertions.len().saturating_sub(1));
                        e.assertions.get_mut(i)?.dev = other;
                    }
                }
                Some(self.step(e))
            }
            NearMiss::JoinWithWrongSecret => {
                let mut e = self.cand(OpKind::MemberJoin)?;
                if let MOp::MemberJoin { secret, .. } = &mut e.op {
                    *secret = secret.wrapping_add(1);
                }
                Some(self.step(e))
            }
            NearMiss::PrivateToManagedWithoutConsent => {
                let member = self.member_where(|m| {
                    m.kind == MKind::Person && m.state == MState::Active && !m.managed
                })?;
                let (dev, mem) = self.actor(&OWNER_ADMIN)?;
                if mem == member {
                    return None;
                }
                let e = self.entry(
                    dev,
                    MOp::MemberManaged {
                        member,
                        managed: true,
                    },
                );
                Some(self.step(e))
            }
            NearMiss::DeviceAddQuorumUnderSelfPolicy => {
                if s.policy.device_approval != DevApproval::SelfOnly {
                    return None;
                }
                let (dev, mem) = self.actor(&OWNER_ADMIN)?;
                let member = self.member_where(|m| {
                    m.id != mem && m.kind == MKind::Person && m.state == MState::Active
                })?;
                let device = self.spare_device()?;
                let e = self.entry(dev, MOp::DeviceAdd { member, device });
                let e = self.finish(e);
                Some(self.step(e))
            }
            NearMiss::GroupAdminPullsAnAdminIntoScope => {
                let (dev, mem) = self.actor(&[Role::GroupAdmin])?;
                let scope = s.members.get(&mem)?.scope?;
                let inside: Vec<Grp> = s.subtree(scope).into_iter().collect();
                let group = self.pick(&inside)?;
                let target = self.member_where(|m| {
                    m.state != MState::Removed && {
                        let theirs = s.groups_of(m.id);
                        !theirs.is_empty() && theirs.iter().all(|g| !s.in_subtree(*g, scope))
                    }
                })?;
                let e = self.entry(
                    dev,
                    MOp::GroupMember {
                        group,
                        member: target,
                        present: true,
                    },
                );
                Some(self.step(e))
            }
            NearMiss::AdminEnrollsADeviceForTheOwner => {
                let (dev, mem) = self.actor(&[Role::Admin])?;
                let owner = self.member_where(|m| {
                    m.role == Role::Owner && m.state == MState::Active && m.id != mem
                })?;
                let device = self.spare_device()?;
                let e = self.entry(
                    dev,
                    MOp::DeviceAdd {
                        member: owner,
                        device,
                    },
                );
                let e = self.finish(e);
                Some(self.step(e))
            }
            NearMiss::DelegateSuspendsAnOwner => {
                let key = self.live_delegate_for(OpKind::MemberSuspend)?;
                let owner = self.member_where(|m| {
                    m.role == Role::Owner && m.state == MState::Active && m.kind == MKind::Person
                })?;
                let id = self.new_eid();
                let e = ModelEntry::new(
                    id,
                    MSigner::Delegate(key),
                    MOp::MemberSuspend { member: owner },
                );
                let e = self.finish(e);
                Some(self.step(e))
            }
            NearMiss::EntitlementNotNewer => {
                let held = s.entitlement.as_ref()?.version;
                let version = if held > 1 && self.pct(40) {
                    held - 1
                } else {
                    held
                };
                let id = self.new_eid();
                let e = ModelEntry::new(
                    id,
                    MSigner::Payments,
                    MOp::Entitlement {
                        version,
                        active: true,
                        quota: 1 << 20,
                        seats: 1,
                    },
                );
                Some(self.step(e))
            }
            NearMiss::ReusedId => {
                let which = between(self.rng, 0, 3);
                let device_ids: Vec<Dev> = s.devices.keys().copied().collect();
                let e = match which {
                    0 => {
                        let mut e = self.cand(OpKind::MemberInvite)?;
                        let ids: Vec<Mem> = s.members.keys().copied().collect();
                        let taken = self.pick(&ids)?;
                        if let MOp::MemberInvite { member, .. } = &mut e.op {
                            *member = taken;
                        }
                        e
                    }
                    1 => {
                        let mut e = self.cand(OpKind::DeviceAdd)?;
                        let taken = self.pick(&device_ids)?;
                        if let MOp::DeviceAdd { device, .. } = &mut e.op {
                            *device = taken;
                        }
                        e
                    }
                    2 => {
                        let mut e = self.cand(OpKind::GroupCreate)?;
                        let ids: Vec<Grp> = s.groups.keys().copied().collect();
                        let taken = self.pick(&ids)?;
                        if let MOp::GroupCreate { group, .. } = &mut e.op {
                            *group = taken;
                        }
                        e
                    }
                    _ => {
                        let mut e = self.cand(OpKind::MemberJoin)?;
                        let taken = self.pick(&device_ids)?;
                        if let MOp::MemberJoin { device, .. } = &mut e.op {
                            *device = taken;
                        }
                        e.signer = MSigner::Device(taken);
                        e
                    }
                };
                Some(self.step(e))
            }
            NearMiss::RootGroupMoves => {
                let roots: Vec<Grp> = s
                    .groups
                    .iter()
                    .filter(|(_, p)| p.is_none())
                    .map(|(g, _)| *g)
                    .collect();
                let group = self.pick(&roots)?;
                let outside: Vec<Grp> = s
                    .groups
                    .keys()
                    .copied()
                    .filter(|g| !s.in_subtree(*g, group))
                    .collect();
                let new_parent = self.pick(&outside)?;
                let signer = self.actor(&OWNER_ADMIN)?.0;
                let e = self.entry(signer, MOp::GroupMove { group, new_parent });
                let e = self.finish(e);
                Some(self.step(e))
            }
        }
    }

    /// A valid entry of an op that needs a quorum of at least two people and carries cosignatures.
    fn quorum_base(&mut self) -> Option<ModelEntry> {
        let ops = [
            OpKind::MemberRole,
            OpKind::PolicySet,
            OpKind::EscrowSet,
            OpKind::AuditKeySet,
            OpKind::AdminQuorumSet,
            OpKind::Delegate,
            OpKind::EscrowOpen,
            OpKind::MemberRemove,
            OpKind::MemberSuspend,
            OpKind::DeviceAdd,
        ];
        for _ in 0..6 {
            let e = self.base(&ops)?;
            let need = quorum_requirement(self.s, &e, self.now)?;
            if need >= 2 && !e.cosigs.is_empty() {
                return Some(e);
            }
        }
        None
    }

    /// A random element of a list of clonable things.
    fn pick_clone<T: Clone>(&mut self, items: &[T]) -> Option<T> {
        pick(self.rng, items).cloned()
    }

    /// Puts into the patch a value of `f` that is looser than the inherited one; `false` when the field has
    /// no looser value.
    fn loosen(&mut self, patch: &mut MPatch, inh: &crate::model::MPolicy, f: PField) -> bool {
        match f {
            PField::CredentialTtl => {
                patch.credential_ttl = Some(Span::secs(inh.credential_ttl.as_secs() + 3600));
                true
            }
            PField::EnrollHold => {
                if inh.enroll_hold.as_secs() < 3600 {
                    return false;
                }
                patch.enroll_hold = Some(Span::secs(inh.enroll_hold.as_secs() - 3600));
                true
            }
            PField::EscrowOpenWait => {
                if inh.escrow_open_wait.as_secs() < 3600 {
                    return false;
                }
                patch.escrow_open_wait = Some(Span::secs(inh.escrow_open_wait.as_secs() - 3600));
                true
            }
            PField::MaxDevices => {
                if inh.max_devices == u8::MAX {
                    return false;
                }
                patch.max_devices = Some(inh.max_devices + 1);
                true
            }
            PField::AuditLevel => {
                if inh.audit_level == 0 {
                    return false;
                }
                patch.audit_level = Some(inh.audit_level - 1);
                true
            }
            PField::AdminQuorum => {
                if inh.admin_quorum <= 1 {
                    return false;
                }
                patch.admin_quorum = Some(inh.admin_quorum - 1);
                true
            }
            PField::SsoRequired => {
                if !inh.sso_required {
                    return false;
                }
                patch.sso_required = Some(false);
                true
            }
            PField::PasskeyRecovery => {
                if inh.passkey_recovery {
                    return false;
                }
                patch.passkey_recovery = Some(true);
                true
            }
            PField::BrowserDevices => {
                if inh.browser_devices {
                    return false;
                }
                patch.browser_devices = Some(true);
                true
            }
            PField::ExternalSharing => {
                if inh.external_sharing == ExtSharing::Allowed {
                    return false;
                }
                patch.external_sharing = Some(ExtSharing::Allowed);
                true
            }
            PField::DeviceApproval => {
                if inh.device_approval == DevApproval::SelfOrQuorum {
                    return false;
                }
                patch.device_approval = Some(DevApproval::SelfOrQuorum);
                true
            }
            PField::ManagedDefault => {
                patch.managed_default = Some(!inh.managed_default);
                true
            }
            PField::Updates => {
                patch.updates = Some(if inh.updates == 0 { 1 } else { 0 });
                true
            }
            PField::Padding => {
                patch.padding = Some(if inh.padding == 0 { 1 } else { 0 });
                true
            }
        }
    }
}

/// Generates one entry in a mode from the model's state: `None` when the state has no entry of that kind to
/// offer (the caller tries another mode). A near miss whose precondition is missing may come back as a
/// plain valid entry that makes the precondition (`Generated::near_miss` is then `None`).
///
/// Implements planning/architecture.md section 12 E1 ("a seeded generator of random sequences of ops, group
/// moves and delegations (valid, and near-misses on purpose)"); Caller: `compare.rs` (`differential`), the
/// near-miss tests.
pub fn gen_entry(
    rng: &mut Rng,
    u: &Universe,
    s: &ModelState,
    now: Stamp,
    eid: &mut u64,
    mode: Mode,
) -> Option<Generated> {
    let mut g = G {
        rng,
        u,
        s,
        now,
        eid,
    };
    match mode {
        Mode::Valid => {
            let mut e = g.valid()?;
            g.dress(&mut e);
            Some(Generated {
                step: g.step(e),
                near_miss: None,
            })
        }
        Mode::Random => {
            let e = g.random()?;
            Some(Generated {
                step: g.step(e),
                near_miss: None,
            })
        }
        Mode::NearMiss(nm) => g.near_miss(nm),
    }
}

/// The account kinds a seeded run draws from, as weights (team accounts can make admins, so they are the
/// most useful; a business shows the quorum deadlock).
///
/// Implements planning/architecture.md section 5.10 (the four columns); Caller: `compare.rs`.
pub fn pick_kind(rng: &mut Rng) -> AcctKind {
    match between(rng, 0, 99) {
        0..=9 => AcctKind::Personal,
        10..=19 => AcctKind::Family,
        20..=74 => AcctKind::Team,
        _ => AcctKind::Business,
    }
}

/// The mode of the next entry of a seeded run: half valid, a third near misses (every kind equally), the
/// rest random.
///
/// Implements planning/architecture.md section 12 E1; Caller: `compare.rs`.
pub fn pick_mode(rng: &mut Rng) -> Mode {
    match between(rng, 0, 99) {
        0..=44 => Mode::Valid,
        45..=79 => {
            let i = between(rng, 0, NearMiss::ALL.len().saturating_sub(1));
            match NearMiss::ALL.get(i) {
                Some(nm) => Mode::NearMiss(*nm),
                None => Mode::Valid,
            }
        }
        _ => Mode::Random,
    }
}

/// Whether the model admits the step (a convenience for the tests).
///
/// Implements planning/architecture.md section 12 E1; Caller: the tests.
pub fn step_is_valid(s: &ModelState, step: &Step) -> bool {
    model_admit(s, &step.entry, step.now) == Verdict::Admit
}

// ---------------------------------------------------------------------------------------------------
// The Rust source of a step: the pasteable body of a failing test.
// ---------------------------------------------------------------------------------------------------

fn src_stamp(t: Stamp) -> String {
    format!("Stamp::from_secs({})", t.as_secs())
}

fn src_span(s: Span) -> String {
    format!("Span::secs({})", s.as_secs())
}

/// A set (or any collection) as `[a, b].into_iter().collect()`, which a field of the right type accepts.
fn src_set<T>(items: impl Iterator<Item = T>, show: impl Fn(T) -> String) -> String {
    let parts: Vec<String> = items.map(show).collect();
    format!("[{}].into_iter().collect()", parts.join(", "))
}

fn src_opt<T>(o: &Option<T>, show: impl Fn(&T) -> String) -> String {
    match o {
        Some(v) => format!("Some({})", show(v)),
        None => "None".to_string(),
    }
}

fn src_app(a: &MApp) -> String {
    format!(
        "MApp {{ scopes: {}, expires: {} }}",
        src_set(a.scopes.iter(), |x| format!("{x:?}")),
        src_opt(&a.expires, |t| src_stamp(*t))
    )
}

fn src_patch(p: &MPatch) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(v) = p.credential_ttl {
        parts.push(format!("credential_ttl: Some({})", src_span(v)));
    }
    if let Some(v) = p.device_approval {
        parts.push(format!("device_approval: Some(DevApproval::{v:?})"));
    }
    if let Some(v) = p.max_devices {
        parts.push(format!("max_devices: Some({v})"));
    }
    if let Some(v) = p.managed_default {
        parts.push(format!("managed_default: Some({v})"));
    }
    if let Some(v) = p.sso_required {
        parts.push(format!("sso_required: Some({v})"));
    }
    if let Some(v) = p.enroll_hold {
        parts.push(format!("enroll_hold: Some({})", src_span(v)));
    }
    if let Some(v) = p.passkey_recovery {
        parts.push(format!("passkey_recovery: Some({v})"));
    }
    if let Some(v) = p.escrow_open_wait {
        parts.push(format!("escrow_open_wait: Some({})", src_span(v)));
    }
    if let Some(v) = p.audit_level {
        parts.push(format!("audit_level: Some({v})"));
    }
    if let Some(v) = p.admin_quorum {
        parts.push(format!("admin_quorum: Some({v})"));
    }
    if let Some(v) = p.browser_devices {
        parts.push(format!("browser_devices: Some({v})"));
    }
    if let Some(v) = p.external_sharing {
        parts.push(format!("external_sharing: Some(ExtSharing::{v:?})"));
    }
    if let Some(v) = p.updates {
        parts.push(format!("updates: Some({v})"));
    }
    if let Some(v) = p.padding {
        parts.push(format!("padding: Some({v})"));
    }
    parts.push("..MPatch::default()".to_string());
    format!("MPatch {{ {} }}", parts.join(", "))
}

fn src_op(op: &MOp) -> String {
    match op {
        MOp::Genesis { owner, device } => {
            format!("MOp::Genesis {{ owner: {owner:?}, device: {device:?} }}")
        }
        MOp::DeviceAdd { member, device } => {
            format!("MOp::DeviceAdd {{ member: {member:?}, device: {device:?} }}")
        }
        MOp::DeviceRemove { device } => format!("MOp::DeviceRemove {{ device: {device:?} }}"),
        MOp::MemberInvite {
            member,
            kind,
            role,
            secret,
            groups,
            app,
        } => format!(
            "MOp::MemberInvite {{ member: {member:?}, kind: MKind::{kind:?}, role: Role::{role:?}, secret: {secret}, groups: {}, app: {} }}",
            src_set(groups.iter(), |g| format!("{g:?}")),
            src_opt(app, src_app)
        ),
        MOp::MemberJoin {
            member,
            device,
            secret,
        } => format!(
            "MOp::MemberJoin {{ member: {member:?}, device: {device:?}, secret: {secret} }}"
        ),
        MOp::MemberRole {
            member,
            role,
            scope,
        } => format!(
            "MOp::MemberRole {{ member: {member:?}, role: Role::{role:?}, scope: {scope:?} }}"
        ),
        MOp::MemberSuspend { member } => format!("MOp::MemberSuspend {{ member: {member:?} }}"),
        MOp::MemberRemove { member } => format!("MOp::MemberRemove {{ member: {member:?} }}"),
        MOp::MemberManaged { member, managed } => {
            format!("MOp::MemberManaged {{ member: {member:?}, managed: {managed} }}")
        }
        MOp::GroupCreate { group, parent } => {
            format!("MOp::GroupCreate {{ group: {group:?}, parent: {parent:?} }}")
        }
        MOp::GroupMember {
            group,
            member,
            present,
        } => format!(
            "MOp::GroupMember {{ group: {group:?}, member: {member:?}, present: {present} }}"
        ),
        MOp::GroupMove { group, new_parent } => {
            format!("MOp::GroupMove {{ group: {group:?}, new_parent: {new_parent:?} }}")
        }
        MOp::PolicySet { scope, patch } => format!(
            "MOp::PolicySet {{ scope: PScope::{scope:?}, patch: {} }}",
            src_patch(patch)
        ),
        MOp::EscrowSet { key, holders } => format!(
            "MOp::EscrowSet {{ key: {key}, holders: {} }}",
            src_set(holders.iter(), |m| format!("{m:?}"))
        ),
        MOp::AuditKeySet { key } => format!("MOp::AuditKeySet {{ key: {key} }}"),
        MOp::AdminQuorumSet { quorum } => format!("MOp::AdminQuorumSet {{ quorum: {quorum} }}"),
        MOp::EscrowOpen { member, reason } => {
            format!("MOp::EscrowOpen {{ member: {member:?}, reason: Reason::{reason:?} }}")
        }
        MOp::Delegate { key, ops, expires } => format!(
            "MOp::Delegate {{ key: {key:?}, ops: {}, expires: {} }}",
            src_set(ops.iter(), |k| format!("OpKind::{k:?}")),
            src_opt(expires, |t| src_stamp(*t))
        ),
        MOp::Entitlement {
            version,
            active,
            quota,
            seats,
        } => format!(
            "MOp::Entitlement {{ version: {version}, active: {active}, quota: {quota}, seats: {seats} }}"
        ),
        MOp::App { name, body_len } => {
            format!("MOp::App {{ name: {name:?}, body_len: {body_len} }}")
        }
        MOp::Unknown { word } => format!("MOp::Unknown {{ word: {word} }}"),
    }
}

fn src_entry(e: &ModelEntry) -> String {
    let cosigs: Vec<String> = e
        .cosigs
        .iter()
        .map(|(d, c)| format!("({d:?}, CosigClaim::{c:?})"))
        .collect();
    let assertions: Vec<String> = e
        .assertions
        .iter()
        .map(|a| {
            format!(
                "MAssertion {{ dev: {:?}, user_verified: {}, challenge_ok: {}, rp_ok: {} }}",
                a.dev, a.user_verified, a.challenge_ok, a.rp_ok
            )
        })
        .collect();
    format!(
        "ModelEntry {{ eid: {}, seq: SeqSpec::{:?}, prev: PrevSpec::{:?}, epoch: EpochSpec::{:?}, account: AccountSpec::{:?}, at: {}, signer: MSigner::{:?}, sig: SigClaim::{:?}, cosigs: vec![{}], assertions: vec![{}], corrupt: {}, op: {} }}",
        e.eid,
        e.seq,
        e.prev,
        e.epoch,
        e.account,
        src_stamp(e.at),
        e.signer,
        e.sig,
        cosigs.join(", "),
        assertions.join(", "),
        e.corrupt,
        src_op(&e.op)
    )
}

impl Step {
    /// The step as Rust source: an expression that builds it with `use azlin_escalate_e1::model::*;`,
    /// `use azlin_escalate_e1::gen::Step;` and `use azlin_seams::{Span, Stamp};` in scope. The body of a
    /// failing test is a list of these.
    ///
    /// Implements planning/architecture.md section 12 E1 ("the failure message prints the seed AND the
    /// minimal sequence as a pasteable test body"); Caller: `compare::pasteable_test`, the tests.
    pub fn rust(&self) -> String {
        format!(
            "Step {{ entry: {}, now: {} }}",
            src_entry(&self.entry),
            src_stamp(self.now)
        )
    }
}
