//! Layout crate for the Azul GUI framework.
//!
//! Provides the layout solver (`solver3`), text shaping (`text3`), font
//! management (`font`), hit testing, page fragmentation, and widget support.
//! Integrates with `azul-core` for DOM types and `azul-css` for style
//! properties.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico"
)]
// Lint policy: deny correctness/safety issues, warn on style
#![deny(unused_must_use)]
#![warn(clippy::all)]
#![allow(
    clippy::non_canonical_partial_ord_impl,
    clippy::legacy_numeric_constants,
    clippy::should_implement_trait,
    clippy::result_unit_err,
    clippy::ptr_as_ptr,
    clippy::too_many_arguments,
    clippy::type_complexity,
    unused_imports,
    unused_variables,
    unused_mut,
    dead_code,
    unused_parens,
    unused_doc_comments,                   // doc comments before macro invocations
    unused_assignments,                    // layout solver incremental updates
    unused_labels,
    dropping_references,                   // intentional scope markers in layout solver
    private_interfaces,                    // internal solver types exposed for testing
    function_casts_as_integer,             // widget callback pointer identity
    improper_ctypes_definitions,           // node_graph extern fn returns ()
    mismatched_lifetime_syntaxes,
    unreachable_patterns,                  // exhaustive match in generated property code
    unexpected_cfgs,
    deprecated,                            // image crate tiff encoder
)]

#[macro_use]
extern crate alloc;
extern crate core;

/// Web-lift diagnostic marker: a volatile store of `val` to the absolute wasm
/// linear-memory address `addr` (the 0x40000–0xF0000 free band the e2e harness
/// peeks via `AzStartup_peekU32`). Compiles to NOTHING without the `web_lift`
/// feature — absolute-address stores would segfault native builds (macOS
/// `__PAGEZERO` covers the low 4 GiB). All in-tree diagnostic markers MUST go
/// through this helper rather than calling `core::ptr::write_volatile` on a
/// literal address directly.
#[inline(always)]
pub unsafe fn az_mark(_addr: u32, _val: u32) {
    #[cfg(feature = "web_lift")]
    core::ptr::write_volatile(_addr as usize as *mut u32, _val);
}

/// Read counterpart of [`az_mark`] (marker counters like `0x60758`).
/// Returns 0 without the `web_lift` feature.
#[inline(always)]
pub unsafe fn az_mark_read(_addr: u32) -> u32 {
    #[cfg(feature = "web_lift")]
    return core::ptr::read_volatile(_addr as usize as *const u32);
    #[cfg(not(feature = "web_lift"))]
    0
}

/// Font traits available regardless of text layout feature.
pub mod font_traits;
/// Optional probe instrumentation. With the `probe` feature off this
/// is a tiny module of no-op stubs and pays zero cost.
pub mod probe;
/// Image decoding and encoding (wraps the `image` crate).
#[cfg(feature = "image_decoding")]
pub mod image;
/// Scroll, hover, clipboard, cursor, and focus managers.
#[cfg(feature = "text_layout")]
pub mod managers;
/// CSS layout solver: block, inline, flex, grid, and table formatting.
#[cfg(feature = "text_layout")]
pub mod solver3;

/// C-compatible string formatting via `strfmt`.
#[cfg(feature = "strfmt")]
pub mod fmt;
#[cfg(feature = "strfmt")]
pub use fmt::{FmtArg, FmtArgVec, FmtArgVecDestructor, FmtValue, fmt_string};

/// Built-in widgets: button, text input, tabs, tree view, node graph, etc.
#[cfg(feature = "widgets")]
pub mod widgets;

/// Desktop platform helpers (file dialogs, notifications).
#[cfg(feature = "extra")]
pub mod desktop;

/// ICU internationalization: date/time formatting, plurals, list formatting.
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

/// Project Fluent localization: message bundles, argument formatting, ZIP I/O.
#[cfg(feature = "fluent")]
pub mod fluent;
#[cfg(feature = "fluent")]
pub use fluent::{
    check_fluent_syntax, check_fluent_syntax_bytes, create_fluent_zip,
    create_fluent_zip_from_strings, export_to_zip, FluentError,
    FluentLanguageInfo, FluentLanguageInfoVec,
    FluentLocalizerHandle, FluentSyntaxCheckResult,
    FluentZipLoadResult,
};

/// URL parsing (RFC 3986 compliant). Pure-Rust, always present (no TLS deps).
pub mod url;
pub use url::{Url, UrlParseError, ResultUrlUrlParseError};

/// File system operations (C-compatible wrappers for `std::fs`).
pub mod file;
pub use file::{
    dir_create, dir_create_all, dir_list, dir_delete, dir_delete_all,
    file_append, file_copy, path_exists, file_metadata, file_read, file_read_string,
    file_delete, file_rename, file_write, file_write_string,
    path_canonicalize, path_extension, path_file_name, path_is_dir, path_is_file,
    path_join, path_parent, temp_dir,
    DirEntry, DirEntryVec, DirEntryVecDestructor, DirEntryVecDestructorType,
    FileError, FileErrorKind, FileMetadata, FilePath, OptionFilePath,
};

/// HTTP client: GET/POST requests with pure-Rust TLS.
/// API surface always present (stub when off); ureq/rustls only pulled in with `http`.
pub mod http;
pub use http::{
    download_bytes, download_bytes_with_config, http_get,
    http_get_with_config, is_url_reachable, HttpError, HttpHeader,
    HttpRequestConfig, HttpResponse, HttpResponseTooLargeError, HttpResult,
    HttpStatusError,
};

