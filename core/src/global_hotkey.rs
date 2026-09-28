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

use crate::window::{VirtualKeyCode, VirtualKeyCode as K, VirtualKeyCodeCombo};

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

impl GlobalHotkey {
    /// A combination from its parts. Not validated - see [`Self::validate`].
    #[must_use]
    pub const fn create(modifiers: HotkeyModifiers, key: VirtualKeyCode) -> Self {
        Self { modifiers, key }
    }

    /// Parse an accelerator string for the host platform, e.g.
    /// `"Cmd+Shift+K"`, `"Ctrl+Alt+K"`, `"CmdOrCtrl+Shift+Space"`, `"F13"`.
    ///
    /// See [`Self::parse_for`] for the grammar.
    ///
    /// # Errors
    /// [`GlobalHotkeyError::InvalidAccelerator`] with the reason.
    pub fn parse(accelerator: &str) -> Result<Self, GlobalHotkeyError> {
        Self::parse_for(accelerator, crate::window::mac_shortcut_conventions())
    }

    /// Parse an accelerator string as the given platform reads it.
    ///
    /// Parts are separated by `+`, case and surrounding spaces are ignored,
    /// and the order of the modifiers is irrelevant. Modifiers:
    ///
    /// - `Ctrl` / `Control`
    /// - `Alt` / `Option` / `Opt`
    /// - `Shift`
    /// - `Cmd` / `Command` / `Super` / `Win` / `Meta` / `Logo` - the physical Cmd / Windows / Super
    ///   key
    /// - `CmdOrCtrl` / `CommandOrControl` / `Primary` - Cmd when `mac`, Ctrl otherwise
    ///
    /// plus exactly ONE key: a letter, a digit, `F1`..`F24`, `Space`, `Enter`,
    /// `Tab`, `Escape`, `Backspace`, `Delete`, `Insert`, `Home`, `End`,
    /// `PageUp`, `PageDown`, the arrows (`Left` / `ArrowLeft`, ...),
    /// `PrintScreen`, `Pause`, `ScrollLock`, the punctuation names (`Minus`,
    /// `Equals`, `Plus`, `Comma`, `Period`, `Slash`, `Backslash`, `Semicolon`,
    /// `Apostrophe`, `Grave`, `BracketLeft`, `BracketRight`, or the character
    /// itself), `Numpad0`..`Numpad9` and friends, and the media keys
    /// (`MediaPlayPause`, `MediaStop`, `MediaNextTrack`, `MediaPrevTrack`,
    /// `VolumeMute`, `VolumeUp`, `VolumeDown`).
    ///
    /// The result is validated ([`Self::validate`]).
    ///
    /// # Errors
    /// [`GlobalHotkeyError::InvalidAccelerator`] with the reason.
    pub fn parse_for(accelerator: &str, mac: bool) -> Result<Self, GlobalHotkeyError> {
        if accelerator.trim().is_empty() {
            return Err(invalid(String::from("the accelerator is empty")));
        }
        let mut modifiers = HotkeyModifiers::NONE;
        let mut key: Option<VirtualKeyCode> = None;
        for raw in accelerator.split('+') {
            let token = raw.trim();
            if token.is_empty() {
                return Err(invalid(alloc::format!(
                    "{accelerator:?} has an empty part (write the + key as \"Plus\")"
                )));
            }
            if let Some(m) = modifier_from_token(token, mac) {
                modifiers = modifiers.union(m);
                continue;
            }
            let Some(k) = key_from_token(token) else {
                return Err(invalid(alloc::format!(
                    "{token:?} is neither a modifier nor a key a global hotkey can use"
                )));
            };
            if let Some(previous) = key {
                return Err(invalid(alloc::format!(
                    "{accelerator:?} names two keys ({} and {}): a hotkey is modifiers plus ONE key",
                    key_display_name(previous).unwrap_or("?"),
                    key_display_name(k).unwrap_or("?"),
                )));
            }
            key = Some(k);
        }
        let Some(key) = key else {
            return Err(invalid(alloc::format!(
                "{accelerator:?} names no key: a hotkey needs one key besides its modifiers"
            )));
        };
        let hotkey = Self { modifiers, key };
        hotkey.validate()?;
        Ok(hotkey)
    }

    /// Convert a menu-style [`VirtualKeyCodeCombo`] for the host platform.
    ///
    /// # Errors
    /// See [`Self::from_combo_for`].
    pub fn from_combo(combo: &VirtualKeyCodeCombo) -> Result<Self, GlobalHotkeyError> {
        Self::from_combo_for(combo, crate::window::mac_shortcut_conventions())
    }

