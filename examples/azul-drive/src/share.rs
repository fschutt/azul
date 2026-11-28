//! Share on a phone: the quick-action sheet's Share row hands a FILE of this computer to the platform's
//! share sheet (`CallbackInfo::share`, WP-F's: `ShareRequest::file(path)`), where the platform has one
//! (`can_share()`). The row is there only for a file on a drive on this computer: a cloud file has no path to
//! hand over (its link is Share > Copy link, `Action::Share`), a folder is not a file.
//!
//! WP-F's calls are written against the names its brief gives (briefs/app_refactor_2026-11-28.md, section
//! 1b) and are behind the cargo feature `share`, off until master has them; every such call site is marked
//! `WP-F share`. Without the feature the platform can share nothing as far as AzDrive knows, and the sheet has
//! no Share row.

use azul::prelude::*;

use crate::{browse::Entry, DriveState};

/// Whether the platform has a share sheet to hand a file to: asked at the long press, kept in
/// `DriveState::can_share` for the sheet's build.
#[cfg(feature = "share")]
pub(crate) fn available(info: &CallbackInfo) -> bool {
    // WP-F share: `CallbackInfo::can_share()`
    info.can_share()
}

/// Without WP-F's calls (the feature `share` is off): no share sheet.
#[cfg(not(feature = "share"))]
pub(crate) fn available(_info: &CallbackInfo) -> bool {
    false
}

/// Whether the sheet of `entry` has a Share row: the platform shares, and the entry is a file of a drive on
/// this computer.
#[must_use]
pub(crate) fn row_shown(can_share: bool, entry: &Entry, cloud: bool) -> bool {
    can_share && !cloud && !entry.is_folder
}

/// Hands the file `key` of the open drive to the platform's share sheet.
#[cfg(feature = "share")]
pub(crate) fn share_item(info: &mut CallbackInfo, s: &mut DriveState, key: &str) {
    use azul_appkit::l10n::Phrase;
    let Some(index) = s.current_drive() else {
        return;
    };
    let Some(path) = s.local_dir(index, key) else {
        return;
    };
    // WP-F share: `ShareRequest::file(path)`, its status
    let request = azul::share::ShareRequest::file(path.display().to_string());
    match info.share(request) {
        azul::share::ShareStatus::Shown => println!("AZDRIVE_SHARED {key}"),
        azul::share::ShareStatus::Unsupported(why)
        | azul::share::ShareStatus::Failed(why)
        | azul::share::ShareStatus::Refused(why) => {
            s.warn(Phrase::new("azdrive-share-not-shown").arg("why", why.as_str()).into());
        }
    }
}

/// Without WP-F's calls: the row is never shown, so this is never asked.
#[cfg(not(feature = "share"))]
pub(crate) fn share_item(_info: &mut CallbackInfo, s: &mut DriveState, _key: &str) {
    s.warn(azul_appkit::l10n::Text::key("azdrive-share-unavailable"));
}

#[cfg(test)]
#[path = "share_test.rs"]
mod share_test;
