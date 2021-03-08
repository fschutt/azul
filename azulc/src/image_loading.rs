#![cfg(feature = "std")]
#![cfg_attr(not(feature = "std"), no_std)]

use std::{
    path::PathBuf,
    io::Error as IoError,
};
use core::fmt;
use azul_css::AzString;
use azul_core::app_resources::{
    ImageSource, RawImage,
    OptionLoadedImageSource
};
#[cfg(feature = "image_loading")]
use crate::image::decode::DecodeImageError;

#[derive(Debug)]
pub enum ImageReloadError {
    Io(IoError, AzString),
    #[cfg(feature = "image_loading")]
    DecodingError(DecodeImageError),
    DecodingModuleNotActive,
}

impl fmt::Display for ImageReloadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::ImageReloadError::*;
        match &self {
            Io(err, path_buf) => write!(f, "Could not load \"{}\" - IO error: {}", path_buf.as_str(), err),
            #[cfg(feature = "image_loading")]
            DecodingError(err) => write!(f, "Image decoding error: \"{:?}\"", err),
            DecodingModuleNotActive => write!(f, "Found decoded image, but crate was not compiled with --features=\"image_loading\""),
        }
    }
}

pub extern "C" fn image_source_get_bytes(image_source: ImageSource) -> OptionLoadedImageSource {
    let raw_image = match image_source_get_bytes_inner(image_source) {
        Ok(o) => o,
        Err(e) => return OptionLoadedImageSource::None,
    };
    raw_image.into_loaded_image_source().into()
}

/// Returns the **decoded** bytes of the image + the descriptor (contains width / height).
/// Returns an error if the data is encoded, but the crate wasn't built with `--features="image_loading"`
pub fn image_source_get_bytes_inner(image_source: ImageSource) -> Result<RawImage, ImageReloadError> {
    match image_source {
        ImageSource::Embedded(bytes) => {
            #[cfg(feature = "image_loading")] {
                use crate::image::decode::decode_raw_image_from_any_bytes;
                decode_raw_image_from_any_bytes(bytes.as_ref()).into_result().map_err(|e| ImageReloadError::DecodingError(e))
            }
            #[cfg(not(feature = "image_loading"))] {
                Err(ImageReloadError::DecodingModuleNotActive)
            }
        },
        ImageSource::Raw(raw_image) => Ok(raw_image),
        ImageSource::File(file_path) => {
            #[cfg(feature = "image_loading")] {
                use std::fs;
                use crate::image::decode::decode_raw_image_from_any_bytes;
                let bytes = fs::read(&file_path.as_ref()).map_err(|e| ImageReloadError::Io(e, file_path.clone()))?;
                decode_raw_image_from_any_bytes(&bytes).into_result().map_err(|e| ImageReloadError::DecodingError(e))
            }
            #[cfg(not(feature = "image_loading"))] {
                Err(ImageReloadError::DecodingModuleNotActive)
            }
        },
    }
}