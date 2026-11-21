//! The universe of one account: who exists, in which role, in which group, with which devices, and the
//! plain bytes of every id and key (planning/architecture.md section 12 E1: "random sequences of ops, group
//! moves and delegations"; section 7.2 the roles and the group hierarchy; lane W1-S47a).
//!
//! A [`Universe`] is a PLAN, not a state: it names the members, groups, devices, delegations and app
//! principals that the setup of a sequence creates through real (valid) entries, and the spare ids that the
//! random phase may use for new members, devices and groups. It imports nothing from `azlin-teamlog`: the
//! ids are the model's own small numbers, and this file turns them into the bytes the production types are
//! built from (the converter in `compare.rs` is the only place that builds a production id).
//!
//! Every function is a pure function of its arguments and of the seeded [`Rng`]: the same seed gives the
//! same universe on every platform.

use azlin_seams::sim::Rng;
use azlin_seams::Span;

use crate::model::{AcctKind, AppId, Dev, Grp, Key, MKind, Mem, OpKind, Role};

/// The tag byte of a member id (the first byte of its 16 bytes).
const TAG_MEM: u8 = 0x4D;
/// The tag byte of a device id.
const TAG_DEV: u8 = 0x44;
/// The tag byte of a group id.
const TAG_GRP: u8 = 0x47;
/// The tag byte of an account id.
const TAG_ACCOUNT: u8 = 0x41;
/// The tag byte of a device's public key.
const TAG_DEV_KEY: u8 = 0xD1;
/// The tag byte of a member's public key.
const TAG_MEM_KEY: u8 = 0xE1;
/// The tag byte of a delegate key.
const TAG_DELEGATE_KEY: u8 = 0xC1;
/// The tag byte of the payments key the check trusts.
const TAG_PAYMENTS_KEY: u8 = 0xB1;
/// The tag byte of a payments key the check does NOT trust.
const TAG_PAYMENTS_OTHER: u8 = 0xB2;
/// The tag byte of an escrow or audit public key.
const TAG_BLOB_KEY: u8 = 0xF1;
/// The tag byte of a key that nobody holds (a signature "by somebody else").
const TAG_STRANGER_KEY: u8 = 0xA9;

/// An id of a member that no universe ever creates (a subject that does not exist).
pub const UNKNOWN_MEM: Mem = Mem(9000);
/// An id of a device that no universe ever creates.
pub const UNKNOWN_DEV: Dev = Dev(9000);
/// An id of a group that no universe ever creates.
pub const UNKNOWN_GRP: Grp = Grp(9000);
/// A delegate key that no delegation ever names.
pub const UNKNOWN_KEY: Key = Key(9000);

/// A 16-byte id: the tag, the number in big-endian, zeros.
fn tagged16(tag: u8, n: u16) -> [u8; 16] {
    let [hi, lo] = n.to_be_bytes();
    core::array::from_fn(|i| match i {
        0 => tag,
        1 => hi,
        2 => lo,
        _ => 0,
    })
}

/// A 32-byte key: the tag, the number in big-endian, a fixed fill.
fn tagged32(tag: u8, n: u16) -> [u8; 32] {
    let [hi, lo] = n.to_be_bytes();
    core::array::from_fn(|i| match i {
        0 => tag,
        1 => hi,
        2 => lo,
        _ => 0x5A,
    })
}

/// The number inside a tagged array, if the array has exactly the shape `tagged16`/`tagged32` makes.
fn untag<const N: usize>(bytes: &[u8; N], tag: u8, fill: u8) -> Option<u16> {
    let mut it = bytes.iter();
    let first = it.next()?;
    let hi = it.next()?;
    let lo = it.next()?;
    if *first != tag {
        return None;
    }
    if it.any(|b| *b != fill) {
        return None;
    }
    Some(u16::from_be_bytes([*hi, *lo]))
}

/// The 16 bytes of a member id.
///
/// Implements planning/architecture.md section 5.3 (`member_id`, pseudonymous); Caller: `compare.rs`.
pub fn mem_bytes(m: Mem) -> [u8; 16] {
    tagged16(TAG_MEM, m.0)
}

