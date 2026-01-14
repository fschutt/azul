#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico"
)]
#![allow(warnings)]

// #![no_std]

#[macro_use]
extern crate alloc;
extern crate core;

pub mod font_traits;
#[cfg(feature = "image_decoding")]
pub mod image;
#[cfg(feature = "text_layout")]
pub mod managers;
#[cfg(feature = "text_layout")]
pub mod solver3;

// Widget modules (behind feature flags)
#[cfg(feature = "widgets")]
pub mod str;
#[cfg(feature = "widgets")]
pub mod widgets;

// Extra APIs (dialogs, file operations)
#[cfg(feature = "extra")]
pub mod desktop;
#[cfg(feature = "extra")]
pub mod extra;

// ICU4X internationalization support
#[cfg(feature = "icu")]
pub mod icu;
#[cfg(feature = "icu")]
pub use icu::{
    DateTimeFieldSet, FormatLength, IcuDate, IcuDateTime, IcuError,
    IcuLocalizer, IcuLocalizerHandle, IcuResult, IcuStringVec, IcuTime,
    LayoutCallbackInfoIcuExt, ListType, PluralCategory,
};

#[cfg(feature = "text_layout")]
pub mod callbacks;
#[cfg(feature = "cpurender")]
pub mod cpurender;
#[cfg(feature = "text_layout")]
pub mod event_determination;
#[cfg(feature = "text_layout")]
pub mod font;
// Re-export allsorts types needed by printpdf
#[cfg(feature = "text_layout")]
pub use allsorts::subset::CmapTarget;
#[cfg(feature = "text_layout")]
pub use font::parsed::{
    FontParseWarning, FontParseWarningSeverity, FontType, OwnedGlyph, ParsedFont, PdfFontMetrics,
    SubsetFont,
};
// Re-export hyphenation for external crates (like printpdf)
#[cfg(feature = "text_layout_hyphenation")]
pub use hyphenation;
pub mod fragmentation;
#[cfg(feature = "text_layout")]
pub mod hit_test;
pub mod paged;
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
pub use fragmentation::{
    BoxBreakBehavior, BreakDecision, FragmentationDefaults, FragmentationLayoutContext,
    KeepTogetherPriority, PageCounter, PageFragment, PageMargins, PageNumberStyle, PageSlot,
    PageSlotContent, PageSlotPosition, PageTemplate,
};
#[cfg(feature = "text_layout")]
pub use hit_test::{CursorTypeHitTest, FullHitTest};
pub use paged::FragmentationState;
#[cfg(feature = "text_layout")]
pub use solver3::cache::LayoutCache as Solver3LayoutCache;
#[cfg(feature = "text_layout")]
pub use solver3::display_list::DisplayList as DisplayList3;
#[cfg(feature = "text_layout")]
pub use solver3::layout_document;
#[cfg(feature = "text_layout")]
pub use solver3::paged_layout::layout_document_paged;
#[cfg(feature = "text_layout")]
pub use solver3::{LayoutContext, LayoutError, Result as LayoutResult3};
#[cfg(feature = "text_layout")]
pub use text3::cache::{FontManager, LayoutCache as TextLayoutCache};
#[cfg(feature = "text_layout")]
pub use window::{LayoutWindow, ScrollbarDragState};

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
        &mut Vec::new(), // Ignore warnings for now
    )
    .map(|parsed_font| parsed_font_to_font_ref(parsed_font))
}

#[cfg(feature = "text_layout")]
pub fn parsed_font_to_font_ref(
    parsed_font: crate::font::parsed::ParsedFont,
) -> azul_css::props::basic::FontRef {
    use core::ffi::c_void;

    extern "C" fn parsed_font_destructor(ptr: *mut c_void) {
        unsafe {
            let _ = Box::from_raw(ptr as *mut crate::font::parsed::ParsedFont);
        }
    }

    let boxed = Box::new(parsed_font);
    let raw_ptr = Box::into_raw(boxed) as *const c_void;
    azul_css::props::basic::FontRef::new(raw_ptr, parsed_font_destructor)
}

#[cfg(feature = "text_layout")]
pub fn font_ref_to_parsed_font(
    font_ref: &azul_css::props::basic::FontRef,
) -> &crate::font::parsed::ParsedFont {
    unsafe { &*(font_ref.parsed as *const crate::font::parsed::ParsedFont) }
}
