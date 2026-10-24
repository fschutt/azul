//! An unpaid Azlin drive's last months in AzDrive, and its way back.
//!
//! After its paid period an Azlin drive stays writable for 14 days, then it is read-only for 90.
//! Then it is locked: nothing is read or written any more, and its files move to offline
//! storage until its owner proves the drive is theirs and pays. AzDrive says each step before it
//! comes:
//! - while it is unpaid, a bar over the drive says exactly what happens when (the reminders 30,
//!   7 and 1 days before each step are renewals.rs's);
//! - while it is read-only, AzDrive copies every file of the drive to this computer, into
//!   `Azlin rescue/<drive>` in the home folder, so nothing is lost when it locks; the bar shows
//!   how far that got and which files are still missing, and the copy runs again at every look;
//! - once it is locked, the drive is empty in AzDrive: a page says why and offers "Get my drive
//!   back" - this computer's own key, one of the drive's recovery keys, signs the token server's
//!   statement (or the recovery code, typed, on a computer without one), then Renew pays the
//!   three months the drive owes, and the paid checkout is named in a second proof so its months
//!   count at once;
//! - when its files are back, the drive opens again and AzDrive says so once.
//!
//! On stdout, for scripts: `AZDRIVE_ARCHIVE_STAGE <drive> <grace|read_only|locked|restore_pending>`,
//! `AZDRIVE_ARCHIVE_BACK <drive>`, `AZDRIVE_RESCUE <drive> <copied> <total> <missing>`,
//! `AZDRIVE_ARCHIVE_PROVEN <drive> <locked|restore_pending|restored>`.

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use azcloud_kit::{
    azlin_contract::{
        months_owed, ArchiveState, ArchiveWire, LOCK_AFTER_SECS, WRITABLE_GRACE_SECS,
    },
    DriveStatus, ProofAnswer, SharedKeyring, TokenError, TokenServer,
};
use azul::{
    callbacks::ButtonOnClickCallbackType, file::FilePath, prelude::*, str::String as AzString,
};
use azul_appkit::l10n::{label, t_phrase, Phrase};
use azul_storage::{azul_transport::AzulTransport, ByteRange, Drive, ObjectInfo};
use serde::{Deserialize, Serialize};

use crate::{actions::now_secs, browse::Place, ids, jobs::Job, spawn, DriveState, HOME_ID};

/// The folder in the home folder the read-only drives are copied into.
pub(crate) const RESCUE_DIR: &str = "Azlin rescue";
/// A copy reads a file in pieces of this size.
const PIECE: u64 = 8 * 1024 * 1024;
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
    if now < period_until {
        return None;
    }
    let grace = u64::try_from(WRITABLE_GRACE_SECS).unwrap_or(0);
    let debt = u64::try_from(LOCK_AFTER_SECS).unwrap_or(0);
    let read_only_at = period_until + grace;
    let locks_at = period_until + debt;
    if now < read_only_at && !status.payment_due() {
        Some(Stage::Grace {
            period_until,
            read_only_at,
            locks_at,
        })
    } else {
        Some(Stage::ReadOnly {
            period_until,
            locks_at,
        })
    }
}

/// `YYYY-MM-DD` of `at` (seconds since 1970).
fn day_of(at: u64) -> String {
    azul_storage::time::iso8601(at).chars().take(10).collect()
}

