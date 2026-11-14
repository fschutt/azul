//! The state the team log produces: `TeamState` (planning/architecture.md section 5.9 "The team log
//! and the space chains": "the metadata group applies each entry once, as it arrives, into the
//! registry's tables"; section 5.3 the rows; build unit S17, lane W1-S17c1).
//!
//! `TeamState` holds, for one account, everything the permission check reads and everything `apply`
//! changes: the account and its kind, the genesis hash that is the account's pin (planning/
//! architecture.md section 8.4), the members, the devices, the groups and their members, the account
//! policy and the patch of every group, the policy epoch, the delegations, the escrow and audit public
//! keys and the escrow holders, the seal key's epoch, the latest entitlement, the escrow openings that
//! wait, and the head of the chain (an `azlin_chain::Tip`). It holds no clock, no entropy and no I/O.
//!
//! EVERY collection is ordered (`BTreeMap`, `BTreeSet`): two replicas that apply the same entries
//! hold equal states and equal [`TeamState::state_hash`]es, whatever order a member or a device was
//! first inserted in, and nothing in the crate iterates an unordered collection.
//!
//! The state is changed only by `apply::apply` (every field is `pub(crate)`); readers get the
//! accessors below and [`TeamState::view`], plain data for the differential test of lane S47a.
//!
//! LIMITS (planning/architecture.md section 1.4 rule 4: everything has a limit, stated up front; the
//! plan gives no number for the log, so each is an estimate): see the `MAX_*` constants.

use std::collections::{BTreeMap, BTreeSet};

use azlin_chain::{ChainId, Head, Tip};
use azlin_policy::{effective, Policy, PolicyPatch};
use azlin_seams::{
    AccountId, AccountKind, ByteCount, ChainSeq, DeviceId, Epoch, GroupId, Hash32, KeyId, MemberId,
    PolicyEpoch, RestoreEpoch, Stamp,
};

use crate::devices::{Device, DeviceState, EnrolledBy, PublicKeys};
use crate::groups::{Group, Groups};
use crate::members::{Member, MemberState};
use crate::ops::{EntitlementState, EntitlementVersion, EscrowReason, OpName};
use crate::roles::Role;

