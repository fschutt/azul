//! A top-level window the app opens as OWNED by the window whose callback opened it:
//! an Options window, an event editor, a presenter's console.
//!
//! An owned window stays above its owner, minimises and comes back with it, and closes
//! when the owner closes (through its own close guard, so a window with unsaved work
//! can still ask). The app sets `WindowCreateOptions::owned`; the shell fills in the
//! owner, the window that created it, because an app has no platform id to name.

use azul_layout::window_state::WindowCreateOptions;

/// The options a window created by the window `creator` (its platform id, the
/// registry key) is opened with: an owned window whose options name no parent takes
/// `creator` as its owner. Anything else is opened as it was asked for.
#[must_use]
pub fn with_owner(options: &WindowCreateOptions, creator: u64) -> WindowCreateOptions {
    let mut options = options.clone();
    if options.owned && options.parent_window_id == 0 {
        options.parent_window_id = creator;
    }
    options
}

/// The owner a window opened with `options` records (`CommonWindowState::owner_window_id`),
/// so the owner's close finds it: its parent's id when it is owned, 0 otherwise.
#[must_use]
pub const fn owner_of(options: &WindowCreateOptions) -> u64 {
    if options.owned {
        options.parent_window_id
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_owned_window_takes_the_window_that_opened_it_as_its_owner() {
        let mut options = WindowCreateOptions::default();
        options.owned = true;
        let opened = with_owner(&options, 0xBEEF);
        assert_eq!(opened.parent_window_id, 0xBEEF);
        assert_eq!(owner_of(&opened), 0xBEEF, "and remembers it, to close with it");
    }

    #[test]
    fn a_window_that_is_not_owned_has_no_owner() {
        let opened = with_owner(&WindowCreateOptions::default(), 0xBEEF);
        assert_eq!(opened.parent_window_id, 0, "a plain window keeps no parent");
        assert_eq!(owner_of(&opened), 0);
    }

    #[test]
    fn an_owner_the_app_named_is_kept() {
        let mut options = WindowCreateOptions::default();
        options.owned = true;
        options.parent_window_id = 0xCAFE;
        assert_eq!(with_owner(&options, 0xBEEF).parent_window_id, 0xCAFE);
    }
}
