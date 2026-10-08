//! Unified `Keyring` namespace (the keyring's blocking calls for worker
//! threads). See [`crate::unified`].
//!
//! Off-wasm this is the real one of `desktop::extra::keyring`. On wasm there
//! is no keyring backend: the stub below has the identical `#[repr(C)]` layout
//! and answers every call `Unavailable`.

#[cfg(target_arch = "wasm32")]
use azul_core::keyring::KeyringResult;
#[cfg(target_arch = "wasm32")]
use azul_css::AzString;

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::keyring::Keyring;

/// wasm stub of the desktop `Keyring` namespace; `#[repr(C)]` layout MUST match.
#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub struct Keyring {
    pub _reserved: u8,
}

#[cfg(target_arch = "wasm32")]
impl Keyring {
    pub const fn new() -> Self {
        Self { _reserved: 0 }
    }
    /// No keyring on wasm.
    pub fn get_blocking(_key: AzString) -> KeyringResult {
        KeyringResult::Unavailable
    }
    /// No keyring on wasm.
    pub fn store_blocking(_key: AzString, _secret: AzString, _require_biometry: bool) -> KeyringResult {
        KeyringResult::Unavailable
    }
    /// No keyring on wasm.
    pub fn delete_blocking(_key: AzString) -> KeyringResult {
        KeyringResult::Unavailable
    }
}