/// The member id of 16 bytes made by [`mem_bytes`]; `None` for any other bytes.
///
/// Implements planning/architecture.md section 5.3; Caller: `project.rs`.
pub fn mem_from_bytes(bytes: &[u8; 16]) -> Option<Mem> {
    untag(bytes, TAG_MEM, 0).map(Mem)
}

/// The 16 bytes of a device id.
///
/// Implements planning/architecture.md section 5.3 (`device_id`); Caller: `compare.rs`.
pub fn dev_bytes(d: Dev) -> [u8; 16] {
    tagged16(TAG_DEV, d.0)
}

/// The device id of 16 bytes made by [`dev_bytes`]; `None` for any other bytes.
///
/// Implements planning/architecture.md section 5.3; Caller: `project.rs`.
pub fn dev_from_bytes(bytes: &[u8; 16]) -> Option<Dev> {
    untag(bytes, TAG_DEV, 0).map(Dev)
}

/// The 16 bytes of a group id.
///
/// Implements planning/architecture.md section 5.3 (`reg_group`); Caller: `compare.rs`.
pub fn grp_bytes(g: Grp) -> [u8; 16] {
    tagged16(TAG_GRP, g.0)
}

/// The group id of 16 bytes made by [`grp_bytes`]; `None` for any other bytes.
///
/// Implements planning/architecture.md section 5.3; Caller: `project.rs`.
pub fn grp_from_bytes(bytes: &[u8; 16]) -> Option<Grp> {
    untag(bytes, TAG_GRP, 0).map(Grp)
}

/// The 16 bytes of the account id.
///
/// Implements planning/architecture.md section 5.9 (`account`); Caller: `compare.rs`.
pub fn account_bytes(n: u16) -> [u8; 16] {
    tagged16(TAG_ACCOUNT, n)
}

/// The Ed25519 "public key" of a device: the key its entries are signed with (the fake scheme's key id).
///
/// Implements planning/architecture.md section 5.9 (`sig`: "every entry is signed by a device");
/// Caller: `compare.rs`.
pub fn device_key_bytes(d: Dev) -> [u8; 32] {
    tagged32(TAG_DEV_KEY, d.0)
}

/// The signing key of a member (the `member_keys` of a genesis and a join).
///
/// Implements planning/architecture.md section 5.3; Caller: `compare.rs`.
pub fn member_key_bytes(m: Mem) -> [u8; 32] {
    tagged32(TAG_MEM_KEY, m.0)
}

/// The key of a delegate (the identity service, the key agent): the bytes that verify its signatures.
///
/// Implements planning/architecture.md section 7.3 (the identity service's authority is a `delegate`
/// entry: its key and the exact ops it may sign); Caller: `compare.rs`.
pub fn delegate_key_bytes(k: Key) -> [u8; 32] {
    tagged32(TAG_DELEGATE_KEY, k.0)
}

/// The delegate key of 32 bytes made by [`delegate_key_bytes`]; `None` for any other bytes.
///
/// Implements planning/architecture.md section 7.3; Caller: `project.rs`.
pub fn delegate_key_from_bytes(bytes: &[u8; 32]) -> Option<Key> {
    untag(bytes, TAG_DELEGATE_KEY, 0x5A).map(Key)
}

/// The payments service's key; `trusted` says whether the check trusts it (an entitlement signed by an
/// untrusted payments key must be refused).
///
/// Implements planning/architecture.md section 5.9 (`entitlement`: "replayed from the payments service's
/// submission"); Caller: `compare.rs`.
pub fn payments_key_bytes(trusted: bool) -> [u8; 32] {
    if trusted {
        tagged32(TAG_PAYMENTS_KEY, 1)
    } else {
        tagged32(TAG_PAYMENTS_OTHER, 1)
    }
}

/// The bytes of an escrow or audit public key (a number of the model).
///
/// Implements planning/architecture.md section 5.9 (`escrow_set`, `audit_key_set`); Caller:
/// `compare.rs`, `project.rs`.
pub fn blob_key_bytes(n: u16) -> [u8; 32] {
    tagged32(TAG_BLOB_KEY, n)
}

