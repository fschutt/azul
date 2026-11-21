//! `admit`: THE PERMISSION CHECK of a space chain (planning/architecture.md section 5.9 "Space chains ...
//! signed by its managers", section 5.4 "Spaces", section 5.5 "Epochs must be authorized", section 5.10
//! `external_sharing`, section 6.5 "Members create conversations by policy", section 7.6 "the key agent
//! rotates forward"; section 12 E1 "no entry is accepted unless its signer may sign it"; build unit S16,
//! lane W1-S16b).
//!
//! `admit(state, proposal, who, account, now)` says whether an entry may extend a space's chain. It is a
//! pure function of the state, the proposal (an entry whose signatures hold), the facts about the signer
//! (built by the glue from the team log), the account's view and the leader's stamp `now` (section 2.4):
//! it changes nothing, reads no clock and draws no entropy. The metadata group runs it before it
//! proposes an entry to Raft and again when it applies; every device runs it on every entry it verifies.
//!
//! THE ORDER OF THE CHECKS (the first that fails is the reason):
//!
//! 1. the account view is the space's account (`WrongAccount`); the space is not archived (`Archived`);
//! 2. the chain: the space, the position, `prev`, the leader's stamp, the restore epoch, a fork, and
//!    "the first entry is the `space_create` and nothing but the first entry is" (`Chain`);
//! 3. the signer: the facts are about the signing device, the device is active, its member is active
//!    (`FactsOfAnotherDevice`, `SignerNotActive`, `MemberNotActive`);
//! 4. the op, in this order: the app gate (`AppMayOnlyRekey`), the authority (`NotAManager`,
//!    `QuorumShort`, the creation rules), the arguments (the grantee, the cap, the payer, the limits),
//!    the conditions of the state (`NoSuchGrant`, `LastManager`, `EpochNotNext`, `KeyIdReused`).
//!
//! WHO MAY SIGN WHAT (the v0 table of the brief, its drift item 1; drift row X-DELTA-W1S16B-1):
//!
//! | op | who |
//! |---|---|
//! | `space_create` | the creation rules: an owner for `root` (only while the account has none), an admin for `system_audit` (only when `audit_level` is not `none`), a family member for its own `private_member` space, an admin or (where the account's policy lets members) a member for every other kind; never an app, a guest or an external member |
//! | `space_grant` | a manager (directly, through a group, or as an external manager); an account owner or admin who is not a manager only to add a MANAGER to an ORPHANED space, with the admin quorum |
//! | `space_revoke` | a manager; the grantee itself (leaving); an account owner or admin |
//! | `space_rekey` | a manager (an app principal included: the key agent, section 7.6); nobody else, an admin included |
//! | `space_history` | a manager |
//!
//! Nobody removes the last manager. An app principal signs a rekey and its own leaving, nothing else. A
//! suspended or removed member, and a device that is not active, sign nothing.

use azlin_chain::{ChainRefusal, Placement};
use azlin_policy::{quorum_met, AuditLevel, ExternalSharing, Policy, QuorumClass, Signatory};
use azlin_seams::{
    mutant, AccountId, AccountKind, AccountRole, ByteCount, DeviceId, Epoch, GroupId, KeyId,
    MemberId, MemberKind, Stamp,
};

use crate::apply::Admitted;
use crate::grant::{Grantee, Payer};
use crate::kind::SpaceKind;
use crate::ops::{SpaceOp, SpaceOpName};
use crate::proposal::{SpaceEntry, SpaceProposal};
use crate::role::{role_caps, union, Caps, SpaceRole};
use crate::state::{Actor, Authority, EntryRecord, SpaceState, MAX_CAPABILITIES, MAX_GRANTS};

