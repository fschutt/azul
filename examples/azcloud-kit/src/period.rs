//! A paid checkout's period tokens (AZLINSEC17 F24): the months it pays for, as blind-signed
//! tokens the drive redeems one per month - the token server signs them without seeing them,
//! so a redemption names no payment. A prepay's months are issued one a month (the contract's
//! `IssueSchedule`), so a refund stops the months not issued yet.
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
//!    sign-up's issue key and the id of the key it was blinded for ([`issue_next`]); the
//!    request is kept ([`IssueRequest`], in the drive's vaults) before it is sent: the token
//!    server answers the same request with the same signatures, counted once, so a lost answer
//!    is asked for again - by any device of the drive;
//! 3. finalizes each blind signature `z` into `z * r^-1 mod n` and checks it as an RSASSA-PSS
//!    signature of `randomizer || message` ([`Issuer::finalize`]);
//! 4. keeps the token in the drive's vaults ([`crate::vault`]: this computer's and the
//!    drive's own) until `POST /v1/drives/{id}/redeem` takes it
//!    ([`TokenServer::redeem_period_token`]) - a week before the period ends, from whichever
//!    device looks first ([`look_at_drive`]).
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

use azlin_contract::{IssueRefusal, IssueSchedule};

use crate::{
    error::{CloudError, CloudResult, Context},
    shared::SharedKeyring,
    state::{create_private_dir, read_json, write_json},
    token::{check_id, DriveStatus, IssueAnswer, TokenError, TokenServer, MAX_BLINDED},
    vault::{DriveTokens, LocalVault, TokenVault, VaultGrant, VaultState},
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
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
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

    /// What names the request across devices: a hash of its checkout, its key and its blinded
    /// messages in order - what the token server keeps its answer by.
    #[must_use]
    pub fn id(&self) -> String {
        let what = format!("{}\n{}\n{}", self.checkout_id, self.key_id, self.messages().join(","));
        hex(blake3::hash(what.as_bytes()).as_bytes())
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

/// What [`issue_next`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Issued {
    /// The next month's token, checked and kept in the drive's vaults (with the grant's new
    /// schedule).
    Tokens(Vec<PeriodToken>),
    /// Nothing is due: the grant's next month comes from `next_issue_at` on (seconds since 1970;
    /// `None`: every month is issued, or the grant ended).
    NotDue { next_issue_at: Option<u64> },
    /// The token server will never issue the rest (a reversed payment, a wrong issue key, every
    /// month issued elsewhere): why, to be said once. The grant is marked ended in the vaults.
    Ended(String),
}

/// What a final refusal of an issue means for the months not issued yet, in the user's words.
#[must_use]
pub fn ended_words(refusal: &IssueRefusal) -> String {
    match refusal {
        IssueRefusal::NotPaid => String::from(
            "its payment was taken back (a refund or a chargeback): the months not issued yet \
             end with it",
        ),
        IssueRefusal::MandateStopped => String::from(
            "its payment mandate was stopped after payments went back: no more months come from it",
        ),
        IssueRefusal::AlreadyIssued => String::from(
            "every month it paid for was issued already, to a request this computer does not \
             hold - support can help",
        ),
        IssueRefusal::IssueKeyWrong => String::from(
            "the token server takes no issue key of this checkout (one from before period \
             tokens): support can help with its months",
        ),
        IssueRefusal::NoSuchCheckout => {
            String::from("the token server no longer knows its checkout: support can help")
        }
        other => format!(
            "the token server refuses its months ({}): support can help",
            other.as_str()
        ),
    }
}

/// The next month of the grant of `checkout_id` kept in `tokens` (its drive's vaults), issued
/// at `server` and kept there: an issue request of the grant kept in a vault - by this device or
/// another one whose answer was lost, however long ago - is sent again as it is first (the
/// token server answers it with the same signatures, counted once); else, when the grant's next
/// month is due at `now`, one token message is blinded for the tier's issuer key, the request
/// kept in the vaults BEFORE it is sent, and sent. A key that changed since the keys were read
/// (`key_changed`, the year turned) gets the message blinded anew; `not_due` moves the grant's
/// schedule; a final refusal ends the grant ([`Issued::Ended`]).
///
/// # Errors
///
/// No answer, a busy server, an answer that does not verify (the request stays kept for the
/// next try); the vaults could not keep the request or the tokens ([`TokenError::Config`]);
/// no such grant, or no issuer key named for its tier ([`TokenError::Protocol`]).
pub fn issue_next(
    server: &TokenServer<'_>,
    tokens: &DriveTokens,
    checkout_id: &str,
    now: u64,
) -> Result<Issued, TokenError> {
    let state = tokens.state().map_err(not_kept)?;
    let Some(grant) = state.grant(checkout_id).cloned() else {
        return Err(TokenError::Protocol(format!(
            "the drive {} keeps no grant of the checkout {checkout_id}",
            tokens.drive_id()
        )));
    };
    if let Some(why) = &grant.ended {
        return Ok(Issued::Ended(why.clone()));
    }
    // A request kept before its answer came: sent again as it is, whatever the schedule says.
    for kept in state.open_issues(checkout_id) {
        match send_kept(server, tokens, &grant, kept)? {
            Sent::Tokens(tokens) => return Ok(Issued::Tokens(tokens)),
            Sent::Ended(why) => return Ok(Issued::Ended(why)),
            // never signed (not due, a key that changed): given up, a fresh one below
            Sent::NotDue | Sent::KeyChanged { .. } => {}
        }
    }
    let grant = tokens.state().map_err(not_kept)?.grant(checkout_id).cloned().unwrap_or(grant);
    if !grant.may_ask(now) {
        return Ok(Issued::NotDue {
            next_issue_at: grant.is_open().then_some(grant.next_issue_at).flatten(),
        });
    }
    let mut key = issuer_key(server, &grant.tier, None)?;
    for _ in 0..KEY_CHANGES {
        let request = IssueRequest::new(&key, checkout_id, 1)?;
        let keep = VaultState {
            issues: vec![request.clone()],
            ..VaultState::default()
        };
        tokens.join(&keep).map_err(not_kept)?;
        let answer = server.issue_period_tokens(
            checkout_id,
            &grant.issue_key,
            request.key_id(),
            &request.messages(),
        )?;
        match answered(tokens, &grant, &request, answer)? {
            Sent::Tokens(issued) => return Ok(Issued::Tokens(issued)),
            Sent::Ended(why) => return Ok(Issued::Ended(why)),
            Sent::NotDue => {
                let moved = tokens.state().map_err(not_kept)?;
                let moved = moved.grant(checkout_id).unwrap_or(&grant);
                return Ok(Issued::NotDue {
                    next_issue_at: moved.is_open().then_some(moved.next_issue_at).flatten(),
                });
            }
            Sent::KeyChanged { key_id, key: named } => {
                key = match named {
                    Some(named) => named,
                    None => issuer_key(server, &grant.tier, Some(&key_id))?,
                };
            }
        }
    }
    Err(TokenError::Protocol(format!(
        "the token server's issuer key for {} changed {KEY_CHANGES} times in one issue",
        grant.tier
    )))
}

/// What one sent issue request came to.
enum Sent {
    Tokens(Vec<PeriodToken>),
    Ended(String),
    /// Not signed, nothing counted: the month is not due (the grant's schedule moved on).
    NotDue,
    /// Not signed: the messages must be blinded for the key `key_id` (`key`: with its public
    /// key, when the token server sent it).
    KeyChanged { key_id: String, key: Option<IssuerKey> },
}

/// The kept issue request `kept` of `grant` sent again as it is.
fn send_kept(
    server: &TokenServer<'_>,
    tokens: &DriveTokens,
    grant: &VaultGrant,
    kept: &IssueRequest,
) -> Result<Sent, TokenError> {
    let answer =
        server.issue_period_tokens(&grant.checkout_id, &grant.issue_key, kept.key_id(), &kept.messages())?;
    answered(tokens, grant, kept, answer)
}

/// Acts on `answer` to `request` of `grant`: the tokens finalized, checked and kept with the
/// grant's new schedule and the request marked done; a refusal recorded (`not_due` moves the
/// schedule, a final one ends the grant, an unsigned request is marked done); a busy server
/// keeps the request for the next try (an error).
fn answered(
    tokens: &DriveTokens,
    grant: &VaultGrant,
    request: &IssueRequest,
    answer: IssueAnswer,
) -> Result<Sent, TokenError> {
    let done = |grant: Option<VaultGrant>| VaultState {
        grants: grant.into_iter().collect(),
        done: vec![request.id()],
        ..VaultState::default()
    };
    let scheduled = |schedule: &IssueSchedule| {
        let mut moved = grant.clone();
        moved.issued = schedule.issued;
        moved.months = schedule.months;
        moved.next_issue_at = schedule
            .next_issue_at
            .as_ref()
            .and_then(|at| u64::try_from(at.unix()).ok());
        moved
    };
    match answer {
        IssueAnswer::Signed(signed) => {
            if signed.key_id != request.key_id() {
                return Err(TokenError::Protocol(format!(
                    "the token server signed the period tokens with the issuer key {}, not with \
                     {} they were blinded for",
                    signed.key_id,
                    request.key_id()
                )));
            }
            let issued = request.finalize(&signed.signatures)?;
            let mut change = done(Some(scheduled(&signed.schedule)));
            change.tokens.clone_from(&issued);
            tokens.join(&change).map_err(not_kept)?;
            Ok(Sent::Tokens(issued))
        }
        IssueAnswer::Refused(IssueRefusal::NotDue { schedule }) => {
            tokens.join(&done(Some(scheduled(&schedule)))).map_err(not_kept)?;
            Ok(Sent::NotDue)
        }
        IssueAnswer::Refused(IssueRefusal::KeyChanged {
            key_id,
            public_key_pem,
        }) => {
            tokens.join(&done(None)).map_err(not_kept)?;
            let key = public_key_pem
                .map(|pem| issuer_key_named(&grant.tier, &key_id, pem))
                .transpose()?;
            Ok(Sent::KeyChanged { key_id, key })
        }
        IssueAnswer::Refused(refusal) if refusal.is_final() => {
            let why = ended_words(&refusal);
            let mut ended = grant.clone();
            ended.ended = Some(why.clone());
            tokens.join(&done(Some(ended))).map_err(not_kept)?;
            Ok(Sent::Ended(why))
        }
        IssueAnswer::Refused(refusal) => Err(TokenError::Refused {
            status: refusal.status(),
            code: refusal.as_str().to_string(),
            message: String::from("the token server could not issue the month now"),
        }),
    }
}

/// A request or tokens the vaults did not keep.
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

/// The token server's refusals of a token that can never buy a month: it was redeemed before
/// (by another drive), it does not verify, it is too old, or its issuer key is gone. It is
/// spent in the vaults.
const USELESS: [&str; 4] = ["token_used", "bad_token", "token_expired", "unknown_issuer"];

/// The most tokens one look redeems and the most months it issues: more than any period needs
/// at once (a lapsed drive is bought a month from tomorrow, one token at a time).
const MAX_ROUNDS: usize = 2 * MAX_BLINDED + 2;

/// What [`redeem_due`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Redeemed {
    /// No period token of the drive's tier is kept for it, and no grant has a month due.
    Nothing,
    /// The period ends more than [`REDEEM_AHEAD_SECS`] from now (or the token server named no
    /// end, in seconds since 1970): the tokens wait.
    NotDue { period_until: Option<u64> },
    /// `count` tokens bought a month each (used or useless ones were spent on the way); the
    /// period now ends at `period_until`.
    Extended {
        count: usize,
        period_until: Option<u64>,
    },
    /// The drive is banned: it cannot use a month, so none is bought (the tokens stay).
    Banned,
    /// Nothing bought this time: why (no answer, a refusal, no session of the drive, the
    /// vaults). The tokens stay for the next try.
    Kept(String),
}

