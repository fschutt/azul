//! Azul-XML-to-Rust compiler (library)

extern crate gleam;
extern crate xmlparser;
#[macro_use(impl_from, impl_display)]
extern crate azul_core;
extern crate azul_css;
extern crate azul_layout;
#[cfg(feature = "font_loading")]
extern crate font_loader;
#[cfg(feature = "image_loading")]
extern crate image as image_crate;

pub struct Dummy;

/// XML-based DOM serialization and XML-to-Rust compiler implementation
pub mod xml;
/// Module for compiling CSS to Rust code
pub mod css;
#[cfg(feature = "font_loading")]
pub mod font;
#[cfg(feature = "image_loading")]
pub mod image;
/// Re-export of the `azul-layout` crate
pub mod layout {
    pub use azul_layout::*;
}
/// Module for decoding and loading fonts
pub mod font_loading {

    use std::{
        path::PathBuf,
        io::Error as IoError,
    };
    use azul_core::app_resources::FontSource;
    #[cfg(feature = "text_layout")]
    use azul_core::app_resources::LoadedFontSource;

    const DEFAULT_FONT_INDEX: i32 = 0;

    #[derive(Debug)]
    pub enum FontReloadError {
        Io(IoError, PathBuf),
        FontNotFound(String),
        FontLoadingNotActive(String),
    }

    impl Clone for FontReloadError {
        fn clone(&self) -> Self {
            use self::FontReloadError::*;
            match self {
                Io(err, path) => Io(IoError::new(err.kind(), "Io Error"), path.clone()),
                FontNotFound(id) => FontNotFound(id.clone()),
                FontLoadingNotActive(id) => FontLoadingNotActive(id.clone()),
            }
        }
    }

    impl_display!(FontReloadError, {
        Io(err, path_buf) => format!("Could not load \"{}\" - IO error: {}", path_buf.as_path().to_string_lossy(), err),
        FontNotFound(id) => format!("Could not locate system font: \"{}\" found", id),
        FontLoadingNotActive(id) => format!("Could not load system font: \"{}\": crate was not compiled with --features=\"font_loading\"", id)
    });

    #[cfg(feature = "text_layout")]
    pub fn font_source_get_bytes(font_source: &FontSource) -> Option<LoadedFontSource> {
        use crate::layout::text_layout::text_shaping::get_font_metrics_freetype;
        // TODO: logging!
        let (font_bytes, font_index) = font_source_get_bytes_inner(font_source).ok()?;
        let font_metrics = get_font_metrics_freetype(&font_bytes, DEFAULT_FONT_INDEX);
        Some(LoadedFontSource{ font_bytes, font_index, font_metrics })
    }

    /// Returns the bytes of the font (loads the font from the system in case it is a `FontSource::System` font).
    /// Also returns the index into the font (in case the font is a font collection).
    pub fn font_source_get_bytes_inner(font_source: &FontSource) -> Result<(Vec<u8>, i32), FontReloadError> {

        match font_source {
            FontSource::Embedded(font_bytes) => Ok((font_bytes.to_vec(), DEFAULT_FONT_INDEX)),
            FontSource::File(file_path) => {
                std::fs::read(file_path)
                .map_err(|e| FontReloadError::Io(e, file_path.clone()))
                .map(|font_bytes| (font_bytes, DEFAULT_FONT_INDEX))
            },
            FontSource::System(id) => {
                #[cfg(feature = "font_loading")] {
                    crate::font::load_system_font(id)
                    .ok_or(FontReloadError::FontNotFound(id.clone()))
                }
                #[cfg(not(feature = "font_loading"))] {
                    Err(FontReloadError::FontLoadingNotActive(id.clone()))
                }
            },
        }
    }
}

/// Module for decoding and loading images
pub mod image_loading {

    use std::{
        fmt,
        path::PathBuf,
        io::Error as IoError,
    };
    use azul_core::app_resources::{ImageSource, LoadedImageSource};
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

    /// Returns the **decoded** bytes of the image + the descriptor (contains width / height).
    /// Returns an error if the data is encoded, but the crate wasn't built with `--features="image_loading"`
    pub fn image_source_get_bytes(image_source: &ImageSource) -> Option<LoadedImageSource> {
        image_source_get_bytes_inner(image_source).ok()
    }

    pub fn image_source_get_bytes_inner(image_source: &ImageSource) -> Result<LoadedImageSource, ImageReloadError> {

        use azul_core::app_resources::{ImageDescriptor, ImageDescriptorFlags, ImageData};

        match image_source {
            ImageSource::Embedded(bytes) => {
                #[cfg(feature = "image_loading")] {
                    use crate::image::decode_image_data;
                    decode_image_data(bytes.to_vec()).map_err(|e| ImageReloadError::DecodingError(e))
                }
                #[cfg(not(feature = "image_loading"))] {
                    Err(ImageReloadError::DecodingModuleNotActive)
                }
            },
            ImageSource::Raw(raw_image) => {
                use std::sync::Arc;
                use azul_core::app_resources::is_image_opaque;
                let is_opaque = is_image_opaque(raw_image.data_format, &raw_image.pixels[..]);
                let descriptor = ImageDescriptor {
                    format: raw_image.data_format,
                    dimensions: raw_image.image_dimensions,
                    stride: None,
                    offset: 0,
                    flags: ImageDescriptorFlags {
                        is_opaque,
                        allow_mipmaps: true,
                    },
                };
                let data = ImageData::Raw(Arc::new(raw_image.pixels.clone()));
                Ok(LoadedImageSource { image_bytes_decoded: data, image_descriptor: descriptor })
            },
            ImageSource::File(file_path) => {
                #[cfg(feature = "image_loading")] {
                    use std::fs;
                    use crate::image::decode_image_data;
                    let bytes = fs::read(file_path).map_err(|e| ImageReloadError::Io(e, file_path.clone()))?;
                    decode_image_data(bytes).map_err(|e| ImageReloadError::DecodingError(e))
                }
                #[cfg(not(feature = "image_loading"))] {
                    Err(ImageReloadError::DecodingModuleNotActive)
                }
            },
        }
    }
}
