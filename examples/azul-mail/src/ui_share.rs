//! Handing a message or an attachment to another app through the system's share sheet (WP-F's
//! `CallbackInfo::share` and `can_share`): the message as its `.eml` file, one attachment as the
//! file it is, "Open with..." for one attachment. On a phone this is also how a file reaches the
//! Files app: the sheet the system shows has "Save to Files" (`FileDialog::save_bytes` answers
//! false there, so there is no other way). A desktop has no share sheet: nothing here shows.
//!
//! The engine copies a shared file under the display name the app gives, so this file stages the
//! bytes once in `<data root>/azmail-share/`, hands the staged path over with the name and the MIME
//! type, and removes the staged file when the call has returned.
//!
//! The only code that names the share API is [`can_share`] and [`hand_over`], behind the cargo
//! feature `share`: WP-F's types are not in the generated bindings yet (ci-wip/wp-f-honest,
//! core/src/share.rs). Without the feature they answer "not available", so no row of the phone's
//! sheet and no button of the attachments shows; turning the feature on is the whole change once
//! api.json has them.

use std::path::{Path, PathBuf};

use azul::prelude::*;
use azul_appkit::l10n::{t, t_args, Arg};

use crate::{message, MailApp};

/// The most characters of a name or a title handed over (a longer subject is cut at a character).
pub(crate) const MAX_NAME_CHARS: usize = 100;

/// The largest file one share carries: the engine's own limit (WP-F: 256 MiB a file).
pub(crate) const MAX_SHARED_BYTES: usize = 256 * 1024 * 1024;

/// The folder under the data root where what is being shared is staged.
pub(crate) const STAGING_FOLDER: &str = "azmail-share";

/// What is handed over: the bytes, the name the receiver shows, their MIME type and the title of
/// the sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Offer {
    pub bytes: Vec<u8>,
    pub name: String,
    pub mime: String,
    pub title: String,
}

/// Why nothing is handed over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// There are no bytes (no such attachment, an empty file).
    Nothing,
    /// More than [`MAX_SHARED_BYTES`]; the size.
    TooBig(usize),
}

/// How the platform answered a share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Handed {
    /// The system's sheet is up.
    Shown,
    /// The platform has no share sheet, with the reason.
    Unsupported(String),
    /// The platform could not show it, with the reason.
    Failed(String),
    /// The request was refused (a limit, a name), with the reason.
    Refused(String),
}

/// A name for the receiver to show: the characters a file name may not hold become `_`, no leading
/// dots, at most [`MAX_NAME_CHARS`] characters; `fallback` where nothing is left.
#[must_use]
pub(crate) fn display_name(name: &str, fallback: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cut: String = cleaned
        .trim()
        .trim_start_matches('.')
        .chars()
        .take(MAX_NAME_CHARS)
        .collect();
    let cut = cut.trim_end();
    if cut.is_empty() {
        fallback.to_string()
    } else {
        cut.to_string()
    }
}

/// Whether `len` bytes can be shared.
fn check_size(len: usize) -> Result<(), Refusal> {
    match len {
        0 => Err(Refusal::Nothing),
        n if n > MAX_SHARED_BYTES => Err(Refusal::TooBig(n)),
        _ => Ok(()),
    }
}

/// The message itself, as the `.eml` file it is stored as, named after its subject.
pub(crate) fn message_offer(bytes: Vec<u8>, subject: &str) -> Result<Offer, Refusal> {
    check_size(bytes.len())?;
    let base = display_name(subject, "message");
    Ok(Offer {
        name: format!("{base}.eml"),
        mime: String::from("message/rfc822"),
        title: base,
        bytes,
    })
}

/// The `index`-th attachment of a message's bytes (the order of `MessageView::attachments`), as
/// the file it is.
pub(crate) fn attachment_offer(message_bytes: &[u8], index: usize) -> Result<Offer, Refusal> {
    let part = message::attachment_parts(message_bytes)
        .into_iter()
        .nth(index)
        .ok_or(Refusal::Nothing)?;
    check_size(part.bytes.len())?;
    let name = display_name(&part.name, "attachment");
    Ok(Offer {
        title: name.clone(),
        name,
        mime: part.mime_type,
        bytes: part.bytes,
    })
}

/// Where a share of message `uid` (its attachment `index`, or the message itself) is staged.
#[must_use]
pub(crate) fn staged_path(data_root: &Path, uid: u32, index: Option<usize>) -> PathBuf {
    let file = match index {
        None => format!("{uid}.eml"),
        Some(i) => format!("{uid}-{i}.part"),
    };
    data_root.join(STAGING_FOLDER).join(file)
}

/// What the status bar says of an answer: nothing where the sheet is up.
#[must_use]
pub(crate) fn notice_of(answer: &Handed) -> String {
    let with = |key: &str, reason: &str| t_args(key, &[("reason", Arg::from(reason))]);
    match answer {
        Handed::Shown => String::new(),
        Handed::Unsupported(reason) => with("azmail-share-unsupported", reason),
        Handed::Failed(reason) => with("azmail-share-failed", reason),
        Handed::Refused(reason) => with("azmail-share-refused", reason),
    }
}