/// The drive is in good standing as its status says: the only state a month is bought in. A
/// banned drive has none ([`Active::of`]).
#[derive(Debug, Clone, Copy)]
pub struct Active<'a> {
    status: &'a DriveStatus,
}

impl<'a> Active<'a> {
    /// `status` when the drive is not banned.
    #[must_use]
    pub fn of(status: &'a DriveStatus) -> Option<Active<'a>> {
        status.ban.is_none().then_some(Active { status })
    }

    /// Its tier (`100GB`; empty when the token server named none).
    #[must_use]
    pub fn tier(&self) -> &'a str {
        self.status.tier.as_deref().unwrap_or_default()
    }

    /// Paid until, in seconds since 1970.
    #[must_use]
    pub fn period_until(&self) -> Option<u64> {
        self.status.period_until
    }
}

/// Buys `tokens`' drive its next month when its period nears its end: under the drive's lock,
/// with its newest drive token ([`SharedKeyring::with_drive_token`]), the period's end is asked
/// (`GET /v1/drives/{id}`) and, within [`REDEEM_AHEAD_SECS`] of `now`, the oldest kept token of
/// the drive's tier redeemed (`POST /v1/drives/{id}/redeem`) and spent in the vaults - again
/// until the end is far enough. A drive that holds no token then gets the next month of a grant
/// whose month is due ([`issue_next`]). A token the server calls used or useless is spent and
/// the next one tried. Asks nothing when nothing is kept for the drive.
#[must_use]
pub fn redeem_due(
    server: &TokenServer<'_>,
    shared: &SharedKeyring,
    tokens: &DriveTokens,
    now: u64,
) -> Redeemed {
    let kept = match tokens.state() {
        Ok(kept) => kept,
        Err(e) => return Redeemed::Kept(e.to_string()),
    };
    if !holds_months(&kept) {
        return Redeemed::Nothing;
    }
    let drive_id = tokens.drive_id();
    shared
        .with_drive_token(drive_id, |token| match server.drive_status(drive_id, token) {
            Ok(status) => match Active::of(&status) {
                Some(active) => redeem_with(server, tokens, token, active, now),
                None => Redeemed::Banned,
            },
            Err(e) => Redeemed::Kept(e.to_string()),
        })
        .unwrap_or_else(|e| Redeemed::Kept(e.to_string()))
}

