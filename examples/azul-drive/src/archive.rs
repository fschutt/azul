//! An unpaid Azlin drive's last months in AzDrive, and its way back.
//!
//! After its paid period an Azlin drive stays writable, then it is read-only, then it is locked:
//! nothing is read or written any more, and its files move to one offline copy until its owner
//! proves the drive is theirs and pays (the contract's promise, [`promise_phrase`], with its
//! days). AzDrive says each step before it comes:
//! - while it is unpaid, a bar over the drive says exactly what happens when (the reminders 30,
//!   7 and 1 days before each step are renewals.rs's);
//! - while it is read-only, AzDrive copies every file of the drive to this computer, into
//!   `Azlin rescue/<drive>` in the home folder, so nothing is lost when it locks; the bar shows
//!   how far that got and which files are still missing, and the copy runs again at every look;
//! - once it is locked, the drive is empty in AzDrive: a page says why and offers "Get my drive
//!   back" - this computer's own device key (registered for restores only when it joined the
//!   drive, set up its encryption or unlocked it) signs the token server's statement, or the
//!   recovery code, typed, on a computer without one; then Renew pays the three months the drive
//!   owes, and the paid checkout is named in a second proof so its months count at once;
//! - when its files are back, the drive opens again and AzDrive says so once.
//!
//! On stdout, for scripts: `AZDRIVE_ARCHIVE_STAGE <drive> <grace|read_only|locked|restore_pending>`,
//! `AZDRIVE_ARCHIVE_BACK <drive>`, `AZDRIVE_RESCUE <drive> <copied> <total> <missing>`,
//! `AZDRIVE_ARCHIVE_PROVEN <drive> <locked|restore_pending|restored>`.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::UNIX_EPOCH,
};

use azcloud_kit::{
    azlin_contract::{
        months_owed, ArchiveRefusal, ArchiveState, ArchiveWire, LOCK_AFTER_SECS, UNPAID_DAYS,
        WRITABLE_GRACE_SECS,
    },
    sync::names::{self, Volume},
    DriveStatus, ProofAnswer, SharedKeyring, TokenError, TokenServer,
};
use azul::{
    callbacks::ButtonOnClickCallbackType, file::FilePath, prelude::*, str::String as AzString,
};
use azul_appkit::l10n::{label, t_phrase, Phrase};
use azul_storage::{
    azul_transport::AzulTransport,
    beneath,
    cap::{Cap, Posture},
    ctl::Quiet,
    durable::{self, Expect},
    key::System,
    names::{Policy, Purpose},
    transfer, ByteRange, Drive, DriveError, Limit, ListPage, ListRequest, LocalDrive, ObjectInfo,
};
use serde::{Deserialize, Serialize};

use crate::{actions::now_secs, browse::Place, ids, jobs::Job, spawn, DriveState, HOME_ID};

/// The folder in the home folder the read-only drives are copied into.
pub(crate) const RESCUE_DIR: &str = "Azlin rescue";
/// What a copy leaves free on the disk besides the files.
const SPARE_BYTES: u64 = 1024 * 1024 * 1024;
/// A day: how often a finished copy runs again.
const DAY: u64 = 86_400;

/// Where an unpaid drive is in its last months.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Stage {
    /// Unpaid since `period_until`, still writable until `read_only_at`, locked at `locks_at`.
    Grace {
        period_until: u64,
        read_only_at: u64,
        locks_at: u64,
    },
    /// Read-only (reads and the owner's deletes go on) until `locks_at`.
    ReadOnly { period_until: u64, locks_at: u64 },
    /// Locked: its files in offline storage, or on their way back.
    Locked {
        wire: ArchiveWire,
        period_until: Option<u64>,
    },
}

impl Stage {
    /// Its name on stdout.
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Stage::Grace { .. } => "grace",
            Stage::ReadOnly { .. } => "read_only",
            Stage::Locked { wire, .. } if wire.state == ArchiveState::RestorePending => {
                "restore_pending"
            }
            Stage::Locked { .. } => "locked",
        }
    }
}

/// The stage of a drive with `status` at `now`; `None` while it is paid. A banned drive has its
/// own bar (ban.rs) and none of these.
#[must_use]
pub(crate) fn stage_of(status: &DriveStatus, now: u64) -> Option<Stage> {
    if status.ban.is_some() {
        return None;
    }
    if let Some(wire) = &status.archive {
        return Some(Stage::Locked {
            wire: wire.clone(),
            period_until: status.period_until,
        });
    }
    let period_until = status.period_until?;
    let grace = u64::try_from(WRITABLE_GRACE_SECS).unwrap_or(0);
    let debt = u64::try_from(LOCK_AFTER_SECS).unwrap_or(0);
    let read_only_at = period_until + grace;
    let locks_at = period_until + debt;
    // The token server says the drive takes no writes because a payment is due: it IS read-only,
    // whatever this computer's clock says - a clock a month behind would hide the bar of a drive
    // that refuses every write (AZ-P19).
    if status.payment_due() {
        return Some(Stage::ReadOnly {
            period_until,
            locks_at,
        });
    }
    // Otherwise the drive takes writes. The clock only tells whether the countdown of its grace
    // shows yet; it never makes a writable drive look read-only (a clock ahead past the grace
    // did) or locked - the server flips the drive, and the next look says so.
    if now < period_until {
        return None;
    }
    Some(Stage::Grace {
        period_until,
        read_only_at,
        locks_at,
    })
}

/// `YYYY-MM-DD` of `at` (seconds since 1970).
fn day_of(at: u64) -> String {
    azul_storage::time::iso8601(at).chars().take(10).collect()
}

/// The months (of 30 days) a locked drive still needs before it comes back: its grace and
/// read-only time, less what was paid since.
#[must_use]
pub(crate) fn months_needed(wire: &ArchiveWire, period_until: Option<u64>) -> u32 {
    let until = period_until
        .and_then(|p| i64::try_from(p).ok())
        .unwrap_or(0);
    months_owed(until, wire.pay_through.unix())
}

/// The promise to an unpaid drive's owner (the contract's [`unpaid_promise`] in the window's
/// language), its days from the contract.
///
/// [`unpaid_promise`]: azcloud_kit::azlin_contract::unpaid_promise
#[must_use]
pub(crate) fn promise_phrase() -> Phrase {
    Phrase::new("azdrive-unpaid-promise")
        .arg("grace", UNPAID_DAYS.grace)
        .arg("read_only", UNPAID_DAYS.read_only)
        .arg("lock", UNPAID_DAYS.lock)
}

/// What the bar over an unpaid drive `drive` says of `stage`: what happens when.
#[must_use]
pub(crate) fn stage_phrase(drive: &str, stage: &Stage) -> Phrase {
    match stage {
        Stage::Grace {
            period_until,
            read_only_at,
            locks_at,
        } => Phrase::new("azdrive-archive-grace")
            .arg("drive", drive)
            .arg("paid", day_of(*period_until))
            .arg("read_only", day_of(*read_only_at))
            .arg("locked", day_of(*locks_at)),
        Stage::ReadOnly {
            period_until,
            locks_at,
        } => Phrase::new("azdrive-archive-read-only")
            .arg("drive", drive)
            .arg(
                "read_only",
                day_of(*period_until + u64::try_from(WRITABLE_GRACE_SECS).unwrap_or(0)),
            )
            .arg("locked", day_of(*locks_at)),
        Stage::Locked { wire, .. } if wire.state == ArchiveState::RestorePending => {
            Phrase::new("azdrive-archive-restore-pending").arg("drive", drive)
        }
        Stage::Locked { wire, period_until } => {
            let paid = wire.locked_at.unix() - LOCK_AFTER_SECS;
            let key = if wire.proven {
                "azdrive-archive-locked-proven"
            } else {
                "azdrive-archive-locked"
            };
            Phrase::new(key)
                .arg("drive", drive)
                .arg(
                    "locked",
                    day_of(u64::try_from(wire.locked_at.unix()).unwrap_or(0)),
                )
                .arg("paid", day_of(u64::try_from(paid).unwrap_or(0)))
                .arg("months", i64::from(months_needed(wire, *period_until)))
        }
    }
}

