//! The checkouts this device started and has no drive of yet: ONE keyring entry
//! ([`PENDING_KEY`]) holding them all - each one's id, the secret of its claim key, its tier,
//! when it started, the token server it was made at and the name typed for the drive - changed
//! only under the lock of that entry, so two windows (two processes) never lose each other's.
//! The entry outlives the app: a drive paid after "Stop waiting", or while the app was closed,
//! still reaches it. It stays within [`MAX_PENDING_BYTES`] (Windows' Credential Manager keeps no
//! more in one entry): a checkout that would not fit is refused before its payment page opens.
//!
//! A checkout's claim secret is the only key to the drive it pays for, so a checkout leaves the
//! list only through [`settle`], with a [`Settled`] made from what the token server answered:
//! the drive claimed and nothing more owed, the payment declined, the checkout gone ("no such
//! checkout" in the contract's word, not any 404) or ended by its abandon ([`abandon`]), a sealed
//! sign-up that did not OPEN twice, a period grant kept in two places. Nothing the app decides on
//! its own (a closed popover, a timeout) takes one off. The proofs and their evidence are in
//! `proof.rs`, the only file that makes a [`Settled`].
//!
//! A new order goes on the list BEFORE its `POST /v1/checkout` is sent ([`begin`], [`place_order`]:
//! the entry is a "posting", [`PendingCheckout::is_posting`]), under the list's lock: an answer
//! that is lost leaves a record, the same order asked again goes out under the same claim key
//! and gets the first answer, and no second order of the same [`Intent`] is open at the same
//! time - from this window, a second window or the run after a restart - so the buyer never pays
//! twice for one thing. [`posted`] puts the real checkout in the posting's place.
//!
//! An app polls them while its dialog waits, at every start and in the background after "Stop
//! waiting" ([`poll`]): an approved checkout becomes the drive - its session goes into the
//! keyring under the drive's lock unless the keyring has it already (idempotent by drive id: a
//! session that rotated since is never replaced by the sign-up's spent token) - and the app
//! tells the list once the drive is in its drives file ([`claimed`]): a checkout without period
//! tokens is done then; a paid one stays, its claim secret and now its issue key with it, until
//! [`finish`] has kept its period grant with the drive, where every device of it issues the
//! months (AZDRIVE-INTEGRATION §4). What a poll
//! found is [`Polled`]: an answer that ends the checkout ([`Final`], said by the one poll that
//! took it off) or one that keeps it for the next poll ([`Transient`]).
//!
//! Blocking (the keyring, the token server): call it from an azul `Thread`.

use std::fmt;

use serde::{Deserialize, Serialize};

use azlin_contract::IdempotentScope;
#[cfg(feature = "encryption")]
use azul_storage::crypto::{device, DriveKey, KeyId, KEY_LEN};
#[cfg(feature = "encryption")]
use zeroize::Zeroizing;

use crate::{
    bundle::{DriveBundle, PeriodTokens},
    claim::{ClaimError, ClaimKey},
    error::{fail, CloudError, CloudResult},
    period::{issue_next, Issued, PeriodTokenStore},
    shared::SharedKeyring,
    token::{CheckoutStatus, Paid, TokenError, TokenServer, MAX_BLINDED},
    vault::{DriveTokens, RenewalLock, VaultGrant, VaultState},
};

mod paid;
mod proof;

pub use paid::{PaidList, PaidRecord};
pub use proof::Settled;
use proof::{ClaimsEnded, GrantKept, NeverIssued, NoSuchCheckout, NotMade, Strikes};

/// The keyring entry of the unfinished checkouts.
pub const PENDING_KEY: &str = "azcloud/checkouts";
/// The `format` of its text.
pub const PENDING_FORMAT: &str = "azcloud.checkouts";
/// The most bytes the list's entry takes: Windows' Credential Manager keeps at most 2560 per
/// credential (`CRED_MAX_CREDENTIAL_BLOB_SIZE`), the strictest of the keyrings - about ten
/// checkouts.
pub const MAX_PENDING_BYTES: usize = 2560;
/// What an entry's id starts with while its order is on its way (`posting-<claim key's digest>`):
/// a word no checkout id of the token server (`ck_...`) starts with.
pub const POSTING_PREFIX: &str = "posting-";
/// How long the entry of an order that got no answer is kept as the record of it: a day. Its
/// payment page was never shown, so nobody can pay what it may have made; the entry is there for
/// the retry (the same claim key, so the same idempotency key, so the first answer) and for
/// support.
pub const POSTING_LIFE_SECS: i64 = 86_400;
/// The longest drive name a checkout keeps, in characters.
pub const MAX_NAME_CHARS: usize = 64;
/// The room [`add`] leaves for each unclaimed checkout's [`PendingTokens`] (a drive id of up to
/// 33 characters, the months, the issue key): its claim never overflows the entry.
const PERIOD_ROOM: usize = 128;

/// One unfinished checkout. `Debug` shows no claim secret.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingCheckout {
    pub checkout_id: String,
    /// The claim key's secret ([`ClaimKey::to_base64`]): what opens the sealed sign-up.
    pub claim_secret: String,
    /// The tier bought (`100GB`).
    pub tier: String,
    /// When the checkout was made, in seconds since 1970.
    pub started_at: i64,
    /// The token server it was made at: it is asked there, whatever this run's is.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token_url: String,
    /// The name typed for the drive (empty: the token server's).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Claimed - its drive is saved - with period tokens still to issue ([`claimed`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period: Option<PendingTokens>,
    /// How it is paid when that changes how it is waited for: `cash` (cash by post: asked once
    /// a day, [`PendingCheckout::is_cash`]); empty for every other method.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub method: String,
    /// A cash checkout's slip, to print again (no secret of the drive: what the posted page
    /// shows); `None` for one picked up by its claim code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cash: Option<CashKept>,
    /// Picked up by its claim code on this computer (not bought here): its drive is claimed as
    /// a token family of this computer's own ([`poll`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub picked_up: bool,
    /// When its abandon did not end it (it was paid meanwhile, or the token server could not
    /// expire its payment session), in seconds since 1970: its payment may be under way, so
    /// the app asks about it before it starts another order ([`abandon`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_flight_since: Option<i64>,
    /// How often its sealed sign-up did not open with its claim key: the second time it is
    /// given up ([`Final::Unopenable`]), the first may have been a damaged answer.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub open_failures: u32,
    /// The drive whose next months (or new tier) it buys; empty for a new drive. Its sealed
    /// sign-up holds no drive: its months are claimed for this one ([`poll`]).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub for_drive: String,
    /// The earlier checkout whose price lock it renews under; empty for none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub renews: String,
    /// Bought to move `for_drive` to its tier.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub change_tier: bool,
    /// The signed price ladder version the checkout's answer named: its drive's lock keeps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ladder_version: Option<u64>,
    /// The key a cash checkout of a new drive made for the drive with its order (64 hex digits,
    /// a secret): the drive is set up with it, so the claim code printed before the drive
    /// exists names its id. Only on the buyer's computer, and only until the drive is claimed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drive_key: Option<String>,
    /// The id of the drive key its claim code names (32 hex digits): the buyer's of its
    /// `drive_key`, a picked-up one's from its code. This computer takes no other key for the
    /// drive. `None` for every other checkout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// How often a sealed sign-up must fail to open before its checkout is given up.
pub const UNOPENABLE_AFTER: u32 = 2;

/// What a cash checkout's slip prints (cash contract v1): the amount, the activation code, the
/// operator's address, the checkout's end.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CashKept {
    /// The months paid at once (0: not said).
    #[serde(default)]
    pub months: u32,
    pub amount_cents: u64,
    pub currency: String,
    pub activation_code: String,
    pub mail_to_name: String,
    #[serde(default)]
    pub mail_to_lines: Vec<String>,
    /// RFC 3339, as the token server wrote it (empty: none said).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub expires_at: String,
}

