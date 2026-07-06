//! FFI bindings for rust-fontconfig
//!
//! This module provides C-compatible bindings for the rust-fontconfig library.

use crate::*;
#[cfg(feature = "async-registry")]
use crate::registry::FcFontRegistry;
use std::ffi::{c_char, c_uint, c_void, CStr, CString};
use std::fmt::Write;
use std::mem;
use std::ptr;
use std::slice;
#[cfg(feature = "async-registry")]
use std::sync::Arc;

/// C-compatible font ID representation
#[repr(C)]
pub struct FcFontIdC {
    high: u64,
    low: u64,
}

impl FcFontIdC {
    fn from_fontid(id: &FontId) -> Self {
        let id_value = id.0;
        Self {
            high: (id_value >> 64) as u64,
            low: id_value as u64,
        }
    }
}

impl FontId {
    fn from_fontid_c(id: &FcFontIdC) -> Self {
        let combined = ((id.high as u128) << 64) | (id.low as u128);
        FontId(combined)
    }
}

/// C-compatible representation of a font match without fallbacks
#[repr(C)]
pub struct FcFontMatchNoFallbackC {
    id: FcFontIdC,
    unicode_ranges: *mut UnicodeRange,
    unicode_ranges_count: usize,
}

/// C-compatible representation of a font match with fallbacks
#[repr(C)]
pub struct FcFontMatchC {
    id: FcFontIdC,
    unicode_ranges: *mut UnicodeRange,
    unicode_ranges_count: usize,
    fallbacks: *mut FcFontMatchNoFallbackC,
    fallbacks_count: usize,
}

/// C-compatible font path
#[repr(C)]
pub struct FcFontPathC {
    path: *mut c_char,
    font_index: usize,
}

/// C-compatible in-memory font data
#[repr(C)]
pub struct FcFontC {
    bytes: *mut u8,
    bytes_len: usize,
    font_index: usize,
    id: *mut c_char,
}

/// C-compatible font metadata
#[repr(C)]
pub struct FcFontMetadataC {
    copyright: *mut c_char,
    designer: *mut c_char,
    designer_url: *mut c_char,
    font_family: *mut c_char,
    font_subfamily: *mut c_char,
    full_name: *mut c_char,
    id_description: *mut c_char,
    license: *mut c_char,
    license_url: *mut c_char,
    manufacturer: *mut c_char,
    manufacturer_url: *mut c_char,
    postscript_name: *mut c_char,
    preferred_family: *mut c_char,
    preferred_subfamily: *mut c_char,
    trademark: *mut c_char,
    unique_id: *mut c_char,
    version: *mut c_char,
}

/// C-compatible render config (uses -1 for "unset" instead of Option)
#[repr(C)]
pub struct FcFontRenderConfigC {
    antialias: i32,      // -1=unset, 0=false, 1=true
    hinting: i32,
    hintstyle: i32,      // -1=unset, or FcHintStyle value
    autohint: i32,
    rgba: i32,           // -1=unset, or FcRgba value
    lcdfilter: i32,      // -1=unset, or FcLcdFilter value
    embeddedbitmap: i32,
    embolden: i32,
    dpi: f64,            // -1.0=unset
    scale: f64,          // -1.0=unset
    minspace: i32,
}

fn render_config_to_c(rc: &FcFontRenderConfig) -> FcFontRenderConfigC {
    fn bool_opt(v: Option<bool>) -> i32 {
        match v { Some(true) => 1, Some(false) => 0, None => -1 }
    }
    FcFontRenderConfigC {
        antialias: bool_opt(rc.antialias),
        hinting: bool_opt(rc.hinting),
        hintstyle: rc.hintstyle.map(|v| v as i32).unwrap_or(-1),
        autohint: bool_opt(rc.autohint),
        rgba: rc.rgba.map(|v| v as i32).unwrap_or(-1),
        lcdfilter: rc.lcdfilter.map(|v| v as i32).unwrap_or(-1),
        embeddedbitmap: bool_opt(rc.embeddedbitmap),
        embolden: bool_opt(rc.embolden),
        dpi: rc.dpi.unwrap_or(-1.0),
        scale: rc.scale.unwrap_or(-1.0),
        minspace: bool_opt(rc.minspace),
    }
}

fn c_to_render_config(rc: &FcFontRenderConfigC) -> FcFontRenderConfig {
    fn int_bool(v: i32) -> Option<bool> {
        match v { 0 => Some(false), 1 => Some(true), _ => None }
    }
    FcFontRenderConfig {
        antialias: int_bool(rc.antialias),
        hinting: int_bool(rc.hinting),
        hintstyle: match rc.hintstyle {
            0 => Some(FcHintStyle::None), 1 => Some(FcHintStyle::Slight),
            2 => Some(FcHintStyle::Medium), 3 => Some(FcHintStyle::Full),
            _ => None,
        },
        autohint: int_bool(rc.autohint),
        rgba: match rc.rgba {
            0 => Some(FcRgba::Unknown), 1 => Some(FcRgba::Rgb), 2 => Some(FcRgba::Bgr),
            3 => Some(FcRgba::Vrgb), 4 => Some(FcRgba::Vbgr), 5 => Some(FcRgba::None),
            _ => None,
        },
        lcdfilter: match rc.lcdfilter {
            0 => Some(FcLcdFilter::None), 1 => Some(FcLcdFilter::Default),
            2 => Some(FcLcdFilter::Light), 3 => Some(FcLcdFilter::Legacy),
            _ => None,
        },
        embeddedbitmap: int_bool(rc.embeddedbitmap),
        embolden: int_bool(rc.embolden),
        dpi: if rc.dpi < 0.0 { None } else { Some(rc.dpi) },
        scale: if rc.scale < 0.0 { None } else { Some(rc.scale) },
        minspace: int_bool(rc.minspace),
    }
}

/// C-compatible pattern for matching
#[repr(C)]
pub struct FcPatternC {
    name: *mut c_char,
    family: *mut c_char,
    italic: PatternMatch,
    oblique: PatternMatch,
    bold: PatternMatch,
    monospace: PatternMatch,
    condensed: PatternMatch,
    weight: FcWeight,
    stretch: FcStretch,
    unicode_ranges: *mut UnicodeRange,
    unicode_ranges_count: usize,
    metadata: FcFontMetadataC,
    render_config: FcFontRenderConfigC,
}

/// Reason type for trace messages
#[repr(C)]
pub enum FcReasonTypeC {
    NameMismatch = 0,
    FamilyMismatch = 1,
    StyleMismatch = 2,
    WeightMismatch = 3,
    StretchMismatch = 4,
    UnicodeRangeMismatch = 5,
    Success = 6,
}

/// Trace message level
#[repr(C)]
pub enum FcTraceLevelC {
    Debug = 0,
    Info = 1,
    Warning = 2,
    Error = 3,
}

