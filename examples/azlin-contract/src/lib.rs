//! The contract between the Azlin apps (AzDrive, AzMail, the `azcloud` command line, through
//! azcloud-kit) and the Azlin services (the token server, the storage nodes).
//!
//! - [`CONTRACT_VERSION`] and [`ClientHello`]: what every request says it speaks
//!   (`x-azlin-client: azdrive/0.3.0; contract=3`); [`ClientMinimum`]: the oldest the services
//!   answer, anything older gets `426` [`ClientTooOld`].
//! - Ids and units ([`DriveId`], [`CheckoutId`], [`TierId`], [`Months`], [`StoredBytes`],
//!   [`Cents`], ...): checked once when they are read, so the rest of the code can trust them.
//! - [`IdempotencyKey`]: what makes a repeated request answer like the first one, within its
//!   [`IdempotentScope`]; a repeat whose first answer was used is refused [`ALREADY_ANSWERED`].
//! - [`Rfc3339`] and [`Deadline`]: the times the services name, and what ends at them.
//! - [`AzlinError`]: the codes a node sends in `x-azlin-error` and the token server in its
//!   `{"error": ...}` body.
//! - Buying a drive: [`TiersAnswer`] and its [`TierOffer`]s, [`CheckoutAnswer`] with its
//!   [`CheckoutStatusWire`], [`AbandonAnswer`] and [`CheckoutRefusal`].
//! - [`RefreshRefusal`]: how the token server refuses a refresh, and whether the token was spent.
//! - A paid checkout's period tokens: their [`IssueSchedule`] (one instalment a month,
//!   [`INSTALMENT_SECS`]), the [`IssueAnswer`] and the [`IssueRefusal`]s, final or not.
//! - [`ReadOnlyWhy`]: why a drive takes no writes, each reason with its node code; the cluster
//!   map's epoch in [`MAP_HEADER`].
//!
//! [`FIXTURES`] are the golden JSON answers: the crate's tests prove the types read and write
//! exactly them, and the services check [`fixtures_digest`] against the digest they were built
//! for, so both sides cannot drift apart unnoticed.

mod checkout;
mod error;
mod hello;
mod ids;
mod idempotency;
mod period;
mod read_only;
mod refresh;
mod renewal;
mod time;

pub use checkout::{
    AbandonAnswer, AbandonWire, CheckoutAnswer, CheckoutRefusal, CheckoutStatusWire, DeclineWire,
    LadderRef, Legal, MailTo, Prepay, ReceiptWire, RetentionAddon, ScheduledChange, Settles,
    TierClosed, TierOffer, TierPriceWire, TiersAnswer,
};
pub use error::AzlinError;
pub use hello::{
    client_too_old, AppName, ClientHello, ClientMinimum, ClientTooOld, SemVer, CLIENT_HEADER,
    MIN_CONTRACT, MIN_VERSIONS,
};
pub use idempotency::{
    IdempotencyKey, IdempotentScope, ALREADY_ANSWERED, IDEMPOTENCY_HEADER, IDEMPOTENCY_REQUIRED,
};
pub use period::{IssueAnswer, IssueRefusal, IssueSchedule, INSTALMENT_SECS};
pub use read_only::{map_epoch, ReadOnlyWhy, MAP_HEADER};
pub use refresh::{BanWire, RefreshRefusal};
pub use renewal::{
    expiry, GrantKind, GrantWire, RedeemAnswer, RedeemRefusal, RenewalSignup, CHANGE_AHEAD_SECS,
    READ_ONLY_SECS, REMIND_BEFORE_DAYS, USAGE_PROOF_HEADER, WRITABLE_GRACE_SECS,
};
pub use ids::{
    BucketName, Cents, CheckoutId, Currency, DriveId, FamilyId, InvalidValue, LogicalBytes,
    MemberId, Months, StoredBytes, TierId,
};
pub use time::{Deadline, DeadlineKind, Rfc3339};

/// The version of this contract. It goes up when a request or an answer changes in a way an
/// app built for the previous version cannot read.
pub const CONTRACT_VERSION: u32 = 3;

/// The golden fixtures, `(file name, JSON)`, in the order the digest reads them.
pub const FIXTURES: &[(&str, &str)] = &[
    ("abandon_answers.json", include_str!("../fixtures/abandon_answers.json")),
    ("checkout_answers.json", include_str!("../fixtures/checkout_answers.json")),
    ("checkout_refusals.json", include_str!("../fixtures/checkout_refusals.json")),
    ("tiers_answer.json", include_str!("../fixtures/tiers_answer.json")),
    ("azlin_errors.json", include_str!("../fixtures/azlin_errors.json")),
    ("client_hello.json", include_str!("../fixtures/client_hello.json")),
    ("deadlines.json", include_str!("../fixtures/deadlines.json")),
    ("idempotency_key.json", include_str!("../fixtures/idempotency_key.json")),
    ("idempotent_scopes.json", include_str!("../fixtures/idempotent_scopes.json")),
    ("ids.json", include_str!("../fixtures/ids.json")),
    ("issue_answers.json", include_str!("../fixtures/issue_answers.json")),
    ("issue_refusals.json", include_str!("../fixtures/issue_refusals.json")),
    ("refresh_refusals.json", include_str!("../fixtures/refresh_refusals.json")),
    ("read_only_why.json", include_str!("../fixtures/read_only_why.json")),
    ("redeem_answers.json", include_str!("../fixtures/redeem_answers.json")),
    ("redeem_refusals.json", include_str!("../fixtures/redeem_refusals.json")),
    ("renewal_signups.json", include_str!("../fixtures/renewal_signups.json")),
    ("client_too_old.json", include_str!("../fixtures/client_too_old.json")),
];

/// The digest of [`FIXTURES`] (`fnv1a64:<16 hex digits>`): the services keep the one they were
/// built against and compare it, so a checkout of this crate at another revision fails loudly.
/// A drift check, not a security measure.
#[must_use]
pub fn fixtures_digest() -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    let mut hash = OFFSET;
    let mut eat = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(PRIME);
        }
    };
    for (name, json) in FIXTURES {
        eat(name.as_bytes());
        eat(&[0]);
        // Line endings as git may check them out on Windows do not change the digest.
        eat(json.replace("\r\n", "\n").as_bytes());
        eat(&[0]);
    }
    format!("fnv1a64:{hash:016x}")
}
