#![cfg(feature = "image_loading")]

use azul_core::app_resources::LoadedImageSource;
pub use image_crate::{ImageError, DynamicImage, GenericImageView};

pub fn decode_image_data(image_data: Vec<u8>) -> Result<LoadedImageSource, ImageError> {
    let image_format = image_crate::guess_format(&image_data)?;
    let decoded = image_crate::load_from_memory_with_format(&image_data, image_format)?;
    Ok(prepare_image(decoded)?)
}

// The next three functions are taken from:
// https://github.com/christolliday/limn/blob/master/core/src/resources/image.rs

pub fn prepare_image(image_decoded: DynamicImage) -> Result<LoadedImageSource, ImageError> {
    use azul_core::app_resources::{
        RawImageFormat, ImageDescriptor, ImageDescriptorFlags, ImageData,
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
        dimensions: (image_dims.0 as usize, image_dims.1 as usize),
        offset: 0,
        stride: None,
        flags: ImageDescriptorFlags {
            is_opaque,
            allow_mipmaps,
        }
    };
    let data = ImageData::new_raw(bytes);

    Ok(LoadedImageSource { image_bytes_decoded: data, image_descriptor: descriptor })
}

#[inline]
pub fn normalize_u16(i: u16) -> u8 {
    ((65535.0 / i as f32) * 255.0) as u8
}