impl CashKept {
    /// What the order posts: the amount and its currency as one value, or `None` for a damaged
    /// currency (the entry keeps the pair as the wire wrote it, so its JSON keeps its shape).
    #[must_use]
    pub fn charge(&self) -> Option<azlin_contract::Cents> {
        Some(azlin_contract::Cents {
            amount: self.amount_cents,
            currency: azlin_contract::Currency::parse(&self.currency).ok()?,
        })
    }
}

impl fmt::Debug for PendingCheckout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingCheckout")
            .field("checkout_id", &self.checkout_id)
            .field("claim_secret", &"<hidden>")
            .field("tier", &self.tier)
            .field("started_at", &self.started_at)
            .field("token_url", &self.token_url)
            .field("name", &self.name)
            .field("period", &self.period)
            .field("method", &self.method)
            .field("cash", &self.cash)
            .field("picked_up", &self.picked_up)
            .field("in_flight_since", &self.in_flight_since)
            .field("open_failures", &self.open_failures)
            .field("for_drive", &self.for_drive)
            .field("renews", &self.renews)
            .field("change_tier", &self.change_tier)
            .field("ladder_version", &self.ladder_version)
            .field("drive_key", &self.drive_key.as_ref().map(|_| "<hidden>"))
            .field("key_id", &self.key_id)
            .finish()
    }
}

/// A claimed checkout's period tokens not issued yet: which drive they are for, how many, and
/// the issue key of its sealed sign-up (kept here: the sign-up is purged 30 days after the
/// payment). `Debug` shows no key.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingTokens {
    pub drive_id: String,
    pub months: u32,
    pub issue_key: String,
    /// A monthly plan's: each payment adds a month.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub recurring: bool,
}

impl fmt::Debug for PendingTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingTokens")
            .field("drive_id", &self.drive_id)
            .field("months", &self.months)
            .field("issue_key", &"<hidden>")
            .field("recurring", &self.recurring)
            .finish()
    }
}

impl PendingTokens {
    /// The grant of the checkout `checkout_id` these are the tokens of.
    #[must_use]
    pub fn grant(&self, checkout_id: &str) -> PeriodTokens {
        PeriodTokens {
            checkout_id: checkout_id.to_string(),
            months: self.months,
            issue_key: self.issue_key.clone(),
            recurring: self.recurring,
        }
    }
}

impl PendingCheckout {
    /// The checkout `checkout_id` just made at `token_url` for `tier` with `claim`, the drive to
    /// be called `name`.
    #[must_use]
    pub fn new(
        checkout_id: &str,
        claim: &ClaimKey,
        tier: &str,
        token_url: &str,
        name: &str,
    ) -> PendingCheckout {
        PendingCheckout {
            checkout_id: checkout_id.trim().to_string(),
            claim_secret: claim.to_base64(),
            tier: tier.trim().to_string(),
            started_at: crate::now(),
            token_url: token_url.trim().trim_end_matches('/').to_string(),
            name: short_name(name),
            period: None,
            method: String::new(),
            cash: None,
            picked_up: false,
            in_flight_since: None,
            open_failures: 0,
            for_drive: String::new(),
            renews: String::new(),
            change_tier: false,
            ladder_version: None,
            drive_key: None,
            key_id: None,
        }
    }

    /// A cash checkout (cash by post): its letter takes days or weeks, so it is asked about
    /// once a day, never in a loop.
    #[must_use]
    pub fn is_cash(&self) -> bool {
        self.method == crate::cash::CASH_METHOD
    }

    /// An order whose answer has not come yet: its entry was made before `POST /v1/checkout` was
    /// sent and has no checkout id of the token server's ([`POSTING_PREFIX`]).
    #[must_use]
    pub fn is_posting(&self) -> bool {
        self.checkout_id.starts_with(POSTING_PREFIX)
    }

    /// The entry of the order about to be sent with `claim` (state `Posting`): it holds the claim
    /// secret and what the order is for - set `for_drive` and `change_tier` after this, for a
    /// renewal or a change of tier - so an answer that is lost leaves a record, and the same
    /// order asked again finds it ([`begin`]).
    #[must_use]
    pub fn posting(claim: &ClaimKey, tier: &str, token_url: &str, name: &str) -> PendingCheckout {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(claim.public_base64().as_bytes());
        let id = format!("{POSTING_PREFIX}{}", crate::period::hex(&digest[..8]));
        PendingCheckout::new(&id, claim, tier, token_url, name)
    }

    /// What the checkout is for. Derived from the entry, never stored: its JSON keeps its shape.
    #[must_use]
    pub fn intent(&self) -> Intent {
        let drive = self.for_drive.trim();
        if drive.is_empty() {
            Intent::NewDrive {
                tier: self.tier.trim().to_string(),
            }
        } else if self.change_tier {
            Intent::Change {
                drive: drive.to_string(),
            }
        } else {
            Intent::Renew {
                drive: drive.to_string(),
            }
        }
    }

    /// An order of this computer that may still be paid and has not brought its drive (or
    /// months) yet - the kind that a second order of the same intent would pay twice. Not a
    /// claimed one (its months wait for the vault), not one picked up with a claim code (bought
    /// elsewhere), not a cash order (posted money is the buyer's own act).
    #[must_use]
    pub fn is_open_order(&self) -> bool {
        self.period.is_none() && !self.picked_up && !self.is_cash()
    }

    /// The claim key the checkout named.
    ///
    /// # Errors
    ///
    /// When the kept secret is no claim key.
    pub fn claim_key(&self) -> Result<ClaimKey, ClaimError> {
        ClaimKey::from_base64(&self.claim_secret)
    }
}

#[cfg(feature = "encryption")]
impl PendingCheckout {
    /// A cash checkout of a new drive makes the drive's key with its order: the claim code on
    /// the buyer's copy names its id, so the computer that picks the drive up takes no other
    /// key for it, and the claim of the drive keeps it as the key the drive is set up with
    /// ([`poll`]).
    ///
    /// # Errors
    ///
    /// When the OS random source gives no key.
    pub fn mint_drive_key(&mut self) -> CloudResult<()> {
        let key = DriveKey::generate().map_err(|e| {
            CloudError::failed(format!("the new drive's key could not be made: {e}"))
        })?;
        self.drive_key = Some(crate::period::hex(key.as_bytes()));
        self.key_id = Some(key.id().to_hex());
        Ok(())
    }

    /// The drive key this checkout made for its new drive; `None` for every other checkout.
    ///
    /// # Errors
    ///
    /// A kept key that is no key, or not the one its claim code names.
    pub fn planned_drive_key(&self) -> CloudResult<Option<DriveKey>> {
        let Some(text) = self.drive_key.as_deref() else {
            return Ok(None);
        };
        let Some(bytes) = crate::period::unhex::<KEY_LEN>(text.trim()) else {
            fail!("the checkout's kept drive key is damaged");
        };
        let bytes = Zeroizing::new(bytes);
        let key = DriveKey::from_bytes(*bytes);
        if self.named_key_id()? != Some(key.id()) {
            fail!("the checkout's kept drive key is not the one its claim code names");
        }
        Ok(Some(key))
    }

    /// The id of the drive key this checkout's claim code names; `None` when it names none.
    ///
    /// # Errors
    ///
    /// A kept id that is no key id (never read as none: that would take any key).
    pub fn named_key_id(&self) -> CloudResult<Option<KeyId>> {
        match self.key_id.as_deref() {
            Some(text) => match KeyId::from_hex(text.trim()) {
                Some(id) => Ok(Some(id)),
                None => fail!("the checkout's kept drive key id is damaged"),
            },
            None => Ok(None),
        }
    }
}

