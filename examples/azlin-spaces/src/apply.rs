//! `apply`: turns an admitted entry, deterministically, into the state's change and the effects the
//! node and the key code act on (planning/architecture.md section 5.9 "Space chains", section 5.4 "Spaces",
//! section 5.5 "Rotation ... plus the chain wrap", section 5.6 "What a removal costs", section 5.13
//! "key_epoch" and "log_appended" events; build unit S16, lane W1-S16b).
//!
//! `apply(state, admitted)` is the one place a [`SpaceState`] changes. It is a function of the state and
//! the admitted entry (the leader's stamp is in the witness): two replicas that apply the same entries in
//! the same order hold equal states (`SpaceState::state_hash`) and make equal [`SpaceEffects`]. It reads no
//! clock, draws no entropy, does no I/O and iterates only ordered collections.
//!
//! [`Admitted`] is a witness: its fields are private and it has no `Clone`, `Default` or `From`, so only
//! `admit` can make one. `apply` does not re-decide the permission, but it never leaves a half-applied
//! entry behind: it first checks that the op fits the state it is applied to (an `Admitted` made against
//! another state is not applied: `applied == false`, nothing changed), then moves the chain, then changes
//! the state, and nothing after the chain moved can fail.
//!
//! WHAT AN ENTRY MAKES THE NODE AND THE DEVICES DO (the effects; the key wraps themselves are S15's, this
//! crate only says who needs one):
//!
//! - `rows`: the `reg_space` row (the chain head moved) and the `reg_grant` row of every grant that
//!   changed, whole, as upserts.
//! - `wraps`: the wraps that must exist, by recipient (a member, an external member, a group, a
//!   capability) and epoch. A grant that gives a new reader the key in a `full` space needs one at the
//!   current epoch; a rekey needs one for every remaining reader at the new epoch. A grant in a `forward`
//!   space needs none now: the key rotates on add, so the joiner's wrap comes with the rekey.
//! - `chain_wraps`: the chain wrap of a rekey in a `full` space (the old epoch's key under the new one).
//! - `stop_serving`: the wraps the node stops serving to a grantee whose read access ended.
//! - `notices`: `log_appended` for every entry and `key_epoch` for a new epoch (section 5.13).
//! - `obligations`: `RekeyDue` when a removal or a joiner of a forward space makes the current key
//!   something somebody should not hold; a `space_rekey` clears it (`RekeyCleared`).

use azlin_chain::Admitted as ChainAdmitted;
use azlin_keys::CommitKind;
use azlin_seams::{
    mutant, AccountId, ByteCount, ChainSeq, Epoch, Hash32, KeyId, MemberId, SpaceId, Stamp,
};

use crate::admit::{admit, AccountView, SignerFacts, SpaceRefusal};
use crate::epochs::EpochRecord;
use crate::grant::{Grant, GrantState, Grantee, Payer, Source};
use crate::kind::{HistoryMode, SpaceKind};
use crate::ops::{RekeyReason, SpaceOp, SpaceOpName};
use crate::proposal::SpaceProposal;
use crate::role::{role_caps, SpaceRole};
use crate::state::{Authority, Core, EntryRecord, SpaceRow, SpaceState, Window};

/// An entry the permission check admitted: the witness `apply` takes. Only `admit` makes one (the
/// constructor is crate-private; there is no `Clone`, `Default` or `From`).
///
/// ```compile_fail,E0599
/// // An admission cannot be copied: it is applied once.
/// fn twice(a: azlin_spaces::Admitted) -> (azlin_spaces::Admitted, azlin_spaces::Admitted) {
///     (a.clone(), a)
/// }
/// ```
///
/// ```compile_fail,E0451
/// // An admission cannot be built by hand: its fields are private.
/// let _forged = azlin_spaces::Admitted { record: todo!(), cosigners: vec![] };
/// ```
///
/// Implements planning/architecture.md section 5.9 ("The server enforces only what was signed") and the
/// brief's `Admitted` witness.
///
/// Caller: `admit` (makes it), `apply` (takes it), `replay`.
#[derive(Debug)]
pub struct Admitted {
    record: EntryRecord,
    cosigners: Vec<MemberId>,
}

