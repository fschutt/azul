//! The projection: both sides' states as one plain [`StateView`] (planning/architecture.md section 12 E1:
//! "one pure function, checked against a separate reference model"; lane W1-S47a).
//!
//! The model's [`ModelState`] and the production's `azlin_teamlog::StateView` are both turned into the same
//! plain data: members, devices, the group tree and its members, the tracked part of the policy and the
//! group patches, the delegations, the escrow facts, the entitlement and the escrow openings that wait. Ids
//! are the model's own numbers (the production ids are built from them, `universe.rs`); an id that does not
//! decode is listed in `alien`, so a production state that holds something the generator never made shows up
//! as a difference. The restore epoch, the head's position and the last stamp are part of the view; the
//! head's hash is not (the chain check covers it) and neither are the log positions of the rows.

#![allow(missing_docs)]

use std::collections::BTreeSet;
use std::fmt::Debug;

use azlin_policy::{
    AuditLevel, DeviceApproval, ExternalSharing, LinkPolicyPatch, PaddingPolicy, Policy,
    PolicyPatch, RequestPolicyPatch, UpdatePolicy,
};
use azlin_seams::{AccountRole, DeviceId, GroupId, KeyId, MemberId, MemberKind, Stamp};
use azlin_teamlog as tl;

use crate::model::{
    AppId, DState, Dev, DevApproval, Enrolled, ExtSharing as MExt, Grp, Key, MKind, MPatch,
    MPolicy, MState, Mem, ModelState, OpKind, Reason, Role,
};
use crate::universe::{
    blob_key_from_bytes, delegate_key_from_bytes, dev_from_bytes, grp_from_bytes, mem_from_bytes,
};

/// One member as the view shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberRow {
    pub id: Mem,
    pub kind: MKind,
    pub role: Role,
    pub scope: Option<Grp>,
    pub managed: bool,
    pub state: MState,
    pub has_keys: bool,
    /// `None`: no open invitation; `Some(inviter)`: an open one (the inviter is `None` for a delegate key).
    pub open_invite: Option<Option<Mem>>,
    /// The scopes and the expiry of an app principal.
    pub app: Option<(BTreeSet<AppId>, Option<Stamp>)>,
    pub added_at: Stamp,
    pub removed_at: Option<Stamp>,
}

/// One device as the view shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceRow {
    pub id: Dev,
    pub member: Mem,
    pub state: DState,
    pub enrolled: Enrolled,
    pub added_at: Stamp,
    pub revoked_at: Option<Stamp>,
}

/// One delegation as the view shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationRow {
    pub key: Key,
    pub ops: BTreeSet<OpKind>,
    pub expires: Option<Stamp>,
    pub granted_by: Mem,
}

/// The latest entitlement as the view shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntitlementRow {
    pub version: u64,
    pub active: bool,
    pub quota: u64,
    pub seats: u32,
    pub applied_at: Stamp,
}

/// An escrow opening that waits, as the view shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenRow {
    pub seq: u64,
    pub member: Mem,
    pub reason: Reason,
    pub requested_at: Stamp,
    pub ready_at: Option<Stamp>,
    pub signers: BTreeSet<Mem>,
}

/// The plain state both sides are compared by.
///
/// Implements planning/architecture.md section 12 E1 ("devices run their own check, and the test runs
/// both"); Caller: `compare.rs`, `stories.rs`, the tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateView {
    pub head_seq: Option<u64>,
    pub last_at: Option<Stamp>,
    pub epoch: u64,
    pub members: Vec<MemberRow>,
    pub devices: Vec<DeviceRow>,
    pub groups: Vec<(Grp, Option<Grp>)>,
    pub memberships: Vec<(Grp, Mem)>,
    pub policy: MPolicy,
    pub group_patches: Vec<(Grp, MPatch)>,
    pub policy_epoch: u32,
    pub delegations: Vec<DelegationRow>,
    pub escrow_pub: Option<u16>,
    pub escrow_holders: Vec<Mem>,
    pub audit_pub: Option<u16>,
    pub entitlement: Option<EntitlementRow>,
    pub opens: Vec<OpenRow>,
    /// Things the production state holds that the model's ids and fields cannot say.
    pub alien: Vec<String>,
}

/// Debug text, cut at 500 characters.
fn clip<T: Debug>(value: &T) -> String {
    let text = format!("{value:?}");
    if text.len() > 500 {
        let cut: String = text.chars().take(500).collect();
        format!("{cut}...")
    } else {
        text
    }
}