// ==== What the looks saw ====

/// What the looks saw of an Azlin drive's last months.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Seen {
    pub stage: Stage,
}

/// What a look at the Azlin drive `azlin_id` saw: its stage kept for the bar and the locked
/// page; a new stage said on stdout; a drive that was locked and is open again said once.
pub(crate) fn seen(
    info: &mut CallbackInfo,
    s: &mut DriveState,
    azlin_id: &str,
    status: &DriveStatus,
) {
    let now = now_secs();
    let stage = stage_of(status, now);
    let before = s.archives.get(azlin_id).map(|seen| seen.stage.clone());
    if before.as_ref().map(Stage::name) != stage.as_ref().map(Stage::name) {
        if let Some(stage) = &stage {
            println!("AZDRIVE_ARCHIVE_STAGE {azlin_id} {}", stage.name());
        }
    }
    let was_locked = matches!(before, Some(Stage::Locked { .. }));
    match stage {
        Some(stage) => {
            s.archives.insert(azlin_id.to_string(), Seen { stage });
        }
        None => {
            s.archives.remove(azlin_id);
            if was_locked {
                back(info, s, azlin_id);
            }
        }
    }
}

/// A locked drive opened again: said once, and its folder read again.
fn back(info: &mut CallbackInfo, s: &mut DriveState, azlin_id: &str) {
    println!("AZDRIVE_ARCHIVE_BACK {azlin_id}");
    let name = s.drive_name(&Place::folder(
        &slot_of(s, azlin_id).unwrap_or_default(),
        "",
    ));
    let said = Phrase::new("azdrive-archive-back").arg("drive", name.as_str());
    let body = azul_appkit::l10n::translate_text(info, &said.clone().into());
    info.post_notification(
        azul::notification::Notification::create(
            format!("azdrive-archive-back-{azlin_id}"),
            "AzDrive",
        )
        .with_body(body),
    );
    s.success(said);
}

/// AzDrive's id of the Azlin drive `azlin_id`.
fn slot_of(s: &DriveState, azlin_id: &str) -> Option<String> {
    s.slots
        .iter()
        .find(|slot| slot.entry.azlin().is_some_and(|(id, _)| id == azlin_id))
        .map(|slot| slot.entry.id.clone())
}

/// The Azlin id of AzDrive's drive `drive_id`.
fn azlin_of(s: &DriveState, drive_id: &str) -> Option<String> {
    let index = s.slot_index(drive_id)?;
    s.slots[index].entry.azlin().map(|(id, _)| id.to_string())
}

/// What the looks saw of the drive in view: its AzDrive id, its Azlin id, its stage.
fn in_view(s: &DriveState) -> Option<(String, String, &Stage)> {
    let drive_id = s.current_drive_id()?;
    let azlin_id = azlin_of(s, &drive_id)?;
    let seen = s.archives.get(&azlin_id)?;
    Some((drive_id, azlin_id, &seen.stage))
}

/// The status line of synced drive `drive_id` (AzDrive's id) while it is locked: since when,
/// and that its files are in offline storage, or that they are coming back.
#[must_use]
pub(crate) fn sync_status(s: &DriveState, drive_id: &str) -> Option<Phrase> {
    let azlin_id = azlin_of(s, drive_id)?;
    let Stage::Locked { wire, .. } = &s.archives.get(&azlin_id)?.stage else {
        return None;
    };
    if wire.state == ArchiveState::RestorePending {
        return Some(Phrase::new("azdrive-archive-sync-restoring"));
    }
    let locked = u64::try_from(wire.locked_at.unix()).unwrap_or(0);
    Some(Phrase::new("azdrive-archive-sync-locked").arg("locked", day_of(locked)))
}

// ==== The bar over an unpaid drive ====

/// The bar over the drive in view while it is unpaid and not locked yet: what happens when,
/// how far the copy to this computer got and what it still misses, Renew, and Open the copy.
#[must_use]
pub(crate) fn banner(s: &DriveState, app: &RefAny) -> Option<Dom> {
    let (drive_id, azlin_id, stage) = in_view(s)?;
    if matches!(stage, Stage::Locked { .. }) {
        return None;
    }
    let name = s.drive_name(&Place::folder(&drive_id, ""));
    let mut text = Dom::create_div()
        .with_css("flex-grow: 1; margin-right: 12px; display: flex; flex-direction: column;")
        .with_child(
            Dom::create_span_with_text(AzString::from(t_phrase(&stage_phrase(&name, stage))))
                .with_id(ids::ARCHIVE_TEXT),
        )
        .with_child(
            Dom::create_span_with_text(AzString::from(t_phrase(&promise_phrase())))
                .with_id(ids::ARCHIVE_PROMISE)
                .with_css("font-size: 12px; margin-top: 2px;"),
        );
    let rescue = s.rescues.get(&azlin_id);
    if let Some(phrase) = rescue.and_then(rescue_phrase) {
        text.add_child(
            Dom::create_span_with_text(AzString::from(t_phrase(&phrase)))
                .with_id(ids::ARCHIVE_RESCUE),
        );
    }
    let mut bar = Dom::create_div()
        .with_id(ids::ARCHIVE_BAR)
        .with_css(
            "display: flex; flex-direction: row; align-items: center; padding: 8px 12px; \
             background: #FFF4CE; color: #3B2E00;",
        )
        .with_child(text);
    bar.add_child(button(
        "azdrive-renew",
        app,
        &drive_id,
        on_renew,
        ids::ARCHIVE_RENEW,
    ));
    // read-only: a copy where the user wants it too (the automatic one may lack the room)
    if matches!(stage, Stage::ReadOnly { .. }) {
        bar.add_child(
            crate::ban::copy_everything_button(app, &drive_id).with_id(ids::ARCHIVE_COPY),
        );
    }
    if rescue.is_some() {
        bar.add_child(button(
            "azdrive-archive-open-copy",
            app,
            &drive_id,
            on_open_copy,
            ids::ARCHIVE_OPEN_COPY,
        ));
    }
    Some(bar)
}

/// What the bar says of the copy to this computer.
pub(crate) fn rescue_phrase(r: &Rescue) -> Option<Phrase> {
    let folder = r.folder.display().to_string();
    if let Some((needed, free)) = r.no_space {
        return Some(
            Phrase::new("azdrive-archive-rescue-no-space")
                .arg("needed", azcloud_kit::usage::size_text(needed))
                .arg("free", azcloud_kit::usage::size_text(free)),
        );
    }
    // A record a cut-off run left says how far it got, not "done": the next look goes on.
    if r.running || (r.cut_off() && r.total > 0) {
        return Some(
            Phrase::new("azdrive-archive-rescue-running")
                .arg("folder", folder.as_str())
                .arg("copied", r.copied as u64)
                .arg("total", r.total as u64),
        );
    }
    if !r.missing.is_empty() {
        let names: Vec<&str> = r.missing.iter().take(3).map(String::as_str).collect();
        return Some(
            Phrase::new("azdrive-archive-rescue-missing")
                .arg("missing", r.missing.len() as u64)
                .arg("names", names.join(", "))
                .arg("folder", folder.as_str()),
        );
    }
    (r.total > 0 && r.copied >= r.total)
        .then(|| Phrase::new("azdrive-archive-rescue-done").arg("folder", folder.as_str()))
}

