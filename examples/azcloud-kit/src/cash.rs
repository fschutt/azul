//! Cash by post (cash contract v1) on the app's side: the two codes a cash checkout puts on
//! paper.
//!
//! - The ACTIVATION CODE (`AZC1-...`, the token server's, azlin-token's cash.rs): the base32
//!   (RFC 4648, upper case, no padding, blocks of four joined by `-`) of the checkout id's 16
//!   random bytes (the base32 behind `ck_`, not the id's ASCII), the amount in cents (u32, big
//!   endian), the currency (three ASCII letters) and the first ten bytes of HMAC-SHA256(the
//!   server's cash key, what comes before): 33 bytes, 53 characters. It goes on the slip posted with the
//!   cash; the operator's AzCtl reads it back and checks the MAC. The app checks that the code
//!   it prints is its checkout's, for its amount and currency ([`ActivationCode::check`]): a
//!   slip for another amount would have the operator take the wrong money.
//! - The CLAIM CODE (`AZK1-...`, the app's): what another computer needs to pick the drive up
//!   when this one is lost - the checkout id and the claim key's secret, nothing else. Its
//!   bytes: the id's length (one byte), the id (ASCII), the 32 bytes of the secret, then four
//!   bytes of SHA-256(`AZK1` || what comes before) against a mistyped code; written like the
//!   activation code. On the buyer's copy as text and as a QR code.
//!
//!   Whoever reads the claim code learns the checkout id and can open its sealed sign-up while
//!   the token server keeps it (30 days after the payment): the drive's id, its bucket, its
//!   temporary S3 credentials, its first drive token, its period tokens' issue key and its claim
//!   ticket - with which a computer claims a token family of its own (`POST /v1/drives/{id}/
//!   claim`, three pick-ups within 30 days of the first): the drive, as a device of its owner.
//!   Before the payment and after the 30 days it shows the checkout's status only.

use std::fmt;

use azul_storage::base32;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{claim::ClaimKey, pending::PendingCheckout};

/// What every activation code starts with (its version).
pub const ACTIVATION_PREFIX: &str = "AZC1-";
/// What every claim code starts with (its version).
pub const CLAIM_CODE_PREFIX: &str = "AZK1-";
/// The bytes of the activation code's MAC.
pub const ACTIVATION_MAC_LEN: usize = 10;
/// The payment method of a cash checkout (`POST /v1/checkout {"method": "cash"}`).
pub const CASH_METHOD: &str = "cash";
/// The bytes of a claim secret.
const SECRET_LEN: usize = 32;
/// The random bytes behind a checkout id's `ck_` (26 base32 characters).
const ID_BYTES: usize = 16;
/// The bytes of an activation code: the id's, the amount, the currency, the MAC.
const ACTIVATION_BYTES: usize = ID_BYTES + 4 + 3 + ACTIVATION_MAC_LEN;

/// The 16 bytes behind `ck_<26 base32>` (the token server's ids), written as the server writes
/// them (lower case); `None` for an id of another form.
fn id_bytes(checkout_id: &str) -> Option<[u8; ID_BYTES]> {
    let text = checkout_id.trim().strip_prefix("ck_")?;
    let bytes: [u8; ID_BYTES] = base32::decode(text)?.try_into().ok()?;
    (base32::encode(&bytes).to_ascii_lowercase() == text).then_some(bytes)
}

/// The checkout id of its 16 bytes: `ck_` and their base32 in lower case.
fn id_of(bytes: &[u8]) -> String {
    format!("ck_{}", base32::encode(bytes).to_ascii_lowercase())
}
/// The bytes of a claim code's check value.
const CHECK_LEN: usize = 4;

/// Why a code is not what it should be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CashError {
    /// Not a code of this kind and version: what it is not.
    Malformed(&'static str),
    /// A code of another checkout, amount or currency: what differs.
    Mismatch(String),
}

impl fmt::Display for CashError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CashError::Malformed(what) => write!(f, "this is not {what}"),
            CashError::Mismatch(what) => f.write_str(what),
        }
    }
}

impl std::error::Error for CashError {}

