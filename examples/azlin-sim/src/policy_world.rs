//! The policy tree property world (planning/architecture.md section 5.10 "groups inherit and may only
//! tighten", section 7.2, section 5.11 `policy_epoch`, section 12 E1 "a quorum counts distinct
//! people, never one person's devices").
//!
//! An account policy of a random kind, a tree of up to eight groups (parent indices), and the
//! `policy` step: it sets a random patch the way `azlin-teamlog` will (the parent's effective
//! policy, the child's policy is `apply`, accepted only if `check_tightens`), moves the account's
//! own policy, and asks `effective` for every group. Each step also draws a random list of signers
//! and compares `quorum_met` with the harness's own set count (distinct people, apps never,
//! devices of one person once). The oracles are the harness's: the epoch must rise exactly when
//! some effective policy changed, and every group must be at least as tight as its parent.

use azlin_policy::{
    apply, check_tightens, distinct_people, effective, next_epoch, quorum_met, required_people,
    AccountKind, AdminQuorum, AudienceClass, AudienceSet, AuditLevel, DeviceApproval,
    ExternalSharing, Field, ManagedDefault, PaddingPolicy, Policy, PolicyEpoch, PolicyPatch,
    QuorumClass, Signatory, UpdatePolicy,
};
use azlin_seams::sim::Rng;
use azlin_seams::{ByteCount, MemberId, Span};

use crate::config::Defect;
use crate::draw::{below, mix};

/// Which property a policy event is about.
///
/// Implements planning/architecture.md section 5.10 and section 5.11 and section 12 E1.
///
/// Caller: `PolicyEvent`, `invariants`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyKind {
    /// A group is at least as tight as its parent.
    Tightens,
    /// The epoch rises exactly when an effective policy changed.
    EpochBump,
    /// The quorum counts distinct people.
    Quorum,
}

/// What a `policy` step found: the property, whether it held, and why not.
///
/// Implements planning/architecture.md section 5.10 and section 5.11 and section 12 E1.
///
/// Caller: `PolicyWorld::step`, `invariants`.
#[derive(Clone, Debug)]
pub struct PolicyEvent {
    /// The property.
    pub kind: PolicyKind,
    /// Whether it held.
    pub ok: bool,
    /// What did not hold.
    pub message: String,
}

/// One group of the tree.
#[derive(Clone, Debug)]
struct Group {
    parent: Option<usize>,
    patch: PolicyPatch,
}

/// The policy world.
///
/// Implements planning/architecture.md section 5.10 and section 5.11.
///
/// Caller: `world::World`, tests/s1_10.
#[derive(Clone, Debug)]
pub struct PolicyWorld {
    defect: Defect,
    account: Policy,
    groups: Vec<Group>,
    epoch: PolicyEpoch,
}

/// One value out of a small pool.
fn pick<T: Copy>(rng: &mut Rng, pool: &[T]) -> Option<T> {
    rng.pick(pool).copied()
}

fn flip(rng: &mut Rng) -> bool {
    below(rng, 2) == 1
}

fn some_span(rng: &mut Rng, hours: &[u32]) -> Span {
    match pick(rng, hours) {
        Some(h) => Span::hours(h),
        None => Span::ZERO,
    }
}

fn bound(rng: &mut Rng) -> Option<Span> {
    if flip(rng) {
        None
    } else {
        Some(some_span(rng, &[1, 6, 24, 72, 720]))
    }
}