// ==== The locked drive ====

/// What a locked drive in view shows instead of its files: why it is empty, since when, what
/// brings it back, and Get my drive back (or Pay, once it is proven); while its files come back,
/// that they do.
#[must_use]
pub(crate) fn locked_view(s: &DriveState, app: &RefAny) -> Option<Dom> {
    if s.find.is_some() || !matches!(s.place, Place::Folder { .. }) {
        return None;
    }
    let (drive_id, azlin_id, stage) = in_view(s)?;
    let Stage::Locked { wire, .. } = stage else {
        return None;
    };
    let name = s.drive_name(&Place::folder(&drive_id, ""));
    let mut page = Dom::create_div()
        .with_id(ids::ARCHIVE_LOCKED)
        .with_css(
            "display: flex; flex-direction: column; align-items: center; \
             justify-content: center; flex-grow: 1; padding: 40px; font-size: 14px;",
        )
        .with_child(
            Dom::create_span_with_text(AzString::from(t_phrase(&stage_phrase(&name, stage))))
                .with_css("max-width: 560px; margin-bottom: 8px;"),
        )
        .with_child(
            Dom::create_span_with_text(AzString::from(t_phrase(&promise_phrase())))
                .with_id(ids::ARCHIVE_PROMISE)
                .with_css("max-width: 560px; margin-bottom: 16px; font-size: 12px;"),
        );
    if wire.state != ArchiveState::RestorePending {
        let mut row = Dom::create_div().with_css("display: flex; flex-direction: row;");
        if !wire.proven {
            row.add_child(button(
                "azdrive-archive-get-back",
                app,
                &drive_id,
                on_get_back,
                ids::ARCHIVE_GET_BACK,
            ));
        } else {
            row.add_child(button(
                "azdrive-archive-pay",
                app,
                &drive_id,
                on_renew,
                ids::ARCHIVE_PAY,
            ));
        }
        page.add_child(row);
    }
    if s.rescues.contains_key(&azlin_id) {
        page.add_child(
            button(
                "azdrive-archive-open-copy",
                app,
                &drive_id,
                on_open_copy,
                ids::ARCHIVE_OPEN_COPY,
            )
            .with_css("margin-top: 12px;"),
        );
    }
    Some(page)
}

/// What a button of the bar or the page carries: the drive (AzDrive's id).
struct ArchiveRef {
    app: RefAny,
    drive_id: String,
}

fn button(
    key: &str,
    app: &RefAny,
    drive_id: &str,
    on_click: ButtonOnClickCallbackType,
    id: AzString,
) -> Dom {
    Button::create(label(key))
        .with_on_click(
            RefAny::new(ArchiveRef {
                app: app.clone(),
                drive_id: drive_id.to_string(),
            }),
            on_click,
        )
        .dom()
        .with_id(id)
}

fn clicked(data: &mut RefAny) -> Option<(RefAny, String)> {
    data.downcast_ref::<ArchiveRef>()
        .map(|r| (r.app.clone(), r.drive_id.clone()))
}

/// Renew (and Pay, for a proven locked drive): Buy storage for the drive's next months.
extern "C" fn on_renew(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, drive_id)) = clicked(&mut data) else {
        return Update::DoNothing;
    };
    crate::with_state(&mut app, &mut info, |info, app, s| {
        crate::renewals::open_renew(info, app, s, &drive_id);
    })
}

/// Open the copy: the drive's copy on this computer, in AzDrive.
extern "C" fn on_open_copy(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, drive_id)) = clicked(&mut data) else {
        return Update::DoNothing;
    };
    crate::with_state(&mut app, &mut info, |info, app, s| {
        let name = folder_name(s, &drive_id);
        let prefix = format!("{RESCUE_DIR}/{name}/");
        crate::go(info, app, s, Place::folder(HOME_ID, &prefix), true);
    })
}

/// Get my drive back: this computer's device key signs the token server's challenge; on a
/// computer without one the recovery code is asked for (encrypted drives keep it).
extern "C" fn on_get_back(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, drive_id)) = clicked(&mut data) else {
        return Update::DoNothing;
    };
    crate::with_state(&mut app, &mut info, |info, app, s| {
        prove_start(info, app, s, &drive_id, ProveWith::DeviceKey, None);
    })
}

/// A renewal of AzDrive's (or the Azlin) drive `drive_id` was paid: when the drive is locked and
/// not on its way back yet, the proof is sent again naming the payment `checkout_id`, so its
/// months pay what the drive owes at once (its period tokens would come one a month).
pub(crate) fn renewed(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    checkout_id: &str,
) {
    let azlin_id = if s.archives.contains_key(drive_id) {
        drive_id.to_string()
    } else {
        match azlin_of(s, drive_id) {
            Some(id) => id,
            None => return,
        }
    };
    let pending = match s.archives.get(&azlin_id).map(|seen| &seen.stage) {
        Some(Stage::Locked { wire, .. }) => wire.state == ArchiveState::RestorePending,
        _ => return,
    };
    if pending {
        return;
    }
    let Some(slot) = slot_of(s, &azlin_id) else {
        return;
    };
    prove_start(
        info,
        app,
        s,
        &slot,
        ProveWith::DeviceKey,
        Some(checkout_id.to_string()),
    );
}

/// This computer's own key of AzDrive's drive `drive_id` registered for restores, now that this
/// computer holds the drive (it joined it, set up its encryption or unlocked it); a job. Once: a
/// key registered before answers its id without asking. A device key proves ownership for a
/// restore only, never a recovery lockdown.
pub(crate) fn register_device_key(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
) {
    let fallback = s.token.url.clone();
    let Some((azlin_id, token_url)) = s
        .slot_index(drive_id)
        .and_then(|index| crate::periods::azlin_drive(&s.slots[index].entry, fallback.as_deref()))
    else {
        return;
    };
    let job = Job::RegisterDeviceKey {
        token_url,
        keyring: s.keyring.clone(),
        drive_id: azlin_id,
    };
    spawn(info, app, s, job);
}

/// The registration of this computer's key, run in a job: its id (`dk_...`).
pub(crate) fn register_device_key_now(
    token_url: &str,
    keyring: &SharedKeyring,
    drive_id: &str,
) -> Result<String, String> {
    let transport = AzulTransport::azlin(crate::USER_AGENT);
    let server = TokenServer::new(token_url, &transport).map_err(|e| e.to_string())?;
    azcloud_kit::device_key::ensure_registered(&server, keyring, drive_id)
        .map_err(|e| e.to_string())
}

/// How the owner proves a locked drive is theirs.
pub(crate) enum ProveWith {
    /// This computer's own device key (for restores only).
    DeviceKey,
    /// The drive's recovery code, typed.
    #[cfg(feature = "encryption")]
    RecoveryCode(azul_storage::crypto::keys::RecoveryCode),
}

/// Starts a proof of AzDrive's drive `drive_id` (a job: the token server's statement, signed),
/// naming the paid `checkout` it unlocks when there is one.
pub(crate) fn prove_start(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    via: ProveWith,
    checkout: Option<String>,
) {
    let fallback = s.token.url.clone();
    let Some((azlin_id, token_url)) = s
        .slot_index(drive_id)
        .and_then(|index| crate::periods::azlin_drive(&s.slots[index].entry, fallback.as_deref()))
    else {
        return;
    };
    let name = s.drive_name(&Place::folder(drive_id, ""));
    s.info(Phrase::new("azdrive-archive-proving").arg("drive", name.as_str()));
    let job = Job::ArchiveProve {
        token_url,
        keyring: s.keyring.clone(),
        drive_id: azlin_id,
        via,
        checkout,
    };
    spawn(info, app, s, job);
}

