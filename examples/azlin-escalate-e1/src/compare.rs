//! The comparison: the model, the production check as the metadata group runs it (`verify_signatures` +
//! `admit` + `apply`) and a device's own replay (`replay`), run on the same entries and demanded to agree
//! (planning/architecture.md section 12 E1: "devices run their own check, and the test runs both"; lane
//! W1-S47a). This is the only file that sees both implementations.
//!
//! * [`Rig`] holds the three: the model's state, the production's `TeamState` and the log of admitted entries
//!   a device would replay. [`Rig::step`] offers one entry at one leader stamp to all three and reports every
//!   disagreement.
//! * The converter turns a [`ModelEntry`] into the bytes the production reads: ids and keys from numbers
//!   (`universe.rs`), the preimage `signing_input(TEAM_LOG_DOMAIN, bytes)`, signatures by the fake scheme.
//! * [`class_of`] is THE REVIEWED TABLE that maps every production refusal to the model's class.
//! * [`differential`] runs a seeded sequence; [`run_steps`] runs a recorded one; [`shrink`] reduces a failing
//!   one with `azlin_seams::sim::minimize`.
//!
//! WHAT AGREEMENT MEANS. The verdicts agree when both admit, or both refuse and the production's class is one
//! the model found (the plan gives no order of checks, `model.rs`): when the model finds exactly one class the
//! production's class must be that class. The replay refuses with exactly the production's reason and, when it
//! admits, ends in exactly the production's state (`StateView` after every admitted entry; the `state_hash` of
//! the incremental state and of `replay` from the first entry at the end of a sequence).

use std::collections::BTreeMap;

use azlin_chain::ChainRefusal;
use azlin_policy::{
    AdminQuorum, AuditLevel, DeviceApproval, ExternalSharing, ManagedDefault, PaddingPolicy,
    PolicyPatch, UpdatePolicy,
};
use azlin_seams::sim::{minimize, Rng};
use azlin_seams::{
    signing_input, AccountId, AccountKind, AccountRole, ByteCount, ChainSeq, DeviceId, FakeScheme,
    GroupId, Hash32, KeyId, MemberId, MemberKind, RestoreEpoch, Stamp,
};
use azlin_teamlog::{
    admit, apply, entry_hash, invite_hash, replay, verify_signatures, Admitted, AppName, AppSpec,
    AssertionVerdict, EntitlementState, EntitlementVersion, EscrowReason, InviteSecret, Op, OpName,
    PolicyReason, PolicyScope, PublicKeys, RawEntry, Refusal, Signer, TeamState, TrustedKeys,
    TEAM_LOG_DOMAIN,
};

use crate::gen::{gen_entry, pick_kind, pick_mode, Generated, Mode, NearMiss, Setup, Step};
use crate::model::{
    model_apply, model_violations, resolved_epoch, resolved_seq, AcctKind, AppId, Class,
    CosigClaim, Dev, DevApproval, ExtSharing as MExt, Grp, Key, MKind, MOp, MPatch, MSigner, Mem,
    ModelEntry, ModelState, OpKind, PScope, Planted, PrevSpec, Reason, Role, SigClaim, Verdict,
};
use crate::project::{policy_of, project_model, project_prod, StateView};
use crate::universe::{
    account_bytes, between, blob_key_bytes, delegate_key_bytes, dev_bytes, device_key_bytes,
    gen_universe, grp_bytes, mem_bytes, member_key_bytes, payments_key_bytes, pct,
    stranger_key_bytes, Universe,
};

/// The reviewed table: the model's class of a production refusal. Every variant of `Refusal` has exactly
/// one class (the `match` is exhaustive, so a new refusal does not compile until it is classed here); the
/// two refusals that carry a reason are split by it: `Chain(Corrupt)` (a copy whose hash or signature fails,
/// "fetched again from another voter", planning/architecture.md section 5.9) is a `Signature` failure, not a
/// chain failure, and `PolicyForbids(Loosens)` is `PolicyLoosens` while every other policy reason
/// (`device_approval` self, the most devices, an entitlement that is not newer, a member that is not
/// managed) is `PolicyForbids`.
///
/// Implements planning/architecture.md section 5.9 ("The metadata group rejects an entry with a reason")
/// and section 12 E1; Caller: `Rig::step`, the tests (the table is exercised variant by variant).
pub fn class_of(r: &Refusal) -> Class {
    match r {
        Refusal::Chain(ChainRefusal::Corrupt) => Class::Signature,
        Refusal::Chain(_) => Class::Chain,
        Refusal::BadSignature => Class::Signature,
        Refusal::SignerUnknown => Class::SignerNotActive,
        Refusal::SignerNotActive { .. } => Class::SignerNotActive,
        Refusal::MemberNotActive { .. } => Class::SignerNotActive,
        Refusal::RoleMaySignNo { .. } => Class::RoleOrScope,
        Refusal::OutOfScope { .. } => Class::RoleOrScope,
        Refusal::QuorumShort { .. } => Class::QuorumShort,
        Refusal::AssertionMissing => Class::AssertionMissing,
        Refusal::PolicyForbids(PolicyReason::Loosens(_)) => Class::PolicyLoosens,
        Refusal::PolicyForbids(_) => Class::PolicyForbids,
        Refusal::LastOwner => Class::LastOwner,
        Refusal::GroupCycle => Class::GroupCycle,
        Refusal::DelegationDoesNotCover { .. } => Class::DelegationDoesNotCover,
        Refusal::DelegationExpired => Class::DelegationExpired,
        Refusal::InviteInvalid => Class::InviteInvalid,
        Refusal::UnknownOp => Class::UnknownOp,
        Refusal::SubjectUnknown => Class::SubjectUnknown,
        Refusal::ConsentMissing => Class::ConsentMissing,
        Refusal::Invalid(_) => Class::Invalid,
    }
}

// ---------------------------------------------------------------------------------------------------
// The converter: model ids, keys and ops to the bytes the production reads.
// ---------------------------------------------------------------------------------------------------

fn mid(m: Mem) -> MemberId {
    MemberId::from_bytes(mem_bytes(m))
}

fn did(d: Dev) -> DeviceId {
    DeviceId::from_bytes(dev_bytes(d))
}

fn gid(g: Grp) -> GroupId {
    GroupId::from_bytes(grp_bytes(g))
}

fn kid(k: Key) -> KeyId {
    KeyId::from_bytes(delegate_key_bytes(k))
}

fn account_kind(k: AcctKind) -> AccountKind {
    match k {
        AcctKind::Personal => AccountKind::Personal,
        AcctKind::Family => AccountKind::Family,
        AcctKind::Team => AccountKind::Team,
        AcctKind::Business => AccountKind::Business,
    }
}

