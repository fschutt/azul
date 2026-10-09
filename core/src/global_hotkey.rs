//! System-wide ("global") hotkeys - the platform-agnostic model.
//!
//! A global hotkey is a key combination the OS delivers to THIS app even while
//! another app has the keyboard focus: a launcher's summon key, a recorder's
//! start/stop, a push-to-talk. The OS plumbing lives in `azul-dll`
//! (`desktop/global_hotkey`), the App-owned manager that reconciles the grabs
//! in `azul_layout::managers::global_hotkey`. This module only defines what
//! the backends and the app agree on: the combination, what can go wrong, and
//! the vocabulary an app DECLARES its hotkeys in.
//!
//! # Declared from state, not registered
//!
//! Hotkeys usually depend on app state (a setting, a mode, the shortcut the
//! user recorded), so an app does not register and unregister them: it
//! DECLARES the set it wants wherever it is handed an info -
//! `LayoutCallbackInfo::add_global_hotkey` in `layout()` (the shape of
//! `Dom::with_callback`, with the accelerator in the event filter's place),
//! or `GlobalHotkeysCallbackInfo::add_global_hotkey` in the `AppConfig`'s
//! callback for an app with no window. After every pass the engine makes the
//! OS grabs equal the union of every declaration: it grabs what is new,
//! releases what is gone, and swaps the callback of an accelerator that
//! stayed WITHOUT touching the OS.
//!
//! # Identity is the accelerator
//!
//! Every OS keys a grab on the combination, so the engine does too: one grab
//! per [`GlobalHotkey`] however many windows declare it, and exactly one
//! callback per press (the most recently focused declaring window, then the
//! oldest; a window's declaration shadows the app's).
//!
//! # One combination, normalised
//!
//! [`GlobalHotkey`] is four modifier flags plus ONE key. Left and right
//! modifiers are the same modifier (no OS can grab "right Ctrl only"), the
//! order in which an accelerator string names them is irrelevant, and equality
//! is plain struct equality - which is what makes the accelerator usable as
//! the identity at all.
//!
//! `meta` is the PHYSICAL Cmd / Windows / Super key. The menu convention that
//! `LWin` in a [`VirtualKeyCodeCombo`] means "the platform's primary modifier"
//! (Cmd on macOS, Ctrl elsewhere) is honoured by [`GlobalHotkey::from_combo`],
//! and the accelerator parser has `CmdOrCtrl` for the same thing.

use alloc::string::String;
use core::fmt;

use azul_css::AzString;

use crate::{
    callbacks::CoreCallback,
    refany::{OptionRefAny, RefAny},
    window::{VirtualKeyCode, VirtualKeyCode as K, VirtualKeyCodeCombo},
};

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

/// The OS-side handle of one grab: what a backend registers a combination
/// under and reports its presses and answers with (Carbon's
/// `EventHotKeyID.id`, the Win32 hotkey id, the portal shortcut's owner).
///
/// Engine-internal: an app names a hotkey by its accelerator, never by this.
/// Ids are never reused within a manager, so a press reported for a grab
/// that was released meanwhile cannot be mistaken for a newer one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct GlobalHotkeyId {
    pub id: u32,
}

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
    /// Not produced by the declarative manager (a second declaration of an
    /// accelerator is resolved by the owner rule, never refused); kept so
    /// the error type stays stable for the bindings.
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
        AzString::from(alloc::format!("{self}"))
    }
}

/// Where one accelerator stands.
///
/// Most platforms answer a grab on the spot (`Active` or `Failed`). The
/// Wayland portal cannot: binding a shortcut may show the user a dialog, so
/// the grab is `Pending` until the desktop answers.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum GlobalHotkeyStatus {
    /// Nobody declares it and no failure is remembered for it.
    NotRegistered,
    /// Asked for; the desktop has not answered yet.
    Pending,
    /// Grabbed: pressing the combination runs its owner's callback.
    Active,
    /// The platform refused, now or later. STICKY: the accelerator is not
    /// asked for again - declared or not - until the app retries it
    /// (`CallbackInfo::retry_global_hotkey`). Without that, an app whose
    /// first choice is taken and that falls back to a second one would flip
    /// between the two forever, and a declined Wayland dialog would come back
    /// on every relayout.
    Failed(GlobalHotkeyError),
}

impl GlobalHotkeyStatus {
    /// `Pending` or `Active`: the accelerator counts as held.
    #[must_use]
    pub const fn is_live(&self) -> bool {
        matches!(self, Self::Pending | Self::Active)
    }
}