impl From<TraceLevel> for FcTraceLevelC {
    fn from(level: TraceLevel) -> Self {
        match level {
            TraceLevel::Debug => FcTraceLevelC::Debug,
            TraceLevel::Info => FcTraceLevelC::Info,
            TraceLevel::Warning => FcTraceLevelC::Warning,
            TraceLevel::Error => FcTraceLevelC::Error,
        }
    }
}

/// C-compatible trace message
#[repr(C)]
pub struct FcTraceMsgC {
    level: FcTraceLevelC,
    path: *mut c_char,
    reason: *mut c_void, // Opaque pointer to MatchReason
}

/// Helper to convert Rust Option<String> to C char pointer
fn option_string_to_c_char(s: Option<&String>) -> *mut c_char {
    match s {
        Some(s) => {
            let c_str = CString::new(s.as_str()).unwrap_or_default();
            let ptr = c_str.into_raw();
            ptr
        }
        None => ptr::null_mut(),
    }
}

/// Helper to free C string
unsafe fn free_c_string(s: *mut c_char) {
    if !s.is_null() {
        let _ = CString::from_raw(s);
    }
}

/// Helper to convert C string to Rust Option<String>
unsafe fn c_char_to_option_string(s: *const c_char) -> Option<String> {
    if s.is_null() {
        None
    } else {
        Some(CStr::from_ptr(s).to_string_lossy().into_owned())
    }
}

/// Convert Rust FcFontMetadata to C FcFontMetadataC
fn metadata_to_c(metadata: &FcFontMetadata) -> FcFontMetadataC {
    FcFontMetadataC {
        copyright: option_string_to_c_char(metadata.copyright.as_ref()),
        designer: option_string_to_c_char(metadata.designer.as_ref()),
        designer_url: option_string_to_c_char(metadata.designer_url.as_ref()),
        font_family: option_string_to_c_char(metadata.font_family.as_ref()),
        font_subfamily: option_string_to_c_char(metadata.font_subfamily.as_ref()),
        full_name: option_string_to_c_char(metadata.full_name.as_ref()),
        id_description: option_string_to_c_char(metadata.id_description.as_ref()),
        license: option_string_to_c_char(metadata.license.as_ref()),
        license_url: option_string_to_c_char(metadata.license_url.as_ref()),
        manufacturer: option_string_to_c_char(metadata.manufacturer.as_ref()),
        manufacturer_url: option_string_to_c_char(metadata.manufacturer_url.as_ref()),
        postscript_name: option_string_to_c_char(metadata.postscript_name.as_ref()),
        preferred_family: option_string_to_c_char(metadata.preferred_family.as_ref()),
        preferred_subfamily: option_string_to_c_char(metadata.preferred_subfamily.as_ref()),
        trademark: option_string_to_c_char(metadata.trademark.as_ref()),
        unique_id: option_string_to_c_char(metadata.unique_id.as_ref()),
        version: option_string_to_c_char(metadata.version.as_ref()),
    }
}

/// Convert C FcFontMetadataC to Rust FcFontMetadata
unsafe fn c_to_metadata(m: &FcFontMetadataC) -> FcFontMetadata {
    FcFontMetadata {
        copyright: c_char_to_option_string(m.copyright),
        designer: c_char_to_option_string(m.designer),
        designer_url: c_char_to_option_string(m.designer_url),
        font_family: c_char_to_option_string(m.font_family),
        font_subfamily: c_char_to_option_string(m.font_subfamily),
        full_name: c_char_to_option_string(m.full_name),
        id_description: c_char_to_option_string(m.id_description),
        license: c_char_to_option_string(m.license),
        license_url: c_char_to_option_string(m.license_url),
        manufacturer: c_char_to_option_string(m.manufacturer),
        manufacturer_url: c_char_to_option_string(m.manufacturer_url),
        postscript_name: c_char_to_option_string(m.postscript_name),
        preferred_family: c_char_to_option_string(m.preferred_family),
        preferred_subfamily: c_char_to_option_string(m.preferred_subfamily),
        trademark: c_char_to_option_string(m.trademark),
        unique_id: c_char_to_option_string(m.unique_id),
        version: c_char_to_option_string(m.version),
    }
}

/// Free all C strings inside a FcFontMetadataC
unsafe fn free_metadata_c(m: &mut FcFontMetadataC) {
    free_c_string(m.copyright);
    free_c_string(m.designer);
    free_c_string(m.designer_url);
    free_c_string(m.font_family);
    free_c_string(m.font_subfamily);
    free_c_string(m.full_name);
    free_c_string(m.id_description);
    free_c_string(m.license);
    free_c_string(m.license_url);
    free_c_string(m.manufacturer);
    free_c_string(m.manufacturer_url);
    free_c_string(m.postscript_name);
    free_c_string(m.preferred_family);
    free_c_string(m.preferred_subfamily);
    free_c_string(m.trademark);
    free_c_string(m.unique_id);
    free_c_string(m.version);
}

/// Transfer ownership of a Vec into a raw pointer and length.
fn vec_into_raw_parts<T>(vec: Vec<T>) -> (*mut T, usize) {
    let mut vec = vec;
    let ptr = vec.as_mut_ptr();
    let len = vec.len();
    mem::forget(vec);
    (ptr, len)
}

/// Reconstruct and drop a Vec previously leaked via `vec_into_raw_parts`.
unsafe fn free_raw_vec<T>(ptr: *mut T, len: usize) {
    if !ptr.is_null() && len > 0 {
        let _ = Vec::from_raw_parts(ptr, len, len);
    }
}

/// Convert a C string array to a Rust Vec<String>.
unsafe fn c_string_array_to_vec(arr: *const *const c_char, count: usize) -> Vec<String> {
    slice::from_raw_parts(arr, count)
        .iter()
        .filter_map(|&s| {
            if s.is_null() {
                None
            } else {
                Some(CStr::from_ptr(s).to_string_lossy().into_owned())
            }
        })
        .collect()
}

/// Convert Rust FcPattern to C FcPatternC
fn pattern_to_c(pattern: &FcPattern) -> FcPatternC {
    let name = option_string_to_c_char(pattern.name.as_ref());
    let family = option_string_to_c_char(pattern.family.as_ref());

    let unicode_ranges_count = pattern.unicode_ranges.len();
    let unicode_ranges = if unicode_ranges_count > 0 {
        let ranges: Vec<UnicodeRange> = pattern.unicode_ranges.clone();
        let (ptr, _) = vec_into_raw_parts(ranges);
        ptr
    } else {
        ptr::null_mut()
    };

    let metadata = metadata_to_c(&pattern.metadata);

    FcPatternC {
        name,
        family,
        italic: pattern.italic,
        oblique: pattern.oblique,
        bold: pattern.bold,
        monospace: pattern.monospace,
        condensed: pattern.condensed,
        weight: pattern.weight,
        stretch: pattern.stretch,
        unicode_ranges,
        unicode_ranges_count,
        metadata,
        render_config: render_config_to_c(&pattern.render_config),
    }
}