fn arole(r: Role) -> AccountRole {
    match r {
        Role::Owner => AccountRole::Owner,
        Role::Admin => AccountRole::Admin,
        Role::GroupAdmin => AccountRole::GroupAdmin,
        Role::Helpdesk => AccountRole::Helpdesk,
        Role::Member => AccountRole::Member,
        Role::Guest => AccountRole::Guest,
    }
}

fn mkind(k: MKind) -> MemberKind {
    match k {
        MKind::Person => MemberKind::Person,
        MKind::App => MemberKind::App,
    }
}

fn opname(k: OpKind) -> OpName {
    match k {
        OpKind::Genesis => OpName::Genesis,
        OpKind::DeviceAdd => OpName::DeviceAdd,
        OpKind::DeviceRemove => OpName::DeviceRemove,
        OpKind::MemberInvite => OpName::MemberInvite,
        OpKind::MemberJoin => OpName::MemberJoin,
        OpKind::MemberRole => OpName::MemberRole,
        OpKind::MemberSuspend => OpName::MemberSuspend,
        OpKind::MemberRemove => OpName::MemberRemove,
        OpKind::MemberManaged => OpName::MemberManaged,
        OpKind::GroupCreate => OpName::GroupCreate,
        OpKind::GroupMember => OpName::GroupMember,
        OpKind::GroupMove => OpName::GroupMove,
        OpKind::PolicySet => OpName::PolicySet,
        OpKind::EscrowSet => OpName::EscrowSet,
        OpKind::AuditKeySet => OpName::AuditKeySet,
        OpKind::AdminQuorumSet => OpName::AdminQuorumSet,
        OpKind::EscrowOpen => OpName::EscrowOpen,
        OpKind::Delegate => OpName::Delegate,
        OpKind::Entitlement => OpName::Entitlement,
        OpKind::App => OpName::App,
    }
}

fn ereason(r: Reason) -> EscrowReason {
    match r {
        Reason::LeaverHandover => EscrowReason::LeaverHandover,
        Reason::LegalRequest => EscrowReason::LegalRequest,
        Reason::Other => EscrowReason::Other,
    }
}

fn dev_keys(d: Dev) -> PublicKeys {
    PublicKeys::classic(device_key_bytes(d), [0x4B; 32])
}

fn mem_keys(m: Mem) -> PublicKeys {
    PublicKeys::classic(member_key_bytes(m), [0x4C; 32])
}

/// The 32 bytes of an invite secret: its number and a fixed fill.
fn secret_bytes(n: u32) -> [u8; 32] {
    let [a, b, c, d] = n.to_be_bytes();
    core::array::from_fn(|i| match i {
        0 => a,
        1 => b,
        2 => c,
        3 => d,
        _ => 0x33,
    })
}

fn app_name(a: AppId) -> Result<AppName, String> {
    AppName::new(&format!("app{}", a.0)).map_err(|e| e.to_string())
}

fn audit_from(n: u8) -> AuditLevel {
    match n {
        0 => AuditLevel::None,
        1 => AuditLevel::Admin,
        2 => AuditLevel::AccessSummary,
        _ => AuditLevel::AccessFull,
    }
}

fn updates_from(n: u8) -> UpdatePolicy {
    match n {
        0 => UpdatePolicy::Auto,
        1 => UpdatePolicy::Notify,
        _ => UpdatePolicy::ManagedByMdm,
    }
}

fn padding_from(n: u8) -> PaddingPolicy {
    match n {
        0 => PaddingPolicy::None,
        1 => PaddingPolicy::Buckets,
        _ => PaddingPolicy::Full,
    }
}

fn to_patch(p: &MPatch) -> Result<PolicyPatch, String> {
    let admin_quorum = match p.admin_quorum {
        Some(q) => Some(AdminQuorum::new(q).map_err(|e| e.to_string())?),
        None => None,
    };
    Ok(PolicyPatch {
        credential_ttl: p.credential_ttl,
        device_approval: p.device_approval.map(|a| match a {
            DevApproval::SelfOnly => DeviceApproval::SelfOnly,
            DevApproval::SelfOrQuorum => DeviceApproval::SelfOrQuorum,
        }),
        max_devices_per_member: p.max_devices,
        managed_default: p.managed_default.map(|b| {
            if b {
                ManagedDefault::Yes
            } else {
                ManagedDefault::No
            }
        }),
        sso_required: p.sso_required,
        enroll_hold: p.enroll_hold,
        passkey_recovery: p.passkey_recovery,
        escrow_open_wait: p.escrow_open_wait,
        audit_level: p.audit_level.map(audit_from),
        admin_quorum,
        browser_devices: p.browser_devices,
        external_sharing: p.external_sharing.map(|e| match e {
            MExt::Allowed => ExternalSharing::Allowed,
            MExt::SameWorkspaceOnly => ExternalSharing::SameWorkspaceOnly,
        }),
        updates: p.updates.map(updates_from),
        padding: p.padding.map(padding_from),
        ..PolicyPatch::default()
    })
}