/// One global hotkey a `layout()` pass (or the app) wants, and what runs when
/// it fires. The global-hotkey twin of `CoreCallbackData`: the accelerator
/// takes the event filter's place.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct GlobalHotkeyCallbackData {
    /// The combination - also the declaration's identity.
    pub hotkey: GlobalHotkey,
    /// What the desktop shows for it: the Wayland portal's approval dialog
    /// and its shortcut settings. Empty = the combination's display string.
    pub description: AzString,
    /// Runs when the combination is pressed, with a `CallbackInfo` of the
    /// window the press is delivered to.
    pub callback: CoreCallback,
    /// The data `callback` receives.
    pub refany: RefAny,
}

impl_option!(
    GlobalHotkeyCallbackData,
    OptionGlobalHotkeyCallbackData,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(
    GlobalHotkeyCallbackData,
    GlobalHotkeyCallbackDataVec,
    GlobalHotkeyCallbackDataVecDestructor,
    GlobalHotkeyCallbackDataVecDestructorType,
    GlobalHotkeyCallbackDataVecSlice,
    OptionGlobalHotkeyCallbackData
);
impl_vec_clone!(
    GlobalHotkeyCallbackData,
    GlobalHotkeyCallbackDataVec,
    GlobalHotkeyCallbackDataVecDestructor
);
impl_vec_mut!(GlobalHotkeyCallbackData, GlobalHotkeyCallbackDataVec);
impl_vec_debug!(GlobalHotkeyCallbackData, GlobalHotkeyCallbackDataVec);
impl_vec_partialeq!(GlobalHotkeyCallbackData, GlobalHotkeyCallbackDataVec);
impl_vec_eq!(GlobalHotkeyCallbackData, GlobalHotkeyCallbackDataVec);
impl_vec_partialord!(GlobalHotkeyCallbackData, GlobalHotkeyCallbackDataVec);
impl_vec_ord!(GlobalHotkeyCallbackData, GlobalHotkeyCallbackDataVec);
impl_vec_hash!(GlobalHotkeyCallbackData, GlobalHotkeyCallbackDataVec);

impl GlobalHotkeyCallbackData {
    /// A declaration without a description.
    #[must_use]
    pub const fn create(hotkey: GlobalHotkey, data: RefAny, callback: CoreCallback) -> Self {
        Self {
            hotkey,
            description: AzString::from_const_str(""),
            callback,
            refany: data,
        }
    }
}

/// Whose declaration a press of an accelerator runs, as seen from the window
/// (or the app callback) that asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum GlobalHotkeyOwner {
    /// The `AppConfig`'s list or its derived callback, and no window.
    App,
    /// The window whose `layout()` / `CallbackInfo` is asking.
    ThisWindow,
    /// Another window of this app (the most recently focused declarer, then
    /// the oldest window).
    OtherWindow,
    /// Nobody declares it any more; it is listed for its remembered failure.
    Nobody,
}

/// What an app can read about one accelerator.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct GlobalHotkeyInfo {
    pub hotkey: GlobalHotkey,
    pub status: GlobalHotkeyStatus,
    /// The trigger as the DESKTOP reports it. On Wayland the user may have
    /// picked another one in the portal's dialog; everywhere else it is the
    /// combination's display string.
    pub trigger: AzString,
    pub owner: GlobalHotkeyOwner,
}

impl_option!(
    GlobalHotkeyInfo,
    OptionGlobalHotkeyInfo,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(
    GlobalHotkeyInfo,
    GlobalHotkeyInfoVec,
    GlobalHotkeyInfoVecDestructor,
    GlobalHotkeyInfoVecDestructorType,
    GlobalHotkeyInfoVecSlice,
    OptionGlobalHotkeyInfo
);
impl_vec_clone!(
    GlobalHotkeyInfo,
    GlobalHotkeyInfoVec,
    GlobalHotkeyInfoVecDestructor
);
impl_vec_mut!(GlobalHotkeyInfo, GlobalHotkeyInfoVec);
impl_vec_debug!(GlobalHotkeyInfo, GlobalHotkeyInfoVec);
impl_vec_partialeq!(GlobalHotkeyInfo, GlobalHotkeyInfoVec);
impl_vec_eq!(GlobalHotkeyInfo, GlobalHotkeyInfoVec);
impl_vec_partialord!(GlobalHotkeyInfo, GlobalHotkeyInfoVec);
impl_vec_ord!(GlobalHotkeyInfo, GlobalHotkeyInfoVec);
impl_vec_hash!(GlobalHotkeyInfo, GlobalHotkeyInfoVec);

/// Where `hotkey` stands in a snapshot: its entry's status, or
/// `NotRegistered` when the snapshot does not list it.
#[must_use]
pub fn status_in(snapshot: &[GlobalHotkeyInfo], hotkey: &GlobalHotkey) -> GlobalHotkeyStatus {
    snapshot
        .iter()
        .find(|info| info.hotkey == *hotkey)
        .map_or(GlobalHotkeyStatus::NotRegistered, |info| {
            info.status.clone()
        })
}

/// Pressed or released. The backends report presses today; `Released` is
/// reserved for push-to-talk (open question Q5 of the design report).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum GlobalHotkeyState {
    Pressed,
    Released,
}

