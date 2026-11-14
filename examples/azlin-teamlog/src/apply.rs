//! `apply`: turns an admitted entry, deterministically, into the registry's rows and the notices of
//! section 5.9 (planning/architecture.md section 5.9 "The team log and the space chains": "the
//! metadata group applies each entry once, as it arrives, into the registry's tables"; build unit S17,
//! lane W1-S17c1).
//!
//! `apply(state, admitted)` is the one place the state of the team log changes. It is a function of
//! the state, the admitted entry and the leader's stamp: two replicas that apply the same entries in
//! the same order hold equal states (`TeamState::state_hash`) and make equal [`Effects`]. It reads no
//! clock (the stamp is in the witness), draws no entropy, touches no I/O, and iterates only ordered
//! collections.
//!
//! [`Admitted`] is a witness: its fields are private and it has no `Clone`, `Default` or `From`, so
//! only `admit` (the permission check) can make one. `apply` does not re-decide the permission, but
//! it never leaves a half-applied entry behind either: each op first computes everything that can
//! fail and only then changes the state, and an entry that does not fit the state it is applied to
//! (an `Admitted` made against another state of the chain) is not applied at all
//! (`Effects::applied == false`, state untouched).

use azlin_chain::{Admitted as ChainAdmitted, ChainEntry, ChainId};
use azlin_policy::{apply as apply_patch, effective, next_epoch, PolicyPatch};
use azlin_seams::{
    AccountId, ChainSeq, DeviceId, Hash32, KeyId, MemberId, MemberKind, RestoreEpoch, Stamp,
};

use crate::devices::{Device, DeviceState, EnrolledBy};
use crate::effects::{
    subject_of, Effects, Notice, RegistryDelta, RevocationReason, RevocationSubject,
};
use crate::groups::Group;
use crate::members::{Invite, Member, MemberState};
use crate::ops::{Op, PolicyScope};
use crate::roles::needs_scope;
use crate::state::{
    Delegation, EntitlementFacts, PendingOpen, TeamState, MAX_DELEGATIONS, MAX_PENDING_ESCROW_OPENS,
};

/// The fields of an entry that the chain and the state read: its place in the chain and its hash.
///
/// The entry's own `at` is informational (the authority table v0: "`at` is informational") and is
/// not copied here: every decision uses the leader's stamp.
///
/// Implements planning/architecture.md section 5.9 (the chain fields `v`, `account`, `seq`, `prev`).
///
/// Caller: `Admitted`, `Link`, `admit.rs` (made from a `RawEntry`).
// Made by `admit.rs` (lane S17c2) and by the tests; the allow goes when `admit.rs` lands.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct EntryMeta {
    /// The account the entry belongs to.
    pub(crate) account: AccountId,
    /// The position the entry claims.
    pub(crate) seq: ChainSeq,
    /// The hash of the entry before it.
    pub(crate) prev: Hash32,
    /// The hash of the entry.
    pub(crate) hash: Hash32,
    /// The restore epoch the entry was written in.
    pub(crate) epoch: RestoreEpoch,
}

/// Who acted, as `admit` resolved it.
///
/// Implements planning/architecture.md section 5.9 (`signer`: a device, a delegate key or the payments
/// key).
///
/// Caller: `Admitted`, `admit.rs`.
// Made by `admit.rs` (lane S17c2) and by the tests; the allow goes when `admit.rs` lands.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Actor {
    /// A device of a member (for a `genesis` and a `member_join` the device is the new one).
    Device {
        /// The device that signed.
        device: DeviceId,
        /// Its member.
        member: MemberId,
    },
    /// A delegate key.
    Delegate(KeyId),
    /// The payments service's key.
    Payments,
}

impl Actor {
    /// The member behind the signer, when a device signed.
    pub(crate) fn member(&self) -> Option<MemberId> {
        match self {
            Actor::Device { member, .. } => Some(*member),
            Actor::Delegate(_) | Actor::Payments => None,
        }
    }
}