/// The most members (of any state) an account may have (est.).
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 5.9 ("a company of 1,000
/// people").
///
/// Caller: `admit.rs` (`member_invite`, `genesis`).
pub const MAX_MEMBERS: usize = 100_000;

/// The most devices (of any state, revoked ones included: ids are never reused) an account may have
/// (est.).
///
/// Implements planning/architecture.md section 1.4 rule 4.
///
/// Caller: `admit.rs` (`device_add`, `member_join`).
pub const MAX_DEVICES: usize = 1_000_000;

/// The most groups an account may have (est.).
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 7.2 (a department tree).
///
/// Caller: `admit.rs` (`group_create`).
pub const MAX_GROUPS: usize = 10_000;

/// The deepest group tree: a top-level group has depth 0, so at most `MAX_GROUP_DEPTH + 1` levels
/// (est.). Every walk up the tree is bounded by it.
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 7.2.
///
/// Caller: `groups.rs`, `admit.rs` (`group_create`, `group_move`).
pub const MAX_GROUP_DEPTH: usize = 16;

/// The most groups one member may belong to directly (est.).
///
/// Implements planning/architecture.md section 1.4 rule 4.
///
/// Caller: `admit.rs` (`member_invite`, `group_member`).
pub const MAX_GROUPS_PER_MEMBER: usize = 64;

/// The most delegations an account may hold at once (est.): a handful of services (the identity
/// service, the key agent).
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 7.3 (the identity service's
/// authority).
///
/// Caller: `admit.rs` (`delegate`).
pub const MAX_DELEGATIONS: usize = 64;

/// The most escrow holders (est.).
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 7.6 (escrow with a threshold).
///
/// Caller: `admit.rs` (`escrow_set`).
pub const MAX_ESCROW_HOLDERS: usize = 16;

/// The most escrow openings that may wait at once (est.). Nothing in the log ends a waiting opening
/// in v0 (the objection and the cancellation are not in the table: drift row of lane W1-S17c), so
/// the number bounds the state.
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 7.5 (the escrow wait).
///
/// Caller: `admit.rs` (`escrow_open`).
pub const MAX_PENDING_ESCROW_OPENS: usize = 256;

/// A delegation: a key and the exact ops it may sign (planning/architecture.md section 5.9
/// "`delegate` | admin: names a key and the exact ops it may sign", section 7.3 "its key and the
/// exact ops it may sign").
///
/// Implements planning/architecture.md section 5.9 and section 7.3.
///
/// Caller: `TeamState::delegation`, `RegistryDelta::Delegation`, `delegate.rs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delegation {
    /// The delegate key.
    pub key: KeyId,
    /// The ops the key may sign: never empty (an empty set revokes, and the delegation is gone).
    pub ops: BTreeSet<OpName>,
    /// The leader's stamp from which the delegation is over; `None`: it does not expire.
    pub expires: Option<Stamp>,
    /// The member who signed the `delegate` entry.
    pub granted_by: MemberId,
    /// The leader's stamp of the `delegate` entry.
    pub granted_at: Stamp,
    /// The position of the `delegate` entry.
    pub log_seq: ChainSeq,
}

impl Delegation {
    /// Whether the delegation still holds at the leader's stamp `now`: it holds until `expires`, which
    /// is itself already over (an `expires` equal to `now` is expired).
    ///
    /// Implements planning/architecture.md section 2.4 (time is the leader's stamp) and the
    /// authority table v0 (`delegation_expiry_uses_the_leader_stamp`).
    ///
    /// Caller: `delegate.rs` (`covers`).
    pub fn is_live_at(&self, now: Stamp) -> bool {
        self.expires.is_none_or(|expires| now < expires)
    }
}

/// The latest entitlement the payments service submitted (planning/architecture.md section 5.3
/// `reg_account.entitlement`, `quota_bytes`, `seats`; COMMON-W1R R10).
///
/// Implements planning/architecture.md section 5.3.
///
/// Caller: `TeamState::entitlement`, `RegistryDelta::Entitlement`, `apply.rs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntitlementFacts {
    /// The version: monotone, the higher wins.
    pub version: EntitlementVersion,
    /// Active or lapsed.
    pub state: EntitlementState,
    /// The quota the plan gives.
    pub quota: ByteCount,
    /// The seats the plan gives.
    pub seats: u32,
    /// The leader's stamp of the entry that recorded it.
    pub applied_at: Stamp,
}

/// An escrow opening that waits (planning/architecture.md section 7.5 "Opening someone's data through
/// escrow": "72 h for an active member, during which the person or any other admin can object; no
/// wait for a member already offboarded by a quorum"). The wait itself is lane S17f's: the log only
/// records when it ends.
///
/// Implements planning/architecture.md section 7.5.
///
/// Caller: `TeamState::pending_escrow_opens`, `RegistryDelta::EscrowOpen`, `apply.rs`; lane S17f
/// (`azlin-enroll`) reads it through the registry glue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingOpen {
    /// The position of the `escrow_open` entry: the opening's id.
    pub seq: ChainSeq,
    /// The member whose escrow is opened.
    pub member: MemberId,
    /// Why.
    pub reason: EscrowReason,
    /// The leader's stamp of the entry.
    pub requested_at: Stamp,
    /// When the wait ends; `None`: no wait applies (the member was already removed).
    pub ready_at: Option<Stamp>,
    /// The members whose devices signed (the signer and the counted cosigners), in order.
    pub signers: Vec<MemberId>,
}