/// The months (of 30 days) a locked drive still needs before it comes back: its grace and
/// read-only time, less what was paid since.
#[must_use]
pub(crate) fn months_needed(wire: &ArchiveWire, period_until: Option<u64>) -> u32 {
    let until = period_until.and_then(|p| i64::try_from(p).ok()).unwrap_or(0);
    months_owed(until, wire.pay_through.unix())
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
            .arg("read_only", day_of(*period_until + u64::try_from(WRITABLE_GRACE_SECS).unwrap_or(0)))
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
                .arg("locked", day_of(u64::try_from(wire.locked_at.unix()).unwrap_or(0)))
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
pub(crate) fn seen(info: &mut CallbackInfo, s: &mut DriveState, azlin_id: &str, status: &DriveStatus) {
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
    let name = s.drive_name(&Place::folder(&slot_of(s, azlin_id).unwrap_or_default(), ""));
    let said = Phrase::new("azdrive-archive-back").arg("drive", name.as_str());
    let body = azul_appkit::l10n::translate_text(info, &said.clone().into());
    info.post_notification(
        azul::notification::Notification::create(format!("azdrive-archive-back-{azlin_id}"), "AzDrive")
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
    bar.add_child(button("azdrive-renew", app, &drive_id, on_renew, ids::ARCHIVE_RENEW));
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
fn rescue_phrase(r: &Rescue) -> Option<Phrase> {
    let folder = r.folder.display().to_string();
    if let Some((needed, free)) = r.no_space {
        return Some(
            Phrase::new("azdrive-archive-rescue-no-space")
                .arg("needed", azcloud_kit::usage::size_text(needed))
                .arg("free", azcloud_kit::usage::size_text(free)),
        );
    }
    if r.running {
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
    (r.total > 0).then(|| Phrase::new("azdrive-archive-rescue-done").arg("folder", folder.as_str()))
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
                .with_css("max-width: 560px; margin-bottom: 16px;"),
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
        let name = copy_name(&s.drive_name(&Place::folder(&drive_id, "")));
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
    prove_start(info, app, s, &slot, ProveWith::DeviceKey, Some(checkout_id.to_string()));
}

/// This computer's own key of AzDrive's drive `drive_id` registered as one of its recovery keys,
/// `signer` (the recovery code's key, the code at hand) signing the addition; a job. Once: a key
/// registered before answers its id without asking.
pub(crate) fn register_device_key(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    signer: azcloud_kit::RecoveryKey,
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
        signer,
    };
    spawn(info, app, s, job);
}

/// The registration of this computer's key, run in a job: its id among the drive's recovery
/// keys.
pub(crate) fn register_device_key_now(
    token_url: &str,
    keyring: &SharedKeyring,
    drive_id: &str,
    signer: &azcloud_kit::RecoveryKey,
) -> Result<String, String> {
    let transport = AzulTransport::new(crate::USER_AGENT);
    let server = TokenServer::new(token_url, &transport).map_err(|e| e.to_string())?;
    azcloud_kit::device_key::register_as_recovery_key(&server, keyring, drive_id, |message| {
        Ok(signer.sign_base64(message))
    })
    .map_err(|e| e.to_string())
}

/// How the owner proves a locked drive is theirs.
pub(crate) enum ProveWith {
    /// This computer's own key, one of the drive's recovery keys.
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
/// server's statement signed with `via`. With the recovery code at hand, this computer's own key
/// becomes one of the drive's recovery keys too, so the next proof needs no code. Runs in a job.
pub(crate) fn prove(
    token_url: &str,
    keyring: &SharedKeyring,
    drive_id: &str,
    via: &ProveWith,
    checkout: Option<&str>,
) -> Result<ProofAnswer, String> {
    let transport = AzulTransport::new(crate::USER_AGENT);
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
                .or_else(|e| match &e {
                    // not the drive key: the code's findable key (a second kit registers only it)
                    TokenError::Refused { code, .. } if code == "bad_signature" => server
                        .archive_prove(drive_id, token, checkout, None, |message| {
                            Ok(findable.sign_base64(message))
                        }),
                    _ => Err(e),
                })
                .map_err(proof_error)
        }
    });
    let answer = answered.map_err(|e| e.to_string())?;
    #[cfg(feature = "encryption")]
    if let (Ok(_), ProveWith::RecoveryCode(code)) = (&answer, via) {
        let key = crate::encryption::recovery_key_of(code, drive_id);
        if let Err(e) = azcloud_kit::device_key::register_as_recovery_key(
            &server,
            keyring,
            drive_id,
            |message| Ok(key.sign_base64(message)),
        ) {
            eprintln!("[azdrive] this computer's key of {drive_id} is not registered: {e}");
        }
    }
    answer
}

/// A refused proof's words: the contract's refusals as message keys.
fn proof_error(e: TokenError) -> String {
    match &e {
        TokenError::Refused { code, .. } => match code.as_str() {
            "no_key" => String::from("azdrive-archive-no-key-at-all"),
            "bad_signature" => String::from("azdrive-archive-wrong-key"),
            "bad_challenge" | "challenge_used" => String::from("azdrive-archive-try-again"),
            "not_archived" => String::from("azdrive-archive-not-locked"),
            _ => e.to_string(),
        },
        _ => e.to_string(),
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
    /// When the last copy ended (seconds since 1970).
    #[serde(default)]
    pub at: u64,
    #[serde(skip)]
    pub running: bool,
}

/// A drive's name as a folder name (no separators, never empty).
fn copy_name(name: &str) -> String {
    crate::ban::copy_prefix(name).trim_end_matches('/').to_string()
}

/// The home folder (the This PC entry of the user's home).
fn home(s: &DriveState) -> Option<PathBuf> {
    s.slots.iter().find(|slot| slot.entry.id == HOME_ID).and_then(|slot| {
        match &slot.entry.location {
            azul_storage::config::DriveLocation::Local { root } => Some(PathBuf::from(root)),
            _ => None,
        }
    })
}

/// Where the copy of the Azlin drive `azlin_id` is kept between runs.
fn rescue_file(s: &DriveState, azlin_id: &str) -> PathBuf {
    let file: String = azlin_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    s.period_tokens.dir().join(format!("{file}.rescue.json"))
}

/// The copy's state the last run left (none before a first copy).
fn rescue_read(path: &Path) -> Option<Rescue> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
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
            if let Some(kept) = rescue_read(&state) {
                s.rescues.insert(azlin_id.clone(), kept);
            }
        }
        let due = s.rescues.get(&azlin_id).is_none_or(|r| {
            !r.running && (!r.missing.is_empty() || r.no_space.is_some() || now >= r.at + DAY)
        });
        if !due {
            continue;
        }
        let Some(drive_id) = slot_of(s, &azlin_id) else {
            continue;
        };
        let Some(source) = crate::open_drive(s, &drive_id) else {
            continue;
        };
        let folder = home
            .join(RESCUE_DIR)
            .join(copy_name(&s.drive_name(&Place::folder(&drive_id, ""))));
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

/// How far a running copy got.
pub(crate) fn rescue_progress(s: &mut DriveState, azlin_id: &str, copied: usize, total: usize) {
    if let Some(r) = s.rescues.get_mut(azlin_id) {
        r.copied = copied;
        r.total = total;
    }
}

/// A copy ended: kept for the bar, said on stdout.
pub(crate) fn rescue_done(s: &mut DriveState, azlin_id: &str, report: Rescue) {
    println!(
        "AZDRIVE_RESCUE {azlin_id} {} {} {}",
        report.copied,
        report.total,
        report.missing.len()
    );
    s.rescues.insert(azlin_id.to_string(), report);
}

/// Where the file `key` goes under `folder`: each part of the key a plain name (no `..`, no
/// empty part), or `None` for a key that cannot be a path.
#[must_use]
pub(crate) fn target_of(folder: &Path, key: &str) -> Option<PathBuf> {
    let mut path = folder.to_path_buf();
    for part in key.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains('\\') {
            return None;
        }
        path.push(part);
    }
    Some(path)
}

