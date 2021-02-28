#![cfg(feature = "std")]
#![cfg_attr(not(feature = "std"), no_std)]

use std::{
    path::PathBuf,
    io::Error as IoError,
};
use rust_fontconfig::FcFontCache;
use azul_core::app_resources::FontSource;
#[cfg(feature = "text_layout")]
use azul_core::app_resources::{LoadedFontSource, OptionLoadedFontSource};
use azul_css::{U8Vec, StringVec};

const DEFAULT_FONT_INDEX: i32 = 0;

#[derive(Debug)]
pub enum FontReloadError {
    Io(IoError, PathBuf),
    FontNotFound(StringVec),
    FontLoadingNotActive(StringVec),
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
    Io(err, path_buf) => format!("Could not load \"{}\" - IO error: {}", path_buf.as_path().to_string_lossy(), err),
    FontNotFound(id) => format!("Could not locate system font: \"{:?}\" found", id),
    FontLoadingNotActive(id) => format!("Could not load system font: \"{:?}\": crate was not compiled with --features=\"font_loading\"", id)
});

pub extern "C" fn font_source_get_bytes(font_source: &FontSource, fc_cache: &FcFontCache) -> OptionLoadedFontSource {
    // TODO: logging!
    let (font_bytes, font_index, parse_glyph_outlines) = match font_source_get_bytes_inner(font_source, fc_cache).ok() {
        Some(s) => s,
        None => { return OptionLoadedFontSource::None; },
    };
    Some(LoadedFontSource{ font_bytes: font_bytes, font_index: font_index as u32, parse_glyph_outlines }).into()
}

/// Returns the bytes of the font (loads the font from the system in case it is a `FontSource::System` font).
/// Also returns the index into the font (in case the font is a font collection).
pub fn font_source_get_bytes_inner(font_source: &FontSource, fc_cache: &FcFontCache) -> Result<(U8Vec, i32, bool), FontReloadError> {

    match font_source {
        FontSource::Embedded(embedded_font) => Ok((embedded_font.font_data.clone(), DEFAULT_FONT_INDEX, embedded_font.load_glyph_outlines)),
        FontSource::File(file_font) => {
            let file_path: String = file_font.file_path.clone().into_library_owned_string();
            let file_path = PathBuf::from(file_path);
            std::fs::read(&file_path)
            .map_err(|e| FontReloadError::Io(e, file_path.clone()))
            .map(|font_bytes| (font_bytes.into(), DEFAULT_FONT_INDEX, file_font.load_glyph_outlines))
        },
        FontSource::System(system_fonts) => {
            #[cfg(feature = "font_loading")] {
                crate::font::load_system_fonts(system_fonts.names.as_ref(), fc_cache)
                .map(|(font_bytes, font_index)| (font_bytes, font_index, system_fonts.load_glyph_outlines))
                .ok_or(FontReloadError::FontNotFound(system_fonts.names.clone()))
            }
            #[cfg(not(feature = "font_loading"))] {
                Err(FontReloadError::FontLoadingNotActive(system_fonts.names.clone()))
            }
        },
    }
}