/// The state of one account's team log. See the module documentation.
///
/// Implements planning/architecture.md section 5.9 and section 5.3.
///
/// Caller: `apply::apply`, `admit::admit`, `replay::replay`, `verify::verify_signatures`; the registry
/// glue (TEAMLOG-APPLY) holds one for each account; lane S47a compares two through
/// [`TeamState::view`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeamState {
    pub(crate) account: AccountId,
    pub(crate) kind: AccountKind,
    pub(crate) genesis_hash: Option<Hash32>,
    pub(crate) tip: Tip,
    pub(crate) members: BTreeMap<MemberId, Member>,
    pub(crate) devices: BTreeMap<DeviceId, Device>,
    pub(crate) groups: Groups,
    pub(crate) policy: Policy,
    pub(crate) group_patches: BTreeMap<GroupId, PolicyPatch>,
    pub(crate) policy_epoch: PolicyEpoch,
    pub(crate) delegations: BTreeMap<KeyId, Delegation>,
    pub(crate) escrow_pub: Option<[u8; 32]>,
    pub(crate) escrow_holders: BTreeSet<MemberId>,
    pub(crate) audit_pub: Option<[u8; 32]>,
    pub(crate) seal_epoch: Epoch,
    pub(crate) entitlement: Option<EntitlementFacts>,
    pub(crate) pending_opens: BTreeMap<ChainSeq, PendingOpen>,
}

impl TeamState {
    /// The state of an account whose log has no entry yet: no member, no device, no group, the
    /// default policy of its kind, and an empty chain whose first entry must be a `genesis`. `kind`
    /// is the kind of the registry's account row (planning/architecture.md section 5.3
    /// `reg_account.kind`).
    ///
    /// Implements planning/architecture.md section 5.9 (the first entry is the genesis) and section
    /// 5.10 (the default policy of a kind).
    ///
    /// Caller: the registry glue (TEAMLOG-APPLY: a new account), `replay` (a device that verifies from
    /// the first entry), the tests.
    pub fn empty(account: AccountId, kind: AccountKind) -> TeamState {
        TeamState {
            account,
            kind,
            genesis_hash: None,
            tip: Tip::new(ChainId::team_log(account)),
            members: BTreeMap::new(),
            devices: BTreeMap::new(),
            groups: Groups::new(),
            policy: Policy::defaults(kind),
            group_patches: BTreeMap::new(),
            policy_epoch: PolicyEpoch::new(1),
            delegations: BTreeMap::new(),
            escrow_pub: None,
            escrow_holders: BTreeSet::new(),
            audit_pub: None,
            seal_epoch: Epoch::FIRST,
            entitlement: None,
            pending_opens: BTreeMap::new(),
        }
    }

    /// The state right after a `genesis` entry with the hash `genesis_hash`, stamped `stamp` by the
    /// leader, in the restore epoch `epoch`: the first owner (`owner`, Active, role owner) with its
    /// first device (`device`, Active), the genesis hash pinned, and the chain standing at position
    /// 0. `apply` of a `genesis` entry and this constructor make the same state.
    ///
    /// Implements planning/architecture.md section 5.9 (`genesis`: the first owner device) and
    /// section 8.4 (`genesis_hash` is the account's pin).
    ///
    /// Caller: the tests and the differential tests (a state to start from); `apply.rs` installs the
    /// same rows through the same function.
    #[allow(clippy::too_many_arguments)]
    pub fn genesis_state(
        account: AccountId,
        kind: AccountKind,
        owner: MemberId,
        device: DeviceId,
        member_keys: &PublicKeys,
        device_keys: &PublicKeys,
        genesis_hash: Hash32,
        stamp: Stamp,
        epoch: RestoreEpoch,
    ) -> TeamState {
        let mut state = TeamState::empty(account, kind);
        state.install_genesis(owner, device, member_keys, device_keys, genesis_hash, stamp);
        state.tip = Tip::resume(
            Head {
                chain: ChainId::team_log(account),
                seq: ChainSeq::MIN,
                hash: genesis_hash,
            },
            stamp,
            epoch,
        );
        state
    }

