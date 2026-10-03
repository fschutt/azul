//! Unified `Pdf` handle. See [`crate::unified`].

#[cfg(target_arch = "wasm32")]
use azul_core::dom::Dom;
#[cfg(target_arch = "wasm32")]
use azul_core::json::Json;
#[cfg(target_arch = "wasm32")]
use azul_css::U8Vec;

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::pdf::*;

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
    /// No PDF backend on wasm: returns an empty byte vec.
    pub fn from_dom_in_callback(
        &self,
        _callback_info: &azul_layout::callbacks::CallbackInfo,
        _dom: Dom,
        _page_width_px: f32,
        _page_height_px: f32,
    ) -> U8Vec {
        U8Vec::from_vec(Vec::new())
    }
    /// No PDF backend on wasm: returns an empty (0-page) handle.
    pub fn compute_pagination(
        &self,
        _styled_dom: azul_core::styled_dom::StyledDom,
        _page_width_px: f32,
        _page_height_px: f32,
        _font_cache: &azul_layout::resource_handles::FontCacheSnapshot,
        _image_cache: &azul_layout::resource_handles::ImageCacheSnapshot,
    ) -> azul_layout::resource_handles::PaginationSnapshot {
        azul_layout::resource_handles::PaginationSnapshot::empty()
    }
    /// No PDF backend on wasm: returns an empty (0-page) handle.
    pub fn compute_pagination_with_policy(
        &self,
        _styled_dom: azul_core::styled_dom::StyledDom,
        _page_width_px: f32,
        _page_height_px: f32,
        _font_cache: &azul_layout::resource_handles::FontCacheSnapshot,
        _image_cache: &azul_layout::resource_handles::ImageCacheSnapshot,
        _policy: azul_layout::solver3::page_breaks::BreakPolicy,
    ) -> azul_layout::resource_handles::PaginationSnapshot {
        azul_layout::resource_handles::PaginationSnapshot::empty()
    }
    /// No PDF backend on wasm: returns an empty byte vec.
    pub fn from_styled_dom_with_resources(
        &self,
        _styled_dom: azul_core::styled_dom::StyledDom,
        _page_width_px: f32,
        _page_height_px: f32,
        _font_cache: &azul_layout::resource_handles::FontCacheSnapshot,
        _image_cache: &azul_layout::resource_handles::ImageCacheSnapshot,
    ) -> U8Vec {
        U8Vec::from_vec(Vec::new())
    }
}

/// wasm stub of the desktop `PdfPageSize` (identical `#[repr(C)]` layout).
#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct PdfPageSize {
    pub width_pt: f32,
    pub height_pt: f32,
}

#[cfg(target_arch = "wasm32")]
impl PdfPageSize {
    /// The size in CSS px (96 per inch).
    pub fn to_logical_size(&self) -> azul_core::geom::LogicalSize {
        azul_core::geom::LogicalSize {
            width: self.width_pt * 96.0 / 72.0,
            height: self.height_pt * 96.0 / 72.0,
        }
    }
}

/// wasm stub of the desktop `ParsedPdf` (no printpdf backend on wasm): every
/// handle is empty and not valid. Identical `#[repr(C)]` layout.
#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Debug)]
pub struct ParsedPdf {
    pub ptr: *mut core::ffi::c_void,
    pub run_destructor: bool,
}

#[cfg(target_arch = "wasm32")]
impl Clone for ParsedPdf {
    fn clone(&self) -> Self {
        ParsedPdf::default()
    }
}

#[cfg(target_arch = "wasm32")]
impl Default for ParsedPdf {
    fn default() -> Self {
        ParsedPdf {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for ParsedPdf {
    fn drop(&mut self) {}
}

#[cfg(target_arch = "wasm32")]
impl ParsedPdf {
    /// No PDF backend on wasm: an empty handle.
    pub fn from_bytes(_bytes: &[u8]) -> Self {
        ParsedPdf::default()
    }
    pub fn is_valid(&self) -> bool {
        false
    }
    pub fn get_error(&self) -> azul_css::AzString {
        azul_css::AzString::from_const_str("no PDF support on wasm")
    }
    pub fn get_warnings(&self) -> azul_css::StringVec {
        Vec::<String>::new().into()
    }
    pub fn get_title(&self) -> azul_css::AzString {
        azul_css::AzString::from_const_str("")
    }
    pub fn page_count(&self) -> usize {
        0
    }
    pub fn page_size(&self, _index: usize) -> PdfPageSize {
        PdfPageSize::default()
    }
    pub fn page_to_svg(&self, _index: usize) -> azul_css::OptionString {
        azul_css::OptionString::None
    }
    pub fn page_text(&self, _index: usize) -> azul_css::StringVec {
        Vec::<String>::new().into()
    }
    pub fn outline_count(&self) -> usize {
        0
    }
    pub fn outline_title(&self, _index: usize) -> azul_css::AzString {
        azul_css::AzString::from_const_str("")
    }
    pub fn outline_page(&self, _index: usize) -> usize {
        0
    }
}
