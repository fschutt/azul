//! System-wide ("global") hotkeys - the platform-agnostic model.
//!
//! A global hotkey is a key combination the OS delivers to THIS app even while
//! another app has the keyboard focus: a launcher's summon key, a recorder's
//! start/stop, a push-to-talk. The OS plumbing lives in `azul-dll`
//! (`desktop/global_hotkey`), the process-wide registry and the fire mailbox in
//! `azul_layout::managers::global_hotkey`. This module only defines what the
//! backends agree on: the combination, its id, and what can go wrong.
//!
//! # App-wide, never per window
//!
//! Every platform registers a hotkey for the PROCESS (Carbon: the application
//! event target; Win32: the registering thread's queue; X11: the root window;
//! the portal: the app's D-Bus session), and none of them has a notion of
//! "this hotkey belongs to window 2". A per-window API would be a lie that
//! breaks the moment its window closes while the grab stays. So the id is
//! app-wide, the callback carries its own `RefAny`, and it runs against the
//! app's first window, exactly like a tray menu click does.
//!
//! # One combination, normalised
//!
//! [`GlobalHotkey`] is four modifier flags plus ONE key. Left and right
//! modifiers are the same modifier (no OS can grab "right Ctrl only"), the
//! order in which an accelerator string names them is irrelevant, and equality
//! is plain struct equality - which is what makes "register the same
//! combination twice" detectable at all.
//!
//! `meta` is the PHYSICAL Cmd / Windows / Super key. The menu convention that
//! `LWin` in a [`VirtualKeyCodeCombo`] means "the platform's primary modifier"
//! (Cmd on macOS, Ctrl elsewhere) is honoured by [`GlobalHotkey::from_combo`],
//! and the accelerator parser has `CmdOrCtrl` for the same thing.

use alloc::string::String;
use core::fmt;

use azul_css::AzString;

use crate::window::{VirtualKeyCode, VirtualKeyCodeCombo};

/// The modifiers a global hotkey requires, resolved to physical keys.
///
/// `meta` is Cmd on macOS, the Windows key on Windows and Super on Linux.
/// `alt` is Option on macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub struct HotkeyModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl HotkeyModifiers {
    /// No modifier at all.
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
        meta: false,
    };

    /// No modifier is required.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        !(self.ctrl || self.alt || self.shift || self.meta)
    }

    /// At least one modifier other than Shift is required. Shift alone does
    /// not make a character key safe to grab: Shift+K is how every other app
    /// receives a capital K.
    #[must_use]
    pub const fn has_non_shift(&self) -> bool {
        self.ctrl || self.alt || self.meta
    }

    /// Both sets together.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self {
            ctrl: self.ctrl || other.ctrl,
            alt: self.alt || other.alt,
            shift: self.shift || other.shift,
            meta: self.meta || other.meta,
        }
    }

    /// The platform's PRIMARY shortcut modifier: Cmd (`meta`) when `mac`,
    /// Ctrl otherwise. The same rule `menu::accelerator_matches` applies to
    /// `LWin` in a menu accelerator.
    #[must_use]
    pub const fn primary_for(mac: bool) -> Self {
        if mac {
            Self {
                meta: true,
                ..Self::NONE
            }
        } else {
            Self {
                ctrl: true,
                ..Self::NONE
            }
        }
    }

    /// [`Self::primary_for`] for the host this process runs on.
    #[must_use]
    pub fn primary() -> Self {
        Self::primary_for(crate::window::mac_shortcut_conventions())
    }
}

/// The app-wide handle of one registered global hotkey.
///
/// Ids are never reused within a process: an id that was unregistered stays
/// dead, so a stale id held by the app cannot silently start meaning a
/// different combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct GlobalHotkeyId {
    pub id: u32,
}

impl_option!(
    GlobalHotkeyId,
    OptionGlobalHotkeyId,
    [Debug, Clone, Copy, PartialEq, Eq, Hash]
);

/// One system-wide key combination: modifiers plus exactly one key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct GlobalHotkey {
    pub modifiers: HotkeyModifiers,
    pub key: VirtualKeyCode,
}

/// Why a global hotkey could not be registered (or stopped working).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum GlobalHotkeyError {
    /// The accelerator is not a usable global hotkey; the string says why
    /// (no key, two keys, a bare letter that would swallow typing, ...).
    InvalidAccelerator(AzString),
    /// THIS app already holds the same combination, under the carried id.
    AlreadyRegistered(GlobalHotkeyId),
    /// Another application - or the system itself - owns the combination.
    TakenByAnotherApp,
    /// The key does not exist on the current keyboard layout (X11 has no
    /// keycode for its keysym, Win32 no virtual-key code, ...).
    KeyNotMappable,
    /// The desktop asked the user and the user declined (the Wayland
    /// `GlobalShortcuts` portal's dialog was cancelled).
    Denied,
    /// This platform has global hotkeys in principle but not in this session;
    /// the string says why (e.g. no `GlobalShortcuts` portal on a Wayland
    /// desktop, no X display).
    Unavailable(AzString),
    /// No global-hotkey backend exists on this platform (iOS, Android, web).
    Unsupported,
    /// The platform refused for a reason of its own; the string carries it.
    Platform(AzString),
}