/// The entry's text.
#[derive(Serialize, Deserialize)]
struct PendingFile {
    format: String,
    version: u32,
    #[serde(default)]
    checkouts: Vec<PendingCheckout>,
}

fn read(shared: &SharedKeyring) -> CloudResult<Vec<PendingCheckout>> {
    let Some(text) = shared.get(PENDING_KEY)? else {
        return Ok(Vec::new());
    };
    // The parser's message is not passed on: it could quote a claim secret.
    let file: PendingFile = serde_json::from_str(&text).map_err(|_| {
        CloudError::failed("the keyring's list of unfinished checkouts does not parse")
    })?;
    if file.format != PENDING_FORMAT {
        fail!(
            "the keyring entry {PENDING_KEY} is no list of checkouts (format {:?})",
            file.format
        );
    }
    Ok(file.checkouts)
}

/// The entry's text of `checkouts`.
fn text_of(checkouts: Vec<PendingCheckout>) -> CloudResult<String> {
    let file = PendingFile {
        format: PENDING_FORMAT.to_string(),
        version: 1,
        checkouts,
    };
    serde_json::to_string(&file)
        .map_err(|_| CloudError::failed("the list of unfinished checkouts cannot be written"))
}

fn write(shared: &SharedKeyring, checkouts: Vec<PendingCheckout>) -> CloudResult<()> {
    if checkouts.is_empty() {
        return shared.delete(PENDING_KEY);
    }
    let text = text_of(checkouts)?;
    shared.set(PENDING_KEY, &text)
}

/// The unfinished checkouts, as the keyring has them now.
///
/// # Errors
///
/// When the keyring cannot be read, or its entry is no list of checkouts.
pub fn list(shared: &SharedKeyring) -> CloudResult<Vec<PendingCheckout>> {
    read(shared)
}

/// What a checkout is for. Two orders of one intent are never open at once - the buyer would pay
/// twice for one thing ([`begin`]) -; two of different intents (a new drive and a renewal, two
/// drives' renewals) are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// A new drive of this tier.
    NewDrive { tier: String },
    /// The next months of this drive.
    Renew { drive: String },
    /// This drive moved to another tier.
    Change { drive: String },
}

/// Why the list did not take an entry.
#[derive(Debug)]
pub enum AddError {
    /// An order of the same intent is open: it may be paid, or paid already. The buyer never
    /// pays twice for one thing, so no second one is started before the first one is ended
    /// ([`abandon`]) or brought its drive.
    SameIntentOpen {
        /// The open order's checkout.
        checkout_id: String,
        /// Its abandon found it paid, or could not end it ([`PendingCheckout::in_flight_since`]).
        may_be_paid: bool,
    },
    /// The entry would not fit in the keyring's one entry of the list ([`MAX_PENDING_BYTES`]).
    NoRoom { waiting: usize },
    /// The keyring could not be read or written.
    Keyring(CloudError),
}

