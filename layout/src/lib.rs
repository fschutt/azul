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
    // transitive dependency-version dups not resolvable in azul's source —
    // syn 1↔2 (proc-macro migration), heck/jni-sys/rustc-hash/rustls-webpki;
    // re-audit when the dep tree aligns.
    clippy::multiple_crate_versions,
)]

#[macro_use]
extern crate alloc;
extern crate core;

// Dependencies kept for downstream/feature-plumbing use but not referenced
// directly in this crate's source — marked intentionally linked so
// unused_crate_dependencies stays quiet (the lint's own suggested fix).
// `brotli-decompressor`: decompresses the codegen material_icons.ttf.br in azul-dll.
#[cfg(feature = "icons")]
use brotli_decompressor as _;
// `lru`: reserved for the slippy-map tile cache (azul-dll widgets).
use lru as _;
// `unicode-normalization` / `xmlwriter`: pulled by text_layout / xml for the
// shaping + SVG-writer paths consumed downstream.
#[cfg(feature = "text_layout")]
use unicode_normalization as _;
#[cfg(feature = "xml")]
use xmlwriter as _;

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
#[cfg(feature = "web_lift")]
#[inline]
pub unsafe fn az_mark(_addr: u32, _val: u32) {
    // Volatile isn't const-callable, so this variant is a plain (non-const) fn.
    core::ptr::write_volatile(_addr as usize as *mut u32, _val);
}
/// No-op `const` variant used when the `web_lift` feature is off.
///
/// # Safety
///
/// Always safe — this variant does nothing; the `unsafe` marker only exists to
/// keep the signature identical to the `web_lift` variant so call sites compile
/// unchanged under both features.
#[cfg(not(feature = "web_lift"))]
#[inline]
pub const unsafe fn az_mark(_addr: u32, _val: u32) {}

/// Read counterpart of [`az_mark`] (marker counters like `0x60758`).
/// Returns 0 without the `web_lift` feature.
///
/// # Safety
///
/// With the `web_lift` feature enabled, `addr` must be a valid, readable wasm
/// linear-memory address (within the 0x40000–0xF0000 diagnostic band). Without
/// the feature this is a no-op that returns 0 and is always safe.
#[cfg(feature = "web_lift")]
#[inline]
#[must_use] pub unsafe fn az_mark_read(_addr: u32) -> u32 {
    // Volatile isn't const-callable, so this variant is a plain (non-const) fn.
    core::ptr::read_volatile(_addr as usize as *const u32)
}
/// No-op `const` variant (returns 0) used when the `web_lift` feature is off.
///
/// # Safety
///
/// Always safe — returns 0 and touches nothing; the `unsafe` marker only exists
/// to keep the signature identical to the `web_lift` variant.
#[cfg(not(feature = "web_lift"))]
#[inline]
#[must_use] pub const unsafe fn az_mark_read(_addr: u32) -> u32 {
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
#[cfg(feature = "zip")]
pub mod zip;
#[cfg(feature = "zip")]
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
// signature must match the `ParseFontFn = fn(LoadedFontSource) -> ...` callback type
// (core/src/resources.rs) and the api.json export, so the owned param cannot become &.
#[allow(clippy::needless_pass_by_value)]
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
            drop(Box::from_raw(ptr.cast::<ParsedFont>()));
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

#[cfg(test)]
mod autotest_generated {
    //! Adversarial unit tests generated by the autotest fleet.
    //!
    //! Covers the four items defined directly in `lib.rs`:
    //!   * `az_mark` / `az_mark_read` — the web-lift diagnostic markers. Only the
    //!     `#[cfg(not(feature = "web_lift"))]` (no-op `const`) variants are
    //!     exercised: the `web_lift` variants store to *absolute* addresses and
    //!     would segfault a native test binary, so they are deliberately untested
    //!     here (the doc comment says as much).
    //!   * `parse_font_fn` — raw bytes → `Option<FontRef>`.
    //!   * `parsed_font_to_font_ref` / `font_ref_to_parsed_font` — the
    //!     `Box::into_raw` / reborrow round-trip, plus the refcounted
    //!     clone/drop contract of the `FontRef` handle those two produce.

    use super::*;

    // ---------------------------------------------------------------
    // az_mark / az_mark_read  (numeric — no-op variants)
    // ---------------------------------------------------------------

