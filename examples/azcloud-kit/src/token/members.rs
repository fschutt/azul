//! The devices of a drive: a picked-up drive's own family, another device's join, the
//! lockdown from a device, and restores.

use super::*;

/// What `POST /v1/drives/{id}/claim` answers: a token family of the claiming computer's own.
/// `Debug` shows no token.
#[derive(Clone, PartialEq, Eq)]
pub struct ClaimedFamily {
    /// `owner`.
    pub member: String,
    pub drive_token: String,
    /// The pick-ups the ticket still takes.
    pub claims_left: Option<u32>,
}

impl fmt::Debug for ClaimedFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClaimedFamily")
            .field("member", &self.member)
            .field("drive_token", &"<hidden>")
            .field("claims_left", &self.claims_left)
            .finish()
    }
}

impl<'a> TokenServer<'a> {
    /// A token family of this computer's own for the paid drive `drive_id`, claimed with its
    /// sealed sign-up's `ticket` (`POST /v1/drives/{id}/claim {"ticket"}`, no drive token): what
    /// a computer that picks the drive up by its claim code spends, so it and the buyer's
    /// computer never share a family.
    ///
    /// `key` names the pick-up across its retries ([`crate::outbox`]): asked again after a lost
    /// answer, it is the same pick-up (the same family, counted once).
    ///
    /// # Errors
    ///
    /// The token server's refusal: 401 another ticket (or a drive from before the tickets), 409
    /// `claims_used`, 410 `claim_expired`, 403 `drive_banned`, 409 `already_answered` (the
    /// pick-up's answer arrived and its token was used); no answer.
    pub fn claim_drive(
        &self,
        drive_id: &str,
        ticket: &str,
        key: &IdempotencyKey,
    ) -> Result<ClaimedFamily, TokenError> {
        let path = format!("/v1/drives/{}/claim", check_id(drive_id)?);
        let body = json!({ "ticket": ticket.trim() });
        let value = self.grant(Method::Post, &path, None, Some(&body), key)?;
        let drive_token = value["drive_token"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_string();
        if drive_token.is_empty() {
            return Err(TokenError::Protocol(String::from(
                "the claim answered no drive token",
            )));
        }
        Ok(ClaimedFamily {
            member: value["member"].as_str().unwrap_or("owner").to_string(),
            drive_token,
            claims_left: value["claims_left"]
                .as_u64()
                .and_then(|n| u32::try_from(n).ok()),
        })
    }

    /// A new member token family of the drive (`POST /v1/drives/{id}/members`): what a second
    /// device joins with, `{"member", "drive_token"}`. `member` names it, else the token server
    /// does. `key` names the request across its retries: asked again, it is the same member.
    pub fn add_member(
        &self,
        drive_id: &str,
        drive_token: &str,
        member: Option<&str>,
        key: &IdempotencyKey,
    ) -> Result<Value, TokenError> {
        let path = format!("/v1/drives/{}/members", check_id(drive_id)?);
        let body = match member {
            Some(member) => json!({ "member": member }),
            None => json!({}),
        };
        self.grant(Method::Post, &path, Some(token_of(drive_token)?), Some(&body), key)
    }

    /// One device out (`DELETE /v1/drives/{id}/members/{member}`), from another device of the
    /// drive: the member's token families and the credentials made for it stop working.
    /// `member` is a name the token server gave ([`check_id`]'s letters only). `key` names the
    /// request across its retries.
    ///
    /// # Errors
    ///
    /// A member name that is no path segment (nothing is sent); the token server's refusal:
    /// 409 `own_member` (this device's own: a lockdown is for that), 404 `no_such_member`.
    pub fn remove_member(
        &self,
        drive_id: &str,
        drive_token: &str,
        member: &str,
        key: &IdempotencyKey,
    ) -> Result<Value, TokenError> {
        let path = format!(
            "/v1/drives/{}/members/{}",
            check_id(drive_id)?,
            check_id(member)?
        );
        self.grant(Method::Delete, &path, Some(token_of(drive_token)?), None, key)
    }

    /// The immediate lockdown by this device (`POST /v1/drives/{id}/lockdown`): every other
    /// token family, key and public link of the drive is revoked at once; the answer is this
    /// device's new grant (a bundle). The token sent is revoked by it: when the answer is lost,
    /// the same token with the same `key` asks again (within ten minutes) and gets that grant.
    pub fn lockdown(
        &self,
        drive_id: &str,
        drive_token: &str,
        key: &IdempotencyKey,
    ) -> Result<Value, TokenError> {
        let path = format!("/v1/drives/{}/lockdown", check_id(drive_id)?);
        let body = json!({});
        self.grant(Method::Post, &path, Some(token_of(drive_token)?), Some(&body), key)
    }

    /// Queues a restore of `prefix` (a key or a folder) as it was at `as_of` (RFC 3339; `POST
    /// /v1/drives/{id}/restore`): `{"request_id", "status"}`. `key` names the request across its
    /// retries: asked again, it is queued once.
    pub fn restore(
        &self,
        drive_id: &str,
        drive_token: &str,
        prefix: &str,
        as_of: &str,
        key: &IdempotencyKey,
    ) -> Result<Value, TokenError> {
        let path = format!("/v1/drives/{}/restore", check_id(drive_id)?);
        let body = json!({ "prefix": prefix, "as_of": as_of });
        self.grant(Method::Post, &path, Some(token_of(drive_token)?), Some(&body), key)
    }

    /// A restore's progress (`GET /v1/drives/{id}/restore/{request}`).
    pub fn restore_status(
        &self,
        drive_id: &str,
        drive_token: &str,
        request: &str,
    ) -> Result<Value, TokenError> {
        let path = format!(
            "/v1/drives/{}/restore/{}",
            check_id(drive_id)?,
            check_id(request)?
        );
        self.call_or_null(Method::Get, &path, Some(token_of(drive_token)?), None)
    }
}