/// Whether a vault holds anything a month can come from: a token, or an open grant.
fn holds_months(state: &VaultState) -> bool {
    !state.tokens.is_empty() || state.grants.iter().any(VaultGrant::is_open)
}

/// The grant a month buys was changed by the token server (a redemption, a voucher): the
/// drive's credentials may be read-only ones from before. [`crate::AzlinDrive::apply`] takes it
/// and refreshes them before the next request; nothing else can make it, so a month bought is
/// never followed by writes the node refuses.
#[derive(Debug, PartialEq, Eq)]
#[must_use = "the drive must refresh its credentials: AzlinDrive::apply"]
pub struct GrantChanged {
    drive_id: String,
}

impl GrantChanged {
    /// The grant of `drive_id` changed (a voucher's answer, a redemption).
    pub(crate) fn of(drive_id: &str) -> GrantChanged {
        GrantChanged {
            drive_id: drive_id.to_string(),
        }
    }

    #[must_use]
    pub fn drive_id(&self) -> &str {
        &self.drive_id
    }
}

/// What a look at a drive found ([`look_at_drive`]).
#[derive(Debug, PartialEq, Eq)]
pub struct Look {
    /// The drive as the token server keeps it (after a redemption: with the new end); `None`
    /// when it could not be asked.
    pub status: Option<DriveStatus>,
    pub redeemed: Redeemed,
    /// The drive's members this device had not seen at its last look (D42: a new device, or
    /// [`crate::token::RECOVERY_MEMBER`] for a use of the recovery code), never itself; none at
    /// the first look, which learns them ([`PeriodTokenStore::see_members`]).
    pub new_members: Vec<String>,
    /// A month was bought: the drive's credentials must follow ([`crate::AzlinDrive::apply`]).
    pub grant_changed: Option<GrantChanged>,
}