impl fmt::Display for AddError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AddError::SameIntentOpen {
                checkout_id,
                may_be_paid,
            } => write!(
                f,
                "an earlier order for the same ({checkout_id}) is still open{}",
                if *may_be_paid { " and may be paid" } else { "" }
            ),
            AddError::NoRoom { waiting } => write!(
                f,
                "{waiting} unfinished checkouts wait already, and the keyring keeps no more of \
                 them in one entry (Windows keeps {MAX_PENDING_BYTES} bytes): let them be paid or \
                 expire first"
            ),
            AddError::Keyring(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AddError {}

impl AddError {
    /// The error as the kit's other calls say it.
    fn into_cloud(self) -> CloudError {
        match self {
            AddError::Keyring(e) => e,
            other => CloudError::failed(other.to_string()),
        }
    }
}

/// What [`begin`] found.
#[derive(Debug)]
pub enum Begun {
    /// The order's entry is on the list, under the claim key the order brought.
    Fresh,
    /// The same order was begun before and got no answer: its entry - claim secret included -
    /// stays, and the order goes on under THAT claim key, so the token server, which knows the
    /// first attempt by it, answers with the first attempt's checkout instead of a second one
    /// ([`IdempotentScope::Checkout`]).
    Resumed(Box<PendingCheckout>),
}

/// Adds `checkout` to the list (replacing one with its id; its name cut to
/// [`MAX_NAME_CHARS`]) - unless the list's entry would grow past [`MAX_PENDING_BYTES`], with
/// room for every unclaimed checkout's issue key ([`claimed`]): then the checkout is refused,
/// before its payment page opens (the ones waiting finish or expire first; a paid one is
/// claimed at the next poll).
///
/// This is the list's primitive. A new ORDER goes in through [`begin`], which also keeps the
/// buyer from paying twice for one thing.
///
/// # Errors
///
/// When the list cannot be read or written, or has no room left.
pub fn add(shared: &SharedKeyring, checkout: &PendingCheckout) -> CloudResult<()> {
    insert(shared, checkout, false)
        .map(|_| ())
        .map_err(AddError::into_cloud)
}

/// The entry of a new order, made BEFORE its `POST /v1/checkout` is sent (state `Posting`, see
/// [`PendingCheckout::posting`]), under the list's lock - so a second window, and the run after a
/// restart, see it: an answer that is lost leaves a record of the order, and no second order of
/// the same [`Intent`] is open at the same time as the first.
///
/// - an open order of the same intent that is a real checkout: [`AddError::SameIntentOpen`] (it
///   is ended with [`abandon`] first, or it brings its drive);
/// - an entry of the same intent that is a posting (an earlier attempt whose answer was lost):
///   [`Begun::Resumed`];
/// - else the entry goes on the list: [`Begun::Fresh`].
///
/// A claimed checkout, a cash order and a picked-up one do not count as open
/// ([`PendingCheckout::is_open_order`]).
///
/// # Errors
///
/// [`AddError`].
pub fn begin(shared: &SharedKeyring, order: &PendingCheckout) -> Result<Begun, AddError> {
    insert(shared, order, true)
}

/// Puts `checkout` on the list, `guarded` by the intent check of [`begin`].
fn insert(
    shared: &SharedKeyring,
    checkout: &PendingCheckout,
    guarded: bool,
) -> Result<Begun, AddError> {
    let _lock = shared.lock(PENDING_KEY).map_err(AddError::Keyring)?;
    let mut checkouts = read(shared).map_err(AddError::Keyring)?;
    if guarded {
        let intent = checkout.intent();
        // A posting whose claim key does not read cannot be resumed: it is left to age out
        // ([`forget_stale_postings`]) and a new one is made beside it.
        let open = checkouts.iter().find(|c| {
            c.is_open_order()
                && c.intent() == intent
                && (c.checkout_id == checkout.checkout_id
                    || !c.is_posting()
                    || c.claim_key().is_ok())
        });
        if let Some(open) = open {
            if open.is_posting() {
                return Ok(Begun::Resumed(Box::new(open.clone())));
            }
            return Err(AddError::SameIntentOpen {
                checkout_id: open.checkout_id.clone(),
                may_be_paid: open.in_flight_since.is_some(),
            });
        }
    }
    checkouts.retain(|c| c.checkout_id != checkout.checkout_id);
    place(shared, checkouts, checkout).map(|()| Begun::Fresh)
}

/// `checkouts` and `checkout` as the list's one entry, if they fit.
fn place(
    shared: &SharedKeyring,
    mut checkouts: Vec<PendingCheckout>,
    checkout: &PendingCheckout,
) -> Result<(), AddError> {
    let waiting = checkouts.len();
    let mut kept = checkout.clone();
    kept.name = short_name(&kept.name);
    checkouts.push(kept);
    let unclaimed = checkouts.iter().filter(|c| c.period.is_none()).count();
    let text = text_of(checkouts).map_err(AddError::Keyring)?;
    if text.len() + unclaimed * PERIOD_ROOM > MAX_PENDING_BYTES {
        return Err(AddError::NoRoom { waiting });
    }
    shared.set(PENDING_KEY, &text).map_err(AddError::Keyring)
}

/// The order whose entry is `posting_id` (made by [`begin`]) got its answer: `checkout`, the real
/// entry (the token server's checkout id), takes its place on the list. Whoever made the entry
/// shows nothing of the checkout before this has returned.
///
/// # Errors
///
/// [`AddError`]: no room for the real entry (it holds more than the posting), or the keyring. The
/// posting stays then, and the order asked again gets the same checkout.
pub fn posted(
    shared: &SharedKeyring,
    posting_id: &str,
    checkout: &PendingCheckout,
) -> Result<(), AddError> {
    let _lock = shared.lock(PENDING_KEY).map_err(AddError::Keyring)?;
    let mut checkouts = read(shared).map_err(AddError::Keyring)?;
    checkouts.retain(|c| c.checkout_id != posting_id && c.checkout_id != checkout.checkout_id);
    place(shared, checkouts, checkout)
}

/// The token server REFUSED the order whose entry is `posting_id` with `refusal`: it made no
/// checkout, so the entry goes. An error that is not a refusal (no answer, a busy server) leaves
/// the entry where it is - the checkout may exist - and answers `false`; so does an entry that is
/// not on the list any more.
///
/// # Errors
///
/// When the list cannot be read or written.
pub fn order_refused(
    shared: &SharedKeyring,
    posting_id: &str,
    refusal: &TokenError,
) -> CloudResult<bool> {
    match NotMade::refused(refusal) {
        Some(none) => settle(shared, posting_id, Settled::not_made(&none)),
        None => Ok(false),
    }
}

/// The checkout `checkout_id`, which the order whose entry is `posting_id` made, is not shown (a
/// price that is not the one the buyer was shown, a page that is not the token server's): it is
/// abandoned at `server` ([`abandon`]), and when that ended it the order's record goes too - the
/// same order asked again must not get a checkout that is over. One that could not be ended stays
/// on the list in flight, as [`abandon`] leaves it.
#[must_use]
pub fn abandon_unshown(
    server: &TokenServer<'_>,
    shared: &SharedKeyring,
    posting_id: &str,
    checkout_id: &str,
) -> Abandoned {
    let result = abandon(server, shared, checkout_id);
    if let Some(none) = NotMade::unshown(&result) {
        // collapse-ok: a record that stays ages out; the result of the abandon is what the caller needs
        let _ = settle(shared, posting_id, Settled::not_made(&none));
    }
    result
}

/// Takes off the list the entries of orders whose answer never came, [`POSTING_LIFE_SECS`] or
/// more before `now` (seconds since 1970): how many went. Their payment page was never shown.
///
/// # Errors
///
/// When the list cannot be read or written.
pub fn forget_stale_postings(shared: &SharedKeyring, now: i64) -> CloudResult<usize> {
    let mut gone = 0;
    for entry in list(shared)? {
        if let Some(none) = NotMade::stale(&entry, now) {
            if settle(shared, &entry.checkout_id, Settled::not_made(&none))? {
                gone += 1;
            }
        }
    }
    Ok(gone)
}

/// An order placed: the claim key it went out under, the entry it holds on the list until
/// [`posted`], and what the token server answered.
#[derive(Debug)]
pub struct Placed<T> {
    pub claim: ClaimKey,
    /// The id of the order's entry on the list (to hand to [`posted`]).
    pub posting_id: String,
    pub answer: T,
}

/// Why an order was not placed.
#[derive(Debug)]
pub enum OrderError {
    /// The list refused to begin it ([`begin`]): nothing was sent.
    Refused(AddError),
    /// The token server's answer to the order is an error. A refusal ended the order's entry
    /// (the server made no checkout); no answer at all, a busy server or a 5xx leaves it as the
    /// record - the same order asked again goes out under the same claim key ([`Begun::Resumed`]).
    Token(TokenError),
}

impl fmt::Display for OrderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderError::Refused(e) => write!(f, "{e}"),
            OrderError::Token(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for OrderError {}

/// Places the order `draft` (an entry made by [`PendingCheckout::posting`], with the order's
/// `for_drive` and `change_tier` set): the entry goes on the list BEFORE anything is sent
/// ([`begin`]), `send` makes the call under the claim key the entry holds (the draft's, or the
/// one of an earlier attempt that got no answer), and the answer - or the refusal that ended the
/// order - is dealt with on the list. The caller turns the answer into the real entry and calls
/// [`posted`] before it shows anything of the checkout.
///
/// # Errors
///
/// [`OrderError`].
pub fn place_order<T>(
    shared: &SharedKeyring,
    draft: &PendingCheckout,
    send: impl FnOnce(&ClaimKey) -> Result<T, TokenError>,
) -> Result<Placed<T>, OrderError> {
    let (claim, posting_id) = match begin(shared, draft).map_err(OrderError::Refused)? {
        Begun::Fresh => (draft.claim_key(), draft.checkout_id.clone()),
        Begun::Resumed(earlier) => (earlier.claim_key(), earlier.checkout_id.clone()),
    };
    let claim = claim.map_err(|e| {
        OrderError::Refused(AddError::Keyring(CloudError::failed(format!(
            "the order's claim key cannot be read: {e}"
        ))))
    })?;
    match send(&claim) {
        Ok(answer) => Ok(Placed {
            claim,
            posting_id,
            answer,
        }),
        Err(e) => {
            // A refusal made no checkout; anything else may have: the entry stays as the record.
            // A list that cannot be written changes nothing for the caller (the error is the
            // token server's) and leaves the entry, which ages out.
            // collapse-ok: the entry is only a record; keeping it is the safe side of a failure
            let _ = order_refused(shared, &posting_id, &e);
            Err(OrderError::Token(e))
        }
    }
}

/// `name` trimmed and cut to [`MAX_NAME_CHARS`] characters.
fn short_name(name: &str) -> String {
    name.trim().chars().take(MAX_NAME_CHARS).collect::<String>().trim_end().to_string()
}

/// Takes the checkout `checkout_id` off the list; whether this call did (`false`: it was not
/// on it - another window took it off). Only [`settle`] calls it: a checkout leaves the list
/// with the token server's word only.
fn take_off(shared: &SharedKeyring, checkout_id: &str) -> CloudResult<bool> {
    let _lock = shared.lock(PENDING_KEY)?;
    let mut checkouts = read(shared)?;
    let before = checkouts.len();
    checkouts.retain(|c| c.checkout_id != checkout_id);
    if checkouts.len() == before {
        return Ok(false);
    }
    write(shared, checkouts)?;
    Ok(true)
}

/// Takes the checkout `checkout_id` off the list for `proof`: its claim secret goes, and with it
/// every way to the drive it paid for. Whether this call did (`false`: another window did).
///
/// # Errors
///
/// When the list cannot be read or written: the checkout stays.
pub fn settle(shared: &SharedKeyring, checkout_id: &str, proof: Settled) -> CloudResult<bool> {
    take_off(shared, checkout_id).map_err(|e| e.context(proof.what()))
}

/// Changes the kept checkout `checkout_id` by `change` (a mark of its lifecycle: in flight, a
/// sign-up that did not open); the checkout as it is now, `None` when it is not on the list
/// (another window took it off). The marks fit in the room [`add`] keeps for each unclaimed
/// checkout's period tokens, and [`claimed`] clears them when it fills that room.
fn change(
    shared: &SharedKeyring,
    checkout_id: &str,
    change: impl FnOnce(&mut PendingCheckout),
) -> CloudResult<Option<PendingCheckout>> {
    let _lock = shared.lock(PENDING_KEY)?;
    let mut checkouts = read(shared)?;
    let Some(checkout) = checkouts.iter_mut().find(|c| c.checkout_id == checkout_id) else {
        return Ok(None);
    };
    change(checkout);
    let changed = checkout.clone();
    let text = text_of(checkouts)?;
    if text.len() > MAX_PENDING_BYTES {
        fail!("the keyring's list of unfinished checkouts has no room left for {checkout_id}");
    }
    shared.set(PENDING_KEY, &text)?;
    Ok(Some(changed))
}

/// What became of an abandon ([`abandon`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Abandoned {
    /// The token server ended it (nobody can pay it any more): off the list.
    Ended,
    /// Off the list already: another window dealt with it.
    Settled,
    /// It stays on the list, its payment possibly under way ([`PendingCheckout::in_flight_since`]):
    /// it was paid meanwhile (`paid`: its drive comes with the payment's confirmation), or the
    /// token server could not end it (no answer, the provider did not answer, an answer this app
    /// does not read) - a payment made in a forgotten tab still brings the drive. Why.
    Kept { paid: bool, why: String },
}

/// Abandons the checkout `checkout_id` at `server` (its own token server): the popover closed
/// before the payment was confirmed. Only an answer that ends it takes it off the list; a paid
/// one (409 `checkout_paid`) and one the token server could not end stay, marked in flight, for
/// the polls that bring its drive. A 404 is a checkout the token server no longer has.
#[must_use]
pub fn abandon(server: &TokenServer<'_>, shared: &SharedKeyring, checkout_id: &str) -> Abandoned {
    let kept = |paid: bool, why: String| {
        let marked = change(shared, checkout_id, |c| {
            c.in_flight_since.get_or_insert_with(crate::now);
        });
        match marked {
            Ok(Some(_)) => Abandoned::Kept { paid, why },
            Ok(None) => Abandoned::Settled,
            Err(e) => Abandoned::Kept {
                paid,
                why: format!("{why}; it was not marked as in flight: {e}"),
            },
        }
    };
    let settled = |proof: Settled| match settle(shared, checkout_id, proof) {
        Ok(true) => Abandoned::Ended,
        Ok(false) => Abandoned::Settled,
        Err(e) => Abandoned::Kept {
            paid: false,
            why: format!("it ended, but could not be taken off the list: {e}"),
        },
    };
    match server.abandon_checkout(checkout_id) {
        Ok(answer) => match Settled::abandoned(answer) {
            Some(proof) => settled(proof),
            None => kept(false, format!("the token server keeps it open ({answer:?})")),
        },
        Err(e) if e.is_checkout_paid() => kept(true, e.said()),
        // The token server's own word: it has no such checkout. A bare 404 is not it - the
        // checkout may be paid, and its claim key is the only key to the drive (AZ-P1).
        Err(e) => match NoSuchCheckout::said_by(&e) {
            Some(none) => settled(Settled::gone(&none)),
            None => kept(false, e.to_string()),
        },
    }
}

/// A paid checkout's drive, its session in the keyring. `Debug` shows no session.
pub struct Claimed {
    /// The drive as the token server sealed it (its entry under the server's name).
    pub bundle: DriveBundle,
    /// The keyring's text of the drive's session now: the sign-up's, or the one the keyring had.
    pub session: String,
    /// The keyring had the drive's session already (another window or an earlier start claimed
    /// it; it may have rotated since): it stayed as it was.
    pub already: bool,
}

impl fmt::Debug for Claimed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Claimed")
            .field("bundle", &self.bundle)
            .field("session", &"<hidden>")
            .field("already", &self.already)
            .finish()
    }
}

