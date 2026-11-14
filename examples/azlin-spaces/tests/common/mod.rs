//! Helpers the test files share: typed ids and stamps, an account view over plain structs (the glue's
//! `AccountView` is written later, in iso), a device directory over `FakeScheme`, signed space-chain
//! entries, and `World`, a space whose chain is built one signed entry at a time through `verify_entry`,
//! `admit` and `apply`.
//!
//! The signed bytes of a space-chain entry are the TEST's own (a text layout under the real domain): the
//! real ones are the glue's (azlin-cbor), and this crate never builds them.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;

use azlin_keys::DirectoryError;
use azlin_policy::Policy;
use azlin_seams::{
    signing_input, AccountId, AccountKind, AccountRole, ByteCount, ChainSeq, DeviceDirectory,
    DeviceId, Epoch, FakeScheme, GroupId, Hash32, KeyId, MemberId, MemberKind, RestoreEpoch,
    Secret, SpaceId, Stamp, UnixTime, VerifyAt, VerifyKey,
};
use azlin_spaces::{
    admit, apply, entry_hash, verify_entry, AccountView, Admitted, Cosig, Grantee, HistoryMode,
    Payer, RekeyReason, SignerFacts, SpaceEffects, SpaceEntry, SpaceKind, SpaceOp, SpaceProposal,
    SpaceRefusal, SpaceRole, SpaceState, SPACE_CHAIN_DOMAIN,
};

// ---------------------------------------------------------------------------------------------
// Typed values
// ---------------------------------------------------------------------------------------------

pub fn at(secs: u64) -> Stamp {
    UnixTime::from_secs(secs)
}

pub fn space(n: u8) -> SpaceId {
    SpaceId::from_bytes([n; 16])
}

/// The account of the spaces under test.
pub fn account(n: u8) -> AccountId {
    AccountId::from_bytes([n; 16])
}

/// The member `n` of the account 1.
pub fn member(n: u8) -> MemberId {
    MemberId::from_bytes([n; 16])
}

/// The device `n` (the device of the member `n`).
pub fn device(n: u8) -> DeviceId {
    DeviceId::from_bytes([n; 16])
}

pub fn group(n: u8) -> GroupId {
    GroupId::from_bytes([n; 16])
}

/// The key id that a chain entry commits for an epoch.
pub fn key(n: u8) -> KeyId {
    KeyId::from_bytes([0x40 + n; 32])
}

/// The fake signing key of the device `n` (its public key is its own bytes).
pub fn device_key(n: u8) -> KeyId {
    KeyId::from_bytes([n ^ 0x5A; 32])
}

/// The member `n` of the OTHER account (the account 2).
pub fn ext_member(n: u8) -> MemberId {
    MemberId::from_bytes([0xE0 + n; 16])
}

/// The device of the external member `n`.
pub fn ext_device(n: u8) -> DeviceId {
    DeviceId::from_bytes([0xE0 + n; 16])
}

pub fn epoch(n: u32) -> Epoch {
    Epoch::new(n).expect("a test epoch is not zero")
}

pub fn seq(n: u64) -> ChainSeq {
    ChainSeq::new(n)
}

pub fn bytes(n: u64) -> ByteCount {
    ByteCount::new(n)
}

pub fn hash_of_fill(fill: u8) -> Hash32 {
    Hash32::from_bytes([fill; 32])
}

/// A bearer secret whose 32 bytes are all different from each other and from every other `n`.
pub fn secret(n: u8) -> Secret<32> {
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = (i as u8)
            .wrapping_mul(7)
            .wrapping_add(n.wrapping_mul(31))
            .wrapping_add(0x13);
    }
    Secret::from_bytes(out)
}

/// The grantee that is the member `n` of the account.
pub fn m(n: u8) -> Grantee {
    Grantee::Member(member(n))
}

/// The grantee that is the group `n`.
pub fn g(n: u8) -> Grantee {
    Grantee::Group(group(n))
}

/// The grantee that is the external member `n` (of the account 2).
pub fn x(n: u8) -> Grantee {
    Grantee::External {
        account: account(2),
        member: ext_member(n),
    }
}

