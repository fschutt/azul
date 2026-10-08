//! Sharing and the emergency calls (AZDRIVE-INTEGRATION.md §3, PLAN §18.7).
//!
//! - A public link is a presigned GET (SigV4 query signing) of one object:
//!   anyone with the URL reads that object until it expires, no account
//!   needed. It is signed with this device's temporary credentials, so it
//!   ends with them (12 hours at most) - and SigV4 caps any presigned URL at
//!   seven days. A longer-lived link needs the token server's link route over
//!   its `public_links` table, which does not exist yet; the link says why it
//!   was cut short.
//! - A synced file is shared by its path: the folder's index names its blob.
//!   The link's file name is then the blob's hash (a presigned GET here signs
//!   no `response-content-disposition`).
//! - Lockdown (a stolen device or token): every other device, key and public
//!   link loses access at once; this device continues with a new token
//!   family. Restore: the drive's objects under a prefix as they were at a
//!   time (the nodes keep every version for the retention window, D38).

use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use serde_json::Value;

use crate::{
    account::Account,
    sync::{remote, RemoteIndex, RemoteStore},
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
) -> Result<Link> {
    let creds = account.credentials()?;
    let record = account.record();
    let mut secs = expires_secs.clamp(1, MAX_PRESIGN_SECS);
    let mut note = (secs != expires_secs)
        .then(|| format!("a presigned link lives between 1 s and {MAX_PRESIGN_SECS} s (SigV4)"));
    if record.expires_at > 0 {
        let left = record.expires_at - now;
        if left <= 0 {
            bail!("the credentials expired: azcloud refresh first");
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
    let mut client = azlin_proto::s3req::S3Client::new(endpoint, &record.bucket, &creds);
    client.region = record.region.clone();
    client.path_style = record.path_style;
    let expires = u32::try_from(secs).unwrap_or(u32::MAX);
    Ok(Link {
        url: client.presign_get(now, key, expires),
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
pub async fn synced_link<S: RemoteStore>(
    store: &S,
    account: &Account,
    endpoint: &str,
    prefix: &str,
    path: &str,
    expires_secs: i64,
    now: i64,
) -> Result<Link> {
    let prefix = remote::normalize_prefix(prefix)?;
    let index_key = remote::index_key(&prefix);
    let index = match store.get_unless(&index_key, None).await? {
        crate::drive::Conditional::Found { body, .. } => RemoteIndex::parse(&body)?,
        _ => bail!("nothing is synced to {prefix:?} (no {index_key})"),
    };
    let file = index
        .files
        .get(path)
        .ok_or_else(|| anyhow!("{path:?} is not synced to {prefix:?}"))?;
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
pub async fn lockdown(account: &mut Account) -> Result<Value> {
    account.lockdown().await
}

/// Cancels a pending recovery-key lockdown ([`Account::lockdown_cancel`]).
///
/// # Errors
///
/// The server's refusal or no answer.
pub async fn lockdown_cancel(account: &Account) -> Result<Value> {
    account.lockdown_cancel().await
}

/// Queues a restore of `prefix` as it was at `as_of` ([`Account::restore`]).
///
/// # Errors
///
/// A time that is no RFC 3339, the server's refusal, or no answer.
pub async fn restore(account: &Account, prefix: &str, as_of: &str) -> Result<Value> {
    account.restore(prefix, as_of).await
}

/// A restore's progress ([`Account::restore_status`]).
///
/// # Errors
///
/// The server's refusal or no answer.
pub async fn restore_status(account: &Account, request: &str) -> Result<Value> {
    account.restore_status(request).await
}

#[cfg(test)]
mod tests {
    use azul_storage::testing::TempDir;
    use serde_json::json;

    use super::*;
    use crate::{
        account::{read_grant, store_grant},
        state::StateDir,
    };

    fn account(dir: &TempDir, expires_at: &str) -> Account {
        let state = StateDir::open(dir.path()).unwrap();
        let answer = json!({
            "drive": {"id": "d_1", "location": {"endpoint": "http://127.0.0.1:9000", "bucket": "d-1",
                       "region": "us-east-1", "path_style": true}},
            "credentials": {"access_key_id": "AZTK", "secret_access_key": "s", "session_token": "t",
                            "expires_at": expires_at},
            "drive_token": "dt_f.0.x",
        });
        store_grant(
            &state,
            &read_grant(&answer, "http://t", "owner", 0).unwrap(),
        )
        .unwrap();
        Account::open(&state, "http://t", None).unwrap()
    }

    #[test]
    fn a_link_is_a_presigned_get_cut_to_the_credentials_life_with_the_reason() {
        let dir = TempDir::new("azcloud-share");
        let a = account(&dir, "2026-10-08T12:00:00Z");
        let expires = azlin_proto::time::parse_iso("2026-10-08T12:00:00Z").unwrap();
        let now = expires - 3600;
        let link = public_link(&a, "http://127.0.0.1:9000", "docs/a.pdf", 600, now).unwrap();
        assert!(
            link.url
                .starts_with("http://127.0.0.1:9000/d-1/docs/a.pdf?"),
            "{}",
            link.url
        );
        assert!(link.url.contains("X-Amz-Signature="), "{}", link.url);
        assert_eq!(link.expires_at, now + 600);
        assert!(link.note.is_none());
        let long = public_link(&a, "http://127.0.0.1:9000", "docs/a.pdf", 86_400, now).unwrap();
        assert_eq!(long.expires_at, expires);
        assert!(long.note.unwrap().contains("public_links"));
        assert!(public_link(&a, "http://127.0.0.1:9000", "a", 60, expires + 1).is_err());
    }
}