/// Writes the first row where two lists differ.
fn list_diff<T: PartialEq + Debug>(name: &str, left: &[T], right: &[T], out: &mut String) {
    if left == right {
        return;
    }
    let first = left.iter().zip(right.iter()).position(|(a, b)| a != b);
    match first {
        Some(i) => {
            let a = left.get(i);
            let b = right.get(i);
            out.push_str(&format!(
                "{name}: row {i} differs: model {} | production {}\n",
                clip(&a),
                clip(&b)
            ));
        }
        None => out.push_str(&format!(
            "{name}: model has {} rows, production {}: model tail {} | production tail {}\n",
            left.len(),
            right.len(),
            clip(&left.get(right.len()..)),
            clip(&right.get(left.len()..))
        )),
    }
}

impl StateView {
    /// What differs between the model's view (`self`) and the production's (`other`), field by field; `None`
    /// when they are equal.
    ///
    /// Implements planning/architecture.md section 12 E1; Caller: `compare.rs`, the tests.
    pub fn diff(&self, other: &StateView) -> Option<String> {
        let mut out = String::new();
        if self.head_seq != other.head_seq {
            out.push_str(&format!(
                "head_seq: model {:?} | production {:?}\n",
                self.head_seq, other.head_seq
            ));
        }
        if self.last_at != other.last_at {
            out.push_str(&format!(
                "last_at: model {:?} | production {:?}\n",
                self.last_at, other.last_at
            ));
        }
        if self.epoch != other.epoch {
            out.push_str(&format!(
                "epoch: model {} | production {}\n",
                self.epoch, other.epoch
            ));
        }
        list_diff("members", &self.members, &other.members, &mut out);
        list_diff("devices", &self.devices, &other.devices, &mut out);
        list_diff("groups", &self.groups, &other.groups, &mut out);
        list_diff(
            "memberships",
            &self.memberships,
            &other.memberships,
            &mut out,
        );
        if self.policy != other.policy {
            out.push_str(&format!(
                "policy: model {} | production {}\n",
                clip(&self.policy),
                clip(&other.policy)
            ));
        }
        list_diff(
            "group_patches",
            &self.group_patches,
            &other.group_patches,
            &mut out,
        );
        if self.policy_epoch != other.policy_epoch {
            out.push_str(&format!(
                "policy_epoch: model {} | production {}\n",
                self.policy_epoch, other.policy_epoch
            ));
        }
        list_diff(
            "delegations",
            &self.delegations,
            &other.delegations,
            &mut out,
        );
        if self.escrow_pub != other.escrow_pub {
            out.push_str(&format!(
                "escrow_pub: model {:?} | production {:?}\n",
                self.escrow_pub, other.escrow_pub
            ));
        }
        list_diff(
            "escrow_holders",
            &self.escrow_holders,
            &other.escrow_holders,
            &mut out,
        );
        if self.audit_pub != other.audit_pub {
            out.push_str(&format!(
                "audit_pub: model {:?} | production {:?}\n",
                self.audit_pub, other.audit_pub
            ));
        }
        if self.entitlement != other.entitlement {
            out.push_str(&format!(
                "entitlement: model {} | production {}\n",
                clip(&self.entitlement),
                clip(&other.entitlement)
            ));
        }
        list_diff("opens", &self.opens, &other.opens, &mut out);
        if self.alien != other.alien {
            out.push_str(&format!(
                "alien: model {:?} | production {:?}\n",
                self.alien, other.alien
            ));
        }
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }
}