/// The grantee that is the capability of the secret `n` (its hash).
pub fn cap(n: u8) -> Grantee {
    Grantee::Capability(azlin_spaces::capability_hash(&secret(n)))
}

// ---------------------------------------------------------------------------------------------
// The ops
// ---------------------------------------------------------------------------------------------

pub fn create_op(kind: SpaceKind, history: HistoryMode, first_manager: u8, key_n: u8) -> SpaceOp {
    SpaceOp::Create {
        kind,
        history,
        first_manager: member(first_manager),
        key_id: key(key_n),
    }
}

pub fn grant_op(grantee: Grantee, role: SpaceRole) -> SpaceOp {
    SpaceOp::Grant {
        grantee,
        role,
        cap_bytes: None,
        payer: Payer::Owner,
    }
}

pub fn grant_full(
    grantee: Grantee,
    role: SpaceRole,
    cap_bytes: Option<u64>,
    payer: Payer,
) -> SpaceOp {
    SpaceOp::Grant {
        grantee,
        role,
        cap_bytes: cap_bytes.map(ByteCount::new),
        payer,
    }
}

pub fn revoke_op(grantee: Grantee) -> SpaceOp {
    SpaceOp::Revoke { grantee }
}

pub fn rekey_op(epoch_n: u32, key_n: u8) -> SpaceOp {
    SpaceOp::Rekey {
        epoch: epoch(epoch_n),
        key_id: key(key_n),
        reason: RekeyReason::Manual,
    }
}

pub fn history_op(mode: HistoryMode) -> SpaceOp {
    SpaceOp::History { mode }
}

// ---------------------------------------------------------------------------------------------
// The account, as the glue will see it
// ---------------------------------------------------------------------------------------------

/// A member of the account: its role, what it is, whether it is active.
#[derive(Clone, Copy, Debug)]
pub struct Person {
    pub role: AccountRole,
    pub kind: MemberKind,
    pub active: bool,
}

/// A device of the account: its member and whether it is active.
#[derive(Clone, Copy, Debug)]
pub struct DeviceRow {
    pub member: MemberId,
    pub active: bool,
}

/// The roles of the members 1 to 12 of the test account: 1 owner, 2 admin, 3 group admin, 4 helpdesk,
/// 5 member, 6 guest, and plain members from 7 on.
pub fn role_of(n: u8) -> AccountRole {
    match n {
        1 => AccountRole::Owner,
        2 => AccountRole::Admin,
        3 => AccountRole::GroupAdmin,
        4 => AccountRole::Helpdesk,
        6 => AccountRole::Guest,
        _ => AccountRole::Member,
    }
}

/// The test account: members `1..=12` (each with its device of the same number), three groups, the
/// policy defaults of its kind, and the kinds of space that members may create.
#[derive(Clone)]
pub struct FakeAccount {
    pub id: AccountId,
    pub kind: AccountKind,
    pub policy: Policy,
    pub creators: BTreeSet<SpaceKind>,
    pub people: BTreeMap<MemberId, Person>,
    pub devices: BTreeMap<DeviceId, DeviceRow>,
    pub groups: BTreeSet<GroupId>,
    pub membership: BTreeMap<MemberId, Vec<GroupId>>,
    pub root_space: bool,
}

impl FakeAccount {
    pub fn new(kind: AccountKind) -> FakeAccount {
        let mut account_view = FakeAccount {
            id: account(1),
            kind,
            policy: Policy::defaults(kind),
            creators: BTreeSet::from([SpaceKind::Shared, SpaceKind::Conversation]),
            people: BTreeMap::new(),
            devices: BTreeMap::new(),
            groups: BTreeSet::from([group(1), group(2), group(3)]),
            membership: BTreeMap::new(),
            root_space: false,
        };
        for n in 1..=12u8 {
            account_view.add_person(n, role_of(n), MemberKind::Person);
        }
        account_view
    }

    /// Adds the member `n` with its device `n`.
    pub fn add_person(&mut self, n: u8, role: AccountRole, kind: MemberKind) {
        self.people.insert(
            member(n),
            Person {
                role,
                kind,
                active: true,
            },
        );
        self.devices.insert(
            device(n),
            DeviceRow {
                member: member(n),
                active: true,
            },
        );
    }

