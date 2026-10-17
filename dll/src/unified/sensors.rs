//! Unified device-state readings: `PowerState`, `BatteryState`, `ThermalState`,
//! `NetworkState` and `NetworkKind`. See
//! [`crate::unified`] and `crate::desktop::extra::sensors` (the per-platform readings, beside
//! the motion sensors). api.json's `sensor` module names these paths
//! (`azul_dll::unified::sensors::<Type>`).
//!
//! What they read, and that nothing of it leaves the device: the privacy section of
//! `crate::desktop::extra::sensors`. No identifier (a network's name, an address, a carrier, a
//! serial number) is read at all.

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::sensors::{
    battery::{BatteryState, ThermalState, BATTERY_STATE_FILE_VAR},
    network::{NetworkKind, NetworkState, NETWORK_STATE_FILE_VAR},
    power::PowerState,
};

/// wasm stub of the desktop `ThermalState` - IDENTICAL `#[repr(C)]` layout.
#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThermalState {
    Nominal,
    Fair,
    Serious,
    Critical,
    Unknown,
}

/// wasm stub of the desktop `BatteryState` - IDENTICAL `#[repr(C)]` layout. A browser's Battery
/// Status API answers a promise (and is gone from Firefox and Safari), and a page has no
/// thermal or power-saver reading, so every query answers [`BatteryState::UNKNOWN`].
#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BatteryState {
    pub present: bool,
    pub charging: bool,
    pub level_percent: u8,
    pub low_power_mode: bool,
    pub thermal: ThermalState,
}

#[cfg(target_arch = "wasm32")]
impl BatteryState {
    pub const LEVEL_UNKNOWN: u8 = 255;
    pub const HEADLESS: BatteryState = BatteryState {
        present: false,
        charging: false,
        level_percent: BatteryState::LEVEL_UNKNOWN,
        low_power_mode: false,
        thermal: ThermalState::Nominal,
    };
    pub const UNKNOWN: BatteryState = BatteryState {
        present: false,
        charging: false,
        level_percent: BatteryState::LEVEL_UNKNOWN,
        low_power_mode: false,
        thermal: ThermalState::Unknown,
    };

    #[must_use]
    pub fn query() -> BatteryState {
        BatteryState::UNKNOWN
    }

    #[must_use]
    pub fn headless() -> BatteryState {
        BatteryState::HEADLESS
    }

    #[must_use]
    pub fn runs_low(&self, below_percent: u8) -> bool {
        self.present
            && !self.charging
            && self.level_percent != BatteryState::LEVEL_UNKNOWN
            && self.level_percent < below_percent
    }
}

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
    pub hotspot: bool,
}

#[cfg(target_arch = "wasm32")]
impl NetworkState {
    pub const HEADLESS: NetworkState = NetworkState {
        kind: NetworkKind::Wired,
        connected: true,
        metered: false,
        constrained: false,
        hotspot: false,
    };
    pub const UNKNOWN: NetworkState = NetworkState {
        kind: NetworkKind::Unknown,
        connected: true,
        metered: false,
        constrained: false,
        hotspot: false,
    };
    pub const OFFLINE: NetworkState = NetworkState {
        kind: NetworkKind::Unknown,
        connected: false,
        metered: false,
        constrained: false,
        hotspot: false,
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
