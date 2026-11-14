//! Tests of the state hash (planning/architecture.md section 5.9 "a hash of the registry state"): it is
//! independent of the order the facts were inserted in, and it changes with EVERY field of the state
//! (one case per field; a field that is added to a structure of the state and is not written into the
//! hash makes `every_field_of_every_structure_is_named_here` fail to compile; lane W1-S17c1).

use azlin_chain::{ChainId, Head, Tip};
use azlin_policy::{
    apply as apply_patch, AdminQuorum, AudienceSet, AuditLevel, DeviceApproval, ExternalSharing,
    Field, LinkPolicyPatch, ManagedDefault, PaddingPolicy, PolicyPatch, RequestPolicyPatch,
    UpdatePolicy,
};
use azlin_seams::{
    AccountId, AccountKind, ByteCount, ChainSeq, Epoch, PolicyEpoch, RestoreEpoch, Span,
};

use super::kit::{account, at, by, device, group, hash, keys, member, rich_business};
use crate::apply::{apply, Admitted, EntryMeta};
use crate::devices::{Device, DeviceState, EnrolledBy, PublicKeys};
use crate::groups::Group;
use crate::hash::{entry_hash, ENTRY_HASH_DOMAIN, STATE_HASH_DOMAIN};
use crate::members::{AppSpec, Invite, Member, MemberState};
use crate::ops::{EntitlementState, EntitlementVersion, EscrowReason, Op, OpName};
use crate::roles::Role;
use crate::state::{Delegation, EntitlementFacts, PendingOpen, TeamState};

fn base() -> TeamState {
    rich_business().state
}

#[test]
fn state_hash_independent_of_insertion_order() {
    let rich = base();
    // Two replicas that learned the same facts in opposite orders.
    let mut forward = TeamState::empty(account(), AccountKind::Business);
    let mut backward = TeamState::empty(account(), AccountKind::Business);
    for state in [&mut forward, &mut backward] {
        state.genesis_hash = rich.genesis_hash;
        state.tip = rich.tip.clone();
        state.policy = rich.policy.clone();
        state.policy_epoch = rich.policy_epoch;
        state.escrow_pub = rich.escrow_pub;
        state.escrow_holders = rich.escrow_holders.clone();
        state.audit_pub = rich.audit_pub;
        state.seal_epoch = rich.seal_epoch;
        state.entitlement = rich.entitlement.clone();
    }
    let members: Vec<&Member> = rich.members.values().collect();
    let devices: Vec<&Device> = rich.devices.values().collect();
    let groups: Vec<&Group> = rich.groups.iter().collect();
    let pairs = rich.groups.membership_pairs();
    let patches: Vec<_> = rich.group_patches.iter().collect();
    let delegations: Vec<&Delegation> = rich.delegations.values().collect();
    let opens: Vec<&PendingOpen> = rich.pending_opens.values().collect();
    for value in &members {
        forward.members.insert(value.id, (*value).clone());
    }
    for value in members.iter().rev() {
        backward.members.insert(value.id, (*value).clone());
    }
    for value in &devices {
        forward.devices.insert(value.id, (*value).clone());
    }
    for value in devices.iter().rev() {
        backward.devices.insert(value.id, (*value).clone());
    }
    for value in &groups {
        assert!(forward.groups.insert((*value).clone()));
    }
    for value in groups.iter().rev() {
        assert!(backward.groups.insert((*value).clone()));
    }
    for (group_id, member_id) in &pairs {
        assert!(forward.groups.add_member(group_id, *member_id));
    }
    for (group_id, member_id) in pairs.iter().rev() {
        assert!(backward.groups.add_member(group_id, *member_id));
    }
    for (group_id, patch) in &patches {
        forward.group_patches.insert(**group_id, (*patch).clone());
    }
    for (group_id, patch) in patches.iter().rev() {
        backward.group_patches.insert(**group_id, (*patch).clone());
    }
    for value in &delegations {
        forward.delegations.insert(value.key, (*value).clone());
    }
    for value in delegations.iter().rev() {
        backward.delegations.insert(value.key, (*value).clone());
    }
    for value in &opens {
        forward.pending_opens.insert(value.seq, (*value).clone());
    }
    for value in opens.iter().rev() {
        backward.pending_opens.insert(value.seq, (*value).clone());
    }
    assert_eq!(forward.canonical_bytes(), backward.canonical_bytes());
    assert_eq!(forward.state_hash(), backward.state_hash());
    // And both are the state they were copied from.
    assert_eq!(forward, rich);
    assert_eq!(backward, rich);
    assert_eq!(forward.state_hash(), rich.state_hash());
}

