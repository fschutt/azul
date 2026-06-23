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
// Lint policy: deny correctness/safety issues, warn on style (`clippy::all`).
//
// Crate-wide allows are intentionally limited to lints that are either
//   (a) pervasive AND feature-sensitive — an import/binding/field that is unused
//       under one feature set is live under another, so a per-site fix would
//       break a different feature build — or
//   (b) churny / newer-toolchain lints with little value in scoping.
// Lints that fire in only a few, well-localized places are scoped with
// `#[allow(...)]` on the specific `pub mod` declarations further down, so the
// rest of the (hand-written) crate is actually checked.
#![deny(unused_must_use)]
#![warn(clippy::all)]
// === "extreme lints" lockdown (2026-06-20) — maximal opt-in lint set ===
// All clippy groups + opt-in rustc lints, warn-level so normal builds still
// pass; the CI clippy job runs `-D warnings`, turning every one of these into
// the outstanding-lint-failure report for Monday triage. NOT yet fixed.
#![warn(
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    // missing_docs,  // TODO(docs): re-enable as a dedicated final docs pass; disabled
    //                // for now so the cleanup focuses on code-quality lints, not doc debt.
    missing_debug_implementations,
    missing_copy_implementations,
    unreachable_pub,
    unused_qualifications,
    unused_lifetimes,
    unused_import_braces,
    unused_macro_rules,
    unused_crate_dependencies,
    meta_variable_misuse,
    trivial_casts,
    trivial_numeric_casts,
    elided_lifetimes_in_paths,
    single_use_lifetimes,
    variant_size_differences,
    non_ascii_idents,
    unsafe_op_in_unsafe_fn,
    let_underscore_drop,
)]
#![allow(
    // `unknown_lints` lets the two forward-compat lints below be listed even on
    // the CI toolchain (1.88), where they are not yet known, without emitting an
    // "unknown lint" warning of their own. They still apply on newer rustc.
    unknown_lints,
    mismatched_lifetime_syntaxes,          // newer rustc; fires in macro-generated code
    function_casts_as_integer,             // newer rustc; widget callback pointer identity
    // pervasive + feature-sensitive (unused under one feature, live under another):
    unused_imports,
    unused_variables,
    unused_mut,
    dead_code,
    // design lint, pervasive across the layout solver / renderer:
    clippy::too_many_arguments,
    // churny / 3rd-party, low value to scope:
    clippy::legacy_numeric_constants,
    unexpected_cfgs,                        // web-lift diagnostic cfgs
    deprecated,                             // image crate tiff encoder (only under `tiff`)
)]

#[macro_use]
extern crate alloc;
extern crate core;

/// Web-lift diagnostic marker: a volatile store of `val` to the absolute wasm
/// linear-memory address `addr` (the 0x40000–0xF0000 free band the e2e harness
/// peeks via `AzStartup_peekU32`).
///
/// Compiles to NOTHING without the `web_lift`
/// feature — absolute-address stores would segfault native builds (macOS
/// `__PAGEZERO` covers the low 4 GiB). All in-tree diagnostic markers MUST go
/// through this helper rather than calling `core::ptr::write_volatile` on a
/// literal address directly.
///
/// # Safety
///
/// With the `web_lift` feature enabled, `addr` must be a valid, writable wasm
/// linear-memory address (within the 0x40000–0xF0000 diagnostic band). Without
/// the feature this is a no-op and always safe.
#[inline]
pub const unsafe fn az_mark(_addr: u32, _val: u32) {
    #[cfg(feature = "web_lift")]
    core::ptr::write_volatile(_addr as usize as *mut u32, _val);
}

/// Read counterpart of [`az_mark`] (marker counters like `0x60758`).
/// Returns 0 without the `web_lift` feature.
///
/// # Safety
///
/// With the `web_lift` feature enabled, `addr` must be a valid, readable wasm
/// linear-memory address (within the 0x40000–0xF0000 diagnostic band). Without
/// the feature this is a no-op that returns 0 and is always safe.
#[inline]
#[must_use] pub const unsafe fn az_mark_read(_addr: u32) -> u32 {
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
// Scoped (was crate-wide): internal manager types exposed for tests.
#[allow(private_interfaces)]
pub mod managers;
/// CSS layout solver: block, inline, flex, grid, and table formatting.
#[cfg(feature = "text_layout")]
// Scoped (was crate-wide): solver internals — intentional `drop(&_)` scope
// markers, internal types exposed for tests, incremental-relayout assignments,
// generated/parenthesized property code, and exhaustive generated matches.
#[allow(
    dropping_references,
    private_interfaces,
    unreachable_patterns,
    unused_parens,
    unused_doc_comments,
    unused_assignments
)]
pub mod solver3;

