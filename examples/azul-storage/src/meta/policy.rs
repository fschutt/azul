//! The drive's policy and its members' key wraps in the drive index (feature `encryption`):
//! membership and access are files of the repository, so every change of them is a commit -
//! versioned, merged and restorable like the drive's own files.
//!
//! - `.azlin/policy.toml` ([`Policy`]): the members (id, name, role) and the folder grants.
//! - `.azlin/keys/<member>.key`: the drive key wrapped for a member (the bytes the encrypted
//!   drive's key files hold, `crate::crypto::keys`).
//!
//! `.azlin` is the drive's own folder: the drive's listing does not show it, and the encrypted
//! drive cannot write it (`super::index`); only these calls change it.
//!
//! **Roles are advisory.** Every member holds the drive key, and the key opens everything the
//! drive stores - the files, the policy, the manifest - so no check in this crate can stop a
//! member that wants to read or rewrite what its role does not allow: [`Policy::allows`] and
//! [`Policy::administers`] say what an app offers a member, not what a device can do. What takes
//! access away for good is a new drive key (`crate::rotation`), which the removed member never
//! receives. What this module does keep honest is the record itself:
//!
//! - **One effective policy.** Two devices that rewrite the policy (or one key wrap) at the same
//!   time never leave one version effective and the other in a conflict copy: the rewrites are
//!   merged, and what either device removed stays removed ([`Policy::merge`]; a member's wrap
//!   that one device removed and another wrapped again stays removed).
//! - **A revision that only grows.** Every rewrite of an existing policy makes
//!   [`Policy::revision`] greater than the policy it replaces; [`MetaRepo::set_policy`] refuses
//!   a policy read before another rewrite landed on this device, so an older policy is never
//!   written back over a newer one by accident.
//!
//! [`MemberRecord`] is how the key flows (`crate::crypto::device`, `crate::rotation`) use the
//! policy. A device that holds the drive key reads the members from it, and every change of
//! the members is committed to it before the bucket's key file changes. The bucket's
//! `.azlin/keys/<member>.key` files stay as the copy a device without the drive key opens
//! (the repository is sealed with the drive key), and a device the policy no longer names
//! is refused even when its key file is still in the bucket. A drive whose index has no
//! policy yet keeps its key files as the record until its first member change, which
//! writes the policy from them (every member key file an owner) in the same commit. A
//! bucket without an index keeps its key files as the record.
//!
//! ```toml
//! [members.d_laptop]
//! name = "Laptop"
//! role = "owner"
//!
//! [members.d_anna]
//! name = "Anna"
//! role = "guest"
//!
//! [[grants]]
//! member = "d_anna"
//! folder = "photos/2026/"
//! access = "read"
//! ```

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{
    bucket::{Bucket, DriveBucket},
    index::to_drive,
    merge::{Conflict, Resolution, Side},
    objects::{is_valid_name, Mode, ObjectId},
    repo::{CommitOutcome, MetaRepo, RepoOptions},
    seal::Sealer,
    tree::{entry_at, folder_at, Change},
    MetaError,
};
use crate::{
    crypto::{keys::load_member_wraps, DriveKey},
    Drive, DriveError,
};

/// Where the policy lives in the repository.
pub const POLICY_PATH: &str = ".azlin/policy.toml";
/// Where the members' key wraps live in the repository.
pub const KEYS_FOLDER: &str = ".azlin/keys";

/// What a member may do everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Everything, the members and the policy too.
    Owner,
    /// Reads and writes every file.
    Editor,
    /// Reads every file.
    Viewer,
    /// Only what the grants name.
    Guest,
}

impl Role {
    /// The role with less to do: what two devices that changed one member's role in different
    /// ways are merged to.
    fn lesser(self, other: Role) -> Role {
        fn rank(role: Role) -> u8 {
            match role {
                Role::Guest => 0,
                Role::Viewer => 1,
                Role::Editor => 2,
                Role::Owner => 3,
            }
        }
        if rank(other) < rank(self) {
            other
        } else {
            self
        }
    }
}

/// What a grant allows in its folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    Read,
    Write,
}

impl Access {
    /// The access with less to do: reading, when one of the two only reads.
    fn lesser(self, other: Access) -> Access {
        if self == Access::Read || other == Access::Read {
            Access::Read
        } else {
            Access::Write
        }
    }
}

