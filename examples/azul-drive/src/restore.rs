//! "Restore as of..." of an Azlin drive (D42; Options > Drives and the drive's menu in the
//! source list): a time typed in a dialog - a while ago ("1 hour ago") or a UTC time - and the
//! whole drive put back as it was then ([`Job::RestoreDrive`]):
//!
//! - an encrypted drive (feature `encryption`) from its metadata repository's history, as one
//!   new commit: nothing in between is lost ([`azcloud_kit::restore_drive_as_of`]);
//! - a plain one's bucket by the token server and the drive's node, from the node's retention
//!   ([`azcloud_kit::restore_bucket_as_of`]: a grant, under the drive's keyring lock with its
//!   newest drive token).
//!
//! AzDrive restores the last [`RESTORE_DAYS`] days (what a node keeps, D38).
//!
//! On stdout: `AZDRIVE_RESTORED <drive id> <as of> files|objects <count>`, or `... queued
//! <request>` when the node had not done it when AzDrive stopped waiting.

use std::time::Duration;

use azcloud_kit::{restore_bucket_as_of, BucketRestore, SharedKeyring, TokenServer};
use azul::prelude::*;
use azul_appkit::l10n::{t, Phrase, Text};
use azul_storage::{
    azul_transport::AzulTransport,
    time::{iso8601, parse_iso8601},
};

use crate::{
    actions::now_secs,
    browse::Place,
    jobs::Job,
    l10n::{cloud_error_text, token_error_text},
    periods, spawn, with_state, DriveState, Popup, USER_AGENT,
};

/// How far back AzDrive restores a drive.
pub(crate) const RESTORE_DAYS: u64 = 14;
/// How long the job waits for the drive's node, and how often it asks.
const WAIT: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_secs(2);

/// An encrypted drive's own restore, run on the job's thread with the time (seconds since
/// 1970): the files it put back or took away, or why not; `None` from it when the drive turns
/// out plain (its bucket is restored by the token server).
pub(crate) type EncryptedRestore = Box<dyn FnOnce(i64) -> Option<Result<usize, String>> + Send>;

/// What a restore came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Restored {
    /// An encrypted drive: the files its repository put back or took away.
    Files(usize),
    /// A plain drive: the objects the node changed.
    Objects(u64),
    /// Still queued at the token server when AzDrive stopped waiting: the request.
    Queued(String),
}

/// The time typed: `<n> minutes|hours|days ago` (German `vor <n> Minuten|Stunden|Tagen`), an
/// RFC 3339 UTC time, or `YYYY-MM-DD HH:MM` (UTC) - not to come yet, and within the last
/// [`RESTORE_DAYS`] days of `now`.
pub(crate) fn parse_as_of(text: &str, now: u64) -> Result<u64, Text> {
    let text = text.trim();
    if text.is_empty() {
        return Err(Text::key("azdrive-restore-type-time"));
    }
    let at = ago(text, now)
        .or_else(|| utc(text))
        .ok_or_else(|| Text::from(Phrase::new("azdrive-restore-unknown-time").arg("text", text)))?;
    if at > now {
        return Err(Text::key("azdrive-restore-in-future"));
    }
    if now - at > RESTORE_DAYS * 86_400 {
        return Err(Phrase::new("azdrive-restore-too-old")
            .arg("days", RESTORE_DAYS)
            .into());
    }
    Ok(at)
}

/// `<n> minutes|hours|days ago` (or `vor <n> Minuten|Stunden|Tagen`) before `now`.
fn ago(text: &str, now: u64) -> Option<u64> {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    let (count, unit) = match words.as_slice() {
        [count, unit, "ago"] => (*count, *unit),
        ["vor", count, unit] => (*count, *unit),
        _ => return None,
    };
    let count: u64 = count.parse().ok()?;
    let unit: u64 = match unit {
        "minute" | "minutes" | "minuten" => 60,
        "hour" | "hours" | "stunde" | "stunden" => 3_600,
        "day" | "days" | "tag" | "tage" | "tagen" => 86_400,
        _ => return None,
    };
    now.checked_sub(count.checked_mul(unit)?)
}