/// The message key a refused proof is said with; anything else as the token server said it.
const NO_DEVICE_KEY: &str = "azdrive-archive-no-key-here";

/// A proof of the locked drive `drive_id` (its Azlin id) at `token_url`, naming the paid
/// `checkout` it unlocks: under the drive's keyring lock with its newest drive token, the token
/// server's statement signed with `via`. Runs in a job.
pub(crate) fn prove(
    token_url: &str,
    keyring: &SharedKeyring,
    drive_id: &str,
    via: &ProveWith,
    checkout: Option<&str>,
) -> Result<ProofAnswer, String> {
    let transport = AzulTransport::azlin(crate::USER_AGENT);
    let server = TokenServer::new(token_url, &transport).map_err(|e| e.to_string())?;
    let answered = keyring.with_drive_token(drive_id, |token| match via {
        ProveWith::DeviceKey => {
            let key = azcloud_kit::DeviceKey::load(keyring, drive_id)
                .map_err(|e| e.to_string())?
                .filter(|key| key.key_id().is_some())
                .ok_or_else(|| String::from(NO_DEVICE_KEY))?;
            server
                .archive_prove(drive_id, token, checkout, key.key_id(), |message| {
                    Ok(key.sign_base64(message))
                })
                .map_err(proof_error)
        }
        #[cfg(feature = "encryption")]
        ProveWith::RecoveryCode(code) => {
            let key = crate::encryption::recovery_key_of(code, drive_id);
            let findable = crate::recovery_keys::findable_key_of(code);
            server
                .archive_prove(drive_id, token, checkout, None, |message| {
                    Ok(key.sign_base64(message))
                })
                .or_else(|e| match e.archive_refusal() {
                    // not the drive key: the code's findable key (a second kit registers only it)
                    Some(ArchiveRefusal::BadSignature) => {
                        server.archive_prove(drive_id, token, checkout, None, |message| {
                            Ok(findable.sign_base64(message))
                        })
                    }
                    _ => Err(e),
                })
                .map_err(proof_error)
        }
    });
    answered.map_err(|e| e.to_string())?
}

/// A refused proof's words: the contract's refusals of the proof as message keys.
fn proof_error(e: TokenError) -> String {
    match e.archive_refusal() {
        Some(ArchiveRefusal::NoKey) => String::from("azdrive-archive-no-key-at-all"),
        Some(ArchiveRefusal::BadSignature) => String::from("azdrive-archive-wrong-key"),
        Some(ArchiveRefusal::BadChallenge | ArchiveRefusal::ChallengeUsed) => {
            String::from("azdrive-archive-try-again")
        }
        Some(ArchiveRefusal::NotArchived) => String::from("azdrive-archive-not-locked"),
        Some(ArchiveRefusal::CheckoutNotPaid) => String::from("azdrive-archive-not-paid"),
        Some(ArchiveRefusal::CheckoutOtherTier) => String::from("azdrive-archive-other-tier"),
        Some(
            ArchiveRefusal::BadRequest | ArchiveRefusal::NotVerified | ArchiveRefusal::Unknown,
        )
        | None => e.to_string(),
    }
}

/// A proof answered for the Azlin drive `azlin_id` (naming the paid `checkout`, or none): proven
/// and paid with its files still on the servers - open again; proven - Renew opens to pay what
/// it owes (unless the proof named a payment already); its files on their way back - said;
/// refused - why (on a computer without its own key, the recovery code is asked for).
pub(crate) fn proven(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    azlin_id: &str,
    checkout: Option<String>,
    result: Result<ProofAnswer, String>,
) {
    let drive_id = slot_of(s, azlin_id).unwrap_or_else(|| azlin_id.to_string());
    let name = s.drive_name(&Place::folder(&drive_id, ""));
    match result {
        Ok(ProofAnswer::Restored) => {
            println!("AZDRIVE_ARCHIVE_PROVEN {azlin_id} restored");
            s.archives.remove(azlin_id);
            back(info, s, azlin_id);
            crate::periods::start_redemptions(info, app, s, Some(azlin_id));
        }
        Ok(ProofAnswer::Locked(wire)) => {
            let period_until = s.archives.get(azlin_id).and_then(|seen| match &seen.stage {
                Stage::Locked { period_until, .. } => *period_until,
                _ => None,
            });
            let pending = wire.state == ArchiveState::RestorePending;
            println!(
                "AZDRIVE_ARCHIVE_PROVEN {azlin_id} {}",
                if pending { "restore_pending" } else { "locked" }
            );
            let stage = Stage::Locked {
                wire: wire.clone(),
                period_until,
            };
            s.success(stage_phrase(&name, &stage));
            s.archives.insert(azlin_id.to_string(), Seen { stage });
            if !pending && checkout.is_none() {
                crate::renewals::open_renew(info, app, s, &drive_id);
            }
        }
        Err(why) if why == NO_DEVICE_KEY => {
            #[cfg(feature = "encryption")]
            crate::encryption::ask_get_back(s, &drive_id, checkout);
            #[cfg(not(feature = "encryption"))]
            {
                let _ = checkout;
                s.error(Phrase::new(NO_DEVICE_KEY).arg("drive", name.as_str()));
            }
        }
        Err(why) if why.starts_with("azdrive-") => {
            s.error(Phrase::new(&why).arg("drive", name.as_str()));
        }
        Err(why) => s.error(
            Phrase::new("azdrive-archive-proof-failed")
                .arg("drive", name.as_str())
                .arg("why", why.as_str()),
        ),
    }
}

// ==== The copy to this computer ====

/// The copy of a read-only drive to this computer, as far as it got.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Rescue {
    /// `<home>/Azlin rescue/<drive>`.
    pub folder: PathBuf,
    pub total: usize,
    pub copied: usize,
    /// The files not on this computer yet (their keys), when the last copy ended.
    pub missing: Vec<String>,
    /// The disk had too little room: what the copy needed, and what was free.
    #[serde(default)]
    pub no_space: Option<(u64, u64)>,
    /// When the last copy ended (seconds since 1970); 0 in the record a run keeps while it is still
    /// copying - a run that is cut off leaves that one, and it is not "done".
    #[serde(default)]
    pub at: u64,
    /// What the copy wrote, key by key: the length and the time of the file as it was written. A
    /// file of the copy that has another length or time was changed by its user: the copy never
    /// touches it again (AZ-U6).
    #[serde(default)]
    pub written: BTreeMap<String, Written>,
    /// The files that are on this computer already and are not what the copy wrote (a file of
    /// the user's own, an edit of a file the copy wrote): left exactly as they are.
    #[serde(default)]
    pub kept: Vec<String>,
    /// What the last run went on without (a check whose probe could not answer: the disk's free
    /// space): the report says what was not checked.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    #[serde(skip)]
    pub running: bool,
}

impl Rescue {
    /// Whether this is the record of a run that did not end: kept part-way (no end time) or with
    /// fewer files copied than the drive has and nothing said to be missing.
    pub(crate) fn cut_off(&self) -> bool {
        self.no_space.is_none()
            && self.missing.is_empty()
            && (self.at == 0 || self.copied < self.total)
    }
}

