//! Unified `Capability` type + probes. See [`crate::unified`].

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::capability::*;

#[cfg(target_arch = "wasm32")]
use azul_css::AzString;

/// wasm stub of the desktop `Capability` — IDENTICAL `#[repr(C)]` layout. On
/// wasm there are no native device backends, so every probe reports unavailable.
#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct Capability {
    pub available: bool,
    pub backend: AzString,
    pub reason: AzString,
}

#[cfg(target_arch = "wasm32")]
impl Capability {
    fn unavailable() -> Capability {
        Capability {
            available: false,
            backend: AzString::from_const_str("none"),
            reason: AzString::from_const_str("no native backend on wasm"),
        }
    }
    pub fn udp() -> Capability {
        Capability {
            available: false,
            backend: AzString::from_const_str("none"),
            reason: AzString::from_const_str("UDP has no wasm backend"),
        }
    }
    pub fn camera() -> Capability { Self::unavailable() }
    pub fn microphone() -> Capability { Self::unavailable() }
    pub fn audio_output() -> Capability { Self::unavailable() }
    pub fn sensors() -> Capability { Self::unavailable() }
    pub fn gamepad() -> Capability { Self::unavailable() }
    pub fn geolocation() -> Capability { Self::unavailable() }
    pub fn keyring() -> Capability { Self::unavailable() }
    pub fn biometric() -> Capability { Self::unavailable() }
    pub fn video_codec() -> Capability { Self::unavailable() }
}