    /// Writes the rows a `genesis` makes: the first owner, its first device, the genesis hash. The
    /// chain is the caller's.
    pub(crate) fn install_genesis(
        &mut self,
        owner: MemberId,
        device: DeviceId,
        member_keys: &PublicKeys,
        device_keys: &PublicKeys,
        genesis_hash: Hash32,
        stamp: Stamp,
    ) {
        self.genesis_hash = Some(genesis_hash);
        self.members.insert(
            owner,
            Member {
                id: owner,
                kind: azlin_seams::MemberKind::Person,
                role: Role::Owner,
                admin_scope: None,
                managed: self.policy.managed_default.is_managed(),
                state: MemberState::Active,
                keys: Some(member_keys.clone()),
                invite: None,
                app: None,
                added_at: stamp,
                removed_at: None,
                log_seq: ChainSeq::MIN,
            },
        );
        self.devices.insert(
            device,
            Device {
                id: device,
                member: owner,
                keys: device_keys.clone(),
                state: DeviceState::Active,
                enrolled_by: EnrolledBy::Genesis,
                label_sealed: None,
                added_at: stamp,
                revoked_at: None,
                log_seq: ChainSeq::MIN,
            },
        );
    }

    /// Whether the log has no entry yet: the next entry must be a `genesis`.
    ///
    /// Implements planning/architecture.md section 5.9 (the first entry is the genesis).
    ///
    /// Caller: `admit.rs` and `apply.rs` (`genesis` only on an empty state), the tests.
    pub fn is_empty(&self) -> bool {
        self.tip.head().is_none()
    }

    /// The account.
    pub fn account(&self) -> AccountId {
        self.account
    }

    /// The kind of the account.
    pub fn kind(&self) -> AccountKind {
        self.kind
    }

    /// The hash of the genesis entry: the account's pin; `None` before the genesis.
    ///
    /// Implements planning/architecture.md section 5.3 (`reg_account.genesis_hash`).
    ///
    /// Caller: the registry glue (`reg_account.genesis_hash`), the tests.
    pub fn genesis_hash(&self) -> Option<Hash32> {
        self.genesis_hash
    }

    /// The link state of the chain: the head, the stamp of the newest entry and the restore epoch.
    ///
    /// Implements planning/architecture.md section 5.9 (hash-chained).
    ///
    /// Caller: the registry glue (a checkpoint), `admit.rs` (the chain check).
    pub fn tip(&self) -> &Tip {
        &self.tip
    }

    /// The head of the chain: the position and hash of the newest entry; `None` before the genesis.
    ///
    /// Implements planning/architecture.md section 5.9.
    ///
    /// Caller: the registry glue and the tests (`replay_from_checkpoint_state_equals_from_genesis`).
    pub fn head(&self) -> Option<Head> {
        self.tip.head()
    }

    /// The member with this id.
    pub fn member(&self, id: &MemberId) -> Option<&Member> {
        self.members.get(id)
    }

    /// Every member, in the order of the ids.
    pub fn members(&self) -> impl Iterator<Item = &Member> {
        self.members.values()
    }

    /// The device with this id (any state).
    pub fn device(&self, id: &DeviceId) -> Option<&Device> {
        self.devices.get(id)
    }

    /// Every device, in the order of the ids.
    pub fn devices(&self) -> impl Iterator<Item = &Device> {
        self.devices.values()
    }