/// JSON parsing and serialization for the C API.
#[cfg(feature = "json")]
pub mod json;
#[cfg(feature = "json")]
pub use json::{
    json_parse, json_stringify,
    Json, JsonInternal, JsonKeyValue, JsonKeyValueVec, JsonKeyValueVecDestructor, JsonKeyValueVecDestructorType,
    JsonParseError, JsonType, JsonVec,
    ResultJsonJsonParseError, OptionJson, OptionJsonVec, OptionJsonKeyValueVec,
};

/// ZIP file creation, extraction, and listing.
#[cfg(feature = "zip_support")]
pub mod zip;
#[cfg(feature = "zip_support")]
pub use zip::{
    zip_create, zip_create_from_files, zip_extract_all, zip_list_contents,
    ZipFile, ZipFileEntry, ZipFileEntryVec, ZipPathEntry, ZipPathEntryVec,
    ZipReadConfig, ZipWriteConfig, ZipReadError, ZipWriteError,
};

/// Icon provider: resolves icons from Material Icons font, images, or ZIP packs.
pub mod icon;
pub use icon::{
    // Resolver
    default_icon_resolver,
    // Data types for RefAny
    ImageIconData, FontIconData,
    // Helpers
    register_image_icon,
    register_font_icon,
    register_icons_from_zip,
    create_default_icon_provider,
    register_material_icons,
    register_embedded_material_icons,
};
// Re-export core icon types
pub use azul_core::icon::{
    IconProviderHandle, IconResolverCallbackType,
    resolve_icons_in_styled_dom, OptionIconProviderHandle,
};

/// Callback handling for layout events (invocation, result processing).
#[cfg(feature = "text_layout")]
pub mod callbacks;
/// CPU-based software rendering (no GPU required).
#[cfg(feature = "cpurender")]
pub mod cpurender;
/// Glyph path and cell cache for CPU text rendering.
#[cfg(feature = "cpurender")]
pub mod glyph_cache;
/// Default keyboard actions (copy, paste, select-all, undo, etc.).
#[cfg(feature = "text_layout")]
pub mod default_actions;
/// Event determination: maps raw input to DOM node callbacks.
#[cfg(feature = "text_layout")]
pub mod event_determination;
/// Font parsing, metrics extraction, and subsetting.
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
/// Hit-testing: maps screen coordinates to DOM nodes.
#[cfg(feature = "text_layout")]
pub mod hit_test;
/// Paged media: the `FragmentationContext` (continuous vs. paged) and page margins.
/// The primitive types live in `azul_core::paged`; re-exported here so existing
/// `azul_layout::paged::*` / `crate::paged::*` paths keep resolving.
pub use azul_core::paged;
/// Text shaping, line breaking (Knuth-Plass), and inline formatting.
#[cfg(feature = "text_layout")]
pub mod text3;
/// Thread callback wrappers for the C API.
#[cfg(feature = "text_layout")]
pub mod thread;
/// Timer callback wrappers for the C API.
#[cfg(feature = "text_layout")]
pub mod timer;
/// Scroll physics timer for momentum-based smooth scrolling.
#[cfg(feature = "text_layout")]
pub mod scroll_timer;
/// Window layout management: relayout, event processing, state sync.
#[cfg(feature = "text_layout")]
pub mod window;
/// Window state types (keyboard, mouse, DPI, focus).
#[cfg(feature = "text_layout")]
pub mod window_state;
/// XML and XHTML parsing for declarative UI definitions.
#[cfg(feature = "xml")]
pub mod xml;

// Export the main layout function and window management
/// Canonical paged-media page margins (defined in [`paged`]).
pub use paged::PageMargins;
#[cfg(feature = "text_layout")]
pub use hit_test::{CursorTypeHitTest, FullHitTest};
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
pub use text3::cache::{FontContext, FontManager, TextShapingCache};
/// Backwards-compat alias for the old `TextLayoutCache` name.
/// Will be dropped at the next API revision; new code should use
/// [`TextShapingCache`] directly.
#[cfg(feature = "text_layout")]
pub use text3::cache::TextShapingCache as TextLayoutCache;
#[cfg(feature = "font_async_registry")]
pub use rust_fontconfig::registry::FcFontRegistry;
#[cfg(feature = "text_layout")]
pub use window::{CursorBlinkTimerAction, LayoutWindow, ScrollbarDragState, TooltipTimerAction};
#[cfg(feature = "text_layout")]
pub use managers::text_input::{PendingTextEdit, OptionPendingTextEdit};

#[cfg(feature = "text_layout")]
/// Parses raw font bytes into a [`FontRef`](azul_css::props::basic::FontRef)
/// suitable for use in the layout system.
pub fn parse_font_fn(
    source: azul_core::resources::LoadedFontSource,
) -> Option<azul_css::props::basic::FontRef> {
    use crate::font::parsed::ParsedFont;

    ParsedFont::from_bytes(
        source.data.as_ref(),
        source.index as usize,
        &mut Vec::new(), // Ignore warnings for now
    )
    .map(parsed_font_to_font_ref)
}

#[cfg(feature = "text_layout")]
/// Wraps a [`ParsedFont`] in a [`FontRef`](azul_css::props::basic::FontRef),
/// transferring ownership to the returned handle.
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
/// Recovers a reference to the [`ParsedFont`] stored inside a [`FontRef`](azul_css::props::basic::FontRef).
///
/// # Safety contract
/// The `font_ref` must have been created by [`parsed_font_to_font_ref`],
/// so that `font_ref.parsed` points to a valid `ParsedFont`.
pub fn font_ref_to_parsed_font(
    font_ref: &azul_css::props::basic::FontRef,
) -> &crate::font::parsed::ParsedFont {
    // SAFETY: `font_ref.parsed` was created by `parsed_font_to_font_ref`
    // via `Box::into_raw`, so it points to a valid, aligned `ParsedFont`.
    unsafe { &*(font_ref.parsed as *const crate::font::parsed::ParsedFont) }
}
