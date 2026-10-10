//! The drive's policy and its members' key wraps in the drive index (feature `encryption`).
//! Not written yet.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{
    bucket::Bucket,
    repo::{CommitOutcome, MetaRepo},
    seal::Sealer,
    MetaError,
};

/// Where the policy lives in the repository.
pub const POLICY_PATH: &str = ".azlin/policy.toml";
/// Where the members' key wraps live in the repository.
pub const KEYS_FOLDER: &str = ".azlin/keys";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Owner,
    Editor,
    Viewer,
    Guest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub name: String,
    pub role: Role,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    pub member: String,
    pub folder: String,
    pub access: Access,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    #[serde(default)]
    pub members: BTreeMap<String, Member>,
    #[serde(default)]
    pub grants: Vec<Grant>,
}

impl Policy {
    pub fn to_toml(&self) -> Result<String, MetaError> {
        Ok(String::new())
    }

    pub fn from_toml(text: &str) -> Result<Policy, MetaError> {
        let _ = text;
        Ok(Policy::default())
    }

    #[must_use]
    pub fn allows(&self, member: &str, path: &str, write: bool) -> bool {
        let _ = (member, path, write);
        false
    }

    #[must_use]
    pub fn administers(&self, member: &str) -> bool {
        let _ = member;
        false
    }
}

impl<B: Bucket, S: Sealer> MetaRepo<B, S> {
    pub fn policy(&self) -> Result<Option<Policy>, MetaError> {
        Ok(None)
    }

    pub fn set_policy(&mut self, policy: &Policy) -> Result<CommitOutcome, MetaError> {
        let _ = policy;
        Err(MetaError::Unsupported("the policy".to_string()))
    }

    pub fn member_wraps(&self) -> Result<BTreeMap<String, Vec<u8>>, MetaError> {
        Ok(BTreeMap::new())
    }

    pub fn set_member_wrap(
        &mut self,
        member: &str,
        wrap: Option<&[u8]>,
    ) -> Result<CommitOutcome, MetaError> {
        let _ = (member, wrap);
        Err(MetaError::Unsupported("the member wraps".to_string()))
    }
}
