//! The shared enums of the registry, the policy table, the team log and the space chains
//! (planning/architecture.md section 5.3 "reg_account.kind", "reg_member.kind" and
//! "reg_member.role", section 5.10 policy defaults per kind, section 5.11 `policy_epoch`;
//! COMMON-W1R R3b and conflict C-04; build unit S17, lane W1-S17a).
//!
//! They are defined ONCE, here, and imported by `azlin-regbase` (S9a), `azlin-policy` (S17b),
//! `azlin-teamlog` (S17c) and `azlin-spaces` (S16b): no crate keeps a second copy.
//!
//! - Every enum has strict words: `as_str` gives the one spelling (lower-case snake case, as the
//!   registry's columns spell them) and `FromStr` accepts exactly that spelling and nothing else
//!   (no other case, no whitespace, no other separator); an unknown word is an error, never a
//!   default. They are `Copy`, `Ord` (declaration order: for maps and sorting only, NOT a
//!   privilege order: who may do what is the authority table's) and `Hash`.
//! - `PolicyEpoch` is the `u32` counter of `reg_account.policy_epoch`: a policy change bumps it
//!   with a CHECKED `next`, and an older credential is refused (section 5.11).
//! - `RestoreEpoch` is the `u64` counter of restores (section 2.17, 5.13, 6.10 V3): a restore
//!   raises it by one, the metadata group signs the epoch into every head and every entry written
//!   after the restore, and a device refuses a head or an entry of an older epoch than the newest
//!   restore it accepted. It is a typed counter with a CHECKED `next`, never a raw `u64` in a
//!   public API (decision of the coordinator on X-DELTA-W1S17A-3, 2026-11-14).

use core::fmt;
use core::str::FromStr;

use azlin_keys::IdError;

/// Why a text is not a word of a vocabulary.
///
/// Implements planning/architecture.md section 5.3 (strict words of the registry's columns).
///
/// Caller: every `FromStr` of this module; S9a (reading `reg_account`/`reg_member` rows).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
#[error("not a word of the {vocabulary} vocabulary")]
pub struct UnknownWord {
    /// The vocabulary the text was read as: `account kind`, `account role` or `member kind`.
    pub vocabulary: &'static str,
}

/// The `ALL`, `as_str`, `Display` and strict `FromStr` of a vocabulary enum, from one table of
/// variant and word: a variant missing from (or extra in) the table does not compile.
macro_rules! words {
    ($name:ident, $what:literal, [$($variant:ident => $word:literal),+ $(,)?]) => {
        impl $name {
            /// Every value, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The word of this value: lower-case snake case, the one spelling `FromStr` accepts.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $word),+
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        /// Strict: exactly one of the words, in lower case.
        impl FromStr for $name {
            type Err = UnknownWord;

            fn from_str(text: &str) -> Result<$name, UnknownWord> {
                match text {
                    $($word => Ok($name::$variant),)+
                    _ => Err(UnknownWord { vocabulary: $what }),
                }
            }
        }
    };
}

/// The kind of an account: kinds differ only in policy defaults and entitlements
/// (planning/architecture.md section 5.3 `reg_account.kind`, section 5.10).
///
/// Implements planning/architecture.md section 5.3 (`personal`, `family`, `team`, `business`).
///
/// Caller: S9a (`reg_account.kind`), S17b (policy defaults per kind), S17c (`genesis`), S13a.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccountKind {
    /// One person's account.
    Personal,
    /// A family.
    Family,
    /// A team.
    Team,
    /// A business.
    Business,
}

words!(
    AccountKind,
    "account kind",
    [
        Personal => "personal",
        Family => "family",
        Team => "team",
        Business => "business",
    ]
);

/// The role of a member of an account (planning/architecture.md section 5.3 `reg_member.role`,
/// section 7.2). The declaration order is for sorting and map keys only; it is NOT a privilege
/// order (the authority table of S17c says who may sign what).
///
/// Implements planning/architecture.md section 5.3 (`owner`, `admin`, `group_admin`, `helpdesk`,
/// `member`, `guest`).
///
/// Caller: S9a (`reg_member.role`), S17c (`member_role`, the authority table), S16b (grants).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccountRole {
    /// An owner: the whole account.
    Owner,
    /// An admin: the whole account, with the policy's quorum where it asks.
    Admin,
    /// A group admin: the subtree of its `admin_scope` group.
    GroupAdmin,
    /// Helpdesk: opens device requests and suspends in its scope; never approves alone.
    Helpdesk,
    /// An ordinary member.
    Member,
    /// A guest.
    Guest,
}

words!(
    AccountRole,
    "account role",
    [
        Owner => "owner",
        Admin => "admin",
        GroupAdmin => "group_admin",
        Helpdesk => "helpdesk",
        Member => "member",
        Guest => "guest",
    ]
);

/// What a member is: a person or an app (planning/architecture.md section 5.3
/// `reg_member.kind`).
///
/// Implements planning/architecture.md section 5.3 (`person`, `app`).
///
/// Caller: S9a (`reg_member.kind`), S17c (`MemberKind` of `member_invite`), S16b (grantees).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MemberKind {
    /// A person.
    Person,
    /// An app: it has scopes and an expiry and is not escrow-recoverable.
    App,
}

words!(
    MemberKind,
    "member kind",
    [Person => "person", App => "app"]
);