/// What the status bar says of a refusal.
#[must_use]
pub(crate) fn refusal_notice(refusal: &Refusal) -> String {
    match refusal {
        Refusal::Nothing => t("azmail-share-nothing"),
        Refusal::TooBig(len) => t_args(
            "azmail-share-too-big",
            &[(
                "size",
                Arg::from(azul::file::DiskSpace::format_bytes(*len as u64).as_str().to_string()),
            )],
        ),
    }
}

// ---- the platform's share sheet (WP-F): the two places that name its API ----

/// Whether the platform has a share sheet to offer (a phone does, a desktop answers "unsupported").
#[cfg(feature = "share")]
#[must_use]
pub(crate) fn can_share(info: &CallbackInfo) -> bool {
    info.can_share()
}

/// Without WP-F's API in the bindings there is no sheet to offer.
#[cfg(not(feature = "share"))]
#[must_use]
pub(crate) fn can_share(_info: &CallbackInfo) -> bool {
    false
}

/// Hands the staged `file` to the system's share sheet under the offer's name and MIME type; with
/// `open_with` the one file is offered only to the apps that can open it.
#[cfg(feature = "share")]
fn hand_over(info: &mut CallbackInfo, file: &Path, offer: &Offer, open_with: bool) -> Handed {
    use azul::share::{ShareItem, ShareRequest, ShareStatus};
    let item = ShareItem::file(file.to_string_lossy().as_ref().into())
        .with_name(offer.name.as_str().into())
        .with_mime(offer.mime.as_str().into());
    let request = ShareRequest::new()
        .with_item(item)
        .with_title(offer.title.as_str().into())
        .with_open_with(open_with);
    match info.share(request) {
        ShareStatus::Shown => Handed::Shown,
        ShareStatus::Unsupported(reason) => Handed::Unsupported(reason.as_str().to_string()),
        ShareStatus::Failed(reason) => Handed::Failed(reason.as_str().to_string()),
        ShareStatus::Refused(reason) => Handed::Refused(reason.as_str().to_string()),
    }
}

/// Without WP-F's API in the bindings nothing is handed over.
#[cfg(not(feature = "share"))]
fn hand_over(_info: &mut CallbackInfo, _file: &Path, _offer: &Offer, _open_with: bool) -> Handed {
    Handed::Unsupported(String::from("this build has no share sheet"))
}

/// Stages the offer, hands it over and removes the staged file (the engine copied it).
fn share_offer(s: &MailApp, info: &mut CallbackInfo, uid: u32, index: Option<usize>, offer: &Offer, open_with: bool) -> Handed {
    let path = staged_path(&s.data_root, uid, index);
    if let Some(folder) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(folder) {
            return Handed::Failed(e.to_string());
        }
    }
    if let Err(e) = std::fs::write(&path, &offer.bytes) {
        return Handed::Failed(e.to_string());
    }
    let answer = hand_over(info, &path, offer, open_with);
    // the engine made its own copy under the offer's name; the staged one has done its work
    let _ = std::fs::remove_file(&path);
    answer
}

/// The open message's uid, folder, stored path and subject, with its bytes: what a share needs.
fn open_message_bytes(s: &MailApp) -> Option<(u32, String, Vec<u8>)> {
    let open = s.open.as_ref().filter(|o| o.view.is_some())?;
    let bytes = s.message_store(&open.folder)?.get(&open.entry.path).ok()?;
    Some((open.entry.uid, open.entry.subject.clone(), bytes))
}

/// Share the open message as an `.eml` file (the phone's sheet: Share).
pub(crate) fn share_message(s: &mut MailApp, info: &mut CallbackInfo) {
    let Some((uid, subject, bytes)) = open_message_bytes(s) else {
        s.notice = t("azmail-notice-select-first");
        return;
    };
    s.notice = match message_offer(bytes, &subject) {
        Ok(offer) => notice_of(&share_offer(s, info, uid, None, &offer, false)),
        Err(refusal) => refusal_notice(&refusal),
    };
}

/// Share (or, with `open_with`, open in another app) the `index`-th attachment of the open message.
pub(crate) fn share_attachment(s: &mut MailApp, info: &mut CallbackInfo, index: usize, open_with: bool) {
    let Some((uid, _, bytes)) = open_message_bytes(s) else {
        s.notice = t("azmail-notice-select-first");
        return;
    };
    s.notice = match attachment_offer(&bytes, index) {
        Ok(offer) => notice_of(&share_offer(s, info, uid, Some(index), &offer, open_with)),
        Err(refusal) => refusal_notice(&refusal),
    };
}

#[cfg(test)]
#[path = "ui_share_test.rs"]
mod ui_share_test;
