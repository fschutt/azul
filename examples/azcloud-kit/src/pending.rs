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
//! the drive claimed and nothing more owed, the payment declined, the checkout gone or ended by
//! its abandon ([`abandon`]), a sealed sign-up that did not open twice. Nothing the app decides
//! on its own (a closed popover, a timeout) takes one off.
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

use azlin_contract::{AbandonAnswer, IdempotentScope};

use crate::{
    bundle::{DriveBundle, PeriodTokens},
    claim::{ClaimError, ClaimKey},
    error::{fail, CloudError, CloudResult},
    period::{issue_next, Issued, PeriodTokenStore},
    shared::SharedKeyring,
    token::{CheckoutStatus, TokenError, TokenServer, MAX_BLINDED},
    vault::{DriveTokens, RenewalLock, VaultGrant, VaultState},
};

/// The keyring entry of the unfinished checkouts.
pub const PENDING_KEY: &str = "azcloud/checkouts";
/// The `format` of its text.
pub const PENDING_FORMAT: &str = "azcloud.checkouts";
/// The most bytes the list's entry takes: Windows' Credential Manager keeps at most 2560 per
/// credential (`CRED_MAX_CREDENTIAL_BLOB_SIZE`), the strictest of the keyrings - about ten
/// checkouts.
pub const MAX_PENDING_BYTES: usize = 2560;
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
        }
    }

    /// A cash checkout (cash by post): its letter takes days or weeks, so it is asked about
    /// once a day, never in a loop.
    #[must_use]
    pub fn is_cash(&self) -> bool {
        self.method == crate::cash::CASH_METHOD
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

/// Adds `checkout` to the list (replacing one with its id; its name cut to
/// [`MAX_NAME_CHARS`]) - unless the list's entry would grow past [`MAX_PENDING_BYTES`], with
/// room for every unclaimed checkout's issue key ([`claimed`]): then the checkout is refused,
/// before its payment page opens (the ones waiting finish or expire first; a paid one is
/// claimed at the next poll).
///
/// # Errors
///
/// When the list cannot be read or written, or has no room left.
pub fn add(shared: &SharedKeyring, checkout: &PendingCheckout) -> CloudResult<()> {
    let _lock = shared.lock(PENDING_KEY)?;
    let mut checkouts = read(shared)?;
    checkouts.retain(|c| c.checkout_id != checkout.checkout_id);
    let waiting = checkouts.len();
    let mut kept = checkout.clone();
    kept.name = short_name(&kept.name);
    checkouts.push(kept);
    let unclaimed = checkouts.iter().filter(|c| c.period.is_none()).count();
    let text = text_of(checkouts)?;
    if text.len() + unclaimed * PERIOD_ROOM > MAX_PENDING_BYTES {
        fail!(
            "{waiting} unfinished checkouts wait already, and the keyring keeps no more of them in \
             one entry (Windows keeps {MAX_PENDING_BYTES} bytes): let them be paid or expire \
             first"
        );
    }
    shared.set(PENDING_KEY, &text)
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

/// What the token server said that ends a checkout: the only thing that takes one off the list
/// ([`settle`]). Made from its answers here and by [`Settled::abandoned`], never from a guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settled(SettledBy);

#[derive(Debug, Clone, PartialEq, Eq)]
enum SettledBy {
    /// Its drive is in the drives file and nothing more is owed for it.
    Claimed,
    /// Its period grant is kept with its drive, or the token server will never issue it.
    Finished,
    /// The payment did not go through, or went back.
    Declined,
    /// The token server no longer has it, or its drive's hand-over is gone.
    Gone,
    /// Its abandon ended it at the token server.
    Abandoned(AbandonAnswer),
    /// Its sealed sign-up did not open with its claim key, [`UNOPENABLE_AFTER`] times.
    Unopenable,
}

impl Settled {
    /// What it ends the checkout for, for an error's context.
    fn what(&self) -> String {
        let why = match &self.0 {
            SettledBy::Claimed => "its drive is saved",
            SettledBy::Finished => "its period grant is kept with its drive",
            SettledBy::Declined => "its payment did not go through",
            SettledBy::Gone => "the token server no longer has it",
            SettledBy::Abandoned(answer) => {
                return format!("ending the checkout: its abandon answered {answer:?}")
            }
            SettledBy::Unopenable => "its sign-up does not open",
        };
        format!("ending the checkout: {why}")
    }

    /// The abandon's answer, when it ended the checkout ([`AbandonAnswer::is_over`]); `None`
    /// when the checkout may still be paid.
    #[must_use]
    pub fn abandoned(answer: AbandonAnswer) -> Option<Settled> {
        answer
            .is_over()
            .then_some(Settled(SettledBy::Abandoned(answer)))
    }
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
        Err(TokenError::Refused { code, message, .. }) if code == "checkout_paid" => {
            kept(true, if message.is_empty() { code } else { message })
        }
        Err(TokenError::Refused { status: 404, .. }) => settled(Settled(SettledBy::Gone)),
        Err(e) => kept(false, e.to_string()),
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
    /// Off the list: its sealed sign-up did not open with its claim key, [`UNOPENABLE_AFTER`]
    /// times (sealed to another key, or holding no drive). `error_id` is what support traces:
    /// the checkout's id.
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
    /// The token server was busy (5xx, 429).
    Busy(String),
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
            Transient::NoAnswer(why)
            | Transient::Busy(why)
            | Transient::Unopened(why)
            | Transient::NotKept(why)
            | Transient::Unread(why) => why,
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
    let claim = match checkout.claim_key() {
        Ok(claim) => claim,
        // Its sealed sign-up can never be opened.
        Err(e) => {
            return end(
                shared,
                checkout,
                SettledBy::Unopenable,
                Final::Unopenable {
                    error_id: checkout.checkout_id.clone(),
                    why: format!("its claim key cannot be read: {e}"),
                },
            )
        }
    };
    match server.checkout_status(&checkout.checkout_id, &claim) {
        Ok(CheckoutStatus::Pending) => Polled::Pending,
        Ok(CheckoutStatus::Approved(bundle)) => claim_drive(server, shared, checkout, *bundle),
        Ok(CheckoutStatus::Declined(why)) => end(
            shared,
            checkout,
            SettledBy::Declined,
            Final::Declined(format!("the payment did not go through: {why}")),
        ),
        Ok(CheckoutStatus::Gone(why)) => end(
            shared,
            checkout,
            SettledBy::Gone,
            Final::Gone(format!("the token server no longer has it: {why}")),
        ),
        Ok(CheckoutStatus::HandoverGone) => end(
            shared,
            checkout,
            SettledBy::Gone,
            Final::Gone(format!(
                "it was paid, but the token server keeps its drive's hand-over only 30 days \
                 after the payment and has it no more: support can help with the checkout \
                 {}",
                checkout.checkout_id
            )),
        ),
        Ok(CheckoutStatus::Unopenable(why)) => unopenable(shared, checkout, why),
        Ok(CheckoutStatus::Renewal(signup)) => claim_months(shared, checkout, &signup),
        Ok(CheckoutStatus::Unknown(why)) => Polled::Transient(Transient::Unread(why)),
        Err(TokenError::Connect(why)) => Polled::Transient(Transient::NoAnswer(format!(
            "no answer from the token server: {why}"
        ))),
        Err(e @ TokenError::Refused { status, .. }) if status >= 500 || status == 429 => {
            Polled::Transient(Transient::Busy(e.to_string()))
        }
        Err(e) => Polled::Transient(Transient::Unread(e.to_string())),
    }
}

/// The approved `checkout`'s drive `bundle`, its session kept in the keyring (a picked-up one
/// claimed as this computer's own token family first).
fn claim_drive(
    server: &TokenServer<'_>,
    shared: &SharedKeyring,
    checkout: &PendingCheckout,
    bundle: DriveBundle,
) -> Polled {
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

/// A sealed sign-up of `checkout` that did not open, `why`: counted on the list, given up the
/// [`UNOPENABLE_AFTER`]th time.
/// A sign-up without a drive (a renewal, a tier change): its months for the drive the checkout
/// was made for, kept on the list until [`finish`] has the grant with that drive. A checkout
/// that names no drive stays unclaimed: no drive is guessed for months someone paid.
fn claim_months(
    shared: &SharedKeyring,
    checkout: &PendingCheckout,
    signup: &azlin_contract::RenewalSignup,
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
        return unopenable(
            shared,
            checkout,
            String::from("its sign-up grants months of another checkout, or none"),
        );
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

fn unopenable(shared: &SharedKeyring, checkout: &PendingCheckout, why: String) -> Polled {
    let counted = change(shared, &checkout.checkout_id, |c| {
        c.open_failures = c.open_failures.saturating_add(1);
    });
    match counted {
        Ok(Some(now)) if now.open_failures >= UNOPENABLE_AFTER => end(
            shared,
            checkout,
            SettledBy::Unopenable,
            Final::Unopenable {
                error_id: checkout.checkout_id.clone(),
                why,
            },
        ),
        Ok(Some(_)) => Polled::Transient(Transient::Unopened(why)),
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
            Err(TokenError::Refused { status: 401, .. }) => {}
            Err(TokenError::Refused { code, message, .. })
                if code == "claims_used" || code == "claim_expired" =>
            {
                let why = if message.is_empty() { code } else { message };
                return Err(end(
                    shared,
                    checkout,
                    SettledBy::Gone,
                    Final::Gone(format!("the drive cannot be picked up here any more: {why}")),
                ));
            }
            Err(TokenError::Connect(why)) => {
                return Err(Polled::Transient(Transient::NoAnswer(format!(
                    "no answer from the token server: {why}"
                ))))
            }
            // A final refusal closed the call (409 already_answered: the next claim is a pick-up
            // of its own); a busy server left it open for the next poll.
            Err(e) => {
                return Err(Polled::Transient(Transient::Unread(format!(
                    "the drive was not picked up: {e}"
                ))))
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

/// Takes `checkout` off the list for `by`; `said` once by the poll that did.
fn end(shared: &SharedKeyring, checkout: &PendingCheckout, by: SettledBy, said: Final) -> Polled {
    match settle(shared, &checkout.checkout_id, Settled(by)) {
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
        settle(shared, checkout_id, Settled(SettledBy::Claimed))?;
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
}

/// The lock under which one checkout's period tokens are issued (a second window waits, then
/// finds it settled: the token server issues them once).
fn finish_lock(checkout_id: &str) -> String {
    format!("{PENDING_KEY}/{checkout_id}/tokens")
}

/// Hands the claimed `checkout`'s period grant (`owed`: its drive, its months, its issue key)
/// to its drive's vaults `tokens` - from then on any device of the drive issues its months, one
/// a month - then takes the checkout off the list, its claim secret with it, and asks
/// `server` (its own token server) for its first month. An issue request an older version kept
/// in `store` moves into the vaults with the grant (it is sent again as it is). See
/// [`Finished`].
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
    let done = |finished: Finished| match settle(shared, id, Settled(SettledBy::Finished)) {
        Ok(_) => finished,
        Err(e) => Finished::Kept(format!("it could not be taken off the list: {e}")),
    };
    // A grant no token server issues: nothing to wait for.
    let months_ok = usize::try_from(owed.months).is_ok_and(|m| (1..=MAX_BLINDED).contains(&m));
    if !months_ok || owed.issue_key.trim().is_empty() {
        return done(Finished::Dropped(format!(
            "its sealed sign-up grants {} months without an issue key, or more than one issue \
             takes: support can help",
            owed.months
        )));
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
    if let Err(e) = tokens.join(&grant) {
        return Finished::Kept(format!("its period grant could not be kept with the drive: {e}"));
    }
    // In the vaults now; a file left behind is joined in again, harmlessly.
    let _ = store.forget_issue_request(id);
    let drive_id = owed.drive_id.clone();
    match issue_next(server, tokens, id, now) {
        Ok(Issued::Tokens(issued)) => done(Finished::Issued {
            drive_id,
            count: issued.len(),
        }),
        Ok(Issued::Ended(why)) => done(Finished::Dropped(why)),
        Ok(Issued::NotDue { .. }) => done(Finished::Granted {
            drive_id,
            why: String::from("its first month is not due yet"),
        }),
        Err(TokenError::Connect(why)) => done(Finished::Granted {
            drive_id,
            why: format!("no answer from the token server: {why}"),
        }),
        Err(e) => done(Finished::Granted {
            drive_id,
            why: e.to_string(),
        }),
    }
}
