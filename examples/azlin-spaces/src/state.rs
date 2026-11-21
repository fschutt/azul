//! The state of one space as its membership chain made it (planning/architecture.md section 5.4
//! "Spaces", section 5.9 "Space chains", section 5.5 "Epochs must be authorized"; build unit S16, lane
//! W1-S16b).
//!
//! [`SpaceState`] is the space's `reg_space` row and its `reg_grant` rows, plus what the key rules need
//! from the chain (the epoch each signed entry committed, who could sign at each position) and the
//! chain itself, kept by `azlin_chain::Chain` so that the position, the `prev` hash, the leader's stamp,
//! the restore epoch and a fork are `azlin-chain`'s one set of rules. The state changes in ONE place,
//! `apply`; every collection is ordered, so two replicas that apply the same entries hold equal states
//! and equal [`SpaceState::state_hash`]es, and a device that replays a chain gets the state the node
//! holds.
//!
//! LIMITS (planning/architecture.md section 1.4 rule 4, every one an estimate): [`MAX_ENTRIES`] entries
//! of a chain, [`MAX_GRANTS`] grant rows, [`MAX_CAPABILITIES`] capability grants.

use std::collections::BTreeMap;

use azlin_chain::{Chain, ChainEntry, ChainId, Head};
use azlin_keys::CommitKind;
use azlin_seams::{
    AccountId, ByteCount, ChainSeq, DeviceId, Epoch, Hash32, MemberId, RestoreEpoch, SpaceId, Stamp,
};

use crate::epochs::EpochRecord;
use crate::grant::{Grant, Grantee, Source};
use crate::kind::{HistoryMode, SpaceKind};
use crate::ops::{RekeyReason, SpaceOp};

/// The most entries a space chain holds (est.): the chain is "short" (section 5.9), a company-wide
/// channel with daily churn writes a few thousand a year. The next entry after it is refused (`Chain`
/// `Full`).
///
/// Implements planning/architecture.md section 1.4 rule 4 (everything has a limit, stated up front) and
/// section 5.9 ("short signed membership chain").
///
/// Caller: `SpaceState::new` (the bound of the chain).
pub const MAX_ENTRIES: usize = 100_000;

/// The most grant rows a space holds, revoked ones included (est.): a grant to a group covers a whole
/// department, so a space needs a row for each individual only where someone is shared with by name.
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 5.6 ("Wraps go to groups, not to
/// each member").
///
/// Caller: `admit` (a grant to a grantee with no row is refused at the bound).
pub const MAX_GRANTS: usize = 20_000;

/// The most capability grants a space holds, revoked ones included (est.): each is a bearer secret
/// somebody holds.
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 5.15 M6 (capability grants).
///
/// Caller: `admit` (a capability grant beyond the bound is refused).
pub const MAX_CAPABILITIES: usize = 1_000;

/// The hash domain of [`SpaceState::state_hash`]. It signs nothing: it separates this hash from every
/// other BLAKE3 hash of the engine (drift row X-DELTA-W1S16B-13).
///
/// Implements planning/architecture.md section 5.9 (a state replayed from the chain is the state the
/// node holds) and section 2.5 (a domain string per structure).
///
/// Caller: `SpaceState::state_hash`.
pub const STATE_HASH_DOMAIN: &str = "azlin-spaces-state-v1";

/// The account and the member behind a signing device, as the facts of the signature said them.
///
/// Implements planning/architecture.md section 5.9 (every entry is signed by a device of a member).
///
/// Caller: `EntryRecord::actor`, `SpaceChains::learn_device`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Actor {
    /// The account the member belongs to (the space's own for an internal member).
    pub account: AccountId,
    /// The member.
    pub member: MemberId,
}

impl Actor {
    /// The grantee that this actor is, in a space of the account `space_account`: a member of that
    /// account, or an external member of another.
    pub(crate) fn grantee(&self, space_account: AccountId) -> Grantee {
        if self.account == space_account {
            Grantee::Member(self.member)
        } else {
            Grantee::External {
                account: self.account,
                member: self.member,
            }
        }
    }
}

