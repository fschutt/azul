//! The shared vocabulary of the registry's rows and answers (planning/architecture.md section 5.3
//! `kind`, `role`, `state`; section 5.11 entitlement `state`; section 2.5 status codes; build unit
//! S9, lane W1-S9a).
//!
//! `AccountKind`, `AccountRole`, `MemberKind` and `PolicyEpoch` are defined ONCE, in
//! `azlin_seams::vocab` (COMMON-W1R R3b, conflict C-04), and re-exported here: S9a's old `Role` is
//! [`AccountRole`]. This module adds the two sets only the registry and its clients know:
//!
//! - [`EntitlementState`]: `Active` or `Lapsed` (COMMON-W1R R10; the words of S13a's entitlement,
//!   strict: an unknown word is refused, never read as a default);
//! - [`RefusalWord`]: the closed set of machine words a REFRESH refusal carries beside its
//!   status, so a client tells `token_reuse` from `credentials_revoked` and `device_pending` from
//!   `account_locked` (DRIFT X-D-07/D-10 of S9b: the reason is a header key of the answer). The
//!   client crate azlin-session reads the same type.
//!
//! Every enum has `ALL`, one spelling (`as_str`, lower-case snake case) and a strict `FromStr`.

use core::fmt;
use core::str::FromStr;

pub use azlin_seams::vocab::{AccountKind, AccountRole, MemberKind, PolicyEpoch, UnknownWord};

/// The `ALL`, `as_str`, `Display` and strict `FromStr` of a closed set of words, from one table of
/// variant and word: a variant missing from the table does not compile.
macro_rules! words {
    (
        $(#[$meta:meta])*
        $name:ident, $what:literal, {
            $($(#[$vmeta:meta])* $variant:ident => $word:literal),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $name {
            $($(#[$vmeta])* $variant),+
        }

        impl $name {
            /// Every value, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The word of this value: lower-case snake case, the one spelling `FromStr`
            /// accepts.
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

words! {
    /// The state of an account's entitlement (planning/architecture.md section 5.11 `state`):
    /// paid up, or lapsed. Only these two words exist (COMMON-W1R R10: one set for S9, S13 and the
    /// payments service); an unknown word is refused at decode, never read as `Active`.
    ///
    /// Caller: azlin-creds' entitlement claims (S9e), S9d (the lifecycle reads it), S13a.
    EntitlementState, "entitlement state", {
        /// The period is paid.
        Active => "active",
        /// The period was not renewed.
        Lapsed => "lapsed",
    }
}

words! {
    /// The machine word of a refused refresh, beside the status (planning/architecture.md section
    /// 2.5 status codes: `revoked`, `locked`, `retry_after`, `forbidden`, `client_too_old`). The
    /// status gives the class (retry, elsewhere, after, fatal); the word tells the client WHICH
    /// refusal it was, so it can sign in again (`token_reuse`, `credentials_revoked`), wait for a
    /// hold (`device_pending`), show a lock or a ban, or ask for an update.
    ///
    /// These are exactly the words of S9b's `RefreshRefusal` (its brief, section 4, DRIFT D-10);
    /// the refusals of the other registry units (devices, account, keys, checks) carry their own
    /// words through `Refusal::reason`, which are not on a refresh answer.
    ///
    /// Caller: S9b (`RefreshRefusal::word`), S9e (`RefreshAnswerWire::reason`), S9h (the client's
    /// decision table).
    RefusalWord, "refusal word", {
        /// The token or proof is not one this device holds.
        Unauthorized => "unauthorized",
        /// An old token was presented again: the family is revoked.
        TokenReuse => "token_reuse",
        /// The family, the device or the member is revoked or removed.
        CredentialsRevoked => "credentials_revoked",
        /// The device is still in its enrollment hold.
        DevicePending => "device_pending",
        /// The account is locked or archived.
        AccountLocked => "account_locked",
        /// The account is banned and its grace is over.
        AccountBanned => "account_banned",
        /// The app is older than the account's minimum.
        ClientTooOld => "client_too_old",
        /// The account requires a single sign-on attestation and none came.
        SsoRequired => "sso_required",
        /// The single sign-on attestation is older than the policy allows.
        SsoStale => "sso_stale",
        /// The app principal is past its expiry.
        AppExpired => "app_expired",
        /// The app principal is suspended or removed.
        AppNotActive => "app_not_active",
    }
}
