//! A paid checkout's period tokens: the issuer keys, the blind issue, the redemption.

use super::*;

/// What `POST /v1/tokens/issue` answers: one blind signature per blinded message, by the
/// issuer key of the tier and year.
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
}

/// The most blinded messages one `POST /v1/tokens/issue` takes (24 months, prepaid).
pub const MAX_BLINDED: usize = 24;

/// What `POST /v1/tokens/issue` answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueAnswer {
    /// The blind signatures, by the key the request named.
    Signed(BlindSignatures),
    /// 409 `key_changed`: the request named a key the token server no longer signs with
    /// (the year turned since `GET /v1/tokens/keys`); nothing was signed or counted. Blind the
    /// messages again for `key_id` - with `public_key_pem` when the server sent it, else from
    /// the keys read again.
    KeyChanged {
        key_id: String,
        public_key_pem: Option<String>,
    },
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
    /// message of `blinded` (standard base64, at most [`MAX_BLINDED`]), up to the checkout's
    /// months in all. Only with `issue_key`, the key its sealed sign-up carries
    /// ([`crate::bundle::PeriodTokens`]): the checkout id alone, which the payment provider sees,
    /// issues nothing. `key_id` names the issuer key the messages were blinded for
    /// (`GET /v1/tokens/keys`); when it is no longer the one the token server signs with, the
    /// answer is [`IssueAnswer::KeyChanged`] and nothing is signed or counted. The same request
    /// again (same key, same messages, same order) is answered the same, counted once: what to
    /// send when an answer was lost.
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] without an issue key, a key id or messages (nothing is sent); the
    /// token server's refusals with their codes - `issue_key_required`, `key_id_required`,
    /// `issue_key_wrong` (403, no sign-in matter), `already_issued`, `not_paid`,
    /// `mandate_stopped`; an answer whose signatures do not match the messages.
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
            let value: Value = serde_json::from_slice(&reply.body).unwrap_or(Value::Null);
            if reply.status == 409 && value["error"] == "key_changed" {
                return Ok(IssueAnswer::KeyChanged {
                    key_id: value["key_id"].as_str().unwrap_or_default().trim().to_string(),
                    public_key_pem: value["public_key_pem"]
                        .as_str()
                        .filter(|pem| !pem.trim().is_empty())
                        .map(str::to_string),
                });
            }
            return Err(refusal(&reply, false));
        }
        let value: Value = serde_json::from_slice(&reply.body)
            .map_err(|_| TokenError::Protocol(String::from("the answer is not JSON")))?;
        let text = |key: &str| value[key].as_str().unwrap_or_default().to_string();
        let signatures: Vec<String> = value["blind_signatures"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if signatures.len() != blinded.len() {
            return Err(TokenError::Protocol(format!(
                "{} blind signatures for {} blinded messages",
                signatures.len(),
                blinded.len()
            )));
        }
        Ok(IssueAnswer::Signed(BlindSignatures {
            tier: text("tier"),
            key_id: text("key_id"),
            public_key_pem: text("public_key_pem"),
            signatures,
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