    /// Adds a second device (`device_n`) to the member `member_n`.
    pub fn add_device(&mut self, device_n: u8, member_n: u8) {
        self.devices.insert(
            device(device_n),
            DeviceRow {
                member: member(member_n),
                active: true,
            },
        );
    }

    /// Puts the member `n` into the groups `groups`.
    pub fn put_in_groups(&mut self, n: u8, groups: &[u8]) {
        self.membership
            .insert(member(n), groups.iter().map(|g| group(*g)).collect());
    }

    /// Suspends the member `n`: it is no active member any more.
    pub fn suspend(&mut self, n: u8) {
        if let Some(person) = self.people.get_mut(&member(n)) {
            person.active = false;
        }
    }

    /// The device `n` is revoked or still in its hold.
    pub fn deactivate_device(&mut self, device_n: u8) {
        if let Some(row) = self.devices.get_mut(&device(device_n)) {
            row.active = false;
        }
    }

    /// The facts about an internal device, as the team log would give them.
    pub fn facts_of(&self, device_id: DeviceId) -> Option<SignerFacts> {
        let row = self.devices.get(&device_id)?;
        let person = self.people.get(&row.member)?;
        Some(SignerFacts {
            device: device_id,
            member: row.member,
            account: self.id,
            member_kind: person.kind,
            device_active: row.active,
            member_active: person.active,
            account_role: person.role,
        })
    }
}

impl AccountView for FakeAccount {
    fn policy(&self) -> &Policy {
        &self.policy
    }

    fn kind(&self) -> AccountKind {
        self.kind
    }

    fn members_may_create(&self, kind: SpaceKind) -> bool {
        self.creators.contains(&kind)
    }

    fn is_member(&self, member_id: MemberId) -> bool {
        self.people
            .get(&member_id)
            .is_some_and(|person| person.active)
    }

    fn has_group(&self, group_id: GroupId) -> bool {
        self.groups.contains(&group_id)
    }

    fn groups_of(&self, member_id: MemberId) -> Vec<GroupId> {
        self.membership.get(&member_id).cloned().unwrap_or_default()
    }

    fn same_account(&self, other: AccountId) -> bool {
        other == self.id
    }

    fn has_root_space(&self) -> bool {
        self.root_space
    }

    fn signer_facts(&self, device_id: DeviceId) -> Option<SignerFacts> {
        self.facts_of(device_id)
    }
}

// ---------------------------------------------------------------------------------------------
// The device directory over FakeScheme
// ---------------------------------------------------------------------------------------------

/// The directory of the tests: every device's fake signing key, and the answers of the devices the test
/// breaks.
#[derive(Default)]
pub struct FakeDirectory {
    pub keys: BTreeMap<DeviceId, KeyId>,
    pub errors: BTreeMap<DeviceId, DirectoryError>,
}

impl FakeDirectory {
    /// Every device of the account 1 (1 to 40) and of the external members (1 to 9).
    pub fn full() -> FakeDirectory {
        let mut dir = FakeDirectory::default();
        for n in 1..=40u8 {
            dir.keys.insert(device(n), device_key(n));
        }
        for n in 1..=9u8 {
            dir.keys
                .insert(ext_device(n), KeyId::from_bytes([0xE0 + n; 32]));
        }
        dir
    }
}

impl DeviceDirectory for FakeDirectory {
    fn verify_key(&self, device_id: &DeviceId, _at: VerifyAt) -> Result<VerifyKey, DirectoryError> {
        if let Some(error) = self.errors.get(device_id) {
            return Err(*error);
        }
        match self.keys.get(device_id) {
            Some(key_id) => Ok(VerifyKey::from_bytes(*key_id.as_bytes())),
            None => Err(DirectoryError::Unknown),
        }
    }
}

