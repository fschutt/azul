//! The checkouts this device started and has no drive of yet: ONE keyring entry
//! ([`PENDING_KEY`]) holding them all - each one's id, the secret of its claim key, its tier,
//! when it started, the token server it was made at and the name typed for the drive - changed
//! only under the lock of that entry, so two windows (two processes) never lose each other's.
//! The entry outlives the app: a drive paid after "Stop waiting", or while the app was closed,
//! still reaches it.
//!
//! An app polls them while its dialog waits, at every start and in the background after "Stop
//! waiting" ([`poll`]): an approved checkout becomes the drive - its session goes into the
//! keyring under the drive's lock unless the keyring has it already (idempotent by drive id: a
//! session that rotated since is never replaced by the sign-up's spent token) - and the app takes
//! the checkout off the list ([`remove`]) once the drive is in its drives file; a checkout whose
//! payment was declined, or that the token server no longer has, is taken off at once - and said
//! by the one poll that took it off.
//!
//! Blocking (the keyring, the token server): call it from an azul `Thread`.

use std::fmt;

use azul_storage::config::keyring_key;
use serde::{Deserialize, Serialize};

use crate::{
    bundle::DriveBundle,
    claim::{ClaimError, ClaimKey},
    error::{fail, CloudError, CloudResult},
    session::AzlinSession,
    shared::SharedKeyring,
    token::{CheckoutStatus, TokenError, TokenServer},
};

/// The keyring entry of the unfinished checkouts.
pub const PENDING_KEY: &str = "azcloud/checkouts";
/// The `format` of its text.
pub const PENDING_FORMAT: &str = "azcloud.checkouts";

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
            .finish()
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
            name: name.trim().to_string(),
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

fn write(shared: &SharedKeyring, checkouts: Vec<PendingCheckout>) -> CloudResult<()> {
    if checkouts.is_empty() {
        return shared.delete(PENDING_KEY);
    }
    let file = PendingFile {
        format: PENDING_FORMAT.to_string(),
        version: 1,
        checkouts,
    };
    let text = serde_json::to_string(&file)
        .map_err(|_| CloudError::failed("the list of unfinished checkouts cannot be written"))?;
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

/// Adds `checkout` to the list (replacing one with its id).
///
/// # Errors
///
/// When the list cannot be read or written.
pub fn add(shared: &SharedKeyring, checkout: &PendingCheckout) -> CloudResult<()> {
    let _lock = shared.lock(PENDING_KEY)?;
    let mut checkouts = read(shared)?;
    checkouts.retain(|c| c.checkout_id != checkout.checkout_id);
    checkouts.push(checkout.clone());
    write(shared, checkouts)
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
pub fn poll(server: &TokenServer<'_>, shared: &SharedKeyring, checkout: &PendingCheckout) -> Polled {
    let claim = match checkout.claim_key() {
        Ok(claim) => claim,
        // Its sealed sign-up can never be opened.
        Err(e) => return drop_it(shared, checkout, format!("its claim key cannot be read ({e})")),
    };
    match server.checkout_status(&checkout.checkout_id, &claim) {
        Ok(CheckoutStatus::Pending) => Polled::Pending,
        Ok(CheckoutStatus::Approved(bundle)) => match take_drive(shared, &bundle) {
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

/// The session of `bundle`'s drive into the keyring - unless the keyring has one of the drive
/// already, which stays (it may have rotated: the sign-up's token is spent then). Under the
/// drive's lock, as every refresh of the drive is. The keyring's text and whether it was there.
fn take_drive(shared: &SharedKeyring, bundle: &DriveBundle) -> CloudResult<(String, bool)> {
    let key = keyring_key(bundle.drive_id());
    let _lock = shared.lock(&key)?;
    if let Some(text) = shared.get(&key)? {
        let ours = AzlinSession::from_keyring_secret(&text)
            .is_ok_and(|session| session.drive_id == bundle.drive_id());
        if ours {
            return Ok((text, true));
        }
    }
    let text = bundle.session().to_keyring_secret();
    shared.set(&key, &text)?;
    Ok((text, false))
}
