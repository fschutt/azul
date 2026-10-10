//! A paid checkout's period tokens (AZLINSEC17 F24): the months it pays for, as blind-signed
//! tokens the drive redeems one per month - the token server signs them without seeing them,
//! so a redemption names no payment.
//!
//! RFC 9474 RSABSSA-SHA384-PSS-Randomized, as the token server's `blind-rsa-signatures` does
//! it: one issuer key per tier and year (`GET /v1/tokens/keys`, SPKI PEM, 2048 to 4096 bits).
//! A token's message is `azlin-period-v1:<tier>:<year>:<32 random bytes, hex>`; the app
//!
//! 1. prepares it with a fresh 32-byte randomizer in front (what is signed is
//!    `randomizer || message`) and EMSA-PSS-encodes that (SHA-384, MGF1-SHA-384, a 48-byte
//!    salt) into `m`;
//! 2. blinds `m` with a random `r` coprime to the modulus: `m * r^e mod n`, as many bytes as
//!    the modulus, standard base64 - what `POST /v1/tokens/issue` takes, with the sealed
//!    sign-up's issue key and the id of the key it was blinded for ([`issue_tokens`]); the
//!    request is kept ([`IssueRequest`]) before it is sent: the token server answers the same
//!    request with the same signatures, counted once, so a lost answer is asked for again;
//! 3. finalizes each blind signature `z` into `z * r^-1 mod n` and checks it as an RSASSA-PSS
//!    signature of `randomizer || message` ([`Issuer::finalize`]);
//! 4. keeps the token ([`PeriodTokenStore`], one 0600 file per drive) until
//!    `POST /v1/drives/{id}/redeem` takes it ([`TokenServer::redeem_period_token`]).
//!
//! The token server counts what it signed: tokens that do not verify are months the app does
//! not get (support: a voucher).

use std::{
    fmt::{self, Write as _},
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use num_bigint_dig::ModInverse;
use rsa::{
    pkcs1::DecodeRsaPublicKey,
    pkcs8::DecodePublicKey,
    pss::{Signature, VerifyingKey},
    signature::Verifier,
    traits::PublicKeyParts,
    BigUint, RsaPublicKey,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha384};

use crate::{
    bundle::PeriodTokens,
    error::{CloudError, CloudResult, Context},
    shared::SharedKeyring,
    state::{create_private_dir, read_json, write_json},
    token::{check_id, IssueAnswer, TokenError, TokenServer, MAX_BLINDED},
};

/// What every token message starts with.
pub const TOKEN_PREFIX: &str = "azlin-period-v1";
/// A token's nonce: random bytes, hex in the message.
const NONCE_BYTES: usize = 32;
/// The randomizer in front of the message (RFC 9474's randomized variant).
const RANDOMIZER_BYTES: usize = 32;
/// SHA-384's length: the PSS salt's too.
const HASH_BYTES: usize = 48;

/// The message of a token of `tier` and `year` with the nonce `nonce_hex`.
#[must_use]
pub fn token_message(tier: &str, year: u32, nonce_hex: &str) -> String {
    format!("{TOKEN_PREFIX}:{tier}:{year}:{nonce_hex}")
}

/// An issuer key as `GET /v1/tokens/keys` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuerKey {
    /// `100GB`.
    pub tier: String,
    pub year: u32,
    /// `<tier>/<year>`.
    pub key_id: String,
    /// SPKI PEM.
    pub public_key_pem: String,
}

/// A finished period token: what buys its drive a month, once. Anyone who has it can redeem it
/// for a drive of its tier - `Debug` shows neither the signature nor the randomizer.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeriodToken {
    pub tier: String,
    pub year: u32,
    /// 32 random bytes, hex.
    pub nonce: String,
    /// The RSASSA-PSS signature, standard base64.
    pub signature: String,
    /// The 32-byte randomizer signed in front of the message, standard base64.
    pub randomizer: String,
}

impl PeriodToken {
    /// The token's message ([`token_message`]).
    #[must_use]
    pub fn message(&self) -> String {
        token_message(&self.tier, self.year, &self.nonce)
    }
}