/// A look at `tokens`' drive: under the drive's lock with its newest drive token, its status
/// (`GET /v1/drives/{id}`: the period, a pending recovery-key lockdown, the members, a ban)
/// whether or not anything is kept for it, the members it had not seen (kept in `store`), and -
/// with tokens or a grant kept, the period due and the drive in good standing - [`redeem_due`]'s
/// redemption in the same look.
#[must_use]
pub fn look_at_drive(
    server: &TokenServer<'_>,
    shared: &SharedKeyring,
    store: &PeriodTokenStore,
    tokens: &DriveTokens,
    now: u64,
) -> Look {
    let drive_id = tokens.drive_id();
    let kept = tokens.state();
    let looked = shared.with_drive_token(drive_id, |token| {
        let mut status = match server.drive_status(drive_id, token) {
            Ok(status) => status,
            Err(e) => {
                return Look {
                    status: None,
                    redeemed: Redeemed::Kept(e.to_string()),
                    new_members: Vec::new(),
                    grant_changed: None,
                }
            }
        };
        let redeemed = match (&kept, Active::of(&status)) {
            (Err(e), _) => Redeemed::Kept(e.to_string()),
            (Ok(kept), _) if !holds_months(kept) => Redeemed::Nothing,
            (Ok(_), None) => Redeemed::Banned,
            (Ok(_), Some(active)) => redeem_with(server, tokens, token, active, now),
        };
        let grant_changed = match &redeemed {
            Redeemed::Extended { period_until, .. } => {
                if let Some(until) = period_until {
                    status.period_until = Some(*until);
                }
                Some(GrantChanged::of(drive_id))
            }
            _ => None,
        };
        // A members file that cannot be written names them again at the next look.
        let new_members = store.see_members(drive_id, &status).unwrap_or_default();
        Look {
            status: Some(status),
            redeemed,
            new_members,
            grant_changed,
        }
    });
    looked.unwrap_or_else(|e| Look {
        status: None,
        redeemed: Redeemed::Kept(e.to_string()),
        new_members: Vec::new(),
        grant_changed: None,
    })
}