/// What is known about the device that signed an entry, built by the glue from the team log of the
/// signer's account. The facts are about ONE device and its member, and say which account the member
/// belongs to: an external member's facts name another account, and its `account_role` is its role THERE
/// (it gives no authority here).
///
/// Implements planning/architecture.md section 5.9 (the team log knows the devices and the members) and
/// section 7.2 (account roles). The `account` field is an addition to the brief's struct (drift row
/// X-DELTA-W1S16B-4): without it an external member whose id equals an internal member's could not be
/// told from it.
///
/// Caller: `admit` (the signer), `AccountView::signer_facts` (the cosigners); built by the SPACES-APPLY
/// glue from S17c's team state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignerFacts {
    /// The device.
    pub device: DeviceId,
    /// The member the device belongs to.
    pub member: MemberId,
    /// The account the member belongs to.
    pub account: AccountId,
    /// Whether the member is a person or an app.
    pub member_kind: MemberKind,
    /// Whether the device is active (not pending its hold, not revoked).
    pub device_active: bool,
    /// Whether the member is active (not invited, suspended or removed).
    pub member_active: bool,
    /// The member's role in its own account.
    pub account_role: AccountRole,
}

/// The account, as the space chain needs to see it: its policy, its members and groups, and who may
/// create what. Built by the glue from the team log (S17c's `TeamState`) FOR THE ACCOUNT OF THE SPACE and
/// for the signer: `policy()` is the signer's effective policy (`azlin_policy::effective`: a group may
/// only tighten it).
///
/// Implements planning/architecture.md section 5.9 ("The account log's policy bounds what chains may
/// do"), section 5.10 (`external_sharing`), section 7.2 and section 6.5 (members create conversations by
/// policy). `kind`, `has_group`, `has_root_space` and `signer_facts` are additions to the brief's trait
/// (drift row X-DELTA-W1S16B-4).
///
/// Caller: `admit`; implemented by the SPACES-APPLY glue and, in the tests, by plain structs (an azul
/// crate never names an iso type, COMMON-W1R R5; S46b's `AccountView` is another crate's own trait,
/// R15).
pub trait AccountView {
    /// The account's policy as it applies to the signer.
    fn policy(&self) -> &Policy;

    /// The kind of the account (`private_member` spaces exist only in a family account).
    fn kind(&self) -> AccountKind;

    /// Whether members (who are not admins) may create a space of this kind: the account's policy says
    /// (drift row X-DELTA-W1S16B-5: the plan has no policy field for conversation spaces yet).
    fn members_may_create(&self, kind: SpaceKind) -> bool;

    /// Whether `member` is a member of this account who is active (joined, not suspended or removed).
    fn is_member(&self, member: MemberId) -> bool;

    /// Whether the group exists in this account.
    fn has_group(&self, group: GroupId) -> bool;

    /// Every group the member is in, ancestors included (nesting is resolved by the team log's group
    /// tree: a grant to a group reaches the members of its subgroups).
    fn groups_of(&self, member: MemberId) -> Vec<GroupId>;

    /// Whether `account` is this account.
    fn same_account(&self, account: AccountId) -> bool;

    /// Whether the account already has its root space.
    fn has_root_space(&self) -> bool;

    /// The facts about a device: used for the devices that cosigned an entry (the admin quorum). `None`
    /// for a device the team log does not know.
    fn signer_facts(&self, device: DeviceId) -> Option<SignerFacts>;
}

