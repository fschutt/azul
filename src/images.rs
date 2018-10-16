//! Module for loading and handling images

use std::{
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};
use webrender::api::{
    ImageFormat as WebrenderImageFormat,
    ImageData, ImageDescriptor, ImageKey
};
#[cfg(feature = "image_loading")]
use image::{
    self, ImageResult, ImageFormat,
    ImageError, DynamicImage, GenericImage,
};

static IMAGE_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageId {
    id: usize,
}

pub(crate) fn new_image_id() -> ImageId {
    let unique_id =IMAGE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    ImageId {
        id: unique_id,
    }
}

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
    // Image is about to get deleted in the next frame
    AboutToBeDeleted((Option<ImageKey>, ImageDescriptor)),
}

impl ImageState {
    /// Returns the original dimensions of the image
    pub fn get_dimensions(&self) -> (f32, f32) {
        use self::ImageState::*;
        match self {
            Uploaded(ImageInfo { descriptor, .. }) |
            ReadyForUpload((_, descriptor)) |
            AboutToBeDeleted((_, descriptor)) => (descriptor.size.width as f32, descriptor.size.height as f32)
        }
    }
}

impl ImageType {

    #[cfg(feature = "image_loading")]
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
                image::guess_format(data)
            }
        }
    }
}

// The next three functions are taken from:
// https://github.com/christolliday/limn/blob/master/core/src/resources/image.rs

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

#[cfg(feature = "image_loading")]
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
pub(crate) fn premultiply(data: &mut [u8]) {
    for pixel in data.chunks_mut(4) {
        let a = u32::from(pixel[3]);
        pixel[0] = (((pixel[0] as u32 * a) + 128) / 255) as u8;
        pixel[1] = (((pixel[1] as u32 * a) + 128) / 255) as u8;
        pixel[2] = (((pixel[2] as u32 * a) + 128) / 255) as u8;
    }
}

#[test]
fn test_premultiply() {
    let mut color = [255, 0, 0, 127];
    premultiply(&mut color);
    assert_eq!(color, [127, 0, 0, 127]);
}