/// A file as the copy wrote it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Written {
    pub len: u64,
    /// Its modification time, nanoseconds since 1970.
    pub mtime_ns: u64,
    /// The drive's own version of the file the copy was made from, as the listing named it (the
    /// entity tag, the date): a file of the drive written to again with the length it had is told
    /// from the one that was copied by this, never by its size (X-DELTA-AZDRIVE-FILES-17). Nothing
    /// in a ledger an older AzDrive wrote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<u64>,
    /// The name below the copy's folder the file was written to. Two keys a disk reads as one name
    /// ("report.txt", "Report.txt") get names of their own; a key that arrives later never takes the
    /// name of a file that is there (X-DELTA-AZDRIVE-FILES-17). Empty in an older ledger.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target: String,
}

impl Written {
    /// Whether the drive's file `o` is the version this was copied from: `Some(true)` the same,
    /// `Some(false)` another, `None` when the drive names no version of one of them that the other
    /// names too (nothing is then known).
    fn same_version(&self, o: &ObjectInfo) -> Option<bool> {
        fn bare(tag: &str) -> &str {
            tag.trim().trim_matches('"')
        }
        if let (Some(then), Some(now)) = (&self.etag, &o.etag) {
            return Some(bare(then) == bare(now));
        }
        match (self.modified, o.modified) {
            (Some(then), Some(now)) => Some(then == now),
            _ => None,
        }
    }

    /// Whether a version of the drive's file is recorded at all.
    fn has_version(&self) -> bool {
        self.etag.is_some() || self.modified.is_some()
    }
}

/// One file the copy landed, written down at once in a file of its own beside the ledger
/// ([`note_landed`]): a kill or a power cut loses the file in flight, not the up to 12,500 the ledger's
/// whole writes were spread over - and the next run took those copies for files nobody recorded
/// ("Have" by size, or "the user's own") and never refreshed them while the bar said done
/// (X-DELTA-SYNC2-10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Landed {
    key: String,
    written: Written,
}

/// The folder the landed files are written down in: beside the ledger `state`.
pub(crate) fn landed_dir(state: &Path) -> PathBuf {
    let mut name = state.as_os_str().to_os_string();
    name.push(".landed");
    PathBuf::from(name)
}

/// The landed files of `state`'s folder, in the order they landed (their names are their numbers).
fn landed_names(state: &Path) -> Vec<(u64, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(landed_dir(state)) else {
        return Vec::new();
    };
    let mut found: Vec<(u64, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let number = path.file_stem()?.to_str()?.parse::<u64>().ok()?;
            (path.extension()? == "json").then_some((number, path))
        })
        .collect();
    found.sort();
    found
}

/// Writes the file `key` down as landed, as the `number`th of the folder: one small file of its own,
/// put in place whole ([`durable::replace`]), so that what a kill leaves is every file but the one in
/// flight. `Err` says it could not be written: the file is then known only by the next whole write of
/// the ledger, as before.
fn note_landed(state: &Path, number: u64, key: &str, written: &Written) -> std::io::Result<()> {
    let landed = Landed {
        key: key.to_string(),
        written: written.clone(),
    };
    let text = serde_json::to_string(&landed).map_err(std::io::Error::other)?;
    durable::replace(
        &landed_dir(state).join(format!("{number:012}.json")),
        text.as_bytes(),
        durable::Mode::Private,
    )
}

/// What the landed files say, oldest first: a key that landed twice is as its last file says.
fn read_landed(state: &Path) -> Vec<Landed> {
    landed_names(state)
        .into_iter()
        // collapse-ok: a landed file that does not read (cut in two by the same crash) is a file
        // the next run looks at again, which is what a file nobody wrote down is.
        .filter_map(|(_, path)| serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok())
        .collect()
}

/// The landed files go once the ledger holds them: after a whole write of it that took, and in
/// the order they landed, so that a failure in between leaves the later ones.
fn forget_landed(state: &Path) {
    for (_, path) in landed_names(state) {
        // collapse-ok: a landed file that stays is read again by the next run, and says what the
        // ledger says.
        if std::fs::remove_file(path).is_err() {
            break;
        }
    }
}

/// How long a copy goes between two whole writes of its ledger at most, whatever the count says.
pub(crate) const KEEP_EVERY: std::time::Duration = std::time::Duration::from_secs(60);

/// A drive's name as a folder name (no separators, never empty).
fn copy_name(name: &str) -> String {
    crate::ban::copy_prefix(name)
        .trim_end_matches('/')
        .to_string()
}

/// The folder of the copy of AzDrive's drive `drive_id` in `<home>/Azlin rescue/`: the drive's name,
/// and - when another Azlin drive has the same name or one a disk reads as the same - the start of
/// its id after it. Two drives called "Photos" shared one folder and wrote into each other's
/// files (AZ-U6).
pub(crate) fn folder_name(s: &DriveState, drive_id: &str) -> String {
    let name = copy_name(&s.drive_name(&Place::folder(drive_id, "")));
    let others: Vec<String> = s
        .slots
        .iter()
        .filter(|slot| slot.entry.id != drive_id && slot.entry.azlin().is_some())
        .map(|slot| copy_name(&s.drive_name(&Place::folder(&slot.entry.id, ""))))
        .collect();
    let azlin_id = azlin_of(s, drive_id).unwrap_or_default();
    folder_name_among(&name, &azlin_id, &others)
}

/// [`folder_name`] of a drive named `name` (as a folder name) with the Azlin id `azlin_id`, among
/// the folder names `others` of the other Azlin drives.
pub(crate) fn folder_name_among(name: &str, azlin_id: &str, others: &[String]) -> String {
    let folded = names::fold(name, &STRONGEST);
    if !others
        .iter()
        .any(|other| names::fold(other, &STRONGEST) == folded)
    {
        return name.to_string();
    }
    let id: String = azlin_id
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect();
    if id.is_empty() {
        name.to_string()
    } else {
        format!("{name} ({id})")
    }
}

/// The names as the strongest disk compares them: case and Unicode form folded.
const STRONGEST: Volume = Volume {
    case_insensitive: true,
    norm_insensitive: true,
    windows_names: false,
    max_file: None,
    no_exec_bits: false,
};

/// The home folder (the This PC entry of the user's home).
fn home(s: &DriveState) -> Option<PathBuf> {
    s.slots
        .iter()
        .find(|slot| slot.entry.id == HOME_ID)
        .and_then(|slot| match &slot.entry.location {
            azul_storage::config::DriveLocation::Local { root } => Some(PathBuf::from(root)),
            _ => None,
        })
}

/// Where the copy of the Azlin drive `azlin_id` is kept between runs: `rescue/` beside the drives
/// file, with the other state of the computer - never the system's temporary folder. `None` without
/// a drives file: the copy then runs without a memory of the last one (it never overwrites a file
/// it did not just write).
fn rescue_file(s: &DriveState, azlin_id: &str) -> Option<PathBuf> {
    rescue_file_of(s.drives_file.as_deref()?, azlin_id)
}

/// [`rescue_file`] from the drives file's path alone: the one place the name is spelled. Remove
/// drive asks it (`footprint.rs`): the state lists every key of the drive's copy.
pub(crate) fn rescue_file_of(drives_file: &Path, azlin_id: &str) -> Option<PathBuf> {
    let file: String = azlin_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    let beside = drives_file.parent()?;
    Some(beside.join("rescue").join(format!("{file}.rescue.json")))
}

/// Sets the copy's state `path`, which is there and could not be read, aside whole.
fn set_aside_unread(path: &Path) -> Option<Rescue> {
    // collapse-ok: a state that cannot even be moved is overwritten by the next run, as it was before;
    // the copy never overwrites a file it did not write.
    let _aside = durable::set_aside(path, &now_secs().to_string());
    None
}