    /// Convert a menu-style [`VirtualKeyCodeCombo`], with the menu
    /// accelerator's rules: `LControl`/`RControl`, `LShift`/`RShift` and
    /// `LAlt`/`RAlt` are the modifiers (left and right are the same), `LWin` /
    /// `RWin` mean the platform's PRIMARY modifier (Cmd when `mac`, Ctrl
    /// otherwise) and every other code is the key - exactly one.
    ///
    /// # Errors
    /// [`GlobalHotkeyError::InvalidAccelerator`] for no key, two keys, or a
    /// combination [`Self::validate`] rejects.
    pub fn from_combo_for(
        combo: &VirtualKeyCodeCombo,
        mac: bool,
    ) -> Result<Self, GlobalHotkeyError> {
        let mut modifiers = HotkeyModifiers::NONE;
        let mut key: Option<VirtualKeyCode> = None;
        for k in combo.keys.as_ref() {
            let add = match k {
                K::LControl | K::RControl => HotkeyModifiers {
                    ctrl: true,
                    ..HotkeyModifiers::NONE
                },
                K::LShift | K::RShift => HotkeyModifiers {
                    shift: true,
                    ..HotkeyModifiers::NONE
                },
                K::LAlt | K::RAlt => HotkeyModifiers {
                    alt: true,
                    ..HotkeyModifiers::NONE
                },
                K::LWin | K::RWin => HotkeyModifiers::primary_for(mac),
                other => {
                    if let Some(previous) = key {
                        return Err(invalid(alloc::format!(
                            "the combination names two keys ({previous:?} and {other:?}): a \
                             hotkey is modifiers plus ONE key"
                        )));
                    }
                    key = Some(*other);
                    continue;
                }
            };
            modifiers = modifiers.union(add);
        }
        let Some(key) = key else {
            return Err(invalid(String::from(
                "the combination names no key besides its modifiers",
            )));
        };
        let hotkey = Self { modifiers, key };
        hotkey.validate()?;
        Ok(hotkey)
    }

    /// Is this a combination a global hotkey may use?
    ///
    /// - The key must not itself be a modifier or a lock key.
    /// - The key must be one the platforms can name (see [`Self::parse_for`]).
    /// - A key that TYPES something or moves a caret (letters, digits, punctuation, Space, Enter,
    ///   Tab, Backspace, the arrows, ...) needs Ctrl, Alt or Cmd/Super: grabbing a bare or
    ///   Shift-only `K` would swallow that key in every other application for as long as this app
    ///   runs. Function keys, Print Screen, Pause, Scroll Lock and the media keys may stand alone.
    ///
    /// # Errors
    /// [`GlobalHotkeyError::InvalidAccelerator`] with the reason.
    pub fn validate(&self) -> Result<(), GlobalHotkeyError> {
        if matches!(
            self.key,
            K::LControl
                | K::RControl
                | K::LShift
                | K::RShift
                | K::LAlt
                | K::RAlt
                | K::LWin
                | K::RWin
        ) {
            return Err(invalid(alloc::format!(
                "{:?} is a modifier; a hotkey needs one non-modifier key",
                self.key
            )));
        }
        let Some(name) = key_display_name(self.key) else {
            return Err(invalid(alloc::format!(
                "{:?} cannot be used in a global hotkey",
                self.key
            )));
        };
        if !may_stand_alone(self.key) && !self.modifiers.has_non_shift() {
            return Err(invalid(alloc::format!(
                "{name} needs Ctrl, Alt or Cmd/Super: grabbing it bare or with Shift alone would \
                 swallow it in every other application"
            )));
        }
        Ok(())
    }

    /// The combination as a person reads it on the host platform:
    /// `Cmd+Shift+K` on macOS, `Ctrl+Alt+K` on Windows (`Win` for the Windows
    /// key) and Linux (`Super`).
    #[must_use]
    pub fn to_display_string(&self) -> AzString {
        let mac = crate::window::mac_shortcut_conventions();
        let meta = if mac {
            "Cmd"
        } else if cfg!(target_os = "windows") {
            "Win"
        } else {
            "Super"
        };
        AzString::from(self.format_with(mac, meta))
    }

