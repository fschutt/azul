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

// String formatting utilities
#[cfg(feature = "strfmt")]
pub mod fmt;
#[cfg(feature = "strfmt")]
pub use fmt::{FmtArg, FmtArgVec, FmtArgVecDestructor, FmtValue, fmt_string};

// Widget modules (behind feature flags)
#[cfg(feature = "widgets")]
pub mod widgets;

// Extra APIs (dialogs, file operations)
#[cfg(feature = "extra")]
pub mod desktop;
#[cfg(feature = "extra")]
pub mod extra;

// ICU internationalization support.
// Active with the ICU4X backend (`icu` feature), the macOS Foundation backend
// (`icu_macos` on macOS), or the Windows NLS backend (`icu_windows` on Windows).
#[cfg(any(
    feature = "icu",
    all(target_os = "macos", feature = "icu_macos"),
    all(target_os = "windows", feature = "icu_windows"),
))]
pub mod icu;
#[cfg(any(
    feature = "icu",
    all(target_os = "macos", feature = "icu_macos"),
    all(target_os = "windows", feature = "icu_windows"),
))]
pub use icu::{
    DateTimeFieldSet, FormatLength, IcuDate, IcuDateTime, IcuError,
    IcuLocalizer, IcuLocalizerHandle, IcuResult, IcuStringVec, IcuTime,
    LayoutCallbackInfoIcuExt, ListType, PluralCategory,
};

// Project Fluent localization support
#[cfg(feature = "fluent")]
pub mod fluent;
#[cfg(feature = "fluent")]
pub use fluent::{
    check_fluent_syntax, check_fluent_syntax_bytes, create_fluent_zip,
    create_fluent_zip_from_strings, export_to_zip, FluentError,
    FluentLanguageInfo, FluentLanguageInfoVec,
    FluentLocalizerHandle, FluentResult, FluentSyntaxCheckResult,
    FluentSyntaxError, FluentZipLoadResult, LayoutCallbackInfoFluentExt,
};

// URL parsing support
#[cfg(feature = "http")]
pub mod url;
#[cfg(feature = "http")]
pub use url::{Url, UrlParseError, ResultUrlUrlParseError};

// File system operations (C-compatible wrapper for std::fs)
pub mod file;
pub use file::{
    dir_create, dir_create_all, dir_list, dir_delete, dir_delete_all,
    file_copy, path_exists, file_metadata, file_read, file_delete, file_rename, file_write,
    path_is_dir, path_is_file, path_join, temp_dir,
    DirEntry, DirEntryVec, DirEntryVecDestructor, DirEntryVecDestructorType,
    FileError, FileErrorKind, FileMetadata, FilePath, OptionFilePath,
};

// HTTP client support
#[cfg(feature = "http")]
pub mod http;
#[cfg(feature = "http")]
pub use http::{
    download_bytes, download_bytes_with_config, http_get,
    http_get_with_config, is_url_reachable, HttpError, HttpHeader,
    HttpRequestConfig, HttpResponse, HttpResponseTooLargeError, HttpResult,
    HttpStatusError,
};

// JSON parsing support
#[cfg(feature = "json")]
pub mod json;
#[cfg(feature = "json")]
pub use json::{
    json_parse, json_parse_bytes, json_stringify, json_stringify_pretty,
    Json, JsonInternal, JsonKeyValue, JsonKeyValueVec, JsonKeyValueVecDestructor, JsonKeyValueVecDestructorType,
    JsonParseError, JsonType, JsonVec,
    ResultJsonJsonParseError, OptionJson, OptionJsonVec, OptionJsonKeyValueVec,
};

// ZIP file manipulation support
#[cfg(feature = "zip_support")]
pub mod zip;
#[cfg(feature = "zip_support")]
pub use zip::{
    zip_create, zip_create_from_files, zip_extract_all, zip_list_contents,
    ZipFile, ZipFileEntry, ZipFileEntryVec, ZipPathEntry, ZipPathEntryVec,
    ZipReadConfig, ZipWriteConfig, ZipReadError, ZipWriteError,
};

// Icon provider support (always available)
pub mod icon;
pub use icon::{
    // Resolver
    default_icon_resolver,
    // Data types for RefAny
    ImageIconData, FontIconData,
    // Helpers
    create_default_icon_provider,
    register_material_icons,
    register_embedded_material_icons,
    get_material_icons_font_bytes,
};
// Re-export core icon types
pub use azul_core::icon::{
    IconProviderHandle, IconResolverCallbackType,
    resolve_icons_in_styled_dom, OptionIconProviderHandle,
};

#[cfg(feature = "text_layout")]
pub mod callbacks;
#[cfg(feature = "cpurender")]
pub mod cpurender;
#[cfg(feature = "text_layout")]
pub mod default_actions;
#[cfg(feature = "text_layout")]
pub mod event_determination;
#[cfg(feature = "text_layout")]
pub mod font;

/// Headless backend for CPU-only rendering without a display server.
/// Used with `AZUL_HEADLESS=1` for E2E testing, CI, and screenshot capture.
#[cfg(feature = "text_layout")]
pub mod headless;
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
pub mod scroll_timer;
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
pub use window::{CursorBlinkTimerAction, LayoutWindow, ScrollbarDragState};
#[cfg(feature = "text_layout")]
pub use managers::text_input::{PendingTextEdit, OptionPendingTextEdit};

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
