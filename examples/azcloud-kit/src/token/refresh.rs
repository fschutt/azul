//! The drive token's refresh into fresh credentials, the drive's status and its ban.

use super::*;
use crate::shared::{PendingSpend, RefreshOutcome, SpendOnce};
use azlin_contract::RefreshRefusal;

/// A drive as the token server keeps it ([`TokenServer::drive_status`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DriveStatus {
    /// `100GB`.
    pub tier: Option<String>,
    /// Paid until, in seconds since 1970.
    pub period_until: Option<u64>,
    /// A recovery-key lockdown takes effect then (seconds since 1970) unless a device of the
    /// owner cancels it with the recovery code ([`TokenServer::lockdown_cancel_signed`]).
    pub lockdown_pending_until: Option<u64>,
    /// The drive takes no writes (unpaid past its grace, a pending lockdown).
    pub read_only: bool,
    /// Every member the drive was given (its devices; [`RECOVERY_MEMBER`] while a recovery-key
    /// lockdown is pending), sorted.
    pub members: Vec<String>,
    /// The member whose token asked.
    pub you: Option<String>,
    /// The drive's quota in bytes - of STORED bytes: what arrives at the storage nodes,
    /// compressed and encrypted.
    pub quota_bytes: Option<u64>,
    /// The bytes the drive holds as the storage nodes count them (stored: compressed and
    /// encrypted); what the quota is counted against.
    pub used_bytes: Option<u64>,
    /// The drive is banned (ban contract v1): why, and until when it may still be read.
    pub ban: Option<Ban>,
    /// Why it takes no writes, when the server says it (`read_only_reason`; a server of
    /// before names none, and then read-only was a payment due).
    pub read_only_why: Option<azlin_contract::ReadOnlyWhy>,
    /// The drive is locked (unpaid past its read-only time; its files in offline storage):
    /// since when, the day its period must reach, whether its owner proved it is theirs, and
    /// whether its files are on their way back (`archive`).
    pub archive: Option<azlin_contract::ArchiveWire>,
}

impl DriveStatus {
    /// Whether the drive is locked: nothing is read or written until its owner gets it back.
    #[must_use]
    pub fn locked(&self) -> bool {
        self.archive.is_some()
    }

    /// Whether the drive takes no writes because a payment is due (not a recovery's freeze, an
    /// operator's, a ban).
    #[must_use]
    pub fn payment_due(&self) -> bool {
        self.read_only
            && self.ban.is_none()
            && self.archive.is_none()
            && matches!(self.read_only_why, None | Some(azlin_contract::ReadOnlyWhy::Unpaid))
    }
}

/// The error code of a banned drive's refusals: its writes during the grace period, everything
/// (the credentials, its status) after it.
pub const DRIVE_BANNED: &str = "drive_banned";

/// The banner over a banned drive: why, and how long its files can still be copied (`hours`;
/// `None`: no end said). AzDrive and AzMail show the same words.
#[must_use]
pub fn banner_text(reason: &str, hours: Option<u64>) -> String {
    match hours {
        Some(1) => format!(
            "Due to {reason}, your account has been banned, but you have 1 hour to migrate \
             your files."
        ),
        Some(hours) => format!(
            "Due to {reason}, your account has been banned, but you have {hours} hours to \
             migrate your files."
        ),
        None => format!(
            "Due to {reason}, your account has been banned: copy your files to this computer \
             now."
        ),
    }
}

/// What a banned drive shows past its end: when (`until`, the ban's end) and why.
#[must_use]
pub fn closed_text(until: Option<u64>, reason: &str) -> String {
    match until {
        Some(until) => {
            let when = azul_storage::time::iso8601(until);
            format!(
                "This drive was closed on {} because {reason}.",
                when.get(..10).unwrap_or(&when)
            )
        }
        None => format!("This drive was closed because {reason}."),
    }
}

/// What the token server says when a ban has no reason of its own.
const BAN_REASON: &str = "a breach of the terms of service";