    /// The combination as [`Self::parse_for`] reads it back: `Cmd+Ctrl+Option+Shift+K`
    /// order and names when `mac`, `Ctrl+Alt+Shift+Super+K` otherwise.
    #[must_use]
    pub fn to_display_string_for(&self, mac: bool) -> String {
        self.format_with(mac, if mac { "Cmd" } else { "Super" })
    }

    fn format_with(&self, mac: bool, meta_name: &str) -> String {
        let mut out = String::new();
        let mut push = |part: &str| {
            if !out.is_empty() {
                out.push('+');
            }
            out.push_str(part);
        };
        // Cmd leads on a Mac ("Cmd+Shift+K", how Mac users say it); the
        // Windows / Super key trails elsewhere ("Ctrl+Alt+Super+K").
        if mac && self.modifiers.meta {
            push(meta_name);
        }
        if self.modifiers.ctrl {
            push("Ctrl");
        }
        if self.modifiers.alt {
            push(if mac { "Option" } else { "Alt" });
        }
        if self.modifiers.shift {
            push("Shift");
        }
        if !mac && self.modifiers.meta {
            push(meta_name);
        }
        push(key_display_name(self.key).unwrap_or("?"));
        out
    }
}

/// Every key a global hotkey can use: `(key, display name, xkb keysym name)`.
///
/// ONE table on purpose: the parser, the display string, the X11 grab
/// (`XStringToKeysym` of the third column) and the Wayland portal's trigger
/// string all read it, so a key cannot be parseable and ungrabbable.
const NAMED_KEYS: &[(VirtualKeyCode, &str, &str)] = &[
    (K::A, "A", "a"),
    (K::B, "B", "b"),
    (K::C, "C", "c"),
    (K::D, "D", "d"),
    (K::E, "E", "e"),
    (K::F, "F", "f"),
    (K::G, "G", "g"),
    (K::H, "H", "h"),
    (K::I, "I", "i"),
    (K::J, "J", "j"),
    (K::K, "K", "k"),
    (K::L, "L", "l"),
    (K::M, "M", "m"),
    (K::N, "N", "n"),
    (K::O, "O", "o"),
    (K::P, "P", "p"),
    (K::Q, "Q", "q"),
    (K::R, "R", "r"),
    (K::S, "S", "s"),
    (K::T, "T", "t"),
    (K::U, "U", "u"),
    (K::V, "V", "v"),
    (K::W, "W", "w"),
    (K::X, "X", "x"),
    (K::Y, "Y", "y"),
    (K::Z, "Z", "z"),
    (K::Key0, "0", "0"),
    (K::Key1, "1", "1"),
    (K::Key2, "2", "2"),
    (K::Key3, "3", "3"),
    (K::Key4, "4", "4"),
    (K::Key5, "5", "5"),
    (K::Key6, "6", "6"),
    (K::Key7, "7", "7"),
    (K::Key8, "8", "8"),
    (K::Key9, "9", "9"),
    (K::F1, "F1", "F1"),
    (K::F2, "F2", "F2"),
    (K::F3, "F3", "F3"),
    (K::F4, "F4", "F4"),
    (K::F5, "F5", "F5"),
    (K::F6, "F6", "F6"),
    (K::F7, "F7", "F7"),
    (K::F8, "F8", "F8"),
    (K::F9, "F9", "F9"),
    (K::F10, "F10", "F10"),
    (K::F11, "F11", "F11"),
    (K::F12, "F12", "F12"),
    (K::F13, "F13", "F13"),
    (K::F14, "F14", "F14"),
    (K::F15, "F15", "F15"),
    (K::F16, "F16", "F16"),
    (K::F17, "F17", "F17"),
    (K::F18, "F18", "F18"),
    (K::F19, "F19", "F19"),
    (K::F20, "F20", "F20"),
    (K::F21, "F21", "F21"),
    (K::F22, "F22", "F22"),
    (K::F23, "F23", "F23"),
    (K::F24, "F24", "F24"),
    (K::Escape, "Escape", "Escape"),
    (K::Space, "Space", "space"),
    (K::Return, "Enter", "Return"),
    (K::Tab, "Tab", "Tab"),
    (K::Back, "Backspace", "BackSpace"),
    (K::Delete, "Delete", "Delete"),
    (K::Insert, "Insert", "Insert"),
    (K::Home, "Home", "Home"),
    (K::End, "End", "End"),
    (K::PageUp, "PageUp", "Page_Up"),
    (K::PageDown, "PageDown", "Page_Down"),
    (K::Left, "Left", "Left"),
    (K::Right, "Right", "Right"),
    (K::Up, "Up", "Up"),
    (K::Down, "Down", "Down"),
    (K::Snapshot, "PrintScreen", "Print"),
    (K::Pause, "Pause", "Pause"),
    (K::Scroll, "ScrollLock", "Scroll_Lock"),
    (K::Minus, "Minus", "minus"),
    (K::Equals, "Equals", "equal"),
    (K::Plus, "Plus", "plus"),
    (K::Comma, "Comma", "comma"),
    (K::Period, "Period", "period"),
    (K::Slash, "Slash", "slash"),
    (K::Backslash, "Backslash", "backslash"),
    (K::Semicolon, "Semicolon", "semicolon"),
    (K::Apostrophe, "Apostrophe", "apostrophe"),
    (K::Grave, "Grave", "grave"),
    (K::LBracket, "BracketLeft", "bracketleft"),
    (K::RBracket, "BracketRight", "bracketright"),
    (K::Numpad0, "Numpad0", "KP_0"),
    (K::Numpad1, "Numpad1", "KP_1"),
    (K::Numpad2, "Numpad2", "KP_2"),
    (K::Numpad3, "Numpad3", "KP_3"),
    (K::Numpad4, "Numpad4", "KP_4"),
    (K::Numpad5, "Numpad5", "KP_5"),
    (K::Numpad6, "Numpad6", "KP_6"),
    (K::Numpad7, "Numpad7", "KP_7"),
    (K::Numpad8, "Numpad8", "KP_8"),
    (K::Numpad9, "Numpad9", "KP_9"),
    (K::NumpadAdd, "NumpadAdd", "KP_Add"),
    (K::NumpadSubtract, "NumpadSubtract", "KP_Subtract"),
    (K::NumpadMultiply, "NumpadMultiply", "KP_Multiply"),
    (K::NumpadDivide, "NumpadDivide", "KP_Divide"),
    (K::NumpadDecimal, "NumpadDecimal", "KP_Decimal"),
    (K::NumpadEnter, "NumpadEnter", "KP_Enter"),
    (K::PlayPause, "MediaPlayPause", "XF86AudioPlay"),
    (K::MediaStop, "MediaStop", "XF86AudioStop"),
    (K::NextTrack, "MediaNextTrack", "XF86AudioNext"),
    (K::PrevTrack, "MediaPrevTrack", "XF86AudioPrev"),
    (K::Mute, "VolumeMute", "XF86AudioMute"),
    (K::VolumeUp, "VolumeUp", "XF86AudioRaiseVolume"),
    (K::VolumeDown, "VolumeDown", "XF86AudioLowerVolume"),
];