/// The chain machinery's view of an entry: the leader's stamp stands in for `at` (planning/
/// architecture.md section 2.4 "Time is the leader's stamp"), so a device whose clock runs behind
/// another's does not make the chain's stamp rule refuse an entry. The signatures were checked by
/// `verify_signatures`, so the copy is intact.
///
/// Implements planning/architecture.md section 5.9 (hash-chained) and section 2.4.
///
/// Caller: `admit.rs` (the chain check), `apply` (the tip moves with it).
pub(crate) struct Link<'a> {
    /// The entry's chain fields.
    pub(crate) meta: &'a EntryMeta,
    /// The leader's stamp.
    pub(crate) stamp: Stamp,
}

impl ChainEntry for Link<'_> {
    fn chain(&self) -> ChainId {
        ChainId::team_log(self.meta.account)
    }

    fn seq(&self) -> ChainSeq {
        self.meta.seq
    }

    fn prev(&self) -> Hash32 {
        self.meta.prev
    }

    fn hash(&self) -> Hash32 {
        self.meta.hash
    }

    fn at(&self) -> Stamp {
        self.stamp
    }

    fn epoch(&self) -> RestoreEpoch {
        self.meta.epoch
    }

    fn is_intact(&self) -> bool {
        true
    }
}

/// An entry the permission check admitted: the witness `apply` takes. Only `admit` makes one (the
/// constructor is crate-private; there is no `Clone`, `Default` or `From`), and it records the entry's
/// place in the chain, the leader's stamp it was admitted at, who acted, the members whose devices
/// cosigned and were counted, and the op itself.
///
/// Implements planning/architecture.md section 5.9 ("The server enforces only what was signed") and
/// the brief's `Admitted` witness.
///
/// Caller: `admit::admit` (makes it), `apply` (consumes it), `replay::replay`.
#[derive(Debug)]
pub struct Admitted {
    meta: EntryMeta,
    stamp: Stamp,
    actor: Actor,
    cosigners: Vec<MemberId>,
    op: Op,
    sealed: Option<Vec<u8>>,
}

impl Admitted {
    /// Makes the witness. Only `admit` (and the tests of this crate) call it.
    // Called by `admit.rs` (lane S17c2) and by the tests; the allow goes when `admit.rs` lands.
    #[allow(dead_code)]
    pub(crate) fn new(
        meta: EntryMeta,
        stamp: Stamp,
        actor: Actor,
        cosigners: Vec<MemberId>,
        op: Op,
        sealed: Option<Vec<u8>>,
    ) -> Admitted {
        Admitted {
            meta,
            stamp,
            actor,
            cosigners,
            op,
            sealed,
        }
    }

    /// The op of the entry.
    pub fn op(&self) -> &Op {
        &self.op
    }

    /// The position of the entry.
    pub fn seq(&self) -> ChainSeq {
        self.meta.seq
    }

    /// The hash of the entry.
    pub fn hash(&self) -> Hash32 {
        self.meta.hash
    }

    /// The leader's stamp the entry was admitted at.
    pub fn stamp(&self) -> Stamp {
        self.stamp
    }
}

/// What one op changed.
struct Out {
    registry: Vec<RegistryDelta>,
    credentials_invalidated: bool,
}

impl Out {
    /// A change that invalidates no credential.
    fn rows(registry: Vec<RegistryDelta>) -> Option<Out> {
        Some(Out {
            registry,
            credentials_invalidated: false,
        })
    }
}

