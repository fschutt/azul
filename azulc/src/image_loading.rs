#![cfg(feature = "std")]
#![cfg_attr(not(feature = "std"), no_std)]

use std::{
    path::PathBuf,
    io::Error as IoError,
};
use alloc::string::String;
use core::fmt;
use azul_core::app_resources::{ImageSource, LoadedImageSource, OptionLoadedImageSource};
#[cfg(feature = "image_loading")]
use image::ImageError;

#[derive(Debug)]
pub enum ImageReloadError {
    Io(IoError, PathBuf),
    #[cfg(feature = "image_loading")]
    DecodingError(ImageError),
    #[cfg(not(feature = "image_loading"))]
    DecodingModuleNotActive,
}

impl fmt::Display for ImageReloadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::ImageReloadError::*;
        match &self {
            Io(err, path_buf) => write!(f, "Could not load \"{}\" - IO error: {}", path_buf.as_path().to_string_lossy(), err),
            #[cfg(feature = "image_loading")]
            DecodingError(err) => write!(f, "Image decoding error: \"{}\"", err),
            #[cfg(not(feature = "image_loading"))]
            DecodingModuleNotActive => write!(f, "Found decoded image, but crate was not compiled with --features=\"image_loading\""),
        }
    }
}

pub extern "C" fn image_source_get_bytes(image_source: &ImageSource) -> OptionLoadedImageSource {
    image_source_get_bytes_inner(image_source).ok().into()
}

/// Returns the **decoded** bytes of the image + the descriptor (contains width / height).
/// Returns an error if the data is encoded, but the crate wasn't built with `--features="image_loading"`
pub fn image_source_get_bytes_inner(image_source: &ImageSource) -> Result<LoadedImageSource, ImageReloadError> {

    use azul_core::app_resources::{ImageDescriptor, ImageDescriptorFlags, ImageData};

    match image_source {
        ImageSource::Embedded(bytes) => {
            #[cfg(feature = "image_loading")] {
                use crate::image::decode_image_data;
                decode_image_data(bytes.as_ref()).map_err(|e| ImageReloadError::DecodingError(e))
            }
            #[cfg(not(feature = "image_loading"))] {
                Err(ImageReloadError::DecodingModuleNotActive)
            }
        },
        ImageSource::Raw(raw_image) => {
            use azul_core::app_resources::is_image_opaque;
            let is_opaque = is_image_opaque(raw_image.data_format, &raw_image.pixels.as_ref());
            let descriptor = ImageDescriptor {
                format: raw_image.data_format,
                width: raw_image.width,
                height: raw_image.height,
                stride: None.into(),
                offset: 0,
                flags: ImageDescriptorFlags {
                    is_opaque,
                    allow_mipmaps: true,
                },
            };
            let data = ImageData::Raw(raw_image.pixels.clone().into());
            Ok(LoadedImageSource { image_bytes_decoded: data, image_descriptor: descriptor })
        },
        ImageSource::File(file_path) => {
            #[cfg(feature = "image_loading")] {
                use std::fs;
                use crate::image::decode_image_data;
                let file_path: String = file_path.clone().into_library_owned_string();
                let file_path = PathBuf::from(file_path);
                let bytes = fs::read(&file_path).map_err(|e| ImageReloadError::Io(e, file_path.clone()))?;
                decode_image_data(&bytes).map_err(|e| ImageReloadError::DecodingError(e))
            }
            #[cfg(not(feature = "image_loading"))] {
                Err(ImageReloadError::DecodingModuleNotActive)
            }
        },
    }
}