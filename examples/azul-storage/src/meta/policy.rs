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

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{
    bucket::Bucket,
    merge::keep_both,
    objects::{is_valid_name, Mode},
    repo::{CommitOutcome, MetaRepo},
    seal::Sealer,
    tree::{entry_at, folder_at, Change},
    MetaError,
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

/// What a grant allows in its folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    Read,
    Write,
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
    /// By member id.
    #[serde(default)]
    pub members: BTreeMap<String, Member>,
    #[serde(default)]
    pub grants: Vec<Grant>,
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
    /// member does nothing.
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

    /// Whether `member` may change the members and the policy.
    #[must_use]
    pub fn administers(&self, member: &str) -> bool {
        self.members.get(member).is_some_and(|m| m.role == Role::Owner)
    }
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
            Some(entry) if entry.mode == Mode::File => {
                let text = String::from_utf8(self.objects().blob(&entry.id)?.to_vec())
                    .map_err(|_| MetaError::Corrupt {
                        key: POLICY_PATH.to_string(),
                        reason: "not UTF-8".to_string(),
                    })?;
                Policy::from_toml(&text).map(Some)
            }
            _ => Ok(None),
        }
    }

    /// Commits `policy` as the drive's policy (both versions are kept when another device
    /// changed it at the same time).
    pub fn set_policy(&mut self, policy: &Policy) -> Result<CommitOutcome, MetaError> {
        let id = self.write_blob(policy.to_toml()?.as_bytes());
        self.commit(
            &[Change::Put {
                path: POLICY_PATH.to_string(),
                id,
            }],
            "Policy\n",
            &mut keep_both,
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

    /// Commits the key wrap of `member` (`None`: removes it - the member leaves).
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
        self.commit(&[change], &message, &mut keep_both)
    }
}