/// Convert C FcPatternC to Rust FcPattern
unsafe fn c_to_pattern(pattern: *const FcPatternC) -> FcPattern {
    let pattern = &*pattern;

    let name = c_char_to_option_string(pattern.name);
    let family = c_char_to_option_string(pattern.family);

    let mut unicode_ranges = Vec::new();
    if !pattern.unicode_ranges.is_null() && pattern.unicode_ranges_count > 0 {
        unicode_ranges =
            slice::from_raw_parts(pattern.unicode_ranges, pattern.unicode_ranges_count).to_vec();
    }

    let metadata = c_to_metadata(&pattern.metadata);

    FcPattern {
        name,
        family,
        italic: pattern.italic,
        oblique: pattern.oblique,
        bold: pattern.bold,
        monospace: pattern.monospace,
        condensed: pattern.condensed,
        weight: pattern.weight,
        stretch: pattern.stretch,
        unicode_ranges,
        metadata,
        render_config: c_to_render_config(&pattern.render_config),
    }
}

/// Free a C pattern
unsafe fn free_pattern_c(pattern: *mut FcPatternC) {
    if pattern.is_null() {
        return;
    }

    let pattern = &mut *pattern;

    free_c_string(pattern.name);
    free_c_string(pattern.family);

    free_raw_vec(pattern.unicode_ranges, pattern.unicode_ranges_count);

    // Free metadata strings
    free_metadata_c(&mut pattern.metadata);

    let _ = Box::from_raw(pattern);
}

/// Convert Rust font match to C representation
fn font_match_to_c(cache: &FcFontCache, match_obj: &FontMatch) -> FcFontMatchC {
    let id = FcFontIdC::from_fontid(&match_obj.id);

    let unicode_ranges_count = match_obj.unicode_ranges.len();
    let unicode_ranges = if unicode_ranges_count > 0 {
        let ranges: Vec<UnicodeRange> = match_obj.unicode_ranges.clone();
        let (ptr, _) = vec_into_raw_parts(ranges);
        ptr
    } else {
        ptr::null_mut()
    };

    // Compute fallbacks lazily for FFI (expensive operation)
    let mut trace = Vec::new();
    let computed_fallbacks = cache.compute_fallbacks(&match_obj.id, &mut trace);
    let fallbacks_count = computed_fallbacks.len();
    let fallbacks = if fallbacks_count > 0 {
        let mut fb = Vec::with_capacity(fallbacks_count);
        for fallback in &computed_fallbacks {
            let fallback_ranges_count = fallback.unicode_ranges.len();
            let fallback_ranges = if fallback_ranges_count > 0 {
                let ranges: Vec<UnicodeRange> = fallback.unicode_ranges.clone();
                let (ptr, _) = vec_into_raw_parts(ranges);
                ptr
            } else {
                ptr::null_mut()
            };

            fb.push(FcFontMatchNoFallbackC {
                id: FcFontIdC::from_fontid(&fallback.id),
                unicode_ranges: fallback_ranges,
                unicode_ranges_count: fallback_ranges_count,
            });
        }
        let (ptr, _) = vec_into_raw_parts(fb);
        ptr
    } else {
        ptr::null_mut()
    };

    FcFontMatchC {
        id,
        unicode_ranges,
        unicode_ranges_count,
        fallbacks,
        fallbacks_count,
    }
}

/// Free a C font match
unsafe fn free_font_match_c(match_obj: *mut FcFontMatchC) {
    if match_obj.is_null() {
        return;
    }

    let match_obj = &mut *match_obj;

    free_raw_vec(match_obj.unicode_ranges, match_obj.unicode_ranges_count);

    if !match_obj.fallbacks.is_null() && match_obj.fallbacks_count > 0 {
        let fallbacks = slice::from_raw_parts_mut(match_obj.fallbacks, match_obj.fallbacks_count);

        for fallback in fallbacks {
            free_raw_vec(fallback.unicode_ranges, fallback.unicode_ranges_count);
        }

        free_raw_vec(match_obj.fallbacks, match_obj.fallbacks_count);
    }

    let _ = Box::from_raw(match_obj);
}

/// Convert trace messages to C representation
fn trace_msgs_to_c(trace: &[TraceMsg]) -> (*mut FcTraceMsgC, usize) {
    if trace.is_empty() {
        return (ptr::null_mut(), 0);
    }

    let count = trace.len();
    let mut trace_c = Vec::with_capacity(count);

    for msg in trace {
        let path = CString::new(msg.path.clone())
            .unwrap_or_default()
            .into_raw();

        // Create a boxed MatchReason and convert to opaque pointer
        let reason = Box::new(msg.reason.clone());
        let reason_ptr = Box::into_raw(reason) as *mut c_void;

        trace_c.push(FcTraceMsgC {
            level: msg.level.into(),
            path,
            reason: reason_ptr,
        });
    }

    let (ptr, count) = vec_into_raw_parts(trace_c);

    (ptr, count)
}

/// Create a new font ID
#[no_mangle]
pub extern "C" fn fc_font_id_new() -> FcFontIdC {
    FcFontIdC::from_fontid(&FontId::new())
}

/// Create a new font cache
#[no_mangle]
pub extern "C" fn fc_cache_build() -> *mut FcFontCache {
    let cache = FcFontCache::build();
    Box::into_raw(Box::new(cache))
}

/// Free the font cache
#[no_mangle]
pub extern "C" fn fc_cache_free(cache: *mut FcFontCache) {
    if !cache.is_null() {
        unsafe {
            let _ = Box::from_raw(cache);
        }
    }
}

/// Create a new default pattern
#[no_mangle]
pub extern "C" fn fc_pattern_new() -> *mut FcPatternC {
    let pattern = FcPattern::default();
    let pattern_c = pattern_to_c(&pattern);
    Box::into_raw(Box::new(pattern_c))
}

/// Free a pattern
#[no_mangle]
pub extern "C" fn fc_pattern_free(pattern: *mut FcPatternC) {
    if !pattern.is_null() {
        unsafe {
            free_pattern_c(pattern);
        }
    }
}

/// Set pattern name
#[no_mangle]
pub extern "C" fn fc_pattern_set_name(pattern: *mut FcPatternC, name: *const c_char) {
    if pattern.is_null() || name.is_null() {
        return;
    }

    unsafe {
        let pattern = &mut *pattern;

        // Free existing name if any
        free_c_string(pattern.name);

        // Set new name
        let name_str = CStr::from_ptr(name).to_string_lossy().into_owned();
        pattern.name = CString::new(name_str).unwrap_or_default().into_raw();
    }
}