/// A ban's words: (message id, English, German) - [`ban_fluent_source`].
const BAN_WORDS: [(&str, &str, &str); 5] = [
    (
        "azlin-ban-banner",
        "Due to { $reason }, your account has been banned, but you have { $hours ->\n        \
         [one] 1 hour\n       *[other] { $hours } hours\n    } to migrate your files.",
        "Wegen { $reason } wurde dein Konto gesperrt, aber du hast noch { $hours ->\n        \
         [one] 1 Stunde\n       *[other] { $hours } Stunden\n    }, um deine Dateien zu \
         übertragen.",
    ),
    (
        "azlin-ban-banner-no-end",
        "Due to { $reason }, your account has been banned: copy your files to this computer now.",
        "Wegen { $reason } wurde dein Konto gesperrt: Kopiere deine Dateien jetzt auf diesen \
         Computer.",
    ),
    (
        "azlin-ban-closed",
        "This drive was closed on { $day } because { $reason }.",
        "Dieses Laufwerk wurde am { $day } geschlossen, wegen { $reason }.",
    ),
    (
        "azlin-ban-closed-no-day",
        "This drive was closed because { $reason }.",
        "Dieses Laufwerk wurde geschlossen, wegen { $reason }.",
    ),
    (
        "azlin-ban-reason-terms",
        BAN_REASON,
        "eines Verstoßes gegen die Nutzungsbedingungen",
    ),
];

/// A ban's words as a Fluent resource in `lang`: an app gives it to the engine with the error
/// table and says a ban by [`Ban::banner_phrase`] / [`Ban::closed_phrase`].
#[must_use]
pub fn ban_fluent_source(lang: crate::user_errors::Lang) -> String {
    let mut out = String::from("# A banned drive's words (azcloud-kit token).\n");
    for (id, en, de) in BAN_WORDS {
        let text = match lang {
            crate::user_errors::Lang::En => en,
            crate::user_errors::Lang::De => de,
        };
        out.push_str(&format!("{id} = {text}\n"));
    }
    out
}

/// A ban's reason as a phrase's argument: the default one a word of [`BAN_WORDS`] (said in the
/// window's language), the token server's own as it is.
#[must_use]
pub fn ban_reason_arg(reason: &str) -> azul_appkit::phrase::Arg {
    if reason == BAN_REASON {
        azul_appkit::phrase::Arg::word("azlin-ban-reason-terms", BAN_REASON)
    } else {
        azul_appkit::phrase::Arg::from(reason)
    }
}

/// [`banner_text`] as a phrase of [`BAN_WORDS`].
#[must_use]
pub fn banner_phrase(reason: &str, hours: Option<u64>) -> azul_appkit::phrase::Phrase {
    use azul_appkit::phrase::Phrase;
    match hours {
        Some(hours) => Phrase::new("azlin-ban-banner")
            .arg("reason", ban_reason_arg(reason))
            .arg("hours", hours),
        None => Phrase::new("azlin-ban-banner-no-end").arg("reason", ban_reason_arg(reason)),
    }
}

/// [`closed_text`] as a phrase of [`BAN_WORDS`].
#[must_use]
pub fn closed_phrase(until: Option<u64>, reason: &str) -> azul_appkit::phrase::Phrase {
    use azul_appkit::phrase::Phrase;
    match until {
        Some(until) => {
            let when = azul_storage::time::iso8601(until);
            Phrase::new("azlin-ban-closed")
                .arg("day", when.get(..10).unwrap_or(&when))
                .arg("reason", ban_reason_arg(reason))
        }
        None => Phrase::new("azlin-ban-closed-no-day").arg("reason", ban_reason_arg(reason)),
    }
}

/// A drive banned with a grace period (ban contract v1): its files can be read, listed and
/// copied until `until`; nothing new is written; then the drive is closed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ban {
    /// Why, as the token server says it ("spam distribution").
    pub reason: String,
    /// Reads and credentials work until then (seconds since 1970); `None`: no end said.
    pub until: Option<u64>,
    /// The token server refused the drive past the end (403 `drive_banned`): it is closed,
    /// whatever this computer's clock says.
    pub closed: bool,
}

