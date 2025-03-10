
use core::fmt;

use azul_core::app_resources::{RawImage, RawImageFormat};
use azul_css::U8Vec;
use image::{
    DynamicImage,
    error::{ImageError, LimitError, LimitErrorKind},
};
use azul_css::{impl_result, impl_result_inner};

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
#[repr(C)]
pub enum DecodeImageError {
    InsufficientMemory,
    DimensionError,
    UnsupportedImageFormat,
    Unknown,
}

impl fmt::Display for DecodeImageError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DecodeImageError::InsufficientMemory => write!(
                f,
                "Error decoding image: Not enough memory available to perform encoding \
                 operation"
            ),
            DecodeImageError::DimensionError => {
                write!(f, "Error decoding image: Wrong dimensions")
            }
            DecodeImageError::UnsupportedImageFormat => {
                write!(f, "Error decoding image: Invalid data format")
            }
            DecodeImageError::Unknown => write!(f, "Error decoding image: Unknown error"),
        }
    }
}

fn translate_image_error_decode(i: ImageError) -> DecodeImageError {
    match i {
        ImageError::Limits(l) => match l.kind() {
            LimitErrorKind::InsufficientMemory => DecodeImageError::InsufficientMemory,
            LimitErrorKind::DimensionError => DecodeImageError::DimensionError,
            _ => DecodeImageError::Unknown,
        },
        _ => DecodeImageError::Unknown,
    }
}

impl_result!(
    RawImage,
    DecodeImageError,
    ResultRawImageDecodeImageError,
    copy = false,
    [Debug, Clone]
);

pub fn decode_raw_image_from_any_bytes(image_bytes: &[u8]) -> ResultRawImageDecodeImageError {
    use azul_core::app_resources::RawImageData;

    let image_format = match image::guess_format(image_bytes) {
        Ok(o) => o,
        Err(e) => {
            return ResultRawImageDecodeImageError::Err(translate_image_error_decode(e));
        }
    };

    let decoded = match image::load_from_memory_with_format(image_bytes, image_format) {
        Ok(o) => o,
        Err(e) => {
            return ResultRawImageDecodeImageError::Err(translate_image_error_decode(e));
        }
    };

    let ((width, height), data_format, pixels) = match decoded {
        DynamicImage::ImageLuma8(i) => (
            i.dimensions(),
            RawImageFormat::R8,
            RawImageData::U8(i.into_vec().into()),
        ),
        DynamicImage::ImageLumaA8(i) => (
            i.dimensions(),
            RawImageFormat::RG8,
            RawImageData::U8(i.into_vec().into()),
        ),
        DynamicImage::ImageRgb8(i) => (
            i.dimensions(),
            RawImageFormat::RGB8,
            RawImageData::U8(i.into_vec().into()),
        ),
        DynamicImage::ImageRgba8(i) => (
            i.dimensions(),
            RawImageFormat::RGBA8,
            RawImageData::U8(i.into_vec().into()),
        ),
        DynamicImage::ImageLuma16(i) => (
            i.dimensions(),
            RawImageFormat::R16,
            RawImageData::U16(i.into_vec().into()),
        ),
        DynamicImage::ImageLumaA16(i) => (
            i.dimensions(),
            RawImageFormat::RG16,
            RawImageData::U16(i.into_vec().into()),
        ),
        DynamicImage::ImageRgb16(i) => (
            i.dimensions(),
            RawImageFormat::RGB16,
            RawImageData::U16(i.into_vec().into()),
        ),
        DynamicImage::ImageRgba16(i) => (
            i.dimensions(),
            RawImageFormat::RGBA16,
            RawImageData::U16(i.into_vec().into()),
        ),
        DynamicImage::ImageRgb32F(i) => (
            i.dimensions(),
            RawImageFormat::RGBF32,
            RawImageData::F32(i.into_vec().into()),
        ),
        DynamicImage::ImageRgba32F(i) => (
            i.dimensions(),
            RawImageFormat::RGBAF32,
            RawImageData::F32(i.into_vec().into()),
        ),
        _ => {
            return ResultRawImageDecodeImageError::Err(DecodeImageError::Unknown);
        }
    };

    ResultRawImageDecodeImageError::Ok(RawImage {
        tag: Vec::new().into(),
        pixels,
        width: width as usize,
        height: height as usize,
        premultiplied_alpha: false,
        data_format,
    })
}