/// Applies an admitted entry to the state, once, in order.
///
/// The entry must be the next one of the state's chain (checked again here: an `Admitted` made
/// against another state is not applied). On success the state holds the op's change and the chain's
/// head, stamp and restore epoch have moved to the entry; the effects list the registry changes (in
/// the order to apply them) and the `log_appended` notice. Otherwise the state is untouched and the
/// effects are empty with `applied == false`.
///
/// Implements planning/architecture.md section 5.9 ("the metadata group applies each entry once, as
/// it arrives, into the registry's tables"; "Notices come from the metadata group"), section 5.11 (a
/// policy change invalidates older credentials) and section 12 E1 (the pure function the metadata
/// group and every device run).
///
/// Caller: the registry glue (TEAMLOG-APPLY, inside `MetaDb::apply`), `replay::replay`, the tests.
pub fn apply(state: &mut TeamState, a: Admitted) -> Effects {
    let link = Link {
        meta: &a.meta,
        stamp: a.stamp,
    };
    // The chain moves with the entry, or the entry is not applied. The new tip is computed first so
    // that nothing can fail after the op changed the state.
    let mut next_tip = state.tip.clone();
    match next_tip.advance(&link) {
        Ok(ChainAdmitted::Appended { .. }) => {}
        Ok(ChainAdmitted::AlreadyHeld { .. }) | Err(_) => return Effects::not_applied(),
    }
    // The first entry of a log is its genesis, and a genesis is nothing but the first entry.
    if state.is_empty() != matches!(a.op, Op::Genesis { .. }) {
        return Effects::not_applied();
    }
    match apply_op(state, &a) {
        Some(out) => {
            state.tip = next_tip;
            let (subject, audience) = subject_of(&a.op);
            Effects {
                registry: out.registry,
                notices: vec![Notice {
                    op: a.op.name(),
                    subject,
                    audience,
                }],
                credentials_invalidated: out.credentials_invalidated,
                applied: true,
            }
        }
        None => Effects::not_applied(),
    }
}