impl Ban {
    /// The ban an answer describes - `GET /v1/drives/{id}`, a credentials bundle (`status:
    /// "banned"`, `ban_reason`, `ban_until`) or a 403 `drive_banned`; `None` for a drive in good
    /// standing.
    #[must_use]
    pub fn of(value: &Value) -> Option<Ban> {
        let banned = value["status"].as_str() == Some("banned")
            || value["error"].as_str() == Some(DRIVE_BANNED)
            || value["ban_reason"].is_string();
        if !banned {
            return None;
        }
        let reason = value["ban_reason"]
            .as_str()
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .unwrap_or(BAN_REASON);
        let until = value["ban_until"].as_u64().or_else(|| {
            value["ban_until"]
                .as_str()
                .and_then(azul_storage::time::parse_iso8601)
        });
        Some(Ban {
            reason: reason.chars().take(200).collect(),
            until,
            closed: value["error"].as_str() == Some(DRIVE_BANNED),
        })
    }

    /// Whether the drive is closed at `now` (seconds since 1970): the token server said so, or
    /// its end has come.
    #[must_use]
    pub fn is_closed(&self, now: u64) -> bool {
        self.closed || self.until.is_some_and(|until| until <= now)
    }

    /// What every app shows over the banned drive at `now`: [`banner_text`] with the hours left,
    /// then, past the end, [`closed_text`].
    #[must_use]
    pub fn banner(&self, now: u64) -> String {
        if self.is_closed(now) {
            return self.closed_text();
        }
        banner_text(&self.reason, self.until.map(|_| self.hours_left(now)))
    }

    /// "This drive was closed on <date> because <reason>." ([`closed_text`]).
    #[must_use]
    pub fn closed_text(&self) -> String {
        closed_text(self.until, &self.reason)
    }

    /// [`Ban::banner`] as a phrase of the kit's messages (an app says it in the window's
    /// language).
    #[must_use]
    pub fn banner_phrase(&self, now: u64) -> azul_appkit::phrase::Phrase {
        if self.is_closed(now) {
            return self.closed_phrase();
        }
        banner_phrase(&self.reason, self.until.map(|_| self.hours_left(now)))
    }

    /// [`Ban::closed_text`] as a phrase of the kit's messages.
    #[must_use]
    pub fn closed_phrase(&self) -> azul_appkit::phrase::Phrase {
        closed_phrase(self.until, &self.reason)
    }

    /// The hours left at `now` to copy the files, rounded up (0 once it is closed, and when no
    /// end was said).
    #[must_use]
    pub fn hours_left(&self, now: u64) -> u64 {
        if self.is_closed(now) {
            return 0;
        }
        self.until
            .map_or(0, |until| until.saturating_sub(now).div_ceil(3_600))
    }
}

