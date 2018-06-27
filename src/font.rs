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
    /// Font that is about to be deleted
    /// We need both the ID (to delete the bytes of the font)
    /// as well as the FontKey to delete all the font instances
    AboutToBeDeleted(Option<FontKey>),
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

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_font_file() {

}