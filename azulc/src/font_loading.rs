#![cfg(feature = "std")]
#![cfg(feature = "font_loading")]
#![cfg_attr(not(feature = "std"), no_std)]

use std::{
    path::PathBuf,
    io::Error as IoError,
};
use azul_core::app_resources::LoadedFontSource;
use rust_fontconfig::FcFontCache;
use azul_css::{
    U8Vec, FontRef, StyleFontFamily,
    AzString, StringVec
};

const DEFAULT_FONT_INDEX: i32 = 0;

pub fn build_font_cache() -> FcFontCache {
    FcFontCache::build()
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

impl_display!(FontReloadError, {
    Io(err, path_buf) => format!("Could not load \"{}\" - IO error: {}", path_buf.as_str(), err),
    FontNotFound(id) => format!("Could not locate system font: \"{:?}\" found", id),
    FontLoadingNotActive(id) => format!("Could not load system font: \"{:?}\": crate was not compiled with --features=\"font_loading\"", id)
});

/// Returns the bytes of the font (loads the font from the system in case it is a `FontSource::System` font).
/// Also returns the index into the font (in case the font is a font collection).
pub fn font_source_get_bytes(font_family: &StyleFontFamily, fc_cache: &FcFontCache) -> Option<LoadedFontSource> {

    use azul_css::StyleFontFamily::*;

    let (font_bytes, font_index) = match font_family {
        Native(id) => {
            #[cfg(feature = "font_loading")] {
                crate::font::load_system_font(id.as_str(), fc_cache)
                .map(|(font_bytes, font_index)| (font_bytes, font_index))
                .ok_or(FontReloadError::FontNotFound(id.clone()))
            }
            #[cfg(not(feature = "font_loading"))] {
                Err(FontReloadError::FontLoadingNotActive(id.clone()))
            }
        },
        File(path) => {
            std::fs::read(path.as_str())
            .map_err(|e| FontReloadError::Io(e, path.clone()))
            .map(|font_bytes| (font_bytes.into(), DEFAULT_FONT_INDEX))
        },
        Ref(r) => {
            // NOTE: this path should never execute
            Ok((r.get_data().bytes.clone(), DEFAULT_FONT_INDEX))
        }
    }.ok()?;

    Some(LoadedFontSource {
        data: font_bytes,
        index: font_index.max(0) as u32,
        // only fonts added via FontRef can load glyph outlines!
        load_outlines: false,
    })
}