impl<'a> TokenServer<'a> {
    /// Spends the drive token of `spend` for fresh credentials and the next token (`name`: what
    /// this device calls the drive, which the answer's drive then carries). A refusal that says
    /// it spent nothing (the rate limit, `not_verified`, `try_again`, `unavailable` with
    /// `"spent": false`) is sent again with the same token under the policy, and when it lasts
    /// the token stays the device's ([`TokenError::Busy`] says how long to wait); no answer, or a
    /// failure that does not say so, is [`RefreshOutcome::Unknown`]: the token may be spent, and
    /// only the same token goes on ([`Self::retry`], or the next refresh: the keyring still
    /// holds it).
    pub fn refresh<'k>(&self, spend: SpendOnce<'k>, name: Option<&str>) -> RefreshOutcome<'k> {
        let drive_id = spend.drive_id().to_string();
        if drive_id.is_empty() {
            return RefreshOutcome::Refused {
                refusal: None,
                error: TokenError::Config(String::from("The drive has no id.")),
            };
        }
        let path = format!("/v1/drives/{}/credentials", uri_encode(&drive_id, true));
        let body = match name.map(str::trim) {
            Some(name) => json!({ "name": name }),
            None => json!({}),
        };
        let token = spend.token().as_str();
        let reply = match self.exchange(Method::Post, &path, Some(token), Some(&body)) {
            Ok(reply) => reply,
            // The request may have reached the token server and spent the token.
            Err(e) => return RefreshOutcome::Unknown(PendingSpend::new(spend, e)),
        };
        if reply.is_success() {
            let bundle = serde_json::from_slice::<Value>(&reply.body)
                .map_err(|_| TokenError::Protocol(String::from("the answer is not JSON")))
                .and_then(|value| DriveBundle::from_value(&value));
            return match bundle {
                Ok(bundle) if bundle.drive_id() == drive_id => {
                    RefreshOutcome::Rotated(spend.answered(bundle))
                }
                Ok(bundle) => {
                    let why = format!(
                        "the answer is for the drive {}, not {drive_id}",
                        bundle.drive_id()
                    );
                    RefreshOutcome::Unknown(PendingSpend::new(spend, TokenError::Protocol(why)))
                }
                // Spent, and the answer cannot be read.
                Err(e) => RefreshOutcome::Unknown(PendingSpend::new(spend, e)),
            };
        }
        let said: Option<RefreshRefusal> = serde_json::from_slice(&reply.body).ok();
        let error = refusal(&reply, true);
        match crate::answer::of_token_reply(&reply) {
            // A failure that does not say it spent nothing (or says it did): maybe it did.
            Outcome::Transient {
                spent: Spent::Maybe,
                ..
            } => RefreshOutcome::Unknown(PendingSpend::new(spend, error)),
            // Refused for good, or busy still after the policy's tries: nothing was spent, the
            // keyring keeps the token for the next refresh.
            _ => RefreshOutcome::Refused {
                refusal: said,
                error,
            },
        }
    }

    /// The same token once more after [`RefreshOutcome::Unknown`]: the token server answers it
    /// again while the token it may have rotated to was never used.
    pub fn retry<'k>(&self, pending: PendingSpend<'k>, name: Option<&str>) -> RefreshOutcome<'k> {
        self.refresh(pending.into_spend(), name)
    }

    // ==== A drive's account calls, with this device's drive token ====

    /// The drive's state as the token server keeps it (`GET /v1/drives/{id}`): its tier, quota,
    /// read-only flag, members, a pending lockdown.
    pub fn info(&self, drive_id: &str, drive_token: &str) -> Result<Value, TokenError> {
        let path = format!("/v1/drives/{}", check_id(drive_id)?);
        self.call_or_null(Method::Get, &path, Some(token_of(drive_token)?), None)
    }

    /// [`Self::info`] as a [`DriveStatus`]: the tier, the period's end, a pending recovery-key
    /// lockdown, whether the drive takes no writes, its members and which one asked, a ban.
    /// Past a ban's end the token server refuses the drive (403 `drive_banned`): that answer is
    /// the status of a closed drive, not an error.
    pub fn drive_status(
        &self,
        drive_id: &str,
        drive_token: &str,
    ) -> Result<DriveStatus, TokenError> {
        let path = format!("/v1/drives/{}", check_id(drive_id)?);
        let reply = self.exchange(Method::Get, &path, Some(token_of(drive_token)?), None)?;
        if reply.status == 403 {
            let value: Value = serde_json::from_slice(&reply.body).unwrap_or(Value::Null);
            if value["error"].as_str() == Some(DRIVE_BANNED) {
                return Ok(DriveStatus {
                    read_only: true,
                    ban: Ban::of(&value),
                    ..DriveStatus::default()
                });
            }
        }
        if !reply.is_success() {
            return Err(refusal(&reply, true));
        }
        let info: Value = if reply.body.iter().all(u8::is_ascii_whitespace) {
            Value::Null
        } else {
            serde_json::from_slice(&reply.body)
                .map_err(|_| TokenError::Protocol(String::from("the answer is not JSON")))?
        };
        let time = |key: &str| info[key].as_str().and_then(azul_storage::time::parse_iso8601);
        let mut members: Vec<String> = info["members"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|m| m["member"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        members.sort();
        members.dedup();
        Ok(DriveStatus {
            tier: info["tier"].as_str().map(str::to_string),
            period_until: time("period_until"),
            lockdown_pending_until: time("lockdown_pending_until"),
            read_only: info["read_only"].as_bool().unwrap_or(false),
            members,
            you: info["you"].as_str().map(str::to_string),
            quota_bytes: info["quota_bytes"].as_u64(),
            // Stored bytes (compressed and encrypted, what arrived); `stored_bytes` is the same
            // count under the name some servers give it.
            used_bytes: info["used_bytes"]
                .as_u64()
                .or_else(|| info["stored_bytes"].as_u64()),
            ban: Ban::of(&info),
            read_only_why: info["read_only_reason"]
                .as_str()
                .map(azlin_contract::ReadOnlyWhy::from_wire),
            archive: serde_json::from_value(info["archive"].clone()).ok(),
        })
    }
}