/// [`redeem_due`] with the drive token `token` and the drive in good standing `active`, the
/// drive's lock held: while the period is due, the oldest unspent token of the drive's tier is
/// redeemed; with none left, the next month of a grant of the tier whose month is due is issued
/// first.
fn redeem_with(
    server: &TokenServer<'_>,
    tokens: &DriveTokens,
    token: &str,
    active: Active<'_>,
    now: u64,
) -> Redeemed {
    let drive_id = tokens.drive_id();
    let tier = active.tier();
    let mut period_until = active.period_until();
    let due =
        |until: Option<u64>| until.is_some_and(|at| at <= now.saturating_add(REDEEM_AHEAD_SECS));
    let mut count = 0;
    let mut failed: Option<String> = None;
    for _ in 0..MAX_ROUNDS {
        if !due(period_until) {
            break;
        }
        let state = match tokens.state() {
            Ok(state) => state,
            Err(e) => {
                failed = Some(e.to_string());
                break;
            }
        };
        let candidate = state
            .tokens
            .iter()
            .find(|t| tier.is_empty() || t.tier == tier)
            .cloned();
        let Some(candidate) = candidate else {
            // none kept: the next month of a grant of this tier, when one is due
            match issue_for(server, tokens, &state, tier, now) {
                Ok(true) => continue,
                Ok(false) => break,
                Err(e) => {
                    failed = Some(e.to_string());
                    break;
                }
            }
        };
        match server.redeem_period_token(drive_id, token, &candidate) {
            Ok(until) => {
                // A token not marked spent is answered again (the same month) and spent then.
                let _ = tokens.spend(&candidate.nonce);
                count += 1;
                period_until = until;
            }
            Err(TokenError::Refused { code, .. }) if USELESS.contains(&code.as_str()) => {
                let _ = tokens.spend(&candidate.nonce);
            }
            Err(e) => {
                failed = Some(e.to_string());
                break;
            }
        }
    }
    if count > 0 {
        Redeemed::Extended {
            count,
            period_until,
        }
    } else if let Some(why) = failed {
        Redeemed::Kept(why)
    } else if due(period_until) {
        Redeemed::Nothing
    } else {
        Redeemed::NotDue { period_until }
    }
}

