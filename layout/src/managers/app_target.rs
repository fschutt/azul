//! Which window an APP-LEVEL event runs against.
//!
//! A tray click, a notification click and a global hotkey belong to the app,
//! not to one of its windows - but their callbacks run through
//! `invoke_menu_callback`, and a `CallbackInfo` is built from a window. So
//! the shell has to pick one. It used to take "the first window" of its
//! registry, which meant three different arbitrary things:
//!
//! * macOS: a `BTreeMap` keyed by `NSWindow` pointer - allocation order;
//! * X11 / Wayland: a `HashMap` of X window ids / `wl_surface` pointers - random, and different
//!   between two runs of the same app;
//! * Win32: a `BTreeMap` of `HWND` values.
//!
//! # The rule
//!
//! **The most recently focused window, else the oldest.** Menus and tooltips
//! are only considered when nothing else is open: a hotkey that "lands in" a
//! context menu that happened to hold the focus would run the app's callback
//! against the menu's DOM.
//!
//! "Most recently focused" is what the user means: the hotkey acts on the
//! window they last worked in, and a tray click on the app they last used.
//! The global-hotkey design (`scripts/GLOBAL_HOTKEYS_DECLARATIVE_DESIGN_2026_09_28.md`,
//! §5.1) states the same rule for its owner pick, so the two agree.
//!
//! # The clock
//!
//! Every window gets a [`WindowActivationOrder`] when it is created, and each
//! backend's focus-in handler (`windowDidBecomeKey`, X11 `FocusIn`, Wayland
//! `wl_keyboard.enter`, Win32 `WM_SETFOCUS`) stamps it through
//! [`WindowActivationOrder::note_focused`]. Both read one process-wide
//! counter, so "newer" is a total order with no ties and no wall clock.

use core::sync::atomic::{AtomicU64, Ordering};

/// The process-wide activation clock. Starts at 1 so `0` can mean "never".
static CLOCK: AtomicU64 = AtomicU64::new(1);

fn tick() -> u64 {
    CLOCK.fetch_add(1, Ordering::Relaxed)
}

/// When a window was created and when it last gained the keyboard focus, on
/// the process-wide activation clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowActivationOrder {
    /// Stamped once, when the window's state is created.
    pub created: u64,
    /// Stamped on every focus-in; `0` = the window never had the focus.
    pub last_focused: u64,
}

impl WindowActivationOrder {
    /// The order of a window being created now: newer than every existing
    /// window, never focused.
    #[must_use]
    pub fn for_new_window() -> Self {
        Self {
            created: tick(),
            last_focused: 0,
        }
    }

    /// The window just gained the keyboard focus.
    pub fn note_focused(&mut self) {
        self.last_focused = tick();
    }

    /// Has the window ever had the focus?
    #[must_use]
    pub const fn was_ever_focused(&self) -> bool {
        self.last_focused != 0
    }
}

/// One window the shell could run an app-level callback against. `key` is
/// whatever the shell uses to find the window again (a registry pointer, an
/// id).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppTargetCandidate<K> {
    pub key: K,
    pub order: WindowActivationOrder,
    /// A menu or a tooltip: a target only when nothing else is open.
    pub transient: bool,
}

/// THE rule: the most recently focused window, else the oldest; transient
/// windows (menus, tooltips) only when no other window exists. `None` only
/// for an empty slice.
///
/// Independent of the slice's order, which is the point - the registries
/// that feed it iterate in pointer, hash or handle order.
#[must_use]
pub fn pick_app_target<K: Copy>(candidates: &[AppTargetCandidate<K>]) -> Option<K> {
    let any_regular = candidates.iter().any(|c| !c.transient);
    let eligible = |c: &&AppTargetCandidate<K>| !any_regular || !c.transient;
    candidates
        .iter()
        .filter(eligible)
        .filter(|c| c.order.was_ever_focused())
        .max_by_key(|c| c.order.last_focused)
        .or_else(|| {
            candidates
                .iter()
                .filter(eligible)
                .min_by_key(|c| c.order.created)
        })
        .map(|c| c.key)
}