/// A member of the drive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// As the apps show it.
    pub name: String,
    pub role: Role,
}

/// Access to one folder (and everything in it) for one member, beyond the role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    pub member: String,
    /// `photos/2026/`; empty: the whole drive.
    pub folder: String,
    pub access: Access,
}

/// The drive's members, their roles and the folder grants: `.azlin/policy.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    /// How many times the policy was rewritten: 0 for its first version (and for a policy
    /// written before the count existed), and every rewrite of an existing policy -
    /// [`MetaRepo::set_policy`], [`MemberRecord::record`], the merge of two rewrites made at
    /// once - makes it greater than the one it replaces. It only grows; it is not written
    /// while it is 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub revision: u64,
    /// By member id.
    #[serde(default)]
    pub members: BTreeMap<String, Member>,
    #[serde(default)]
    pub grants: Vec<Grant>,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// Whether `path` is `folder` or inside it (`""`: the whole drive).
fn within(path: &str, folder: &str) -> bool {
    let folder = folder.trim_end_matches('/');
    folder.is_empty()
        || path == folder
        || path
            .strip_prefix(folder)
            .is_some_and(|rest| rest.starts_with('/'))
}

impl Policy {
    /// The policy as TOML text.
    pub fn to_toml(&self) -> Result<String, MetaError> {
        toml::to_string(self).map_err(|e| MetaError::Corrupt {
            key: POLICY_PATH.to_string(),
            reason: e.to_string(),
        })
    }

    /// The policy of TOML text.
    pub fn from_toml(text: &str) -> Result<Policy, MetaError> {
        toml::from_str(text).map_err(|e| MetaError::Corrupt {
            key: POLICY_PATH.to_string(),
            reason: e.to_string(),
        })
    }

    /// Whether `member` may read `path` (`write` false) or write it. Owners and editors do
    /// everything, viewers read everything; a grant adds a folder for a member; who is no
    /// member does nothing. Advisory: it says what an app offers the member; the member's
    /// device holds the drive key and could do the rest (see the module).
    #[must_use]
    pub fn allows(&self, member: &str, path: &str, write: bool) -> bool {
        let Some(role) = self.members.get(member).map(|m| m.role) else {
            return false;
        };
        match role {
            Role::Owner | Role::Editor => true,
            Role::Viewer if !write => true,
            Role::Viewer | Role::Guest => self.grants.iter().any(|grant| {
                grant.member == member
                    && within(path, &grant.folder)
                    && (!write || grant.access == Access::Write)
            }),
        }
    }

    /// Whether `member` may change the members and the policy. Advisory, like
    /// [`Policy::allows`]: the key flows record a change for any device that holds the drive
    /// key.
    #[must_use]
    pub fn administers(&self, member: &str) -> bool {
        self.members.get(member).is_some_and(|m| m.role == Role::Owner)
    }

    /// The policy two devices that rewrote `base` at the same time (`mine` and `theirs`) agree
    /// on. What either of them removed stays removed - a revoke is never lost to a rewrite made
    /// without it - and what one of them added is kept. A member or a grant the two changed in
    /// different ways gets the lesser role or access, and the grants of a member that is gone
    /// go with it. The grants come out sorted. Its revision is greater than both sides'.
    #[must_use]
    pub fn merge(base: &Policy, mine: &Policy, theirs: &Policy) -> Policy {
        let mut merged = Policy {
            revision: mine.revision.max(theirs.revision).saturating_add(1),
            ..Policy::default()
        };
        let ids: BTreeSet<&String> = mine.members.keys().chain(theirs.members.keys()).collect();
        for id in ids {
            let was = base.members.get(id);
            let member = match (mine.members.get(id), theirs.members.get(id)) {
                (Some(m), Some(t)) => Member {
                    name: if t.name == m.name || was.is_some_and(|w| w.name == t.name) {
                        m.name.clone()
                    } else {
                        t.name.clone()
                    },
                    role: if m.role == t.role || was.is_some_and(|w| w.role == t.role) {
                        m.role
                    } else if was.is_some_and(|w| w.role == m.role) {
                        t.role
                    } else {
                        m.role.lesser(t.role)
                    },
                },
                // On one side only: added there, unless the other side removed it.
                (Some(only), None) | (None, Some(only)) => {
                    if was.is_some() {
                        continue;
                    }
                    only.clone()
                }
                (None, None) => continue,
            };
            merged.members.insert(id.clone(), member);
        }
        let was = grants_by_folder(base);
        let ours = grants_by_folder(mine);
        let theirs = grants_by_folder(theirs);
        let keys: BTreeSet<(&str, &str)> = ours.keys().chain(theirs.keys()).copied().collect();
        for key in keys {
            let before = was.get(&key).copied();
            let access = match (ours.get(&key).copied(), theirs.get(&key).copied()) {
                (Some(m), Some(t)) => {
                    if m == t || before == Some(t) {
                        m
                    } else if before == Some(m) {
                        t
                    } else {
                        m.lesser(t)
                    }
                }
                (Some(only), None) | (None, Some(only)) => {
                    if before.is_some() {
                        continue;
                    }
                    only
                }
                (None, None) => continue,
            };
            if merged.members.contains_key(key.0) {
                merged.grants.push(Grant {
                    member: key.0.to_string(),
                    folder: key.1.to_string(),
                    access,
                });
            }
        }
        merged
    }
}

