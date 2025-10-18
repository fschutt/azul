#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico"
)]
#![allow(warnings)]

// #![no_std]

#[macro_use]
extern crate alloc;
extern crate core;

#[cfg(feature = "text_layout")]
pub mod callbacks;
#[cfg(feature = "cpurender")]
pub mod cpurender;
#[cfg(feature = "text_layout")]
pub mod focus;
#[cfg(feature = "font_loading")]
pub mod font;
#[cfg(feature = "text_layout")]
pub mod gpu;
#[cfg(feature = "text_layout")]
pub mod hit_test;
#[cfg(feature = "text_layout")]
pub mod iframe;
pub mod image;
#[cfg(feature = "pdf")]
pub mod paged;
#[cfg(feature = "pdf")]
pub mod pdf;
#[cfg(feature = "text_layout")]
pub mod scroll;
pub mod solver3;
#[cfg(feature = "text_layout")]
pub mod text3;
#[cfg(feature = "text_layout")]
pub mod thread;
#[cfg(feature = "text_layout")]
pub mod timer;
#[cfg(feature = "text_layout")]
pub mod window;
#[cfg(feature = "text_layout")]
pub mod window_state;
#[cfg(feature = "xml")]
pub mod xml;

// Export the main layout function and window management
#[cfg(feature = "text_layout")]
pub use hit_test::{CursorTypeHitTest, FullHitTest};
#[cfg(feature = "pdf")]
pub use paged::{generate_display_lists_from_paged_layout, layout_to_pages, Page};
#[cfg(feature = "pdf")]
pub use pdf::{
    display_list_to_pdf_ops, FontId, PdfColor, PdfOp, PdfPageRender, PdfPoint, PdfRenderResources,
    XObjectId,
};
#[cfg(feature = "text_layout")]
pub use solver3::cache::LayoutCache as Solver3LayoutCache;
#[cfg(feature = "text_layout")]
pub use solver3::display_list::DisplayList as DisplayList3;
#[cfg(feature = "text_layout")]
pub use solver3::layout_document;
#[cfg(feature = "text_layout")]
pub use solver3::{LayoutContext, LayoutError, Result as LayoutResult3};
#[cfg(feature = "text_layout")]
pub use text3::cache::{FontManager, LayoutCache as TextLayoutCache};
#[cfg(feature = "text_layout")]
pub use window::LayoutWindow;

// #[cfg(feature = "text_layout")]
// pub use solver::{callback_info_shape_text, do_the_layout, do_the_relayout};
#[cfg(feature = "text_layout")]
pub fn parse_font_fn(
    source: azul_core::resources::LoadedFontSource,
) -> Option<azul_css::props::basic::FontRef> {
    use core::ffi::c_void;

    use crate::font::parsed::ParsedFont;

    fn parsed_font_destructor(ptr: *mut c_void) {
        unsafe {
            let _ = Box::from_raw(ptr as *mut ParsedFont);
        }
    }

    ParsedFont::from_bytes(
        source.data.as_ref(),
        source.index as usize,
        source.load_outlines,
    )
    .map(|parsed_font| {
        azul_css::props::basic::FontRef::new(azul_css::props::basic::FontData {
            bytes: source.data,
            font_index: source.index,
            parsed: Box::into_raw(Box::new(parsed_font)) as *const c_void,
            parsed_destructor: parsed_font_destructor,
        })
    })
}
