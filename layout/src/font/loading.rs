#![cfg(feature = "std")]
#![cfg(feature = "font_loading")]
#![cfg_attr(not(feature = "std"), no_std)]

use std::io::Error as IoError;

use azul_css::{AzString, StringVec, U8Vec};
use rust_fontconfig::FcFontCache;

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