/// The policy epoch of an account: a counter that a policy change bumps, and that invalidates
/// credentials of an older epoch (planning/architecture.md section 5.3 `policy_epoch`, section
/// 5.11 credential v2). A plain `u32` with a CHECKED successor: a counter never wraps. Its text is
/// plain decimal.
///
/// Implements planning/architecture.md section 5.11 (`policy_epoch`: a policy change invalidates
/// older credentials).
///
/// Caller: S9a (`reg_account.policy_epoch`), S17b (`next_epoch`), S17c (`policy_set`), S9g
/// (`check_connection`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PolicyEpoch(u32);

impl PolicyEpoch {
    /// The epoch 0: the number zero, which makes no claim about being the first epoch of an
    /// account (the genesis state of the team log decides where an account starts).
    pub const ZERO: PolicyEpoch = PolicyEpoch(0);

    /// The epoch with this number.
    pub const fn new(number: u32) -> PolicyEpoch {
        PolicyEpoch(number)
    }

    /// The number of the epoch.
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The epoch after this one; `None` at the largest number (a counter never wraps).
    pub fn next(self) -> Option<PolicyEpoch> {
        if mutant!("seams_policy_epoch_wraps") {
            return Some(PolicyEpoch(self.0.wrapping_add(1)));
        }
        self.0.checked_add(1).map(PolicyEpoch)
    }
}

/// Decimal.
impl fmt::Display for PolicyEpoch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for PolicyEpoch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PolicyEpoch({})", self.0)
    }
}

/// Strict decimal: digits only, no sign, no leading zero (but `0` itself), 0 to 4,294,967,295. The
/// error is `IdError::Length` for the empty text, `IdError::BadDigit` for a character that is not
/// an ASCII digit or a leading zero, and `IdError::OutOfRange` above `u32::MAX`.
impl FromStr for PolicyEpoch {
    type Err = IdError;

    fn from_str(text: &str) -> Result<PolicyEpoch, IdError> {
        let digits = text.as_bytes();
        if digits.is_empty() {
            return Err(IdError::Length);
        }
        if !digits.iter().all(u8::is_ascii_digit) {
            return Err(IdError::BadDigit);
        }
        if digits.len() > 1 && digits.first() == Some(&b'0') {
            return Err(IdError::BadDigit);
        }
        match text.parse::<u32>() {
            Ok(number) => Ok(PolicyEpoch(number)),
            Err(_) => Err(IdError::OutOfRange),
        }
    }
}

/// The restore epoch: how many restores the data has been through. A restore from the off-site copy
/// (or a point-in-time restore) moves positions back and raises the epoch by one; the metadata group
/// signs the epoch into every head and every entry it writes afterwards, so a head or an entry of the
/// lost timeline (an older epoch) is refused once a device has accepted the restore. `ZERO` is data
/// that was never restored; the epoch of a restore starts at 1. A plain `u64` with a CHECKED
/// successor: a counter never wraps. Its text is plain decimal.
///
/// Implements planning/architecture.md section 6.10 V3 (a restore epoch the metadata group signs),
/// section 5.13 (a head never goes backwards; the only way back is a restore) and section 2.17
/// (what a restore re-anchors).
///
/// Caller: azlin-chain (`SignedHead::epoch`, `ChainEntry::epoch`, `RestoreRecord::epoch`,
/// `HeadPin::restore_epoch`), S16a (`HeadRecord::restore_epoch`, `Seen`, `baseline`), S43 (the signed
/// restore record), S9 (the registry's restore epoch).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RestoreEpoch(u64);

impl RestoreEpoch {
    /// The epoch of data that was never restored.
    pub const ZERO: RestoreEpoch = RestoreEpoch(0);

    /// The epoch with this number.
    pub const fn new(number: u64) -> RestoreEpoch {
        RestoreEpoch(number)
    }

    /// The number of the epoch.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The epoch after this one (the epoch of the next restore); `None` at the largest number (a
    /// counter never wraps).
    pub fn next(self) -> Option<RestoreEpoch> {
        if mutant!("seams_restore_epoch_wraps") {
            return Some(RestoreEpoch(self.0.wrapping_add(1)));
        }
        self.0.checked_add(1).map(RestoreEpoch)
    }
}

/// Decimal.
impl fmt::Display for RestoreEpoch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for RestoreEpoch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RestoreEpoch({})", self.0)
    }
}

/// Strict decimal: digits only, no sign, no leading zero (but `0` itself), 0 to
/// 18,446,744,073,709,551,615. The error is `IdError::Length` for the empty text,
/// `IdError::BadDigit` for a character that is not an ASCII digit or a leading zero, and
/// `IdError::OutOfRange` above `u64::MAX`.
impl FromStr for RestoreEpoch {
    type Err = IdError;

    fn from_str(text: &str) -> Result<RestoreEpoch, IdError> {
        let digits = text.as_bytes();
        if digits.is_empty() {
            return Err(IdError::Length);
        }
        if !digits.iter().all(u8::is_ascii_digit) {
            return Err(IdError::BadDigit);
        }
        if digits.len() > 1 && digits.first() == Some(&b'0') {
            return Err(IdError::BadDigit);
        }
        match text.parse::<u64>() {
            Ok(number) => Ok(RestoreEpoch(number)),
            Err(_) => Err(IdError::OutOfRange),
        }
    }
}