/// The change of one op. `None`: the op does not fit the state (nothing was changed).
fn apply_op(state: &mut TeamState, a: &Admitted) -> Option<Out> {
    let seq = a.meta.seq;
    let stamp = a.stamp;
    match &a.op {
        Op::Genesis {
            owner,
            device,
            member_keys,
            device_keys,
        } => {
            // planning/architecture.md section 5.9: `genesis`, the first owner device; seq 0, prev
            // zero, account empty (`apply` checked it); pins `genesis_hash`.
            state.install_genesis(
                *owner,
                *device,
                member_keys,
                device_keys,
                a.meta.hash,
                stamp,
            );
            let member = state.members.get(owner)?.clone();
            let device_row = state.devices.get(device)?.clone();
            Out::rows(vec![
                RegistryDelta::AccountGenesis {
                    kind: state.kind,
                    genesis_hash: a.meta.hash,
                },
                RegistryDelta::AccountPolicy {
                    policy: state.policy.clone(),
                    epoch: state.policy_epoch,
                },
                RegistryDelta::Member(member),
                RegistryDelta::Device(device_row),
            ])
        }
        Op::DeviceAdd {
            member,
            device,
            keys,
        } => {
            // `device_add`: the member's own device approves at once; admins approve by quorum and the
            // new device starts Pending until `hold_until` (policy `enroll_hold`).
            if !state.members.contains_key(member) || state.devices.contains_key(device) {
                return None;
            }
            let own_device = a.actor.member() == Some(*member);
            let (device_state, enrolled_by) = if own_device {
                (DeviceState::Active, EnrolledBy::OwnDevice)
            } else {
                let hold_until = stamp.checked_add(state.policy.enroll_hold)?;
                (DeviceState::Pending { hold_until }, EnrolledBy::Quorum)
            };
            let row = Device {
                id: *device,
                member: *member,
                keys: keys.clone(),
                state: device_state,
                enrolled_by,
                label_sealed: a.sealed.clone(),
                added_at: stamp,
                revoked_at: None,
                log_seq: seq,
            };
            state.devices.insert(*device, row.clone());
            Out::rows(vec![RegistryDelta::Device(row)])
        }
        Op::DeviceRemove { device } => {
            // `device_remove`: revokes the device (a revoked device signs nothing, for ever).
            let found = state.devices.get_mut(device)?;
            if found.state == DeviceState::Revoked {
                return None;
            }
            found.state = DeviceState::Revoked;
            found.revoked_at = Some(stamp);
            found.log_seq = seq;
            let row = found.clone();
            Out::rows(vec![
                RegistryDelta::Device(row),
                RegistryDelta::Revocation {
                    subject: RevocationSubject::Device(*device),
                    at: stamp,
                    reason: RevocationReason::DeviceRemoved,
                },
            ])
        }
        Op::MemberInvite {
            member,
            kind,
            role,
            invite_hash,
            groups,
            app,
        } => {
            // `member_invite`: creates an Invited member (the secret's hash is in the entry; the
            // pseudonymous id is the inviter's choice).
            if state.members.contains_key(member) {
                return None;
            }
            if groups.iter().any(|group| !state.groups.contains(group)) {
                return None;
            }
            let managed = *kind == MemberKind::Person && state.policy.managed_default.is_managed();
            let row = Member {
                id: *member,
                kind: *kind,
                role: *role,
                admin_scope: None,
                managed,
                state: MemberState::Invited,
                keys: None,
                invite: Some(Invite {
                    hash: *invite_hash,
                    inviter: a.actor.member(),
                }),
                app: app.clone(),
                added_at: stamp,
                removed_at: None,
                log_seq: seq,
            };
            state.members.insert(*member, row.clone());
            let mut registry = vec![RegistryDelta::Member(row)];
            for group in groups {
                state.groups.add_member(group, *member);
                registry.push(RegistryDelta::GroupMember {
                    group: *group,
                    member: *member,
                    present: true,
                });
            }
            Out::rows(registry)
        }
        Op::MemberJoin {
            member,
            device,
            member_keys,
            device_keys,
            ..
        } => {
            // `member_join`: the invitee's device joins; the member becomes Active and the invitation
            // is used up (planning/architecture.md section 5.11).
            let found = state.members.get(member)?;
            if found.state != MemberState::Invited
                || found.invite.is_none()
                || state.devices.contains_key(device)
            {
                return None;
            }
            let found = state.members.get_mut(member)?;
            found.state = MemberState::Active;
            found.keys = Some(member_keys.clone());
            found.invite = None;
            found.log_seq = seq;
            let member_row = found.clone();
            let device_row = Device {
                id: *device,
                member: *member,
                keys: device_keys.clone(),
                state: DeviceState::Active,
                enrolled_by: EnrolledBy::Invite,
                label_sealed: a.sealed.clone(),
                added_at: stamp,
                revoked_at: None,
                log_seq: seq,
            };
            state.devices.insert(*device, device_row.clone());
            Out::rows(vec![
                RegistryDelta::Member(member_row),
                RegistryDelta::Device(device_row),
            ])
        }
        Op::MemberRole {
            member,
            role,
            admin_scope,
        } => {
            // `member_role`: the role, and the scope group of a group admin or a helpdesk member. A
            // removed member has no role to change.
            let found = state.members.get_mut(member)?;
            if found.state == MemberState::Removed {
                return None;
            }
            found.role = *role;
            found.admin_scope = if needs_scope(*role) {
                *admin_scope
            } else {
                None
            };
            found.log_seq = seq;
            Out::rows(vec![RegistryDelta::Member(found.clone())])
        }
        Op::MemberSuspend { member } => {
            // `member_suspend`: only an Active member can be suspended.
            let found = state.members.get_mut(member)?;
            if found.state != MemberState::Active {
                return None;
            }
            found.state = MemberState::Suspended;
            found.log_seq = seq;
            Out::rows(vec![RegistryDelta::Member(found.clone())])
        }
        Op::MemberRemove { member } => member_remove(state, member, stamp, seq),
        Op::MemberManaged { member, managed } => {
            // `member_managed`: a removed member has nothing to manage.
            let found = state.members.get_mut(member)?;
            if found.state == MemberState::Removed {
                return None;
            }
            found.managed = *managed;
            found.log_seq = seq;
            Out::rows(vec![RegistryDelta::Member(found.clone())])
        }
        Op::GroupCreate { group, parent } => {
            // `group_create`: the name is the entry's `sealed` field.
            if state.groups.contains(group) {
                return None;
            }
            if let Some(parent) = parent {
                if !state.groups.contains(parent) {
                    return None;
                }
            }
            let row = Group {
                id: *group,
                parent: *parent,
                name_sealed: a.sealed.clone(),
                log_seq: seq,
            };
            state.groups.insert(row.clone());
            Out::rows(vec![RegistryDelta::Group(row)])
        }
        Op::GroupMember {
            group,
            member,
            present,
        } => {
            // `group_member`: a removed member joins no group.
            let found = state.members.get(member)?;
            if !state.groups.contains(group) || (*present && found.state == MemberState::Removed) {
                return None;
            }
            if *present {
                state.groups.add_member(group, *member);
            } else {
                state.groups.remove_member(group, member);
            }
            Out::rows(vec![RegistryDelta::GroupMember {
                group: *group,
                member: *member,
                present: *present,
            }])
        }
        Op::GroupMove { group, new_parent } => {
            // `group_move`: the group never becomes its own ancestor, and a top-level group never moves.
            let found = state.groups.get(group)?;
            if found.parent.is_none()
                || !state.groups.contains(new_parent)
                || state.groups.is_in_subtree(new_parent, group)
            {
                return None;
            }
            state.groups.set_parent(group, Some(*new_parent), seq);
            let row = state.groups.get(group)?.clone();
            Out::rows(vec![RegistryDelta::Group(row)])
        }
        Op::PolicySet { scope, patch } => policy_set(state, scope, patch),
        Op::EscrowSet {
            escrow_pub,
            holders,
        } => {
            // `escrow_set`: the escrow public key and the holder set.
            state.escrow_pub = Some(*escrow_pub);
            state.escrow_holders = holders.clone();
            Out::rows(vec![RegistryDelta::AccountEscrow {
                escrow_pub: *escrow_pub,
                holders: holders.iter().copied().collect(),
            }])
        }
        Op::AuditKeySet { audit_pub } => {
            // `audit_key_set`.
            state.audit_pub = Some(*audit_pub);
            Out::rows(vec![RegistryDelta::AccountAudit {
                audit_pub: *audit_pub,
            }])
        }
        Op::AdminQuorumSet { quorum } => {
            // `admin_quorum_set`: a change of the account's policy, so the epoch moves with it.
            let mut after = state.policy.clone();
            after.admin_quorum = *quorum;
            let epoch = match next_epoch(state.policy_epoch, &state.policy, &after) {
                Ok(epoch) => epoch,
                Err(_) => return None,
            };
            let moved = epoch != state.policy_epoch;
            state.policy = after;
            state.policy_epoch = epoch;
            Some(Out {
                registry: vec![RegistryDelta::AccountPolicy {
                    policy: state.policy.clone(),
                    epoch,
                }],
                credentials_invalidated: moved,
            })
        }
        Op::EscrowOpen { member, reason } => {
            // `escrow_open`: records when the wait ends (the wait itself is lane S17f's): the policy's
            // `escrow_open_wait` for a member who is still a member, none for one already removed.
            let found = state.members.get(member)?;
            let ready_at = match found.state {
                MemberState::Removed => None,
                MemberState::Active | MemberState::Suspended => {
                    Some(stamp.checked_add(state.policy.escrow_open_wait)?)
                }
                MemberState::Invited | MemberState::Pending => return None,
            };
            if state.pending_opens.len() >= MAX_PENDING_ESCROW_OPENS {
                return None;
            }
            let mut signers = a.cosigners.clone();
            if let Some(signer) = a.actor.member() {
                signers.push(signer);
            }
            signers.sort();
            signers.dedup();
            let open = PendingOpen {
                seq,
                member: *member,
                reason: *reason,
                requested_at: stamp,
                ready_at,
                signers,
            };
            state.pending_opens.insert(seq, open.clone());
            Out::rows(vec![RegistryDelta::EscrowOpen(open)])
        }
        Op::Delegate { key, ops, expires } => {
            // `delegate`: names a key and the exact ops; an empty set revokes.
            let granted_by = a.actor.member()?;
            if ops.is_empty() {
                state.delegations.remove(key)?;
                return Out::rows(vec![RegistryDelta::Delegation {
                    key: *key,
                    delegation: None,
                }]);
            }
            if !state.delegations.contains_key(key) && state.delegations.len() >= MAX_DELEGATIONS {
                return None;
            }
            let row = Delegation {
                key: *key,
                ops: ops.clone(),
                expires: *expires,
                granted_by,
                granted_at: stamp,
                log_seq: seq,
            };
            state.delegations.insert(*key, row.clone());
            Out::rows(vec![RegistryDelta::Delegation {
                key: *key,
                delegation: Some(row),
            }])
        }
        Op::Entitlement {
            version,
            state: entitlement_state,
            quota,
            seats,
        } => {
            // `entitlement`: the payments service's submission; the higher version wins, and the entry
            // changes no member and no role.
            if let Some(current) = &state.entitlement {
                if *version <= current.version {
                    return None;
                }
            }
            let facts = EntitlementFacts {
                version: *version,
                state: *entitlement_state,
                quota: *quota,
                seats: *seats,
                applied_at: stamp,
            };
            state.entitlement = Some(facts.clone());
            Out::rows(vec![RegistryDelta::AccountEntitlement(facts)])
        }
        // `app:<name>/...`: stored and served, never interpreted: no row changes.
        Op::App { .. } => Out::rows(Vec::new()),
    }
}