/// Set pattern family
#[no_mangle]
pub extern "C" fn fc_pattern_set_family(pattern: *mut FcPatternC, family: *const c_char) {
    if pattern.is_null() || family.is_null() {
        return;
    }

    unsafe {
        let pattern = &mut *pattern;

        // Free existing family if any
        free_c_string(pattern.family);

        // Set new family
        let family_str = CStr::from_ptr(family).to_string_lossy().into_owned();
        pattern.family = CString::new(family_str).unwrap_or_default().into_raw();
    }
}

/// Set pattern italic
#[no_mangle]
pub extern "C" fn fc_pattern_set_italic(pattern: *mut FcPatternC, italic: PatternMatch) {
    if pattern.is_null() {
        return;
    }

    unsafe {
        let pattern = &mut *pattern;
        pattern.italic = italic;
    }
}

/// Set pattern bold
#[no_mangle]
pub extern "C" fn fc_pattern_set_bold(pattern: *mut FcPatternC, bold: PatternMatch) {
    if pattern.is_null() {
        return;
    }

    unsafe {
        let pattern = &mut *pattern;
        pattern.bold = bold;
    }
}

/// Set pattern monospace
#[no_mangle]
pub extern "C" fn fc_pattern_set_monospace(pattern: *mut FcPatternC, monospace: PatternMatch) {
    if pattern.is_null() {
        return;
    }

    unsafe {
        let pattern = &mut *pattern;
        pattern.monospace = monospace;
    }
}

/// Set pattern weight
#[no_mangle]
pub extern "C" fn fc_pattern_set_weight(pattern: *mut FcPatternC, weight: FcWeight) {
    if pattern.is_null() {
        return;
    }

    unsafe {
        let pattern = &mut *pattern;
        pattern.weight = weight;
    }
}

/// Set pattern stretch
#[no_mangle]
pub extern "C" fn fc_pattern_set_stretch(pattern: *mut FcPatternC, stretch: FcStretch) {
    if pattern.is_null() {
        return;
    }

    unsafe {
        let pattern = &mut *pattern;
        pattern.stretch = stretch;
    }
}

/// Add unicode range to pattern
#[no_mangle]
pub extern "C" fn fc_pattern_add_unicode_range(
    pattern: *mut FcPatternC,
    start: c_uint,
    end: c_uint,
) {
    if pattern.is_null() {
        return;
    }

    unsafe {
        let pattern = &mut *pattern;

        let new_range = UnicodeRange { start, end };

        // Create a new array with additional capacity
        let mut new_ranges = Vec::with_capacity(pattern.unicode_ranges_count + 1);

        // Copy existing ranges if any
        if !pattern.unicode_ranges.is_null() && pattern.unicode_ranges_count > 0 {
            new_ranges.extend_from_slice(slice::from_raw_parts(
                pattern.unicode_ranges,
                pattern.unicode_ranges_count,
            ));

            // Free the old array
            free_raw_vec(pattern.unicode_ranges, pattern.unicode_ranges_count);
        }

        // Add the new range
        new_ranges.push(new_range);

        // Update the pattern
        let (ptr, len) = vec_into_raw_parts(new_ranges);
        pattern.unicode_ranges = ptr;
        pattern.unicode_ranges_count = len;
    }
}

/// Free a font match
#[no_mangle]
pub extern "C" fn fc_font_match_free(match_obj: *mut FcFontMatchC) {
    if !match_obj.is_null() {
        unsafe {
            free_font_match_c(match_obj);
        }
    }
}

/// Free an array of font matches
#[no_mangle]
pub extern "C" fn fc_font_matches_free(matches: *mut *mut FcFontMatchC, count: usize) {
    if matches.is_null() || count == 0 {
        return;
    }

    unsafe {
        let matches_slice = slice::from_raw_parts_mut(matches, count);

        for match_ptr in matches_slice {
            if !match_ptr.is_null() {
                free_font_match_c(*match_ptr);
            }
        }

        free_raw_vec(matches, count);
    }
}

/// Free font path
#[no_mangle]
pub extern "C" fn fc_font_path_free(path: *mut FcFontPathC) {
    if path.is_null() {
        return;
    }

    unsafe {
        let path = &mut *path;
        free_c_string(path.path);
        let _ = Box::from_raw(path);
    }
}

/// Free an in-memory font
#[no_mangle]
pub extern "C" fn fc_font_free(font: *mut FcFontC) {
    if font.is_null() {
        return;
    }

    unsafe {
        let font = &mut *font;

        free_raw_vec(font.bytes, font.bytes_len);

        free_c_string(font.id);
        let _ = Box::from_raw(font);
    }
}

/// Get trace reason type
#[no_mangle]
pub extern "C" fn fc_trace_get_reason_type(trace: *const FcTraceMsgC) -> FcReasonTypeC {
    if trace.is_null() {
        return FcReasonTypeC::Success;
    }

    unsafe {
        let trace = &*trace;

        if trace.reason.is_null() {
            return FcReasonTypeC::Success;
        }

        let reason = &*(trace.reason as *const MatchReason);

        match reason {
            MatchReason::NameMismatch { .. } => FcReasonTypeC::NameMismatch,
            MatchReason::FamilyMismatch { .. } => FcReasonTypeC::FamilyMismatch,
            MatchReason::StyleMismatch { .. } => FcReasonTypeC::StyleMismatch,
            MatchReason::WeightMismatch { .. } => FcReasonTypeC::WeightMismatch,
            MatchReason::StretchMismatch { .. } => FcReasonTypeC::StretchMismatch,
            MatchReason::UnicodeRangeMismatch { .. } => FcReasonTypeC::UnicodeRangeMismatch,
            MatchReason::Success => FcReasonTypeC::Success,
        }
    }
}

/// Free trace messages
#[no_mangle]
pub extern "C" fn fc_trace_free(trace: *mut FcTraceMsgC, count: usize) {
    if trace.is_null() || count == 0 {
        return;
    }

    unsafe {
        let trace_slice = slice::from_raw_parts_mut(trace, count);

        for msg in trace_slice {
            free_c_string(msg.path);

            if !msg.reason.is_null() {
                let _ = Box::from_raw(msg.reason as *mut MatchReason);
            }
        }

        free_raw_vec(trace, count);
    }
}