impl Admitted {
    /// Makes the witness. Only `admit` calls it.
    pub(crate) fn new(record: EntryRecord, cosigners: Vec<MemberId>) -> Admitted {
        Admitted { record, cosigners }
    }

    /// The position of the entry.
    pub fn seq(&self) -> ChainSeq {
        self.record.seq
    }

    /// The hash of the entry.
    pub fn hash(&self) -> Hash32 {
        self.record.hash
    }

    /// The op of the entry.
    pub fn op(&self) -> &SpaceOp {
        &self.record.op
    }

    /// The leader's stamp the entry was admitted at.
    pub fn stamp(&self) -> Stamp {
        self.record.at
    }

    /// The rule that let the signer sign the entry.
    pub fn authority(&self) -> Authority {
        self.record.authority
    }

    /// The members whose cosignature counted toward an admin override (empty for every other entry).
    pub fn cosigners(&self) -> &[MemberId] {
        &self.cosigners
    }
}

/// One change of the registry that an applied entry makes: the whole row after the change (an upsert).
///
/// Implements planning/architecture.md section 5.4 (`reg_space` and `reg_grant`) and section 5.9 ("the
/// metadata group applies each entry once, as it arrives, into the registry's tables").
///
/// Caller: `SpaceEffects::rows`; the registry glue (SPACES-APPLY) turns each into a write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpaceDelta {
    /// The `reg_space` row: the chain head (and, after a create, a rekey or a history switch, the kind,
    /// the key epoch and the history mode).
    Space(SpaceRow),
    /// A `reg_grant` row, whole.
    Grant(Grant),
}

/// A wrap that must exist: the recipient (a member, an external member, a group or a capability: the
/// space key is wrapped to a group's key, not to each member of it) and the epoch whose key it holds.
///
/// Implements planning/architecture.md section 5.5 ("a new epoch key, wrapped only to the remaining
/// members and groups") and section 5.6 ("Wraps go to groups, not to each member").
///
/// Caller: `SpaceEffects::wraps`; S15c2's rekey plans and the registry's `reg_wrap` rows (through
/// SPACES-APPLY).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WrapNeed {
    /// Who needs the wrap.
    pub to: Grantee,
    /// The epoch whose key it must hold.
    pub epoch: Epoch,
}

/// The chain wrap of a rotation: the key of epoch `of` wrapped under the key of epoch `under`, so that
/// whoever holds the newer key reads the older epoch.
///
/// Implements planning/architecture.md section 5.5 ("plus the chain wrap (epoch n under epoch n+1)").
///
/// Caller: `SpaceEffects::chain_wraps`; S15c1's `chain_wrap`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainWrapNeed {
    /// The epoch whose key is wrapped.
    pub of: Epoch,
    /// The epoch whose key it is wrapped under.
    pub under: Epoch,
}

/// The wraps the node stops serving to a grantee whose read access ended: every wrap of this recipient
/// for the space, up to and including this epoch.
///
/// Implements planning/architecture.md section 5.6 (what a removal costs) and section 2.4 (a revocation
/// takes effect at once).
///
/// Caller: `SpaceEffects::stop_serving`; the node's wrap service (through SPACES-APPLY).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StopServing {
    /// Whose wraps stop being served.
    pub to: Grantee,
    /// The newest epoch whose wrap of this recipient existed.
    pub through: Epoch,
}

/// A notice the metadata group sends over `watch` and push.
///
/// Implements planning/architecture.md section 5.13 (`log_appended`: "team log: op, subject; space
/// chain: space"; `key_epoch`: "space or group, epoch", a security event that reaches every device of
/// the member).
///
/// Caller: `SpaceEffects::notices`; S33 derives the events from them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpaceNotice {
    /// An entry was appended to the space's chain.
    LogAppended {
        /// The space.
        space: SpaceId,
        /// The op of the entry.
        op: SpaceOpName,
    },
    /// The space's key epoch changed (created, or rotated).
    KeyEpoch {
        /// The space.
        space: SpaceId,
        /// The new epoch.
        epoch: Epoch,
    },
}