/// Spellings the parser accepts besides the display names (lower case).
const KEY_ALIASES: &[(&str, VirtualKeyCode)] = &[
    ("esc", K::Escape),
    ("return", K::Return),
    ("bksp", K::Back),
    ("del", K::Delete),
    ("ins", K::Insert),
    ("pgup", K::PageUp),
    ("pgdn", K::PageDown),
    ("pgdown", K::PageDown),
    ("arrowleft", K::Left),
    ("arrowright", K::Right),
    ("arrowup", K::Up),
    ("arrowdown", K::Down),
    ("print", K::Snapshot),
    ("prtsc", K::Snapshot),
    ("snapshot", K::Snapshot),
    ("scroll", K::Scroll),
    ("-", K::Minus),
    ("=", K::Equals),
    ("equal", K::Equals),
    (",", K::Comma),
    (".", K::Period),
    ("/", K::Slash),
    ("\\", K::Backslash),
    (";", K::Semicolon),
    ("'", K::Apostrophe),
    ("quote", K::Apostrophe),
    ("`", K::Grave),
    ("backquote", K::Grave),
    ("backtick", K::Grave),
    ("[", K::LBracket),
    ("]", K::RBracket),
    ("leftbracket", K::LBracket),
    ("rightbracket", K::RBracket),
    ("playpause", K::PlayPause),
    ("mediaplay", K::PlayPause),
    ("medianext", K::NextTrack),
    ("nexttrack", K::NextTrack),
    ("mediaprev", K::PrevTrack),
    ("prevtrack", K::PrevTrack),
    ("mediaprevioustrack", K::PrevTrack),
    ("mute", K::Mute),
];