    /// The devices of a member (any state), in the order of the ids.
    pub fn devices_of<'a>(&'a self, member: &'a MemberId) -> impl Iterator<Item = &'a Device> + 'a {
        self.devices
            .values()
            .filter(move |device| device.member == *member)
    }

    /// The groups and their members.
    pub fn groups(&self) -> &Groups {
        &self.groups
    }

    /// The account's policy.
    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    /// The patch a group set on top of what it inherits; `None` when it set none.
    pub fn group_patch(&self, group: &GroupId) -> Option<&PolicyPatch> {
        self.group_patches.get(group)
    }

    /// The policy that holds for a group: the account's policy with the patches of the groups from the
    /// top of the tree down to this one, each clamped to the level above (`azlin_policy::effective`);
    /// `None` for an unknown group.
    ///
    /// Implements planning/architecture.md section 5.10 ("groups inherit and may only tighten") and
    /// section 7.2.
    ///
    /// Caller: `admit.rs` and `apply.rs` (`policy_set` of a group), the registry glue (`AccountView`
    /// of the links, requests and spaces lanes).
    pub fn effective_policy_of(&self, group: &GroupId) -> Option<Policy> {
        let path = self.groups.path_from_top(group)?;
        let patches: Vec<&PolicyPatch> = path
            .iter()
            .filter_map(|id| self.group_patches.get(id))
            .collect();
        Some(effective(&self.policy, &patches))
    }

    /// The policy epoch: a policy change bumps it and an older credential is refused.
    ///
    /// Implements planning/architecture.md section 5.11 (`policy_epoch`).
    ///
    /// Caller: the registry glue (`reg_account.policy_epoch`).
    pub fn policy_epoch(&self) -> PolicyEpoch {
        self.policy_epoch
    }

    /// The delegation of a key.
    pub fn delegation(&self, key: &KeyId) -> Option<&Delegation> {
        self.delegations.get(key)
    }

    /// Every delegation, in the order of the keys.
    pub fn delegations(&self) -> impl Iterator<Item = &Delegation> {
        self.delegations.values()
    }

    /// The escrow public key; `None` until an `escrow_set`.
    pub fn escrow_pub(&self) -> Option<&[u8; 32]> {
        self.escrow_pub.as_ref()
    }

    /// The escrow holders.
    pub fn escrow_holders(&self) -> &BTreeSet<MemberId> {
        &self.escrow_holders
    }

    /// The audit public key; `None` until an `audit_key_set`.
    pub fn audit_pub(&self) -> Option<&[u8; 32]> {
        self.audit_pub.as_ref()
    }

    /// The epoch of the seal key that encrypts the `sealed` fields. No op of the table moves it in v0
    /// (drift row of lane W1-S17c), so it stays at the first epoch.
    pub fn seal_epoch(&self) -> Epoch {
        self.seal_epoch
    }

    /// The latest entitlement; `None` until the payments service submitted one.
    pub fn entitlement(&self) -> Option<&EntitlementFacts> {
        self.entitlement.as_ref()
    }

    /// The escrow openings that wait, in the order of their positions.
    pub fn pending_escrow_opens(&self) -> impl Iterator<Item = &PendingOpen> {
        self.pending_opens.values()
    }

    /// The Ed25519 public key of a device (any state, revoked ones included: a signature of a revoked
    /// device is told apart from a signature of an unknown one); `None` for an unknown device.
    ///
    /// Implements planning/architecture.md section 5.9 ("every entry is signed by a device").
    ///
    /// Caller: `verify.rs` (`verify_signatures`), the registry glue (`DeviceDirectory`).
    pub fn device_key(&self, device: DeviceId) -> Option<&[u8; 32]> {
        self.devices.get(&device).map(|found| &found.keys.sign)
    }

    /// Whether the device may sign at the leader's stamp `now`: it exists and is Active, or Pending
    /// with its hold over. This says nothing about its member.
    ///
    /// Implements planning/architecture.md section 7.5 (the enrollment hold).
    ///
    /// Caller: `admit.rs` (the signer and the cosigners).
    pub fn device_is_active_at(&self, device: &DeviceId, now: Stamp) -> bool {
        match self.devices.get(device) {
            Some(found) => found.state.is_active_at(now),
            None => false,
        }
    }

    /// How many Active owners there are, not counting `except`.
    ///
    /// Implements planning/architecture.md section 7.2 ("owner: everything, billing, owners") and the
    /// authority table v0 ("at least one Active owner remains").
    ///
    /// Caller: `admit.rs` (`last_owner`).
    pub fn active_owner_count(&self, except: Option<&MemberId>) -> usize {
        self.members
            .values()
            .filter(|member| {
                member.role == Role::Owner
                    && member.state == MemberState::Active
                    && Some(&member.id) != except
            })
            .count()
    }
}