/// The fake signing key of any test device: an internal device `n`, or an external one.
pub fn signing_key(device_id: DeviceId) -> KeyId {
    for n in 1..=40u8 {
        if device_id == device(n) {
            return device_key(n);
        }
    }
    for n in 1..=9u8 {
        if device_id == ext_device(n) {
            return KeyId::from_bytes([0xE0 + n; 32]);
        }
    }
    KeyId::from_bytes([0xFF; 32])
}

// ---------------------------------------------------------------------------------------------
// Signed entries
// ---------------------------------------------------------------------------------------------

/// The text the tests sign for an entry: every field of the entry that matters.
fn canonical(
    space_id: SpaceId,
    entry_seq: ChainSeq,
    prev: Hash32,
    signer: DeviceId,
    op: &SpaceOp,
    restore: RestoreEpoch,
) -> String {
    format!("{space_id}|{entry_seq}|{prev}|{signer}|{restore}|{op:?}")
}

/// An entry at an arbitrary place, signed by `signer` and cosigned by `cosigners`.
pub fn signed_entry(
    space_id: SpaceId,
    entry_seq: ChainSeq,
    prev: Hash32,
    signer: DeviceId,
    op: SpaceOp,
    cosigners: &[DeviceId],
    restore: RestoreEpoch,
) -> SpaceEntry {
    let text = canonical(space_id, entry_seq, prev, signer, &op, restore);
    let preimage = signing_input(&SPACE_CHAIN_DOMAIN, text.as_bytes());
    SpaceEntry {
        space: space_id,
        seq: entry_seq,
        prev,
        hash: entry_hash(&preimage),
        epoch: restore,
        signer,
        op,
        sig: FakeScheme::sign(&signing_key(signer), &preimage),
        cosigs: cosigners
            .iter()
            .map(|cosigner| Cosig {
                device: *cosigner,
                sig: FakeScheme::sign(&signing_key(*cosigner), &preimage),
            })
            .collect(),
        preimage,
    }
}

// ---------------------------------------------------------------------------------------------
// The world: a space and its account, built one signed entry at a time
// ---------------------------------------------------------------------------------------------

pub struct World {
    pub account: FakeAccount,
    pub externals: BTreeMap<DeviceId, SignerFacts>,
    pub state: SpaceState,
    pub dir: FakeDirectory,
    pub clock: u64,
    pub restore: RestoreEpoch,
}

impl World {
    /// An account of this kind and the space 1 of it, whose chain is empty.
    pub fn new(kind: AccountKind) -> World {
        let mut externals = BTreeMap::new();
        for n in 1..=9u8 {
            externals.insert(
                ext_device(n),
                SignerFacts {
                    device: ext_device(n),
                    member: ext_member(n),
                    account: account(2),
                    member_kind: MemberKind::Person,
                    device_active: true,
                    member_active: true,
                    // Its role in its OWN account: it must give no authority in this one.
                    account_role: AccountRole::Owner,
                },
            );
        }
        World {
            account: FakeAccount::new(kind),
            externals,
            state: SpaceState::new(space(1), account(1)),
            dir: FakeDirectory::full(),
            clock: 1_000,
            restore: RestoreEpoch::ZERO,
        }
    }

    /// A world whose space was created by the owner (member 1): a space of this kind and history, the
    /// owner its first manager, the key 1 its first epoch.
    pub fn created(kind: AccountKind, space_kind: SpaceKind, history: HistoryMode) -> World {
        let mut world = World::new(kind);
        world.do_by(1, create_op(space_kind, history, 1, 1));
        world
    }

    /// The stamp the next entry is admitted at.
    pub fn now(&self) -> Stamp {
        at(self.clock + 10)
    }

    /// The facts about a device (an internal one, or an external member's).
    pub fn facts(&self, device_id: DeviceId) -> SignerFacts {
        if let Some(facts) = self.account.facts_of(device_id) {
            return facts;
        }
        *self
            .externals
            .get(&device_id)
            .expect("a test device has facts")
    }

    /// The next entry of the chain, signed by `signer` (and cosigned).
    pub fn build(&self, signer: DeviceId, op: SpaceOp, cosigners: &[DeviceId]) -> SpaceEntry {
        let next = self
            .state
            .chain()
            .tip()
            .next_seq()
            .expect("a test chain has a next position");
        let prev = self
            .state
            .chain_head()
            .map_or(Hash32::ZERO, |head| head.hash);
        signed_entry(
            self.state.id(),
            next,
            prev,
            signer,
            op,
            cosigners,
            self.restore,
        )
    }