/// The name a person reads for `key`, or `None` for a key a global hotkey
/// cannot use.
#[must_use]
pub fn key_display_name(key: VirtualKeyCode) -> Option<&'static str> {
    NAMED_KEYS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, display, _)| *display)
}

/// The xkb keysym NAME of `key` (`"k"`, `"F5"`, `"Page_Up"`,
/// `"XF86AudioPlay"`), or `None` for a key a global hotkey cannot use.
///
/// The X11 backend turns it into a keysym with `XStringToKeysym` and a
/// keycode with `XKeysymToKeycode`; the Wayland portal takes it verbatim in a
/// shortcut's `preferred_trigger`.
#[must_use]
pub fn xkb_keysym_name(key: VirtualKeyCode) -> Option<&'static str> {
    NAMED_KEYS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, xkb)| *xkb)
}

/// The `preferred_trigger` the xdg-desktop-portal `GlobalShortcuts`
/// interface takes, in the XDG shortcuts format: the modifiers `CTRL`, `ALT`,
/// `SHIFT`, `LOGO` (in that order) and the xkb keysym name, joined by `+` -
/// `"CTRL+ALT+k"`, `"SHIFT+LOGO+k"`, `"F13"`.
///
/// `None` for a key the portal cannot name.
#[must_use]
pub fn portal_trigger(hotkey: &GlobalHotkey) -> Option<String> {
    let name = xkb_keysym_name(hotkey.key)?;
    let mut out = String::new();
    let mut push = |part: &str| {
        if !out.is_empty() {
            out.push('+');
        }
        out.push_str(part);
    };
    if hotkey.modifiers.ctrl {
        push("CTRL");
    }
    if hotkey.modifiers.alt {
        push("ALT");
    }
    if hotkey.modifiers.shift {
        push("SHIFT");
    }
    if hotkey.modifiers.meta {
        push("LOGO");
    }
    push(name);
    Some(out)
}

/// Keys that may be grabbed without Ctrl / Alt / Cmd: they type nothing and
/// move no caret, so owning them system-wide steals nothing from typing.
fn may_stand_alone(key: VirtualKeyCode) -> bool {
    matches!(
        key,
        K::F1
            | K::F2
            | K::F3
            | K::F4
            | K::F5
            | K::F6
            | K::F7
            | K::F8
            | K::F9
            | K::F10
            | K::F11
            | K::F12
            | K::F13
            | K::F14
            | K::F15
            | K::F16
            | K::F17
            | K::F18
            | K::F19
            | K::F20
            | K::F21
            | K::F22
            | K::F23
            | K::F24
            | K::Snapshot
            | K::Pause
            | K::Scroll
            | K::PlayPause
            | K::MediaStop
            | K::NextTrack
            | K::PrevTrack
            | K::Mute
            | K::VolumeUp
            | K::VolumeDown
    )
}

/// Is `token` one of `names`, ignoring ASCII case?
fn any_ci(names: &[&str], token: &str) -> bool {
    names.iter().any(|n| n.eq_ignore_ascii_case(token))
}

fn modifier_from_token(token: &str, mac: bool) -> Option<HotkeyModifiers> {
    if any_ci(&["ctrl", "control", "ctl"], token) {
        Some(HotkeyModifiers {
            ctrl: true,
            ..HotkeyModifiers::NONE
        })
    } else if any_ci(&["alt", "option", "opt"], token) {
        Some(HotkeyModifiers {
            alt: true,
            ..HotkeyModifiers::NONE
        })
    } else if any_ci(&["shift"], token) {
        Some(HotkeyModifiers {
            shift: true,
            ..HotkeyModifiers::NONE
        })
    } else if any_ci(&[
        "cmd", "command", "super", "win", "windows", "meta", "logo",
    ], token) {
        Some(HotkeyModifiers {
            meta: true,
            ..HotkeyModifiers::NONE
        })
    } else if any_ci(&[
        "cmdorctrl",
        "commandorcontrol",
        "ctrlorcmd",
        "primary",
    ], token) {
        Some(HotkeyModifiers::primary_for(mac))
    } else {
        None
    }
}

fn key_from_token(token: &str) -> Option<VirtualKeyCode> {
    NAMED_KEYS
        .iter()
        .find(|(_, display, _)| display.eq_ignore_ascii_case(token))
        .map(|(k, _, _)| *k)
        .or_else(|| {
            KEY_ALIASES
                .iter()
                .find(|(alias, _)| alias.eq_ignore_ascii_case(token))
                .map(|(_, k)| *k)
        })
}