impl fmt::Debug for PeriodToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PeriodToken")
            .field("tier", &self.tier)
            .field("year", &self.year)
            .field("nonce", &self.nonce)
            .field("signature", &"<hidden>")
            .field("randomizer", &"<hidden>")
            .finish()
    }
}

/// A token message blinded for an issuer: what the token server signs, and what the app needs
/// to finalize its signature (the randomizer and `r^-1`, which nobody else may learn: they link
/// the token to this request). `Debug` shows only the blinded message.
pub struct Blinded {
    nonce: String,
    randomizer: [u8; RANDOMIZER_BYTES],
    inverse: BigUint,
    message: String,
}

impl Blinded {
    /// The blinded message, standard base64: one entry of `POST /v1/tokens/issue`'s `blinded`.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Debug for Blinded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Blinded")
            .field("message", &self.message)
            .finish_non_exhaustive()
    }
}

/// The issuer key of a tier and year: blinds token messages for it, finalizes and checks what
/// it signed.
pub struct Issuer {
    tier: String,
    year: u32,
    key: RsaPublicKey,
}

impl fmt::Debug for Issuer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Issuer")
            .field("key_id", &self.key_id())
            .field("bits", &self.key.n().bits())
            .finish()
    }
}

impl Issuer {
    /// The issuer of `tier`'s tokens of `year` with the public key `public_key_pem` (SPKI, or
    /// PKCS#1); refused unless it is an RSA key of 2048 to 4096 bits with the exponent 3 or
    /// 65537 (what the token server's crate accepts).
    pub fn new(tier: &str, year: u32, public_key_pem: &str) -> Result<Issuer, TokenError> {
        let tier = tier.trim();
        if tier.is_empty() {
            return Err(TokenError::Config(String::from(
                "A period token's issuer names its tier.",
            )));
        }
        let pem = public_key_pem.trim();
        let not_a_key = || {
            TokenError::Protocol(format!(
                "the issuer key of {tier}/{year} is no RSA public key of 2048 to 4096 bits"
            ))
        };
        let key = RsaPublicKey::from_public_key_pem(pem)
            .or_else(|_| RsaPublicKey::from_pkcs1_pem(pem))
            .map_err(|_| not_a_key())?;
        let e = key.e();
        let exponent_ok = *e == BigUint::from(3_u32) || *e == BigUint::from(65_537_u32);
        if !(2048..=4096).contains(&(key.size() * 8)) || !exponent_ok {
            return Err(not_a_key());
        }
        Ok(Issuer {
            tier: tier.to_string(),
            year,
            key,
        })
    }

    #[must_use]
    pub fn tier(&self) -> &str {
        &self.tier
    }

    #[must_use]
    pub fn year(&self) -> u32 {
        self.year
    }

    /// `<tier>/<year>`: the key's name at the token server.
    #[must_use]
    pub fn key_id(&self) -> String {
        format!("{}/{}", self.tier, self.year)
    }

    /// A fresh token message (a new nonce), blinded for this key.
    ///
    /// # Errors
    ///
    /// The OS random source failed; an encoding that shares a factor with the modulus (never
    /// for a real key).
    pub fn blind(&self) -> Result<Blinded, TokenError> {
        let nonce = hex(&random::<NONCE_BYTES>()?);
        let randomizer = random::<RANDOMIZER_BYTES>()?;
        let salt = random::<HASH_BYTES>()?;
        let message = token_message(&self.tier, self.year, &nonce);
        let n = self.key.n();
        let encoded = emsa_pss_encode(&prepared_hash(&randomizer, &message), n.bits() - 1, &salt)?;
        let m = BigUint::from_bytes_be(&encoded);
        if inverse(&m, n).is_none() {
            return Err(TokenError::Protocol(String::from(
                "the issuer key's modulus shares a factor with a token: it is no RSA key",
            )));
        }
        let (r, r_inverse) = loop {
            let r = random_below(n)?;
            if let Some(r_inverse) = inverse(&r, n) {
                break (r, r_inverse);
            }
        };
        let blinded = (m * r.modpow(self.key.e(), n)) % n;
        Ok(Blinded {
            nonce,
            randomizer,
            inverse: r_inverse,
            message: STANDARD.encode(pad(&blinded, self.key.size())),
        })
    }