/// The number of a key made by [`blob_key_bytes`].
///
/// Implements planning/architecture.md section 5.9; Caller: `project.rs`.
pub fn blob_key_from_bytes(bytes: &[u8; 32]) -> Option<u16> {
    untag(bytes, TAG_BLOB_KEY, 0x5A)
}

/// A key that no member and no delegation holds: a signature made with it is "by somebody else".
///
/// Implements planning/architecture.md section 5.9 (a bad signature is refused); Caller: `compare.rs`.
pub fn stranger_key_bytes(n: u16) -> [u8; 32] {
    tagged32(TAG_STRANGER_KEY, n)
}

/// A number in `lo..=hi`; an empty range gives `lo` (every call site passes `lo <= hi`).
pub(crate) fn between(rng: &mut Rng, lo: usize, hi: usize) -> usize {
    let upper = (hi as u64).saturating_add(1);
    match rng.range(lo as u64, upper) {
        Some(v) => v as usize,
        None => lo,
    }
}

/// True with the probability `p` percent.
pub(crate) fn pct(rng: &mut Rng, p: u32) -> bool {
    rng.chance(p, 100)
}

/// One element of a slice, uniform; `None` for an empty slice.
pub(crate) fn pick<'a, T>(rng: &mut Rng, items: &'a [T]) -> Option<&'a T> {
    rng.pick(items)
}

/// One member the setup creates.
///
/// Implements planning/architecture.md section 7.2 (members, roles, groups); Caller: `gen.rs` (the
/// setup of a sequence).
#[derive(Clone, Debug)]
pub struct PlannedMember {
    /// The member's id.
    pub mem: Mem,
    /// A person or an app principal.
    pub kind: MKind,
    /// The role the setup gives the member (the first member is the owner).
    pub role: Role,
    /// The group whose subtree a group admin or a helpdesk member acts in.
    pub scope: Option<Grp>,
    /// The groups the member belongs to.
    pub groups: Vec<Grp>,
    /// The member's devices, the first is the one it joins with.
    pub devices: Vec<Dev>,
    /// The apps an app principal may sign for.
    pub app_scopes: Vec<AppId>,
    /// Whether the setup suspends the member at the end.
    pub suspended: bool,
    /// A device that admins enrol by quorum (it starts in its hold).
    pub pending_device: Option<Dev>,
    /// Whether the setup makes the (private) member managed, with its own device's consent.
    pub make_managed: bool,
}

/// One group the setup creates.
///
/// Implements planning/architecture.md section 7.2 ("Groups nest (a department tree)"); Caller: `gen.rs`.
#[derive(Clone, Debug)]
pub struct PlannedGroup {
    /// The group's id.
    pub grp: Grp,
    /// The parent, `None` for a top-level group.
    pub parent: Option<Grp>,
}

/// One delegation the setup records.
///
/// Implements planning/architecture.md section 7.3 (`delegate`); Caller: `gen.rs`.
#[derive(Clone, Debug)]
pub struct PlannedDelegation {
    /// The delegate key.
    pub key: Key,
    /// The ops it may sign.
    pub ops: Vec<OpKind>,
    /// How long after the delegation entry it lives; `None`: for ever.
    pub lives: Option<Span>,
}

/// The universe of one account (see the module).
///
/// Implements planning/architecture.md section 12 E1; Caller: `compare.rs` (`differential`), `gen.rs`,
/// the tests.
#[derive(Clone, Debug)]
pub struct Universe {
    /// The kind of the account.
    pub kind: AcctKind,
    /// The account's number (the account id is built from it).
    pub account: u16,
    /// The members of the setup, the first is the genesis owner.
    pub members: Vec<PlannedMember>,
    /// The groups of the setup, parents before children.
    pub groups: Vec<PlannedGroup>,
    /// The delegations of the setup.
    pub delegations: Vec<PlannedDelegation>,
    /// Member ids the random phase may invite.
    pub spare_members: Vec<Mem>,
    /// Device ids the random phase may enrol or join with.
    pub spare_devices: Vec<Dev>,
    /// Group ids the random phase may create.
    pub spare_groups: Vec<Grp>,
    /// The delegate keys that exist.
    pub delegate_keys: Vec<Key>,
    /// The apps that exist.
    pub apps: Vec<AppId>,
    /// The quorum the setup raises the account to (an account of kind team, personal or family starts
    /// at 1 and can make admins; a business starts at 2 and cannot).
    pub quorum_target: u8,
}

