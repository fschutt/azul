#![cfg(feature = "image_loading")]

use azul_core::app_resources::LoadedImageSource;
use alloc::vec::Vec;
use azul_core::app_resources::RawImageFormat;

pub use image_crate::{ImageError, DynamicImage, GenericImageView};

pub fn decode_image_data(image_data: &[u8]) -> Result<LoadedImageSource, ImageError> {
    let image_format = image_crate::guess_format(image_data)?;
    let decoded = image_crate::load_from_memory_with_format(image_data, image_format)?;
    Ok(prepare_image(decoded)?)
}

// The next three functions are taken from:
// https://github.com/christolliday/limn/blob/master/core/src/resources/image.rs

pub fn prepare_image(image_decoded: DynamicImage) -> Result<LoadedImageSource, ImageError> {
    use azul_core::app_resources::{
        ImageDescriptor, ImageDescriptorFlags, ImageData,
        is_image_opaque, premultiply
    };

    let image_dims = image_decoded.dimensions();

    // see: https://github.com/servo/webrender/blob/80c614ab660bf6cca52594d0e33a0be262a7ac12/wrench/src/yaml_frame_reader.rs#L401-L427
    let (format, bytes) = match image_decoded {
        image_crate::DynamicImage::ImageLuma8(bytes) => {
            let mut pixels = Vec::with_capacity(image_dims.0 as usize * image_dims.1 as usize * 4);
            for grey in bytes.into_iter() {
                pixels.extend_from_slice(&[
                    *grey,
                    *grey,
                    *grey,
                    0xff,
                ]);
            }
            (RawImageFormat::BGRA8, pixels)
        },
        image_crate::DynamicImage::ImageLumaA8(bytes) => {
            let mut pixels = Vec::with_capacity(image_dims.0 as usize * image_dims.1 as usize * 4);
            for greyscale_alpha in bytes.chunks_exact(2) {
                let grey = greyscale_alpha[0];
                let alpha = greyscale_alpha[1];
                pixels.extend_from_slice(&[
                    grey,
                    grey,
                    grey,
                    alpha,
                ]);
            }
            (RawImageFormat::BGRA8, pixels)
        },
        image_crate::DynamicImage::ImageRgb8(bytes) => {
            let mut pixels = Vec::with_capacity(image_dims.0 as usize * image_dims.1 as usize * 4);
            for rgb in bytes.chunks_exact(3) {
                pixels.extend_from_slice(&[
                    rgb[2], // b
                    rgb[1], // g
                    rgb[0], // r
                    0xff    // a
                ]);
            }
            (RawImageFormat::BGRA8, pixels)
        },
        image_crate::DynamicImage::ImageRgba8(bytes) => {
            let mut pixels = bytes.into_raw();
            // no extra allocation necessary, but swizzling
            for rgba in pixels.chunks_exact_mut(4) {
                let r = rgba[0];
                let g = rgba[1];
                let b = rgba[2];
                let a = rgba[3];
                rgba[0] = b;
                rgba[1] = r;
                rgba[2] = g;
                rgba[3] = a;
            }
            premultiply(pixels.as_mut_slice());
            (RawImageFormat::BGRA8, pixels)
        },
        image_crate::DynamicImage::ImageBgr8(bytes) => {
            let mut pixels = Vec::with_capacity(image_dims.0 as usize * image_dims.1 as usize * 4);
            for bgr in bytes.chunks_exact(3) {
                pixels.extend_from_slice(&[
                    bgr[0], // b
                    bgr[1], // g
                    bgr[2], // r
                    0xff    // a
                ]);
            }
            (RawImageFormat::BGRA8, pixels)
        },
        image_crate::DynamicImage::ImageBgra8(bytes) => {
            // Already in the correct format
            let mut pixels = bytes.into_raw();
            premultiply(pixels.as_mut_slice());
            (RawImageFormat::BGRA8, pixels)
        },
        image_crate::DynamicImage::ImageLuma16(bytes) => {
            let mut pixels = Vec::with_capacity(image_dims.0 as usize * image_dims.1 as usize * 4);
            for grey in bytes.into_iter() {
                pixels.extend_from_slice(&[
                    normalize_u16(*grey),
                    normalize_u16(*grey),
                    normalize_u16(*grey),
                    0xff,
                ]);
            }
            (RawImageFormat::BGRA8, pixels)
        },
        image_crate::DynamicImage::ImageLumaA16(bytes) => {
            let mut pixels = Vec::with_capacity(image_dims.0 as usize * image_dims.1 as usize * 4);
            for greyscale_alpha in bytes.chunks_exact(2) {
                let grey = greyscale_alpha[0];
                let alpha = greyscale_alpha[1];
                pixels.extend_from_slice(&[
                    normalize_u16(grey),
                    normalize_u16(grey),
                    normalize_u16(grey),
                    normalize_u16(alpha),
                ]);
            }
            (RawImageFormat::BGRA8, pixels)
        },
        image_crate::DynamicImage::ImageRgb16(bytes) => {
            let mut pixels = Vec::with_capacity(image_dims.0 as usize * image_dims.1 as usize * 4);
            for rgb in bytes.chunks_exact(3) {
                pixels.extend_from_slice(&[
                    normalize_u16(rgb[2]), // b
                    normalize_u16(rgb[1]), // g
                    normalize_u16(rgb[0]), // r
                    0xff    // a
                ]);
            }
            (RawImageFormat::BGRA8, pixels)
        },
        image_crate::DynamicImage::ImageRgba16(bytes) => {
            let mut pixels = Vec::with_capacity(image_dims.0 as usize * image_dims.1 as usize * 4);
            for rgba in bytes.chunks_exact(4) {
                let r = rgba[0];
                let g = rgba[1];
                let b = rgba[2];
                let a = rgba[3];
                pixels.extend_from_slice(&[
                    normalize_u16(b),
                    normalize_u16(g),
                    normalize_u16(r),
                    normalize_u16(a),
                ]);
            }
            premultiply(pixels.as_mut_slice());
            (RawImageFormat::BGRA8, pixels)
        },
    };

    let is_opaque = is_image_opaque(format, &bytes[..]);
    let allow_mipmaps = true;
    let descriptor = ImageDescriptor {
        format,
        width: image_dims.0 as usize,
        height: image_dims.1 as usize,
        offset: 0,
        stride: None.into(),
        flags: ImageDescriptorFlags {
            is_opaque,
            allow_mipmaps,
        }
    };
    let data = ImageData::Raw(bytes.into());

    Ok(LoadedImageSource { image_bytes_decoded: data, image_descriptor: descriptor })
}