/// Something the entry obliges the devices to do.
///
/// Implements planning/architecture.md section 5.5 (rotation after a removal), section 5.4 (`history =
/// forward`: rotate on add) and section 7.6 ("it performs forward key rotation when it holds the space
/// key through a grant").
///
/// Caller: `SpaceEffects::obligations`; the key agent and any remaining manager's device (S15c2's rekey
/// plans).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Obligation {
    /// A rekey is due: the space's key must rotate.
    RekeyDue {
        /// The space.
        space: SpaceId,
        /// Why.
        reason: RekeyReason,
    },
    /// A rekey that was due is done.
    RekeyCleared {
        /// The space.
        space: SpaceId,
    },
}

/// What `apply` produced for one entry.
///
/// `applied` is `false` only when the `Admitted` witness does not fit the state it was applied to (it was
/// made against another state of the chain): nothing changed then and every list is empty.
///
/// Implements planning/architecture.md section 5.9 (the metadata group applies each entry once into the
/// registry's tables; notices come from the metadata group), section 5.5 and section 5.6.
///
/// Caller: the registry glue (SPACES-APPLY: persists `rows`, makes the `wraps`, stops serving, derives
/// the events from `notices`, tracks `obligations`), `replay`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceEffects {
    /// Whether the entry was applied.
    pub applied: bool,
    /// The registry changes, in the order to apply them.
    pub rows: Vec<SpaceDelta>,
    /// The wraps that must exist.
    pub wraps: Vec<WrapNeed>,
    /// The chain wrap of a rotation.
    pub chain_wraps: Vec<ChainWrapNeed>,
    /// The wraps to stop serving.
    pub stop_serving: Vec<StopServing>,
    /// The notices to send.
    pub notices: Vec<SpaceNotice>,
    /// What the devices must do.
    pub obligations: Vec<Obligation>,
}

impl SpaceEffects {
    /// The effects of an entry that was applied, before its op's changes are added.
    fn applied() -> SpaceEffects {
        SpaceEffects {
            applied: true,
            rows: Vec::new(),
            wraps: Vec::new(),
            chain_wraps: Vec::new(),
            stop_serving: Vec::new(),
            notices: Vec::new(),
            obligations: Vec::new(),
        }
    }

    /// The effects of an entry that was not applied: nothing.
    fn not_applied() -> SpaceEffects {
        SpaceEffects {
            applied: false,
            ..SpaceEffects::applied()
        }
    }
}

/// An op that fits the state, with everything `apply` needs to change the state without a further
/// check.
enum Plan {
    Create {
        kind: SpaceKind,
        history: HistoryMode,
        first_manager: MemberId,
        key_id: KeyId,
    },
    Grant {
        grantee: Grantee,
        role: SpaceRole,
        cap_bytes: Option<ByteCount>,
        payer: Payer,
        before: Option<Grant>,
        mode: HistoryMode,
        current: Epoch,
    },
    Revoke {
        grantee: Grantee,
        target: Grant,
        current: Epoch,
    },
    Rekey {
        next: Epoch,
        previous: Epoch,
        key_id: KeyId,
        mode: HistoryMode,
    },
    History {
        mode: HistoryMode,
    },
}

/// Whether the op fits the state, and what it needs: `None` when the entry cannot be applied to this
/// state (the space is archived, the op does not fit the state's phase, the grant to revoke is not in
/// force, the epoch is not the next).
fn plan(state: &SpaceState, op: &SpaceOp) -> Option<Plan> {
    if state.archived.is_some() {
        return None;
    }
    match op {
        SpaceOp::Create {
            kind,
            history,
            first_manager,
            key_id,
        } => {
            if state.core.is_some() || !state.chain.is_empty() {
                return None;
            }
            Some(Plan::Create {
                kind: *kind,
                history: *history,
                first_manager: *first_manager,
                key_id: *key_id,
            })
        }
        SpaceOp::Grant {
            grantee,
            role,
            cap_bytes,
            payer,
        } => {
            let core = state.core.as_ref()?;
            Some(Plan::Grant {
                grantee: *grantee,
                role: *role,
                cap_bytes: *cap_bytes,
                payer: *payer,
                before: state.grants.get(grantee).cloned(),
                mode: core.history,
                current: core.key_epoch,
            })
        }
        SpaceOp::Revoke { grantee } => {
            let core = state.core.as_ref()?;
            let target = state
                .grants
                .get(grantee)
                .filter(|grant| grant.is_active())?
                .clone();
            Some(Plan::Revoke {
                grantee: *grantee,
                target,
                current: core.key_epoch,
            })
        }
        SpaceOp::Rekey { epoch, key_id, .. } => {
            let core = state.core.as_ref()?;
            let next = core.key_epoch.next()?;
            if next != *epoch {
                return None;
            }
            Some(Plan::Rekey {
                next,
                previous: core.key_epoch,
                key_id: *key_id,
                mode: core.history,
            })
        }
        SpaceOp::History { mode } => {
            state.core.as_ref()?;
            Some(Plan::History { mode: *mode })
        }
    }
}