/// A plain copy of a `TeamState` for the differential test of lane S47a: the same rows in the same
/// order, with no behaviour, so that two states (the production one and the reference model's) can be
/// compared field by field. Equal states have equal views.
///
/// Implements planning/architecture.md section 12 E1 ("one pure function, checked against a separate
/// reference model").
///
/// Caller: lane S47a (`project.rs`), the tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateView {
    /// The account.
    pub account: AccountId,
    /// The kind of the account.
    pub kind: AccountKind,
    /// The genesis hash.
    pub genesis_hash: Option<Hash32>,
    /// The head of the chain.
    pub head: Option<Head>,
    /// The stamp of the newest entry.
    pub last_at: Option<Stamp>,
    /// The restore epoch the next entry must have at least.
    pub restore_epoch: RestoreEpoch,
    /// Every member, in the order of the ids.
    pub members: Vec<Member>,
    /// Every device, in the order of the ids.
    pub devices: Vec<Device>,
    /// Every group, in the order of the ids.
    pub groups: Vec<Group>,
    /// Every (group, member) pair, in order.
    pub group_members: Vec<(GroupId, MemberId)>,
    /// The account's policy.
    pub policy: Policy,
    /// The patch of every group that set one, in the order of the ids.
    pub group_patches: Vec<(GroupId, PolicyPatch)>,
    /// The policy epoch.
    pub policy_epoch: PolicyEpoch,
    /// Every delegation, in the order of the keys.
    pub delegations: Vec<Delegation>,
    /// The escrow public key.
    pub escrow_pub: Option<[u8; 32]>,
    /// The escrow holders, in order.
    pub escrow_holders: Vec<MemberId>,
    /// The audit public key.
    pub audit_pub: Option<[u8; 32]>,
    /// The seal key's epoch.
    pub seal_epoch: Epoch,
    /// The latest entitlement.
    pub entitlement: Option<EntitlementFacts>,
    /// The escrow openings that wait, in the order of their positions.
    pub pending_escrow_opens: Vec<PendingOpen>,
}

impl TeamState {
    /// The plain copy of this state.
    ///
    /// Implements planning/architecture.md section 12 E1 (the differential test compares states).
    ///
    /// Caller: lane S47a (`project.rs`), the tests (`view_is_plain_data_and_equal_for_equal_states`).
    pub fn view(&self) -> StateView {
        StateView {
            account: self.account,
            kind: self.kind,
            genesis_hash: self.genesis_hash,
            head: self.tip.head(),
            last_at: self.tip.last_at(),
            restore_epoch: self.tip.epoch(),
            members: self.members.values().cloned().collect(),
            devices: self.devices.values().cloned().collect(),
            groups: self.groups.iter().cloned().collect(),
            group_members: self.groups.membership_pairs(),
            policy: self.policy.clone(),
            group_patches: self
                .group_patches
                .iter()
                .map(|(group, patch)| (*group, patch.clone()))
                .collect(),
            policy_epoch: self.policy_epoch,
            delegations: self.delegations.values().cloned().collect(),
            escrow_pub: self.escrow_pub,
            escrow_holders: self.escrow_holders.iter().copied().collect(),
            audit_pub: self.audit_pub,
            seal_epoch: self.seal_epoch,
            entitlement: self.entitlement.clone(),
            pending_escrow_opens: self.pending_opens.values().cloned().collect(),
        }
    }
}