/// Convert font ID to string
#[no_mangle]
pub extern "C" fn fc_font_id_to_string(
    id: *const FcFontIdC,
    buffer: *mut c_char,
    buffer_size: usize,
) -> bool {
    if id.is_null() || buffer.is_null() || buffer_size == 0 {
        return false;
    }

    unsafe {
        let id_rust = FontId::from_fontid_c(&*id);
        let mut id_str = String::new();

        if write!(id_str, "{}", id_rust).is_err() {
            return false;
        }

        if id_str.len() >= buffer_size {
            return false;
        }

        let c_str = CString::new(id_str).unwrap_or_default();
        let src = c_str.as_bytes_with_nul();
        let dest = slice::from_raw_parts_mut(buffer as *mut u8, buffer_size);

        for (i, &byte) in src.iter().enumerate() {
            if i < buffer_size {
                dest[i] = byte;
            } else {
                return false;
            }
        }

        true
    }
}

/// Font info for listing fonts
#[repr(C)]
pub struct FcFontInfoC {
    id: FcFontIdC,
    name: *mut c_char,
    family: *mut c_char,
}

/// Free array of font info
#[no_mangle]
pub extern "C" fn fc_font_info_free(info: *mut FcFontInfoC, count: usize) {
    if info.is_null() || count == 0 {
        return;
    }

    unsafe {
        let info_slice = slice::from_raw_parts_mut(info, count);

        for item in info_slice {
            free_c_string(item.name);
            free_c_string(item.family);
        }

        free_raw_vec(info, count);
    }
}

/// Free font metadata
#[no_mangle]
pub extern "C" fn fc_font_metadata_free(metadata: *mut FcFontMetadataC) {
    if metadata.is_null() {
        return;
    }

    unsafe {
        let metadata = &mut *metadata;
        free_metadata_c(metadata);
        let _ = Box::from_raw(metadata);
    }
}

/// Get font path by ID
#[no_mangle]
pub extern "C" fn fc_cache_get_font_path(
    cache: *const FcFontCache,
    id: *const FcFontIdC,
) -> *mut FcFontPathC {
    if cache.is_null() || id.is_null() {
        return ptr::null_mut();
    }

    unsafe {
        let cache = &*cache;
        let id_rust = FontId::from_fontid_c(&*id);

        match cache.get_font_by_id(&id_rust) {
            Some(OwnedFontSource::Disk(path)) => {
                let path_c = FcFontPathC {
                    path: CString::new(path.path.clone())
                        .unwrap_or_default()
                        .into_raw(),
                    font_index: path.font_index,
                };

                Box::into_raw(Box::new(path_c))
            }
            Some(OwnedFontSource::Memory(font)) => {
                // For memory fonts, return a special path
                let path_c = FcFontPathC {
                    path: CString::new(format!("memory:{}", font.id))
                        .unwrap_or_default()
                        .into_raw(),
                    font_index: font.font_index,
                };

                Box::into_raw(Box::new(path_c))
            }
            None => ptr::null_mut(),
        }
    }
}

/// Query a font from the cache
#[no_mangle]
pub extern "C" fn fc_cache_query(
    cache: *const FcFontCache,
    pattern: *const FcPatternC,
    trace: *mut *mut FcTraceMsgC,
    trace_count: *mut usize,
) -> *mut FcFontMatchC {
    if cache.is_null() || pattern.is_null() || trace.is_null() || trace_count.is_null() {
        return ptr::null_mut();
    }

    unsafe {
        let cache = &*cache;
        let pattern_rust = c_to_pattern(pattern);

        let mut trace_msgs = Vec::new();
        let result = cache.query(&pattern_rust, &mut trace_msgs);

        // Convert trace messages
        let (trace_c, count) = trace_msgs_to_c(&trace_msgs);
        *trace = trace_c;
        *trace_count = count;

        match result {
            Some(match_obj) => {
                let match_c = font_match_to_c(cache, &match_obj);
                Box::into_raw(Box::new(match_c))
            }
            None => ptr::null_mut(),
        }
    }
}

/// Get metadata by font ID
#[no_mangle]
pub extern "C" fn fc_cache_get_font_metadata(
    cache: *const FcFontCache,
    id: *const FcFontIdC,
) -> *mut FcFontMetadataC {
    if cache.is_null() || id.is_null() {
        return ptr::null_mut();
    }

    unsafe {
        let cache = &*cache;
        let id_rust = FontId::from_fontid_c(&*id);

        // Get metadata directly from ID
        let pattern = match cache.get_metadata_by_id(&id_rust) {
            Some(pattern) => pattern,
            None => return ptr::null_mut(),
        };

        // Create metadata from pattern
        Box::into_raw(Box::new(metadata_to_c(&pattern.metadata)))
    }
}

/// Get per-font render config by font ID
#[no_mangle]
pub extern "C" fn fc_cache_get_render_config(
    cache: *const FcFontCache,
    id: *const FcFontIdC,
) -> FcFontRenderConfigC {
    let default = render_config_to_c(&FcFontRenderConfig::default());
    if cache.is_null() || id.is_null() {
        return default;
    }
    unsafe {
        let cache = &*cache;
        let id_rust = FontId::from_fontid_c(&*id);
        cache.get_metadata_by_id(&id_rust)
            .map(|p| render_config_to_c(&p.render_config))
            .unwrap_or(default)
    }
}

/// Get per-font render config by font ID from the registry
#[cfg(feature = "async-registry")]
#[no_mangle]
pub extern "C" fn fc_registry_get_render_config(
    registry: *const Arc<FcFontRegistry>,
    id: *const FcFontIdC,
) -> FcFontRenderConfigC {
    let default = render_config_to_c(&FcFontRenderConfig::default());
    if registry.is_null() || id.is_null() {
        return default;
    }
    unsafe {
        let registry = &*registry;
        let id_rust = FontId::from_fontid_c(&*id);
        registry.get_metadata_by_id(&id_rust)
            .map(|p| render_config_to_c(&p.render_config))
            .unwrap_or(default)
    }
}

/// Create a new in-memory font
#[no_mangle]
pub extern "C" fn fc_font_new(
    bytes: *const u8,
    bytes_len: usize,
    font_index: usize,
    id: *const c_char,
) -> *mut FcFontC {
    if bytes.is_null() || bytes_len == 0 || id.is_null() {
        return ptr::null_mut();
    }

    unsafe {
        let id_rust = CStr::from_ptr(id).to_string_lossy().into_owned();
        let bytes_vec = slice::from_raw_parts(bytes, bytes_len).to_vec();

        let bytes_ptr = Box::into_raw(bytes_vec.into_boxed_slice()) as *mut u8;

        let font = FcFontC {
            bytes: bytes_ptr,
            bytes_len,
            font_index,
            id: CString::new(id_rust).unwrap_or_default().into_raw(),
        };

        Box::into_raw(Box::new(font))
    }
}