/// Which rule let a signer sign an entry: the creation rules, the signer's manager grant, a grantee's
/// leaving, an account admin's revoke, or an account admin's override with the admin quorum.
///
/// Implements planning/architecture.md section 5.9 ("signed by its managers") and the drift answer of
/// the brief (its drift item 1: only managers sign grants; an account admin may revoke, and may add a
/// manager to an orphaned space only with the admin quorum).
///
/// Caller: `EntryRecord::authority`, `admit` (decides it), the audit of a chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Authority {
    /// The signer created the space (the creation rules).
    Creator,
    /// The signer held the manage capability, directly or through a group.
    Manager,
    /// The signer revoked its own grant: it left.
    SelfLeave,
    /// An account owner or admin revoked a grant without being a manager.
    AdminRevoke,
    /// An account owner or admin, with the admin quorum, added a manager to an orphaned space.
    AdminOverride,
}

impl Authority {
    /// The byte of the authority in the state hash.
    const fn tag(self) -> u8 {
        match self {
            Authority::Creator => 0,
            Authority::Manager => 1,
            Authority::SelfLeave => 2,
            Authority::AdminRevoke => 3,
            Authority::AdminOverride => 4,
        }
    }
}

/// One admitted entry of the chain, as the state keeps it: its place in the chain, who signed it, under
/// which authority, and the op. It is the `ChainEntry` of the space's `azlin_chain::Chain` (so the
/// link rules are the chain crate's) and the record a device audits.
///
/// `at` is the LEADER'S stamp the entry was admitted at (planning/architecture.md section 2.4), not a
/// time the signing device claims.
///
/// Implements planning/architecture.md section 5.9 (hash-chained entries, each signed by a device) and
/// section 6.10 V3 (every entry names its restore epoch).
///
/// Caller: `SpaceState::chain` (the entries), `Admitted` (made by `admit`), `SpaceChains` (who signed
/// at each position).
#[derive(Clone, Debug)]
pub struct EntryRecord {
    pub(crate) space: SpaceId,
    pub(crate) seq: ChainSeq,
    pub(crate) prev: Hash32,
    pub(crate) hash: Hash32,
    pub(crate) at: Stamp,
    pub(crate) epoch: RestoreEpoch,
    pub(crate) signer: DeviceId,
    pub(crate) actor: Actor,
    pub(crate) op: SpaceOp,
    pub(crate) authority: Authority,
}

impl EntryRecord {
    /// The position of the entry.
    pub fn seq(&self) -> ChainSeq {
        self.seq
    }

    /// The hash of the entry.
    pub fn hash(&self) -> Hash32 {
        self.hash
    }

    /// The device that signed it.
    pub fn signer(&self) -> DeviceId {
        self.signer
    }

    /// The account and member behind the signing device.
    pub fn actor(&self) -> Actor {
        self.actor
    }

    /// The op of the entry.
    pub fn op(&self) -> &SpaceOp {
        &self.op
    }

    /// The rule that let the signer sign it.
    pub fn authority(&self) -> Authority {
        self.authority
    }
}

impl ChainEntry for EntryRecord {
    fn chain(&self) -> ChainId {
        ChainId::space(self.space)
    }

    fn seq(&self) -> ChainSeq {
        self.seq
    }

    fn prev(&self) -> Hash32 {
        self.prev
    }

    fn hash(&self) -> Hash32 {
        self.hash
    }

    fn at(&self) -> Stamp {
        self.at
    }

    fn epoch(&self) -> RestoreEpoch {
        self.epoch
    }

    /// The signatures were checked before the entry reached `admit` (`verify_entry`), so a copy that
    /// got here is intact.
    fn is_intact(&self) -> bool {
        true
    }
}

/// What the first entry fixed: the kind, the history mode, the current epoch, who created the space and
/// when.
#[derive(Clone, Debug)]
pub(crate) struct Core {
    pub(crate) kind: SpaceKind,
    pub(crate) history: HistoryMode,
    pub(crate) key_epoch: Epoch,
    pub(crate) created_by: MemberId,
    pub(crate) created_at: Stamp,
}

