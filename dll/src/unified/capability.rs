//! Unified `PlatformCapability` type + probes. See [`crate::unified`].

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::capability::*;

#[cfg(target_arch = "wasm32")]
use azul_css::AzString;

/// wasm stub of the desktop `PlatformCapability` — IDENTICAL `#[repr(C)]` layout. On
/// wasm there are no native device backends, so every probe reports unavailable.
#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct PlatformCapability {
    pub available: bool,
    pub backend: AzString,
    pub reason: AzString,
}

#[cfg(target_arch = "wasm32")]
impl PlatformCapability {
    fn unavailable() -> PlatformCapability {
        PlatformCapability {
            available: false,
            backend: AzString::from_const_str("none"),
            reason: AzString::from_const_str("no native backend on wasm"),
        }
    }
    pub fn udp() -> PlatformCapability {
        PlatformCapability {
            available: false,
            backend: AzString::from_const_str("none"),
            reason: AzString::from_const_str("UDP has no wasm backend"),
        }
    }
    pub fn camera() -> PlatformCapability { Self::unavailable() }
    pub fn screen_capture() -> PlatformCapability { Self::unavailable() }
    pub fn microphone() -> PlatformCapability { Self::unavailable() }
    pub fn audio_output() -> PlatformCapability { Self::unavailable() }
    pub fn sensors() -> PlatformCapability { Self::unavailable() }
    pub fn gamepad() -> PlatformCapability { Self::unavailable() }
    pub fn geolocation() -> PlatformCapability { Self::unavailable() }
    pub fn keyring() -> PlatformCapability { Self::unavailable() }
    pub fn biometric() -> PlatformCapability { Self::unavailable() }
    pub fn video_codec() -> PlatformCapability { Self::unavailable() }
}
