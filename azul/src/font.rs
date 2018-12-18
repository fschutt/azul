//! Module for loading and handling fonts
use webrender::api::FontKey;
use rusttype::{Error as RusttypeError, Font, FontCollection};
use azul_css::FontId;

pub(crate) enum FontResourceUpdate {
    /// Raw bytes for the font should be uploaded to the rendering engine, used in Webrender's
    /// add_raw_font function
    Upload(FontId, Font<'static>, Vec<u8>),
    /// Font should be deleted
    /// We need both the ID (to delete the bytes of the font)
    /// as well as the FontKey to delete all the font instances
    Delete(FontId, Option<FontKey>),
}

#[derive(Debug)]
pub enum FontError {
    /// Font failed to upload to the GPU
    UploadError,
    ///
    InvalidFormat,
    /// Rusttype failed to parse the font
    ParseError(RusttypeError),
    /// IO error
    IoError(::std::io::Error),
}

impl_display!{ FontError, {
    UploadError => "Font failed to upload to the GPU",
    InvalidFormat => "Invalid format",
    ParseError(e) => format!("Rusttype failed to parse the font: {}", e),
    IoError(e) => format!("IO error: {}", e),
}}

impl From<RusttypeError> for FontError {
    fn from(e: RusttypeError) -> Self {
        FontError::ParseError(e)
    }
}

/// Read font data to get font information, v_metrics, glyph info etc.
pub fn rusttype_load_font(data: &Vec<u8>, index: Option<i32>) -> Result<Font<'static>, FontError> {
    let collection = FontCollection::from_bytes(data.clone())?;
    let font = collection.clone().into_font().unwrap_or(collection.font_at(index.and_then(|i| Some(i as usize)).unwrap_or(0))?);
    Ok(font)
}