#[test]
fn state_hash_is_a_hash_of_the_canonical_bytes_under_its_own_domain() {
    let state = base();
    let bytes = state.canonical_bytes();
    assert_eq!(
        state.state_hash(),
        azlin_seams::hash32(STATE_HASH_DOMAIN, &[&bytes])
    );
    assert_ne!(
        state.state_hash(),
        azlin_seams::hash32(ENTRY_HASH_DOMAIN, &[&bytes]),
        "two domains, two functions"
    );
    assert_eq!(STATE_HASH_DOMAIN, "azlin-teamlog-state-v1");
    assert_ne!(STATE_HASH_DOMAIN, ENTRY_HASH_DOMAIN);
    // The same state hashes the same, every time.
    assert_eq!(state.state_hash(), state.clone().state_hash());
}

#[test]
fn entry_hash_is_the_hash_of_the_preimage_under_its_domain() {
    assert_eq!(
        entry_hash(b"preimage"),
        azlin_seams::hash32(ENTRY_HASH_DOMAIN, &[b"preimage".as_slice()])
    );
    assert_ne!(entry_hash(b"preimage"), entry_hash(b"preimagf"));
    assert_ne!(entry_hash(b""), entry_hash(b"\0"));
    assert_eq!(ENTRY_HASH_DOMAIN, "azlin-teamlog-entry-v1");
}

#[test]
fn genesis_state_pins_genesis_hash() {
    let state = TeamState::genesis_state(
        account(),
        AccountKind::Team,
        member(1),
        device(1),
        &keys(1),
        &keys(101),
        hash(7),
        at(500),
        RestoreEpoch::new(2),
    );
    assert_eq!(state.genesis_hash(), Some(hash(7)));
    assert_eq!(
        state.head(),
        Some(Head {
            chain: ChainId::team_log(account()),
            seq: ChainSeq::MIN,
            hash: hash(7),
        })
    );
    assert_eq!(state.tip().last_at(), Some(at(500)));
    assert_eq!(state.tip().epoch(), RestoreEpoch::new(2));
    assert!(!state.is_empty());
    let owner = state.member(&member(1)).expect("the first owner");
    assert_eq!(owner.role, Role::Owner);
    assert_eq!(owner.state, MemberState::Active);
    assert_eq!(owner.keys, Some(keys(1)));
    assert_eq!(owner.log_seq, ChainSeq::MIN);
    let first = state.device(&device(1)).expect("the first device");
    assert_eq!(first.member, member(1));
    assert_eq!(first.state, DeviceState::Active);
    assert_eq!(first.enrolled_by, EnrolledBy::Genesis);
    assert_eq!(first.keys, keys(101));
    // `apply` of the genesis entry makes the very same state.
    let mut via_apply = TeamState::empty(account(), AccountKind::Team);
    let meta = EntryMeta {
        account: account(),
        seq: ChainSeq::MIN,
        prev: azlin_seams::Hash32::ZERO,
        hash: hash(7),
        epoch: RestoreEpoch::new(2),
    };
    let effects = apply(
        &mut via_apply,
        Admitted::new(
            meta,
            at(500),
            by(1, 1),
            Vec::new(),
            Op::Genesis {
                owner: member(1),
                device: device(1),
                member_keys: keys(1),
                device_keys: keys(101),
            },
            None,
        ),
    );
    assert!(effects.applied);
    assert_eq!(via_apply, state);
    assert_eq!(via_apply.state_hash(), state.state_hash());
    // The genesis hash is part of the hash of the state: another genesis, another state.
    let other = TeamState::genesis_state(
        account(),
        AccountKind::Team,
        member(1),
        device(1),
        &keys(1),
        &keys(101),
        hash(8),
        at(500),
        RestoreEpoch::new(2),
    );
    assert_ne!(other.state_hash(), state.state_hash());
}