/// The production op of a model op; `None` for a wire word that is no op.
fn to_op(op: &MOp) -> Result<Option<Op>, String> {
    Ok(Some(match op {
        MOp::Genesis { owner, device } => Op::Genesis {
            owner: mid(*owner),
            device: did(*device),
            member_keys: mem_keys(*owner),
            device_keys: dev_keys(*device),
        },
        MOp::DeviceAdd { member, device } => Op::DeviceAdd {
            member: mid(*member),
            device: did(*device),
            keys: dev_keys(*device),
        },
        MOp::DeviceRemove { device } => Op::DeviceRemove {
            device: did(*device),
        },
        MOp::MemberInvite {
            member,
            kind,
            role,
            secret,
            groups,
            app,
        } => {
            let app = match app {
                Some(a) => {
                    let mut scopes = std::collections::BTreeSet::new();
                    for name in &a.scopes {
                        scopes.insert(app_name(*name)?);
                    }
                    Some(AppSpec {
                        scopes,
                        expires_at: a.expires,
                    })
                }
                None => None,
            };
            Op::MemberInvite {
                member: mid(*member),
                kind: mkind(*kind),
                role: arole(*role),
                invite_hash: invite_hash(&InviteSecret(secret_bytes(*secret))),
                groups: groups.iter().map(|g| gid(*g)).collect(),
                app,
            }
        }
        MOp::MemberJoin {
            member,
            device,
            secret,
        } => Op::MemberJoin {
            member: mid(*member),
            device: did(*device),
            member_keys: mem_keys(*member),
            device_keys: dev_keys(*device),
            secret: InviteSecret(secret_bytes(*secret)),
        },
        MOp::MemberRole {
            member,
            role,
            scope,
        } => Op::MemberRole {
            member: mid(*member),
            role: arole(*role),
            admin_scope: scope.map(gid),
        },
        MOp::MemberSuspend { member } => Op::MemberSuspend {
            member: mid(*member),
        },
        MOp::MemberRemove { member } => Op::MemberRemove {
            member: mid(*member),
        },
        MOp::MemberManaged { member, managed } => Op::MemberManaged {
            member: mid(*member),
            managed: *managed,
        },
        MOp::GroupCreate { group, parent } => Op::GroupCreate {
            group: gid(*group),
            parent: parent.map(gid),
        },
        MOp::GroupMember {
            group,
            member,
            present,
        } => Op::GroupMember {
            group: gid(*group),
            member: mid(*member),
            present: *present,
        },
        MOp::GroupMove { group, new_parent } => Op::GroupMove {
            group: gid(*group),
            new_parent: gid(*new_parent),
        },
        MOp::PolicySet { scope, patch } => Op::PolicySet {
            scope: match scope {
                PScope::Account => PolicyScope::Account,
                PScope::Group(g) => PolicyScope::Group(gid(*g)),
            },
            patch: to_patch(patch)?,
        },
        MOp::EscrowSet { key, holders } => Op::EscrowSet {
            escrow_pub: blob_key_bytes(*key),
            holders: holders.iter().map(|m| mid(*m)).collect(),
        },
        MOp::AuditKeySet { key } => Op::AuditKeySet {
            audit_pub: blob_key_bytes(*key),
        },
        MOp::AdminQuorumSet { quorum } => Op::AdminQuorumSet {
            quorum: AdminQuorum::new(*quorum).map_err(|e| e.to_string())?,
        },
        MOp::EscrowOpen { member, reason } => Op::EscrowOpen {
            member: mid(*member),
            reason: ereason(*reason),
        },
        MOp::Delegate { key, ops, expires } => Op::Delegate {
            key: kid(*key),
            ops: ops.iter().map(|k| opname(*k)).collect(),
            expires: *expires,
        },
        MOp::Entitlement {
            version,
            active,
            quota,
            seats,
        } => Op::Entitlement {
            version: EntitlementVersion(*version),
            state: if *active {
                EntitlementState::Active
            } else {
                EntitlementState::Lapsed
            },
            quota: ByteCount::new(*quota),
            seats: *seats,
        },
        MOp::App { name, body_len } => Op::App {
            name: app_name(*name)?,
            body: vec![0xA5; *body_len],
        },
        MOp::Unknown { .. } => return Ok(None),
    }))
}

/// A signature of `preimage` by the fake key of the bytes `key`.
fn sign_with(key: [u8; 32], preimage: &[u8]) -> Vec<u8> {
    FakeScheme::sign(&KeyId::from_bytes(key), preimage)
}

/// The payments keys the check trusts: the one the generator signs `entitlement`s with.
fn trusted_keys() -> TrustedKeys {
    TrustedKeys::new(vec![payments_key_bytes(true)])
}

// ---------------------------------------------------------------------------------------------------
// Known findings: divergences that are registered as drift rows and kept out of the way so that the rest of
// the differential stays meaningful.
// ---------------------------------------------------------------------------------------------------

/// A divergence between the model and the production that is a registered finding of this lane (a row of
/// `delta/W1S47A.tsv`): the entry is counted under the row's id and applied on NEITHER side, so the two
/// states stay in step and the rest of the sequence keeps testing everything else. The list is data; an
/// entry that matches is the finding's evidence, shown by its own `#[ignore]`d test.
///
/// Implements planning/architecture.md section 12 E1 ("a failure counts as MAJOR"); Caller: `Rig::step`.
pub fn known_finding(
    _entry: &ModelEntry,
    _model_before: &ModelState,
    _model: Verdict,
    _prod: Verdict,
) -> Option<&'static str> {
    None
}

// ---------------------------------------------------------------------------------------------------
// The rig.
// ---------------------------------------------------------------------------------------------------

/// What one step found.
///
/// Implements planning/architecture.md section 12 E1; Caller: `compare.rs`, `stories.rs`, the tests.
#[derive(Clone, Debug)]
pub struct StepReport {
    /// Every class the model found (empty: admitted).
    pub model_classes: Vec<Class>,
    /// The model's verdict (the first class).
    pub model: Verdict,
    /// The production's verdict of `verify_signatures` + `admit`.
    pub prod: Verdict,
    /// The device's verdict of `replay` of this one entry.
    pub replay: Verdict,
    /// The production's refusal as text, when it refused.
    pub prod_refusal: Option<String>,
    /// Every disagreement found at this step (empty: all three agree).
    pub problems: Vec<String>,
    /// Whether the entry was applied on both sides.
    pub applied: bool,
    /// The registered finding this step is the evidence of, if any.
    pub known: Option<&'static str>,
}

impl StepReport {
    /// Whether all three agree.
    pub fn agrees(&self) -> bool {
        self.problems.is_empty()
    }
}

/// An entry that could not be turned into bytes (a bug of the harness, never of the production).
const UNBUILDABLE: &str = "the entry cannot be built";

/// The model, the production and the log a device replays, run in step.
///
/// Implements planning/architecture.md section 12 E1; Caller: `differential`, `run_steps`, `stories.rs`,
/// the tests.
#[derive(Clone)]
pub struct Rig {
    /// The account's number.
    pub account: u16,
    /// The kind of the account.
    pub kind: AcctKind,
    /// The model's state.
    pub model: ModelState,
    /// The production's state, changed only by `apply`.
    pub prod: TeamState,
    /// The hash of every admitted entry, by position.
    hashes: Vec<Hash32>,
    /// The admitted entries with the stamps they were admitted at: what a new device replays.
    pub log: Vec<(RawEntry, Stamp)>,
    trusted: TrustedKeys,
}

impl Rig {
    /// A rig for an empty account of this kind.
    ///
    /// Implements planning/architecture.md section 5.9 (the first entry is the genesis); Caller:
    /// `differential`, `run_steps`, `stories.rs`.
    pub fn new(kind: AcctKind, account: u16) -> Rig {
        Rig {
            account,
            kind,
            model: ModelState::new(kind),
            prod: TeamState::empty(
                AccountId::from_bytes(account_bytes(account)),
                account_kind(kind),
            ),
            hashes: Vec::new(),
            log: Vec::new(),
            trusted: trusted_keys(),
        }
    }

    fn head_hash(&self) -> Hash32 {
        match self.prod.head() {
            Some(h) => h.hash,
            None => Hash32::ZERO,
        }
    }