/// Why an entry is refused (planning/architecture.md section 5.9: the group "rejects an entry with a
/// reason"). The first variants are about the chain and the signer, the rest about the op.
///
/// Implements planning/architecture.md section 5.9 and section 12 E1.
///
/// Caller: `admit`, `verify_entry`, `replay`; the registry glue maps it to a status (the reason words of
/// `azlin-regbase`), S47a maps it to its classes.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SpaceRefusal {
    /// The entry does not continue the chain (a wrong `prev`, a gap, a fork, a stamp that runs backwards,
    /// an entry of the lost timeline of a restore, a second `space_create`, a copy that is not intact).
    #[error("the entry does not continue the space chain: {0}")]
    Chain(ChainRefusal),
    /// The account view is not the space's account.
    #[error("the account view is not the space's account")]
    WrongAccount,
    /// The space is archived: its chain does not change.
    #[error("the space is archived")]
    Archived,
    /// The facts are about another device than the one that signed.
    #[error("the signer facts are about another device than the signer")]
    FactsOfAnotherDevice,
    /// The signer's device is not in the directory.
    #[error("the signer's device is unknown")]
    SignerUnknown,
    /// A signature of the entry does not verify.
    #[error("a signature of the entry does not verify")]
    BadSignature,
    /// The signer's key cannot be read now (a store failure): try again, this is no verdict.
    #[error("the signer's key cannot be read now")]
    KeyUnavailable,
    /// The signer's device is not active: it signs nothing.
    #[error("the signer's device is not active and signs nothing")]
    SignerNotActive,
    /// The signer's member is not active: none of its devices signs anything.
    #[error("the signer's member is not active and signs nothing")]
    MemberNotActive,
    /// A part of the entry is over its limit.
    #[error("{0} is too big")]
    TooBig(&'static str),
    /// The signer does not hold the manage capability the op needs (and no override applies).
    #[error("{op} needs the manage capability of the space")]
    NotAManager {
        /// The op.
        op: SpaceOpName,
    },
    /// An app principal signs a rekey and its own leaving, nothing else.
    #[error("an app principal may only rekey a space or leave it, not {op}")]
    AppMayOnlyRekey {
        /// The op.
        op: SpaceOpName,
    },
    /// Too few different people signed the admin override.
    #[error("quorum short: {have} different people signed, {need} are needed")]
    QuorumShort {
        /// The number of different people among the signatures.
        have: u8,
        /// The number the change needs.
        need: u8,
    },
    /// The signer may not create a space of this kind.
    #[error("the signer may not create a {kind} space")]
    CreateNotAllowed {
        /// The kind.
        kind: SpaceKind,
    },
    /// The account already has its root space.
    #[error("the account already has its root space")]
    RootExists,
    /// A private member space exists only in a family account.
    #[error("a private member space exists only in a family account")]
    PrivateSpaceNeedsFamily,
    /// The account's `audit_level` is `none`: there is no audit space to create.
    #[error("the account's audit_level is none: there is no audit space")]
    AuditLevelNone,
    /// The first manager is no active member of the account.
    #[error("the first manager is no active member of the account")]
    FirstManagerUnknown,
    /// A member grantee is no active member of the account.
    #[error("the grantee is no active member of the account")]
    GranteeNotAMember,
    /// A group grantee does not exist in the account.
    #[error("the grantee's group does not exist")]
    GranteeGroupUnknown,
    /// An external grantee names this very account (or one of its members).
    #[error("an external grantee must be a member of another account")]
    ExternalIsInternal,
    /// The signer's policy does not allow external members.
    #[error("the external_sharing policy forbids external members")]
    ExternalSharingForbidden,
    /// A capability (a bearer secret) cannot be a manager.
    #[error("a capability grant cannot be a manager")]
    CapabilityCannotManage,
    /// A capability has no account of its own: the owner pays.
    #[error("a capability grant is paid by the owner")]
    CapabilityPaidByOwner,
    /// A cap on writes needs a role that writes.
    #[error("a cap needs a role that writes")]
    CapNeedsWriteRole,
    /// A cap of no bytes is no cap: it would be a viewer.
    #[error("a cap of no bytes is no cap")]
    CapIsZero,
    /// The space holds as many grant rows as it may.
    #[error("the space holds the most grants it may")]
    TooManyGrants,
    /// The space holds as many capability grants as it may.
    #[error("the space holds the most capability grants it may")]
    TooManyCapabilities,
    /// The grantee holds no grant in force.
    #[error("the grantee holds no grant in force")]
    NoSuchGrant,
    /// The change would leave the space without a manager.
    #[error("the change would leave the space without a manager")]
    LastManager,
    /// A rekey must commit the next epoch.
    #[error("the new epoch must be {expected}, not {got}")]
    EpochNotNext {
        /// The epoch that is next.
        expected: Epoch,
        /// The epoch the entry commits.
        got: Epoch,
    },
    /// A key id that an earlier epoch already committed.
    #[error("the key id was committed by an earlier epoch")]
    KeyIdReused,
    /// The epoch counter is at its largest number.
    #[error("the epoch counter is at its largest number")]
    EpochExhausted,
}