    /// Without `web_lift` the read is documented to return 0 for *every* address,
    /// including the ends of the u32 range and the 0x40000–0xF0000 diagnostic band.
    /// A non-zero answer here would mean the native build is really dereferencing
    /// an absolute address.
    #[cfg(not(feature = "web_lift"))]
    #[test]
    fn az_mark_read_is_zero_for_every_boundary_address() {
        let addresses = [
            0u32,
            1,
            0x3_FFFF,          // one below the diagnostic band
            0x4_0000,          // band start
            0x6_0758,          // a real marker counter from the docs
            0xF_0000,          // band end
            0xF_0001,          // one past the band
            i32::MIN as u32,   // "negative" input, reinterpreted
            (-1i32) as u32,    // == u32::MAX
            u32::MAX - 1,
            u32::MAX,
            u32::MAX.wrapping_add(1), // wraps to 0, must not panic
        ];
        for addr in addresses {
            assert_eq!(unsafe { az_mark_read(addr) }, 0, "az_mark_read(0x{addr:x})");
        }
    }

    /// Sweep the whole u32 address space at a coarse stride: no address may panic
    /// or return anything but 0.
    #[cfg(not(feature = "web_lift"))]
    #[test]
    fn az_mark_read_sweeps_the_whole_address_space_as_zero() {
        for addr in (0u32..=u32::MAX).step_by(1 << 24) {
            assert_eq!(unsafe { az_mark_read(addr) }, 0);
        }
    }

    /// A write must remain unobservable (the no-op variant stores nothing), for
    /// every combination of boundary address and boundary value.
    #[cfg(not(feature = "web_lift"))]
    #[test]
    fn az_mark_writes_are_unobservable_without_web_lift() {
        let addresses = [0u32, 0x4_0000, 0x6_0758, 0xF_0000, u32::MAX];
        let values = [0u32, 1, u32::MAX / 2, u32::MAX - 1, u32::MAX, i32::MIN as u32];
        for addr in addresses {
            for val in values {
                unsafe { az_mark(addr, val) };
                assert_eq!(
                    unsafe { az_mark_read(addr) },
                    0,
                    "az_mark(0x{addr:x}, {val}) must not be observable"
                );
            }
        }
        // Repeating a write is still a no-op (idempotent, no accumulating state).
        for _ in 0..1_000 {
            unsafe { az_mark(0x6_0758, u32::MAX) };
        }
        assert_eq!(unsafe { az_mark_read(0x6_0758) }, 0);
    }

    /// Both no-op variants are `const fn`; this fails to *compile* if that ever
    /// regresses (the `web_lift` variants are non-const on purpose, so this test
    /// is gated off there).
    #[cfg(not(feature = "web_lift"))]
    #[test]
    fn az_mark_no_op_variants_are_const_evaluable() {
        const _WRITE_MIN: () = unsafe { az_mark(0, 0) };
        const _WRITE_MAX: () = unsafe { az_mark(u32::MAX, u32::MAX) };
        const READ_ZERO: u32 = unsafe { az_mark_read(0) };
        const READ_MAX: u32 = unsafe { az_mark_read(u32::MAX) };
        assert_eq!(READ_ZERO, 0);
        assert_eq!(READ_MAX, 0);
    }

    // ---------------------------------------------------------------
    // Shared font fixtures (text_layout only)
    // ---------------------------------------------------------------

    /// Positive control: a real single-face TrueType font (774 glyphs, upem 1000).
    #[cfg(feature = "text_layout")]
    const KOHO: &[u8] = include_bytes!("../assets/fonts/test/KoHo-Light.ttf");

    #[cfg(feature = "text_layout")]
    fn loaded_source(bytes: Vec<u8>, index: u32) -> azul_core::resources::LoadedFontSource {
        azul_core::resources::LoadedFontSource {
            data: azul_css::U8Vec::from_vec(bytes),
            index,
            load_outlines: true,
        }
    }

    #[cfg(feature = "text_layout")]
    fn parse_koho() -> ParsedFont {
        ParsedFont::from_bytes(KOHO, 0, &mut Vec::new())
            .expect("KoHo-Light.ttf must parse (positive control)")
    }

    // ---------------------------------------------------------------
    // parse_font_fn  (parser)
    // ---------------------------------------------------------------