/// The view of the model's state.
///
/// Implements planning/architecture.md section 12 E1; Caller: `compare.rs`, `stories.rs`, the tests.
pub fn project_model(s: &ModelState) -> StateView {
    StateView {
        head_seq: s.head.map(|(seq, _)| seq),
        last_at: s.last_at,
        epoch: s.epoch,
        members: s
            .members
            .values()
            .map(|m| MemberRow {
                id: m.id,
                kind: m.kind,
                role: m.role,
                scope: m.scope,
                managed: m.managed,
                state: m.state,
                has_keys: m.has_keys,
                open_invite: m.invite.as_ref().map(|i| i.inviter),
                app: m.app.as_ref().map(|a| (a.scopes.clone(), a.expires)),
                added_at: m.added_at,
                removed_at: m.removed_at,
            })
            .collect(),
        devices: s
            .devices
            .values()
            .map(|d| DeviceRow {
                id: d.id,
                member: d.member,
                state: d.state,
                enrolled: d.enrolled,
                added_at: d.added_at,
                revoked_at: d.revoked_at,
            })
            .collect(),
        groups: s.groups.iter().map(|(g, p)| (*g, *p)).collect(),
        memberships: s.memberships.iter().copied().collect(),
        policy: s.policy.clone(),
        group_patches: s
            .group_patches
            .iter()
            .map(|(g, p)| (*g, p.clone()))
            .collect(),
        policy_epoch: s.policy_epoch,
        delegations: s
            .delegations
            .iter()
            .map(|(k, d)| DelegationRow {
                key: *k,
                ops: d.ops.clone(),
                expires: d.expires,
                granted_by: d.granted_by,
            })
            .collect(),
        escrow_pub: s.escrow_pub,
        escrow_holders: s.escrow_holders.iter().copied().collect(),
        audit_pub: s.audit_pub,
        entitlement: s.entitlement.as_ref().map(|e| EntitlementRow {
            version: e.version,
            active: e.active,
            quota: e.quota,
            seats: e.seats,
            applied_at: e.applied_at,
        }),
        opens: s
            .pending_opens
            .iter()
            .map(|o| OpenRow {
                seq: o.seq,
                member: o.member,
                reason: o.reason,
                requested_at: o.requested_at,
                ready_at: o.ready_at,
                signers: o.signers.clone(),
            })
            .collect(),
        alien: Vec::new(),
    }
}

fn mem_of(id: &MemberId, alien: &mut Vec<String>) -> Mem {
    match mem_from_bytes(id.as_bytes()) {
        Some(m) => m,
        None => {
            alien.push(format!("member id {id}"));
            Mem(u16::MAX)
        }
    }
}

fn dev_of(id: &DeviceId, alien: &mut Vec<String>) -> Dev {
    match dev_from_bytes(id.as_bytes()) {
        Some(d) => d,
        None => {
            alien.push(format!("device id {id}"));
            Dev(u16::MAX)
        }
    }
}

fn grp_of(id: &GroupId, alien: &mut Vec<String>) -> Grp {
    match grp_from_bytes(id.as_bytes()) {
        Some(g) => g,
        None => {
            alien.push(format!("group id {id}"));
            Grp(u16::MAX)
        }
    }
}

fn key_of(id: &KeyId, alien: &mut Vec<String>) -> Key {
    match delegate_key_from_bytes(id.as_bytes()) {
        Some(k) => k,
        None => {
            alien.push(format!("delegate key {id}"));
            Key(u16::MAX)
        }
    }
}

fn blob_of(bytes: &Option<[u8; 32]>, name: &str, alien: &mut Vec<String>) -> Option<u16> {
    match bytes {
        None => None,
        Some(b) => match blob_key_from_bytes(b) {
            Some(n) => Some(n),
            None => {
                alien.push(format!("{name} is no key the generator made"));
                None
            }
        },
    }
}

/// The model's role of a production role.
pub fn role_of(r: AccountRole) -> Role {
    match r {
        AccountRole::Owner => Role::Owner,
        AccountRole::Admin => Role::Admin,
        AccountRole::GroupAdmin => Role::GroupAdmin,
        AccountRole::Helpdesk => Role::Helpdesk,
        AccountRole::Member => Role::Member,
        AccountRole::Guest => Role::Guest,
    }
}

fn mkind_of(k: MemberKind) -> MKind {
    match k {
        MemberKind::Person => MKind::Person,
        MemberKind::App => MKind::App,
    }
}

fn mstate_of(s: tl::MemberState) -> MState {
    match s {
        tl::MemberState::Invited => MState::Invited,
        tl::MemberState::Pending => MState::Pending,
        tl::MemberState::Active => MState::Active,
        tl::MemberState::Suspended => MState::Suspended,
        tl::MemberState::Removed => MState::Removed,
    }
}

fn dstate_of(s: tl::DeviceState) -> DState {
    match s {
        tl::DeviceState::Pending { hold_until } => DState::Pending(hold_until),
        tl::DeviceState::Active => DState::Active,
        tl::DeviceState::Revoked => DState::Revoked,
    }
}

fn enrolled_of(e: tl::EnrolledBy) -> Enrolled {
    match e {
        tl::EnrolledBy::Genesis => Enrolled::Genesis,
        tl::EnrolledBy::Invite => Enrolled::Invite,
        tl::EnrolledBy::OwnDevice => Enrolled::OwnDevice,
        tl::EnrolledBy::Quorum => Enrolled::Quorum,
    }
}