/// A UTC time: RFC 3339, or without its seconds or its `Z`.
fn utc(text: &str) -> Option<u64> {
    let full = match text.len() {
        16 => format!("{text}:00Z"),
        19 => format!("{text}Z"),
        _ => text.to_string(),
    };
    let field = |range: std::ops::Range<usize>| full.get(range)?.parse::<u32>().ok();
    let (month, day) = (field(5..7)?, field(8..10)?);
    let (hour, minute, second) = (field(11..13)?, field(14..16)?, field(17..19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    parse_iso8601(&full)
}

/// What the window says after a restore of `drive` as of `as_of`.
pub(crate) fn restored_text(drive: &str, as_of: u64, how: &Restored) -> Phrase {
    let said = match how {
        Restored::Files(count) => Phrase::new("azdrive-restored-files").arg("count", *count),
        Restored::Objects(count) => Phrase::new("azdrive-restored-objects").arg("count", *count),
        Restored::Queued(request) => {
            Phrase::new("azdrive-restore-queued").arg("request", request.as_str())
        }
    };
    said.arg("drive", drive).arg("at", iso8601(as_of))
}

// ==== The dialog ====

/// Opens the "Restore as of..." dialog of `drive_id`.
pub(crate) fn open(s: &mut DriveState, drive_id: &str) {
    if s.popup.is_some() {
        return;
    }
    s.popups_opened += 1;
    s.popup = Some(Popup::Restore {
        drive_id: drive_id.to_string(),
        // What the time field holds when it opens: "1 hour ago" in the window's language.
        text: t("azdrive-restore-default-as-of"),
        error: Text::default(),
        busy: false,
    });
}

/// The dialog's Restore.
pub(crate) extern "C" fn on_restore(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| start(info, app, s))
}

fn start(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(Popup::Restore {
        drive_id,
        text,
        busy: false,
        ..
    }) = s.popup.as_ref()
    else {
        return;
    };
    let (drive_id, text) = (drive_id.clone(), text.clone());
    let fallback = s.token.url.clone();
    let token_url = s
        .slot_index(&drive_id)
        .and_then(|index| periods::azlin_drive(&s.slots[index].entry, fallback.as_deref()))
        .map(|(_, url)| url);
    let as_of = parse_as_of(&text, now_secs());
    let encrypted = match (&token_url, &as_of) {
        (Some(_), Ok(_)) => encrypted_restore(s, &drive_id),
        _ => Ok(None),
    };
    let problem = match (&token_url, &as_of, &encrypted) {
        (None, _, _) => Some(Text::key("azdrive-no-token-server")),
        (_, Err(why), _) | (_, _, Err(why)) => Some(why.clone()),
        _ => None,
    };
    if let Some(Popup::Restore { error, busy, .. }) = s.popup.as_mut() {
        match problem {
            Some(why) => {
                *error = why;
                return;
            }
            None => {
                *error = Text::default();
                *busy = true;
            }
        }
    }
    let (Some(token_url), Ok(as_of), Ok(encrypted)) = (token_url, as_of, encrypted) else {
        return;
    };
    let job = Job::RestoreDrive {
        drive_id,
        as_of,
        token_url,
        keyring: s.keyring.clone(),
        encrypted,
    };
    spawn(info, app, s, job);
}

/// An encrypted drive's restore from its repository (the drive opened first); an error when
/// it cannot be opened - never the bucket's restore for a drive that may be encrypted.
#[cfg(feature = "encryption")]
fn encrypted_restore(s: &mut DriveState, drive_id: &str) -> Result<Option<EncryptedRestore>, Text> {
    crate::encryption::restore_of(s, drive_id)
        .map(Some)
        .ok_or_else(|| Text::key("azdrive-restore-open-first"))
}

/// A build without encrypted drives restores every Azlin drive's bucket.
#[cfg(not(feature = "encryption"))]
fn encrypted_restore(
    _s: &mut DriveState,
    _drive_id: &str,
) -> Result<Option<EncryptedRestore>, Text> {
    Ok(None)
}

// ==== The job ====

/// [`Job::RestoreDrive`] on its thread: the encrypted drive's own restore, else the bucket's
/// at the token server (waited for up to two minutes).
pub(crate) fn run(
    drive_id: &str,
    as_of: u64,
    token_url: &str,
    keyring: &SharedKeyring,
    encrypted: Option<EncryptedRestore>,
) -> Result<Restored, Text> {
    let as_of = i64::try_from(as_of).map_err(|_| Text::key("azdrive-restore-out-of-range"))?;
    if let Some(restore) = encrypted {
        if let Some(result) = restore(as_of) {
            return result.map(Restored::Files).map_err(|why| crate::l10n::said(&why));
        }
    }
    let transport = AzulTransport::new(USER_AGENT);
    let server = TokenServer::new(token_url, &transport).map_err(|e| token_error_text(&e))?;
    match restore_bucket_as_of(&server, keyring, drive_id, as_of, POLL, WAIT)
        .map_err(|e| cloud_error_text(&e))?
    {
        BucketRestore::Done { objects, .. } => Ok(Restored::Objects(objects)),
        BucketRestore::Failed { error, .. } => {
            Err(Text::key("azdrive-restore-node-failed").then(" ").then(error))
        }
        BucketRestore::Queued { request } => Ok(Restored::Queued(request)),
    }
}

/// What the restore of `drive_id` as of `as_of` came to: the dialog closes, the window says it
/// and lists the drive again when it is in view; or the dialog says why not.
pub(crate) fn restored(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    as_of: u64,
    result: Result<Restored, Text>,
) {
    let open = matches!(&s.popup, Some(Popup::Restore { drive_id: id, .. }) if id == drive_id);
    match result {
        Ok(how) => {
            let (kind, what) = match &how {
                Restored::Files(count) => ("files", count.to_string()),
                Restored::Objects(count) => ("objects", count.to_string()),
                Restored::Queued(request) => ("queued", request.clone()),
            };
            println!(
                "AZDRIVE_RESTORED {drive_id} {} {kind} {what}",
                iso8601(as_of)
            );
            if open {
                s.popup = None;
            }
            let name = s.drive_name(&Place::folder(drive_id, ""));
            s.success(restored_text(&name, as_of, &how));
            if s.current_drive_id().as_deref() == Some(drive_id) {
                crate::refresh(info, app, s);
            }
        }
        Err(why) => {
            if open {
                if let Some(Popup::Restore { error, busy, .. }) = s.popup.as_mut() {
                    *error = why;
                    *busy = false;
                }
            } else {
                s.error(Text::key("azdrive-restore-failed").then(" ").then(why));
            }
        }
    }
}