/// The press being delivered, readable from the fired callback
/// (`CallbackInfo::get_global_hotkey_event`), so that one callback can serve
/// several accelerators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct GlobalHotkeyEvent {
    pub hotkey: GlobalHotkey,
    pub state: GlobalHotkeyState,
    /// Milliseconds on the backend's clock; 0 where the OS gives none.
    pub timestamp_ms: u64,
}

impl_option!(
    GlobalHotkeyEvent,
    OptionGlobalHotkeyEvent,
    [Debug, Clone, Copy, PartialEq, Eq, Hash]
);

// ---------------------------------------------------------------------------
// The declaration recorder
// ---------------------------------------------------------------------------

/// More distinct global hotkeys than any real app declares; a callback
/// exceeding this is generating them programmatically. The tail is dropped
/// and the drain says so (`overflowed`).
pub const GLOBAL_HOTKEY_DECLARATION_CAP: usize = 256;

/// What one `layout()` (or `AppConfig` hotkeys callback) call declared,
/// drained on the same thread right after it returns.
#[derive(Debug, Clone, Default)]
pub struct RecordedGlobalHotkeys {
    /// One entry per accelerator; a duplicate replaced the earlier one.
    pub declared: Vec<GlobalHotkeyCallbackData>,
    /// The call read a status: re-run it when a status changes.
    pub read_status: bool,
    /// More than [`GLOBAL_HOTKEY_DECLARATION_CAP`] declarations: the tail
    /// was dropped.
    pub overflowed: bool,
}

/// Thread-local recorder behind `LayoutCallbackInfo::add_global_hotkey` and
/// [`GlobalHotkeysCallbackInfo::add_global_hotkey`].
///
/// A thread-local (rather than a field on the FFI-frozen, `Copy` info
/// structs) for the reason the size-query and style-dependency recorders use
/// one: the callback runs SYNCHRONOUSLY on the calling thread, and the engine
/// drains what it declared right after it returns.
#[cfg(feature = "std")]
mod recorder {
    use super::{GlobalHotkeyCallbackData, RecordedGlobalHotkeys, GLOBAL_HOTKEY_DECLARATION_CAP};

    std::thread_local! {
        static RECORDED: core::cell::RefCell<RecordedGlobalHotkeys> =
            const {
                core::cell::RefCell::new(RecordedGlobalHotkeys {
                    declared: Vec::new(),
                    read_status: false,
                    overflowed: false,
                })
            };
    }

    pub(super) fn declare(item: GlobalHotkeyCallbackData) {
        RECORDED.with(|recorded| {
            let mut recorded = recorded.borrow_mut();
            let earlier = recorded
                .declared
                .iter()
                .position(|d| d.hotkey == item.hotkey);
            if let Some(index) = earlier {
                // The last declaration of an accelerator wins.
                recorded.declared.remove(index);
            } else if recorded.declared.len() >= GLOBAL_HOTKEY_DECLARATION_CAP {
                recorded.overflowed = true;
                return;
            }
            recorded.declared.push(item);
        });
    }

    pub(super) fn read_status() {
        RECORDED.with(|recorded| recorded.borrow_mut().read_status = true);
    }

    pub(super) fn take() -> RecordedGlobalHotkeys {
        RECORDED.with(|recorded| core::mem::take(&mut *recorded.borrow_mut()))
    }
}

/// Record one declaration of the running callback.
#[cfg(feature = "std")]
pub(crate) fn record_declaration(item: GlobalHotkeyCallbackData) {
    recorder::declare(item);
}

/// Without `std` there is no thread-local to record into, and no platform
/// with global hotkeys: the declaration is accepted and dropped.
#[cfg(not(feature = "std"))]
pub(crate) fn record_declaration(_item: GlobalHotkeyCallbackData) {}

/// Record that the running callback read a status.
#[cfg(feature = "std")]
pub(crate) fn record_status_read() {
    recorder::read_status();
}

