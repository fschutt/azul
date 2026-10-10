//! A banned Azlin drive in AzDrive (ban contract v1). The token server says it in the drive's
//! status (the daily look, the look after a refused write) and in its credentials: `banned`,
//! why, and until when its files can still be read. Until then AzDrive shows a banner over the
//! drive - "Due to <reason>, your account has been banned, but you have <n> hours to migrate
//! your files." (the hours count down) - with "Copy everything to this computer" (a folder
//! picked, the whole drive downloaded into a folder named after it by the transfer queue);
//! uploads, new folders and items, paste, rename, delete and share links are refused with the
//! reason; the sync pauses (a pass would only meet refused uploads). Past the end the token
//! server refuses the drive and AzDrive shows "This drive was closed on <date> because
//! <reason>." and nothing else of it.
//!
//! On stdout, for scripts: `AZDRIVE_BANNED <drive id> <hours>`, `AZDRIVE_CLOSED <drive id>`,
//! `AZDRIVE_COPY_EVERYTHING <drive id>`.

use std::{path::PathBuf, sync::Arc};

use azcloud_kit::Ban;
use azul::{
    callbacks::ButtonOnClickCallbackType,
    dialog::{FileDialog, FileOpenResult},
    prelude::*,
    str::String as AzString,
};
use azul_appkit::l10n::{label, t, t_args, Arg, Phrase, Text};
use azul_storage::{Drive, LocalDrive};

use crate::{
    actions::{now_secs, Action},
    browse::Place,
    fileops::{ConflictChoice, SourceItem, TransferKind},
    ids, with_state, DriveState,
};

// ==== The words ====

// The kit's words of the banner and of a closed drive (AzMail's, the tests'); AzDrive says them
// in the window's language ([`banner_phrase`], [`closed_phrase`]).
#[cfg(test)]
pub(crate) use azcloud_kit::token::{banner_text, closed_text};

/// The banner in the window's language: why, and the hours left to copy the files.
#[must_use]
pub(crate) fn banner_phrase(reason: &str, hours: Option<u64>) -> Phrase {
    match hours {
        Some(hours) => Phrase::new("azdrive-ban-banner")
            .arg("reason", reason)
            .arg("hours", hours),
        None => Phrase::new("azdrive-ban-banner-no-end").arg("reason", reason),
    }
}

/// What a closed drive says in the window's language: when (the ban's end) and why.
#[must_use]
pub(crate) fn closed_phrase(until: Option<u64>, reason: &str) -> Phrase {
    match until {
        Some(until) => {
            let when = azul_storage::time::iso8601(until);
            Phrase::new("azdrive-ban-closed")
                .arg("day", when.get(..10).unwrap_or(&when))
                .arg("reason", reason)
        }
        None => Phrase::new("azdrive-ban-closed-no-day").arg("reason", reason),
    }
}

/// Why `action` cannot run on a drive under `ban` at `now`: what writes, during the grace
/// period; what writes or reads, once it is closed.
#[must_use]
pub(crate) fn refusal(action: &Action, ban: &Ban, now: u64) -> Option<String> {
    let writes = matches!(
        action,
        Action::NewFolder
            | Action::NewItemMenu
            | Action::NewTextDocument
            | Action::NewEmptyFile
            | Action::Paste
            | Action::Cut
            | Action::Rename
            | Action::Delete
            | Action::DeletePermanently
            | Action::DeleteMenu
            | Action::MoveToMenu
            | Action::Upload
            | Action::Zip
            | Action::Share
            | Action::Undo
    );
    let reads = matches!(
        action,
        Action::Download
            | Action::Copy
            | Action::CopyToMenu
            | Action::Open
            | Action::OpenMenu
            | Action::Edit
            | Action::Print
            | Action::Email
    );
    if ban.is_closed(now) {
        return (writes || reads)
            .then(|| azul_appkit::l10n::t_phrase(&closed_phrase(ban.until, &ban.reason)));
    }
    writes.then(|| t_args("azdrive-ban-refused", &[("reason", Arg::from(ban.reason.as_str()))]))
}