impl From<ChainRefusal> for SpaceRefusal {
    fn from(why: ChainRefusal) -> SpaceRefusal {
        SpaceRefusal::Chain(why)
    }
}

/// The link rules of the chain (`azlin_chain::Chain::check`) and the rule of the first entry.
fn check_chain(state: &SpaceState, record: &EntryRecord) -> Result<(), SpaceRefusal> {
    match state.chain.check(record) {
        Ok(Placement::Next) => {}
        // An entry the chain already holds does not extend it: it is a gap, as the team log reads it.
        Ok(Placement::Held { .. }) => {
            return Err(match state.chain.tip().next_seq() {
                Some(expected) => SpaceRefusal::Chain(ChainRefusal::Gap {
                    expected,
                    got: record.seq,
                }),
                None => SpaceRefusal::Chain(ChainRefusal::Full),
            });
        }
        Err(_) if mutant!("spaces_chain_unchecked") => {}
        Err(why) => return Err(SpaceRefusal::Chain(why)),
    }
    let is_create = matches!(record.op, SpaceOp::Create { .. });
    match (state.chain.is_empty(), is_create) {
        (true, true) | (false, false) => Ok(()),
        (true, false) => Err(SpaceRefusal::Chain(ChainRefusal::NotGenesis)),
        (false, true) => {
            if mutant!("spaces_second_create_allowed") {
                return Ok(());
            }
            match state.chain.entries().first() {
                Some(genesis) => Err(SpaceRefusal::Chain(ChainRefusal::GenesisAgain {
                    have: genesis.hash,
                    got: record.hash,
                })),
                None => Err(SpaceRefusal::Chain(ChainRefusal::NotGenesis)),
            }
        }
    }
}

/// The signer's own state: the facts are about the signing device, the device is active and its member is
/// active (an internal member must be an active member in the account's view too).
fn check_signer(
    who: &SignerFacts,
    e: &SpaceEntry,
    account: &dyn AccountView,
) -> Result<(), SpaceRefusal> {
    if who.device != e.signer && !mutant!("spaces_facts_device_unchecked") {
        return Err(SpaceRefusal::FactsOfAnotherDevice);
    }
    if !who.device_active && !mutant!("spaces_device_active_unchecked") {
        return Err(SpaceRefusal::SignerNotActive);
    }
    if !who.member_active && !mutant!("spaces_member_active_unchecked") {
        return Err(SpaceRefusal::MemberNotActive);
    }
    if account.same_account(who.account)
        && !account.is_member(who.member)
        && !mutant!("spaces_member_active_unchecked")
    {
        return Err(SpaceRefusal::MemberNotActive);
    }
    Ok(())
}

/// Adds the capabilities of the grantee's grant, if it is in force, to `caps`.
fn add_caps(state: &SpaceState, grantee: &Grantee, caps: &mut Caps) {
    if let Some(grant) = state.grants.get(grantee) {
        if grant.is_active() || mutant!("spaces_standing_uses_revoked") {
            *caps = union(*caps, role_caps(grant.role));
        }
    }
}

/// What the signer's grants give it in this space now: its direct grant and the grants of its groups
/// (an internal member), or its external grant (a member of another account).
fn standing(
    state: &SpaceState,
    who: &SignerFacts,
    account: &dyn AccountView,
    internal: bool,
) -> Caps {
    let mut caps = Caps::NONE;
    if internal {
        add_caps(state, &Grantee::Member(who.member), &mut caps);
        if !mutant!("spaces_standing_ignores_groups") {
            for group in account.groups_of(who.member) {
                add_caps(state, &Grantee::Group(group), &mut caps);
            }
        }
    } else {
        add_caps(
            state,
            &Grantee::External {
                account: who.account,
                member: who.member,
            },
            &mut caps,
        );
        if mutant!("spaces_external_confused_with_member") {
            add_caps(state, &Grantee::Member(who.member), &mut caps);
        }
    }
    caps
}

/// What `admit` checks an op with.
struct Ctx<'a> {
    state: &'a SpaceState,
    who: &'a SignerFacts,
    account: &'a dyn AccountView,
    /// Whether the signer is a member of the space's own account.
    internal: bool,
    /// What the signer's grants give it now.
    caps: Caps,
}