/// Sets one field of a patch to a random in-range value from a small pool.
fn set_field(patch: &mut PolicyPatch, field: Field, rng: &mut Rng) {
    match field {
        Field::CredentialTtl => patch.credential_ttl = Some(some_span(rng, &[1, 2, 6, 12, 24])),
        Field::DeviceApproval => {
            patch.device_approval = pick(
                rng,
                &DeviceApproval::ALL.iter().copied().collect::<Vec<_>>(),
            );
        }
        Field::MaxDevicesPerMember => {
            patch.max_devices_per_member = pick(rng, &[1u8, 3, 5, 10, 20]);
        }
        Field::ManagedDefault => {
            patch.managed_default = pick(
                rng,
                &ManagedDefault::ALL.iter().copied().collect::<Vec<_>>(),
            );
        }
        Field::SsoRequired => patch.sso_required = Some(flip(rng)),
        Field::SsoMaxAge => patch.sso_max_age = Some(bound(rng)),
        Field::EnrollHold => patch.enroll_hold = Some(some_span(rng, &[1, 24, 48, 72])),
        Field::PasskeyRecovery => patch.passkey_recovery = Some(flip(rng)),
        Field::EscrowOpenWait => patch.escrow_open_wait = Some(some_span(rng, &[1, 24, 72, 120])),
        Field::AuditLevel => {
            patch.audit_level = pick(rng, &AuditLevel::ALL.iter().copied().collect::<Vec<_>>());
        }
        Field::AdminQuorum => {
            let people = 1 + below(rng, 4) as u8;
            if let Ok(quorum) = AdminQuorum::new(people) {
                patch.admin_quorum = Some(quorum);
            }
        }
        Field::LinkAudiences => {
            let mut set = AudienceSet::NONE;
            for class in AudienceClass::ALL {
                if flip(rng) {
                    set = set.with(*class);
                }
            }
            patch.link_policy.audiences = Some(set);
        }
        Field::LinkMaxExpiry => patch.link_policy.max_expiry = Some(bound(rng)),
        Field::LinkPasswordForAnyone => patch.link_policy.password_for_anyone = Some(flip(rng)),
        Field::LinkForcePreviewOnly => patch.link_policy.force_preview_only = Some(flip(rng)),
        Field::LinkForceWatermark => patch.link_policy.force_watermark = Some(flip(rng)),
        Field::LinkForceAppOnly => patch.link_policy.force_app_only = Some(flip(rng)),
        Field::LinkUploads => patch.link_policy.uploads = Some(flip(rng)),
        Field::LinkEdit => patch.link_policy.edit = Some(flip(rng)),
        Field::LinkComments => patch.link_policy.comments = Some(flip(rng)),
        Field::LinkOpenLogs => patch.link_policy.open_logs = Some(flip(rng)),
        Field::LinkSlugs => patch.link_policy.slugs = Some(flip(rng)),
        Field::RequestAllowed => patch.request_policy.allowed = Some(flip(rng)),
        Field::RequestMaxExpiry => patch.request_policy.max_expiry = Some(bound(rng)),
        Field::RequestMaxFiles => {
            patch.request_policy.max_files = pick(rng, &[1u32, 10, 100, 1_000])
        }
        Field::RequestMaxBytes => {
            patch.request_policy.max_bytes = pick(rng, &[1u32, 16, 256, 4_096]).map(ByteCount::mib);
        }
        Field::BrowserDevices => patch.browser_devices = Some(flip(rng)),
        Field::ExternalSharing => {
            patch.external_sharing = pick(
                rng,
                &ExternalSharing::ALL.iter().copied().collect::<Vec<_>>(),
            );
        }
        Field::Updates => {
            patch.updates = pick(rng, &UpdatePolicy::ALL.iter().copied().collect::<Vec<_>>());
        }
        Field::Padding => {
            patch.padding = pick(rng, &PaddingPolicy::ALL.iter().copied().collect::<Vec<_>>());
        }
    }
}

/// A patch that sets 0 to 6 random fields to random in-range values (every field of `Field::ALL` is
/// reachable).
///
/// Implements planning/architecture.md section 5.10 (the account policy: one patch field for each
/// row).
///
/// Caller: `PolicyWorld::step`, tests/s1_10.
pub fn random_patch(rng: &mut Rng) -> PolicyPatch {
    let mut patch = PolicyPatch::none();
    let count = below(rng, 7);
    for _ in 0..count {
        let index = below(rng, Field::ALL.len() as u64) as usize;
        if let Some(field) = Field::ALL.get(index) {
            set_field(&mut patch, *field, rng);
        }
    }
    patch
}

impl PolicyWorld {
    /// The policy world of a seed: the kind of account and the shape of the tree come from the
    /// seed alone.
    pub fn new(seed: u64, defect: Defect) -> PolicyWorld {
        let kind = match AccountKind::ALL.get((mix(seed, 3) % 4) as usize) {
            Some(kind) => *kind,
            None => AccountKind::Personal,
        };
        let count = 1 + (mix(seed, 1) % 8) as usize;
        let groups = (0..count)
            .map(|i| Group {
                parent: if i == 0 {
                    None
                } else {
                    Some((mix(seed, 100 + i as u64) % i as u64) as usize)
                },
                patch: PolicyPatch::none(),
            })
            .collect();
        PolicyWorld {
            defect,
            account: Policy::defaults(kind),
            groups,
            epoch: PolicyEpoch::ZERO,
        }
    }

    /// The number of groups in the tree.
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    /// The epoch now.
    pub fn epoch(&self) -> PolicyEpoch {
        self.epoch
    }

    /// The patches of the groups from the account down to `group`, the topmost first.
    fn path(&self, group: usize) -> Vec<PolicyPatch> {
        let mut chain = Vec::new();
        let mut at = Some(group);
        let mut guard = 0;
        while let Some(index) = at {
            let Some(g) = self.groups.get(index) else {
                break;
            };
            chain.push(g.patch.clone());
            at = g.parent;
            guard += 1;
            if guard > self.groups.len() {
                break;
            }
        }
        chain.reverse();
        chain
    }

    /// The policy of a group: what `effective` says, or, for the faulty double, the patches applied
    /// one after the other without the clamp.
    fn policy_of(&self, group: usize) -> Policy {
        let path = self.path(group);
        if self.defect == Defect::PolicySkipsCheck {
            let mut policy = self.account.clone();
            for patch in &path {
                policy = apply(&policy, patch);
            }
            return policy;
        }
        let refs: Vec<&PolicyPatch> = path.iter().collect();
        effective(&self.account, &refs)
    }