/// The status line of a banned drive's sync.
#[must_use]
pub(crate) fn sync_text(ban: &Ban, now: u64) -> Phrase {
    if ban.is_closed(now) {
        return closed_phrase(ban.until, &ban.reason);
    }
    Phrase::new("azdrive-ban-sync-paused").arg("reason", ban.reason.as_str())
}

/// The folder "Copy everything" fills: the drive's name as a folder name (`Photos 2026/`).
#[must_use]
pub(crate) fn copy_prefix(drive_name: &str) -> String {
    let cleaned: String = drive_name
        .trim()
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control()
            {
                '-'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim();
    if cleaned.is_empty() {
        format!("{}/", t("azdrive-ban-copy-folder"))
    } else {
        format!("{cleaned}/")
    }
}

// ==== The state ====

/// The ban of drive `drive_id` (AzDrive's id of it), if it is a banned Azlin drive.
#[must_use]
pub(crate) fn ban_of<'s>(s: &'s DriveState, drive_id: &str) -> Option<&'s Ban> {
    let index = s.slot_index(drive_id)?;
    let (azlin_id, _) = s.slots[index].entry.azlin()?;
    s.bans.get(azlin_id)
}

/// Why `action` cannot run on the drive in view, if it is banned.
#[must_use]
pub(crate) fn refuses(s: &DriveState, action: &Action) -> Option<String> {
    let drive_id = s.current_drive_id()?;
    refusal(action, ban_of(s, &drive_id)?, now_secs())
}

/// Why nothing may be written into drive `drive_id`, if it is banned.
#[must_use]
pub(crate) fn refuses_writes_to(s: &DriveState, drive_id: &str) -> Option<String> {
    refusal(&Action::Upload, ban_of(s, drive_id)?, now_secs())
}

/// The sync's status line of drive `drive_id`, if it is banned (its uploads are paused).
#[must_use]
pub(crate) fn sync_status(s: &DriveState, drive_id: &str) -> Option<Phrase> {
    ban_of(s, drive_id).map(|ban| sync_text(ban, now_secs()))
}

/// What a look at the Azlin drive `azlin_id` said of a ban: kept (said once, and again when it
/// closes), or forgotten (none any more). A closed drive stays closed.
pub(crate) fn seen(s: &mut DriveState, azlin_id: &str, ban: Option<Ban>) {
    let Some(mut ban) = ban else {
        s.bans.remove(azlin_id);
        return;
    };
    let now = now_secs();
    let before = s.bans.get(azlin_id).cloned();
    if before.as_ref().is_some_and(|b| b.closed) {
        ban.closed = true;
    }
    let closed = ban.is_closed(now);
    let changed = before
        .as_ref()
        .is_none_or(|b| b.is_closed(now) != closed || b.until != ban.until);
    if changed {
        if closed {
            println!("AZDRIVE_CLOSED {azlin_id}");
        } else {
            println!("AZDRIVE_BANNED {azlin_id} {}", ban.hours_left(now));
        }
    }
    s.bans.insert(azlin_id.to_string(), ban);
}

// ==== The banner, the closed drive, Copy everything ====

/// The ban of the drive in view, with its id.
fn in_view(s: &DriveState) -> Option<(String, &Ban)> {
    let drive_id = s.current_drive_id()?;
    let ban = ban_of(s, &drive_id)?;
    Some((drive_id, ban))
}