    fn prev_hash(&self, spec: PrevSpec) -> Hash32 {
        match spec {
            PrevSpec::Head => self.head_hash(),
            PrevSpec::Zero => Hash32::ZERO,
            PrevSpec::Stale(0) => self.head_hash(),
            PrevSpec::Stale(k) => {
                let len = self.hashes.len();
                match len
                    .checked_sub(usize::from(k) + 1)
                    .and_then(|i| self.hashes.get(i))
                {
                    Some(h) => *h,
                    None => garbage_hash(u64::from(k)),
                }
            }
            PrevSpec::Garbage(n) => garbage_hash(n),
        }
    }

    /// The production's entry for a model entry: `Ok(None)` for a wire word that is no op.
    ///
    /// Implements planning/architecture.md section 5.9 (the fields of an entry); Caller: `Rig::step`, the
    /// tests.
    pub fn convert(&self, e: &ModelEntry) -> Result<Option<RawEntry>, String> {
        let Some(op) = to_op(&e.op)? else {
            return Ok(None);
        };
        let seq = resolved_seq(&self.model, e);
        let epoch = resolved_epoch(&self.model, e);
        let prev = self.prev_hash(e.prev);
        let account_number = match e.account {
            crate::model::AccountSpec::Own => self.account,
            crate::model::AccountSpec::Other => self.account.wrapping_add(1),
        };
        let canonical = format!(
            "e1|eid={}|acct={}|seq={}|prev={}|epoch={}|signer={:?}|op={:?}",
            e.eid, account_number, seq, prev, epoch, e.signer, e.op
        );
        let preimage = signing_input(&TEAM_LOG_DOMAIN, canonical.as_bytes());
        let honest = entry_hash(&preimage);
        let hash = if e.corrupt {
            flip_first_byte(&honest)
        } else {
            honest
        };
        let (signer, sig) = match e.signer {
            MSigner::Device(d) => {
                let key = match e.sig {
                    SigClaim::Valid => device_key_bytes(d),
                    SigClaim::Invalid => stranger_key_bytes(1),
                };
                (Signer::Device(did(d)), sign_with(key, &preimage))
            }
            MSigner::Delegate(k) => {
                let key = match e.sig {
                    SigClaim::Valid => delegate_key_bytes(k),
                    SigClaim::Invalid => stranger_key_bytes(2),
                };
                (Signer::DelegateKey(kid(k)), sign_with(key, &preimage))
            }
            MSigner::Payments => {
                let key = match e.sig {
                    SigClaim::Valid => payments_key_bytes(true),
                    SigClaim::Invalid => payments_key_bytes(false),
                };
                (Signer::PaymentsKey, sign_with(key, &preimage))
            }
        };
        let cosigs = e
            .cosigs
            .iter()
            .map(|(d, claim)| {
                let bytes = match claim {
                    CosigClaim::Valid => sign_with(device_key_bytes(*d), &preimage),
                    CosigClaim::OverOtherEntry => {
                        let mut other = preimage.clone();
                        other.push(b'x');
                        sign_with(device_key_bytes(*d), &other)
                    }
                    CosigClaim::Garbage => vec![7u8; FakeScheme::SIG_LEN],
                };
                (did(*d), bytes)
            })
            .collect();
        let assertions = e
            .assertions
            .iter()
            .map(|a| AssertionVerdict {
                key: did(a.dev),
                user_verified: a.user_verified,
                challenge_matches: a.challenge_ok,
                rp_ok: a.rp_ok,
            })
            .collect();
        Ok(Some(RawEntry {
            account: AccountId::from_bytes(account_bytes(account_number)),
            seq: ChainSeq::new(seq),
            prev,
            hash,
            at: e.at,
            epoch: RestoreEpoch::new(epoch),
            signer,
            op,
            sig,
            cosigs,
            assertions,
            preimage,
            sealed: None,
        }))
    }

    /// The production's verdict on a converted entry at the leader's stamp, with the witness when it admits.
    fn prod_verdict(&self, raw: &RawEntry, now: Stamp) -> Result<Admitted, Refusal> {
        let proposal = verify_signatures(&self.prod, raw, &FakeScheme, &self.trusted)?;
        admit(&self.prod, &proposal, now)
    }

    /// The production's verdict alone on an entry that a test has put together (or changed after it was
    /// signed): `Admit`, or the class of its refusal. Nothing is applied and the model is not asked.
    ///
    /// Implements planning/architecture.md section 5.9 (the metadata group's check); Caller: the story
    /// tests, `Story::probe_production`.
    pub fn probe(&self, raw: &RawEntry, now: Stamp) -> Verdict {
        match self.prod_verdict(raw, now) {
            Ok(_) => Verdict::Admit,
            Err(r) => Verdict::Refuse(class_of(&r)),
        }
    }

    /// A device's `replay` from the first entry over entries and stamps that a test has put together (a log
    /// with a duplicate, a log with other stamps than the group's): the state it ends in, or the position
    /// and the class of the first refusal. Nothing of the rig changes.
    ///
    /// Implements planning/architecture.md section 5.9 (a device verifies the chain it keeps); Caller: the
    /// probe tests.
    pub fn replay_log(
        &self,
        entries: &[RawEntry],
        stamps: &[Stamp],
    ) -> Result<StateView, (usize, Class)> {
        let start = TeamState::empty(
            AccountId::from_bytes(account_bytes(self.account)),
            account_kind(self.kind),
        );
        match replay(start, entries, &FakeScheme, &self.trusted, stamps) {
            Ok(state) => Ok(project_prod(&state.view())),
            Err((i, r)) => Err((i, class_of(&r))),
        }
    }

