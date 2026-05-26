//! Unified `Pdf` handle. See [`crate::unified`].

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::pdf::*;

#[cfg(target_arch = "wasm32")]
use azul_core::dom::Dom;
#[cfg(target_arch = "wasm32")]
use azul_core::json::Json;
#[cfg(target_arch = "wasm32")]
use azul_css::U8Vec;

/// wasm stub of the desktop `Pdf` handle (stateless; no printpdf backend).
/// Identical `#[repr(C)]` layout to the real type (a single reserved byte).
#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pdf {
    pub _reserved: u8,
}

#[cfg(target_arch = "wasm32")]
impl Default for Pdf {
    fn default() -> Self {
        Pdf::new()
    }
}

#[cfg(target_arch = "wasm32")]
impl Pdf {
    pub fn new() -> Self {
        Pdf { _reserved: 0 }
    }
    /// No PDF backend on wasm: returns an empty byte vec.
    pub fn write_json(&self, _json: Json) -> U8Vec {
        U8Vec::from_vec(Vec::new())
    }
    /// No PDF backend on wasm: returns a JSON null.
    pub fn read_json(&self, _bytes: U8Vec) -> Json {
        Json::null()
    }
    /// No PDF backend on wasm: returns an empty byte vec.
    pub fn from_dom(&self, _dom: Dom, _page_width_px: f32, _page_height_px: f32) -> U8Vec {
        U8Vec::from_vec(Vec::new())
    }
}