/// The grants of `policy` by member and folder; a pair listed twice has the access that
/// allows more (as [`Policy::allows`] reads it).
fn grants_by_folder(policy: &Policy) -> BTreeMap<(&str, &str), Access> {
    let mut out = BTreeMap::new();
    for grant in &policy.grants {
        let access = out
            .entry((grant.member.as_str(), grant.folder.as_str()))
            .or_insert(grant.access);
        if grant.access == Access::Write {
            *access = Access::Write;
        }
    }
    out
}

fn key_path(member: &str) -> Result<String, MetaError> {
    let name = format!("{member}.key");
    if !is_valid_name(&name) || member.is_empty() {
        return Err(MetaError::Corrupt {
            key: member.to_string(),
            reason: "not a member id".to_string(),
        });
    }
    Ok(format!("{KEYS_FOLDER}/{name}"))
}

impl<B: Bucket, S: Sealer> MetaRepo<B, S> {
    /// The drive's policy; `None` while it has none.
    pub fn policy(&self) -> Result<Option<Policy>, MetaError> {
        let Some(root) = self.root()? else {
            return Ok(None);
        };
        match entry_at(self.objects(), &root, POLICY_PATH)? {
            Some(entry) if entry.mode == Mode::File => self.read_policy(&entry.id).map(Some),
            _ => Ok(None),
        }
    }

    /// The policy the blob `id` holds.
    fn read_policy(&self, id: &ObjectId) -> Result<Policy, MetaError> {
        let text = String::from_utf8(self.objects().blob(id)?.to_vec()).map_err(|_| {
            MetaError::Corrupt {
                key: POLICY_PATH.to_string(),
                reason: "not UTF-8".to_string(),
            }
        })?;
        Policy::from_toml(&text)
    }

    /// The policy one side of a conflict holds; no file reads as an empty policy.
    fn policy_of_side(&self, side: &Side) -> Result<Policy, MetaError> {
        match side {
            None => Ok(Policy::default()),
            Some((Mode::File, id)) => self.read_policy(id),
            Some(_) => Err(MetaError::Corrupt {
                key: POLICY_PATH.to_string(),
                reason: "not a file".to_string(),
            }),
        }
    }