#[inline]
pub fn normalize_u16(i: u16) -> u8 {
    ((65535.0 / i as f32) * 255.0) as u8
}

const fn translate_rawimage_colortype(i: RawImageFormat) -> image_crate::ColorType {
    match i {
        RawImageFormat::R8 => image_crate::ColorType::L8,
        RawImageFormat::RG8 => image_crate::ColorType::La8,
        RawImageFormat::RGB8 => image_crate::ColorType::Rgb8,
        RawImageFormat::RGBA8 => image_crate::ColorType::Rgba8,
        RawImageFormat::R16 => image_crate::ColorType::L16,
        RawImageFormat::RG16 => image_crate::ColorType::La16,
        RawImageFormat::RGB16 => image_crate::ColorType::Rgb16,
        RawImageFormat::RGBA16 => image_crate::ColorType::Rgba16,
        RawImageFormat::BGR8 => image_crate::ColorType::Bgr8,
        RawImageFormat::BGRA8 => image_crate::ColorType::Bgra8,
    }
}

#[cfg(feature = "std")]
pub mod encode {

    use super::translate_rawimage_colortype;
    use image_crate::codecs::{
        bmp::BmpEncoder,
        png::PngEncoder,
        jpeg::JpegEncoder,
        tga::TgaEncoder,
        gif::GifEncoder,
        dxt::DxtEncoder,
        pnm::PnmEncoder,
        tiff::TiffEncoder,
        hdr::HdrEncoder,
    };
    use azul_css::U8Vec;
    use image_crate::error::ImageError;
    use image::error::LimitError;
    use image::error::LimitErrorKind;
    use std::io::Cursor;
    use azul_core::app_resources::RawImage;

    #[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
    #[repr(C)]
    pub enum EncodeImageError {
        InsufficientMemory,
        DimensionError,
        Unknown,
    }

    fn translate_image_error_encode(i: ImageError) -> EncodeImageError {
        match i {
            ImageError::Limits(l) => match l.kind() {
                LimitErrorKind::InsufficientMemory => EncodeImageError::InsufficientMemory,
                LimitErrorKind::DimensionError => EncodeImageError::DimensionError,
                _ => EncodeImageError::Unknown,
            },
            _ => EncodeImageError::Unknown,
        }
    }

    impl_result!(U8Vec, EncodeImageError, ResultU8VecEncodeImageError, copy = false, [Debug, Clone]);

    macro_rules! encode_func {($func:ident, $encoder:ident) => (
        pub fn $func(image: &RawImage) -> ResultU8VecEncodeImageError {
            let mut result = Vec::<u8>::new();

            {
                let mut cursor = Cursor::new(&mut result);
                let mut encoder = $encoder::new(&mut cursor);
                if let Err(e) = encoder.encode(
                    image.pixels.as_ref(),
                    image.width as u32,
                    image.height as u32,
                    translate_rawimage_colortype(image.data_format),
                ) {
                    return ResultU8VecEncodeImageError::Err(translate_image_error_encode(e));
                }
            }

            ResultU8VecEncodeImageError::Ok(result.into())
        }
    )}

    encode_func!(encode_bmp, BmpEncoder);
    encode_func!(encode_png, PngEncoder);
    encode_func!(encode_jpeg, JpegEncoder);
    encode_func!(encode_tga, TgaEncoder);
    encode_func!(encode_tiff, TiffEncoder);
    encode_func!(encode_gif, GifEncoder);
    encode_func!(encode_pnm, PnmEncoder);
}