/// The copy's state the last run left (none before a first copy). A state that is there and does
/// not read is no state - but it is not lost: it is set aside whole (`<name>.damaged-<time>`), so
/// the next run's state does not replace the only record of what the copy wrote
/// (X-DELTA-AZDRIVE-FILES-34).
fn rescue_read(path: &Path) -> Option<Rescue> {
    let ledger = match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<Rescue>(&text) {
            Ok(rescue) => Some(rescue),
            Err(_) => set_aside_unread(path),
        },
        // No state before a first copy.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        // A state that is there and cannot be read - not UTF-8, a failing read, no permission - is no
        // "no state" either: the next whole write of the ledger would replace the only record of what
        // the copy wrote (X-DELTA-AZDRIVE-FILES-34).
        Err(_) => set_aside_unread(path),
    };
    // The files that landed after the ledger's last whole write: what a run that was cut off had
    // written down one by one (X-DELTA-SYNC2-10). A cut-off run before its first whole write has
    // only these.
    let landed = read_landed(path);
    if landed.is_empty() {
        return ledger;
    }
    let mut rescue = ledger.unwrap_or_default();
    for file in landed {
        rescue.written.insert(file.key, file.written);
    }
    Some(rescue)
}

/// After a look: every read-only drive whose copy is not running and is due (none yet, files
/// missing, or a day since the last one) is copied again; files already there are skipped.
pub(crate) fn after_look(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(home) = home(s) else {
        return;
    };
    let now = now_secs();
    let read_only: Vec<String> = s
        .archives
        .iter()
        .filter(|(_, seen)| matches!(seen.stage, Stage::ReadOnly { .. }))
        .map(|(id, _)| id.clone())
        .collect();
    for azlin_id in read_only {
        let state = rescue_file(s, &azlin_id);
        if !s.rescues.contains_key(&azlin_id) {
            if let Some(kept) = state.as_deref().and_then(rescue_read) {
                s.rescues.insert(azlin_id.clone(), kept);
            }
        }
        if !rescue_due(s.rescues.get(&azlin_id), now) {
            continue;
        }
        let Some(drive_id) = slot_of(s, &azlin_id) else {
            continue;
        };
        let Some(source) = crate::open_drive(s, &drive_id) else {
            continue;
        };
        // The copy reads through a drive that stops answering when the drive is removed
        // (X-DELTA-AZDRIVE-KEYS-32): its credentials are not used for a drive that is gone.
        let stop = Arc::new(AtomicBool::new(false));
        s.rescue_stops.insert(azlin_id.clone(), Arc::clone(&stop));
        let source: Arc<dyn Drive> = Arc::new(Stops {
            inner: source,
            stop,
        });
        let folder = home.join(RESCUE_DIR).join(folder_name(s, &drive_id));
        let entry = s.rescues.entry(azlin_id.clone()).or_default();
        entry.folder = folder.clone();
        entry.running = true;
        println!("AZDRIVE_RESCUE_START {azlin_id} {}", folder.display());
        let job = Job::Rescue {
            drive_id: azlin_id,
            source,
            folder,
            home: home.clone(),
            state,
        };
        spawn(info, app, s, job);
    }
}

/// A drive that stops answering once its flag is raised (`Cancelled`): what the copy of a locked
/// drive to this computer reads through, so that the removal of the drive can end the copy.
pub(crate) struct Stops {
    pub inner: Arc<dyn Drive>,
    pub stop: Arc<AtomicBool>,
}

impl Stops {
    fn alive(&self) -> Result<(), DriveError> {
        if self.stop.load(Ordering::SeqCst) {
            Err(DriveError::Cancelled)
        } else {
            Ok(())
        }
    }
}

impl Drive for Stops {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.alive()?;
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.alive()?;
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.alive()?;
        self.inner.get_range(key, range)
    }
    // The inner drive's guard stands: a drive that wraps another forwards the limit.
    fn get_limited(&self, key: &str, limit: Limit) -> Result<Vec<u8>, DriveError> {
        self.alive()?;
        self.inner.get_limited(key, limit)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.alive()?;
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.alive()?;
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.alive()?;
        self.inner.head(key)
    }
}

/// Whether the copy of a drive whose last state is `last` (none before a first copy) runs now: it
/// is not running, and it has not ended well yet - files missing, too little room, a record kept
/// part-way by a run that was cut off (no end time, or fewer files than the drive has) - or a day
/// passed since it ended (X-DELTA-AZDRIVE-FILES-18: the record of a cut run said "done" and
/// waited a day).
pub(crate) fn rescue_due(last: Option<&Rescue>, now: u64) -> bool {
    last.is_none_or(|r| {
        !r.running
            && (!r.missing.is_empty()
                || r.no_space.is_some()
                || r.cut_off()
                || now >= r.at.saturating_add(DAY))
    })
}

/// How far a running copy got.
pub(crate) fn rescue_progress(s: &mut DriveState, azlin_id: &str, copied: usize, total: usize) {
    if let Some(r) = s.rescues.get_mut(azlin_id) {
        r.copied = copied;
        r.total = total;
    }
}

/// A copy ended: kept for the bar, said on stdout. A copy whose drive was removed while it ran
/// (the removal raised its stop: [`Stops`]) leaves nothing of itself: the state it kept on the way
/// goes (the removal took the file first, and the run wrote it again at every step) and the bar is
/// not told (X-DELTA-AZDRIVE-KEYS-32). The files it copied are the user's and stay.
pub(crate) fn rescue_done(s: &mut DriveState, azlin_id: &str, report: Rescue) {
    s.rescue_stops.remove(azlin_id);
    println!(
        "AZDRIVE_RESCUE {azlin_id} {} {} {}",
        report.copied,
        report.total,
        report.missing.len()
    );
    if slot_of(s, azlin_id).is_none() {
        s.rescues.remove(azlin_id);
        if let Some(file) = rescue_file(s, azlin_id) {
            if let Err(e) = std::fs::remove_file(&file) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!(
                        "AZDRIVE_RESCUE_STATE_STAYS {azlin_id}: {}: {e}",
                        file.display()
                    );
                }
            }
        }
        return;
    }
    s.rescues.insert(azlin_id.to_string(), report);
}

/// Where the file `key` goes under `folder`: each part of the key a plain name this computer's
/// disk takes as it is meant (no `..`, no empty part, and on Windows no `D:Docs` that replaces
/// the base path, device name, stream or trailing dot), or `None` for a key that cannot be a
/// path here.
#[must_use]
pub(crate) fn target_of(folder: &Path, key: &str) -> Option<PathBuf> {
    target_of_on(folder, key, System::here())
}

/// [`target_of`] for a disk of `system` (a test asks for Windows' rules on any computer).
///
/// The keys are other devices' words: `D:Docs/x` pushed onto a `PathBuf` on Windows is a path
/// relative to drive D's current folder that REPLACES the base (the rename then overwrites a
/// file outside the copy), `nul.txt` opens a device and `a.txt:s` writes a stream. The one name
/// table ([`azul_storage::key`]) says which keys a disk of `system` reads as something else
/// (AZ-W2).
#[must_use]
pub(crate) fn target_of_on(folder: &Path, key: &str, system: System) -> Option<PathBuf> {
    // The name policy admits it (also the protected names in the spellings a disk reads as the
    // real ones); only a key it let through becomes a path (X-AR5a-17).
    let policy = Policy {
        system,
        volume: Volume::default(),
    };
    policy.admit(key, Purpose::Rescue).ok().map(|admitted| {
        let mut path = folder.to_path_buf();
        for name in admitted.as_str().split('/') {
            path.push(rescued_name(name).as_ref());
        }
        path
    })
}