/// Issues the next month of the first grant of `tier` (any tier when the server named none)
/// whose month is due or that has a request waiting: whether a token came.
fn issue_for(
    server: &TokenServer<'_>,
    tokens: &DriveTokens,
    state: &VaultState,
    tier: &str,
    now: u64,
) -> Result<bool, TokenError> {
    let candidates = state.grants.iter().filter(|g| {
        (tier.is_empty() || g.tier == tier)
            && (g.may_ask(now) || (g.ended.is_none() && !state.open_issues(&g.checkout_id).is_empty()))
    });
    for grant in candidates {
        match issue_next(server, tokens, &grant.checkout_id, now)? {
            Issued::Tokens(issued) if !issued.is_empty() => return Ok(true),
            Issued::Tokens(_) | Issued::NotDue { .. } | Issued::Ended(_) => {}
        }
    }
    Ok(false)
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

/// The period tokens a device keeps, in one folder (readable by this user only): each drive's
/// vault on this computer (`<dir>/<drive id>.json`, [`LocalVault`]), the members it saw, and -
/// from an older version - the issue requests it kept per checkout (`<dir>/issues/`). A drive's
/// tokens are reached through [`PeriodTokenStore::for_drive`] (here and in the drive) or, for a
/// drive whose key this device does not hold, [`PeriodTokenStore::on_this_device`].
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

    /// The file of `drive_id`'s vault on this computer; refused for an id that is no drive id
    /// (it would name another path).
    pub fn path_of(&self, drive_id: &str) -> CloudResult<PathBuf> {
        Ok(self.dir.join(format!("{}.json", check_id(drive_id)?)))
    }

    /// `drive_id`'s vault on this computer.
    pub fn local(&self, drive_id: &str) -> CloudResult<LocalVault> {
        Ok(LocalVault::new(&self.dir, self.path_of(drive_id)?, drive_id))
    }

    /// `drive`'s period tokens, kept on this computer and in the drive itself (encrypted with
    /// its key): any device of the drive that holds the key redeems them and issues its next
    /// months, whichever looks first. Tokens this computer kept before are joined into the
    /// drive at the first look.
    ///
    /// # Errors
    ///
    /// An id that is no drive id.
    #[cfg(feature = "encryption")]
    pub fn for_drive(&self, drive: &crate::vault::UnlockedDrive) -> CloudResult<DriveTokens> {
        let local = self.local(drive.drive_id())?;
        let in_drive: std::sync::Arc<dyn TokenVault> =
            std::sync::Arc::new(crate::vault::DriveVault::new(drive));
        Ok(DriveTokens::new(drive.drive_id(), local, Some(in_drive)))
    }

    /// `drive_id`'s period tokens kept on this computer only: for a drive this device holds no
    /// key of (a plain drive, one waiting for its join code). No other device can redeem them -
    /// a drive with its key here goes through [`Self::for_drive`].
    ///
    /// # Errors
    ///
    /// An id that is no drive id.
    pub fn on_this_device(&self, drive_id: &str) -> CloudResult<DriveTokens> {
        Ok(DriveTokens::new(drive_id, self.local(drive_id)?, None))
    }

    /// `drive_id`'s tokens on this computer not spent, oldest first.
    pub fn tokens(&self, drive_id: &str) -> CloudResult<Vec<PeriodToken>> {
        Ok(self.local(drive_id)?.read()?.tokens)
    }

    /// Adds `tokens` to `drive_id`'s on this computer (each nonce once); how many it keeps then.
    pub fn add(&self, drive_id: &str, tokens: &[PeriodToken]) -> CloudResult<usize> {
        let kept = self.local(drive_id)?.join(&VaultState::with_tokens(tokens))?;
        Ok(kept.tokens.len())
    }

    /// Spends `drive_id`'s token with `nonce` on this computer (redeemed, or refused as used);
    /// whether it was kept unspent.
    pub fn remove(&self, drive_id: &str, nonce: &str) -> CloudResult<bool> {
        let local = self.local(drive_id)?;
        let was = local.read()?.tokens.iter().any(|t| t.nonce == nonce);
        local.join(&VaultState::with_spent(nonce))?;
        Ok(was)
    }

    /// Where an older version kept the checkout `checkout_id`'s issue request
    /// (`<dir>/issues/<id>.json`).
    pub fn issue_request_path(&self, checkout_id: &str) -> CloudResult<PathBuf> {
        Ok(self
            .dir
            .join("issues")
            .join(format!("{}.json", check_id(checkout_id)?)))
    }

    /// The issue request an older version kept for the checkout `checkout_id`; none without
    /// one (or of another format). It moves into the drive's vaults when its checkout's grant
    /// does ([`crate::pending::finish`]).
    pub fn issue_request(&self, checkout_id: &str) -> CloudResult<Option<IssueRequest>> {
        let path = self.issue_request_path(checkout_id)?;
        Ok(read_json::<IssueRequest>(&path)?
            .filter(|request| request.format == ISSUE_REQUEST_FORMAT))
    }

    /// Forgets the issue request an older version kept for `checkout_id` (it is in the drive's
    /// vaults now).
    pub fn forget_issue_request(&self, checkout_id: &str) -> CloudResult<()> {
        let path = self.issue_request_path(checkout_id)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e).context(path.display()),
        }
    }

    /// The members of `status` this device had not seen at its last look at `drive_id`, without
    /// itself (`status.you`); none the first time, which learns them. What it saw is kept in
    /// `<dir>/<drive id>.members.json`, so a restart remembers it.
    pub fn see_members(&self, drive_id: &str, status: &DriveStatus) -> CloudResult<Vec<String>> {
        let path = self
            .dir
            .join(format!("{}.members.json", check_id(drive_id)?));
        let seen = read_json::<SeenMembers>(&path)?;
        let new = seen.as_ref().map_or_else(Vec::new, |seen| {
            status
                .members
                .iter()
                .filter(|m| !seen.members.contains(m) && status.you.as_ref() != Some(*m))
                .cloned()
                .collect()
        });
        if seen.is_none_or(|seen| seen.members != status.members) {
            create_private_dir(&self.dir).with_context(|| self.dir.display().to_string())?;
            let members = status.members.clone();
            write_json(&path, &SeenMembers { members }, true)?;
        }
        Ok(new)
    }
}

/// The members a device saw at its last look at a drive.
#[derive(Serialize, Deserialize)]
struct SeenMembers {
    members: Vec<String>,
}