/// Get all available fonts in the cache
#[no_mangle]
pub extern "C" fn fc_cache_list_fonts(
    cache: *const FcFontCache,
    count: *mut usize,
) -> *mut FcFontInfoC {
    if cache.is_null() || count.is_null() {
        return ptr::null_mut();
    }

    unsafe {
        let cache = &*cache;
        let font_list = cache.list();

        if font_list.is_empty() {
            *count = 0;
            return ptr::null_mut();
        }

        let mut font_info = Vec::with_capacity(font_list.len());

        for (pattern, id) in font_list {
            let name = option_string_to_c_char(pattern.name.as_ref());
            let family = option_string_to_c_char(pattern.family.as_ref());

            font_info.push(FcFontInfoC {
                id: FcFontIdC::from_fontid(&id),
                name,
                family,
            });
        }

        let (ptr, len) = vec_into_raw_parts(font_info);
        *count = len;

        ptr
    }
}

/// Add in-memory fonts to the cache
#[no_mangle]
pub extern "C" fn fc_cache_add_memory_fonts(
    cache: *mut FcFontCache,
    patterns: *const FcPatternC,
    fonts: *const FcFontC,
    count: usize,
) {
    if cache.is_null() || patterns.is_null() || fonts.is_null() || count == 0 {
        return;
    }

    unsafe {
        let cache = &mut *cache;
        let patterns_slice = slice::from_raw_parts(patterns, count);
        let fonts_slice = slice::from_raw_parts(fonts, count);

        let mut memory_fonts = Vec::with_capacity(count);

        for i in 0..count {
            let pattern = c_to_pattern(&patterns_slice[i]);
            let font = &fonts_slice[i];

            let font_id = c_char_to_option_string(font.id).unwrap_or_default();
            let bytes = if font.bytes.is_null() || font.bytes_len == 0 {
                Vec::new()
            } else {
                slice::from_raw_parts(font.bytes, font.bytes_len).to_vec()
            };

            memory_fonts.push((
                pattern,
                FcFont {
                    bytes,
                    font_index: font.font_index,
                    id: font_id,
                },
            ));
        }

        cache.with_memory_fonts(memory_fonts);
    }
}

/// C-compatible representation of a resolved font run
#[repr(C)]
pub struct FcResolvedFontRunC {
    /// The text for this run
    pub text: *mut c_char,
    /// Start byte offset in original text
    pub start_byte: usize,
    /// End byte offset in original text
    pub end_byte: usize,
    /// Font ID for this run (or null if no font found)
    pub font_id: FcFontIdC,
    /// Whether font_id is valid
    pub has_font: bool,
    /// CSS source name
    pub css_source: *mut c_char,
}

/// C-compatible representation of a CSS fallback group
#[repr(C)]
pub struct FcCssFallbackGroupC {
    /// The CSS font name
    pub css_name: *mut c_char,
    /// Array of font matches
    pub fonts: *mut FcFontMatchNoFallbackC,
    /// Number of fonts
    pub fonts_count: usize,
}

/// C-compatible font fallback chain (opaque type)
pub struct FcFontFallbackChainC {
    inner: FontFallbackChain,
}

/// Resolve a font chain from CSS font families
/// 
/// This is the first step in the two-step font resolution process.
/// 
/// @param cache The font cache
/// @param families Array of CSS font family names (e.g., ["Arial", "sans-serif"])
/// @param families_count Number of family names
/// @param weight Font weight
/// @param italic Whether to match italic fonts
/// @param oblique Whether to match oblique fonts
/// @param trace Array to store trace messages
/// @param trace_count Pointer to trace count (will be updated)
/// @return Font fallback chain or NULL on error
#[no_mangle]
pub extern "C" fn fc_resolve_font_chain(
    cache: *const FcFontCache,
    families: *const *const c_char,
    families_count: usize,
    weight: FcWeight,
    italic: PatternMatch,
    oblique: PatternMatch,
    trace: *mut *mut FcTraceMsgC,
    trace_count: *mut usize,
) -> *mut FcFontFallbackChainC {
    if cache.is_null() || families.is_null() || trace.is_null() || trace_count.is_null() {
        return ptr::null_mut();
    }

    unsafe {
        let cache = &*cache;
        
        // Convert C string array to Vec<String>
        let families_rust = c_string_array_to_vec(families, families_count);

        let mut trace_msgs = Vec::new();
        let chain = cache.resolve_font_chain(&families_rust, weight, italic, oblique, &mut trace_msgs);

        // Convert trace messages
        let (trace_c, count) = trace_msgs_to_c(&trace_msgs);
        *trace = trace_c;
        *trace_count = count;

        Box::into_raw(Box::new(FcFontFallbackChainC { inner: chain }))
    }
}

/// Free a font fallback chain
#[no_mangle]
pub extern "C" fn fc_font_chain_free(chain: *mut FcFontFallbackChainC) {
    if !chain.is_null() {
        unsafe {
            let _ = Box::from_raw(chain);
        }
    }
}

/// Query which fonts should be used for a text string
/// 
/// This is the second step in the two-step font resolution process.
/// Returns runs of consecutive characters that use the same font.
/// 
/// @param chain The font fallback chain (from fc_resolve_font_chain)
/// @param cache The font cache
/// @param text The text to find fonts for
/// @param runs_count Pointer to store number of runs (will be updated)
/// @return Array of font runs or NULL on error (must be freed with fc_resolved_runs_free)
#[no_mangle]
pub extern "C" fn fc_chain_query_for_text(
    chain: *const FcFontFallbackChainC,
    cache: *const FcFontCache,
    text: *const c_char,
    runs_count: *mut usize,
) -> *mut FcResolvedFontRunC {
    if chain.is_null() || cache.is_null() || text.is_null() || runs_count.is_null() {
        return ptr::null_mut();
    }

    unsafe {
        let chain = &*chain;
        let cache = &*cache;
        let text_rust = CStr::from_ptr(text).to_string_lossy().into_owned();

        let runs = chain.inner.query_for_text(cache, &text_rust);

        if runs.is_empty() {
            *runs_count = 0;
            return ptr::null_mut();
        }

        let mut runs_c = Vec::with_capacity(runs.len());
        for run in &runs {
            let text_c = CString::new(run.text.as_str()).unwrap_or_default().into_raw();
            let css_source_c = CString::new(run.css_source.as_str()).unwrap_or_default().into_raw();
            
            let (font_id, has_font) = match &run.font_id {
                Some(id) => (FcFontIdC::from_fontid(id), true),
                None => (FcFontIdC { high: 0, low: 0 }, false),
            };

            runs_c.push(FcResolvedFontRunC {
                text: text_c,
                start_byte: run.start_byte,
                end_byte: run.end_byte,
                font_id,
                has_font,
                css_source: css_source_c,
            });
        }

        let (ptr, len) = vec_into_raw_parts(runs_c);
        *runs_count = len;

        ptr
    }
}