/// `member_remove`: the member is removed, every device of it is revoked, it leaves every group, and
/// its open invitation goes. The rows come out in this order: the member, each device (in the order of
/// the ids), each group membership that ended, the revocation of the member.
fn member_remove(
    state: &mut TeamState,
    member: &MemberId,
    stamp: Stamp,
    seq: ChainSeq,
) -> Option<Out> {
    let found = state.members.get_mut(member)?;
    if found.state == MemberState::Removed {
        return None;
    }
    found.state = MemberState::Removed;
    found.removed_at = Some(stamp);
    found.invite = None;
    found.log_seq = seq;
    let mut registry = vec![RegistryDelta::Member(found.clone())];
    for device in state.devices.values_mut() {
        if device.member == *member && device.state != DeviceState::Revoked {
            device.state = DeviceState::Revoked;
            device.revoked_at = Some(stamp);
            device.log_seq = seq;
            registry.push(RegistryDelta::Device(device.clone()));
        }
    }
    for group in state.groups.remove_all_memberships(member) {
        registry.push(RegistryDelta::GroupMember {
            group,
            member: *member,
            present: false,
        });
    }
    registry.push(RegistryDelta::Revocation {
        subject: RevocationSubject::Member(*member),
        at: stamp,
        reason: RevocationReason::MemberRemoved,
    });
    Out::rows(registry)
}