/// What an op decided: the authority the signer signed under and the members whose cosignature counted.
type Decision = (Authority, Vec<MemberId>);

impl Ctx<'_> {
    /// Whether the signer is an account owner or admin of THIS account.
    fn is_admin(&self) -> bool {
        matches!(
            self.who.account_role,
            AccountRole::Owner | AccountRole::Admin
        )
    }

    /// Whether a grant of this grantee is a manager that is really there: a member that is an active
    /// member of the account, a group that exists, an external member (whose standing in its own account
    /// this account cannot see). A capability is never a manager.
    fn viable(&self, grantee: &Grantee) -> bool {
        match grantee {
            Grantee::Member(member) => self.account.is_member(*member),
            Grantee::Group(group) => self.account.has_group(*group),
            Grantee::External { .. } => true,
            Grantee::Capability(_) => false,
        }
    }

    /// Whether some manager grant other than `except`'s is in force and viable.
    fn other_viable_manager(&self, except: &Grantee) -> bool {
        self.state.grants.iter().any(|(grantee, grant)| {
            grantee != except
                && grant.is_active()
                && grant.role == SpaceRole::Manager
                && self.viable(grantee)
        })
    }

    /// Whether the space has no manager that is really there: an account owner or admin may then add a
    /// manager, with the admin quorum.
    fn orphaned(&self) -> bool {
        !self.state.grants.iter().any(|(grantee, grant)| {
            grant.is_active() && grant.role == SpaceRole::Manager && self.viable(grantee)
        })
    }

    /// Whether the grant of `grantee` is the signer's own.
    fn is_own(&self, grantee: &Grantee) -> bool {
        match grantee {
            Grantee::Member(member) => self.internal && *member == self.who.member,
            Grantee::External { account, member } => {
                !self.internal && *account == self.who.account && *member == self.who.member
            }
            Grantee::Group(_) | Grantee::Capability(_) => false,
        }
    }

    /// An app principal signs a rekey and its own leaving, nothing else.
    fn app_gate(&self, op: SpaceOpName, own_grant: bool) -> Result<(), SpaceRefusal> {
        if self.who.member_kind != MemberKind::App || mutant!("spaces_app_any_op") {
            return Ok(());
        }
        match op {
            SpaceOpName::Rekey => Ok(()),
            SpaceOpName::Revoke if own_grant => Ok(()),
            SpaceOpName::Create
            | SpaceOpName::Grant
            | SpaceOpName::Revoke
            | SpaceOpName::History => Err(SpaceRefusal::AppMayOnlyRekey { op }),
        }
    }

    /// The admin quorum of an override: the signer and the cosigners whose cosignature counts are
    /// different people; a cosigner counts only if it is an active owner or admin of this account (an
    /// app never counts). Returns the members of the cosigners that counted.
    fn admin_quorum(&self, p: &SpaceProposal) -> Result<Vec<MemberId>, SpaceRefusal> {
        let mut signatories = vec![Signatory {
            member: self.who.member,
            kind: self.who.member_kind,
        }];
        let mut counted = Vec::new();
        for device in p.cosigners() {
            let Some(facts) = self.account.signer_facts(*device) else {
                continue;
            };
            let admin_of_this_account = self.account.same_account(facts.account)
                && facts.device == *device
                && facts.device_active
                && facts.member_active
                && matches!(facts.account_role, AccountRole::Owner | AccountRole::Admin);
            if !admin_of_this_account && !mutant!("spaces_cosigner_role_unchecked") {
                continue;
            }
            signatories.push(Signatory {
                member: facts.member,
                kind: facts.member_kind,
            });
            if facts.member_kind == MemberKind::Person
                && facts.member != self.who.member
                && !counted.contains(&facts.member)
            {
                counted.push(facts.member);
            }
        }
        if mutant!("spaces_override_quorum_skipped") {
            return Ok(counted);
        }
        quorum_met(
            self.account.policy(),
            QuorumClass::AdminChange,
            &signatories,
        )
        .map_err(|short| SpaceRefusal::QuorumShort {
            have: short.have,
            need: short.need,
        })?;
        Ok(counted)
    }

    /// `space_create`: the creation rules.
    fn create(&self, kind: SpaceKind, first_manager: MemberId) -> Result<Decision, SpaceRefusal> {
        self.app_gate(SpaceOpName::Create, false)?;
        let not_allowed = SpaceRefusal::CreateNotAllowed { kind };
        if !self.internal {
            return Err(not_allowed);
        }
        let role = self.who.account_role;
        let may_create = match kind {
            SpaceKind::Root => role == AccountRole::Owner,
            SpaceKind::SystemAudit => self.is_admin(),
            SpaceKind::Shared
            | SpaceKind::Team
            | SpaceKind::PrivateMember
            | SpaceKind::Inbox
            | SpaceKind::Conversation => {
                self.is_admin()
                    || (role != AccountRole::Guest && self.account.members_may_create(kind))
            }
        };
        if !may_create && !mutant!("spaces_creation_role_unchecked") {
            return Err(not_allowed);
        }
        match kind {
            SpaceKind::Root => {
                if self.account.has_root_space() && !mutant!("spaces_root_anywhere") {
                    return Err(SpaceRefusal::RootExists);
                }
            }
            SpaceKind::SystemAudit => {
                if self.account.policy().audit_level == AuditLevel::None
                    && !mutant!("spaces_audit_space_unchecked")
                {
                    return Err(SpaceRefusal::AuditLevelNone);
                }
            }
            SpaceKind::PrivateMember => {
                if self.account.kind() != AccountKind::Family
                    && !mutant!("spaces_private_any_account")
                {
                    return Err(SpaceRefusal::PrivateSpaceNeedsFamily);
                }
                // A private space is its member's own.
                if first_manager != self.who.member {
                    return Err(not_allowed);
                }
            }
            SpaceKind::Shared | SpaceKind::Team | SpaceKind::Inbox | SpaceKind::Conversation => {}
        }
        if !self.account.is_member(first_manager) && !mutant!("spaces_first_manager_unchecked") {
            return Err(SpaceRefusal::FirstManagerUnknown);
        }
        Ok((Authority::Creator, Vec::new()))
    }

    /// `space_grant`: a manager signs it; an account owner or admin who is not a manager only adds a
    /// manager to an orphaned space, with the admin quorum.
    fn grant(
        &self,
        p: &SpaceProposal,
        grantee: Grantee,
        role: SpaceRole,
        cap_bytes: Option<ByteCount>,
        payer: Payer,
    ) -> Result<Decision, SpaceRefusal> {
        self.app_gate(SpaceOpName::Grant, false)?;
        let (authority, cosigners) =
            if self.caps.can_manage() || mutant!("spaces_grant_needs_no_manage") {
                (Authority::Manager, Vec::new())
            } else if self.may_override(role) {
                (Authority::AdminOverride, self.admin_quorum(p)?)
            } else {
                return Err(SpaceRefusal::NotAManager {
                    op: SpaceOpName::Grant,
                });
            };
        self.check_grant(&grantee, role, cap_bytes, payer)?;
        Ok((authority, cosigners))
    }

    /// Whether an account owner or admin who is not a manager may sign a grant of `role` (to be decided
    /// by the quorum): an internal owner or admin, adding a manager, to a space that has none.
    fn may_override(&self, role: SpaceRole) -> bool {
        let internal = self.internal || mutant!("spaces_override_for_external");
        let admin = self.is_admin();
        let adds_manager = role == SpaceRole::Manager || mutant!("spaces_override_any_role");
        let orphaned = self.orphaned() || mutant!("spaces_override_not_orphaned");
        internal && admin && adds_manager && orphaned
    }

    /// The arguments of a grant: the grantee, the cap, the payer, the limits and the last manager.
    fn check_grant(
        &self,
        grantee: &Grantee,
        role: SpaceRole,
        cap_bytes: Option<ByteCount>,
        payer: Payer,
    ) -> Result<(), SpaceRefusal> {
        match grantee {
            Grantee::Member(member) => {
                if !self.account.is_member(*member) && !mutant!("spaces_grantee_member_unchecked") {
                    return Err(SpaceRefusal::GranteeNotAMember);
                }
            }
            Grantee::External { account, member } => {
                if (self.account.same_account(*account) || self.account.is_member(*member))
                    && !mutant!("spaces_external_internal_unchecked")
                {
                    return Err(SpaceRefusal::ExternalIsInternal);
                }
                if self.account.policy().external_sharing != ExternalSharing::Allowed
                    && !mutant!("spaces_external_policy_unchecked")
                {
                    return Err(SpaceRefusal::ExternalSharingForbidden);
                }
            }
            Grantee::Group(group) => {
                if !self.account.has_group(*group) && !mutant!("spaces_group_unchecked") {
                    return Err(SpaceRefusal::GranteeGroupUnknown);
                }
            }
            Grantee::Capability(_) => {
                if role == SpaceRole::Manager && !mutant!("spaces_capability_manager_allowed") {
                    return Err(SpaceRefusal::CapabilityCannotManage);
                }
                if payer != Payer::Owner && !mutant!("spaces_capability_paid_by_grantee") {
                    return Err(SpaceRefusal::CapabilityPaidByOwner);
                }
            }
        }
        if let Some(cap) = cap_bytes {
            if !role_caps(role).can_write() {
                return Err(SpaceRefusal::CapNeedsWriteRole);
            }
            if cap.is_zero() {
                return Err(SpaceRefusal::CapIsZero);
            }
        }
        let existing = self.state.grants.get(grantee);
        if existing.is_none() && !mutant!("spaces_grants_limit_unchecked") {
            if self.state.grants.len() >= MAX_GRANTS {
                return Err(SpaceRefusal::TooManyGrants);
            }
            if matches!(grantee, Grantee::Capability(_)) {
                let capabilities = self
                    .state
                    .grants
                    .keys()
                    .filter(|key| matches!(key, Grantee::Capability(_)))
                    .count();
                if capabilities >= MAX_CAPABILITIES {
                    return Err(SpaceRefusal::TooManyCapabilities);
                }
            }
        }
        // Replacing the last manager's grant with a lesser role removes the last manager as surely as a
        // revoke does.
        if let Some(held) = existing {
            if held.is_active()
                && held.role == SpaceRole::Manager
                && role != SpaceRole::Manager
                && self.viable(grantee)
                && !self.other_viable_manager(grantee)
                && !mutant!("spaces_last_manager_check_off")
            {
                return Err(SpaceRefusal::LastManager);
            }
        }
        Ok(())
    }

    /// `space_revoke`: a manager, the grantee itself, or an account owner or admin.
    fn revoke(&self, grantee: Grantee) -> Result<Decision, SpaceRefusal> {
        let own = self.is_own(&grantee);
        self.app_gate(SpaceOpName::Revoke, own)?;
        let authority = if self.caps.can_manage() || mutant!("spaces_revoke_needs_no_authority") {
            Authority::Manager
        } else if own || mutant!("spaces_leave_any_grantee") {
            Authority::SelfLeave
        } else if self.internal && self.is_admin() {
            Authority::AdminRevoke
        } else {
            return Err(SpaceRefusal::NotAManager {
                op: SpaceOpName::Revoke,
            });
        };
        let Some(target) = self
            .state
            .grants
            .get(&grantee)
            .filter(|grant| grant.is_active())
        else {
            return Err(SpaceRefusal::NoSuchGrant);
        };
        if target.role == SpaceRole::Manager
            && self.viable(&grantee)
            && !self.other_viable_manager(&grantee)
            && !mutant!("spaces_last_manager_check_off")
        {
            return Err(SpaceRefusal::LastManager);
        }
        Ok((authority, Vec::new()))
    }

    /// `space_rekey`: a manager (an app principal included), the next epoch, a key id never used.
    fn rekey(&self, epoch: Epoch, key_id: KeyId) -> Result<Decision, SpaceRefusal> {
        self.app_gate(SpaceOpName::Rekey, false)?;
        let admin_override = mutant!("spaces_rekey_admin_override") && self.is_admin();
        if !self.caps.can_manage() && !admin_override && !mutant!("spaces_rekey_needs_no_manage") {
            return Err(SpaceRefusal::NotAManager {
                op: SpaceOpName::Rekey,
            });
        }
        let Some(core) = self.state.core.as_ref() else {
            return Err(SpaceRefusal::Chain(ChainRefusal::NotGenesis));
        };
        let Some(expected) = core.key_epoch.next() else {
            return Err(SpaceRefusal::EpochExhausted);
        };
        if epoch != expected && !mutant!("spaces_rekey_epoch_unchecked") {
            return Err(SpaceRefusal::EpochNotNext {
                expected,
                got: epoch,
            });
        }
        if self
            .state
            .epochs
            .values()
            .any(|record| record.key_id == key_id)
            && !mutant!("spaces_rekey_key_reuse_allowed")
        {
            return Err(SpaceRefusal::KeyIdReused);
        }
        Ok((Authority::Manager, Vec::new()))
    }

    /// `space_history`: a manager.
    fn history(&self) -> Result<Decision, SpaceRefusal> {
        self.app_gate(SpaceOpName::History, false)?;
        if !self.caps.can_manage() && !mutant!("spaces_history_needs_no_manage") {
            return Err(SpaceRefusal::NotAManager {
                op: SpaceOpName::History,
            });
        }
        Ok((Authority::Manager, Vec::new()))
    }
}