/// Free an array of resolved font runs
#[no_mangle]
pub extern "C" fn fc_resolved_runs_free(runs: *mut FcResolvedFontRunC, count: usize) {
    if runs.is_null() || count == 0 {
        return;
    }

    unsafe {
        let runs_slice = slice::from_raw_parts_mut(runs, count);

        for run in runs_slice {
            free_c_string(run.text);
            free_c_string(run.css_source);
        }

        free_raw_vec(runs, count);
    }
}

/// Get the original CSS font stack from a font chain
#[no_mangle]
pub extern "C" fn fc_chain_get_original_stack(
    chain: *const FcFontFallbackChainC,
    stack_count: *mut usize,
) -> *mut *mut c_char {
    if chain.is_null() || stack_count.is_null() {
        return ptr::null_mut();
    }

    unsafe {
        let chain = &*chain;
        let stack = &chain.inner.original_stack;

        if stack.is_empty() {
            *stack_count = 0;
            return ptr::null_mut();
        }

        let mut stack_c = Vec::with_capacity(stack.len());
        for name in stack {
            let name_c = CString::new(name.as_str()).unwrap_or_default().into_raw();
            stack_c.push(name_c);
        }

        let (ptr, len) = vec_into_raw_parts(stack_c);
        *stack_count = len;

        ptr
    }
}

/// Free a string array (from fc_chain_get_original_stack)
#[no_mangle]
pub extern "C" fn fc_string_array_free(arr: *mut *mut c_char, count: usize) {
    if arr.is_null() || count == 0 {
        return;
    }

    unsafe {
        let arr_slice = slice::from_raw_parts_mut(arr, count);
        for s in arr_slice {
            free_c_string(*s);
        }
        free_raw_vec(arr, count);
    }
}

/// Get CSS fallback groups from a font chain
#[no_mangle]
pub extern "C" fn fc_chain_get_css_fallbacks(
    chain: *const FcFontFallbackChainC,
    groups_count: *mut usize,
) -> *mut FcCssFallbackGroupC {
    if chain.is_null() || groups_count.is_null() {
        return ptr::null_mut();
    }

    unsafe {
        let chain = &*chain;
        let fallbacks = &chain.inner.css_fallbacks;

        if fallbacks.is_empty() {
            *groups_count = 0;
            return ptr::null_mut();
        }

        let mut groups_c = Vec::with_capacity(fallbacks.len());
        for group in fallbacks {
            let css_name = CString::new(group.css_name.as_str()).unwrap_or_default().into_raw();
            
            let fonts_count = group.fonts.len();
            let fonts = if fonts_count > 0 {
                let mut fonts_c = Vec::with_capacity(fonts_count);
                for font in &group.fonts {
                    let ranges_count = font.unicode_ranges.len();
                    let ranges = if ranges_count > 0 {
                        let ranges_vec: Vec<UnicodeRange> = font.unicode_ranges.clone();
                        let (ptr, _) = vec_into_raw_parts(ranges_vec);
                        ptr
                    } else {
                        ptr::null_mut()
                    };

                    fonts_c.push(FcFontMatchNoFallbackC {
                        id: FcFontIdC::from_fontid(&font.id),
                        unicode_ranges: ranges,
                        unicode_ranges_count: ranges_count,
                    });
                }
                let (ptr, _) = vec_into_raw_parts(fonts_c);
                ptr
            } else {
                ptr::null_mut()
            };

            groups_c.push(FcCssFallbackGroupC {
                css_name,
                fonts,
                fonts_count,
            });
        }

        let (ptr, len) = vec_into_raw_parts(groups_c);
        *groups_count = len;

        ptr
    }
}