/// The model's kind of op of a production op name.
pub fn opkind_of(n: tl::OpName) -> OpKind {
    match n {
        tl::OpName::Genesis => OpKind::Genesis,
        tl::OpName::DeviceAdd => OpKind::DeviceAdd,
        tl::OpName::DeviceRemove => OpKind::DeviceRemove,
        tl::OpName::MemberInvite => OpKind::MemberInvite,
        tl::OpName::MemberJoin => OpKind::MemberJoin,
        tl::OpName::MemberRole => OpKind::MemberRole,
        tl::OpName::MemberSuspend => OpKind::MemberSuspend,
        tl::OpName::MemberRemove => OpKind::MemberRemove,
        tl::OpName::MemberManaged => OpKind::MemberManaged,
        tl::OpName::GroupCreate => OpKind::GroupCreate,
        tl::OpName::GroupMember => OpKind::GroupMember,
        tl::OpName::GroupMove => OpKind::GroupMove,
        tl::OpName::PolicySet => OpKind::PolicySet,
        tl::OpName::EscrowSet => OpKind::EscrowSet,
        tl::OpName::AuditKeySet => OpKind::AuditKeySet,
        tl::OpName::AdminQuorumSet => OpKind::AdminQuorumSet,
        tl::OpName::EscrowOpen => OpKind::EscrowOpen,
        tl::OpName::Delegate => OpKind::Delegate,
        tl::OpName::Entitlement => OpKind::Entitlement,
        tl::OpName::App => OpKind::App,
    }
}

fn approval_of(a: DeviceApproval) -> DevApproval {
    match a {
        DeviceApproval::SelfOnly => DevApproval::SelfOnly,
        DeviceApproval::SelfOrQuorum => DevApproval::SelfOrQuorum,
    }
}

fn audit_of(a: AuditLevel) -> u8 {
    match a {
        AuditLevel::None => 0,
        AuditLevel::Admin => 1,
        AuditLevel::AccessSummary => 2,
        AuditLevel::AccessFull => 3,
    }
}

fn sharing_of(a: ExternalSharing) -> MExt {
    match a {
        ExternalSharing::Allowed => MExt::Allowed,
        ExternalSharing::SameWorkspaceOnly => MExt::SameWorkspaceOnly,
    }
}

fn updates_of(a: UpdatePolicy) -> u8 {
    match a {
        UpdatePolicy::Auto => 0,
        UpdatePolicy::Notify => 1,
        UpdatePolicy::ManagedByMdm => 2,
    }
}

fn padding_of(a: PaddingPolicy) -> u8 {
    match a {
        PaddingPolicy::None => 0,
        PaddingPolicy::Buckets => 1,
        PaddingPolicy::Full => 2,
    }
}

/// The tracked part of a production policy.
///
/// Implements planning/architecture.md section 5.10; Caller: `compare.rs`, the tests (the model's defaults
/// are compared with `Policy::defaults`).
pub fn policy_of(p: &Policy) -> MPolicy {
    MPolicy {
        credential_ttl: p.credential_ttl,
        device_approval: approval_of(p.device_approval),
        max_devices: p.max_devices_per_member,
        managed_default: p.managed_default.is_managed(),
        sso_required: p.sso_required,
        enroll_hold: p.enroll_hold,
        passkey_recovery: p.passkey_recovery,
        escrow_open_wait: p.escrow_open_wait,
        audit_level: audit_of(p.audit_level),
        admin_quorum: p.admin_quorum.get(),
        browser_devices: p.browser_devices,
        external_sharing: sharing_of(p.external_sharing),
        updates: updates_of(p.updates),
        padding: padding_of(p.padding),
    }
}

fn patch_of(p: &PolicyPatch, alien: &mut Vec<String>) -> MPatch {
    if p.link_policy != LinkPolicyPatch::default()
        || p.request_policy != RequestPolicyPatch::default()
        || p.sso_max_age.is_some()
    {
        alien.push("a group patch sets a field the model does not track".to_string());
    }
    MPatch {
        credential_ttl: p.credential_ttl,
        device_approval: p.device_approval.map(approval_of),
        max_devices: p.max_devices_per_member,
        managed_default: p.managed_default.map(|m| m.is_managed()),
        sso_required: p.sso_required,
        enroll_hold: p.enroll_hold,
        passkey_recovery: p.passkey_recovery,
        escrow_open_wait: p.escrow_open_wait,
        audit_level: p.audit_level.map(audit_of),
        admin_quorum: p.admin_quorum.map(|q| q.get()),
        browser_devices: p.browser_devices,
        external_sharing: p.external_sharing.map(sharing_of),
        updates: p.updates.map(updates_of),
        padding: p.padding.map(padding_of),
    }
}