#[cfg(not(feature = "std"))]
pub(crate) fn record_status_read() {}

/// Drain what was declared since the last drain on THIS thread.
///
/// Call right after a `layout()` / `AppConfig` hotkeys callback returns, on the
/// same thread - and once right before it, to clear anything stale.
#[cfg(feature = "std")]
#[must_use]
pub fn take_recorded_global_hotkeys() -> RecordedGlobalHotkeys {
    recorder::take()
}

#[cfg(not(feature = "std"))]
#[must_use]
pub fn take_recorded_global_hotkeys() -> RecordedGlobalHotkeys {
    RecordedGlobalHotkeys::default()
}

// ---------------------------------------------------------------------------
// The AppConfig's derived set (apps with no window)
// ---------------------------------------------------------------------------

/// Derives the app-level global hotkeys from the app's state (the `RefAny` the
/// `App` was created with).
///
/// It serves an app with no `layout()` - a tray-only or background utility - or
/// hotkeys that belong to no window. Declares through
/// [`GlobalHotkeysCallbackInfo::add_global_hotkey`], exactly like `layout()`
/// does through `LayoutCallbackInfo`.
///
/// Runs once when the app starts, again after any callback returns
/// `Update::RefreshDom` / `RefreshDomAllWindows` (the only "the app state
/// may have changed" signal there is), and when a status it read changes.
/// Not on resize or theme changes: app-level hotkeys depend on no window.
pub type GlobalHotkeysCallbackType = extern "C" fn(RefAny, GlobalHotkeysCallbackInfo);

/// Wrapper around [`GlobalHotkeysCallbackType`] (see `AppConfig::
/// with_global_hotkeys_callback`).
#[repr(C)]
pub struct GlobalHotkeysCallback {
    pub cb: GlobalHotkeysCallbackType,
    /// For FFI: stores the foreign callable (e.g., `PyFunction`)
    /// Native Rust code sets this to None
    pub ctx: OptionRefAny,
}

impl_callback!(GlobalHotkeysCallback, GlobalHotkeysCallbackType);

impl GlobalHotkeysCallback {
    #[must_use]
    pub fn create(cb: GlobalHotkeysCallbackType) -> Self {
        Self {
            cb,
            ctx: OptionRefAny::None,
        }
    }
}

// Host-invoker plumbing for managed-FFI bindings (see core/src/host_invoker.rs).
crate::impl_managed_callback! {
    wrapper:        GlobalHotkeysCallback,
    info_ty:        GlobalHotkeysCallbackInfo,
    return_ty:      (),
    // unit default-return, spelled so clippy's unused_unit stays quiet.
    default_ret:    Default::default(),
    invoker_static: GLOBAL_HOTKEYS_CALLBACK_INVOKER,
    invoker_ty:     AzGlobalHotkeysCallbackInvoker,
    thunk_fn:       az_global_hotkeys_callback_thunk,
    setter_fn:      AzApp_setGlobalHotkeysCallbackInvoker,
    from_handle_fn: AzGlobalHotkeysCallback_createFromHostHandle,
    from_handle_byref_fn: AzGlobalHotkeysCallback_createFromHostHandleByref,
}

impl_option!(
    GlobalHotkeysCallback,
    OptionGlobalHotkeysCallback,
    copy = false,
    [Debug, Clone]
);

/// What the `AppConfig`'s hotkeys callback is handed: the same declaring
/// vocabulary as `LayoutCallbackInfo`, without a window.
///
/// `Copy` and pointer-sized like the other callback infos: it points at a
/// snapshot the engine owns for the duration of the call.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct GlobalHotkeysCallbackInfo {
    /// The app's global hotkeys when the call began (owner relative to the
    /// app).
    ref_data: *const GlobalHotkeyInfoVec,
    /// Pointer to the callable (`OptionRefAny`) for FFI language bindings.
    callable_ptr: *const OptionRefAny,
    /// Extension for future ABI stability (mutable data)
    _abi_mut: *mut core::ffi::c_void,
}

impl fmt::Debug for GlobalHotkeysCallbackInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GlobalHotkeysCallbackInfo")
            .finish_non_exhaustive()
    }
}

impl GlobalHotkeysCallbackInfo {
    /// An info reading `snapshot`, which must outlive every use of it (the
    /// engine builds it on the stack around one call).
    #[must_use]
    pub const fn new(snapshot: &GlobalHotkeyInfoVec) -> Self {
        Self {
            ref_data: core::ptr::from_ref::<GlobalHotkeyInfoVec>(snapshot),
            callable_ptr: core::ptr::null(),
            _abi_mut: core::ptr::null_mut(),
        }
    }

