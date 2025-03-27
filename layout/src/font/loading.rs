#![cfg(feature = "std")]
#![cfg(feature = "font_loading")]
#![cfg_attr(not(feature = "std"), no_std)]

use std::io::Error as IoError;

use azul_core::app_resources::LoadedFontSource;
use azul_css::{AzString, FontRef, StringVec, StyleFontFamily, U8Vec};
use rust_fontconfig::FcFontCache;

const DEFAULT_FONT_INDEX: i32 = 0;

#[cfg(not(miri))]
pub fn build_font_cache() -> FcFontCache {
    FcFontCache::build()
}

#[cfg(miri)]
pub fn build_font_cache() -> FcFontCache {
    FcFontCache::default()
}

#[derive(Debug)]
pub enum FontReloadError {
    Io(IoError, AzString),
    FontNotFound(AzString),
    FontLoadingNotActive(AzString),
}

impl Clone for FontReloadError {
    fn clone(&self) -> Self {
        use self::FontReloadError::*;
        match self {
            Io(err, path) => Io(IoError::new(err.kind(), "Io Error"), path.clone()),
            FontNotFound(id) => FontNotFound(id.clone()),
            FontLoadingNotActive(id) => FontLoadingNotActive(id.clone()),
        }
    }
}

azul_core::impl_display!(FontReloadError, {
    Io(err, path_buf) => format!("Could not load \"{}\" - IO error: {}", path_buf.as_str(), err),
    FontNotFound(id) => format!("Could not locate system font: \"{:?}\" found", id),
    FontLoadingNotActive(id) => format!("Could not load system font: \"{:?}\": crate was not compiled with --features=\"font_loading\"", id)
});

/// Same as `font_source_get_bytes` but sets the `load_outlines` on the font source
pub fn font_source_get_bytes_load_outlines(
    font_family: &StyleFontFamily,
    fc_cache: &FcFontCache,
) -> Option<LoadedFontSource> {
    let mut f = font_source_get_bytes(font_family, fc_cache)?;
    f.load_outlines = true;
    Some(f)
}

/// Returns the bytes of the font (loads the font from the system in case it is a
/// `FontSource::System` font). Also returns the index into the font (in case the font is a font
/// collection).
pub fn font_source_get_bytes(
    font_family: &StyleFontFamily,
    fc_cache: &FcFontCache,
) -> Option<LoadedFontSource> {
    use azul_css::StyleFontFamily::*;

    let (font_bytes, font_index) = match font_family {
        System(id) => {
            #[cfg(feature = "font_loading")]
            {
                crate::font::load_system_font(id.as_str(), fc_cache)
                    .map(|(font_bytes, font_index)| (font_bytes, font_index))
                    .ok_or(FontReloadError::FontNotFound(id.clone()))
            }
            #[cfg(not(feature = "font_loading"))]
            {
                Err(FontReloadError::FontLoadingNotActive(id.clone()))
            }
        }
        File(path) => std::fs::read(path.as_str())
            .map_err(|e| FontReloadError::Io(e, path.clone()))
            .map(|font_bytes| (font_bytes.into(), DEFAULT_FONT_INDEX)),
        Ref(r) => {
            // NOTE: this path should never execute
            Ok((r.get_data().bytes.clone(), DEFAULT_FONT_INDEX))
        }
    }
    .ok()?;

    Some(LoadedFontSource {
        data: font_bytes,
        index: font_index.max(0) as u32,
        // only fonts added via FontRef can load glyph outlines!
        load_outlines: false,
    })
}