/// The bytes of a code `text` that starts with `prefix` (any case, blanks, dashes skipped).
fn code_bytes(text: &str, prefix: &str, what: &'static str) -> Result<Vec<u8>, CashError> {
    let compact: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect::<String>()
        .to_ascii_uppercase();
    let version = prefix.trim_end_matches('-');
    let rest = compact
        .strip_prefix(version)
        .ok_or(CashError::Malformed(what))?;
    let bytes = base32::decode(rest).ok_or(CashError::Malformed(what))?;
    // Only the code as it was written: a changed bit after the last byte is a typo too.
    if base32::encode(&bytes) != rest {
        return Err(CashError::Malformed(what));
    }
    Ok(bytes)
}

/// `bytes` as a code: `prefix` and their base32 in blocks of four.
fn code_text(prefix: &str, bytes: &[u8]) -> String {
    format!("{prefix}{}", base32::grouped(&base32::encode(bytes)))
}

/// A cash checkout's activation code, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationCode {
    pub checkout_id: String,
    pub amount_cents: u32,
    /// `EUR`.
    pub currency: String,
    /// The token server's MAC ([`ACTIVATION_MAC_LEN`] bytes): the operator's to check.
    pub mac: Vec<u8>,
}

impl ActivationCode {
    /// Reads `text` (any case; blanks and dashes do not matter).
    ///
    /// # Errors
    ///
    /// [`CashError::Malformed`] for anything but an `AZC1` code of 33 bytes: a checkout id's 16,
    /// an amount, a currency and a MAC.
    pub fn parse(text: &str) -> Result<ActivationCode, CashError> {
        const WHAT: &str = "an activation code (AZC1-...)";
        let bytes = code_bytes(text, ACTIVATION_PREFIX, WHAT)?;
        if bytes.len() != ACTIVATION_BYTES {
            return Err(CashError::Malformed(WHAT));
        }
        let (id, rest) = bytes.split_at(ID_BYTES);
        let (amount, rest) = rest.split_at(4);
        let (currency, mac) = rest.split_at(3);
        let currency = std::str::from_utf8(currency).map_err(|_| CashError::Malformed(WHAT))?;
        if !currency.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(CashError::Malformed(WHAT));
        }
        Ok(ActivationCode {
            checkout_id: id_of(id),
            amount_cents: u32::from_be_bytes([amount[0], amount[1], amount[2], amount[3]]),
            currency: currency.to_string(),
            mac: mac.to_vec(),
        })
    }

    /// The code as the token server writes it (`AZC1-` and blocks of four).
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut bytes = id_bytes(&self.checkout_id).unwrap_or_default().to_vec();
        bytes.extend_from_slice(&self.amount_cents.to_be_bytes());
        bytes.extend_from_slice(self.currency.as_bytes());
        bytes.extend_from_slice(&self.mac);
        code_text(ACTIVATION_PREFIX, &bytes)
    }

    /// Whether this is the code of the checkout `checkout_id` for `amount_cents` of
    /// `currency`.
    ///
    /// # Errors
    ///
    /// [`CashError::Mismatch`]: what differs.
    pub fn check(
        &self,
        checkout_id: &str,
        amount_cents: u64,
        currency: &str,
    ) -> Result<(), CashError> {
        if self.checkout_id != checkout_id.trim() {
            return Err(CashError::Mismatch(String::from(
                "the activation code is another checkout's",
            )));
        }
        if u64::from(self.amount_cents) != amount_cents {
            return Err(CashError::Mismatch(format!(
                "the activation code is for {} cents, the checkout for {amount_cents}",
                self.amount_cents
            )));
        }
        if self.currency != currency.trim() {
            return Err(CashError::Mismatch(format!(
                "the activation code is in {}, the checkout in {}",
                self.currency,
                currency.trim()
            )));
        }
        Ok(())
    }
}

/// The check value of a claim code's bytes.
fn claim_check(payload: &[u8]) -> [u8; CHECK_LEN] {
    let mut hash = Sha256::new();
    hash.update(CLAIM_CODE_PREFIX.trim_end_matches('-').as_bytes());
    hash.update(payload);
    let digest = hash.finalize();
    [digest[0], digest[1], digest[2], digest[3]]
}