/// Whether an entry may extend the space's chain, and the witness that says so. See the module for the
/// order of the checks and the table of who may sign what.
///
/// `who` is the facts about the signing device, `account` the account's view (built for the space's
/// account and for the signer), `now` the leader's stamp of the entry (planning/architecture.md section
/// 2.4: time is the leader's stamp). `Ok` carries the `Admitted` witness that `apply` takes; it changes
/// nothing.
///
/// Implements planning/architecture.md section 5.9 ("signed by its managers"; "The account log's policy
/// bounds what chains may do (for example, external members only if `external_sharing` allows)"),
/// section 5.5 (a rekey commits the next epoch's key id) and section 12 E1.
///
/// Caller: the node's apply path (SPACES-APPLY, before the entry is proposed to Raft and again when it
/// applies) and every device that verifies a chain (`replay`, CHAIN-GLUE).
pub fn admit(
    state: &SpaceState,
    p: &SpaceProposal,
    who: &SignerFacts,
    account: &dyn AccountView,
    now: Stamp,
) -> Result<Admitted, SpaceRefusal> {
    let e = p.entry();
    if !account.same_account(state.account) && !mutant!("spaces_account_view_unchecked") {
        return Err(SpaceRefusal::WrongAccount);
    }
    if state.archived.is_some() && !mutant!("spaces_archived_unchecked") {
        return Err(SpaceRefusal::Archived);
    }
    let mut record = EntryRecord {
        space: e.space,
        seq: e.seq,
        prev: e.prev,
        hash: e.hash,
        at: now,
        epoch: e.epoch,
        signer: e.signer,
        actor: Actor {
            account: who.account,
            member: who.member,
        },
        op: e.op.clone(),
        authority: Authority::Manager,
    };
    check_chain(state, &record)?;
    check_signer(who, e, account)?;
    let internal = account.same_account(who.account);
    let ctx = Ctx {
        state,
        who,
        account,
        internal,
        caps: standing(state, who, account, internal),
    };
    let (authority, cosigners) = match &e.op {
        SpaceOp::Create {
            kind,
            first_manager,
            ..
        } => ctx.create(*kind, *first_manager)?,
        SpaceOp::Grant {
            grantee,
            role,
            cap_bytes,
            payer,
        } => ctx.grant(p, *grantee, *role, *cap_bytes, *payer)?,
        SpaceOp::Revoke { grantee } => ctx.revoke(*grantee)?,
        SpaceOp::Rekey { epoch, key_id, .. } => ctx.rekey(*epoch, *key_id)?,
        SpaceOp::History { .. } => ctx.history()?,
    };
    record.authority = authority;
    Ok(Admitted::new(record, cosigners))
}