    /// The token `blind_signature` (standard base64, the token server's answer to
    /// `blinded`) finalizes into, checked.
    ///
    /// # Errors
    ///
    /// [`TokenError::Protocol`] for a signature that is no base64, not as long as the modulus,
    /// or does not verify (another key signed it, or another message).
    pub fn finalize(
        &self,
        blinded: &Blinded,
        blind_signature: &str,
    ) -> Result<PeriodToken, TokenError> {
        let bad = |why: &str| {
            TokenError::Protocol(format!(
                "the blind signature of a period token of {} {why}",
                self.key_id()
            ))
        };
        let raw = STANDARD
            .decode(blind_signature.trim())
            .map_err(|_| bad("is not base64"))?;
        let size = self.key.size();
        if raw.len() != size {
            return Err(bad(&format!("has {} bytes, not {size}", raw.len())));
        }
        let n = self.key.n();
        let z = BigUint::from_bytes_be(&raw);
        if z >= *n {
            return Err(bad("is not below the key's modulus"));
        }
        let signature = (z * &blinded.inverse) % n;
        let token = PeriodToken {
            tier: self.tier.clone(),
            year: self.year,
            nonce: blinded.nonce.clone(),
            signature: STANDARD.encode(pad(&signature, size)),
            randomizer: STANDARD.encode(blinded.randomizer),
        };
        self.verify(&token)
            .map_err(|_| bad("does not verify under the issuer key"))?;
        Ok(token)
    }

    /// Whether `token` is this key's signature of its message.
    ///
    /// # Errors
    ///
    /// [`TokenError::Protocol`]: another tier or year, or no valid signature.
    pub fn verify(&self, token: &PeriodToken) -> Result<(), TokenError> {
        if token.tier != self.tier || token.year != self.year {
            return Err(TokenError::Protocol(format!(
                "the period token is one of {}/{}, not of {}",
                token.tier,
                token.year,
                self.key_id()
            )));
        }
        let refused = || {
            TokenError::Protocol(format!(
                "the period token does not verify under the issuer key {}",
                self.key_id()
            ))
        };
        let mut prepared = STANDARD
            .decode(token.randomizer.trim())
            .map_err(|_| refused())?;
        if prepared.len() != RANDOMIZER_BYTES {
            return Err(refused());
        }
        prepared.extend_from_slice(token.message().as_bytes());
        let signature = STANDARD
            .decode(token.signature.trim())
            .map_err(|_| refused())?;
        let signature = Signature::try_from(signature.as_slice()).map_err(|_| refused())?;
        VerifyingKey::<Sha384>::new(self.key.clone())
            .verify(&prepared, &signature)
            .map_err(|_| refused())
    }
}

/// One blinded message of a kept [`IssueRequest`]: everything its finalization needs.
#[derive(Clone, Serialize, Deserialize)]
struct KeptBlinding {
    nonce: String,
    /// Standard base64.
    randomizer: String,
    /// `r^-1 mod n`, big-endian, standard base64.
    inverse: String,
    /// The blinded message as sent.
    message: String,
}

impl Blinded {
    fn kept(&self) -> KeptBlinding {
        KeptBlinding {
            nonce: self.nonce.clone(),
            randomizer: STANDARD.encode(self.randomizer),
            inverse: STANDARD.encode(self.inverse.to_bytes_be()),
            message: self.message.clone(),
        }
    }

    fn from_kept(kept: &KeptBlinding) -> Result<Blinded, TokenError> {
        let unreadable =
            || TokenError::Protocol(String::from("a kept issue request is unreadable"));
        let randomizer: [u8; RANDOMIZER_BYTES] = STANDARD
            .decode(&kept.randomizer)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(unreadable)?;
        let inverse = STANDARD.decode(&kept.inverse).map_err(|_| unreadable())?;
        Ok(Blinded {
            nonce: kept.nonce.clone(),
            randomizer,
            inverse: BigUint::from_bytes_be(&inverse),
            message: kept.message.clone(),
        })
    }
}