/// C-compatible string formatting via `strfmt`.
#[cfg(feature = "strfmt")]
pub mod fmt;
#[cfg(feature = "strfmt")]
pub use fmt::{FmtArg, FmtArgVec, FmtArgVecDestructor, FmtValue, fmt_string};

/// Built-in widgets: button, text input, tabs, tree view, node graph, etc.
#[cfg(feature = "widgets")]
// Scoped (was crate-wide): incremental widget-state assignments and the
// node_graph extern "C" fn that returns `()`.
#[allow(unused_assignments, improper_ctypes_definitions)]
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
    FluentLanguageInfo, FluentLanguageInfoVec, FluentLoadError, FluentLoadErrorVec,
    FluentLocalizerHandle, FluentSyntaxCheckResult,
    FluentZipLoadResult,
};

/// URL parsing (RFC 3986 compliant). Pure-Rust, always present (no TLS deps).
/// URL types live in `azul_core::url`; re-exported so `azul_layout::url::*`
/// keeps resolving. `Url::parse`/`join` are enabled via the `http` feature
/// (which turns on `azul-core/url`).
pub use azul_core::url;
pub use azul_core::url::{Url, UrlParseError, ResultUrlUrlParseError};

/// File system operations (C-compatible wrappers for `std::fs`).
// Scoped (was crate-wide): `///` doc comments before `impl_vec!`/`impl_option!`
// macro invocations, and an infallible inherent `FilePath::from_str` (returns
// `Self`, so it cannot implement the fallible `FromStr` trait).
#[allow(unused_doc_comments, clippy::should_implement_trait)]
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
///
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
// Scoped (was crate-wide): complex rasterizer signatures.
#[allow(clippy::type_complexity)]
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
// Scoped (was crate-wide): complex font-table signatures.
#[allow(clippy::type_complexity)]
pub mod font;

/// Headless backend for CPU-only rendering without a display server.
///
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
// Scoped (was crate-wide): internal types exposed for tests, a labelled
// shaping loop, and complex shaping/cache signatures.
#[allow(private_interfaces, unused_labels, clippy::type_complexity)]
pub mod text3;
/// Thread callback wrappers for the C API.
#[cfg(feature = "text_layout")]
pub mod thread;
/// Timer callback wrappers for the C API.
#[cfg(feature = "text_layout")]
// Scoped (was crate-wide): hand-written `Ord`/`PartialOrd` on a timer type.
#[allow(clippy::non_canonical_partial_ord_impl)]
pub mod timer;
/// Scroll physics timer for momentum-based smooth scrolling.
#[cfg(feature = "text_layout")]
pub mod scroll_timer;
/// Window layout management: relayout, event processing, state sync.
#[cfg(feature = "text_layout")]
// Scoped (was crate-wide): parenthesized layout expressions.
#[allow(unused_parens)]
pub mod window;
/// Window state types (keyboard, mouse, DPI, focus).
#[cfg(feature = "text_layout")]
pub mod window_state;
/// XML and XHTML parsing for declarative UI definitions.
#[cfg(feature = "xml")]
// Scoped (was crate-wide): incremental parser-state assignments.
#[allow(unused_assignments)]
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
    parsed_font: ParsedFont,
) -> azul_css::props::basic::FontRef {
    use core::ffi::c_void;

    extern "C" fn parsed_font_destructor(ptr: *mut c_void) {
        unsafe {
            let _ = Box::from_raw(ptr.cast::<ParsedFont>());
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
#[must_use] pub const fn font_ref_to_parsed_font(
    font_ref: &azul_css::props::basic::FontRef,
) -> &ParsedFont {
    // SAFETY: `font_ref.parsed` was created by `parsed_font_to_font_ref`
    // via `Box::into_raw`, so it points to a valid, aligned `ParsedFont`.
    unsafe { &*font_ref.parsed.cast::<ParsedFont>() }
}