    /// Commits `policy` as the drive's policy, as the next revision of the one this device
    /// holds ([`Policy::revision`]). A `policy` read before another rewrite reached this device
    /// is refused ([`MetaError::Conflict`]: read the policy again and make the change on it).
    /// When another device rewrites the policy at the same moment the two are merged
    /// ([`Policy::merge`]): what either removed stays removed, and no conflict copy is left
    /// next to the policy.
    pub fn set_policy(&mut self, policy: &Policy) -> Result<CommitOutcome, MetaError> {
        self.ensure_entry(POLICY_PATH)?;
        let current = self.policy()?;
        let mut wanted = policy.clone();
        if let Some(current) = &current {
            if policy.revision < current.revision {
                return Err(MetaError::Conflict {
                    key: POLICY_PATH.to_string(),
                });
            }
            wanted.revision = policy.revision.saturating_add(1);
        }
        let mut outcome = self.commit_policy(&wanted)?;
        for _ in 0..RECORD_ATTEMPTS {
            let Some(clash) = outcome
                .conflicts
                .iter()
                .find(|resolved| resolved.conflict.path == POLICY_PATH)
                .map(|resolved| resolved.conflict.clone())
            else {
                return Ok(outcome);
            };
            // The other device's policy stands in the tree now; the merge of the two goes on
            // top of it, unless theirs holds everything already.
            let base = self.policy_of_side(&clash.base)?;
            let theirs = self.policy_of_side(&clash.theirs)?;
            let merged = Policy::merge(&base, &wanted, &theirs);
            if merged.members == theirs.members && merged.grants == theirs.grants {
                return Ok(outcome);
            }
            wanted = merged;
            outcome = self.commit_policy(&wanted)?;
        }
        Err(MetaError::Contended {
            attempts: RECORD_ATTEMPTS as u32,
        })
    }

    /// Commits `policy` as it is; the other device's policy on a conflict (the caller merges).
    fn commit_policy(&mut self, policy: &Policy) -> Result<CommitOutcome, MetaError> {
        let id = self.write_blob(policy.to_toml()?.as_bytes());
        self.commit(
            &[Change::Put {
                path: POLICY_PATH.to_string(),
                id,
            }],
            "Policy\n",
            &mut |_: &Conflict| Resolution::TakeTheirs,
        )
    }

    /// The members' key wraps the drive holds, by member id.
    pub fn member_wraps(&self) -> Result<BTreeMap<String, Vec<u8>>, MetaError> {
        let mut out = BTreeMap::new();
        let Some(root) = self.root()? else {
            return Ok(out);
        };
        let Some(folder) = folder_at(self.objects(), &root, KEYS_FOLDER)? else {
            return Ok(out);
        };
        for entry in folder.entries() {
            if entry.mode != Mode::File {
                continue;
            }
            if let Some(member) = entry.name.strip_suffix(".key") {
                out.insert(member.to_string(), self.objects().blob(&entry.id)?.to_vec());
            }
        }
        Ok(out)
    }

    /// Commits the key wrap of `member` (`None`: removes it - the member leaves). When another
    /// device changes the same wrap at the same moment, a removal stands, whichever device made
    /// it (the member left, or the other device wrapped the key from a record that still named
    /// it), and of two new wraps this one is the effective one; no conflict copy of a wrap is
    /// left in the folder.
    pub fn set_member_wrap(
        &mut self,
        member: &str,
        wrap: Option<&[u8]>,
    ) -> Result<CommitOutcome, MetaError> {
        let path = key_path(member)?;
        let change = match wrap {
            Some(bytes) => Change::Put {
                path,
                id: self.write_blob(bytes),
            },
            None => Change::Delete { path },
        };
        let message = if wrap.is_some() {
            format!("Member {member} added\n")
        } else {
            format!("Member {member} removed\n")
        };
        let mut resolve = |conflict: &Conflict| match conflict.theirs {
            None => Resolution::TakeTheirs,
            Some(_) => Resolution::KeepMine,
        };
        self.commit(&[change], &message, &mut resolve)
    }
}

// ==== The record of the key flows ====

/// The device name the key flows' commits carry.
const RECORD_DEVICE: &str = "azul-storage keys";
/// How often a member change is made again when another device changed the members meanwhile.
const RECORD_ATTEMPTS: usize = 5;

/// A drive's members as its policy names them, and the wraps its repository holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Members {
    pub policy: Policy,
    /// The members' key wraps (the bytes of their key files), by member id.
    pub wraps: BTreeMap<String, Vec<u8>>,
}

impl Members {
    /// Whether the policy names `member`.
    #[must_use]
    pub fn contains(&self, member: &str) -> bool {
        self.policy.members.contains_key(member)
    }

    /// Every member id of the record: the policy's and the wraps', sorted.
    #[must_use]
    pub fn ids(&self) -> Vec<String> {
        let mut ids: BTreeSet<String> = self.policy.members.keys().cloned().collect();
        ids.extend(self.wraps.keys().cloned());
        ids.into_iter().collect()
    }