/// Whether `o` is copied already: a file of its size at its place.
fn copied_already(folder: &Path, o: &ObjectInfo) -> bool {
    target_of(folder, &o.key)
        .and_then(|path| std::fs::metadata(path).ok())
        .is_some_and(|meta| meta.is_file() && meta.len() == o.size)
}

/// Copies `o` from `source` into `folder`, in pieces, through a `.part` file renamed at the end
/// (a copy cut halfway never looks whole).
fn copy_one(source: &dyn Drive, folder: &Path, o: &ObjectInfo) -> Result<(), String> {
    let target = target_of(folder, &o.key).ok_or_else(|| String::from("not a path"))?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut part = target.clone().into_os_string();
    part.push(".part");
    let part = PathBuf::from(part);
    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut off = 0u64;
    while off < o.size {
        let end = (off + PIECE).min(o.size) - 1;
        let bytes = source
            .get_range(&o.key, ByteRange::new(off, Some(end)))
            .map_err(|e| e.to_string())?;
        if bytes.is_empty() {
            return Err(String::from("the drive answered no bytes"));
        }
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        off += bytes.len() as u64;
    }
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    std::fs::rename(&part, &target).map_err(|e| e.to_string())
}

/// The free bytes of the disk `path` is on.
pub(crate) fn free_bytes(path: &Path) -> Option<u64> {
    let text = path.to_string_lossy().into_owned();
    FilePath::create(text.as_str())
        .disk_space()
        .into_option()
        .map(|space| space.free)
}

/// Copies every file of `source` into `folder` (a job): the files already there skipped, the
/// rest piece by piece; `progress` hears how far it got. Too little room (`free`: the bytes free
/// on the disk, when known) stops it before it begins. What it came to, kept in `state` for the
/// next start.
pub(crate) fn run_rescue(
    source: &dyn Drive,
    folder: &Path,
    free: Option<u64>,
    state: &Path,
    progress: &mut dyn FnMut(usize, usize),
) -> Rescue {
    let mut report = Rescue {
        folder: folder.to_path_buf(),
        at: now_secs(),
        ..Rescue::default()
    };
    let objects = match azul_storage::ops::list_all(source, "") {
        Ok(objects) => objects,
        Err(e) => {
            report.missing.push(format!("(the drive's list: {e})"));
            return keep_report(report, state);
        }
    };
    let files: Vec<&ObjectInfo> = objects.iter().filter(|o| !o.key.ends_with('/')).collect();
    report.total = files.len();
    let todo: Vec<&ObjectInfo> = files
        .iter()
        .copied()
        .filter(|o| !copied_already(folder, o))
        .collect();
    report.copied = report.total - todo.len();
    let needed: u64 = todo.iter().map(|o| o.size).sum();
    if let Some(free) = free {
        if needed + SPARE_BYTES > free {
            report.no_space = Some((needed, free));
            report.missing = todo.iter().map(|o| o.key.clone()).collect();
            return keep_report(report, state);
        }
    }
    for (n, o) in todo.iter().enumerate() {
        match copy_one(source, folder, o) {
            Ok(()) => report.copied += 1,
            Err(_) => report.missing.push(o.key.clone()),
        }
        if n % 25 == 0 {
            progress(report.copied, report.total);
        }
    }
    keep_report(report, state)
}

fn keep_report(report: Rescue, state: &Path) -> Rescue {
    if let Some(dir) = state.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string(&report) {
        let _ = std::fs::write(state, text);
    }
    report
}