/// What became of an unfinished checkout when its token server was asked.
#[derive(Debug)]
pub enum Polled {
    /// Not paid yet: it stays on the list.
    Pending,
    /// The answer ends the checkout ([`Final`]): to be said once.
    Final(Final),
    /// It stays on the list for the next poll ([`Transient`]).
    Transient(Transient),
    /// Off the list already: another window (or run) dealt with it; nothing to say.
    Settled,
}

/// An answer that ends a checkout.
#[derive(Debug)]
pub enum Final {
    /// Paid: the drive. The app adds it (its drives file, its window) and then tells the list
    /// ([`claimed`]).
    Claimed(Box<Claimed>),
    /// A renewal's (or a tier change's) months, claimed for the drive its checkout was made
    /// for: they wait with it until [`finish`] keeps the grant with the drive.
    Renewed { drive_id: String, months: u32 },
    /// Off the list: the payment did not go through (declined, taken back, the cash refused).
    Declined(String),
    /// Off the list: the token server no longer has it (expired, unknown), the drive can no
    /// longer be picked up here, or its drive's hand-over is gone. Why.
    Gone(String),
    /// Off the list: its sealed sign-up did not OPEN with its claim key, [`UNOPENABLE_AFTER`]
    /// times (sealed to another key or for another checkout; a sign-up that opens and does not
    /// read is kept, [`Transient::Unread`]). `error_id` is what support traces: the checkout's
    /// id.
    Unopenable { error_id: String, why: String },
}

impl Final {
    /// Why it ended, as said once; empty for a claimed one.
    #[must_use]
    pub fn why(&self) -> String {
        match self {
            Final::Claimed(_) | Final::Renewed { .. } => String::new(),
            Final::Declined(why) | Final::Gone(why) => why.clone(),
            Final::Unopenable { error_id, why } => format!(
                "its drive's sign-up does not open with this computer's key ({why}): support \
                 can help with the error ID {error_id}"
            ),
        }
    }
}

/// An answer that keeps a checkout for the next poll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transient {
    /// No answer from the token server (DNS, connection, TLS, a timeout).
    NoAnswer(String),
    /// The token server was busy ([`TokenError::Busy`]: a 5xx, its rate limit), and how many
    /// seconds it asked to wait (`Retry-After`) - the next question waits at least that long
    /// ([`Transient::retry_after`], AZ-P10).
    Busy { why: String, retry_after: Option<u64> },
    /// Its sealed sign-up did not open (fewer than [`UNOPENABLE_AFTER`] times): asked again.
    Unopened(String),
    /// This device could not keep what the answer brought, or change the list (its keyring).
    NotKept(String),
    /// An answer this app cannot act on: a status or a refusal it does not know (a newer token
    /// server - an update helps), a renewal's sign-up (claimed by the renewal, not here).
    Unread(String),
}

impl Transient {
    /// Why it stays.
    #[must_use]
    pub fn why(&self) -> &str {
        match self {
            Transient::Busy { why, .. }
            | Transient::NoAnswer(why)
            | Transient::Unopened(why)
            | Transient::NotKept(why)
            | Transient::Unread(why) => why,
        }
    }

    /// The seconds the token server asked to wait before it is asked again, when it said.
    #[must_use]
    pub fn retry_after(&self) -> Option<u64> {
        match self {
            Transient::Busy { retry_after, .. } => *retry_after,
            Transient::NoAnswer(_)
            | Transient::Unopened(_)
            | Transient::NotKept(_)
            | Transient::Unread(_) => None,
        }
    }
}

impl fmt::Display for Transient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.why())
    }
}