    /// The members of a drive whose index has no policy yet: every member key file in the
    /// bucket, each an owner named by its id.
    fn from_key_files(bucket: &dyn Drive) -> Result<Members, DriveError> {
        let wraps = load_member_wraps(bucket)?;
        let members = wraps.keys().map(|id| (id.clone(), owner(id))).collect();
        Ok(Members {
            policy: Policy {
                members,
                ..Policy::default()
            },
            wraps,
        })
    }

    fn apply(&mut self, change: &MemberChange) {
        match change {
            MemberChange::Add { member, wrap } => {
                self.policy
                    .members
                    .entry(member.clone())
                    .or_insert_with(|| owner(member));
                self.wraps.insert(member.clone(), wrap.clone());
            }
            MemberChange::Remove { member } => {
                self.policy.members.remove(member);
                self.policy.grants.retain(|grant| grant.member != *member);
                self.wraps.remove(member);
            }
        }
    }

    /// Whether `change` shows in these members.
    fn holds(&self, change: &MemberChange) -> bool {
        match change {
            MemberChange::Add { member, wrap } => {
                self.contains(member) && self.wraps.get(member) == Some(wrap)
            }
            MemberChange::Remove { member } => {
                !self.contains(member) && !self.wraps.contains_key(member)
            }
        }
    }
}

/// A member new to the policy: an owner, named by its id until it is renamed. Every device
/// that holds the drive key can do everything the key flows do, so the key flows give no
/// lesser role.
fn owner(member: &str) -> Member {
    Member {
        name: member.to_string(),
        role: Role::Owner,
    }
}

/// A change of a drive's members ([`MemberRecord::record`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberChange {
    /// `member` joins with `wrap` (the bytes of its key file), or keeps its place with a new
    /// wrap. A member new to the policy joins as an owner.
    Add { member: String, wrap: Vec<u8> },
    /// `member` leaves: its entry, its grants and its wrap.
    Remove { member: String },
}

/// What [`MemberRecord::check`] found of a drive key.
pub enum KeyCheck<'a> {
    /// The index opens with it: the drive's record.
    Opens(MemberRecord<'a>),
    /// The bucket holds no index (an empty drive), or cannot hold one: nothing in it says
    /// which key is the drive's.
    NoIndex,
    /// The index does not open with it.
    Refuses,
}

/// The record of an encrypted drive's members: the policy and the member wraps of the
/// bucket's metadata repository, opened with the drive key as a lazy copy (only `.azlin` is
/// read; a change reads the packs, as every commit does).
pub struct MemberRecord<'a> {
    bucket: &'a dyn Drive,
    repo: MetaRepo<DriveBucket<&'a dyn Drive>, DriveKey>,
}