/// The positions at which a member held the manage capability through a DIRECT grant: from `from`
/// (inclusive: the position after the entry that granted it) until `until` (exclusive: the position
/// after the entry that ended it; `None`: still held).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Window {
    pub(crate) from: ChainSeq,
    pub(crate) until: Option<ChainSeq>,
}

/// The `reg_space` row of a space after a change: the columns the chain decides (section 5.4); the
/// registry keeps the rest (bucket, prefix, retention, locks, region, class, replicas).
///
/// Implements planning/architecture.md section 5.4 (`reg_space`: `kind`, `key_epoch`, `history`,
/// `created_by`, `chain_head`, `archived_at`).
///
/// Caller: `SpaceDelta::Space` (the registry glue writes it), `SpaceState::row`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceRow {
    /// The space.
    pub space: SpaceId,
    /// The account the space belongs to.
    pub account: AccountId,
    /// The kind.
    pub kind: SpaceKind,
    /// The current key epoch.
    pub key_epoch: Epoch,
    /// The history mode.
    pub history: HistoryMode,
    /// The head of the space's membership chain.
    pub chain_head: Head,
    /// The member who created the space.
    pub created_by: MemberId,
    /// When the space was archived, if it was.
    pub archived_at: Option<Stamp>,
}

/// The state of one space.
///
/// Implements planning/architecture.md section 5.4 (`reg_space`, `reg_grant`), section 5.9 (the
/// space's membership chain) and section 5.5 (the epochs a signed entry committed).
///
/// Caller: `admit` and `apply` (the node's permission check and apply, through SPACES-APPLY),
/// `replay` (a device's check of a chain), `access` and `charge` (the node's per-connection check and
/// the write path), `epoch_authorized`, `SpaceChains`.
#[derive(Clone, Debug)]
pub struct SpaceState {
    pub(crate) id: SpaceId,
    pub(crate) account: AccountId,
    pub(crate) core: Option<Core>,
    pub(crate) history_switches: Vec<(ChainSeq, HistoryMode)>,
    pub(crate) epochs: BTreeMap<Epoch, EpochRecord>,
    pub(crate) grants: BTreeMap<Grantee, Grant>,
    pub(crate) chain: Chain<EntryRecord>,
    pub(crate) rekey_due: Option<RekeyReason>,
    pub(crate) archived: Option<Stamp>,
    pub(crate) windows: BTreeMap<Grantee, Vec<Window>>,
    pub(crate) signers: BTreeMap<DeviceId, Actor>,
}

impl SpaceState {
    /// The state of a space whose chain is empty: the next entry must be its `space_create`.
    pub fn new(space: SpaceId, account: AccountId) -> SpaceState {
        SpaceState {
            id: space,
            account,
            core: None,
            history_switches: Vec::new(),
            epochs: BTreeMap::new(),
            grants: BTreeMap::new(),
            chain: Chain::with_limit(ChainId::space(space), MAX_ENTRIES),
            rekey_due: None,
            archived: None,
            windows: BTreeMap::new(),
            signers: BTreeMap::new(),
        }
    }

    /// The space.
    pub fn id(&self) -> SpaceId {
        self.id
    }

    /// The account the space belongs to.
    pub fn account(&self) -> AccountId {
        self.account
    }

    /// Whether the space's first entry was applied.
    pub fn is_created(&self) -> bool {
        self.core.is_some()
    }

    /// The kind; `None` before the space is created.
    pub fn kind(&self) -> Option<SpaceKind> {
        self.core.as_ref().map(|core| core.kind)
    }

    /// The history mode in force now; `None` before the space is created.
    pub fn history(&self) -> Option<HistoryMode> {
        self.core.as_ref().map(|core| core.history)
    }

    /// The current key epoch; `None` before the space is created.
    pub fn key_epoch(&self) -> Option<Epoch> {
        self.core.as_ref().map(|core| core.key_epoch)
    }

    /// The member who created the space; `None` before it is created.
    pub fn created_by(&self) -> Option<MemberId> {
        self.core.as_ref().map(|core| core.created_by)
    }

