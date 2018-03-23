//! Module for loading and handling images

use webrender::api::ImageFormat as WebrenderImageFormat;
use image::{ImageResult, ImageFormat, guess_format};
use image::{self, ImageError, DynamicImage, GenericImage};
use webrender::api::{ImageData, ImageDescriptor, ImageKey};

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

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ImageInfo {
    pub(crate) key: ImageKey,
    pub(crate) descriptor: ImageDescriptor,
}

#[derive(Debug, Clone)]
pub(crate) enum ImageState {
    // resource is available for the renderer
    Uploaded(ImageInfo),
    // image is loaded & decoded, but not yet available
    ReadyForUpload((ImageData, ImageDescriptor)),
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

// The next three functions are taken from: 
// https://github.com/christolliday/limn/blob/master/core/src/resources/image.rs

use std::path::Path;

/// Convenience function to get the image type from a path
/// 
/// This function looks at the extension of the image. However, this
/// extension could be wrong, i.e. a user labeling a PNG as a JPG and so on.
/// If you don't know the format of the image, simply use Image::GuessImageType
/// - which will guess the type of the image from the magic header in the 
/// actual image data.
pub fn get_image_type_from_extension(path: &Path) -> Option<ImageType> {
    let ext = path.extension().and_then(|s| s.to_str())
                  .map_or(String::new(), |s| s.to_ascii_lowercase());

    match &ext[..] {
        "jpg" |
        "jpeg" => Some(ImageType::Jpeg),
        "png"  => Some(ImageType::Png),
        "gif"  => Some(ImageType::Gif),
        "webp" => Some(ImageType::WebP),
        "tif" |
        "tiff" => Some(ImageType::Tiff),
        "tga" => Some(ImageType::Tga),
        "bmp" => Some(ImageType::Bmp),
        "ico" => Some(ImageType::Ico),
        "hdr" => Some(ImageType::Hdr),
        "pbm" |
        "pam" |
        "ppm" |
        "pgm" => Some(ImageType::Pnm),
        _ => None,
    }
}

pub(crate) fn prepare_image(image_decoded: DynamicImage) 
    -> Result<(ImageData, ImageDescriptor), ImageError> 
{
    let image_dims = image_decoded.dimensions();

    // see: https://github.com/servo/webrender/blob/80c614ab660bf6cca52594d0e33a0be262a7ac12/wrench/src/yaml_frame_reader.rs#L401-L427
    let (format, bytes) = match image_decoded {
        image::ImageLuma8(_) => {
            (WebrenderImageFormat::R8, image_decoded.raw_pixels())
        },
        image::ImageLumaA8(_) => {
            let bytes = image_decoded.raw_pixels();
            let mut pixels = Vec::with_capacity(image_dims.0 as usize * image_dims.1 as usize * 4);
            for greyscale_alpha in bytes.chunks(2) {
                pixels.extend_from_slice(&[
                    greyscale_alpha[0],
                    greyscale_alpha[0],
                    greyscale_alpha[0],
                    greyscale_alpha[1]
                ]);
            }
            // TODO: necessary for greyscale?
            premultiply(pixels.as_mut_slice());
            (WebrenderImageFormat::BGRA8, pixels)
        },
        image::ImageRgba8(_) => {
            let mut pixels = image_decoded.raw_pixels();
            premultiply(pixels.as_mut_slice());
            (WebrenderImageFormat::BGRA8, pixels)
        },
        image::ImageRgb8(_) => {
            let bytes = image_decoded.raw_pixels();
            let mut pixels = Vec::with_capacity(image_dims.0 as usize * image_dims.1 as usize * 4);
            for bgr in bytes.chunks(3) {
                pixels.extend_from_slice(&[
                    bgr[2],
                    bgr[1],
                    bgr[0],
                    0xff
                ]);
            }
            (WebrenderImageFormat::BGRA8, pixels)
        }
    };

    let opaque = is_image_opaque(format, &bytes[..]);
    let allow_mipmaps = true;
    let descriptor = ImageDescriptor::new(image_dims.0, image_dims.1, format, opaque, allow_mipmaps);
    let data = ImageData::new(bytes);
    Ok((data, descriptor))
}

pub(crate) fn is_image_opaque(format: WebrenderImageFormat, bytes: &[u8]) -> bool {
    match format {
        WebrenderImageFormat::BGRA8 => {
            let mut is_opaque = true;
            for i in 0..(bytes.len() / 4) {
                if bytes[i * 4 + 3] != 255 {
                    is_opaque = false;
                    break;
                }
            }
            is_opaque
        }
        WebrenderImageFormat::R8 => true,
        _ => unreachable!(),
    }
}

// From webrender/wrench
// These are slow. Gecko's gfx/2d/Swizzle.cpp has better versions
// This function also converts from RGBA8 to BRGA8
pub(crate) fn premultiply(data: &mut [u8]) {
    for pixel in data.chunks_mut(4) {
        let a = u32::from(pixel[3]);
        let r = u32::from(pixel[2]);
        let g = u32::from(pixel[1]);
        let b = u32::from(pixel[0]);

        pixel[3] = a as u8;
        pixel[2] = ((r * a + 128) / 255) as u8;
        pixel[1] = ((g * a + 128) / 255) as u8;
        pixel[0] = ((b * a + 128) / 255) as u8;
    }
}