/// The format of a kept issue request.
const ISSUE_REQUEST_FORMAT: u32 = 1;

/// A `POST /v1/tokens/issue` request of a checkout - its issuer key and its blinded messages,
/// with what finalizes their signatures - kept (0600, [`PeriodTokenStore`]) from before it is
/// sent until its tokens are: the token server answers the same request (same key, messages and
/// order) with the same signatures, counted once, so a lost answer is asked for again with it
/// (AZDRIVE-INTEGRATION §4). `Debug` shows no blinding.
#[derive(Clone, Serialize, Deserialize)]
pub struct IssueRequest {
    format: u32,
    checkout_id: String,
    tier: String,
    year: u32,
    key_id: String,
    public_key_pem: String,
    blindings: Vec<KeptBlinding>,
}

impl fmt::Debug for IssueRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IssueRequest")
            .field("checkout_id", &self.checkout_id)
            .field("key_id", &self.key_id)
            .field("messages", &self.blindings.len())
            .finish_non_exhaustive()
    }
}

impl IssueRequest {
    /// `months` fresh token messages of the checkout `checkout_id`, blinded for `key`.
    ///
    /// # Errors
    ///
    /// A key that is no RSA public key; the OS random source failed.
    pub fn new(
        key: &IssuerKey,
        checkout_id: &str,
        months: usize,
    ) -> Result<IssueRequest, TokenError> {
        let issuer = Issuer::new(&key.tier, key.year, &key.public_key_pem)?;
        let blindings = (0..months)
            .map(|_| issuer.blind().map(|blinded| blinded.kept()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(IssueRequest {
            format: ISSUE_REQUEST_FORMAT,
            checkout_id: checkout_id.trim().to_string(),
            tier: issuer.tier().to_string(),
            year: key.year,
            key_id: key.key_id.clone(),
            public_key_pem: key.public_key_pem.clone(),
            blindings,
        })
    }

    #[must_use]
    pub fn checkout_id(&self) -> &str {
        &self.checkout_id
    }

    /// The issuer key the messages are blinded for (`<tier>/<year>`).
    #[must_use]
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// The blinded messages, in the order they are sent.
    #[must_use]
    pub fn messages(&self) -> Vec<String> {
        self.blindings.iter().map(|b| b.message.clone()).collect()
    }

    /// The tokens `signatures` (the answer's, in the order of the messages) finalize into,
    /// every one checked.
    ///
    /// # Errors
    ///
    /// [`TokenError::Protocol`]: another number of signatures, or one that does not verify.
    pub fn finalize(&self, signatures: &[String]) -> Result<Vec<PeriodToken>, TokenError> {
        if signatures.len() != self.blindings.len() {
            return Err(TokenError::Protocol(format!(
                "{} blind signatures for {} blinded messages",
                signatures.len(),
                self.blindings.len()
            )));
        }
        let issuer = Issuer::new(&self.tier, self.year, &self.public_key_pem)?;
        self.blindings
            .iter()
            .zip(signatures)
            .map(|(kept, signature)| issuer.finalize(&Blinded::from_kept(kept)?, signature))
            .collect()
    }
}

/// How often one issue follows a changed issuer key before it gives up.
const KEY_CHANGES: usize = 3;

/// The period tokens a paid checkout's sign-up grants (`grant`), for the drive `drive_id` of
/// `tier`, kept in `store` under the drive: the tier's issuer key (`GET /v1/tokens/keys`), one
/// blinded message per month, kept as an [`IssueRequest`] BEFORE it is sent, then
/// `POST /v1/tokens/issue` with the grant's issue key and the key's id, every signature
/// finalized and checked, the tokens kept and the request forgotten. A request kept by an
/// earlier try whose answer was lost is sent again as it is (no new keys, no new blinding): the
/// token server answers it with the same signatures, counted once. A key that changed since the
/// keys were read (`409 key_changed`, the year turned) gets the messages blinded anew.
///
/// # Errors
///
/// [`TokenError::Config`] for no months or more than one call takes ([`MAX_BLINDED`]; nothing
/// is sent), or a request or tokens that could not be kept; no answer (the request stays kept
/// for the next try); the token server's refusals (`issue_key_wrong`, `already_issued`, ...);
/// [`TokenError::Protocol`] when it names no key for the tier, answers with another key than
/// the request named, or with something that does not verify.
pub fn issue_tokens(
    server: &TokenServer<'_>,
    store: &PeriodTokenStore,
    grant: &PeriodTokens,
    tier: &str,
    drive_id: &str,
) -> Result<Vec<PeriodToken>, TokenError> {
    let months = usize::try_from(grant.months).unwrap_or(usize::MAX);
    if months == 0 || months > MAX_BLINDED {
        return Err(TokenError::Config(format!(
            "A checkout's period tokens are 1 to {MAX_BLINDED} months, not {}.",
            grant.months
        )));
    }
    let tier = tier.trim();
    let fresh = |key: &IssuerKey| {
        let request = IssueRequest::new(key, &grant.checkout_id, months)?;
        store.keep_issue_request(&request).map_err(not_kept)?;
        Ok::<IssueRequest, TokenError>(request)
    };
    let mut request = match store.issue_request(&grant.checkout_id) {
        Ok(Some(kept)) if kept.blindings.len() == months && kept.tier == tier => kept,
        _ => fresh(&issuer_key(server, tier, None)?)?,
    };
    for _ in 0..KEY_CHANGES {
        let answer = server.issue_period_tokens(
            &grant.checkout_id,
            &grant.issue_key,
            &request.key_id,
            &request.messages(),
        )?;
        match answer {
            IssueAnswer::Signed(signed) => {
                if signed.key_id != request.key_id {
                    return Err(TokenError::Protocol(format!(
                        "the token server signed the period tokens with the issuer key {}, not \
                         with {} they were blinded for",
                        signed.key_id, request.key_id
                    )));
                }
                let tokens = request.finalize(&signed.signatures)?;
                store.add(drive_id, &tokens).map_err(not_kept)?;
                // A request left behind is harmless: its checkout is done.
                let _ = store.forget_issue_request(&grant.checkout_id);
                return Ok(tokens);
            }
            IssueAnswer::KeyChanged {
                key_id,
                public_key_pem,
            } => {
                let key = match public_key_pem {
                    Some(pem) => issuer_key_named(tier, &key_id, pem)?,
                    None => issuer_key(server, tier, Some(&key_id))?,
                };
                request = fresh(&key)?;
            }
        }
    }
    Err(TokenError::Protocol(format!(
        "the token server's issuer key for {tier} changed {KEY_CHANGES} times in one issue"
    )))
}

/// A request or tokens the store did not keep.
fn not_kept(e: CloudError) -> TokenError {
    TokenError::Config(format!(
        "The period tokens' issue request or tokens could not be kept: {e}"
    ))
}

/// The issuer key of `tier` the token server lists (`key_id`, when one is named).
fn issuer_key(
    server: &TokenServer<'_>,
    tier: &str,
    key_id: Option<&str>,
) -> Result<IssuerKey, TokenError> {
    server
        .issuer_keys()?
        .into_iter()
        .find(|key| key.tier == tier && key_id.is_none_or(|id| key.key_id == id))
        .ok_or_else(|| {
            TokenError::Protocol(format!(
                "the token server names no issuer key {} for {tier}",
                key_id.unwrap_or_default()
            ))
        })
}

/// The issuer key `key_id` (`<tier>/<year>`) of `tier` with `public_key_pem`, as a
/// `key_changed` answer names it.
fn issuer_key_named(
    tier: &str,
    key_id: &str,
    public_key_pem: String,
) -> Result<IssuerKey, TokenError> {
    let year = key_id
        .rsplit_once('/')
        .filter(|(named, _)| *named == tier)
        .and_then(|(_, year)| year.parse::<u32>().ok())
        .ok_or_else(|| {
            TokenError::Protocol(format!(
                "the token server named the issuer key {key_id:?}, which is none of {tier}"
            ))
        })?;
    Ok(IssuerKey {
        tier: tier.to_string(),
        year,
        key_id: key_id.to_string(),
        public_key_pem,
    })
}

// ==== Redeeming ====

/// How long before a drive's period ends its next month is bought: a week (a redemption adds
/// 30 days to the end - to tomorrow's start for a lapsed drive - so nothing is lost by it).
pub const REDEEM_AHEAD_SECS: u64 = 7 * 86_400;

/// The token server's refusals of a token that can never buy a month: it was redeemed before,
/// it does not verify, it is too old, or its issuer key is gone. It is dropped.
const USELESS: [&str; 4] = ["token_used", "bad_token", "token_expired", "unknown_issuer"];

/// What [`redeem_due`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Redeemed {
    /// No period token of the drive's tier is kept for it.
    Nothing,
    /// The period ends more than [`REDEEM_AHEAD_SECS`] from now (or the token server named no
    /// end, in seconds since 1970): the tokens wait.
    NotDue { period_until: Option<u64> },
    /// `count` tokens bought a month each (used or useless ones were dropped on the way); the
    /// period now ends at `period_until`.
    Extended {
        count: usize,
        period_until: Option<u64>,
    },
    /// Nothing bought this time: why (no answer, a refusal, no session of the drive, the store).
    /// The tokens stay for the next try.
    Kept(String),
}

/// Buys `drive_id` its next month when its period nears its end: under the drive's lock, with
/// its newest drive token ([`SharedKeyring::with_drive_token`]), the period's end is asked
/// (`GET /v1/drives/{id}`) and, within [`REDEEM_AHEAD_SECS`] of `now`, the oldest kept token of
/// the drive's tier redeemed (`POST /v1/drives/{id}/redeem`) and taken out of `store` - again
/// until the end is far enough. A token the server calls used or useless is dropped and the
/// next one tried. Asks nothing when no token is kept for the drive.
#[must_use]
pub fn redeem_due(
    server: &TokenServer<'_>,
    shared: &SharedKeyring,
    store: &PeriodTokenStore,
    drive_id: &str,
    now: u64,
) -> Redeemed {
    let kept = match store.tokens(drive_id) {
        Ok(kept) => kept,
        Err(e) => return Redeemed::Kept(e.to_string()),
    };
    if kept.is_empty() {
        return Redeemed::Nothing;
    }
    shared
        .with_drive_token(drive_id, |token| {
            redeem_with(server, store, drive_id, token, &kept, now)
        })
        .unwrap_or_else(|e| Redeemed::Kept(e.to_string()))
}

/// [`redeem_due`] with the drive token `token`, the drive's lock held.
fn redeem_with(
    server: &TokenServer<'_>,
    store: &PeriodTokenStore,
    drive_id: &str,
    token: &str,
    kept: &[PeriodToken],
    now: u64,
) -> Redeemed {
    let info = match server.info(drive_id, token) {
        Ok(info) => info,
        Err(e) => return Redeemed::Kept(e.to_string()),
    };
    let tier = info["tier"].as_str().unwrap_or_default();
    let mut period_until = info["period_until"]
        .as_str()
        .and_then(azul_storage::time::parse_iso8601);
    let due =
        |until: Option<u64>| until.is_some_and(|at| at <= now.saturating_add(REDEEM_AHEAD_SECS));
    let mut count = 0;
    for candidate in kept.iter().filter(|t| tier.is_empty() || t.tier == tier) {
        if !due(period_until) {
            break;
        }
        match server.redeem_period_token(drive_id, token, candidate) {
            Ok(until) => {
                // A token left behind is refused as used next time, and dropped then.
                let _ = store.remove(drive_id, &candidate.nonce);
                count += 1;
                period_until = until;
            }
            Err(TokenError::Refused { code, .. }) if USELESS.contains(&code.as_str()) => {
                let _ = store.remove(drive_id, &candidate.nonce);
            }
            Err(e) if count == 0 => return Redeemed::Kept(e.to_string()),
            Err(_) => break,
        }
    }
    if count > 0 {
        Redeemed::Extended {
            count,
            period_until,
        }
    } else if due(period_until) {
        Redeemed::Nothing
    } else {
        Redeemed::NotDue { period_until }
    }
}

// ==== RFC 9474 / RFC 8017 pieces ====

/// SHA-384 of the prepared message: the randomizer, then the message.
fn prepared_hash(randomizer: &[u8], message: &str) -> [u8; HASH_BYTES] {
    let mut hasher = Sha384::new();
    hasher.update(randomizer);
    hasher.update(message.as_bytes());
    let mut hash = [0_u8; HASH_BYTES];
    hash.copy_from_slice(&hasher.finalize());
    hash
}

/// MGF1 with SHA-384: `len` bytes of mask from `seed`.
fn mgf1(seed: &[u8], len: usize) -> Vec<u8> {
    let mut mask = Vec::with_capacity(len + HASH_BYTES);
    let mut counter: u32 = 0;
    while mask.len() < len {
        let mut hasher = Sha384::new();
        hasher.update(seed);
        hasher.update(counter.to_be_bytes());
        mask.extend_from_slice(&hasher.finalize());
        counter += 1;
    }
    mask.truncate(len);
    mask
}

/// EMSA-PSS-ENCODE (RFC 8017 §9.1.1) of the hash `m_hash` into `em_bits` bits with `salt`.
fn emsa_pss_encode(m_hash: &[u8], em_bits: usize, salt: &[u8]) -> Result<Vec<u8>, TokenError> {
    let em_len = em_bits.div_ceil(8);
    if em_len < HASH_BYTES + salt.len() + 2 {
        return Err(TokenError::Protocol(String::from(
            "the issuer key is too short for a period token",
        )));
    }
    let mut hasher = Sha384::new();
    hasher.update([0_u8; 8]);
    hasher.update(m_hash);
    hasher.update(salt);
    let h = hasher.finalize();
    let db_len = em_len - HASH_BYTES - 1;
    let mut em = vec![0_u8; db_len];
    em[db_len - salt.len() - 1] = 0x01;
    em[db_len - salt.len()..].copy_from_slice(salt);
    for (byte, mask) in em.iter_mut().zip(mgf1(&h, db_len)) {
        *byte ^= mask;
    }
    em[0] &= 0xff_u8 >> (8 * em_len - em_bits);
    em.extend_from_slice(&h);
    em.push(0xbc);
    Ok(em)
}

/// `a^-1 mod n`, when `a` and `n` are coprime.
fn inverse(a: &BigUint, n: &BigUint) -> Option<BigUint> {
    a.mod_inverse(n).and_then(|inverse| inverse.to_biguint())
}

/// A random number in `[1, n)` (32 bytes more than `n` reduced: no measurable bias).
fn random_below(n: &BigUint) -> Result<BigUint, TokenError> {
    loop {
        let mut bytes = vec![0_u8; n.bits().div_ceil(8) + 32];
        getrandom::getrandom(&mut bytes).map_err(|_| no_random())?;
        let r = BigUint::from_bytes_be(&bytes) % n;
        if r.bits() > 0 {
            return Ok(r);
        }
    }
}

fn random<const N: usize>() -> Result<[u8; N], TokenError> {
    let mut bytes = [0_u8; N];
    getrandom::getrandom(&mut bytes).map_err(|_| no_random())?;
    Ok(bytes)
}

fn no_random() -> TokenError {
    TokenError::Config(String::from(
        "This computer's random source does not answer: no period token can be made.",
    ))
}

/// `value` big-endian in `size` bytes.
fn pad(value: &BigUint, size: usize) -> Vec<u8> {
    let bytes = value.to_bytes_be();
    let mut out = vec![0_u8; size.saturating_sub(bytes.len())];
    out.extend_from_slice(&bytes);
    out
}

/// Lowercase hex.
pub(crate) fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

// ==== The tokens a device keeps ====

/// What a drive's file holds.
#[derive(Serialize, Deserialize)]
struct StoredTokens {
    format: u32,
    tokens: Vec<PeriodToken>,
}

/// The format of a drive's file.
const STORE_FORMAT: u32 = 1;

/// The period tokens a device keeps until they are redeemed: one JSON file per drive
/// (`<dir>/<drive id>.json`, readable by this user only, in a folder of its own). Not locked:
/// the caller changes one drive's tokens from one place at a time.
#[derive(Debug, Clone)]
pub struct PeriodTokenStore {
    dir: PathBuf,
}

impl PeriodTokenStore {
    /// The tokens in `dir` (made, 0700 on Unix, on the first write).
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>) -> PeriodTokenStore {
        PeriodTokenStore { dir: dir.into() }
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The file of `drive_id`'s tokens; refused for an id that is no drive id (it would name
    /// another path).
    pub fn path_of(&self, drive_id: &str) -> CloudResult<PathBuf> {
        Ok(self.dir.join(format!("{}.json", check_id(drive_id)?)))
    }

    /// `drive_id`'s tokens, oldest first; none without a file.
    pub fn tokens(&self, drive_id: &str) -> CloudResult<Vec<PeriodToken>> {
        let path = self.path_of(drive_id)?;
        Ok(read_json::<StoredTokens>(&path)?
            .map(|stored| stored.tokens)
            .unwrap_or_default())
    }

    /// Adds `tokens` to `drive_id`'s (each nonce once); how many it keeps then.
    pub fn add(&self, drive_id: &str, tokens: &[PeriodToken]) -> CloudResult<usize> {
        let mut kept = self.tokens(drive_id)?;
        for token in tokens {
            if !kept.iter().any(|k| k.nonce == token.nonce) {
                kept.push(token.clone());
            }
        }
        self.write(drive_id, kept)
    }

    /// Removes `drive_id`'s token with `nonce` (redeemed, or refused as used); whether it was
    /// there. The file goes with the last one.
    pub fn remove(&self, drive_id: &str, nonce: &str) -> CloudResult<bool> {
        let mut kept = self.tokens(drive_id)?;
        let before = kept.len();
        kept.retain(|k| k.nonce != nonce);
        if kept.len() == before {
            return Ok(false);
        }
        self.write(drive_id, kept)?;
        Ok(true)
    }

    fn write(&self, drive_id: &str, tokens: Vec<PeriodToken>) -> CloudResult<usize> {
        let path = self.path_of(drive_id)?;
        if tokens.is_empty() {
            return match fs::remove_file(&path) {
                Ok(()) => Ok(0),
                Err(e) if e.kind() == ErrorKind::NotFound => Ok(0),
                Err(e) => Err(e).context(path.display()),
            };
        }
        create_private_dir(&self.dir).with_context(|| self.dir.display().to_string())?;
        let count = tokens.len();
        let stored = StoredTokens {
            format: STORE_FORMAT,
            tokens,
        };
        write_json(&path, &stored, true)?;
        Ok(count)
    }

    /// The file of the checkout `checkout_id`'s kept issue request (`<dir>/issues/<id>.json`).
    pub fn issue_request_path(&self, checkout_id: &str) -> CloudResult<PathBuf> {
        Ok(self
            .dir
            .join("issues")
            .join(format!("{}.json", check_id(checkout_id)?)))
    }

    /// The checkout `checkout_id`'s kept issue request; none without one (or of another format).
    pub fn issue_request(&self, checkout_id: &str) -> CloudResult<Option<IssueRequest>> {
        let path = self.issue_request_path(checkout_id)?;
        Ok(read_json::<IssueRequest>(&path)?
            .filter(|request| request.format == ISSUE_REQUEST_FORMAT))
    }

    /// Keeps `request` (readable by this user only) until [`Self::forget_issue_request`].
    pub fn keep_issue_request(&self, request: &IssueRequest) -> CloudResult<()> {
        let path = self.issue_request_path(&request.checkout_id)?;
        create_private_dir(&self.dir).with_context(|| self.dir.display().to_string())?;
        if let Some(dir) = path.parent() {
            create_private_dir(dir).with_context(|| dir.display().to_string())?;
        }
        write_json(&path, request, true)
    }

    /// Forgets the checkout `checkout_id`'s kept issue request (its tokens are kept, or it is
    /// done without them).
    pub fn forget_issue_request(&self, checkout_id: &str) -> CloudResult<()> {
        let path = self.issue_request_path(checkout_id)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e).context(path.display()),
        }
    }
}