/// The patch that sets exactly one leaf of the policy to a value the business default does not have.
/// The `match` names every `Field`: a leaf added to the policy does not compile until it has a case.
fn patch_for(field: Field) -> PolicyPatch {
    let none = PolicyPatch::none();
    let link = LinkPolicyPatch::default();
    let request = RequestPolicyPatch::default();
    match field {
        Field::CredentialTtl => PolicyPatch {
            credential_ttl: Some(Span::minutes(5)),
            ..none
        },
        Field::DeviceApproval => PolicyPatch {
            device_approval: Some(DeviceApproval::SelfOnly),
            ..none
        },
        Field::MaxDevicesPerMember => PolicyPatch {
            max_devices_per_member: Some(3),
            ..none
        },
        Field::ManagedDefault => PolicyPatch {
            managed_default: Some(ManagedDefault::No),
            ..none
        },
        Field::SsoRequired => PolicyPatch {
            sso_required: Some(true),
            ..none
        },
        Field::SsoMaxAge => PolicyPatch {
            sso_max_age: Some(Some(Span::hours(1))),
            ..none
        },
        Field::EnrollHold => PolicyPatch {
            enroll_hold: Some(Span::hours(24)),
            ..none
        },
        Field::PasskeyRecovery => PolicyPatch {
            passkey_recovery: Some(true),
            ..none
        },
        Field::EscrowOpenWait => PolicyPatch {
            escrow_open_wait: Some(Span::hours(1)),
            ..none
        },
        Field::AuditLevel => PolicyPatch {
            audit_level: Some(AuditLevel::AccessFull),
            ..none
        },
        Field::AdminQuorum => PolicyPatch {
            admin_quorum: Some(AdminQuorum::new(3).expect("a quorum of three")),
            ..none
        },
        Field::LinkAudiences => PolicyPatch {
            link_policy: LinkPolicyPatch {
                audiences: Some(AudienceSet::NONE),
                ..link
            },
            ..none
        },
        Field::LinkMaxExpiry => PolicyPatch {
            link_policy: LinkPolicyPatch {
                max_expiry: Some(Some(Span::days(1))),
                ..link
            },
            ..none
        },
        Field::LinkPasswordForAnyone => PolicyPatch {
            link_policy: LinkPolicyPatch {
                password_for_anyone: Some(true),
                ..link
            },
            ..none
        },
        Field::LinkForcePreviewOnly => PolicyPatch {
            link_policy: LinkPolicyPatch {
                force_preview_only: Some(true),
                ..link
            },
            ..none
        },
        Field::LinkForceWatermark => PolicyPatch {
            link_policy: LinkPolicyPatch {
                force_watermark: Some(true),
                ..link
            },
            ..none
        },
        Field::LinkForceAppOnly => PolicyPatch {
            link_policy: LinkPolicyPatch {
                force_app_only: Some(true),
                ..link
            },
            ..none
        },
        Field::LinkUploads => PolicyPatch {
            link_policy: LinkPolicyPatch {
                uploads: Some(false),
                ..link
            },
            ..none
        },
        Field::LinkEdit => PolicyPatch {
            link_policy: LinkPolicyPatch {
                edit: Some(false),
                ..link
            },
            ..none
        },
        Field::LinkComments => PolicyPatch {
            link_policy: LinkPolicyPatch {
                comments: Some(false),
                ..link
            },
            ..none
        },
        Field::LinkOpenLogs => PolicyPatch {
            link_policy: LinkPolicyPatch {
                open_logs: Some(false),
                ..link
            },
            ..none
        },
        Field::LinkSlugs => PolicyPatch {
            link_policy: LinkPolicyPatch {
                slugs: Some(false),
                ..link
            },
            ..none
        },
        Field::RequestAllowed => PolicyPatch {
            request_policy: RequestPolicyPatch {
                allowed: Some(false),
                ..request
            },
            ..none
        },
        Field::RequestMaxExpiry => PolicyPatch {
            request_policy: RequestPolicyPatch {
                max_expiry: Some(Some(Span::days(2))),
                ..request
            },
            ..none
        },
        Field::RequestMaxFiles => PolicyPatch {
            request_policy: RequestPolicyPatch {
                max_files: Some(5),
                ..request
            },
            ..none
        },
        Field::RequestMaxBytes => PolicyPatch {
            request_policy: RequestPolicyPatch {
                max_bytes: Some(ByteCount::kib(1)),
                ..request
            },
            ..none
        },
        Field::BrowserDevices => PolicyPatch {
            browser_devices: Some(false),
            ..none
        },
        Field::ExternalSharing => PolicyPatch {
            external_sharing: Some(ExternalSharing::Allowed),
            ..none
        },
        Field::Updates => PolicyPatch {
            updates: Some(UpdatePolicy::Auto),
            ..none
        },
        Field::Padding => PolicyPatch {
            padding: Some(PaddingPolicy::Full),
            ..none
        },
    }
}

