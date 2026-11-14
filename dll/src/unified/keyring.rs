//! Unified `Keyring` namespace (the keyring's blocking calls for worker
//! threads). See [`crate::unified`].
//!
//! Off-wasm this is the real one of `desktop::extra::keyring`. On wasm there
//! is no keyring backend: the stub below has the identical `#[repr(C)]` layout
//! and answers every call `Unavailable`.

#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
use azul_core::keyring::{KeyringNamespace, KeyringResult};
#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
use azul_css::AzString;

#[cfg(az_desktop)]
pub use crate::desktop::extra::keyring::Keyring;

/// wasm stub of the desktop `Keyring` namespace; `#[repr(C)]` layout MUST match.
#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub struct Keyring {
    pub _reserved: u8,
}

#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
impl Keyring {
    pub const fn new() -> Self {
        Self { _reserved: 0 }
    }
    /// No keyring on wasm.
    pub fn get_blocking(_key: AzString) -> KeyringResult {
        KeyringResult::Unavailable
    }
    /// No keyring on wasm.
    pub fn store_blocking(
        _key: AzString,
        _secret: AzString,
        _require_biometry: bool,
    ) -> KeyringResult {
        KeyringResult::Unavailable
    }
    /// No keyring on wasm.
    pub fn delete_blocking(_key: AzString) -> KeyringResult {
        KeyringResult::Unavailable
    }
    /// No keyring on wasm.
    pub fn get_blocking_in(_namespace: KeyringNamespace, _key: AzString) -> KeyringResult {
        KeyringResult::Unavailable
    }
    /// No keyring on wasm.
    pub fn store_blocking_in(
        _namespace: KeyringNamespace,
        _key: AzString,
        _secret: AzString,
        _require_biometry: bool,
    ) -> KeyringResult {
        KeyringResult::Unavailable
    }
    /// No keyring on wasm.
    pub fn delete_blocking_in(_namespace: KeyringNamespace, _key: AzString) -> KeyringResult {
        KeyringResult::Unavailable
    }
    /// No keyring on wasm.
    pub fn get_quietly_in(_namespace: KeyringNamespace, _key: AzString) -> KeyringResult {
        KeyringResult::Unavailable
    }
    /// No keyring on wasm.
    pub fn store_quietly_in(
        _namespace: KeyringNamespace,
        _key: AzString,
        _secret: AzString,
        _require_biometry: bool,
    ) -> KeyringResult {
        KeyringResult::Unavailable
    }
    /// No keyring on wasm.
    pub fn delete_quietly_in(_namespace: KeyringNamespace, _key: AzString) -> KeyringResult {
        KeyringResult::Unavailable
    }
}