    /// Malformed / hostile byte soup must come back as `None`, never a panic and
    /// never a bogus `FontRef` (which would later be dereferenced as a `ParsedFont`).
    #[cfg(feature = "text_layout")]
    #[test]
    fn parse_font_fn_rejects_malformed_input() {
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("empty", Vec::new()),
            ("single_nul", vec![0u8]),
            ("whitespace_only", b"   \t\n".to_vec()),
            ("garbage", (0u8..=255).cycle().take(4096).collect()),
            ("invalid_utf8", vec![0xFF, 0xFE, 0x00]),
            ("sfnt_magic_only", vec![0x00, 0x01, 0x00, 0x00]),
            ("header_only", KOHO[..12].to_vec()),
            ("truncated_font", KOHO[..64].to_vec()),
            ("half_a_font", KOHO[..KOHO.len() / 2].to_vec()),
            ("unicode_emoji", "\u{1F600}\u{1F600}".repeat(1_000).into_bytes()),
            ("combining_marks", "e\u{0301}".repeat(10_000).into_bytes()),
            ("nested_brackets", vec![b'['; 10_000]),
            ("boundary_numbers", b"0 -0 9223372036854775807 NaN inf -inf 1e309".to_vec()),
            ("leading_junk_then_font", {
                let mut v = b"garbage".to_vec();
                v.extend_from_slice(KOHO);
                v
            }),
        ];
        for (name, bytes) in cases {
            assert!(
                parse_font_fn(loaded_source(bytes, 0)).is_none(),
                "{name} must not parse into a FontRef"
            );
        }
    }

    /// Multi-megabyte junk: must terminate quickly and return `None`, not hang or
    /// allocate its way out of memory (the sfnt table directory is attacker-controlled).
    #[cfg(feature = "text_layout")]
    #[test]
    fn parse_font_fn_survives_extremely_long_input() {
        assert!(parse_font_fn(loaded_source(vec![0u8; 1_000_000], 0)).is_none());
        assert!(parse_font_fn(loaded_source(vec![b'a'; 1_000_000], 0)).is_none());
        // "ttcf" collection magic followed by a megabyte of junk offsets.
        let mut ttcf = b"ttcf".to_vec();
        ttcf.extend_from_slice(&vec![0xABu8; 1_000_000]);
        assert!(parse_font_fn(loaded_source(ttcf, 0)).is_none());
    }

    /// Positive control: a real font parses, and the handle we get back really does
    /// point at the parsed face (this is the only sanctioned way to build the
    /// `FontRef` that `font_ref_to_parsed_font` is allowed to reborrow).
    #[cfg(feature = "text_layout")]
    #[test]
    fn parse_font_fn_parses_the_positive_control() {
        let font_ref = parse_font_fn(loaded_source(KOHO.to_vec(), 0))
            .expect("the positive control must parse");
        let parsed = font_ref_to_parsed_font(&font_ref);

        assert_eq!(parsed.num_glyphs(), 774);
        assert_eq!(parsed.num_glyphs(), parsed.maxp_table.num_glyphs);
        assert_eq!(parsed.font_metrics.units_per_em, 1000);
        assert!(parsed.font_metrics.ascent > 0.0);
        assert!(parsed.font_metrics.descent <= 0.0);
        assert!(parsed.font_metrics.ascent.is_finite());
        assert!(parsed.font_metrics.descent.is_finite());
        assert!(parsed.font_metrics.line_gap.is_finite());
        assert_eq!(parsed.font_type, FontType::TrueType);
        assert_eq!(parsed.original_index, 0);
        assert!(parsed.cmap_subtable.is_some());
        assert_eq!(parsed.hash, parse_koho().hash);
    }

    /// `load_outlines` is not consulted by `parse_font_fn` (only `data` + `index`
    /// are). Both settings must therefore yield the same face — if this ever
    /// diverges, callers that flip the flag silently get a different font.
    #[cfg(feature = "text_layout")]
    #[test]
    fn parse_font_fn_ignores_the_load_outlines_flag() {
        let with = azul_core::resources::LoadedFontSource {
            data: azul_css::U8Vec::from_vec(KOHO.to_vec()),
            index: 0,
            load_outlines: true,
        };
        let without = azul_core::resources::LoadedFontSource {
            data: azul_css::U8Vec::from_vec(KOHO.to_vec()),
            index: 0,
            load_outlines: false,
        };
        let a = parse_font_fn(with).expect("parses with outlines");
        let b = parse_font_fn(without).expect("parses without outlines");
        let (pa, pb) = (font_ref_to_parsed_font(&a), font_ref_to_parsed_font(&b));
        assert_eq!(pa.hash, pb.hash);
        assert_eq!(pa.num_glyphs(), pb.num_glyphs());
        assert_eq!(pa.pdf_font_metrics, pb.pdf_font_metrics);
    }

    /// `index` is cast `u32 as usize` and fed to the table provider. Out-of-range
    /// face indices on a single-face font must be deterministic — no panic, no
    /// `12 + index * 4` overflow, and no face with a different glyph count.
    #[cfg(feature = "text_layout")]
    #[test]
    fn parse_font_fn_with_an_out_of_range_index_is_deterministic() {
        let baseline = parse_koho();
        for index in [1u32, 2, 0x7FFF_FFFF, u32::MAX - 1, u32::MAX] {
            if let Some(font_ref) = parse_font_fn(loaded_source(KOHO.to_vec(), index)) {
                let parsed = font_ref_to_parsed_font(&font_ref);
                assert_eq!(
                    parsed.num_glyphs(),
                    baseline.num_glyphs(),
                    "index {index} must not conjure a different face"
                );
                assert_eq!(parsed.original_index, index as usize);
            }
        }
    }

    /// Empty / garbage input on the failing path must not leak or corrupt state
    /// across repeated calls (the destructor is only installed on the `Some` path).
    #[cfg(feature = "text_layout")]
    #[test]
    fn parse_font_fn_failure_path_is_repeatable() {
        for _ in 0..200 {
            assert!(parse_font_fn(loaded_source(Vec::new(), 0)).is_none());
            assert!(parse_font_fn(loaded_source(vec![0xFF; 3], u32::MAX)).is_none());
        }
        // …and a good parse still works afterwards.
        assert!(parse_font_fn(loaded_source(KOHO.to_vec(), 0)).is_some());
    }

    /// Every successful parse mints a *fresh* identity, even for byte-identical
    /// input: `FontRef` equality is the never-reused `id`, not the heap pointer
    /// (freeing a font and reusing its address must not forge identity).
    #[cfg(feature = "text_layout")]
    #[test]
    fn parse_font_fn_mints_a_fresh_identity_per_call() {
        let a = parse_font_fn(loaded_source(KOHO.to_vec(), 0)).expect("parses");
        let b = parse_font_fn(loaded_source(KOHO.to_vec(), 0)).expect("parses");
        assert_ne!(a, b, "two parses of the same bytes are two distinct handles");
        assert_ne!(a.id, b.id);
        assert!(b > a, "ids are monotonically assigned");
        // …but the *content* is identical.
        assert_eq!(
            font_ref_to_parsed_font(&a).hash,
            font_ref_to_parsed_font(&b).hash
        );
    }

    // ---------------------------------------------------------------
    // parsed_font_to_font_ref / font_ref_to_parsed_font  (round-trip)
    // ---------------------------------------------------------------

    /// encode == decode: wrapping a `ParsedFont` and reborrowing it must hand back
    /// the very same face, field for field.
    #[cfg(feature = "text_layout")]
    #[test]
    fn parsed_font_font_ref_round_trip_preserves_the_face() {
        let original = parse_koho();
        let expected_hash = original.hash;
        let expected_glyphs = original.num_glyphs();
        let expected_metrics = original.pdf_font_metrics;
        let expected_upem = original.font_metrics.units_per_em;
        let expected_ascent = original.font_metrics.ascent;
        let expected_type = original.font_type.clone();
        let expected_index = original.original_index;

        let font_ref = parsed_font_to_font_ref(original);
        let decoded = font_ref_to_parsed_font(&font_ref);

        assert_eq!(decoded.hash, expected_hash);
        assert_eq!(decoded.num_glyphs(), expected_glyphs);
        assert_eq!(decoded.pdf_font_metrics, expected_metrics);
        assert_eq!(decoded.font_metrics.units_per_em, expected_upem);
        assert!((decoded.font_metrics.ascent - expected_ascent).abs() < f32::EPSILON);
        assert_eq!(decoded.font_type, expected_type);
        assert_eq!(decoded.original_index, expected_index);
    }

    /// The freshly-minted handle's invariants: live pointer, refcount of exactly 1,
    /// destructor armed, non-zero id (id 0 flags a raw-reconstructed handle).
    #[cfg(feature = "text_layout")]
    #[test]
    fn parsed_font_to_font_ref_handle_invariants() {
        use core::sync::atomic::Ordering as AtomicOrdering;

        let font_ref = parsed_font_to_font_ref(parse_koho());
        assert!(!font_ref.parsed.is_null());
        assert!(!font_ref.copies.is_null());
        assert!(font_ref.run_destructor);
        assert_ne!(font_ref.id, 0, "id 0 is reserved for un-initialised handles");
        assert_eq!(unsafe { (*font_ref.copies).load(AtomicOrdering::SeqCst) }, 1);
        assert_eq!(font_ref.get_parsed(), font_ref.parsed);
    }

    /// `font_ref_to_parsed_font` is a pure reborrow: repeated calls must yield the
    /// same address, and that address must be the handle's `parsed` pointer.
    #[cfg(feature = "text_layout")]
    #[test]
    fn font_ref_to_parsed_font_is_a_stable_reborrow() {
        use core::ffi::c_void;

        let font_ref = parsed_font_to_font_ref(parse_koho());
        let first: *const ParsedFont = font_ref_to_parsed_font(&font_ref);
        let second: *const ParsedFont = font_ref_to_parsed_font(&font_ref);
        assert!(core::ptr::eq(first, second), "reborrow must be stable");
        assert!(core::ptr::eq(first.cast::<c_void>(), font_ref.get_parsed()));
    }

    /// A clone shares the face (same pointer, same id) and bumps the refcount;
    /// dropping the clone must NOT free the face out from under the original.
    /// Reading through the survivor after the drop is the use-after-free probe.
    #[cfg(feature = "text_layout")]
    #[test]
    fn cloning_a_font_ref_shares_the_face_and_the_drop_is_refcounted() {
        use core::sync::atomic::Ordering as AtomicOrdering;

        let original = parsed_font_to_font_ref(parse_koho());
        let expected_hash = font_ref_to_parsed_font(&original).hash;

        let clone = original.clone();
        assert_eq!(clone.id, original.id, "a clone is the same font");
        assert_eq!(clone, original);
        assert!(core::ptr::eq(clone.parsed, original.parsed));
        assert_eq!(unsafe { (*original.copies).load(AtomicOrdering::SeqCst) }, 2);

        drop(clone);
        assert_eq!(unsafe { (*original.copies).load(AtomicOrdering::SeqCst) }, 1);
        assert_eq!(
            font_ref_to_parsed_font(&original).hash,
            expected_hash,
            "the face must survive its clone being dropped"
        );
        assert_eq!(font_ref_to_parsed_font(&original).num_glyphs(), 774);
    }

    /// Hammer the refcount: 1_000 clone/drop cycles (plus a batch held live at once)
    /// must leave the face readable and the count back at 1 — a double-decrement
    /// would free the `ParsedFont` early and turn the next reborrow into a UAF.
    #[cfg(feature = "text_layout")]
    #[test]
    fn font_ref_clone_drop_cycles_do_not_double_free() {
        use core::sync::atomic::Ordering as AtomicOrdering;

        let original = parsed_font_to_font_ref(parse_koho());
        let expected_hash = font_ref_to_parsed_font(&original).hash;

        for _ in 0..1_000 {
            let c = original.clone();
            assert_eq!(font_ref_to_parsed_font(&c).hash, expected_hash);
        }

        let batch: Vec<_> = (0..1_000).map(|_| original.clone()).collect();
        assert_eq!(
            unsafe { (*original.copies).load(AtomicOrdering::SeqCst) },
            1_001
        );
        drop(batch);
        assert_eq!(unsafe { (*original.copies).load(AtomicOrdering::SeqCst) }, 1);
        assert_eq!(font_ref_to_parsed_font(&original).hash, expected_hash);
    }

    /// Identity semantics as a hash/ordering key: clones collapse, independently
    /// wrapped faces don't — even when they hold byte-identical font data.
    #[cfg(feature = "text_layout")]
    #[test]
    fn font_ref_identity_is_per_handle_not_per_content() {
        use std::collections::{BTreeSet, HashSet};

        let a = parsed_font_to_font_ref(parse_koho());
        let b = parsed_font_to_font_ref(parse_koho());
        assert_ne!(a, b);
        assert!(a < b, "ids are monotonically assigned, so a precedes b");
        assert_eq!(
            font_ref_to_parsed_font(&a).hash,
            font_ref_to_parsed_font(&b).hash,
            "…even though the content hash is the same"
        );

        let set: HashSet<_> = vec![a.clone(), a.clone(), a.clone(), b.clone()]
            .into_iter()
            .collect();
        assert_eq!(set.len(), 2, "clones dedup, distinct handles do not");

        let ordered: BTreeSet<_> = vec![b.clone(), a.clone(), b.clone()].into_iter().collect();
        assert_eq!(ordered.len(), 2);
        assert_eq!(ordered.iter().next(), Some(&a));
    }

    /// Wrapping many faces in a row must keep every handle pointing at its *own*
    /// face — a shared/stale `Box::into_raw` would make them alias.
    #[cfg(feature = "text_layout")]
    #[test]
    fn many_font_refs_do_not_alias_each_other() {
        let refs: Vec<_> = (0..16).map(|_| parsed_font_to_font_ref(parse_koho())).collect();
        for (i, a) in refs.iter().enumerate() {
            assert_eq!(font_ref_to_parsed_font(a).num_glyphs(), 774);
            for b in refs.iter().skip(i + 1) {
                assert!(
                    !core::ptr::eq(a.parsed, b.parsed),
                    "independently boxed faces must not share a pointer"
                );
                assert_ne!(a.id, b.id);
            }
        }
    }
}