impl fmt::Display for GlobalHotkeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAccelerator(why) => write!(f, "invalid global hotkey: {}", why.as_str()),
            Self::AlreadyRegistered(id) => write!(
                f,
                "this app already registered that combination (global hotkey {})",
                id.id
            ),
            Self::TakenByAnotherApp => write!(
                f,
                "the combination is taken: another application or the system already owns it"
            ),
            Self::KeyNotMappable => {
                write!(f, "the key does not exist on the current keyboard layout")
            }
            Self::Denied => write!(f, "the desktop's shortcut dialog was declined"),
            Self::Unavailable(why) => {
                write!(f, "global hotkeys are unavailable here: {}", why.as_str())
            }
            Self::Unsupported => write!(f, "global hotkeys are not supported on this platform"),
            Self::Platform(why) => write!(f, "the platform refused the hotkey: {}", why.as_str()),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for GlobalHotkeyError {}

impl GlobalHotkeyError {
    /// The message [`fmt::Display`] prints, as an `AzString` for the C API.
    #[must_use]
    pub fn to_display_string(&self) -> AzString {
        AzString::from(alloc::format!("{}", self))
    }
}

/// Where a registration stands.
///
/// Most platforms answer a registration on the spot (`Active` or an error).
/// The Wayland portal cannot: binding a shortcut may show the user a dialog,
/// so the registration is `Pending` until the desktop answers, and a refusal
/// arrives later as `Failed`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum GlobalHotkeyStatus {
    /// No such registration (never registered, or unregistered).
    NotRegistered,
    /// Asked for; the desktop has not answered yet.
    Pending,
    /// Grabbed: pressing the combination runs the callback.
    Active,
    /// The desktop refused after the fact. The registration stays readable
    /// (with its reason) until the app unregisters it.
    Failed(GlobalHotkeyError),
}

impl GlobalHotkeyStatus {
    /// `Pending` or `Active`: the registration counts as held.
    #[must_use]
    pub const fn is_live(&self) -> bool {
        matches!(self, Self::Pending | Self::Active)
    }
}

impl_result!(
    GlobalHotkeyId,
    GlobalHotkeyError,
    ResultGlobalHotkeyIdGlobalHotkeyError,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

impl_result!(
    GlobalHotkey,
    GlobalHotkeyError,
    ResultGlobalHotkeyGlobalHotkeyError,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

fn invalid(why: String) -> GlobalHotkeyError {
    GlobalHotkeyError::InvalidAccelerator(AzString::from(why))
}

// RED SKELETON: the types above are final; the logic below is stubbed so the
// tests in `layout/tests/global_hotkeys.rs` compile and fail. The next commit
// implements it.

impl GlobalHotkey {
    /// A combination from its parts. Not validated - see [`Self::validate`].
    #[must_use]
    pub const fn new(modifiers: HotkeyModifiers, key: VirtualKeyCode) -> Self {
        Self { modifiers, key }
    }

    /// Parse an accelerator string for the host platform.
    ///
    /// # Errors
    /// [`GlobalHotkeyError::InvalidAccelerator`] with the reason.
    pub fn parse(accelerator: &str) -> Result<Self, GlobalHotkeyError> {
        Self::parse_for(accelerator, crate::window::mac_shortcut_conventions())
    }

    /// Parse an accelerator string as the given platform reads it.
    ///
    /// # Errors
    /// [`GlobalHotkeyError::InvalidAccelerator`] with the reason.
    pub fn parse_for(accelerator: &str, mac: bool) -> Result<Self, GlobalHotkeyError> {
        let _ = (accelerator, mac);
        Err(invalid(String::from("not implemented yet")))
    }

    /// Convert a menu-style [`VirtualKeyCodeCombo`] for the host platform.
    ///
    /// # Errors
    /// See [`Self::from_combo_for`].
    pub fn from_combo(combo: &VirtualKeyCodeCombo) -> Result<Self, GlobalHotkeyError> {
        Self::from_combo_for(combo, crate::window::mac_shortcut_conventions())
    }

    /// Convert a menu-style [`VirtualKeyCodeCombo`].
    ///
    /// # Errors
    /// [`GlobalHotkeyError::InvalidAccelerator`].
    pub fn from_combo_for(
        combo: &VirtualKeyCodeCombo,
        mac: bool,
    ) -> Result<Self, GlobalHotkeyError> {
        let _ = (combo, mac);
        Err(invalid(String::from("not implemented yet")))
    }

    /// Is this a combination a global hotkey may use?
    ///
    /// # Errors
    /// [`GlobalHotkeyError::InvalidAccelerator`] with the reason.
    pub fn validate(&self) -> Result<(), GlobalHotkeyError> {
        Ok(())
    }

    /// The combination as a person reads it on the host platform.
    #[must_use]
    pub fn to_display_string(&self) -> AzString {
        AzString::from(self.to_display_string_for(crate::window::mac_shortcut_conventions()))
    }

    /// The combination as [`Self::parse_for`] reads it back.
    #[must_use]
    pub fn to_display_string_for(&self, mac: bool) -> String {
        let _ = mac;
        String::new()
    }
}

/// The name a person reads for `key`.
#[must_use]
pub fn key_display_name(key: VirtualKeyCode) -> Option<&'static str> {
    let _ = key;
    None
}

/// The xkb keysym NAME of `key`.
#[must_use]
pub fn xkb_keysym_name(key: VirtualKeyCode) -> Option<&'static str> {
    let _ = key;
    None
}

/// The `preferred_trigger` of the `GlobalShortcuts` portal.
#[must_use]
pub fn portal_trigger(hotkey: &GlobalHotkey) -> Option<String> {
    let _ = hotkey;
    None
}