#[test]
fn state_hash_changes_with_every_policy_field() {
    let state = base();
    let before = state.state_hash();
    assert_eq!(Field::ALL.len(), 30, "the policy has 30 leaves");
    for field in Field::ALL {
        let patch = patch_for(*field);
        assert_eq!(
            patch.sets(),
            vec![*field],
            "{field}: the patch sets that leaf only"
        );
        let mut changed = state.clone();
        changed.policy = apply_patch(&state.policy, &patch);
        assert_ne!(
            changed.policy, state.policy,
            "{field}: the business default differs"
        );
        assert_ne!(changed.state_hash(), before, "the account policy: {field}");
        // The same leaf in the patch of a group.
        let mut patched = state.clone();
        patched.group_patches.insert(group(13), patch);
        assert_ne!(patched.state_hash(), before, "a group patch: {field}");
        let mut replaced = state.clone();
        replaced.group_patches.insert(group(12), patch_for(*field));
        assert_ne!(
            replaced.state_hash(),
            before,
            "the patch of group 12: {field}"
        );
    }
}

/// A change to one field of the state: its name and what it does to a copy of the base state.
type Case = (&'static str, fn(&mut TeamState));

fn member_mut(state: &mut TeamState, n: u8) -> &mut Member {
    state
        .members
        .get_mut(&member(n))
        .expect("a member of the base state")
}

fn device_mut(state: &mut TeamState, n: u8) -> &mut Device {
    state
        .devices
        .get_mut(&device(n))
        .expect("a device of the base state")
}

fn group_mut(state: &mut TeamState, n: u8) -> &mut Group {
    state
        .groups
        .get_mut(&group(n))
        .expect("a group of the base state")
}

/// A case: the closure is coerced to a function pointer here, where its argument type is known.
fn case(name: &'static str, change: fn(&mut TeamState)) -> Case {
    (name, change)
}

fn cases() -> Vec<Case> {
    vec![
        // ---- TeamState --------------------------------------------------------------------
        case("account", |s| s.account = AccountId::from_bytes([0xBB; 16])),
        case("kind", |s| s.kind = AccountKind::Team),
        case("genesis_hash", |s| s.genesis_hash = Some(hash(0x77))),
        case("genesis_hash none", |s| s.genesis_hash = None),
        case("tip head hash", |s| {
            let head = s.tip.head().expect("a head");
            s.tip = Tip::resume(
                Head {
                    hash: hash(0x66),
                    ..head
                },
                s.tip.last_at().expect("a stamp"),
                s.tip.epoch(),
            );
        }),
        case("tip head seq", |s| {
            let head = s.tip.head().expect("a head");
            s.tip = Tip::resume(
                Head {
                    seq: ChainSeq::new(head.seq.get() + 1),
                    ..head
                },
                s.tip.last_at().expect("a stamp"),
                s.tip.epoch(),
            );
        }),
        case("tip last stamp", |s| {
            let stamp = s.tip.last_at().expect("a stamp");
            s.tip = Tip::resume(
                s.tip.head().expect("a head"),
                at(stamp.as_secs() + 1),
                s.tip.epoch(),
            );
        }),
        case("tip restore epoch", |s| {
            s.tip = Tip::resume(
                s.tip.head().expect("a head"),
                s.tip.last_at().expect("a stamp"),
                RestoreEpoch::new(5),
            );
        }),
        case("members: one more", |s| {
            let mut copy = member_mut(s, 2).clone();
            copy.id = member(50);
            s.members.insert(member(50), copy);
        }),
        case("members: one less", |s| {
            s.members.remove(&member(6));
        }),
        case("devices: one more", |s| {
            let mut copy = device_mut(s, 2).clone();
            copy.id = device(50);
            s.devices.insert(device(50), copy);
        }),
        case("devices: one less", |s| {
            s.devices.remove(&device(7));
        }),
        case("groups: one more", |s| {
            assert!(s.groups.insert(Group {
                id: group(50),
                parent: None,
                name_sealed: None,
                log_seq: ChainSeq::MIN,
            }));
        }),
        case("group members: one more", |s| {
            assert!(s.groups.add_member(&group(11), member(2)));
        }),
        case("group members: one less", |s| {
            assert!(s.groups.remove_member(&group(13), &member(4)));
        }),
        case("group patches: one more", |s| {
            s.group_patches.insert(group(13), patch_for(Field::Padding));
        }),
        case("group patches: one less", |s| {
            s.group_patches.remove(&group(12));
        }),
        case("policy_epoch", |s| s.policy_epoch = PolicyEpoch::new(99)),
        case("delegations: one more", |s| {
            let mut copy = s.delegations.values().next().expect("a delegation").clone();
            copy.key = super::kit::key(41);
            s.delegations.insert(copy.key, copy);
        }),
        case("delegations: one less", |s| s.delegations.clear()),
        case("escrow_pub", |s| s.escrow_pub = Some([1; 32])),
        case("escrow_pub none", |s| s.escrow_pub = None),
        case("escrow_holders: one more", |s| {
            s.escrow_holders.insert(member(3));
        }),
        case("escrow_holders: one less", |s| {
            s.escrow_holders.remove(&member(1));
        }),
        case("audit_pub", |s| s.audit_pub = Some([2; 32])),
        case("audit_pub none", |s| s.audit_pub = None),
        case("seal_epoch", |s| {
            s.seal_epoch = Epoch::new(2).expect("epoch 2")
        }),
        case("entitlement none", |s| s.entitlement = None),
        case("pending_opens: one less", |s| s.pending_opens.clear()),
        case("pending_opens: one more", |s| {
            let mut copy = s.pending_opens.values().next().expect("an opening").clone();
            copy.seq = ChainSeq::new(copy.seq.get() + 1);
            s.pending_opens.insert(copy.seq, copy);
        }),
        // ---- Member (m2: an admin; m4: invited; m9: an app) ------------------------------------
        case("member.kind", |s| {
            member_mut(s, 2).kind = azlin_seams::MemberKind::App
        }),
        case("member.role", |s| member_mut(s, 2).role = Role::Guest),
        case("member.admin_scope", |s| {
            member_mut(s, 2).admin_scope = Some(group(11))
        }),
        case("member.managed", |s| member_mut(s, 2).managed = true),
        case("member.state", |s| {
            member_mut(s, 2).state = MemberState::Suspended
        }),
        case("member.keys.sign", |s| {
            member_mut(s, 2).keys.as_mut().expect("keys").sign = [0xF0; 32];
        }),
        case("member.keys.kx", |s| {
            member_mut(s, 2).keys.as_mut().expect("keys").kx = [0xF1; 32];
        }),
        case("member.keys.pq", |s| {
            member_mut(s, 2).keys.as_mut().expect("keys").pq = Some(vec![1, 2, 3]);
        }),
        case("member.keys none", |s| member_mut(s, 2).keys = None),
        case("member.invite", |s| {
            member_mut(s, 2).invite = Some(Invite {
                hash: hash(1),
                inviter: None,
            });
        }),
        case("member.invite.hash", |s| {
            member_mut(s, 4).invite.as_mut().expect("an invite").hash = hash(0x44);
        }),
        case("member.invite.inviter", |s| {
            member_mut(s, 4).invite.as_mut().expect("an invite").inviter = None;
        }),
        case("member.app", |s| {
            member_mut(s, 2).app = Some(AppSpec {
                scopes: Default::default(),
                expires_at: None,
            });
        }),
        case("member.app none", |s| member_mut(s, 9).app = None),
        case("member.app.scopes", |s| {
            member_mut(s, 9)
                .app
                .as_mut()
                .expect("an app")
                .scopes
                .insert(crate::ops::AppName::new("mail").expect("valid"));
        }),
        case("member.app.expires_at", |s| {
            member_mut(s, 9).app.as_mut().expect("an app").expires_at = Some(at(1));
        }),
        case("member.app.expires_at none", |s| {
            member_mut(s, 9).app.as_mut().expect("an app").expires_at = None;
        }),
        case("member.added_at", |s| member_mut(s, 2).added_at = at(1)),
        case("member.removed_at", |s| {
            member_mut(s, 2).removed_at = Some(at(1))
        }),
        case("member.log_seq", |s| {
            member_mut(s, 2).log_seq = ChainSeq::new(900)
        }),
        // ---- Device (d8: pending, quorum, sealed label; d1: active) ----------------------------
        case("device.member", |s| device_mut(s, 8).member = member(2)),
        case("device.keys.sign", |s| {
            device_mut(s, 8).keys.sign = [0xF2; 32]
        }),
        case("device.keys.kx", |s| device_mut(s, 8).keys.kx = [0xF3; 32]),
        case("device.keys.pq", |s| {
            device_mut(s, 8).keys.pq = Some(vec![9])
        }),
        case("device.state hold_until", |s| {
            let until = match device_mut(s, 8).state {
                DeviceState::Pending { hold_until } => hold_until,
                other => panic!("pending expected, was {other:?}"),
            };
            device_mut(s, 8).state = DeviceState::Pending {
                hold_until: at(until.as_secs() + 1),
            };
        }),
        case("device.state active", |s| {
            device_mut(s, 8).state = DeviceState::Active
        }),
        case("device.state revoked", |s| {
            device_mut(s, 8).state = DeviceState::Revoked
        }),
        case("device.state revived", |s| {
            device_mut(s, 3).state = DeviceState::Active
        }),
        case("device.enrolled_by", |s| {
            device_mut(s, 8).enrolled_by = EnrolledBy::OwnDevice
        }),
        case("device.label_sealed none", |s| {
            device_mut(s, 8).label_sealed = None
        }),
        case("device.label_sealed", |s| {
            device_mut(s, 8).label_sealed = Some(vec![9, 8])
        }),
        case("device.label_sealed set", |s| {
            device_mut(s, 1).label_sealed = Some(vec![1])
        }),
        case("device.added_at", |s| device_mut(s, 8).added_at = at(1)),
        case("device.revoked_at", |s| {
            device_mut(s, 3).revoked_at = Some(at(1))
        }),
        case("device.revoked_at none", |s| {
            device_mut(s, 3).revoked_at = None
        }),
        case("device.log_seq", |s| {
            device_mut(s, 8).log_seq = ChainSeq::new(900)
        }),
        // ---- Group (g12: under g11, sealed name) ------------------------------------------------
        case("group.parent none", |s| group_mut(s, 12).parent = None),
        case("group.parent other", |s| {
            group_mut(s, 12).parent = Some(group(13))
        }),
        case("group.name_sealed none", |s| {
            group_mut(s, 12).name_sealed = None
        }),
        case("group.name_sealed", |s| {
            group_mut(s, 12).name_sealed = Some(vec![9])
        }),
        case("group.name_sealed set", |s| {
            group_mut(s, 11).name_sealed = Some(vec![9])
        }),
        case("group.log_seq", |s| {
            group_mut(s, 12).log_seq = ChainSeq::new(900)
        }),
        // ---- Delegation -------------------------------------------------------------------------
        case("delegation.ops", |s| {
            for value in s.delegations.values_mut() {
                value.ops.insert(OpName::MemberInvite);
            }
        }),
        case("delegation.expires none", |s| {
            for value in s.delegations.values_mut() {
                value.expires = None;
            }
        }),
        case("delegation.expires", |s| {
            for value in s.delegations.values_mut() {
                value.expires = Some(at(1));
            }
        }),
        case("delegation.granted_by", |s| {
            for value in s.delegations.values_mut() {
                value.granted_by = member(2);
            }
        }),
        case("delegation.granted_at", |s| {
            for value in s.delegations.values_mut() {
                value.granted_at = at(1);
            }
        }),
        case("delegation.log_seq", |s| {
            for value in s.delegations.values_mut() {
                value.log_seq = ChainSeq::new(900);
            }
        }),
        // ---- EntitlementFacts -------------------------------------------------------------------
        case("entitlement.version", |s| {
            s.entitlement.as_mut().expect("an entitlement").version = EntitlementVersion(4);
        }),
        case("entitlement.state", |s| {
            s.entitlement.as_mut().expect("an entitlement").state = EntitlementState::Lapsed;
        }),
        case("entitlement.quota", |s| {
            s.entitlement.as_mut().expect("an entitlement").quota = ByteCount::gib(101);
        }),
        case("entitlement.seats", |s| {
            s.entitlement.as_mut().expect("an entitlement").seats = 26;
        }),
        case("entitlement.applied_at", |s| {
            s.entitlement.as_mut().expect("an entitlement").applied_at = at(1);
        }),
        // ---- PendingOpen ------------------------------------------------------------------------
        case("pending_open.member", |s| {
            for value in s.pending_opens.values_mut() {
                value.member = member(3);
            }
        }),
        case("pending_open.reason", |s| {
            for value in s.pending_opens.values_mut() {
                value.reason = EscrowReason::Other;
            }
        }),
        case("pending_open.requested_at", |s| {
            for value in s.pending_opens.values_mut() {
                value.requested_at = at(1);
            }
        }),
        case("pending_open.ready_at none", |s| {
            for value in s.pending_opens.values_mut() {
                value.ready_at = None;
            }
        }),
        case("pending_open.ready_at", |s| {
            for value in s.pending_opens.values_mut() {
                value.ready_at = Some(at(1));
            }
        }),
        case("pending_open.signers", |s| {
            for value in s.pending_opens.values_mut() {
                value.signers.push(member(3));
            }
        }),
    ]
}

#[test]
fn state_hash_changes_with_every_registry_field() {
    let state = base();
    let before = state.state_hash();
    let all = cases();
    let mut names = std::collections::BTreeSet::new();
    for (name, change) in &all {
        assert!(names.insert(*name), "case names are unique: {name}");
        let mut changed = state.clone();
        change(&mut changed);
        assert_ne!(changed, state, "{name}: the case changes the state");
        assert_ne!(changed.state_hash(), before, "{name}: the hash must change");
    }
    // Every field of every structure has at least one case: see the count below.
    assert!(
        all.len() >= FIELD_COUNT,
        "{} cases for {FIELD_COUNT} fields",
        all.len()
    );
}

/// The number of fields of the structures the hash covers (the structures of the state, the rows and
/// `PublicKeys`): 17 + 12 + 9 + 4 + 6 + 5 + 6 + 2 + 2 + 3. The policy's 30 leaves have their own test.
const FIELD_COUNT: usize = 17 + 12 + 9 + 4 + 6 + 5 + 6 + 2 + 2 + 3;

// The functions below name EVERY field of one structure the hash covers, with no `..`: a field that is
// added to a structure makes its function stop compiling, so it cannot be added without the author
// reading this test and the encoder in `hash.rs`.

fn fields_of_team_state(state: &TeamState) {
    let TeamState {
        account: _,
        kind: _,
        genesis_hash: _,
        tip: _,
        members: _,
        devices: _,
        groups: _,
        policy: _,
        group_patches: _,
        policy_epoch: _,
        delegations: _,
        escrow_pub: _,
        escrow_holders: _,
        audit_pub: _,
        seal_epoch: _,
        entitlement: _,
        pending_opens: _,
    } = state;
}

fn fields_of_member(value: &Member) {
    let Member {
        id: _,
        kind: _,
        role: _,
        admin_scope: _,
        managed: _,
        state: _,
        keys: _,
        invite: _,
        app: _,
        added_at: _,
        removed_at: _,
        log_seq: _,
    } = value;
}

fn fields_of_device(value: &Device) {
    let Device {
        id: _,
        member: _,
        keys: _,
        state: _,
        enrolled_by: _,
        label_sealed: _,
        added_at: _,
        revoked_at: _,
        log_seq: _,
    } = value;
}

fn fields_of_group(value: &Group) {
    let Group {
        id: _,
        parent: _,
        name_sealed: _,
        log_seq: _,
    } = value;
}

fn fields_of_delegation(value: &Delegation) {
    let Delegation {
        key: _,
        ops: _,
        expires: _,
        granted_by: _,
        granted_at: _,
        log_seq: _,
    } = value;
}

fn fields_of_entitlement(value: &EntitlementFacts) {
    let EntitlementFacts {
        version: _,
        state: _,
        quota: _,
        seats: _,
        applied_at: _,
    } = value;
}

fn fields_of_pending_open(value: &PendingOpen) {
    let PendingOpen {
        seq: _,
        member: _,
        reason: _,
        requested_at: _,
        ready_at: _,
        signers: _,
    } = value;
}

fn fields_of_invite(value: &Invite) {
    let Invite {
        hash: _,
        inviter: _,
    } = value;
}

fn fields_of_app_spec(value: &AppSpec) {
    let AppSpec {
        scopes: _,
        expires_at: _,
    } = value;
}

fn fields_of_public_keys(value: &PublicKeys) {
    let PublicKeys {
        sign: _,
        kx: _,
        pq: _,
    } = value;
}

#[test]
fn every_field_of_every_structure_is_named_in_the_hash_tests() {
    // The functions above compile only while the structures have exactly these fields. Calling them
    // on the rows of the base state keeps them used, and the cases above cover every field.
    let state = base();
    fields_of_team_state(&state);
    for value in state.members.values() {
        fields_of_member(value);
        if let Some(keys) = &value.keys {
            fields_of_public_keys(keys);
        }
        if let Some(invite) = &value.invite {
            fields_of_invite(invite);
        }
        if let Some(app) = &value.app {
            fields_of_app_spec(app);
        }
    }
    state.devices.values().for_each(fields_of_device);
    state.groups.iter().for_each(fields_of_group);
    state.delegations.values().for_each(fields_of_delegation);
    state.entitlement.iter().for_each(fields_of_entitlement);
    state
        .pending_opens
        .values()
        .for_each(fields_of_pending_open);
}

#[test]
fn a_hash_of_a_state_with_one_member_moved_between_groups_differs() {
    // Two states with the same facts in different places are different states: m2 in g12 against m2 in
    // g13.
    let state = base();
    let mut moved = state.clone();
    assert!(moved.groups.remove_member(&group(12), &member(2)));
    assert!(moved.groups.add_member(&group(13), member(2)));
    assert_ne!(moved.state_hash(), state.state_hash());
}

#[test]
fn the_canonical_bytes_do_not_confuse_a_split_byte_string() {
    // Length-prefixed byte strings: the sealed name `[1, 2]` of one group and `[3]` of the next are not
    // the sealed name `[1]` of the first and `[2, 3]` of the next.
    let mut a = base();
    group_mut(&mut a, 11).name_sealed = Some(vec![1, 2]);
    group_mut(&mut a, 12).name_sealed = Some(vec![3]);
    let mut b = base();
    group_mut(&mut b, 11).name_sealed = Some(vec![1]);
    group_mut(&mut b, 12).name_sealed = Some(vec![2, 3]);
    assert_ne!(a.canonical_bytes(), b.canonical_bytes());
    assert_ne!(a.state_hash(), b.state_hash());
}