/// Asks `server` (the checkout's own token server) about `checkout` and acts on the answer: see
/// [`Polled`].
#[must_use]
pub fn poll(
    server: &TokenServer<'_>,
    shared: &SharedKeyring,
    checkout: &PendingCheckout,
) -> Polled {
    // A checkout whose order is on its way has no id to ask about yet.
    if checkout.is_posting() {
        return Polled::Pending;
    }
    let claim = match checkout.claim_key() {
        Ok(claim) => claim,
        // A kept secret that does not read is never erased on that: the entry stays, the app
        // says so, and an update or support can read what it holds (AZ-P6).
        Err(e) => {
            return Polled::Transient(Transient::Unread(format!(
                "its claim key cannot be read: {e}"
            )))
        }
    };
    let status = match server.checkout_status(&checkout.checkout_id, &claim) {
        Ok(status) => status,
        Err(TokenError::Connect(why)) => {
            return Polled::Transient(Transient::NoAnswer(format!(
                "no answer from the token server: {why}"
            )))
        }
        Err(e @ TokenError::Busy { .. }) => {
            return Polled::Transient(Transient::Busy {
                retry_after: e.retry_after(),
                why: e.to_string(),
            })
        }
        Err(e) => return Polled::Transient(Transient::Unread(e.to_string())),
    };
    // Declined, taken back, gone, the hand-over purged: the token server's own words end it.
    if let Some((proof, said)) = Settled::ended_by(&status, &checkout.checkout_id) {
        return end(shared, checkout, proof, said);
    }
    match status {
        CheckoutStatus::Pending => Polled::Pending,
        CheckoutStatus::Approved(bundle, paid) => {
            claim_drive(server, shared, checkout, *bundle, &paid)
        }
        CheckoutStatus::Unopenable(why) => unopenable(shared, checkout, why),
        // Opens, or is no sealed text, and does not read: kept, never counted. The sealed text is
        // intact for 30 days; an update of the app may read it.
        CheckoutStatus::Unreadable(why) => Polled::Transient(Transient::Unread(format!(
            "its sign-up does not read ({why}): update the app"
        ))),
        CheckoutStatus::Renewal(signup, paid) => claim_months(shared, checkout, &signup, &paid),
        CheckoutStatus::Unknown(why) => Polled::Transient(Transient::Unread(why)),
        // Ended above.
        CheckoutStatus::Declined(why) | CheckoutStatus::Gone(why) => {
            Polled::Transient(Transient::Unread(why))
        }
        CheckoutStatus::HandoverGone => Polled::Transient(Transient::Unread(String::from(
            "its drive's hand-over is gone",
        ))),
    }
}

/// The approved `checkout`'s drive `bundle`, its session kept in the keyring (a picked-up one
/// claimed as this computer's own token family first).
fn claim_drive(
    server: &TokenServer<'_>,
    shared: &SharedKeyring,
    checkout: &PendingCheckout,
    bundle: DriveBundle,
    paid: &Paid,
) -> Polled {
    // What was paid, on a durable medium, before the checkout can leave the list: a record that
    // cannot be kept holds the claim back for the next poll (the claim is idempotent).
    if let Err(e) = keep_paid(
        shared,
        checkout,
        bundle.drive_id(),
        bundle.tier.as_deref(),
        paid,
    ) {
        return Polled::Transient(Transient::NotKept(format!(
            "the record of the payment could not be kept: {e}"
        )));
    }
    let kept = if checkout.picked_up {
        keep_picked_up(server, shared, checkout, bundle)
    } else {
        match shared.keep_new_drive(&bundle) {
            Ok((session, already)) => Ok((bundle, session, already)),
            Err(e) => Err(Polled::Transient(Transient::NotKept(format!(
                "the new drive's session could not be kept: {e}"
            )))),
        }
    };
    let (bundle, session, already) = match kept {
        Ok(kept) => kept,
        Err(polled) => return polled,
    };
    // The drive key its claim code names is in the keyring before the app sees the drive, so
    // nothing sets the drive up, or unlocks it, with another one.
    #[cfg(feature = "encryption")]
    {
        if let Err(e) = keep_drive_key(shared, checkout, bundle.drive_id()) {
            return Polled::Transient(Transient::NotKept(format!(
                "the new drive's key could not be kept: {e}"
            )));
        }
    }
    // A checkout picked up by its claim code learns its tier here: its period tokens are
    // issued for it. Quietly: the next claim tries again.
    if checkout.tier.is_empty() {
        if let Some(tier) = bundle.tier.as_deref() {
            let _ = set_tier(shared, &checkout.checkout_id, tier);
        }
    }
    Polled::Final(Final::Claimed(Box::new(Claimed {
        bundle,
        session,
        already,
    })))
}

/// The drive key of the cash checkout `checkout` for its drive `drive_id`, in the keyring's
/// drive entries: on the buyer's computer the key made with the order, as the one the drive is
/// set up with (unless this computer holds the drive's key already); on a computer that picked
/// the drive up the id its claim code names, which every key it takes for the drive must have.
#[cfg(feature = "encryption")]
fn keep_drive_key(
    shared: &SharedKeyring,
    checkout: &PendingCheckout,
    drive_id: &str,
) -> CloudResult<()> {
    let keyring = shared.store();
    if let Some(key) = checkout.planned_drive_key()? {
        if device::load_drive_key(keyring, drive_id)?.is_none() {
            device::plan_drive_key(keyring, drive_id, &key)?;
        }
    } else if let Some(id) = checkout.named_key_id()? {
        device::commit_drive_key(keyring, drive_id, &id)?;
    }
    Ok(())
}

/// Keeps the [`PaidRecord`] of `checkout`, paid for `drive_id`: its tier (the answer's, else
/// the sign-up's, else the order's), its months (the answer's), the sale the token server booked.
fn keep_paid(
    shared: &SharedKeyring,
    checkout: &PendingCheckout,
    drive_id: &str,
    signup_tier: Option<&str>,
    paid: &Paid,
) -> CloudResult<()> {
    let tier = [signup_tier.unwrap_or_default(), checkout.tier.as_str()]
        .into_iter()
        .map(str::trim)
        .find(|t| !t.is_empty())
        .unwrap_or_default()
        .to_string();
    shared.paid().keep(&PaidRecord {
        checkout: checkout.checkout_id.clone(),
        drive: drive_id.to_string(),
        tier,
        months: paid.months_paid.filter(|m| *m > 0).unwrap_or(paid.months),
        at: crate::now(),
        receipt: paid.receipt.clone(),
    })
}

/// A sign-up without a drive (a renewal, a tier change): its months for the drive the checkout
/// was made for, kept on the list until [`finish`] has the grant with that drive. A checkout
/// that names no drive stays unclaimed: no drive is guessed for months someone paid.
fn claim_months(
    shared: &SharedKeyring,
    checkout: &PendingCheckout,
    signup: &azlin_contract::RenewalSignup,
    paid: &Paid,
) -> Polled {
    let drive_id = checkout.for_drive.trim();
    if drive_id.is_empty() {
        return Polled::Transient(Transient::Unread(String::from(
            "its sign-up adds months to a drive, and this computer did not keep which one",
        )));
    }
    let wire = &signup.period_tokens;
    let grant = PeriodTokens {
        checkout_id: wire.checkout_id.trim().to_string(),
        months: wire.months,
        issue_key: wire.issue_key.trim().to_string(),
        recurring: wire.recurring,
    };
    if grant.checkout_id != checkout.checkout_id || grant.months == 0 || grant.issue_key.is_empty() {
        // It opened, so it is the token server's own text: nothing for a strike to count, and no
        // reason to erase the claim secret on it (AZ-P6).
        return Polled::Transient(Transient::Unread(String::from(
            "its sign-up grants months of another checkout, or none",
        )));
    }
    // What was paid, before the months are claimed (see [`claim_drive`]).
    if let Err(e) = keep_paid(shared, checkout, drive_id, Some(signup.tier.as_str()), paid) {
        return Polled::Transient(Transient::NotKept(format!(
            "the record of the payment could not be kept: {e}"
        )));
    }
    match claimed(shared, &checkout.checkout_id, drive_id, Some(&grant)) {
        Ok(()) => Polled::Final(Final::Renewed {
            drive_id: drive_id.to_string(),
            months: grant.months,
        }),
        Err(e) => Polled::Transient(Transient::NotKept(format!(
            "its months could not be kept for the drive {drive_id}: {e}"
        ))),
    }
}