/// The banner over a banned drive in view (not past its end): why, the hours left, Copy
/// everything to this computer.
#[must_use]
pub(crate) fn banner(s: &DriveState, app: &RefAny) -> Option<Dom> {
    let (drive_id, ban) = in_view(s)?;
    let now = now_secs();
    if ban.is_closed(now) {
        return None;
    }
    Some(
        Dom::create_div()
            .with_id(ids::BAN_BAR)
            .with_css(
                "display: flex; flex-direction: row; align-items: center; padding: 8px 12px; \
                 background: #FDE7E9; color: #5C0F14;",
            )
            .with_child(
                Dom::create_div()
                    .with_css("flex-grow: 1; margin-right: 12px;")
                    .with_child(
                        Dom::create_span_with_text(AzString::from(
                            azul_appkit::l10n::t_phrase(&banner_phrase(
                                &ban.reason,
                                ban.until.map(|_| ban.hours_left(now)),
                            )),
                        ))
                        .with_id(ids::BAN_TEXT),
                    ),
            )
            .with_child(
                Button::create(label("azdrive-ban-copy-everything"))
                    .with_on_click(
                        RefAny::new(CopyRef {
                            app: app.clone(),
                            drive_id,
                        }),
                        on_copy_everything as ButtonOnClickCallbackType,
                    )
                    .dom()
                    .with_id(ids::BAN_COPY),
            ),
    )
}

/// What a closed drive in view shows instead of its folder: when and why it was closed.
#[must_use]
pub(crate) fn closed_view(s: &DriveState) -> Option<Dom> {
    if s.find.is_some() || !matches!(s.place, Place::Folder { .. }) {
        return None;
    }
    let (_, ban) = in_view(s)?;
    if !ban.is_closed(now_secs()) {
        return None;
    }
    Some(
        Dom::create_div()
            .with_id(ids::BAN_CLOSED)
            .with_css(
                "display: flex; flex-direction: column; align-items: center; \
                 justify-content: center; flex-grow: 1; padding: 40px; font-size: 14px;",
            )
            .with_child(Dom::create_span_with_text(AzString::from(
                azul_appkit::l10n::t_phrase(&closed_phrase(ban.until, &ban.reason)),
            ))),
    )
}

/// What Copy everything carries.
struct CopyRef {
    app: RefAny,
    drive_id: String,
}

/// Copy everything to this computer: the system's folder picker.
extern "C" fn on_copy_everything(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((app, drive_id)) = data
        .downcast_ref::<CopyRef>()
        .map(|r| (r.app.clone(), r.drive_id.clone()))
    else {
        return Update::DoNothing;
    };
    let _request = FileDialog::open_directory(
        label("azdrive-ban-copy-title"),
        OptionString::None,
        RefAny::new(CopyRef { app, drive_id }),
        on_copy_folder,
    );
    Update::DoNothing
}

/// The folder picked: the whole drive into a folder of its name there, by the transfer queue.
extern "C" fn on_copy_folder(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut app, drive_id)) = data
        .downcast_ref::<CopyRef>()
        .map(|r| (r.app.clone(), r.drive_id.clone()))
    else {
        return Update::DoNothing;
    };
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing; // cancelled
    };
    let folder = PathBuf::from(path.inner.as_str());
    with_state(&mut app, &mut info, |info, app, s| {
        copy_everything(info, app, s, &drive_id, folder);
    })
}

/// Every file of drive `drive_id` downloaded into `<folder>/<the drive's name>/` (the transfer
/// queue's download: its progress, a name taken keeps both).
pub(crate) fn copy_everything(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    folder: PathBuf,
) {
    let Some(source) = crate::open_drive(s, drive_id) else {
        s.error(Text::key("azdrive-ban-copy-not-opened"));
        return;
    };
    let prefix = copy_prefix(&s.drive_name(&Place::folder(drive_id, "")));
    let target: Arc<dyn Drive> = Arc::new(LocalDrive::without_manifest(folder.clone()));
    println!("AZDRIVE_COPY_EVERYTHING {drive_id}");
    let everything = SourceItem {
        key: String::new(),
        is_folder: true,
        size: None,
    };
    crate::actions::enqueue_routed(
        info,
        app,
        s,
        crate::sync_jobs::Transfer {
            kind: TransferKind::Download,
            source: (drive_id.to_string(), source),
            items: vec![everything],
            target: (format!("os:{}", folder.display()), target),
            target_prefix: prefix,
            target_name: folder.display().to_string(),
            auto: Some(ConflictChoice::KeepBoth),
        },
    );
}