/// `policy_set`: a patch of the account's policy or of one group's. The policy epoch moves exactly
/// when the effective policy of the subject changed (`azlin_policy::next_epoch`), and then every
/// credential of an older epoch is invalid.
fn policy_set(state: &mut TeamState, scope: &PolicyScope, patch: &PolicyPatch) -> Option<Out> {
    match scope {
        PolicyScope::Account => {
            let after = apply_patch(&state.policy, patch);
            let epoch = match next_epoch(state.policy_epoch, &state.policy, &after) {
                Ok(epoch) => epoch,
                Err(_) => return None,
            };
            let moved = epoch != state.policy_epoch;
            state.policy = after;
            state.policy_epoch = epoch;
            Some(Out {
                registry: vec![RegistryDelta::AccountPolicy {
                    policy: state.policy.clone(),
                    epoch,
                }],
                credentials_invalidated: moved,
            })
        }
        PolicyScope::Group(group) => {
            let before = state.effective_policy_of(group)?;
            let after = {
                let path = state.groups.path_from_top(group)?;
                let patches: Vec<&PolicyPatch> = path
                    .iter()
                    .filter_map(|id| {
                        if id == group {
                            if patch.is_empty() {
                                None
                            } else {
                                Some(patch)
                            }
                        } else {
                            state.group_patches.get(id)
                        }
                    })
                    .collect();
                effective(&state.policy, &patches)
            };
            let epoch = match next_epoch(state.policy_epoch, &before, &after) {
                Ok(epoch) => epoch,
                Err(_) => return None,
            };
            let moved = epoch != state.policy_epoch;
            let stored = if patch.is_empty() {
                state.group_patches.remove(group);
                None
            } else {
                state.group_patches.insert(*group, patch.clone());
                Some(patch.clone())
            };
            state.policy_epoch = epoch;
            let mut registry = vec![RegistryDelta::GroupPolicy {
                group: *group,
                patch: stored,
            }];
            if moved {
                registry.push(RegistryDelta::PolicyEpoch(epoch));
            }
            Some(Out {
                registry,
                credentials_invalidated: moved,
            })
        }
    }
}