/// A sign-up that does not OPEN with the claim key (an AEAD failure, nothing else), `why`:
/// counted on the list, given up the [`UNOPENABLE_AFTER`]th time.
fn unopenable(shared: &SharedKeyring, checkout: &PendingCheckout, why: String) -> Polled {
    let counted = change(shared, &checkout.checkout_id, |c| {
        c.open_failures = c.open_failures.saturating_add(1);
    });
    match counted {
        Ok(Some(now)) => match Strikes::of(&now) {
            Some(strikes) => end(
                shared,
                checkout,
                Settled::unopenable(&strikes),
                Final::Unopenable {
                    error_id: checkout.checkout_id.clone(),
                    why,
                },
            ),
            None => Polled::Transient(Transient::Unopened(why)),
        },
        Ok(None) => Polled::Settled,
        Err(e) => Polled::Transient(Transient::NotKept(format!(
            "{why}; the list could not count it: {e}"
        ))),
    }
}

/// The tier of the kept checkout `checkout_id`, when it has none yet.
fn set_tier(shared: &SharedKeyring, checkout_id: &str, tier: &str) -> CloudResult<()> {
    let _lock = shared.lock(PENDING_KEY)?;
    let mut checkouts = read(shared)?;
    let Some(checkout) = checkouts
        .iter_mut()
        .find(|c| c.checkout_id == checkout_id && c.tier.is_empty())
    else {
        return Ok(());
    };
    checkout.tier = tier.trim().to_string();
    let text = text_of(checkouts)?;
    shared.set(PENDING_KEY, &text)
}

/// The drive of `checkout`, picked up here by its claim code: a token family of this computer's
/// own, claimed with the sealed sign-up's ticket (`POST /v1/drives/{id}/claim`), so the buyer's
/// computer and this one never spend one family's tokens - under the drive's keyring lock, and
/// once (a session the keyring has for the drive is kept as it is). A drive from before the
/// tickets (no ticket, or 401) keeps the sealed drive token; one picked up as often as it may be
/// (409 `claims_used`, 410 `claim_expired`) is taken off the list. The drive (its token the
/// claimed one), the keyring's text of its session, whether the keyring had it.
fn keep_picked_up(
    server: &TokenServer<'_>,
    shared: &SharedKeyring,
    checkout: &PendingCheckout,
    mut bundle: DriveBundle,
) -> Result<(DriveBundle, String, bool), Polled> {
    let key = azul_storage::config::keyring_key(bundle.drive_id());
    let not_kept = |e: CloudError| {
        Polled::Transient(Transient::NotKept(format!(
            "the new drive's session could not be kept: {e}"
        )))
    };
    let _lock = shared.lock(&key).map_err(not_kept)?;
    if let Some(text) = shared.get(&key).map_err(not_kept)? {
        let ours = crate::session::AzlinSession::from_keyring_secret(&text)
            .is_ok_and(|session| session.drive_id == bundle.drive_id());
        if ours {
            return Ok((bundle, text, true));
        }
    }
    let mut claiming = None;
    if let Some(ticket) = bundle.claim.as_ref().map(|c| c.ticket.clone()) {
        // One key names the pick-up across its retries (crate::outbox): a claim whose answer
        // was lost is the same pick-up asked again - the same family, counted once.
        let call = shared
            .outbox()
            .open(IdempotentScope::Claim, bundle.drive_id(), "")
            .map_err(|e| {
                Polled::Transient(Transient::Unread(format!("the drive was not picked up: {e}")))
            })?;
        let claimed = server.claim_drive(bundle.drive_id(), &ticket, call.key());
        if claimed.is_ok() {
            // closed once the keyring has the family: until then a repeat gets it again
            claiming = Some(call);
        } else if claimed.as_ref().err().is_some_and(crate::outbox::is_final_error) {
            call.close();
        }
        match claimed {
            Ok(family) => bundle.drive_token = family.drive_token,
            // A drive from before the tickets: the sealed drive token is its only one.
            Err(e) if e.is_unauthorized() => {}
            Err(TokenError::Connect(why)) => {
                return Err(Polled::Transient(Transient::NoAnswer(format!(
                    "no answer from the token server: {why}"
                ))))
            }
            Err(e) => {
                // Picked up as often as it may be, or too late: the contract's words end it.
                if let Some(ended) = ClaimsEnded::said_by(&e) {
                    return Err(end(
                        shared,
                        checkout,
                        Settled::claims_ended(&ended),
                        Final::Gone(format!(
                            "the drive cannot be picked up here any more: {}",
                            e.said()
                        )),
                    ));
                }
                // A final refusal closed the call (409 already_answered: the next claim is a
                // pick-up of its own); a busy server left it open for the next poll.
                return Err(Polled::Transient(Transient::Unread(format!(
                    "the drive was not picked up: {e}"
                ))));
            }
        }
    }
    let text = bundle.session().to_keyring_secret();
    shared.set(&key, &text).map_err(not_kept)?;
    if let Some(call) = claiming {
        call.close();
    }
    Ok((bundle, text, false))
}

/// Takes `checkout` off the list for `proof`; `said` once by the poll that did.
fn end(shared: &SharedKeyring, checkout: &PendingCheckout, proof: Settled, said: Final) -> Polled {
    match settle(shared, &checkout.checkout_id, proof) {
        Ok(true) => Polled::Final(said),
        Ok(false) => Polled::Settled,
        Err(e) => Polled::Transient(Transient::NotKept(format!(
            "{}; it could not be taken off the list: {e}",
            said.why()
        ))),
    }
}

/// The drive of the checkout `checkout_id` is saved (its drives file has it) as `drive_id`.
/// Without `grant` (a development sign-up, a checkout from before period tokens) the checkout
/// is done: off the list. With it, it stays - its claim secret, and now what the period
/// tokens' issue needs ([`PendingTokens`]: the drive, the months, the issue key) - until
/// [`finish`] has kept them with the drive.
///
/// # Errors
///
/// When the list cannot be read or written (or, for a drive id longer than its room, would
/// outgrow [`MAX_PENDING_BYTES`]): the checkout stays as it was, and is claimed again.
pub fn claimed(
    shared: &SharedKeyring,
    checkout_id: &str,
    drive_id: &str,
    grant: Option<&PeriodTokens>,
) -> CloudResult<()> {
    let Some(grant) = grant else {
        settle(shared, checkout_id, Settled::claimed())?;
        return Ok(());
    };
    let _lock = shared.lock(PENDING_KEY)?;
    let mut checkouts = read(shared)?;
    let Some(checkout) = checkouts.iter_mut().find(|c| c.checkout_id == checkout_id) else {
        // Finished by another window meanwhile.
        return Ok(());
    };
    checkout.period = Some(PendingTokens {
        drive_id: drive_id.trim().to_string(),
        months: grant.months,
        issue_key: grant.issue_key.trim().to_string(),
        recurring: grant.recurring,
    });
    // Claimed: neither in flight nor unopened any more, and their room is the tokens' now.
    checkout.in_flight_since = None;
    checkout.open_failures = 0;
    // Its drive key went into the keyring's drive entries with the claim: no second copy.
    checkout.drive_key = None;
    let text = text_of(checkouts)?;
    if text.len() > MAX_PENDING_BYTES {
        fail!(
            "the checkout {checkout_id}'s period tokens do not fit in the keyring's list of \
             unfinished checkouts"
        );
    }
    shared.set(PENDING_KEY, &text)
}

