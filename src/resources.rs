use std::sync::atomic::{AtomicUsize, Ordering};
use webrender::api::{ImageKey, FontKey};
use FastHashMap;
use std::io::Read;
use images::ImageType;
use image::{self, ImageError, DynamicImage, GenericImage};
use webrender::api::{ImageData, ImageDescriptor, ImageFormat};

static LAST_FONT_ID: AtomicUsize = AtomicUsize::new(0);
static LAST_IMAGE_ID: AtomicUsize = AtomicUsize::new(0);

/// Font and image keys
/// 
/// The idea is that azul doesn't know where the resources come from,
/// whether they are loaded from the network or a disk.
/// Fonts and images must be added and removed dynamically. If you have a 
/// fonts that should be always accessible, then simply add them before the app
/// starts up. 
///
/// Images and fonts can be references across window contexts 
/// (not yet tested, but should work).
#[derive(Debug, Default, Clone)]
pub(crate) struct AppResources {
    pub(crate) images: FastHashMap<String, ImageState>,
    pub(crate) fonts: FastHashMap<String, FastHashMap<FontSize, FontKey>>,
}

#[derive(Debug, Clone)]
pub(crate) enum ImageState {
    // resource is available for the renderer
    Uploaded(ImageKey),
    // image is loaded & decoded, but not yet available
    ReadyForUpload((ImageData, ImageDescriptor)),
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct FontSize(pub(crate) usize);

impl AppResources {

    /// See `AppState::add_image()`
    pub fn add_image<S: Into<String>, R: Read>(&mut self, id: S, data: &mut R, image_type: ImageType) 
        -> Result<Option<()>, ImageError>
    {
        use std::collections::hash_map::Entry::*;

        match self.images.entry(id.into()) {
            Occupied(_) => Ok(None),
            Vacant(v) => {
                let mut image_data = Vec::<u8>::new();
                data.read_to_end(&mut image_data).map_err(|e| ImageError::IoError(e))?;
                let image_format = image_type.into_image_format(&image_data)?;
                let decoded = image::load_from_memory_with_format(&image_data, image_format)?;
                v.insert(ImageState::ReadyForUpload(prepare_image(decoded)?));
                Ok(Some(()))
            },
        }
    }

    /// See `AppState::remove_image()`
    pub fn remove_image<S: Into<String>>(&mut self, id: S) 
        -> Option<()> 
    {
        Some(())
    }

    /// See `AppState::has_image()`
    pub fn has_image<S: Into<String>>(&mut self, id: S) 
        -> bool 
    {
        false
    }

    /// See `AppState::add_font()`
    pub fn add_font<S: Into<String>, R: Read>(&mut self, id: S, data: R)
        -> Result<Option<()>, ImageError>
    {
        Ok(Some(()))
    }

    /// See `AppState::remove_font()`
    pub(crate) fn remove_font<S: Into<String>>(&mut self, id: S) 
        -> Option<()>
    {
        Some(())
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

fn prepare_image(mut image_decoded: DynamicImage) 
    -> Result<(ImageData, ImageDescriptor), ImageError> 
{
    let image_dims = image_decoded.dimensions();
    let format = match image_decoded {
        image::ImageLuma8(_) => ImageFormat::R8,
        image::ImageLumaA8(_) => {
            image_decoded = DynamicImage::ImageLuma8(image_decoded.to_luma());
            ImageFormat::R8
        },
        image::ImageRgba8(_) => ImageFormat::BGRA8,
        image::ImageRgb8(_) => { 
            image_decoded = DynamicImage::ImageRgba8(image_decoded.to_rgba());
            ImageFormat::BGRA8 
        },
    };

    let mut bytes = image_decoded.raw_pixels();
    if format == ImageFormat::BGRA8 {
        premultiply(bytes.as_mut_slice());
    }

    let opaque = is_image_opaque(format, &bytes[..]);
    let allow_mipmaps = true;
    let descriptor = ImageDescriptor::new(image_dims.0, image_dims.1, format, opaque, allow_mipmaps);
    let data = ImageData::new(bytes);
    Ok((data, descriptor))
}

fn is_image_opaque(format: ImageFormat, bytes: &[u8]) -> bool {
    match format {
        ImageFormat::BGRA8 => {
            let mut is_opaque = true;
            for i in 0..(bytes.len() / 4) {
                if bytes[i * 4 + 3] != 255 {
                    is_opaque = false;
                    break;
                }
            }
            is_opaque
        }
        ImageFormat::R8 => true,
        _ => unreachable!(),
    }
}

// From webrender/wrench
// These are slow. Gecko's gfx/2d/Swizzle.cpp has better versions
pub fn premultiply(data: &mut [u8]) {
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