    /// Offers one entry at the leader's stamp to the model, to the production and to a device's replay;
    /// applies it everywhere when all admit.
    ///
    /// Implements planning/architecture.md section 12 E1 ("devices run their own check, and the test runs
    /// both"); Caller: `differential`, `run_steps`, `stories.rs`.
    pub fn step(&mut self, e: &ModelEntry, now: Stamp) -> StepReport {
        let model_classes = model_violations(&self.model, e, now);
        let model = match model_classes.first() {
            Some(c) => Verdict::Refuse(*c),
            None => Verdict::Admit,
        };
        let mut report = StepReport {
            model_classes,
            model,
            prod: Verdict::Refuse(Class::UnknownOp),
            replay: Verdict::Refuse(Class::UnknownOp),
            prod_refusal: None,
            problems: Vec::new(),
            applied: false,
            known: None,
        };
        let raw = match self.convert(e) {
            Ok(raw) => raw,
            Err(msg) => {
                report.problems.push(format!("{}: {}", UNBUILDABLE, msg));
                return report;
            }
        };
        let Some(raw) = raw else {
            // a wire word that is no op: the glue refuses it before an entry exists, on every side
            let word = match &e.op {
                MOp::Unknown { word } => *word,
                _ => 0,
            };
            match OpName::from_wire(&format!("bogus_{word}")) {
                Ok(_) => report
                    .problems
                    .push("OpName::from_wire accepted a word that is no op".to_string()),
                Err(unknown) => {
                    let refusal: Refusal = unknown.into();
                    report.prod = Verdict::Refuse(class_of(&refusal));
                    report.replay = report.prod;
                    report.prod_refusal = Some(format!("{refusal:?}"));
                }
            }
            self.check_verdicts(&mut report);
            return report;
        };
        // the metadata group's leg: verify_signatures + admit
        let admitted = match self.prod_verdict(&raw, now) {
            Ok(a) => {
                report.prod = Verdict::Admit;
                Some(a)
            }
            Err(r) => {
                report.prod = Verdict::Refuse(class_of(&r));
                report.prod_refusal = Some(format!("{r:?}"));
                None
            }
        };
        // a device's leg: replay of this one entry from the same state
        let replayed = replay(
            self.prod.clone(),
            std::slice::from_ref(&raw),
            &FakeScheme,
            &self.trusted,
            &[now],
        );
        let replay_state = match replayed {
            Ok(state) => {
                report.replay = Verdict::Admit;
                Some(state)
            }
            Err((_, r)) => {
                report.replay = Verdict::Refuse(class_of(&r));
                None
            }
        };
        self.check_verdicts(&mut report);
        if let Some(id) = known_finding(e, &self.model, report.model, report.prod) {
            if !report.agrees() {
                report.known = Some(id);
                report.problems.clear();
                return report;
            }
        }
        if !report.agrees() {
            return report;
        }
        if let (Some(admitted), Verdict::Admit) = (admitted, report.model) {
            let effects = apply(&mut self.prod, admitted);
            if !effects.applied {
                report
                    .problems
                    .push("an admitted entry did not apply".to_string());
            }
            model_apply(&mut self.model, e, now);
            self.hashes.push(raw.hash);
            self.log.push((raw, now));
            report.applied = true;
            let mine = project_model(&self.model);
            let theirs = project_prod(&self.prod.view());
            if let Some(diff) = mine.diff(&theirs) {
                report
                    .problems
                    .push(format!("the state after the entry differs:\n{diff}"));
            }
            if let Some(state) = replay_state {
                let device = project_prod(&state.view());
                if let Some(diff) = theirs.diff(&device) {
                    report.problems.push(format!(
                        "the state after replay differs from the incremental state (left: incremental, right: replay):\n{diff}"
                    ));
                }
            }
        }
        report
    }

    /// Compares the three verdicts and records the disagreements.
    fn check_verdicts(&self, report: &mut StepReport) {
        let agree = match (report.model, report.prod) {
            (Verdict::Admit, Verdict::Admit) => true,
            (Verdict::Refuse(m), Verdict::Refuse(p)) => {
                if report.model_classes.len() == 1 {
                    m == p
                } else {
                    report.model_classes.contains(&p)
                }
            }
            _ => false,
        };
        if !agree {
            let text = match &report.prod_refusal {
                Some(t) => t.clone(),
                None => "admitted".to_string(),
            };
            report.problems.push(format!(
                "verdicts differ: model {:?} (classes {:?}) | production {:?} ({})",
                report.model, report.model_classes, report.prod, text
            ));
        }
        if report.replay != report.prod {
            report.problems.push(format!(
                "the device's replay differs from the metadata group's check: replay {:?} | check {:?}",
                report.replay, report.prod
            ));
        }
    }

    /// A device that replays the whole log from the first entry, and the state it ends in: the answer of
    /// `replay` from an empty state over every admitted entry. `Ok` when its `state_hash` equals the
    /// incremental state's and its view is equal.
    ///
    /// Implements planning/architecture.md section 5.9 (a device verifies the chain it keeps) and section 12
    /// E1; Caller: `differential`, `run_steps`, the tests.
    pub fn replay_all(&self) -> Result<(), String> {
        let entries: Vec<RawEntry> = self.log.iter().map(|(e, _)| e.clone()).collect();
        let stamps: Vec<Stamp> = self.log.iter().map(|(_, s)| *s).collect();
        let start = TeamState::empty(
            AccountId::from_bytes(account_bytes(self.account)),
            account_kind(self.kind),
        );
        match replay(start, &entries, &FakeScheme, &self.trusted, &stamps) {
            Err((i, r)) => Err(format!(
                "replay from the first entry stopped at entry {i}: {r:?}"
            )),
            Ok(state) => {
                if state.state_hash() != self.prod.state_hash() {
                    let theirs = project_prod(&self.prod.view());
                    let device = project_prod(&state.view());
                    let diff = theirs
                        .diff(&device)
                        .unwrap_or_else(|| "views equal, hashes differ".to_string());
                    return Err(format!(
                        "state_hash of replay differs from the incremental state:\n{diff}"
                    ));
                }
                Ok(())
            }
        }
    }
}

/// A hash that no entry has: made from a number.
fn garbage_hash(n: u64) -> Hash32 {
    let bytes = n.to_le_bytes();
    Hash32::from_bytes(core::array::from_fn(|i| match bytes.get(i % 8) {
        Some(b) => *b ^ 0xC3,
        None => 0xC3,
    }))
}

/// A hash that differs from `h` in its first byte.
fn flip_first_byte(h: &Hash32) -> Hash32 {
    let orig = *h.as_bytes();
    Hash32::from_bytes(core::array::from_fn(|i| match orig.get(i) {
        Some(b) if i == 0 => b ^ 0x01,
        Some(b) => *b,
        None => 0,
    }))
}

// ---------------------------------------------------------------------------------------------------
// Statistics, failures and the differential.
// ---------------------------------------------------------------------------------------------------

/// What a near-miss kind did in a run.
///
/// Implements planning/architecture.md section 12 E1; Caller: the near-miss tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NearStats {
    /// How many entries of this kind were run.
    pub cases: usize,
    /// How many both sides refused.
    pub both_refused: usize,
    /// How many the production refused with exactly the intended class.
    pub exact_class: usize,
}

/// A disagreement that a collecting run recorded and went on from (nothing was applied on either side, so the
/// states are still in step).
///
/// Implements planning/architecture.md section 12 E1; Caller: the triage test, the tests.
#[derive(Clone, Debug)]
pub struct Mismatch {
    /// The seed of the run.
    pub seed: u64,
    /// The index of the step.
    pub index: usize,
    /// The step.
    pub step: Step,
    /// What the step found.
    pub report: StepReport,
}