/// The name a name of the drive has in the rescue copy: a repository's own folder (`.git`, `.hg`,
/// `.svn`) is copied - it is the user's data - but not as one. `proj/.git/config` of a shared drive
/// is a file another participant wrote, and a `core.fsmonitor` in it runs the next time git is run
/// in the copy; as `proj/_.git/config` git does not read it, and the user renames it back when
/// they have looked (X-DELTA-AZDRIVE-FILES-32).
fn rescued_name(name: &str) -> std::borrow::Cow<'_, str> {
    if names::Protected::is_vcs(name) {
        std::borrow::Cow::Owned(format!("_{name}"))
    } else {
        std::borrow::Cow::Borrowed(name)
    }
}

/// `key` with each name of it as the rescue copy spells it ([`rescued_name`]).
fn rescued_key(key: &str) -> String {
    key.split('/')
        .map(rescued_name)
        .collect::<Vec<_>>()
        .join("/")
}

/// Where each of `keys` goes under `folder`, parallel to `keys`: [`target_of`], except that two
/// keys a disk would take for one name - case (APFS, NTFS) or Unicode form (APFS, HFS+) - get
/// names of their own (`Report.txt`, `report (2).txt`), whatever disk the copy is made on: it
/// may be moved to such a disk, and a copy that loses a file when it is must not be made
/// (AZ-W2). Keys are taken in the order given; the caller sorts them. A key an earlier run wrote
/// keeps the name it got (`kept`, parallel to `keys`), and the names of earlier files whose keys
/// are not among `keys` any more (`held`) are taken: a twin that comes later gets the number, not
/// the earlier file's name (X-DELTA-AZDRIVE-FILES-17).
fn targets_of(
    folder: &Path,
    keys: &[&str],
    kept: &[Option<&str>],
    held: &[&str],
) -> Vec<Option<PathBuf>> {
    // The names as the copy spells them, then told apart: a drive's `_.git` and its `.git` (copied
    // as `_.git`) are two files.
    let rescued: Vec<String> = keys.iter().map(|key| rescued_key(key)).collect();
    let rescued: Vec<&str> = rescued.iter().map(String::as_str).collect();
    azul_storage::key::without_twins_keeping(&rescued, kept, held, &|name| {
        names::fold(name, &STRONGEST)
    })
    .iter()
    .map(|key| target_of(folder, key))
    .collect()
}

/// The key of `path` below `folder`, in the form the ledger keeps it (`/` between the parts), or
/// `None` for a path that is not below it or a name that is not text.
fn key_under(folder: &Path, path: &Path) -> Option<String> {
    let parts: Vec<String> = path
        .strip_prefix(folder)
        .ok()?
        .components()
        .map(|c| c.as_os_str().to_str().map(str::to_string))
        .collect::<Option<_>>()?;
    Some(parts.join("/"))
}

/// The free bytes of the disk `path` is on, or why the system did not say.
pub(crate) fn free_bytes(path: &Path) -> Cap<u64> {
    let text = path.to_string_lossy().into_owned();
    match FilePath::create(text.as_str()).disk_space().into_option() {
        Some(space) => Cap::Known(space.free),
        None => Cap::Unknown("the system did not tell the free space of this disk"),
    }
}

/// What the rescue copy's look at the disk's room does when the disk cannot say how much is free:
/// the copy begins anyway. The look only saves a long copy that cannot end - a file the disk
/// cannot take fails by itself and is listed in `missing`, which the bar shows - and failing
/// closed would refuse a rescue on exactly the computer that cannot answer.
const ROOM_POSTURE: Posture = Posture::Degrade(
    "the free space of the disk cannot be read: the copy begins without the look at its room",
);

/// What the copy does with one file of the drive.
enum Step {
    /// It is on this computer already, as the copy wrote it or with the size of the drive's.
    Have,
    /// It is copied: the name must hold what this says when the file is put in place.
    Copy(Expect),
    /// Something is there that is not what the copy wrote - the user's own file, an edit of a
    /// file the copy wrote, a folder: left as it is.
    Kept,
    /// The key is no path of the copy, or the name could not be looked at.
    Missing,
}

/// The nanoseconds since 1970 of a file's modification.
fn mtime_ns(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
}

/// What to do with the object `o` of the drive, to be at `target`, given what an earlier run
/// wrote (`written`). A file is replaced only if it is, for certain, the file the copy wrote and
/// nobody touched; the user's edit is the only copy of the edit (AZ-U6).
fn step(written: Option<&Written>, target: Option<&Path>, o: &ObjectInfo) -> Step {
    let Some(path) = target else {
        return Step::Missing;
    };
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Step::Copy(Expect::Absent),
        Err(_) => return Step::Missing,
    };
    if !meta.is_file() {
        return Step::Kept;
    }
    match written {
        // Ours, and unchanged since: the drive's version decides - its own version, where it names
        // one. A file of the drive written to again with the length it had (a database, a disk
        // image, a fixed-size container) is another version; a size alone is not one
        // (X-DELTA-AZDRIVE-FILES-17). A drive that names none, then or now, has only the size.
        Some(w) if w.len == meta.len() && w.mtime_ns == mtime_ns(&meta) => {
            let same = o.size == w.len
                && match w.same_version(o) {
                    Some(same) => same,
                    // Nothing to compare: the size is all there is only when the ledger recorded
                    // no version either (an older ledger, or a drive that names none). A version
                    // that was named and is not any more is not "the same".
                    None => !w.has_version(),
                };
            if same {
                return Step::Have;
            }
            match beneath::stat_path(path) {
                Ok(stat) => Step::Copy(Expect::Is(stat.fingerprint())),
                Err(_) => Step::Kept,
            }
        }
        // Ours once, changed by its user since.
        Some(_) => Step::Kept,
        // Not written by this copy (an earlier version of AzDrive, or the user): the size of the
        // drive's file makes it that file; anything else is the user's.
        None if meta.len() == o.size => Step::Have,
        None => Step::Kept,
    }
}