/// The grant row an op makes.
fn new_grant(
    grantee: Grantee,
    role: SpaceRole,
    cap_bytes: Option<ByteCount>,
    payer: Payer,
    seq: ChainSeq,
    now: Stamp,
) -> Grant {
    Grant {
        grantee,
        role,
        cap_bytes,
        payer,
        source: Source::Chain(seq),
        state: GrantState::Active,
        added_at: now,
        removed_at: None,
    }
}

/// Whether a grantee is a member of some account (a direct grant), whose manager role the windows track.
fn is_direct(grantee: &Grantee) -> bool {
    matches!(grantee, Grantee::Member(_) | Grantee::External { .. })
}

/// The grantee holds the manage capability from position `from` on.
fn open_window(state: &mut SpaceState, grantee: &Grantee, from: ChainSeq) {
    state
        .windows
        .entry(*grantee)
        .or_default()
        .push(Window { from, until: None });
}

/// The grantee's open window ends at position `until` (exclusive).
fn close_window(state: &mut SpaceState, grantee: &Grantee, until: ChainSeq) {
    if let Some(windows) = state.windows.get_mut(grantee) {
        if let Some(open) = windows
            .iter_mut()
            .rev()
            .find(|window| window.until.is_none())
        {
            open.until = Some(until);
        }
    }
}

/// Makes a rekey due for `reason` unless it is already due for that reason or a worse one.
fn raise_due(state: &mut SpaceState, reason: RekeyReason, out: &mut SpaceEffects) {
    let raised = match state.rekey_due {
        Some(old) => reason > old,
        None => true,
    };
    if raised {
        state.rekey_due = Some(reason);
        out.obligations.push(Obligation::RekeyDue {
            space: state.id,
            reason,
        });
    }
}