/// What became of a claimed checkout's period tokens ([`finish`]).
#[derive(Debug)]
pub enum Finished {
    /// Its grant is kept with the drive and its first month issued, checked and kept; the
    /// checkout is off the list.
    Issued { drive_id: String, count: usize },
    /// Its grant is kept with the drive; its first month comes with the drive's next look
    /// (why it did not come now). The checkout is off the list.
    Granted { drive_id: String, why: String },
    /// Off the list without tokens: why - to be said once (a payment taken back, the token
    /// server takes no issue key of a checkout from before period tokens, the payment mandate
    /// was stopped, every month issued to a request this computer does not hold).
    Dropped(String),
    /// It stays on the list, issue key and all, for the next try: why (its grant could not be
    /// kept with the drive).
    Kept(String),
    /// Off the list already: another window finished it.
    Settled,
    /// The grant is kept on this computer and its first month asked for (`count` months issued
    /// now), but the drive's own vault does not hold it yet - `why` -, so the checkout stays on
    /// the list, claim secret and issue key with it, until it does or until [`GRANT_WAIT_SECS`]
    /// are gone (AZ-P5): a paid month is never left to one file.
    Waits {
        drive_id: String,
        count: usize,
        why: String,
    },
    /// Off the list after [`GRANT_WAIT_SECS`] without a vault of the drive: the grant is in two
    /// copies on this computer (`why` the drive has none). The app tells its user to keep a copy
    /// of its data: the months are on this computer only.
    LocalOnly {
        drive_id: String,
        count: usize,
        why: String,
    },
}

/// How long a paid checkout waits in the keyring for the drive's vault to take its grant: 30
/// days, as long as the token server keeps the sealed sign-up. After that it is kept in a
/// second copy on this computer and the keyring's goes.
pub const GRANT_WAIT_SECS: i64 = 30 * 86_400;

/// Whether `started_at` (seconds since 1970) is [`GRANT_WAIT_SECS`] or more before `now`.
fn grant_wait_over(started_at: i64, now: u64) -> bool {
    i64::try_from(now).is_ok_and(|now| now.saturating_sub(started_at) >= GRANT_WAIT_SECS)
}

/// The lock under which one checkout's period tokens are issued (a second window waits, then
/// finds it settled: the token server issues them once).
fn finish_lock(checkout_id: &str) -> String {
    format!("{PENDING_KEY}/{checkout_id}/tokens")
}

/// Hands the claimed `checkout`'s period grant (`owed`: its drive, its months, its issue key)
/// to its drive's vaults `tokens` - from then on any device of the drive issues its months, one
/// a month -, asks `server` (its own token server) for its first month, and takes the checkout
/// off the list, its claim secret and issue key with it - but only when the grant is in two
/// places ([`crate::vault::Kept::is_redundant`]): the drive's own vault has it, or, after
/// [`GRANT_WAIT_SECS`], a second copy on this computer. Until then the keyring's entry is the
/// second copy and stays ([`Finished::Waits`]). An issue request an older version kept in `store`
/// moves into the vaults with the grant (it is sent again as it is). See [`Finished`].
#[must_use]
pub fn finish(
    server: &TokenServer<'_>,
    shared: &SharedKeyring,
    store: &PeriodTokenStore,
    tokens: &DriveTokens,
    checkout: &PendingCheckout,
    owed: &PendingTokens,
    now: u64,
) -> Finished {
    let id = checkout.checkout_id.as_str();
    let _lock = match shared.lock(&finish_lock(id)) {
        Ok(lock) => lock,
        Err(e) => return Finished::Kept(e.to_string()),
    };
    match list(shared) {
        Ok(checkouts) if !checkouts.iter().any(|c| c.checkout_id == id) => return Finished::Settled,
        Ok(_) => {}
        Err(e) => return Finished::Kept(e.to_string()),
    }
    let taken_off = |settled: Settled, finished: Finished| match settle(shared, id, settled) {
        Ok(_) => finished,
        Err(e) => Finished::Kept(format!("it could not be taken off the list: {e}")),
    };
    // A grant no token server issues: nothing to wait for.
    if let Some(never) = NeverIssued::unusable_grant(owed.months, &owed.issue_key, MAX_BLINDED) {
        return taken_off(
            Settled::never_issued(&never),
            Finished::Dropped(format!(
                "its sealed sign-up grants {} months without an issue key, or more than one issue \
                 takes: support can help",
                owed.months
            )),
        );
    }
    if tokens.drive_id() != owed.drive_id {
        return Finished::Kept(format!(
            "its period tokens are for the drive {}, not {}",
            owed.drive_id,
            tokens.drive_id()
        ));
    }
    let mut kept = VaultGrant::new(id, &checkout.tier, owed.months, &owed.issue_key);
    kept.recurring = owed.recurring;
    kept.change_tier = checkout.change_tier;
    let mut grant = VaultState::with_grant(kept);
    // The paid checkout is the drive's price lock for its next renewal; the lock it renewed
    // under is used by it.
    grant.locks.push(RenewalLock {
        checkout_id: id.to_string(),
        tier: checkout.tier.clone(),
        issue_key: owed.issue_key.clone(),
        ladder_version: checkout.ladder_version,
        kept_at: now,
        used_by: None,
    });
    if !checkout.renews.trim().is_empty() {
        if let Ok(state) = tokens.state() {
            if let Some(old) = state.locks.iter().find(|l| l.checkout_id == checkout.renews.trim()) {
                let mut used = old.clone();
                used.used_by = Some(id.to_string());
                grant.locks.push(used);
            }
        }
    }
    if let Ok(Some(older)) = store.issue_request(id) {
        grant.issues.push(older);
    }
    let in_vaults = match tokens.join(&grant) {
        Ok(in_vaults) => in_vaults,
        Err(e) => {
            return Finished::Kept(format!(
                "its period grant could not be kept with the drive: {e}"
            ))
        }
    };
    // In the vaults now; a file left behind is joined in again, harmlessly.
    let _ = store.forget_issue_request(id);
    let drive_id = owed.drive_id.clone();
    // The first month, whether or not the drive's vault has the grant yet: the tokens land in
    // every vault there is.
    let issued = issue_next(server, tokens, id, now);
    if let Ok(over) = &issued {
        if let Some((never, why)) = NeverIssued::ended(over) {
            return taken_off(Settled::never_issued(&never), Finished::Dropped(why));
        }
    }
    let (count, why) = match issued {
        Ok(Issued::Tokens(issued)) => (issued.len(), String::new()),
        Ok(Issued::NotDue { .. }) => (0, String::from("its first month is not due yet")),
        Ok(Issued::Ended(why)) => (0, why),
        Err(TokenError::Connect(why)) => (0, format!("no answer from the token server: {why}")),
        Err(e) => (0, e.to_string()),
    };
    // The keyring's entry is the grant's second copy until the grant is in two places elsewhere.
    let (proof, second_copy) = match GrantKept::of(&in_vaults) {
        Some(proof) => (proof, None),
        None if grant_wait_over(checkout.started_at, now) => match tokens.keep_second_copy() {
            Ok(second) => match GrantKept::of(&second) {
                Some(proof) => (proof, Some(in_vaults.why_not_redundant())),
                None => {
                    return Finished::Kept(String::from("its second copy is not in two places"))
                }
            },
            Err(e) => {
                return Finished::Kept(format!("its period grant's second copy was not kept: {e}"))
            }
        },
        None => {
            return Finished::Waits {
                drive_id,
                count,
                why: in_vaults.why_not_redundant(),
            }
        }
    };
    let finished = match second_copy {
        Some(no_vault) => Finished::LocalOnly {
            drive_id,
            count,
            why: no_vault,
        },
        None if count > 0 => Finished::Issued { drive_id, count },
        None => Finished::Granted { drive_id, why },
    };
    taken_off(Settled::finished(&proof), finished)
}