    /// The policy a group inherits: the account's, or its parent group's.
    fn parent_policy(&self, group: usize) -> Policy {
        match self.groups.get(group).and_then(|g| g.parent) {
            Some(parent) => self.policy_of(parent),
            None => self.account.clone(),
        }
    }

    fn all_effective(&self) -> Vec<Policy> {
        let mut all = vec![self.account.clone()];
        all.extend((0..self.groups.len()).map(|g| self.policy_of(g)));
        all
    }

    /// Every group is at least as tight as its parent; the first that is not.
    fn tightening_failure(&self) -> Option<String> {
        for g in 0..self.groups.len() {
            if let Err(loosened) = check_tightens(&self.parent_policy(g), &self.policy_of(g)) {
                let first = loosened
                    .first()
                    .map(ToString::to_string)
                    .unwrap_or_default();
                return Some(format!("group {g} is looser than its parent: {first}"));
            }
        }
        None
    }

    /// One `policy` step: a patch for the account or for one group, the epoch, the quorum.
    pub fn step(
        &mut self,
        scope: u8,
        group: u8,
        patch_key: u32,
        rng: &mut Rng,
    ) -> Vec<PolicyEvent> {
        let mut r = Rng::new(rng.next_u64() ^ u64::from(patch_key));
        let patch = random_patch(&mut r);
        let before = self.all_effective();
        let (subject_before, subject_after) = if scope == 0 {
            let old = self.account.clone();
            self.account = apply(&self.account, &patch);
            (old, self.account.clone())
        } else {
            let g = usize::from(group) % self.groups.len().max(1);
            let old = self.policy_of(g);
            let parent = self.parent_policy(g);
            let child = apply(&parent, &patch);
            let accepted =
                self.defect == Defect::PolicySkipsCheck || check_tightens(&parent, &child).is_ok();
            if accepted {
                if let Some(slot) = self.groups.get_mut(g) {
                    slot.patch = patch.clone();
                }
            }
            (old, self.policy_of(g))
        };
        let after = self.all_effective();
        let changed = before != after;
        let expected = if changed {
            self.epoch.next()
        } else {
            Some(self.epoch)
        };
        let got = if self.defect == Defect::PolicyAlwaysBumps {
            self.epoch.next()
        } else {
            next_epoch(self.epoch, &subject_before, &subject_after).ok()
        };
        let epoch_ok = got == expected;
        if let Some(next) = got {
            self.epoch = next;
        }
        let mut events = vec![
            PolicyEvent {
                kind: PolicyKind::Tightens,
                ok: self.tightening_failure().is_none(),
                message: self.tightening_failure().unwrap_or_default(),
            },
            PolicyEvent {
                kind: PolicyKind::EpochBump,
                ok: epoch_ok,
                message: format!(
                    "the policy epoch went to {got:?}, the effective policies {} (expected {expected:?})",
                    if changed { "changed" } else { "did not change" }
                ),
            },
        ];
        events.push(self.quorum_event(&mut r));
        events
    }

    /// A random list of signers against the harness's own count.
    fn quorum_event(&self, r: &mut Rng) -> PolicyEvent {
        let class = match r.pick(QuorumClass::ALL) {
            Some(class) => *class,
            None => QuorumClass::Other,
        };
        let len = below(r, 8) as usize;
        let signers: Vec<Signatory> = (0..len)
            .map(|_| {
                let member = MemberId::from_bytes([1 + below(r, 4) as u8; 16]);
                if below(r, 5) == 0 {
                    Signatory::app(member)
                } else {
                    Signatory::person(member)
                }
            })
            .collect();
        let people: std::collections::BTreeSet<[u8; 16]> = signers
            .iter()
            .filter(|s| s.kind == azlin_policy::MemberKind::Person)
            .map(|s| *s.member.as_bytes())
            .collect();
        let apps: std::collections::BTreeSet<[u8; 16]> = signers
            .iter()
            .filter(|s| s.kind == azlin_policy::MemberKind::App)
            .map(|s| *s.member.as_bytes())
            .collect();
        let count = people.difference(&apps).count();
        let need = match class {
            QuorumClass::Other => 1,
            QuorumClass::AdminChange
            | QuorumClass::PolicyChange
            | QuorumClass::EscrowChange
            | QuorumClass::EscrowOpen
            | QuorumClass::QuorumEnrollment => usize::from(self.account.admin_quorum.get()),
        };
        let crate_count = if self.defect == Defect::QuorumCountsDevices {
            signers.len()
        } else {
            usize::from(distinct_people(&signers))
        };
        let crate_need = usize::from(required_people(&self.account, class));
        let crate_ok = if self.defect == Defect::QuorumCountsDevices {
            crate_count >= crate_need
        } else {
            quorum_met(&self.account, class, &signers).is_ok()
        };
        let expected_ok = count >= need;
        let ok = crate_count == count && crate_need == need && crate_ok == expected_ok;
        PolicyEvent {
            kind: PolicyKind::Quorum,
            ok,
            message: format!(
                "{} signatures, class {class}: the harness counts {count} people and needs {need}; the crate counts {crate_count}, needs {crate_need} and says {crate_ok}",
                signers.len()
            ),
        }
    }
}
