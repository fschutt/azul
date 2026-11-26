//! Unified `DesktopState`: the desktop's state, polled. See [`crate::unified`] and
//! `crate::desktop::extra::desktop_state` (the readings, and what they do and do not read).
//! api.json's `sensor` module names this path (`azul_dll::unified::desktop_state::DesktopState`).

#[cfg(az_desktop)]
pub use crate::desktop::extra::desktop_state::DesktopState;

/// wasm stub of the desktop `DesktopState` - IDENTICAL `#[repr(C)]` layout. A browser tells a page
/// nothing of the other apps or of the pointer outside it, so every query answers "not known".
#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DesktopState {
    pub front_bundle: azul_css::OptionString,
    pub front_name: azul_css::OptionString,
    pub pointer: azul_core::geom::OptionScreenPosition,
    pub locked: bool,
    pub display_asleep: bool,
}

#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
impl DesktopState {
    pub const UNKNOWN: DesktopState = DesktopState {
        front_bundle: azul_css::OptionString::None,
        front_name: azul_css::OptionString::None,
        pointer: azul_core::geom::OptionScreenPosition::None,
        locked: false,
        display_asleep: false,
    };
    pub const HEADLESS: DesktopState = DesktopState::UNKNOWN;

    #[must_use]
    pub fn query() -> DesktopState {
        DesktopState::UNKNOWN
    }

    #[must_use]
    pub fn headless() -> DesktopState {
        DesktopState::HEADLESS
    }
}