impl<'a> MemberRecord<'a> {
    /// The record in `bucket` (the bucket itself, below the encryption), opened with
    /// `drive_key`. `None` for a bucket without a metadata repository, or one that cannot
    /// hold one (no conditional writes): its key files are its record.
    pub fn open(
        bucket: &'a dyn Drive,
        drive_key: &DriveKey,
    ) -> Result<Option<MemberRecord<'a>>, DriveError> {
        match Self::open_repo(bucket, drive_key) {
            Ok(repo) => Ok(Some(MemberRecord { bucket, repo })),
            Err(MetaError::NoRepository | MetaError::Unsupported(_)) => Ok(None),
            Err(e) => Err(to_drive(e)),
        }
    }

    /// Whether the drive's index opens with `drive_key`: a device that cannot tell otherwise
    /// which drive key is the drive's asks the index, the one thing in the bucket that only
    /// the drive key could have sealed (once it holds files).
    pub fn check(bucket: &'a dyn Drive, drive_key: &DriveKey) -> Result<KeyCheck<'a>, DriveError> {
        match Self::open_repo(bucket, drive_key) {
            Ok(repo) => Ok(KeyCheck::Opens(MemberRecord { bucket, repo })),
            Err(MetaError::NoRepository | MetaError::Unsupported(_)) => Ok(KeyCheck::NoIndex),
            Err(MetaError::Sealed { .. }) => Ok(KeyCheck::Refuses),
            Err(e) => Err(to_drive(e)),
        }
    }

    /// The bytes of the biggest pack or pack index the repository's manifest names (its word:
    /// nothing is fetched). A key rotation re-seals every pack as one pack of its own, so a pack
    /// the writer would refuse ([`super::wal::check_writable`]) is one it cannot re-seal.
    #[must_use]
    pub fn biggest_object(&self) -> u64 {
        self.repo
            .store()
            .state()
            .packs
            .iter()
            .map(|pack| pack.pack_size.max(pack.idx_size))
            .max()
            .unwrap_or(0)
    }

    fn open_repo(
        bucket: &'a dyn Drive,
        drive_key: &DriveKey,
    ) -> Result<MetaRepo<DriveBucket<&'a dyn Drive>, DriveKey>, MetaError> {
        let options = RepoOptions {
            cache_dir: None,
            lazy: true,
            ..RepoOptions::default()
        };
        let device = crate::ids::new_uuid();
        MetaRepo::open_with(
            DriveBucket::new(bucket),
            drive_key.clone(),
            &device,
            RECORD_DEVICE,
            &options,
        )
    }

    /// The policy (`None` while there is none) and the wraps of the repository.
    fn read(&mut self) -> Result<(Option<Policy>, BTreeMap<String, Vec<u8>>), MetaError> {
        self.repo.ensure_entry(POLICY_PATH)?;
        let policy = self.repo.policy()?;
        self.repo.ensure_folder(KEYS_FOLDER, true)?;
        let wraps = self.repo.member_wraps()?;
        Ok((policy, wraps))
    }

    /// The members the policy names, `None` while the drive has no policy. Until its first
    /// member change, such a drive's key files in the bucket are its record.
    pub fn members(&mut self) -> Result<Option<Members>, DriveError> {
        let (policy, wraps) = self.read().map_err(to_drive)?;
        Ok(policy.map(|policy| Members { policy, wraps }))
    }

    /// Records `changes` in one commit and returns the members after it. A drive without a
    /// policy gets one in the same commit, made from its key files in the bucket (the
    /// migration). When another device changed the members meanwhile, its policy is taken
    /// and the changes are made again on top of it.
    pub fn record(&mut self, changes: &[MemberChange]) -> Result<Members, DriveError> {
        for _ in 0..RECORD_ATTEMPTS {
            let (policy, tree_wraps) = self.read().map_err(to_drive)?;
            let mut wanted = match &policy {
                Some(policy) => Members {
                    policy: policy.clone(),
                    wraps: tree_wraps.clone(),
                },
                None => Members::from_key_files(self.bucket)?,
            };
            for change in changes {
                wanted.apply(change);
            }
            let mut tree = Vec::new();
            if let Some(old) = policy.as_ref().filter(|old| **old != wanted.policy) {
                // A rewrite of the policy is its next revision.
                wanted.policy.revision = old.revision.saturating_add(1);
            }
            if policy.as_ref() != Some(&wanted.policy) {
                let text = wanted.policy.to_toml().map_err(to_drive)?;
                tree.push(Change::Put {
                    path: POLICY_PATH.to_string(),
                    id: self.repo.write_blob(text.as_bytes()),
                });
            }
            for (member, wrap) in &wanted.wraps {
                if tree_wraps.get(member) != Some(wrap) {
                    tree.push(Change::Put {
                        path: key_path(member).map_err(to_drive)?,
                        id: self.repo.write_blob(wrap),
                    });
                }
            }
            for member in tree_wraps.keys() {
                if !wanted.wraps.contains_key(member) {
                    tree.push(Change::Delete {
                        path: key_path(member).map_err(to_drive)?,
                    });
                }
            }
            if tree.is_empty() {
                return Ok(wanted);
            }
            let message = if policy.is_none() {
                "Members: the policy, from the key files\n"
            } else {
                "Members changed\n"
            };
            // Their policy on a conflict (the changes are made again on top of it), this
            // device's wraps.
            let mut resolve = |conflict: &Conflict| {
                if conflict.path == POLICY_PATH {
                    Resolution::TakeTheirs
                } else {
                    Resolution::KeepMine
                }
            };
            self.repo
                .commit(&tree, message, &mut resolve)
                .map_err(to_drive)?;
            if let Some(after) = self.members()? {
                if changes.iter().all(|change| after.holds(change)) {
                    return Ok(after);
                }
            }
        }
        Err(DriveError::Conflict {
            key: POLICY_PATH.to_string(),
        })
    }
}
