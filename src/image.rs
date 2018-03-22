//! Module for loading and handling images

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ImageError {
    DecodingFailed,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ImageType {
    Bmp,
    Gif,
    Hdr,
    Ico,
    Jpeg,
    Png,
    Ppm,
    Pbm,
    Pgm,
    Pam,
    Tga,
    Tiff,
    WebP,
    /// Try to guess the image format, unknown data 
    GuessImageFormat,
}