impl Mismatch {
    /// A short key for grouping: the op, the model's classes and the production's verdict.
    ///
    /// Implements planning/architecture.md section 12 E1; Caller: the triage test.
    pub fn signature(&self) -> String {
        let op = match self.step.entry.op.kind() {
            Some(k) => k.name().to_string(),
            None => "unknown_word".to_string(),
        };
        format!(
            "{op}: model {:?} {:?} | production {:?}",
            self.report.model, self.report.model_classes, self.report.prod
        )
    }
}

/// The most mismatches a collecting run keeps.
const MISMATCH_CAP: usize = 400;

/// The counts of a run: which ops and which refusal classes were exercised, how often.
///
/// Implements planning/architecture.md section 12 E1 ("a generator that never reaches the interesting
/// branches proves nothing"); Caller: the coverage test.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    /// The disagreements a collecting run went on from.
    pub mismatches: Vec<Mismatch>,
    /// Sequences run.
    pub sequences: usize,
    /// Entries run.
    pub entries: usize,
    /// Entries admitted.
    pub admitted: usize,
    /// Entries run, per op.
    pub per_op: BTreeMap<OpKind, usize>,
    /// Entries admitted, per op.
    pub per_op_admitted: BTreeMap<OpKind, usize>,
    /// Entries refused, per class (the production's class).
    pub per_class: BTreeMap<Class, usize>,
    /// Entries refused, per op and class.
    pub per_op_class: BTreeMap<(OpKind, Class), usize>,
    /// Entries whose wire word is no op.
    pub unknown_words: usize,
    /// Near-miss kinds.
    pub near: BTreeMap<NearMiss, NearStats>,
    /// Entries that were the evidence of a known finding, per row id.
    pub known: BTreeMap<&'static str, usize>,
    /// Refusals where the production reported another class than the model's first (both in the model's
    /// set): the order of checks the plan does not give.
    pub class_order_differs: usize,
}

impl Stats {
    /// Adds another run's counts.
    ///
    /// Implements planning/architecture.md section 12 E1; Caller: the tests.
    pub fn merge(&mut self, other: &Stats) {
        self.sequences += other.sequences;
        self.entries += other.entries;
        self.admitted += other.admitted;
        for (k, v) in &other.per_op {
            *self.per_op.entry(*k).or_insert(0) += v;
        }
        for (k, v) in &other.per_op_admitted {
            *self.per_op_admitted.entry(*k).or_insert(0) += v;
        }
        for (k, v) in &other.per_class {
            *self.per_class.entry(*k).or_insert(0) += v;
        }
        for (k, v) in &other.per_op_class {
            *self.per_op_class.entry(*k).or_insert(0) += v;
        }
        self.unknown_words += other.unknown_words;
        for (k, v) in &other.near {
            let mine = self.near.entry(*k).or_default();
            mine.cases += v.cases;
            mine.both_refused += v.both_refused;
            mine.exact_class += v.exact_class;
        }
        for (k, v) in &other.known {
            *self.known.entry(*k).or_insert(0) += v;
        }
        self.class_order_differs += other.class_order_differs;
        for m in &other.mismatches {
            if self.mismatches.len() < MISMATCH_CAP {
                self.mismatches.push(m.clone());
            }
        }
    }

    fn count(&mut self, step: &Step, report: &StepReport, near: Option<NearMiss>) {
        self.entries += 1;
        let kind = step.entry.op.kind();
        match kind {
            Some(k) => *self.per_op.entry(k).or_insert(0) += 1,
            None => self.unknown_words += 1,
        }
        if let Some(id) = report.known {
            *self.known.entry(id).or_insert(0) += 1;
            return;
        }
        match report.prod {
            Verdict::Admit => {
                self.admitted += 1;
                if let Some(k) = kind {
                    *self.per_op_admitted.entry(k).or_insert(0) += 1;
                }
            }
            Verdict::Refuse(c) => {
                *self.per_class.entry(c).or_insert(0) += 1;
                if let Some(k) = kind {
                    *self.per_op_class.entry((k, c)).or_insert(0) += 1;
                }
                if report.model_classes.first() != Some(&c) {
                    self.class_order_differs += 1;
                }
            }
        }
        if let Some(nm) = near {
            let n = self.near.entry(nm).or_default();
            n.cases += 1;
            if matches!(report.model, Verdict::Refuse(_))
                && matches!(report.prod, Verdict::Refuse(_))
            {
                n.both_refused += 1;
            }
            if report.prod == Verdict::Refuse(nm.expected()) {
                n.exact_class += 1;
            }
        }
    }
}

/// Why a run failed.
///
/// Implements planning/architecture.md section 12 E1; Caller: `Failure`, the tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailKind {
    /// The verdicts of the model and the production (or of the replay) differ.
    Verdict,
    /// The states differ after an admitted entry.
    State,
    /// Replay from the first entry ends in another state than the incremental one.
    FinalReplay,
    /// An entry could not be built (a bug of the harness).
    Unbuildable,
}

/// The first disagreement of a run, with everything needed to replay it.
///
/// Implements planning/architecture.md section 12 E1 ("the seed list printed on failure"); Caller: the
/// tests.
#[derive(Clone, Debug)]
pub struct Failure {
    /// The seed of the run.
    pub seed: u64,
    /// The kind of account.
    pub kind: AcctKind,
    /// The account's number.
    pub account: u16,
    /// Which step (0-based) failed; the number of steps when the final replay failed.
    pub index: usize,
    /// What kind of disagreement.
    pub fail: FailKind,
    /// What the step found.
    pub report: Option<StepReport>,
    /// The text of the disagreement.
    pub detail: String,
    /// Every step run up to and including the failing one.
    pub history: Vec<Step>,
    /// The bug planted into the model for this run, if any (tests of the harness only).
    pub plant: Option<Planted>,
}

impl Failure {
    /// The message of a failed run: the seed, the step, the verdicts, the difference and the minimal
    /// sequence as the body of a test that can be pasted.
    ///
    /// Implements planning/architecture.md section 12 E1; Caller: the tests (`failure_message_prints_a_pasteable_test`).
    pub fn message(&self) -> String {
        let minimal = shrink_failure(self);
        let mut out = format!(
            "E1 FAILED: seed {} ({:?} account), step {} of {}: {:?}\n{}\n",
            self.seed,
            self.kind,
            self.index,
            self.history.len(),
            self.fail,
            self.detail
        );
        if let Some(r) = &self.report {
            let text = match &r.prod_refusal {
                Some(t) => t.clone(),
                None => "admitted".to_string(),
            };
            out.push_str(&format!(
                "model {:?} (classes {:?}) | production {:?} ({}) | replay {:?}\n",
                r.model, r.model_classes, r.prod, text, r.replay
            ));
        }
        out.push_str(&format!(
            "replay it with: AZLIN_ESCALATE_SEED={} (or paste the minimal sequence of {} entries below)\n",
            self.seed,
            minimal.len()
        ));
        out.push_str(&pasteable_test(
            self.seed,
            self.kind,
            self.account,
            &minimal,
        ));
        out
    }
}