/// What another computer needs to pick up a cash checkout's drive: the checkout id and its
/// claim key. `Debug` shows the checkout id only.
#[derive(Clone)]
pub struct ClaimCode {
    pub checkout_id: String,
    pub claim: ClaimKey,
}

impl fmt::Debug for ClaimCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClaimCode")
            .field("checkout_id", &self.checkout_id)
            .field("claim", &"<hidden>")
            .finish()
    }
}

impl ClaimCode {
    /// The code of the checkout `checkout_id` with the claim key `claim`.
    #[must_use]
    pub fn new(checkout_id: &str, claim: &ClaimKey) -> ClaimCode {
        ClaimCode {
            checkout_id: checkout_id.trim().to_string(),
            claim: claim.clone(),
        }
    }

    /// The code of the kept checkout `checkout`.
    ///
    /// # Errors
    ///
    /// When its claim secret is no claim key.
    pub fn of(checkout: &PendingCheckout) -> Result<ClaimCode, CashError> {
        let claim = checkout
            .claim_key()
            .map_err(|_| CashError::Malformed("a kept claim key"))?;
        Ok(ClaimCode::new(&checkout.checkout_id, &claim))
    }

    /// The code as the buyer's copy prints it (`AZK1-` and blocks of four). A secret: never
    /// print it anywhere else.
    #[must_use]
    pub fn to_text(&self) -> Zeroizing<String> {
        let id = self.checkout_id.as_bytes();
        let mut payload =
            Zeroizing::new(Vec::with_capacity(1 + id.len() + SECRET_LEN + CHECK_LEN));
        payload.push(u8::try_from(id.len()).unwrap_or(u8::MAX));
        payload.extend_from_slice(id);
        payload.extend_from_slice(&self.claim.secret_bytes()[..]);
        let check = claim_check(&payload);
        payload.extend_from_slice(&check);
        Zeroizing::new(code_text(CLAIM_CODE_PREFIX, &payload))
    }

    /// Reads a claim code as typed or scanned (any case; blanks and dashes do not matter).
    ///
    /// # Errors
    ///
    /// [`CashError::Malformed`] for anything but a whole `AZK1` code whose check value holds (a
    /// mistyped one is refused, never read as another checkout).
    pub fn parse(text: &str) -> Result<ClaimCode, CashError> {
        const WHAT: &str = "a claim code (AZK1-...)";
        let bytes = Zeroizing::new(code_bytes(text, CLAIM_CODE_PREFIX, WHAT)?);
        let Some((&id_len, rest)) = bytes.split_first() else {
            return Err(CashError::Malformed(WHAT));
        };
        let id_len = usize::from(id_len);
        if id_len == 0 || rest.len() != id_len + SECRET_LEN + CHECK_LEN {
            return Err(CashError::Malformed(WHAT));
        }
        let (payload, check) = bytes.split_at(bytes.len() - CHECK_LEN);
        if claim_check(payload).as_slice() != check {
            return Err(CashError::Malformed(WHAT));
        }
        let id = &rest[..id_len];
        let checkout_id = std::str::from_utf8(id).map_err(|_| CashError::Malformed(WHAT))?;
        if !checkout_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(CashError::Malformed(WHAT));
        }
        let mut secret = Zeroizing::new([0u8; SECRET_LEN]);
        secret.copy_from_slice(&rest[id_len..id_len + SECRET_LEN]);
        Ok(ClaimCode {
            checkout_id: checkout_id.to_string(),
            claim: ClaimKey::from_secret_bytes(*secret),
        })
    }

    /// The unfinished cash checkout this code picks up on this computer: asked at `token_url`
    /// (this run's token server: the code does not name one), the drive to be called `name`;
    /// its tier comes with the sealed sign-up.
    #[must_use]
    pub fn pending(&self, token_url: &str, name: &str) -> PendingCheckout {
        let mut kept = PendingCheckout::new(&self.checkout_id, &self.claim, "", token_url, name);
        kept.method = CASH_METHOD.to_string();
        // Its drive is claimed as a token family of this computer's own.
        kept.picked_up = true;
        kept
    }
}