/// Free CSS fallback groups
#[no_mangle]
pub extern "C" fn fc_css_fallback_groups_free(groups: *mut FcCssFallbackGroupC, count: usize) {
    if groups.is_null() || count == 0 {
        return;
    }

    unsafe {
        let groups_slice = slice::from_raw_parts_mut(groups, count);

        for group in groups_slice {
            free_c_string(group.css_name);

            if !group.fonts.is_null() && group.fonts_count > 0 {
                let fonts_slice = slice::from_raw_parts_mut(group.fonts, group.fonts_count);
                for font in fonts_slice {
                    free_raw_vec(font.unicode_ranges, font.unicode_ranges_count);
                }
                free_raw_vec(group.fonts, group.fonts_count);
            }
        }

        free_raw_vec(groups, count);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Registry (async/background thread) API
// ═══════════════════════════════════════════════════════════════════════════

/// Create a new font registry (returns immediately, no scanning yet).
#[cfg(feature = "async-registry")]
#[no_mangle]
pub extern "C" fn fc_registry_new() -> *mut Arc<FcFontRegistry> {
    let registry = FcFontRegistry::new();
    Box::into_raw(Box::new(registry))
}

/// Spawn the Scout thread and Builder pool. Returns immediately.
/// The scout enumerates font directories (~5-20ms), builders parse font files
/// in priority order in the background.
#[cfg(feature = "async-registry")]
#[no_mangle]
pub extern "C" fn fc_registry_spawn(registry: *const Arc<FcFontRegistry>) {
    if registry.is_null() {
        return;
    }
    unsafe {
        let registry = &*registry;
        registry.spawn_scout_and_builders();
    }
}

/// Block until the requested font families are loaded, then return
/// resolved font chains. Each element in `family_stacks` is a
/// null-terminated CSS font-family stack (array of C strings).
///
/// Returns an array of FcFontChain pointers (one per stack).
/// The caller must free each chain with fc_font_chain_free() and the
/// array itself with fc_registry_chains_free().
///
/// Hard timeout: 5 seconds.
#[cfg(feature = "async-registry")]
#[no_mangle]
pub extern "C" fn fc_registry_request_fonts(
    registry: *const Arc<FcFontRegistry>,
    family_stacks: *const *const *const c_char,
    stack_counts: *const usize,
    num_stacks: usize,
    out_count: *mut usize,
) -> *mut *mut FcFontFallbackChainC {
    if registry.is_null()
        || family_stacks.is_null()
        || stack_counts.is_null()
        || out_count.is_null()
        || num_stacks == 0
    {
        if !out_count.is_null() {
            unsafe { *out_count = 0; }
        }
        return ptr::null_mut();
    }

    unsafe {
        let registry = &*registry;
        let stacks_slice = slice::from_raw_parts(family_stacks, num_stacks);
        let counts_slice = slice::from_raw_parts(stack_counts, num_stacks);

        let mut rust_stacks: Vec<Vec<String>> = Vec::with_capacity(num_stacks);
        for i in 0..num_stacks {
            let count = counts_slice[i];
            let stack = c_string_array_to_vec(stacks_slice[i], count);
            rust_stacks.push(stack);
        }

        let chains = registry.request_fonts(&rust_stacks);

        let mut chain_ptrs: Vec<*mut FcFontFallbackChainC> = chains
            .into_iter()
            .map(|chain| {
                Box::into_raw(Box::new(FcFontFallbackChainC { inner: chain }))
            })
            .collect();
        // shrink_to_fit guarantees capacity == len, which free_raw_vec requires
        chain_ptrs.shrink_to_fit();

        let (ptr, len) = vec_into_raw_parts(chain_ptrs);
        *out_count = len;
        ptr
    }
}

/// Free the array returned by fc_registry_request_fonts.
/// Does NOT free the individual chains (use fc_font_chain_free for each).
#[cfg(feature = "async-registry")]
#[no_mangle]
pub extern "C" fn fc_registry_chains_free(
    chains: *mut *mut FcFontFallbackChainC,
    count: usize,
) {
    if chains.is_null() || count == 0 {
        return;
    }
    unsafe {
        free_raw_vec(chains, count);
    }
}

/// Check if the scout has finished enumerating all font directories.
#[cfg(feature = "async-registry")]
#[no_mangle]
pub extern "C" fn fc_registry_is_scan_complete(
    registry: *const Arc<FcFontRegistry>,
) -> bool {
    if registry.is_null() {
        return false;
    }
    unsafe { (*registry).is_scan_complete() }
}

/// Check if all queued font files have been parsed.
#[cfg(feature = "async-registry")]
#[no_mangle]
pub extern "C" fn fc_registry_is_build_complete(
    registry: *const Arc<FcFontRegistry>,
) -> bool {
    if registry.is_null() {
        return false;
    }
    unsafe { (*registry).is_build_complete() }
}

/// Signal all background threads to shut down.
#[cfg(feature = "async-registry")]
#[no_mangle]
pub extern "C" fn fc_registry_shutdown(registry: *const Arc<FcFontRegistry>) {
    if registry.is_null() {
        return;
    }
    unsafe {
        (*registry).shutdown();
    }
}

/// Free a font registry. Shuts down threads first if still running.
#[cfg(feature = "async-registry")]
#[no_mangle]
pub extern "C" fn fc_registry_free(registry: *mut Arc<FcFontRegistry>) {
    if !registry.is_null() {
        unsafe {
            let arc = Box::from_raw(registry);
            arc.shutdown();
            drop(arc);
        }
    }
}

/// Query a single font from the registry (thread-safe).
#[cfg(feature = "async-registry")]
#[no_mangle]
pub extern "C" fn fc_registry_query(
    registry: *const Arc<FcFontRegistry>,
    pattern: *const FcPatternC,
) -> *mut FcFontMatchC {
    if registry.is_null() || pattern.is_null() {
        return ptr::null_mut();
    }

    unsafe {
        let registry = &*registry;
        let pattern_rust = c_to_pattern(pattern);

        match registry.query(&pattern_rust) {
            Some(match_obj) => {
                let cache = registry.shared_cache();
                let match_c = font_match_to_c(&cache, &match_obj);
                Box::into_raw(Box::new(match_c))
            }
            None => ptr::null_mut(),
        }
    }
}

/// List all fonts currently loaded in the registry.
#[cfg(feature = "async-registry")]
#[no_mangle]
pub extern "C" fn fc_registry_list_fonts(
    registry: *const Arc<FcFontRegistry>,
    count: *mut usize,
) -> *mut FcFontInfoC {
    if registry.is_null() || count.is_null() {
        return ptr::null_mut();
    }

    unsafe {
        let registry = &*registry;
        let font_list = registry.list();

        if font_list.is_empty() {
            *count = 0;
            return ptr::null_mut();
        }

        let mut font_info = Vec::with_capacity(font_list.len());
        for (pattern, id) in &font_list {
            font_info.push(FcFontInfoC {
                id: FcFontIdC::from_fontid(id),
                name: option_string_to_c_char(pattern.name.as_ref()),
                family: option_string_to_c_char(pattern.family.as_ref()),
            });
        }

        let (ptr, len) = vec_into_raw_parts(font_info);
        *count = len;
        ptr
    }
}

/// Resolve a font chain from the registry (thread-safe, uses current state).
#[cfg(feature = "async-registry")]
#[no_mangle]
pub extern "C" fn fc_registry_resolve_font_chain(
    registry: *const Arc<FcFontRegistry>,
    families: *const *const c_char,
    families_count: usize,
    weight: FcWeight,
    italic: PatternMatch,
    oblique: PatternMatch,
) -> *mut FcFontFallbackChainC {
    if registry.is_null() || families.is_null() {
        return ptr::null_mut();
    }

    unsafe {
        let registry = &*registry;
        let families_rust = c_string_array_to_vec(families, families_count);

        let chain = registry.resolve_font_chain(&families_rust, weight, italic, oblique);
        Box::into_raw(Box::new(FcFontFallbackChainC { inner: chain }))
    }
}

/// Get font path by ID from the registry.
#[cfg(feature = "async-registry")]
#[no_mangle]
pub extern "C" fn fc_registry_get_font_path(
    registry: *const Arc<FcFontRegistry>,
    id: *const FcFontIdC,
) -> *mut FcFontPathC {
    if registry.is_null() || id.is_null() {
        return ptr::null_mut();
    }

    unsafe {
        let registry = &*registry;
        let id_rust = FontId::from_fontid_c(&*id);

        match registry.get_disk_font_path(&id_rust) {
            Some(path) => {
                let path_c = FcFontPathC {
                    path: CString::new(path.path.clone())
                        .unwrap_or_default()
                        .into_raw(),
                    font_index: path.font_index,
                };
                Box::into_raw(Box::new(path_c))
            }
            None => ptr::null_mut(),
        }
    }
}

/// Get font metadata by ID from the registry.
#[cfg(feature = "async-registry")]
#[no_mangle]
pub extern "C" fn fc_registry_get_metadata(
    registry: *const Arc<FcFontRegistry>,
    id: *const FcFontIdC,
) -> *mut FcFontMetadataC {
    if registry.is_null() || id.is_null() {
        return ptr::null_mut();
    }

    unsafe {
        let registry = &*registry;
        let id_rust = FontId::from_fontid_c(&*id);

        match registry.get_metadata_by_id(&id_rust) {
            Some(pattern) => {
                Box::into_raw(Box::new(metadata_to_c(&pattern.metadata)))
            }
            None => ptr::null_mut(),
        }
    }
}

/// Take a snapshot of the registry as an immutable FcFontCache.
/// Useful for passing to fc_chain_query_for_text().
#[cfg(feature = "async-registry")]
#[no_mangle]
pub extern "C" fn fc_registry_snapshot(
    registry: *const Arc<FcFontRegistry>,
) -> *mut FcFontCache {
    if registry.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        let registry = &*registry;
        let cache = registry.shared_cache();
        Box::into_raw(Box::new(cache))
    }
}