/// The body of a test that replays a sequence: the entries as the Rust source that builds them
/// (`Step::rust`; the model's types are plain data and the `use` lines of the body bring them in).
///
/// Implements planning/architecture.md section 12 E1 ("the failure message prints the seed AND the minimal
/// sequence as a pasteable test body"); Caller: `Failure::message`, the tests.
pub fn pasteable_test(seed: u64, kind: AcctKind, account: u16, steps: &[Step]) -> String {
    let mut out = String::new();
    out.push_str("#[test]\n");
    out.push_str(&format!("fn e1_regression_seed_{seed}() {{\n"));
    out.push_str("    use azlin_escalate_e1::compare::run_steps;\n");
    out.push_str("    use azlin_escalate_e1::gen::Step;\n");
    out.push_str("    use azlin_escalate_e1::model::*;\n");
    out.push_str("    use azlin_seams::{Span, Stamp};\n");
    out.push_str("    let steps: Vec<Step> = vec![\n");
    for step in steps {
        out.push_str(&format!("        {},\n", step.rust()));
    }
    out.push_str("    ];\n");
    out.push_str(&format!(
        "    assert!(run_steps(AcctKind::{kind:?}, {account}, &steps).is_none());\n"
    ));
    out.push_str("}\n");
    out
}

/// The result of one seeded run.
///
/// Implements planning/architecture.md section 12 E1; Caller: the tests.
#[derive(Clone, Debug)]
pub enum Outcome {
    /// Model, production and replay agreed on every entry; the counts of the run.
    Pass(Stats),
    /// The first disagreement.
    Fail(Box<Failure>),
}

/// What a run mixes: every mode, or one near-miss kind as often as the state allows.
///
/// Implements planning/architecture.md section 12 E1; Caller: `differential_plan`, the near-miss tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plan {
    /// Valid, near-miss (every kind) and random entries.
    Mixed,
    /// Six entries in ten are this near miss (the rest valid).
    Focus(NearMiss),
}

/// The stamp the next entry is offered at: mostly seconds later, now and then hours or days.
fn advance(rng: &mut Rng, now: Stamp) -> Stamp {
    let secs = if pct(rng, 92) {
        between(rng, 1, 120) as u64
    } else if pct(rng, 70) {
        between(rng, 1_800, 7_200) as u64
    } else {
        between(rng, 86_400, 3 * 86_400) as u64
    };
    Stamp::from_secs(now.as_secs().saturating_add(secs))
}

fn later(a: Stamp, b: Stamp) -> Stamp {
    if b > a {
        b
    } else {
        a
    }
}

/// Runs one step and turns a disagreement into a failure; a collecting run records a disagreement of the
/// verdicts (nothing was applied, the states are still in step) and goes on.
fn run_one(
    rig: &mut Rig,
    seed: u64,
    index: usize,
    step: &Step,
    near: Option<NearMiss>,
    stats: &mut Stats,
    history: &[Step],
    collect: bool,
) -> Result<(), Box<Failure>> {
    let report = rig.step(&step.entry, step.now);
    stats.count(step, &report, near);
    if report.agrees() {
        return Ok(());
    }
    let fail = if report.problems.iter().any(|p| p.starts_with(UNBUILDABLE)) {
        FailKind::Unbuildable
    } else if report
        .problems
        .iter()
        .any(|p| p.starts_with("the state after"))
    {
        FailKind::State
    } else {
        FailKind::Verdict
    };
    if collect && fail == FailKind::Verdict {
        if stats.mismatches.len() < MISMATCH_CAP {
            stats.mismatches.push(Mismatch {
                seed,
                index,
                step: step.clone(),
                report,
            });
        }
        return Ok(());
    }
    Err(Box::new(Failure {
        seed,
        kind: rig.kind,
        account: rig.account,
        index,
        fail,
        detail: report.problems.join("\n"),
        report: Some(report),
        history: history.to_vec(),
        plant: rig.model.planted,
    }))
}

/// Runs a seeded sequence: the universe the seed draws, its setup, then `steps` entries in the modes the
/// plan mixes; at the end a device replays the whole log from the first entry.
///
/// Implements planning/architecture.md section 12 E1 ("checked against a separate reference model on
/// random sequences of ops, group moves and delegations; devices run their own check, and the test runs
/// both"); Caller: the tests (`e1_differential_...`), `shrink`.
pub fn differential_plan(seed: u64, steps: usize, plan: Plan) -> Outcome {
    let mut rng = Rng::new(seed);
    let kind = pick_kind(&mut rng);
    let u = gen_universe(&mut rng, kind);
    run_generated(seed, &u, rng, steps, plan, false, None).0
}

/// [`differential_plan`] with a bug planted into the model: the harness must catch it (the tests of the
/// shrinker and of the replay of a failing seed).
///
/// Implements planning/architecture.md section 12 E1 ("plant a divergence ... shrink"); Caller: the tests.
pub fn differential_planted(seed: u64, steps: usize, plan: Plan, plant: Planted) -> Outcome {
    let mut rng = Rng::new(seed);
    let kind = pick_kind(&mut rng);
    let u = gen_universe(&mut rng, kind);
    run_generated(seed, &u, rng, steps, plan, false, Some(plant)).0
}

/// The sequence of steps a seed generates (the setup and `steps` entries), however the run ends: a recorded
/// sequence that a test can add to, run with [`run_steps_with`] and shrink.
///
/// Implements planning/architecture.md section 12 E1; Caller: the shrink test.
pub fn record(seed: u64, steps: usize) -> (AcctKind, u16, Vec<Step>) {
    let mut rng = Rng::new(seed);
    let kind = pick_kind(&mut rng);
    let u = gen_universe(&mut rng, kind);
    let (_, history) = run_generated(seed, &u, rng, steps, Plan::Mixed, true, None);
    (kind, u.account, history)
}

/// [`differential_plan`] that does not stop at a disagreement of the verdicts: it records it (in
/// `Stats::mismatches`), applies the entry on neither side and goes on, so one run shows every kind of
/// disagreement (the triage view). A disagreement of the states, or of the final replay, still fails.
///
/// Implements planning/architecture.md section 12 E1; Caller: the triage test.
pub fn differential_collect(seed: u64, steps: usize, plan: Plan) -> Outcome {
    let mut rng = Rng::new(seed);
    let kind = pick_kind(&mut rng);
    let u = gen_universe(&mut rng, kind);
    run_generated(seed, &u, rng, steps, plan, true, None).0
}

