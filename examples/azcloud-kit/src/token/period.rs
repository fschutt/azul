//! A paid checkout's period tokens: the issuer keys, the blind issue, the redemption.

use super::*;
use azlin_contract::{IssueAnswer as ContractIssueAnswer, IssueRefusal, IssueSchedule};

/// What `POST /v1/tokens/issue` answers: one blind signature per blinded message, by the
/// issuer key of the tier and year, and where the checkout's instalments stand after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlindSignatures {
    /// `100GB`.
    pub tier: String,
    /// `<tier>/<year>`: the issuer key's name.
    pub key_id: String,
    /// The issuer's public key (SPKI PEM): what the finished tokens verify against.
    pub public_key_pem: String,
    /// Standard base64, in the order of the blinded messages.
    pub signatures: Vec<String>,
    /// The checkout's months, how many are issued, and from when the next may be asked for.
    pub schedule: IssueSchedule,
}

/// The most blinded messages one `POST /v1/tokens/issue` takes (24 months, prepaid).
pub const MAX_BLINDED: usize = 24;

/// What `POST /v1/tokens/issue` answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueAnswer {
    /// The blind signatures, by the key the request named.
    Signed(BlindSignatures),
    /// Refused as the contract words it: `key_changed` (blind the messages again for the key it
    /// names), `not_due` (the next month comes later), a final one ([`IssueRefusal::is_final`]),
    /// a busy server. Nothing was signed or counted.
    Refused(IssueRefusal),
}

impl<'a> TokenServer<'a> {
    /// The period tokens' issuer keys of this year, one per tier (`GET /v1/tokens/keys`): what
    /// a token is blinded for ([`crate::period::Issuer`]). Entries without a tier, a year or a
    /// key are left out.
    pub fn issuer_keys(&self) -> Result<Vec<IssuerKey>, TokenError> {
        let value = self.call(Method::Get, "/v1/tokens/keys", None, None)?;
        let keys = value["keys"].as_array().ok_or_else(|| {
            TokenError::Protocol(String::from("the answer lists no issuer keys"))
        })?;
        Ok(keys
            .iter()
            .filter_map(|key| {
                let tier = key["tier"].as_str()?.trim().to_string();
                let year = u32::try_from(key["year"].as_u64()?).ok()?;
                let public_key_pem = key["public_key_pem"].as_str()?.to_string();
                let key_id = key["key_id"]
                    .as_str()
                    .map_or_else(|| format!("{tier}/{year}"), str::to_string);
                (!tier.is_empty()).then_some(IssuerKey {
                    tier,
                    year,
                    key_id,
                    public_key_pem,
                })
            })
            .collect())
    }

    /// Blind signatures of a paid checkout's period tokens (`POST /v1/tokens/issue`): one per
    /// message of `blinded` (standard base64, at most [`MAX_BLINDED`]), as many as its
    /// instalments due allow (one a month from its approval). Only with `issue_key`, the key its
    /// sealed sign-up carries ([`crate::bundle::PeriodTokens`]): the checkout id alone, which the
    /// payment provider sees, issues nothing. `key_id` names the issuer key the messages were
    /// blinded for (`GET /v1/tokens/keys`). The same request again (same key, same messages,
    /// same order) is answered the same, counted once: what to send when an answer was lost.
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] without an issue key, a key id or messages (nothing is sent); no
    /// answer; an answer that does not read, or whose signatures do not match the messages; a
    /// refusal without the contract's words (a proxy's).
    pub fn issue_period_tokens(
        &self,
        checkout_id: &str,
        issue_key: &str,
        key_id: &str,
        blinded: &[String],
    ) -> Result<IssueAnswer, TokenError> {
        let issue_key = issue_key.trim();
        if issue_key.is_empty() {
            return Err(TokenError::Config(String::from(
                "There is no issue key: a checkout's period tokens are issued only against the \
                 key its sealed sign-up carries.",
            )));
        }
        let key_id = key_id.trim();
        if key_id.is_empty() {
            return Err(TokenError::Config(String::from(
                "Period tokens name the issuer key they are blinded for.",
            )));
        }
        if blinded.is_empty() || blinded.len() > MAX_BLINDED {
            return Err(TokenError::Config(format!(
                "1 to {MAX_BLINDED} blinded messages, not {}",
                blinded.len()
            )));
        }
        let body = json!({
            "checkout_id": checkout_id.trim(),
            "issue_key": issue_key,
            "key_id": key_id,
            "blinded": blinded,
        });
        let reply = self.exchange(Method::Post, "/v1/tokens/issue", None, Some(&body))?;
        if !reply.is_success() {
            return match serde_json::from_slice::<IssueRefusal>(&reply.body) {
                Ok(refusal) => Ok(IssueAnswer::Refused(refusal)),
                Err(_) => Err(refusal(&reply, false)),
            };
        }
        let answer: ContractIssueAnswer = serde_json::from_slice(&reply.body).map_err(|e| {
            TokenError::Protocol(format!("the issue's answer does not read: {e}"))
        })?;
        if answer.blind_signatures.len() != blinded.len() {
            return Err(TokenError::Protocol(format!(
                "{} blind signatures for {} blinded messages",
                answer.blind_signatures.len(),
                blinded.len()
            )));
        }
        Ok(IssueAnswer::Signed(BlindSignatures {
            tier: answer.tier,
            key_id: answer.key_id,
            public_key_pem: answer.public_key_pem.unwrap_or_default(),
            signatures: answer.blind_signatures,
            schedule: answer.schedule,
        }))
    }

    /// One more month for `drive_id` paid with `token` (`POST /v1/drives/{id}/redeem`, with
    /// this device's drive token, which it does not spend): the period's new end, in seconds
    /// since 1970, when the answer names it.
    ///
    /// # Errors
    ///
    /// The token server's refusals: `token_used` (409: redeemed before - drop it),
    /// `wrong_tier`, `token_expired`, `bad_token`, `unknown_issuer`; a 401 is a sign-in.
    pub fn redeem_period_token(
        &self,
        drive_id: &str,
        drive_token: &str,
        token: &PeriodToken,
    ) -> Result<Option<u64>, TokenError> {
        let path = format!("/v1/drives/{}/redeem", check_id(drive_id)?);
        let body = json!({
            "tier": token.tier,
            "year": token.year,
            "nonce": token.nonce,
            "signature": token.signature,
            "randomizer": token.randomizer,
        });
        let value =
            self.call_or_null(Method::Post, &path, Some(token_of(drive_token)?), Some(&body))?;
        Ok(value["period_until"]
            .as_str()
            .and_then(azul_storage::time::parse_iso8601))
    }
}
