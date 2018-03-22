//! Module for loading and handling images

use image::{ImageResult, ImageFormat, guess_format};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ImageType {
    Bmp,
    Gif,
    Hdr,
    Ico,
    Jpeg,
    Png,
    Pnm,
    Tga,
    Tiff,
    WebP,
    /// Try to guess the image format, unknown data 
    GuessImageFormat,
}

impl ImageType {
    pub(crate) fn into_image_format(&self, data: &[u8]) -> ImageResult<ImageFormat> {
        use self::ImageType::*;
        match *self {
            Bmp => Ok(ImageFormat::BMP),
            Gif => Ok(ImageFormat::GIF),
            Hdr => Ok(ImageFormat::HDR),
            Ico => Ok(ImageFormat::ICO),
            Jpeg => Ok(ImageFormat::JPEG),
            Png => Ok(ImageFormat::PNG),
            Pnm => Ok(ImageFormat::PNM),
            Tga => Ok(ImageFormat::TGA),
            Tiff => Ok(ImageFormat::TIFF),
            WebP => Ok(ImageFormat::WEBP),
            GuessImageFormat => {
                guess_format(data)
            }
        }
    }
}