/// [`differential_plan`] with every mode mixed.
///
/// Implements planning/architecture.md section 12 E1; Caller: the tests, `shrink`.
pub fn differential(seed: u64, steps: usize) -> Outcome {
    differential_plan(seed, steps, Plan::Mixed)
}

fn run_generated(
    seed: u64,
    u: &Universe,
    mut rng: Rng,
    steps: usize,
    plan: Plan,
    collect: bool,
    plant: Option<Planted>,
) -> (Outcome, Vec<Step>) {
    let mut rig = Rig::new(u.kind, u.account);
    rig.model.planted = plant;
    let mut stats = Stats {
        sequences: 1,
        ..Stats::default()
    };
    let mut setup = Setup::new(u);
    let mut eid: u64 = 0;
    let mut now = Stamp::from_secs(1_000_000 + seed % 997);
    let mut history: Vec<Step> = Vec::new();
    let mut index = 0usize;
    while !setup.is_done() {
        now = advance(&mut rng, now);
        let Some(step) = setup.next_step(&mut rng, u, &rig.model, now, &mut eid) else {
            break;
        };
        history.push(step.clone());
        if let Err(f) = run_one(
            &mut rig, seed, index, &step, None, &mut stats, &history, collect,
        ) {
            return (Outcome::Fail(f), history);
        }
        now = later(now, step.now);
        index += 1;
    }
    for _ in 0..steps {
        now = advance(&mut rng, now);
        let mode = match plan {
            Plan::Mixed => pick_mode(&mut rng),
            Plan::Focus(nm) => {
                if pct(&mut rng, 60) {
                    Mode::NearMiss(nm)
                } else {
                    Mode::Valid
                }
            }
        };
        let generated: Option<Generated> = gen_entry(&mut rng, u, &rig.model, now, &mut eid, mode)
            .or_else(|| gen_entry(&mut rng, u, &rig.model, now, &mut eid, Mode::Valid))
            .or_else(|| gen_entry(&mut rng, u, &rig.model, now, &mut eid, Mode::Random));
        let Some(g) = generated else {
            continue;
        };
        history.push(g.step.clone());
        if let Err(f) = run_one(
            &mut rig,
            seed,
            index,
            &g.step,
            g.near_miss,
            &mut stats,
            &history,
            collect,
        ) {
            return (Outcome::Fail(f), history);
        }
        now = later(now, g.step.now);
        index += 1;
    }
    if let Err(detail) = rig.replay_all() {
        let failure = Failure {
            seed,
            kind: rig.kind,
            account: rig.account,
            index,
            fail: FailKind::FinalReplay,
            report: None,
            detail,
            history: history.clone(),
            plant,
        };
        return (Outcome::Fail(Box::new(failure)), history);
    }
    (Outcome::Pass(stats), history)
}

/// Runs a recorded sequence from an empty account: `None` when model, production and replay agree on every
/// entry (and the final replay holds), else the first disagreement. The chain fields of an entry are
/// claims relative to the head (`SeqSpec::Next`, `PrevSpec::Head`), so a subsequence of a run is a valid
/// sequence again.
///
/// Implements planning/architecture.md section 12 E1; Caller: the pasteable test, `shrink`, the tests.
pub fn run_steps(kind: AcctKind, account: u16, steps: &[Step]) -> Option<Failure> {
    run_steps_with(kind, account, steps, None)
}

/// [`run_steps`] with a bug planted into the model (the shrink test).
///
/// Implements planning/architecture.md section 12 E1; Caller: the tests, `minimize_steps`.
pub fn run_steps_with(
    kind: AcctKind,
    account: u16,
    steps: &[Step],
    plant: Option<Planted>,
) -> Option<Failure> {
    let mut rig = Rig::new(kind, account);
    rig.model.planted = plant;
    let mut stats = Stats::default();
    let mut history: Vec<Step> = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        history.push(step.clone());
        if let Err(f) = run_one(&mut rig, 0, index, step, None, &mut stats, &history, false) {
            return Some(Failure { plant, ..*f });
        }
    }
    match rig.replay_all() {
        Ok(()) => None,
        Err(detail) => Some(Failure {
            seed: 0,
            kind,
            account,
            index: steps.len(),
            fail: FailKind::FinalReplay,
            report: None,
            detail,
            history,
            plant,
        }),
    }
}

/// Delta debugging over a failing sequence: the smallest subsequence that still shows a disagreement.
fn minimize_steps(
    kind: AcctKind,
    account: u16,
    steps: Vec<Step>,
    plant: Option<Planted>,
) -> Vec<Step> {
    minimize(steps, |candidate| {
        run_steps_with(kind, account, candidate, plant).is_some()
    })
}

/// The minimal sequence of a failure: the smallest subsequence of its history that still fails.
///
/// Implements planning/architecture.md section 12 E1 ("delta-debugging with `azlin_seams::sim::minimize`");
/// Caller: `Failure::message`, the shrink test.
pub fn shrink_failure(f: &Failure) -> Vec<Step> {
    minimize_steps(f.kind, f.account, f.history.clone(), f.plant)
}

/// The minimal failing sequence of a seeded run, found with `azlin_seams::sim::minimize`; empty when the run
/// passes.
///
/// Implements planning/architecture.md section 12 E1 ("delta-debugging with `azlin_seams::sim::minimize`");
/// Caller: the tests (`shrink_reduces_a_planted_failure_to_a_minimal_sequence`).
pub fn shrink(seed: u64, steps: usize) -> Vec<Step> {
    match differential(seed, steps) {
        Outcome::Pass(_) => Vec::new(),
        Outcome::Fail(f) => shrink_failure(&f),
    }
}

/// The projection of the production's current state (a convenience for the tests).
///
/// Implements planning/architecture.md section 12 E1; Caller: the tests.
pub fn prod_view(rig: &Rig) -> StateView {
    project_prod(&rig.prod.view())
}

/// The tracked policy of a production account of this kind, for the test that compares the model's
/// defaults with `Policy::defaults`.
///
/// Implements planning/architecture.md section 5.10; Caller: the tests.
pub fn production_defaults(kind: AcctKind) -> crate::model::MPolicy {
    policy_of(&azlin_policy::Policy::defaults(account_kind(kind)))
}

/// The production's size cap of an `app:<name>/...` body, for the test that compares it with the model's.
///
/// Implements planning/architecture.md section 5.9 (`app:<name>/...`: size cap); Caller: the tests.
pub fn production_app_body_cap() -> u64 {
    azlin_teamlog::APP_BODY_MAX.get()
}