    /// The grant row of a grantee, in force or ended.
    pub fn grant(&self, grantee: &Grantee) -> Option<&Grant> {
        self.grants.get(grantee)
    }

    /// Every grant row, in the order of the grantees: ended ones included.
    pub fn grants(&self) -> impl Iterator<Item = &Grant> + '_ {
        self.grants.values()
    }

    /// The grants in force.
    pub fn active_grants(&self) -> impl Iterator<Item = &Grant> + '_ {
        self.grants.values().filter(|grant| grant.is_active())
    }

    /// The membership chain: every admitted entry, from the `space_create`.
    pub fn chain(&self) -> &Chain<EntryRecord> {
        &self.chain
    }

    /// The head of the chain; `None` before the first entry.
    pub fn chain_head(&self) -> Option<Head> {
        self.chain.head()
    }

    /// Why a rekey is due, if one is: a forward-space joiner or a removal made the current key
    /// something somebody should not hold. A `space_rekey` entry clears it.
    pub fn rekey_due(&self) -> Option<RekeyReason> {
        self.rekey_due
    }

    /// When the space was archived; `None` while it is live.
    pub fn archived(&self) -> Option<Stamp> {
        self.archived
    }

    /// Archives the space at `at` (the registry's `archived_at`): an archived space admits no further
    /// entry and its principals get at most read access. Archiving again keeps the first stamp.
    ///
    /// Implements planning/architecture.md section 5.4 (`reg_space.archived_at`, bookkeeping).
    ///
    /// Caller: the SPACES-APPLY glue, when the registry marks the space archived.
    pub fn archive(&mut self, at: Stamp) {
        if self.archived.is_none() {
            self.archived = Some(at);
        }
    }

    /// The `reg_space` row of the space now; `None` before it is created.
    pub fn row(&self) -> Option<SpaceRow> {
        let core = self.core.as_ref()?;
        let chain_head = self.chain.head()?;
        Some(SpaceRow {
            space: self.id,
            account: self.account,
            kind: core.kind,
            key_epoch: core.key_epoch,
            history: core.history,
            chain_head,
            created_by: core.created_by,
            archived_at: self.archived,
        })
    }

    /// A canonical BLAKE3 hash of the whole state (every field, every collection in its order). Two
    /// replicas that applied the same entries have the same hash; a device that replays the chain gets
    /// the hash the node holds.
    ///
    /// Implements planning/architecture.md section 5.9 (a replay of the chain is the state the node
    /// holds; "because the chain authenticates itself, any copy can restore it").
    ///
    /// Caller: the tests `replay_equals_incremental_state_hash`; the node's integrity check (SPACES-APPLY).
    pub fn state_hash(&self) -> Hash32 {
        let mut d = Digest::new();
        d.bytes(self.id.as_bytes());
        d.bytes(self.account.as_bytes());
        match &self.core {
            None => d.byte(0),
            Some(core) => {
                d.byte(1);
                d.word(core.kind.as_str());
                d.word(core.history.as_str());
                d.n32(core.key_epoch.get());
                d.bytes(core.created_by.as_bytes());
                d.stamp(core.created_at);
            }
        }
        d.count(self.history_switches.len());
        for (seq, mode) in &self.history_switches {
            d.n64(seq.get());
            d.word(mode.as_str());
        }
        d.count(self.epochs.len());
        for (epoch, record) in &self.epochs {
            d.n32(epoch.get());
            d.bytes(record.key_id.as_bytes());
            d.n64(record.seq.get());
            d.byte(match record.kind {
                CommitKind::Create => 0,
                CommitKind::Rekey => 1,
            });
            d.bytes(record.signer.as_bytes());
            d.byte(u8::from(record.chained));
        }
        d.count(self.grants.len());
        for (grantee, grant) in &self.grants {
            d.grantee(grantee);
            d.word(grant.role.as_str());
            d.opt_bytes_count(grant.cap_bytes);
            d.word(grant.payer.as_str());
            match grant.source {
                Source::Chain(seq) => {
                    d.byte(0);
                    d.n64(seq.get());
                }
                Source::Share(share) => {
                    d.byte(1);
                    d.bytes(share.as_bytes());
                }
            }
            d.word(grant.state.as_str());
            d.stamp(grant.added_at);
            d.opt_stamp(grant.removed_at);
        }
        d.count(self.windows.len());
        for (grantee, windows) in &self.windows {
            d.grantee(grantee);
            d.count(windows.len());
            for window in windows {
                d.n64(window.from.get());
                match window.until {
                    Some(until) => {
                        d.byte(1);
                        d.n64(until.get());
                    }
                    None => d.byte(0),
                }
            }
        }
        d.count(self.signers.len());
        for (device, actor) in &self.signers {
            d.bytes(device.as_bytes());
            d.bytes(actor.account.as_bytes());
            d.bytes(actor.member.as_bytes());
        }
        match self.rekey_due {
            Some(reason) => {
                d.byte(1);
                d.word(reason.as_str());
            }
            None => d.byte(0),
        }
        d.opt_stamp(self.archived);
        match self.chain.head() {
            Some(head) => {
                d.byte(1);
                d.n64(head.seq.get());
                d.bytes(head.hash.as_bytes());
            }
            None => d.byte(0),
        }
        d.n64(self.chain.epoch().get());
        d.count(self.chain.len());
        for entry in self.chain.entries() {
            d.n64(entry.seq.get());
            d.bytes(entry.prev.as_bytes());
            d.bytes(entry.hash.as_bytes());
            d.stamp(entry.at);
            d.n64(entry.epoch.get());
            d.bytes(entry.signer.as_bytes());
            d.bytes(entry.actor.account.as_bytes());
            d.bytes(entry.actor.member.as_bytes());
            d.byte(entry.authority.tag());
        }
        d.finish()
    }
}

