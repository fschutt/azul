#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico"
)]
#![allow(warnings)]

// #![no_std]

#[macro_use]
extern crate alloc;
extern crate core;

#[cfg(feature = "cpurender")]
pub mod cpurender;
#[cfg(feature = "font_loading")]
pub mod font;
pub mod image;
pub mod parsedfont;
pub mod solver2;
pub mod solver3;
#[cfg(feature = "text_layout")]
pub mod text2;
#[cfg(feature = "text_layout")]
pub mod text3;
#[cfg(feature = "xml")]
pub mod xml;

// Export the main layout function
#[cfg(feature = "text_layout")]
pub use solver3::layout_document;
#[cfg(feature = "text_layout")]
pub use solver3::cache::LayoutCache as Solver3LayoutCache;
#[cfg(feature = "text_layout")]
pub use solver3::{LayoutContext, LayoutError, Result as LayoutResult3};
#[cfg(feature = "text_layout")]
pub use solver3::display_list::DisplayList as DisplayList3;
#[cfg(feature = "text_layout")]
pub use text3::cache::{LayoutCache as TextLayoutCache, FontManager};

// #[cfg(feature = "text_layout")]
// pub use solver::{callback_info_shape_text, do_the_layout, do_the_relayout};
#[cfg(feature = "text_layout")]
pub fn parse_font_fn(
    source: azul_core::app_resources::LoadedFontSource,
) -> Option<azul_css::props::basic::FontRef> {
    use core::ffi::c_void;

    use crate::parsedfont::ParsedFont;

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

#[cfg(feature = "text_layout")]
pub fn callback_info_shape_text(
    callbackinfo: &azul_core::callbacks::CallbackInfo,
    node_id: azul_core::callbacks::DomNodeId,
    text: azul_css::AzString,
) -> Option<azul_core::callbacks::InlineText> {
    let font_ref = callbackinfo.get_font_ref(node_id)?;
    let text_layout_options = callbackinfo.get_text_layout_options(node_id)?;
    Some(crate::text2::layout::shape_text(
        &font_ref,
        text.as_str(),
        &text_layout_options,
    ))
}
