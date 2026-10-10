//! Trusted contacts (D51): a drive's recovery code split 2-of-3 to three people (a stub).

use std::fmt;

use super::{
    keys::{MemberPublic, MemberSecret, RecoveryCode, RECOVERY_CODE_LEN},
    CryptoError,
};

pub const THRESHOLD: u8 = 2;
pub const SHARES: u8 = 3;
pub const SET_LEN: usize = 4;
pub const CONTACT_PREFIX: &str = "azlin-contact:";
pub const SHARE_PREFIX: &str = "azlin-share:";
pub const REQUEST_PREFIX: &str = "azlin-recover:";
pub const REPLY_PREFIX: &str = "azlin-share-reply:";

fn not_yet() -> CryptoError {
    CryptoError::Unsupported(String::from("trusted contacts"))
}

#[must_use]
pub fn set_of(_code: &RecoveryCode) -> [u8; SET_LEN] {
    [0; SET_LEN]
}

#[derive(Clone, PartialEq, Eq)]
pub struct CodeShare {
    index: u8,
    set: [u8; SET_LEN],
    y: [u8; RECOVERY_CODE_LEN],
}

impl fmt::Debug for CodeShare {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CodeShare({}, {:?}, {:?})", self.index, self.set, self.y)
    }
}

impl CodeShare {
    pub fn split(_code: &RecoveryCode) -> Result<Vec<CodeShare>, CryptoError> {
        Err(not_yet())
    }

    pub fn combine(_shares: &[CodeShare]) -> Result<RecoveryCode, CryptoError> {
        Err(not_yet())
    }

    #[must_use]
    pub fn index(&self) -> u8 {
        self.index
    }

    #[must_use]
    pub fn set_hex(&self) -> String {
        String::new()
    }

    #[must_use]
    pub fn to_text(&self) -> zeroize::Zeroizing<String> {
        zeroize::Zeroizing::new(String::new())
    }

    #[must_use]
    pub fn parse(_text: &str) -> Option<CodeShare> {
        None
    }
}

#[must_use]
pub fn contact_text(_key: &MemberPublic) -> String {
    String::new()
}

#[must_use]
pub fn contact_from_text(_text: &str) -> Option<MemberPublic> {
    None
}

pub fn seal_share(
    _share: &CodeShare,
    _label: &str,
    _to: &MemberPublic,
) -> Result<String, CryptoError> {
    Err(not_yet())
}

#[must_use]
pub fn share_recipient(_text: &str) -> Option<MemberPublic> {
    None
}

pub fn open_share(_text: &str, _secret: &MemberSecret) -> Result<(CodeShare, String), CryptoError> {
    Err(not_yet())
}

#[must_use]
pub fn request_text(_key: &MemberPublic) -> String {
    String::new()
}

#[must_use]
pub fn request_from_text(_text: &str) -> Option<MemberPublic> {
    None
}

#[must_use]
pub fn safety_number(_request: &MemberPublic) -> String {
    String::new()
}

pub fn seal_reply(_share: &CodeShare, _to: &MemberPublic) -> Result<String, CryptoError> {
    Err(not_yet())
}

pub fn open_reply(_text: &str, _request: &MemberSecret) -> Result<CodeShare, CryptoError> {
    Err(not_yet())
}

pub fn read_share(_text: &str, _request: &MemberSecret) -> Result<CodeShare, CryptoError> {
    Err(not_yet())
}