/// Applies an admitted entry to the state, once, in order.
///
/// The op must fit the state (checked again here: an `Admitted` made against another state is not
/// applied). On success the chain has moved to the entry, the state holds the op's change, and the effects
/// list the registry changes (in the order to apply them), the wraps that must exist, the wraps to stop
/// serving, the notices and the obligations. Otherwise the state is untouched and the effects are empty
/// with `applied == false`.
///
/// Implements planning/architecture.md section 5.9 ("the metadata group applies each entry once"),
/// section 5.5 (rotation), section 5.6 (what a removal costs) and section 12 E1 (the pure function the
/// metadata group and every device run).
///
/// Caller: the registry glue (SPACES-APPLY, inside the metadata group's apply), `replay`, the tests.
pub fn apply(state: &mut SpaceState, a: Admitted) -> SpaceEffects {
    let record = a.record;
    let Some(plan) = plan(state, &record.op) else {
        return SpaceEffects::not_applied();
    };
    let Some(after) = record.seq.next() else {
        return SpaceEffects::not_applied();
    };
    let seq = record.seq;
    let now = record.at;
    let signer = record.signer;
    let actor = record.actor;
    let op_name = record.op.name();
    // The chain moves with the entry, or the entry is not applied: nothing below can fail.
    match state.chain.admit(record) {
        Ok(ChainAdmitted::Appended { .. }) => {}
        Ok(ChainAdmitted::AlreadyHeld { .. }) | Err(_) => return SpaceEffects::not_applied(),
    }
    state.signers.insert(signer, actor);
    let mut out = SpaceEffects::applied();
    out.notices.push(SpaceNotice::LogAppended {
        space: state.id,
        op: op_name,
    });
    match plan {
        Plan::Create {
            kind,
            history,
            first_manager,
            key_id,
        } => {
            let epoch = Epoch::FIRST;
            state.core = Some(Core {
                kind,
                history,
                key_epoch: epoch,
                created_by: actor.member,
                created_at: now,
            });
            state.history_switches.push((seq, history));
            state.epochs.insert(
                epoch,
                EpochRecord {
                    key_id,
                    seq,
                    kind: CommitKind::Create,
                    signer,
                    chained: false,
                },
            );
            let grantee = Grantee::Member(first_manager);
            let grant = new_grant(grantee, SpaceRole::Manager, None, Payer::Owner, seq, now);
            state.grants.insert(grantee, grant.clone());
            open_window(state, &grantee, after);
            out.rows.push(SpaceDelta::Grant(grant));
            out.wraps.push(WrapNeed { to: grantee, epoch });
            out.notices.push(SpaceNotice::KeyEpoch {
                space: state.id,
                epoch,
            });
        }
        Plan::Grant {
            grantee,
            role,
            cap_bytes,
            payer,
            before,
            mode,
            current,
        } => {
            let was_active = before.filter(Grant::is_active);
            let old_reads = was_active.as_ref().is_some_and(Grant::reads);
            let old_manager = was_active
                .as_ref()
                .is_some_and(|grant| grant.role == SpaceRole::Manager);
            let new_reads = role_caps(role).can_read();
            let mut grant = new_grant(grantee, role, cap_bytes, payer, seq, now);
            // An update that keeps the grantee's read access keeps where the access began; gaining read
            // access is a new beginning (the position `chain_visible_to` is asked about).
            if let Some(held) = &was_active {
                if old_reads || !new_reads {
                    grant.source = held.source;
                    grant.added_at = held.added_at;
                }
            }
            state.grants.insert(grantee, grant.clone());
            if is_direct(&grantee) {
                let now_manager = role == SpaceRole::Manager;
                if !old_manager && now_manager {
                    open_window(state, &grantee, after);
                } else if old_manager && !now_manager {
                    close_window(state, &grantee, after);
                }
            }
            out.rows.push(SpaceDelta::Grant(grant));
            if new_reads && !old_reads {
                match mode {
                    HistoryMode::Full => {
                        if mutant!("spaces_forward_style_grant_in_full") {
                            raise_due(state, RekeyReason::Joiner, &mut out);
                        } else if !mutant!("spaces_grant_wrap_missing") {
                            out.wraps.push(WrapNeed {
                                to: grantee,
                                epoch: current,
                            });
                        }
                    }
                    HistoryMode::Forward => {
                        if !mutant!("spaces_forward_grant_no_rekey_due") {
                            raise_due(state, RekeyReason::Joiner, &mut out);
                        }
                    }
                }
            }
            if old_reads && !new_reads {
                if !mutant!("spaces_revoke_no_stop_serving") {
                    out.stop_serving.push(StopServing {
                        to: grantee,
                        through: current,
                    });
                }
                if !mutant!("spaces_revoke_no_rekey_due") {
                    raise_due(state, RekeyReason::Removal, &mut out);
                }
            }
        }
        Plan::Revoke {
            grantee,
            target,
            current,
        } => {
            let mut revoked = target;
            revoked.state = GrantState::Revoked;
            revoked.removed_at = Some(now);
            state.grants.insert(grantee, revoked.clone());
            if is_direct(&grantee) && revoked.role == SpaceRole::Manager {
                close_window(state, &grantee, after);
            }
            let reads = revoked.reads();
            out.rows.push(SpaceDelta::Grant(revoked));
            if reads {
                if !mutant!("spaces_revoke_no_stop_serving") {
                    out.stop_serving.push(StopServing {
                        to: grantee,
                        through: current,
                    });
                }
                if !mutant!("spaces_revoke_no_rekey_due") {
                    raise_due(state, RekeyReason::Removal, &mut out);
                }
            }
        }
        Plan::Rekey {
            next,
            previous,
            key_id,
            mode,
        } => {
            let chained = mode == HistoryMode::Full || mutant!("spaces_rekey_chain_in_forward");
            state.epochs.insert(
                next,
                EpochRecord {
                    key_id,
                    seq,
                    kind: CommitKind::Rekey,
                    signer,
                    chained,
                },
            );
            if let Some(core) = state.core.as_mut() {
                core.key_epoch = next;
            }
            // The rotation wraps the new key to every remaining reader: a grant that is ended is not one.
            for grant in state.grants.values() {
                if grant.reads() && (grant.is_active() || mutant!("spaces_rekey_wraps_revoked")) {
                    out.wraps.push(WrapNeed {
                        to: grant.grantee,
                        epoch: next,
                    });
                }
            }
            if chained {
                out.chain_wraps.push(ChainWrapNeed {
                    of: previous,
                    under: next,
                });
            }
            out.notices.push(SpaceNotice::KeyEpoch {
                space: state.id,
                epoch: next,
            });
            let was_due = if mutant!("spaces_rekey_keeps_due") {
                None
            } else {
                state.rekey_due.take()
            };
            if was_due.is_some() {
                out.obligations
                    .push(Obligation::RekeyCleared { space: state.id });
            }
        }
        Plan::History { mode } => {
            let changed = state.core.as_ref().is_some_and(|core| core.history != mode);
            if changed {
                state.history_switches.push((seq, mode));
                if let Some(core) = state.core.as_mut() {
                    core.history = mode;
                }
            }
        }
    }
    // The chain head moved with every entry: the `reg_space` row comes first.
    if let Some(row) = state.row() {
        out.rows.insert(0, SpaceDelta::Space(row));
    }
    out
}

