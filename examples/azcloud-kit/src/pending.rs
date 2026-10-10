//! The checkouts this device started and has no drive of yet: ONE keyring entry
//! ([`PENDING_KEY`]) holding them all - each one's id, the secret of its claim key, its tier,
//! when it started, the token server it was made at and the name typed for the drive - changed
//! only under the lock of that entry, so two windows (two processes) never lose each other's.
//! The entry outlives the app: a drive paid after "Stop waiting", or while the app was closed,
//! still reaches it. It stays within [`MAX_PENDING_BYTES`] (Windows' Credential Manager keeps no
//! more in one entry): a checkout that would not fit is refused before its payment page opens.
//!
//! An app polls them while its dialog waits, at every start and in the background after "Stop
//! waiting" ([`poll`]): an approved checkout becomes the drive - its session goes into the
//! keyring under the drive's lock unless the keyring has it already (idempotent by drive id: a
//! session that rotated since is never replaced by the sign-up's spent token) - and the app
//! tells the list once the drive is in its drives file ([`claimed`]): a checkout without period
//! tokens is done then; a paid one stays, its claim secret and now its issue key with it, until
//! [`finish`] has issued its period tokens and kept them (AZDRIVE-INTEGRATION §4). A checkout
//! whose payment was declined, or that the token server no longer has, is taken off at once -
//! and said by the one poll that took it off.
//!
//! Blocking (the keyring, the token server): call it from an azul `Thread`.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    bundle::{DriveBundle, PeriodTokens},
    claim::{ClaimError, ClaimKey},
    error::{fail, CloudError, CloudResult},
    period::{issue_tokens, PeriodTokenStore},
    shared::SharedKeyring,
    token::{CheckoutStatus, TokenError, TokenServer},
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
}

impl fmt::Debug for PendingTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingTokens")
            .field("drive_id", &self.drive_id)
            .field("months", &self.months)
            .field("issue_key", &"<hidden>")
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
        }
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
/// on it - another window took it off).
///
/// # Errors
///
/// When the list cannot be read or written.
pub fn remove(shared: &SharedKeyring, checkout_id: &str) -> CloudResult<bool> {
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
    /// Paid: the drive. The app adds it (its drives file, its window) and then takes the
    /// checkout off the list.
    Claimed(Box<Claimed>),
    /// This poll took it off the list: why (the payment was declined, the token server no longer
    /// has it) - to be said once.
    Dropped(String),
    /// It stays on the list for the next try: why (no answer, a refusal, a sign-up that does not
    /// open with its claim key, a keyring that failed).
    Kept(String),
    /// Off the list already: another window (or run) dealt with it; nothing to say.
    Settled,
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
        Err(e) => return drop_it(shared, checkout, format!("its claim key cannot be read ({e})")),
    };
    match server.checkout_status(&checkout.checkout_id, &claim) {
        Ok(CheckoutStatus::Pending) => Polled::Pending,
        Ok(CheckoutStatus::Approved(bundle)) => match shared.keep_new_drive(&bundle) {
            Ok((session, already)) => Polled::Claimed(Box::new(Claimed {
                bundle: *bundle,
                session,
                already,
            })),
            Err(e) => Polled::Kept(format!("the new drive's session could not be kept: {e}")),
        },
        Ok(CheckoutStatus::Declined(why)) => drop_it(
            shared,
            checkout,
            format!("the payment did not go through: {why}"),
        ),
        Ok(CheckoutStatus::Gone(why)) => drop_it(
            shared,
            checkout,
            format!("the token server no longer has it: {why}"),
        ),
        Err(TokenError::Connect(why)) => {
            Polled::Kept(format!("no answer from the token server: {why}"))
        }
        Err(e) => Polled::Kept(e.to_string()),
    }
}

/// Takes `checkout` off the list because of `why`.
fn drop_it(shared: &SharedKeyring, checkout: &PendingCheckout, why: String) -> Polled {
    match remove(shared, &checkout.checkout_id) {
        Ok(true) => Polled::Dropped(why),
        Ok(false) => Polled::Settled,
        Err(e) => Polled::Kept(format!("{why}; it could not be taken off the list: {e}")),
    }
}

/// The drive of the checkout `checkout_id` is saved (its drives file has it) as `drive_id`.
/// Without `grant` (a development sign-up, a checkout from before period tokens) the checkout
/// is done: off the list. With it, it stays - its claim secret, and now what the period
/// tokens' issue needs ([`PendingTokens`]: the drive, the months, the issue key) - until
/// [`finish`] has kept them.
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
        remove(shared, checkout_id)?;
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
    });
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
    /// Issued, checked and kept in the drive's store; the checkout is off the list.
    Issued { drive_id: String, count: usize },
    /// Off the list without tokens: why - to be said once (they were issued before and the
    /// answer was lost, the token server takes no issue key of a checkout from before period
    /// tokens, the payment mandate was stopped).
    Dropped(String),
    /// It stays on the list, issue key and all, for the next try: why.
    Kept(String),
    /// Off the list already: another window finished it.
    Settled,
}

/// The lock under which one checkout's period tokens are issued (a second window waits, then
/// finds it settled: the token server issues them once).
fn finish_lock(checkout_id: &str) -> String {
    format!("{PENDING_KEY}/{checkout_id}/tokens")
}

/// Issues the period tokens `owed` for the claimed `checkout` at `server` (its own token server)
/// against their issue key, keeps them in `store` under their drive and then takes the
/// checkout off the list - its claim secret and issue key with it. See [`Finished`].
#[must_use]
pub fn finish(
    server: &TokenServer<'_>,
    shared: &SharedKeyring,
    store: &PeriodTokenStore,
    checkout: &PendingCheckout,
    owed: &PendingTokens,
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
    let done = |finished: Finished| match remove(shared, id) {
        Ok(_) => finished,
        Err(e) => Finished::Kept(format!("it could not be taken off the list: {e}")),
    };
    match issue_tokens(server, &owed.grant(id), &checkout.tier) {
        Ok(tokens) => match store.add(&owed.drive_id, &tokens) {
            Ok(_) => done(Finished::Issued {
                drive_id: owed.drive_id.clone(),
                count: tokens.len(),
            }),
            Err(e) => Finished::Kept(format!(
                "the {} period tokens could not be kept: {e}",
                tokens.len()
            )),
        },
        Err(TokenError::Refused { status, code, .. }) if code == "already_issued" => {
            done(Finished::Dropped(format!(
                "its period tokens were issued before (HTTP {status}): this device has none of \
                 them - support can help"
            )))
        }
        Err(TokenError::Refused { status: 403, .. }) => done(Finished::Dropped(String::from(
            "the token server takes no issue key of this checkout (one from before period \
             tokens): support can help with its months",
        ))),
        Err(TokenError::Refused { code, message, .. }) if code == "mandate_stopped" => {
            done(Finished::Dropped(message))
        }
        Err(TokenError::Config(why)) => done(Finished::Dropped(why)),
        Err(TokenError::Connect(why)) => {
            Finished::Kept(format!("no answer from the token server: {why}"))
        }
        Err(e) => Finished::Kept(e.to_string()),
    }
}
