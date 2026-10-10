//! Sharing and the emergency calls.
//!
//! - A public link is a presigned GET (SigV4 query signing, azul-storage's
//!   [`S3Drive::presigned_get_url`]) of one object: anyone with the URL reads
//!   that object until it expires, no account needed. It is signed with this
//!   device's temporary credentials, so it ends with them (12 hours at most) -
//!   and SigV4 caps any presigned URL at seven days. A longer-lived link needs
//!   a link route of the token server, which does not exist yet; the link says
//!   why it was cut short.
//! - A synced file is shared by its path: the folder's index names its blob.
//!   The link's file name is then the blob's hash (a presigned GET here signs
//!   no `response-content-disposition`).
//! - Lockdown (a stolen device or token): every other device, key and public
//!   link loses access at once; this device continues with a new token
//!   family. Restore: the drive's objects under a prefix as they were at a
//!   time (the nodes keep every version for the retention window).
//!
//! Nothing here is sent but the calls to the token server and the index read of a synced
//! link: a link is signed on this computer.

use azul_storage::{HttpCall, HttpReply, S3Config, S3Drive, Transport};
use serde::Serialize;
use serde_json::Value;

use crate::{
    account::Account,
    error::{fail, CloudError, CloudResult},
    store::{Conditional, RemoteStore},
    sync::{remote, RemoteIndex},
};

/// The longest life of a presigned URL (SigV4's limit).
pub const MAX_PRESIGN_SECS: i64 = 7 * 86_400;

/// A public link.
#[derive(Clone, Debug, Serialize)]
pub struct Link {
    pub url: String,
    /// The object it reads.
    pub key: String,
    /// When it stops working (seconds since 1970).
    pub expires_at: i64,
    /// Why it lives shorter than asked, if it does.
    pub note: Option<String>,
}

/// The transport of the drive that signs a link: a link is made here, never sent.
struct NothingSent;

impl Transport for NothingSent {
    fn send(&self, _call: &HttpCall) -> Result<HttpReply, String> {
        Err(String::from("a link is signed on this computer, never sent"))
    }
}

/// A presigned GET of `key` through `endpoint` (the drive's HTTPS endpoint),
/// valid for `expires_secs` - cut to the credentials' life and to seven
/// days.
///
/// # Errors
///
/// When the credentials are missing or already expired.
pub fn public_link(
    account: &Account,
    endpoint: &str,
    key: &str,
    expires_secs: i64,
    now: i64,
) -> CloudResult<Link> {
    let creds = account.credentials()?;
    let record = account.record();
    let mut secs = expires_secs.clamp(1, MAX_PRESIGN_SECS);
    let mut note = (secs != expires_secs)
        .then(|| format!("a presigned link lives between 1 s and {MAX_PRESIGN_SECS} s (SigV4)"));
    if record.expires_at > 0 {
        let left = record.expires_at - now;
        if left <= 0 {
            fail!("the credentials expired: azcloud refresh first");
        }
        if secs > left {
            secs = left;
            note = Some(format!(
                "cut to {left} s: the link ends with this device's temporary credentials (12 h \
                 at most); a longer one needs the token server's link route (public_links), \
                 which does not exist yet"
            ));
        }
    }
    let config = S3Config {
        endpoint: endpoint.to_string(),
        region: record.region.clone(),
        bucket: record.bucket.clone(),
        path_style: record.path_style,
    };
    let signed_at = u64::try_from(now).unwrap_or(0);
    let signer = S3Drive::new(config, creds, Box::new(NothingSent))?.with_clock(move || signed_at);
    let url = signer.presigned_get_url(key, u64::try_from(secs).unwrap_or(1))?;
    Ok(Link {
        url,
        key: key.to_string(),
        expires_at: now + secs,
        note,
    })
}

/// A public link to the synced file `path` of the folder synced to `prefix`
/// (its blob, by the index).
///
/// # Errors
///
/// When the index cannot be read or does not name `path`.
pub fn synced_link<S: RemoteStore + ?Sized>(
    store: &S,
    account: &Account,
    endpoint: &str,
    prefix: &str,
    path: &str,
    expires_secs: i64,
    now: i64,
) -> CloudResult<Link> {
    let prefix = remote::normalize_prefix(prefix)?;
    let index_key = remote::index_key(&prefix);
    let index = match store.get_unless(&index_key, None)? {
        Conditional::Found { body, .. } => RemoteIndex::parse(&body)?,
        _ => fail!("nothing is synced to {prefix:?} (no {index_key})"),
    };
    let file = index
        .files
        .get(path)
        .ok_or_else(|| CloudError::failed(format!("{path:?} is not synced to {prefix:?}")))?;
    let mut link = public_link(
        account,
        endpoint,
        &remote::blob_key(&prefix, &file.hash),
        expires_secs,
        now,
    )?;
    let named = format!("the link's file name is the content's hash, not {path:?}");
    link.note = Some(match link.note {
        Some(note) => format!("{note}; {named}"),
        None => named,
    });
    Ok(link)
}

/// Locks the drive down from this device ([`Account::lockdown`]).
///
/// # Errors
///
/// The server's refusal or no answer.
pub fn lockdown(account: &mut Account) -> CloudResult<Value> {
    account.lockdown()
}

/// Cancels a pending recovery-key lockdown with the recovery code's 16 bytes
/// ([`Account::lockdown_cancel`]).
///
/// # Errors
///
/// The server's refusal or no answer.
pub fn lockdown_cancel(account: &Account, recovery_code: &[u8]) -> CloudResult<Value> {
    account.lockdown_cancel(recovery_code)
}

/// Queues a restore of `prefix` as it was at `as_of` ([`Account::restore`]).
///
/// # Errors
///
/// A time that is no RFC 3339, the server's refusal, or no answer.
pub fn restore(account: &Account, prefix: &str, as_of: &str) -> CloudResult<Value> {
    account.restore(prefix, as_of)
}

/// A restore's progress ([`Account::restore_status`]).
///
/// # Errors
///
/// The server's refusal or no answer.
pub fn restore_status(account: &Account, request: &str) -> CloudResult<Value> {
    account.restore_status(request)
}