/// Copies every file of `source` into `folder` (a job): the files already there skipped, the
/// rest piece by piece; `progress` hears how far it got. Too little room (`free`: the bytes free
/// on the disk, when known; an unknown is [`ROOM_POSTURE`]'s) stops it before it begins. What it
/// came to, kept in `state` for the next start.
///
/// A file is written only to a name nobody has, or over the copy this run wrote earlier when
/// nobody touched it since - the length and time of every file written are kept, and a file that
/// differs is its user's edit and is never overwritten, however the drive's file changed
/// (AZ-U6). Every file goes in through the one write path: beside its name, synced, renamed in.
pub(crate) fn run_rescue(
    source: &dyn Drive,
    folder: &Path,
    free: Cap<u64>,
    state: Option<&Path>,
    progress: &mut dyn FnMut(usize, usize),
) -> Rescue {
    let earlier = state.and_then(rescue_read).unwrap_or_default();
    // `at` stays 0 while the copy runs: the record kept on the way is a run that did not end
    // (X-DELTA-AZDRIVE-FILES-18); `keep_report` says when it ended.
    let mut report = Rescue {
        folder: folder.to_path_buf(),
        written: earlier.written,
        ..Rescue::default()
    };
    let objects = match azul_storage::ops::list_all(source, "") {
        Ok(objects) => objects,
        Err(e) => {
            report.missing.push(format!("(the drive's list: {e})"));
            return keep_report(report, state);
        }
    };
    let mut files: Vec<&ObjectInfo> = objects.iter().filter(|o| !o.key.ends_with('/')).collect();
    // The same names every run: a file that was copied under an alias is found under it again.
    files.sort_by(|a, b| a.key.cmp(&b.key));
    report.total = files.len();
    let keys: Vec<&str> = files.iter().map(|o| o.key.as_str()).collect();
    // The names an earlier run gave stay with their files, and the names of files that are in the
    // folder but not on the drive any more are not given to another key (X-DELTA-AZDRIVE-FILES-17).
    let present: std::collections::HashSet<&str> = keys.iter().copied().collect();
    let kept: Vec<Option<&str>> = keys
        .iter()
        .map(|key| report.written.get(*key).map(|w| w.target.as_str()))
        .collect();
    let held: Vec<&str> = report
        .written
        .iter()
        .filter(|(key, _)| !present.contains(key.as_str()))
        .map(|(_, w)| w.target.as_str())
        .filter(|target| !target.is_empty())
        .collect();
    let targets = targets_of(folder, &keys, &kept, &held);
    let steps: Vec<Step> = files
        .iter()
        .zip(&targets)
        .map(|(o, target)| step(report.written.get(&o.key), target.as_deref(), o))
        .collect();
    let needed: u64 = files
        .iter()
        .zip(&steps)
        .filter(|(_, step)| matches!(step, Step::Copy(_)))
        .map(|(o, _)| o.size)
        .sum();
    match (free, ROOM_POSTURE) {
        (Cap::Known(free), _) => {
            if needed + SPARE_BYTES > free {
                report.no_space = Some((needed, free));
                report.missing = files
                    .iter()
                    .zip(&steps)
                    .filter(|(_, step)| matches!(step, Step::Copy(_) | Step::Missing))
                    .map(|(o, _)| o.key.clone())
                    .collect();
                report.copied = files.len() - report.missing.len();
                return keep_report(report, state);
            }
        }
        (Cap::Unknown(why), Posture::Degrade(note)) => {
            report.notes.push(format!("{note} ({why})"));
        }
        (Cap::Unknown(_), Posture::FailClosed) => {
            report.missing = files
                .iter()
                .zip(&steps)
                .filter(|(_, step)| matches!(step, Step::Copy(_) | Step::Missing))
                .map(|(o, _)| o.key.clone())
                .collect();
            return keep_report(report, state);
        }
    }
    let target_drive = LocalDrive::without_manifest(folder);
    let mut since_kept = 0usize;
    let mut last_kept = std::time::Instant::now();
    // The next landed file's number: after the ones an earlier cut-off run left, which are replayed
    // into the ledger (`rescue_read`) and go with its next whole write.
    let mut landed_number = state
        .and_then(|state| landed_names(state).last().map(|(number, _)| number + 1))
        .unwrap_or(1);
    // Files that were in the folder with no record of the copy having written them: taken as they are.
    let mut unrecorded = 0usize;
    for (n, ((o, target), step)) in files.iter().zip(&targets).zip(steps).enumerate() {
        match step {
            Step::Have => {
                report.copied += 1;
                if !report.written.contains_key(&o.key) {
                    unrecorded += 1;
                }
                // A file of the copy that is as the copy wrote it: what an older ledger did not
                // record of it (the drive's version, its name here) is recorded now, from the file
                // that was just looked at.
                let name = target.as_deref().and_then(|path| key_under(folder, path));
                if let Some(w) = report.written.get_mut(&o.key) {
                    if !w.has_version() {
                        w.etag.clone_from(&o.etag);
                        w.modified = o.modified;
                    }
                    if w.target.is_empty() {
                        w.target = name.unwrap_or_default();
                    }
                }
            }
            Step::Kept => {
                report.copied += 1;
                if !report.written.contains_key(&o.key) {
                    unrecorded += 1;
                }
                report.kept.push(o.key.clone());
            }
            Step::Missing => report.missing.push(o.key.clone()),
            Step::Copy(expect) => {
                let landed = target
                    .as_deref()
                    .and_then(|path| Some((path, key_under(folder, path)?)));
                let result = landed.as_ref().map(|(_, key)| {
                    transfer::copy_object_guarded(
                        source,
                        &o.key,
                        Some(o.size),
                        &target_drive,
                        key,
                        expect,
                        &Quiet,
                    )
                });
                match (result, landed) {
                    (Some(Ok(len)), Some((path, name))) => {
                        report.copied += 1;
                        let mtime = std::fs::metadata(path).map_or(0, |meta| mtime_ns(&meta));
                        let written = Written {
                            len,
                            mtime_ns: mtime,
                            // The version the listing named: a file that changed on the drive
                            // since is another version, and is copied again at the next run.
                            etag: o.etag.clone(),
                            modified: o.modified,
                            target: name,
                        };
                        // Written down at once, before the next file: a kill loses this one file
                        // at most, not everything since the ledger's last whole write.
                        if let Some(state) = state {
                            if note_landed(state, landed_number, &o.key, &written).is_ok() {
                                landed_number += 1;
                            }
                        }
                        report.written.insert(o.key.clone(), written);
                    }
                    // Something appeared at the name since the look, or the file there changed:
                    // it is the user's and stays.
                    (Some(Err(DriveError::Conflict { .. })), _) => {
                        report.copied += 1;
                        report.kept.push(o.key.clone());
                    }
                    _ => report.missing.push(o.key.clone()),
                }
            }
        }
        if n % 25 == 0 {
            progress(report.copied, report.total);
        }
        // What is written so far is remembered: a copy cut off here does not forget it - but the
        // whole ledger is rewritten (and synced) each time, so the writes are spread by the
        // ledger's size, not by a fixed number of files.
        since_kept += 1;
        // By count, as the ledger's size allows, and by time: the landed files are one small file
        // each, and a long copy folds them into the ledger at least every KEEP_EVERY.
        if flush_due(since_kept, report.written.len(), last_kept.elapsed()) {
            since_kept = 0;
            last_kept = std::time::Instant::now();
            keep_report_partly(&report, state);
        }
    }
    if unrecorded > 0 {
        report.notes.push(format!(
            "{unrecorded} files were in the folder with no record that this copy wrote them: they are \
             kept as they are, and a change on the drive does not reach them"
        ));
    }
    keep_report(report, state)
}

/// Whether a copy folds what it landed into the ledger now: [`keep_due`], or [`KEEP_EVERY`] since
/// the last time (and something landed since).
pub(crate) fn flush_due(files_since: usize, ledger: usize, waited: std::time::Duration) -> bool {
    keep_due(files_since, ledger) || (files_since > 0 && waited >= KEEP_EVERY)
}

/// How many files a run copies between two writes of its record: 25 at least, and an eighth of the
/// ledger as it is - so the bytes written in all grow with the ledger and not with its square (a
/// 100,000-file drive rewrote a ~10 MB ledger 4,000 times, each time synced:
/// X-DELTA-AZDRIVE-FILES-18).
pub(crate) fn keep_due(files_since: usize, ledger: usize) -> bool {
    files_since >= (ledger / 8).max(25)
}

/// The state written whole to `state` (nowhere without one): the old file or the new one
/// survives a crash or a power cut, and a torn file does not lose the list of what is missing
/// (AZ X-AR5a-5). The end of a run, with the time it ended.
fn keep_report(mut report: Rescue, state: Option<&Path>) -> Rescue {
    report.at = now_secs();
    keep_report_partly(&report, state);
    report
}

fn keep_report_partly(report: &Rescue, state: Option<&Path>) {
    let Some(state) = state else {
        return;
    };
    if let Ok(text) = serde_json::to_string(report) {
        // The landed files go with a whole write that took: the ledger holds them now.
        if durable::replace(state, text.as_bytes(), durable::Mode::Private).is_ok() {
            forget_landed(state);
        }
    }
}
