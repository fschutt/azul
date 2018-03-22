//! Module for loading and handling fonts

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum FontError {
    /// Font failed to upload to the GPU
    UploadError,
    /// Rusttype failed to parse the font
    RusttypeError,
}
