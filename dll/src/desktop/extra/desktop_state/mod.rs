//! The desktop's state, polled: which app is in front, where the pointer is on the desktop, whether
//! the screen is locked and whether the display sleeps.
//!
//! A synchronous READING, cheap enough to ask ten or twenty times a second from the thread that
//! asks. A recorder of a review asks to tell the app in front (so a video can be cut where the
//! person went to a browser), the pointer (so it can zoom to it outside the app) and the lock (so
//! it pauses while nobody is there); an app that wants to be gentle asks the same.
//!
//! * macOS ([`macos`]): `NSWorkspace`'s front application, `CGEvent`'s mouse location, the
//!   session's lock key and `CGDisplayIsAsleep`; none of them asks for a permission of the
//!   system.
//! * Windows, Linux, the others: not yet; every field answers "not known"
//!   ([`DesktopState::UNKNOWN`]) and the screen is not locked.
//! * A headless or E2E TEST run (`AZ_BACKEND=headless`, `AZ_E2E_TEST`; a build with the cargo
//!   feature `test-hooks` only: a shipped build reads the desktop): [`DesktopState::HEADLESS`], or
//!   what the file named by `AZ_DESKTOP_STATE_FILE` says, read at every query (see [`script`]), so
//!   a test of an app that records or reacts needs no desktop and moves the pointer and switches
//!   the app in front by writing a file.
//!
//! # Privacy: what is read, and that nothing of it leaves the device
//!
//! READ: the bundle id and the name of the app in front (which program the person is using), the
//! pointer's place on the desktop, whether the screen is locked and whether the display is
//! asleep. This is more about the person than the device-state readings of
//! `crate::desktop::extra::sensors` say, so it is a module of its own: an app that asks says so to
//! the person (AzReview's recorder shows it in its panel and writes it only to the recording the
//! person made). NEVER READ: the windows' titles, their contents, the keys pressed, the clicks, the
//! list of the other apps that run, the files. Nothing is kept here and nothing is sent anywhere.

mod script;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

use azul_core::geom::{OptionScreenPosition, ScreenPosition};
use azul_css::{AzString, OptionString};

/// The desktop's state (see the module documentation).
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DesktopState {
    /// The bundle id of the app in front (`com.apple.Safari`); none where it is not known.
    pub front_bundle: OptionString,
    /// The name of the app in front.
    pub front_name: OptionString,
    /// The pointer's place on the desktop, in points from the top left of the primary display; none
    /// where it is not known.
    pub pointer: OptionScreenPosition,
    /// The screen is locked, or the screen saver shows.
    pub locked: bool,
    /// The main display is asleep.
    pub display_asleep: bool,
}

#[cfg(not(target_os = "macos"))]
mod platform {
    /// Nothing is known on this platform yet.
    pub(super) fn read() -> super::DesktopState {
        super::DesktopState::UNKNOWN
    }
}

impl DesktopState {
    /// What a platform without a reading reports: nothing is known, the screen is not locked.
    pub const UNKNOWN: DesktopState = DesktopState {
        front_bundle: OptionString::None,
        front_name: OptionString::None,
        pointer: OptionScreenPosition::None,
        locked: false,
        display_asleep: false,
    };

    /// What a headless or E2E run reports when no script says otherwise: the same.
    pub const HEADLESS: DesktopState = DesktopState::UNKNOWN;

    /// The desktop now (see the module documentation).
    #[must_use]
    pub fn query() -> DesktopState {
        if crate::desktop::extra::sensors::headless_run() {
            let hooks = azul_core::run_mode::test_hooks();
            return Self::of_file(hooks.desktop_state_file.as_deref().map(std::path::Path::new));
        }
        platform::read()
    }

    /// [`DesktopState::HEADLESS`].
    #[must_use]
    pub fn headless() -> DesktopState {
        DesktopState::HEADLESS
    }

    /// The desktop a script says (the words of [`script`]); what the script cannot say is not known.
    #[must_use]
    pub fn from_script(text: &str) -> DesktopState {
        let s = script::parse(text);
        DesktopState {
            front_bundle: s.front.as_ref().map_or(OptionString::None, |(b, _)| OptionString::Some(AzString::from(b.clone()))),
            front_name: s.front.as_ref().map_or(OptionString::None, |(_, n)| OptionString::Some(AzString::from(n.clone()))),
            pointer: s.pointer.map_or(OptionScreenPosition::None, |(x, y)| OptionScreenPosition::Some(ScreenPosition::new(x, y))),
            locked: s.locked,
            display_asleep: s.display_asleep,
        }
    }

    /// The desktop the file at `path` says; without a file, or one that cannot be read,
    /// [`DesktopState::HEADLESS`].
    fn of_file(path: Option<&std::path::Path>) -> DesktopState {
        crate::desktop::extra::sensors::switch_file_words(path).map_or(DesktopState::HEADLESS, |text| Self::from_script(&text))
    }
}

#[cfg(test)]
#[path = "desktop_state_test.rs"]
mod desktop_state_test;