impl Universe {
    /// Every group id of the setup.
    ///
    /// Implements planning/architecture.md section 7.2; Caller: `gen.rs`.
    pub fn group_ids(&self) -> Vec<Grp> {
        self.groups.iter().map(|g| g.grp).collect()
    }

    /// The planned parent of a group of the setup.
    ///
    /// Implements planning/architecture.md section 7.2; Caller: `gen.rs`.
    pub fn planned_parent(&self, g: Grp) -> Option<Grp> {
        self.groups
            .iter()
            .find(|p| p.grp == g)
            .and_then(|p| p.parent)
    }

    /// The planned member with this id.
    ///
    /// Implements planning/architecture.md section 7.2; Caller: `gen.rs`.
    pub fn planned(&self, m: Mem) -> Option<&PlannedMember> {
        self.members.iter().find(|p| p.mem == m)
    }
}

/// The depth of a planned group (a top-level group is 0).
fn planned_depth(groups: &[PlannedGroup], g: Grp) -> usize {
    let mut depth = 0usize;
    let mut current = g;
    while let Some(parent) = groups
        .iter()
        .find(|p| p.grp == current)
        .and_then(|p| p.parent)
    {
        depth += 1;
        current = parent;
        if depth > 8 {
            break;
        }
    }
    depth
}

/// Whether `g` lies in the planned subtree of `root` (`root` itself included).
fn planned_in_subtree(groups: &[PlannedGroup], g: Grp, root: Grp) -> bool {
    let mut current = g;
    for _ in 0..10 {
        if current == root {
            return true;
        }
        match groups
            .iter()
            .find(|p| p.grp == current)
            .and_then(|p| p.parent)
        {
            Some(parent) => current = parent,
            None => return false,
        }
    }
    false
}

