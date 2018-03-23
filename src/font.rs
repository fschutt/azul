//! Module for loading and handling fonts
use webrender::api::FontKey;
use rusttype::{Font, FontCollection};
use rusttype::Error as RusttypeError;

#[derive(Debug, Clone)]
pub(crate) enum FontState {
    // Font is available for the renderer
    Uploaded(FontKey),
    // Raw bytes for the font, to be uploaded in the next
    // draw call (for webrenders add_raw_font function)
    ReadyForUpload(Vec<u8>),
    AboutToBeDeleted()
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

impl From<RusttypeError> for FontError {
    fn from(e: RusttypeError) -> Self {
        FontError::ParseError(e)
    }
}

/// Read font data to get font information, v_metrics, glyph info etc.
pub(crate) fn rusttype_load_font<'a>(data: Vec<u8>) -> Result<Font<'a>, FontError> {
    let collection = FontCollection::from_bytes(data)?;
    let font = collection.clone().into_font().unwrap_or(collection.font_at(0)?);
    Ok(font)
}

//     font key, font_instance_key, size in app_units::Au
// 
//     let instance_key = render_api.generate_font_instance_key();
//     resources.add_font_instance(instance_key, font_key, size, None, None, Vec::new());