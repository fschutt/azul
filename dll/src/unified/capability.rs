//! Unified `PlatformCapability` type + probes. See [`crate::unified`].

#[cfg(target_arch = "wasm32")]
use azul_css::AzString;

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::capability::*;

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
    pub fn webtransport() -> PlatformCapability {
        PlatformCapability {
            available: false,
            backend: AzString::from_const_str("none"),
            reason: AzString::from_const_str("WebTransport has no in-process wasm engine"),
        }
    }
    pub fn thread() -> PlatformCapability {
        PlatformCapability {
            available: false,
            backend: AzString::from_const_str("none"),
            reason: AzString::from_const_str("no worker mode: Thread::create is dead-on-arrival"),
        }
    }
    pub fn file_system() -> PlatformCapability {
        Self::unavailable()
    }
    pub fn dialogs() -> PlatformCapability {
        Self::unavailable()
    }
    pub fn http() -> PlatformCapability {
        Self::unavailable()
    }
    pub fn multi_window() -> PlatformCapability {
        PlatformCapability {
            available: false,
            backend: AzString::from_const_str("none"),
            reason: AzString::from_const_str("one window per page"),
        }
    }
    pub fn sql() -> PlatformCapability {
        PlatformCapability {
            available: false,
            backend: AzString::from_const_str("none"),
            reason: AzString::from_const_str("raw SQL is not part of the API"),
        }
    }
    pub fn sync() -> PlatformCapability {
        Self::unavailable()
    }
    pub fn camera() -> PlatformCapability {
        Self::unavailable()
    }
    pub fn screen_capture() -> PlatformCapability {
        Self::unavailable()
    }
    pub fn microphone() -> PlatformCapability {
        Self::unavailable()
    }
    pub fn audio_output() -> PlatformCapability {
        Self::unavailable()
    }
    pub fn sensors() -> PlatformCapability {
        Self::unavailable()
    }
    pub fn gamepad() -> PlatformCapability {
        Self::unavailable()
    }
    pub fn geolocation() -> PlatformCapability {
        Self::unavailable()
    }
    pub fn keyring() -> PlatformCapability {
        Self::unavailable()
    }
    pub fn biometric() -> PlatformCapability {
        Self::unavailable()
    }
    pub fn video_codec() -> PlatformCapability {
        Self::unavailable()
    }
}