/// Draws the universe of a seeded sequence: 3 to 8 members (the first is the owner), a group tree of 3
/// to 8 groups at most 4 levels deep, 1 to 3 devices each, a pending device, a suspended member, app
/// principals and delegations. Eight in ten universes are "rich" (every role present as far as the member
/// count allows); the others draw their roles at random.
///
/// Implements planning/architecture.md section 12 E1 (a universe of accounts, members and roles) and
/// section 7.2 (roles and groups); Caller: `compare.rs` (`differential`), the tests.
pub fn gen_universe(rng: &mut Rng, kind: AcctKind) -> Universe {
    // The group tree.
    let group_count = between(rng, 3, 8);
    let mut groups: Vec<PlannedGroup> = Vec::new();
    for i in 0..group_count {
        let grp = Grp(i as u16 + 1);
        let parent = if groups.is_empty() || pct(rng, 25) {
            None
        } else {
            let candidates: Vec<Grp> = groups
                .iter()
                .map(|g| g.grp)
                .filter(|g| planned_depth(&groups, *g) < 3)
                .collect();
            pick(rng, &candidates).copied()
        };
        groups.push(PlannedGroup { grp, parent });
    }
    let group_ids: Vec<Grp> = groups.iter().map(|g| g.grp).collect();

    // The members and their roles.
    let member_count = between(rng, 3, 8);
    let rich = pct(rng, 80);
    let rich_roles = [
        Role::Admin,
        Role::GroupAdmin,
        Role::Helpdesk,
        Role::Member,
        Role::Guest,
        Role::Admin,
        Role::Member,
    ];
    let mut members: Vec<PlannedMember> = Vec::new();
    for i in 0..member_count {
        let mem = Mem(i as u16 + 1);
        let role = if i == 0 {
            Role::Owner
        } else if rich {
            rich_roles.get(i - 1).copied().unwrap_or(Role::Member)
        } else {
            let all = [
                Role::Owner,
                Role::Admin,
                Role::GroupAdmin,
                Role::Helpdesk,
                Role::Member,
                Role::Member,
                Role::Guest,
            ];
            pick(rng, &all).copied().unwrap_or(Role::Member)
        };
        let device_count = between(rng, 1, 3);
        let devices: Vec<Dev> = (0..device_count)
            .map(|k| Dev(mem.0 * 10 + k as u16 + 1))
            .collect();
        let scope = if role.needs_scope() {
            pick(rng, &group_ids).copied()
        } else {
            None
        };
        let mut member_groups: Vec<Grp> = Vec::new();
        if i != 0 {
            let how_many = between(rng, 0, 2);
            for _ in 0..how_many {
                if let Some(g) = pick(rng, &group_ids) {
                    if !member_groups.contains(g) {
                        member_groups.push(*g);
                    }
                }
            }
        }
        members.push(PlannedMember {
            mem,
            kind: MKind::Person,
            role,
            scope,
            groups: member_groups,
            devices,
            app_scopes: Vec::new(),
            suspended: false,
            pending_device: None,
            make_managed: false,
        });
    }

    // A last member may be an app principal.
    let apps = vec![AppId(1), AppId(2), AppId(3)];
    if member_count >= 5 && pct(rng, 35) {
        if let Some(last) = members.last_mut() {
            last.kind = MKind::App;
            last.role = Role::Member;
            last.scope = None;
            last.app_scopes = vec![AppId(1)];
        }
    }

    // Members inside the scope of every scoped member: a group admin or a helpdesk member with nobody in
    // its subtree could attack nothing.
    let scoped: Vec<(Mem, Grp)> = members
        .iter()
        .filter_map(|m| m.scope.map(|s| (m.mem, s)))
        .collect();
    for (owner_of_scope, scope) in scoped {
        let inside: Vec<Grp> = group_ids
            .iter()
            .copied()
            .filter(|g| planned_in_subtree(&groups, *g, scope))
            .collect();
        for m in members.iter_mut() {
            if m.mem == owner_of_scope || m.mem == Mem(1) {
                continue;
            }
            if m.kind == MKind::Person && pct(rng, 45) {
                if let Some(g) = pick(rng, &inside) {
                    if !m.groups.contains(g) {
                        m.groups.push(*g);
                    }
                }
            }
        }
    }

    // A suspended member, a pending device, a managed member.
    if rich && pct(rng, 40) {
        let candidates: Vec<usize> = members
            .iter()
            .enumerate()
            .filter(|(i, m)| *i != 0 && matches!(m.role, Role::Admin | Role::Member))
            .map(|(i, _)| i)
            .collect();
        if let Some(i) = pick(rng, &candidates) {
            if let Some(m) = members.get_mut(*i) {
                m.suspended = true;
            }
        }
    }
    for m in members.iter_mut() {
        if m.kind == MKind::Person && pct(rng, 30) {
            m.pending_device = Some(Dev(m.mem.0 * 10 + 9));
        }
        if m.kind == MKind::Person && pct(rng, 40) {
            m.make_managed = true;
        }
    }

    // Delegations: a live one, and sometimes one that dies soon.
    let delegate_keys = vec![Key(1), Key(2), Key(3)];
    let mut delegations: Vec<PlannedDelegation> = Vec::new();
    if pct(rng, 70) {
        delegations.push(PlannedDelegation {
            key: Key(1),
            ops: vec![OpKind::MemberSuspend, OpKind::MemberInvite],
            lives: None,
        });
    }
    if pct(rng, 40) {
        delegations.push(PlannedDelegation {
            key: Key(2),
            ops: vec![OpKind::MemberSuspend],
            lives: Some(Span::minutes(between(rng, 20, 600) as u32)),
        });
    }

    let admin_class = members
        .iter()
        .filter(|m| m.kind == MKind::Person && m.role.is_admin_class() && !m.suspended)
        .count();
    let quorum_target = match kind {
        AcctKind::Business => 2,
        _ => {
            let want = between(rng, 1, 3);
            want.min(admin_class.max(1)) as u8
        }
    };

    Universe {
        kind,
        account: 1,
        members,
        groups,
        delegations,
        spare_members: (101..=106).map(Mem).collect(),
        spare_devices: (500..=530).map(Dev).collect(),
        spare_groups: (101..=104).map(Grp).collect(),
        delegate_keys,
        apps,
        quorum_target,
    }
}
