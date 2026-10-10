//! Unified `NetworkState` type + query. See [`crate::unified`] and
//! `crate::desktop::extra::network` (the per-platform readings).

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::network::*;

/// wasm stub of the desktop `NetworkKind` - IDENTICAL `#[repr(C)]` layout.
#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetworkKind {
    Unknown,
    Wired,
    WiFi,
    Cellular,
    Other,
}

/// wasm stub of the desktop `NetworkState` - IDENTICAL `#[repr(C)]` layout. A browser's Network
/// Information API (`navigator.connection`: `saveData`, `type`) is missing from several browsers
/// and is not reached from here, so every query answers [`NetworkState::UNKNOWN`]: connected and
/// free.
#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NetworkState {
    pub kind: NetworkKind,
    pub connected: bool,
    pub metered: bool,
    pub constrained: bool,
}

#[cfg(target_arch = "wasm32")]
impl NetworkState {
    pub const HEADLESS: NetworkState = NetworkState {
        kind: NetworkKind::Wired,
        connected: true,
        metered: false,
        constrained: false,
    };
    pub const UNKNOWN: NetworkState = NetworkState {
        kind: NetworkKind::Unknown,
        connected: true,
        metered: false,
        constrained: false,
    };
    pub const OFFLINE: NetworkState = NetworkState {
        kind: NetworkKind::Unknown,
        connected: false,
        metered: false,
        constrained: false,
    };

    #[must_use]
    pub fn query() -> NetworkState {
        NetworkState::UNKNOWN
    }

    #[must_use]
    pub fn headless() -> NetworkState {
        NetworkState::HEADLESS
    }

    #[must_use]
    pub fn allows_background_transfer(&self) -> bool {
        self.connected && !self.metered && !self.constrained
    }
}
