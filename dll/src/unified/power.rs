//! Unified `PowerState` type + query. See [`crate::unified`] and
//! `crate::desktop::extra::power` (the per-platform readings).

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::power::*;

/// wasm stub of the desktop `PowerState` - IDENTICAL `#[repr(C)]` layout. A browser offers no
/// synchronous reading (the Battery Status API is a promise, idleness a permission prompt), so
/// every query answers the cautious [`PowerState::UNKNOWN`]: background work waits.
#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PowerState {
    pub on_mains: bool,
    pub idle_secs: u64,
}

#[cfg(target_arch = "wasm32")]
impl PowerState {
    pub const HEADLESS: PowerState = PowerState {
        on_mains: true,
        idle_secs: 0,
    };
    pub const UNKNOWN: PowerState = PowerState {
        on_mains: false,
        idle_secs: 0,
    };

    #[must_use]
    pub fn query() -> PowerState {
        PowerState::UNKNOWN
    }

    #[must_use]
    pub fn headless() -> PowerState {
        PowerState::HEADLESS
    }

    #[must_use]
    pub fn is_idle_on_mains(&self, min_idle_secs: u64) -> bool {
        self.on_mains && self.idle_secs >= min_idle_secs
    }
}