/// One entry of a chain to replay, with the facts the check needs.
///
/// Implements planning/architecture.md section 5.9 (a device verifies the chain entry by entry).
///
/// Caller: `replay`; the device's chain check (CHAIN-GLUE builds the facts as of each entry).
#[derive(Debug)]
pub struct ReplayStep {
    /// The entry, its signatures checked (`verify_entry`).
    pub proposal: SpaceProposal,
    /// The facts about the entry's signer.
    pub who: SignerFacts,
    /// The leader's stamp of the entry.
    pub now: Stamp,
}

/// Why a replay stopped.
///
/// Implements planning/architecture.md section 5.9 (a refused entry stops the check).
///
/// Caller: `replay`, the device's chain check.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReplayRefusal {
    /// The check refused the entry at this position.
    #[error("the entry at position {at} is refused: {why}")]
    Refused {
        /// The position of the entry.
        at: ChainSeq,
        /// Why.
        why: SpaceRefusal,
    },
    /// The entry was admitted and not applied (it does not fit the state: a damaged replay).
    #[error("the entry at position {at} was admitted and could not be applied")]
    NotApplied {
        /// The position of the entry.
        at: ChainSeq,
    },
}

/// A device's whole check of a space chain: `admit` and `apply` for every step in order, from an empty
/// state. The first refusal stops the replay. The state it returns is the state the node holds
/// (`SpaceState::state_hash` says so).
///
/// `view` is the account as the entries were made in it: for a chain that is checked long after the fact
/// the glue replays step by step with the facts as of each entry instead, and calls `admit` and `apply`
/// itself.
///
/// Implements planning/architecture.md section 5.9 ("the chain authenticates itself, any copy can
/// restore it") and section 12 E1 (every device runs its own check).
///
/// Caller: the device's chain check and the node's rebuild of a space after a restart (CHAIN-GLUE,
/// SPACES-APPLY), the tests `replay_equals_incremental_state_hash`.
pub fn replay(
    space: SpaceId,
    account: AccountId,
    steps: Vec<ReplayStep>,
    view: &dyn AccountView,
) -> Result<SpaceState, ReplayRefusal> {
    let mut state = SpaceState::new(space, account);
    for step in steps {
        let at = step.proposal.entry().seq;
        let admitted = admit(&state, &step.proposal, &step.who, view, step.now)
            .map_err(|why| ReplayRefusal::Refused { at, why })?;
        let effects = apply(&mut state, admitted);
        if !effects.applied {
            return Err(ReplayRefusal::NotApplied { at });
        }
    }
    Ok(state)
}
