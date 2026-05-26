//! Unified `App` handle. See [`crate::unified`].

// Off-wasm: re-export the real desktop type (zero behaviour change). Gated on
// the same condition as `crate::desktop`.
#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::app::*;

// wasm: stub with an identical `#[repr(C)]` layout (a pointer-width `ptr` +
// `run_destructor` bool, matching `Box<AppInternal>` + bool) so the C-ABI
// transmute to `AzApp` stays valid. There is no platform event loop on wasm —
// the web backend drives the app — so the lifecycle methods are inert. Defined
// directly here so the path resolves to `azul_dll::unified::app::App`.
#[cfg(target_arch = "wasm32")]
use core::ffi::c_void;

#[cfg(target_arch = "wasm32")]
use azul_core::{refany::RefAny, resources::AppConfig, window::MonitorVec};
#[cfg(target_arch = "wasm32")]
use azul_layout::window_state::WindowCreateOptions;

/// wasm stub of the desktop `App` handle (no native event loop on wasm; the
/// web backend owns the application lifecycle).
#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
#[repr(C)]
pub struct App {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

#[cfg(target_arch = "wasm32")]
impl Clone for App {
    fn clone(&self) -> Self {
        App {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for App {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

#[cfg(target_arch = "wasm32")]
impl Default for App {
    fn default() -> Self {
        Self::create(RefAny::new(()), AppConfig::default())
    }
}

#[cfg(target_arch = "wasm32")]
impl App {
    /// No native event loop on wasm: returns an inert handle. The supplied
    /// data/config are dropped here (the web backend constructs its own).
    pub fn create(_initial_data: RefAny, _app_config: AppConfig) -> Self {
        App {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
    pub fn add_window(&mut self, _create_options: WindowCreateOptions) {}
    pub fn get_monitors(&self) -> MonitorVec {
        MonitorVec::from_const_slice(&[])
    }
    pub fn run(&self, _root_window: WindowCreateOptions) {}
}