    /// Set the callable pointer for FFI language bindings.
    pub const fn set_callable_ptr(&mut self, callable: &OptionRefAny) {
        self.callable_ptr = core::ptr::from_ref::<OptionRefAny>(callable);
    }

    /// Get the callable for FFI language bindings (Python, etc.)
    #[must_use]
    pub fn get_ctx(&self) -> OptionRefAny {
        if self.callable_ptr.is_null() {
            OptionRefAny::None
        } else {
            // SAFETY: set by `invoke` for the duration of the call.
            unsafe { (*self.callable_ptr).clone() }
        }
    }

    /// Declare that, in the current app state, `hotkey` is a system-wide
    /// hotkey running `callback` with `data` - app-level, owned by no window.
    /// Same rules as `LayoutCallbackInfo::add_global_hotkey`: the WHOLE
    /// wanted set, the last duplicate wins, an unchanged accelerator is not
    /// touched at the OS.
    pub fn add_global_hotkey<C: Into<CoreCallback>>(
        &self,
        hotkey: GlobalHotkey,
        data: RefAny,
        callback: C,
    ) {
        record_declaration(GlobalHotkeyCallbackData::create(
            hotkey,
            data,
            callback.into(),
        ));
    }

    /// [`Self::add_global_hotkey`] with the text the desktop shows for it.
    pub fn add_global_hotkey_with_description<C: Into<CoreCallback>>(
        &self,
        hotkey: GlobalHotkey,
        description: AzString,
        data: RefAny,
        callback: C,
    ) {
        record_declaration(GlobalHotkeyCallbackData {
            hotkey,
            description,
            callback: callback.into(),
            refany: data,
        });
    }

    /// Where `hotkey` stood when this call began. RECORDED: a later status
    /// change runs the callback once more.
    #[must_use]
    pub fn get_global_hotkey_status(&self, hotkey: GlobalHotkey) -> GlobalHotkeyStatus {
        record_status_read();
        if self.ref_data.is_null() {
            return GlobalHotkeyStatus::NotRegistered;
        }
        // SAFETY: `ref_data` points at the engine's snapshot for this call.
        let snapshot = unsafe { &*self.ref_data };
        status_in(snapshot.as_ref(), &hotkey)
    }

    /// Every accelerator the app currently wants, holds or failed to get.
    /// RECORDED like [`Self::get_global_hotkey_status`].
    #[must_use]
    pub fn get_global_hotkeys(&self) -> GlobalHotkeyInfoVec {
        record_status_read();
        if self.ref_data.is_null() {
            return GlobalHotkeyInfoVec::from_const_slice(&[]);
        }
        // SAFETY: as above.
        unsafe { (*self.ref_data).clone() }
    }
}

impl crate::host_invoker::HostCtxCarrier for GlobalHotkeysCallbackInfo {
    fn install_host_ctx(&mut self, ctx: &OptionRefAny) {
        // Points at the wrapper's own `ctx`, which `invoke` borrows for the
        // whole call.
        self.set_callable_ptr(ctx);
    }
}

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

    /// Is this combination the key event `keyboard` describes? The key that
    /// just went down is this hotkey's key and EXACTLY its modifiers are
    /// held (left and right alike), so `Ctrl+S` does not fire on
    /// `Ctrl+Shift+S`. With it an app keeps one table of combinations -
    /// parsed with [`Self::parse`] (`CmdOrCtrl` is the host's primary
    /// modifier), shown with [`Self::to_display_string`] - and runs its
    /// window key handler through the same table.
    #[must_use]
    pub fn matches(&self, keyboard: &crate::window::KeyboardState) -> bool {
        keyboard.current_virtual_keycode.into_option() == Some(self.key)
            && self.modifiers
                == HotkeyModifiers {
                    ctrl: keyboard.ctrl_down(),
                    alt: keyboard.alt_down(),
                    shift: keyboard.shift_down(),
                    meta: keyboard.super_down(),
                }
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

/// The `preferred_trigger` the xdg-desktop-portal `GlobalShortcuts` interface
/// takes, in the XDG shortcuts format.
///
/// The modifiers `CTRL`, `ALT`, `SHIFT`, `LOGO` (in that order) and the xkb
/// keysym name, joined by `+` - `"CTRL+ALT+k"`, `"SHIFT+LOGO+k"`, `"F13"`.
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
const fn may_stand_alone(key: VirtualKeyCode) -> bool {
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