    /// The signature check of an entry.
    pub fn verify(&self, entry: &SpaceEntry) -> Result<SpaceProposal, SpaceRefusal> {
        verify_entry(entry, &self.dir, &FakeScheme, self.now())
    }

    /// The permission check of an entry, with the facts of its signer.
    pub fn admit_entry(&self, entry: &SpaceEntry) -> Result<Admitted, SpaceRefusal> {
        let proposal = self.verify(entry)?;
        let who = self.facts(entry.signer);
        admit(&self.state, &proposal, &who, &self.account, self.now())
    }

    /// The next entry by `signer`: checked and not applied.
    pub fn try_admit_as(
        &self,
        signer: DeviceId,
        op: SpaceOp,
        cosigners: &[DeviceId],
    ) -> Result<Admitted, SpaceRefusal> {
        self.admit_entry(&self.build(signer, op, cosigners))
    }

    /// The next entry by the device of member `n`: checked and not applied.
    pub fn admit_by(&self, n: u8, op: SpaceOp) -> Result<Admitted, SpaceRefusal> {
        self.try_admit_as(device(n), op, &[])
    }

    /// The next entry by `signer`, checked and applied.
    pub fn submit_as(
        &mut self,
        signer: DeviceId,
        op: SpaceOp,
        cosigners: &[DeviceId],
    ) -> Result<SpaceEffects, SpaceRefusal> {
        let admitted = self.try_admit_as(signer, op, cosigners)?;
        let effects = apply(&mut self.state, admitted);
        assert!(effects.applied, "an admitted entry applies");
        self.clock += 10;
        Ok(effects)
    }

    /// The next entry by the device of member `n`, checked and applied.
    pub fn submit_by(&mut self, n: u8, op: SpaceOp) -> Result<SpaceEffects, SpaceRefusal> {
        self.submit_as(device(n), op, &[])
    }

    /// The next entry by the device of member `n`; it must be admitted.
    pub fn do_by(&mut self, n: u8, op: SpaceOp) -> SpaceEffects {
        match self.submit_by(n, op.clone()) {
            Ok(effects) => effects,
            Err(why) => panic!("member {n} signs {op:?}: refused: {why}"),
        }
    }

    /// The next entry by the device of member `n`; it must be refused, and the state stays as it was.
    pub fn refused_by(&mut self, n: u8, op: SpaceOp) -> SpaceRefusal {
        let before = self.state.state_hash();
        let refusal = match self.admit_by(n, op.clone()) {
            Err(why) => why,
            Ok(admitted) => panic!("member {n} signs {op:?}: admitted {admitted:?}"),
        };
        assert_eq!(self.state.state_hash(), before, "a refusal changes nothing");
        refusal
    }

    /// The next entry by the external member `n`; it must be admitted.
    pub fn do_by_ext(&mut self, n: u8, op: SpaceOp) -> SpaceEffects {
        match self.submit_as(ext_device(n), op.clone(), &[]) {
            Ok(effects) => effects,
            Err(why) => panic!("external member {n} signs {op:?}: refused: {why}"),
        }
    }
}

/// The refusal in a result that must be an `Err`.
pub fn refusal_of<T: Debug>(result: Result<T, SpaceRefusal>) -> SpaceRefusal {
    match result {
        Err(why) => why,
        Ok(value) => panic!("expected a refusal, got {value:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// Compile-time witnesses of what a type is NOT
// ---------------------------------------------------------------------------------------------

/// `<T as AmbiguousIfClone<_>>::check()` compiles only when `T` is not `Clone`.
pub trait AmbiguousIfClone<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfClone<()> for T {}
impl<T: Clone> AmbiguousIfClone<u8> for T {}

/// The same for `Default`.
pub trait AmbiguousIfDefault<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfDefault<()> for T {}
impl<T: Default> AmbiguousIfDefault<u8> for T {}