/// The writer of the state hash: BLAKE3 in `derive_key` mode under [`STATE_HASH_DOMAIN`], every field
/// preceded by its length, so no two different states give the same input.
struct Digest(blake3::Hasher);

impl Digest {
    fn new() -> Digest {
        Digest(blake3::Hasher::new_derive_key(STATE_HASH_DOMAIN))
    }

    /// A length-framed byte string.
    fn bytes(&mut self, bytes: &[u8]) {
        self.0
            .update(&ByteCount::of_len(bytes.len()).get().to_le_bytes());
        self.0.update(bytes);
    }

    fn byte(&mut self, byte: u8) {
        self.bytes(&[byte]);
    }

    fn word(&mut self, word: &str) {
        self.bytes(word.as_bytes());
    }

    fn n32(&mut self, number: u32) {
        self.bytes(&number.to_le_bytes());
    }

    fn n64(&mut self, number: u64) {
        self.bytes(&number.to_le_bytes());
    }

    /// The number of items of a collection.
    fn count(&mut self, count: usize) {
        self.n64(ByteCount::of_len(count).get());
    }

    fn stamp(&mut self, at: Stamp) {
        self.n64(at.as_secs());
    }

    fn opt_stamp(&mut self, at: Option<Stamp>) {
        match at {
            Some(at) => {
                self.byte(1);
                self.stamp(at);
            }
            None => self.byte(0),
        }
    }

    fn opt_bytes_count(&mut self, size: Option<ByteCount>) {
        match size {
            Some(size) => {
                self.byte(1);
                self.n64(size.get());
            }
            None => self.byte(0),
        }
    }

    fn grantee(&mut self, grantee: &Grantee) {
        match grantee {
            Grantee::Member(member) => {
                self.byte(0);
                self.bytes(member.as_bytes());
            }
            Grantee::External { account, member } => {
                self.byte(1);
                self.bytes(account.as_bytes());
                self.bytes(member.as_bytes());
            }
            Grantee::Group(group) => {
                self.byte(2);
                self.bytes(group.as_bytes());
            }
            Grantee::Capability(hash) => {
                self.byte(3);
                self.bytes(hash.as_bytes());
            }
        }
    }

    fn finish(self) -> Hash32 {
        Hash32::from_bytes(*self.0.finalize().as_bytes())
    }
}