/// The number of an app name made by the converter (`app<n>`).
fn app_of(name: &tl::AppName, alien: &mut Vec<String>) -> AppId {
    let parsed = name
        .as_str()
        .strip_prefix("app")
        .and_then(|rest| rest.parse::<u8>().ok());
    match parsed {
        Some(n) => AppId(n),
        None => {
            alien.push(format!("app name {name}"));
            AppId(u8::MAX)
        }
    }
}

fn reason_of(r: tl::EscrowReason) -> Reason {
    match r {
        tl::EscrowReason::LeaverHandover => Reason::LeaverHandover,
        tl::EscrowReason::LegalRequest => Reason::LegalRequest,
        tl::EscrowReason::Other => Reason::Other,
    }
}

/// The view of the production's state (`TeamState::view`).
///
/// Implements planning/architecture.md section 12 E1; Caller: `compare.rs`, `stories.rs`, the tests.
pub fn project_prod(v: &tl::StateView) -> StateView {
    let mut alien: Vec<String> = Vec::new();
    let members = v
        .members
        .iter()
        .map(|m| MemberRow {
            id: mem_of(&m.id, &mut alien),
            kind: mkind_of(m.kind),
            role: role_of(m.role),
            scope: m.admin_scope.map(|g| grp_of(&g, &mut alien)),
            managed: m.managed,
            state: mstate_of(m.state),
            has_keys: m.keys.is_some(),
            open_invite: m
                .invite
                .as_ref()
                .map(|i| i.inviter.map(|inviter| mem_of(&inviter, &mut alien))),
            app: m.app.as_ref().map(|a| {
                (
                    a.scopes.iter().map(|n| app_of(n, &mut alien)).collect(),
                    a.expires_at,
                )
            }),
            added_at: m.added_at,
            removed_at: m.removed_at,
        })
        .collect();
    let devices = v
        .devices
        .iter()
        .map(|d| DeviceRow {
            id: dev_of(&d.id, &mut alien),
            member: mem_of(&d.member, &mut alien),
            state: dstate_of(d.state),
            enrolled: enrolled_of(d.enrolled_by),
            added_at: d.added_at,
            revoked_at: d.revoked_at,
        })
        .collect();
    let groups = v
        .groups
        .iter()
        .map(|g| {
            (
                grp_of(&g.id, &mut alien),
                g.parent.map(|p| grp_of(&p, &mut alien)),
            )
        })
        .collect();
    let memberships = v
        .group_members
        .iter()
        .map(|(g, m)| (grp_of(g, &mut alien), mem_of(m, &mut alien)))
        .collect();
    let group_patches = v
        .group_patches
        .iter()
        .map(|(g, p)| (grp_of(g, &mut alien), patch_of(p, &mut alien)))
        .collect();
    let delegations = v
        .delegations
        .iter()
        .map(|d| DelegationRow {
            key: key_of(&d.key, &mut alien),
            ops: d.ops.iter().map(|n| opkind_of(*n)).collect(),
            expires: d.expires,
            granted_by: mem_of(&d.granted_by, &mut alien),
        })
        .collect();
    let escrow_holders = v
        .escrow_holders
        .iter()
        .map(|m| mem_of(m, &mut alien))
        .collect();
    let escrow_pub = blob_of(&v.escrow_pub, "escrow_pub", &mut alien);
    let audit_pub = blob_of(&v.audit_pub, "audit_pub", &mut alien);
    let entitlement = v.entitlement.as_ref().map(|e| EntitlementRow {
        version: e.version.0,
        active: matches!(e.state, tl::EntitlementState::Active),
        quota: e.quota.get(),
        seats: e.seats,
        applied_at: e.applied_at,
    });
    let opens = v
        .pending_escrow_opens
        .iter()
        .map(|o| OpenRow {
            seq: o.seq.get(),
            member: mem_of(&o.member, &mut alien),
            reason: reason_of(o.reason),
            requested_at: o.requested_at,
            ready_at: o.ready_at,
            signers: o.signers.iter().map(|m| mem_of(m, &mut alien)).collect(),
        })
        .collect();
    StateView {
        head_seq: v.head.map(|h| h.seq.get()),
        last_at: v.last_at,
        epoch: v.restore_epoch.get(),
        members,
        devices,
        groups,
        memberships,
        policy: policy_of(&v.policy),
        group_patches,
        policy_epoch: v.policy_epoch.get(),
        delegations,
        escrow_pub,
        escrow_holders,
        audit_pub,
        entitlement,
        opens,
        alien,
    }
}
