use azcloud_kit::sync::session::FileState;

use super::*;
use crate::model::PREVIEW_SIZE_MAX;

/// A conflict waits for the user's answer: an amber banner, no more words than its own. A failed
/// transfer is red and says why behind the banner's (?).
#[test]
fn a_conflict_is_amber_and_a_failed_transfer_is_red_with_its_reason() {
    let conflict = banner_of(&FileState::Conflict).expect("a conflict has a banner");
    assert!(!conflict.failed);
    assert_eq!(conflict.text, "azdrive-sync-state-conflict");
    assert_eq!(conflict.detail, None);
    let failed = banner_of(&FileState::Error(String::from("403 from the drive"))).expect("an error has one");
    assert!(failed.failed);
    assert_eq!(failed.detail.as_deref(), Some("403 from the drive"));
    assert_ne!(conflict.icon, failed.icon);
}

/// Every other state is the item's own business: the pane shows the item and no banner.
#[test]
fn the_other_sync_states_have_no_banner() {
    for state in [
        FileState::CloudOnly,
        FileState::Downloading { done: 1, total: 2 },
        FileState::Uploading { done: 1, total: 2 },
        FileState::OnDevice,
        FileState::OnDeviceEncrypted,
        FileState::Pinned,
    ] {
        assert_eq!(banner_of(&state), None, "{state:?}");
    }
}

/// The size the user dragged the pane to is kept for the dock it was dragged in, capped; the
/// other dock's size stays.
#[test]
fn the_dragged_size_is_kept_for_its_dock_and_capped() {
    let mut settings = Settings::default();
    keep_size(&mut settings, false, 412.0);
    assert_eq!((settings.preview_width, settings.preview_height), (412.0, 0.0));
    keep_size(&mut settings, true, 180.0);
    assert_eq!((settings.preview_width, settings.preview_height), (412.0, 180.0));
    keep_size(&mut settings, false, f32::NAN);
    assert_eq!(settings.preview_width, 0.0, "a number that is no size is the default");
    keep_size(&mut settings, true, 1.0e9);
    assert_eq!(settings.preview_height, PREVIEW_SIZE_MAX);
}
