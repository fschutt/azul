//! Image decoding and encoding utilities, wrapping the `image` crate
//! with Azul's FFI-compatible types (`RawImage`, `RawImageFormat`).
//!
//! - [`decode`]: Decodes image bytes in any supported format into a [`RawImage`].
//! - [`encode`]: Encodes a [`RawImage`] into various output formats (PNG, JPEG, BMP, etc.).

#[cfg(feature = "std")]
pub mod decode {
    use core::fmt;

    use azul_core::resources::{RawImage, RawImageFormat};
    use azul_css::{impl_result, impl_result_inner, U8Vec};
    use image::{
        error::{ImageError, LimitError, LimitErrorKind},
        DynamicImage,
    };

    /// Errors that can occur when decoding an image from raw bytes.
    #[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
    #[repr(C)]
    pub enum DecodeImageError {
        InsufficientMemory,
        DimensionError,
        UnsupportedImageFormat,
        Unknown,
    }

    impl fmt::Display for DecodeImageError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::InsufficientMemory => write!(
                    f,
                    "Error decoding image: Not enough memory available to perform encoding \
                     operation"
                ),
                Self::DimensionError => {
                    write!(f, "Error decoding image: Wrong dimensions")
                }
                Self::UnsupportedImageFormat => {
                    write!(f, "Error decoding image: Invalid data format")
                }
                Self::Unknown => write!(f, "Error decoding image: Unknown error"),
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

    /// Decodes image bytes in any supported format into a [`RawImage`].
    ///
    /// The image format is guessed from the byte contents. Returns the decoded
    /// pixel data along with dimensions and format information.
    #[must_use] pub fn decode_raw_image_from_any_bytes(image_bytes: &[u8]) -> ResultRawImageDecodeImageError {
        use azul_core::resources::RawImageData;

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
}
#[cfg(feature = "std")]
pub mod encode {
    use alloc::vec::Vec;
    use core::fmt;
    use std::io::Cursor;

    use azul_core::resources::{RawImage, RawImageFormat};
    use azul_css::{impl_result, impl_result_inner, U8Vec};
    #[cfg(feature = "bmp")]
    use image::codecs::bmp::BmpEncoder;
    #[cfg(feature = "gif")]
    use image::codecs::gif::GifEncoder;
#[cfg(feature = "jpeg")]
    use image::codecs::jpeg::JpegEncoder;
    #[cfg(feature = "png")]
    use image::codecs::png::PngEncoder;
    #[cfg(feature = "pnm")]
    use image::codecs::pnm::PnmEncoder;
    #[cfg(feature = "tga")]
    use image::codecs::tga::TgaEncoder;
    #[cfg(feature = "tiff")]
    use image::codecs::tiff::TiffEncoder;
    use image::error::{ImageError, LimitError, LimitErrorKind};

    /// Errors that can occur when encoding a [`RawImage`] into a specific format.
    #[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
    #[repr(C)]
    pub enum EncodeImageError {
        /// Crate was not compiled with the given encoder flags
        EncoderNotAvailable,
        InsufficientMemory,
        DimensionError,
        InvalidData,
        Unknown,
    }

    impl fmt::Display for EncodeImageError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            use self::EncodeImageError::{EncoderNotAvailable, InsufficientMemory, DimensionError, InvalidData, Unknown};
            match self {
                EncoderNotAvailable => write!(
                    f,
                    "Missing encoder (library was not compiled with given codec)"
                ),
                InsufficientMemory => write!(
                    f,
                    "Error encoding image: Not enough memory available to perform encoding \
                     operation"
                ),
                DimensionError => write!(f, "Error encoding image: Wrong dimensions"),
                InvalidData => write!(f, "Error encoding image: Invalid data format"),
                Unknown => write!(f, "Error encoding image: Unknown error"),
            }
        }
    }

    const fn translate_rawimage_colortype(i: RawImageFormat) -> image::ColorType {
        match i {
            RawImageFormat::R8 => image::ColorType::L8,
            RawImageFormat::RG8 => image::ColorType::La8,
            RawImageFormat::RGB8 | RawImageFormat::BGR8 => image::ColorType::Rgb8,
            RawImageFormat::RGBA8 | RawImageFormat::BGRA8 => image::ColorType::Rgba8,
            RawImageFormat::R16 => image::ColorType::L16,
            RawImageFormat::RG16 => image::ColorType::La16,
            RawImageFormat::RGB16 => image::ColorType::Rgb16,
            RawImageFormat::RGBA16 => image::ColorType::Rgba16,
            RawImageFormat::RGBF32 => image::ColorType::Rgb32F,
            RawImageFormat::RGBAF32 => image::ColorType::Rgba32F,
        }
    }

    fn bgr_to_rgb_swap(pixels: &[u8], format: RawImageFormat) -> Option<Vec<u8>> {
        match format {
            RawImageFormat::BGR8 => {
                let mut out = pixels.to_vec();
                for chunk in out.chunks_exact_mut(3) {
                    chunk.swap(0, 2);
                }
                Some(out)
            }
            RawImageFormat::BGRA8 => {
                let mut out = pixels.to_vec();
                for chunk in out.chunks_exact_mut(4) {
                    chunk.swap(0, 2);
                }
                Some(out)
            }
            _ => None,
        }
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

    impl_result!(
        U8Vec,
        EncodeImageError,
        ResultU8VecEncodeImageError,
        copy = false,
        [Debug, Clone]
    );

    macro_rules! encode_func {
        ($func:ident, $encoder:ident, $feature:expr) => {
            #[cfg(feature = $feature)]
            pub fn $func(image: &RawImage) -> ResultU8VecEncodeImageError {
                let width = match u32::try_from(image.width) {
                    Ok(w) => w,
                    Err(_) => return ResultU8VecEncodeImageError::Err(EncodeImageError::DimensionError),
                };
                let height = match u32::try_from(image.height) {
                    Ok(h) => h,
                    Err(_) => return ResultU8VecEncodeImageError::Err(EncodeImageError::DimensionError),
                };
                let mut result = Vec::<u8>::new();

                {
                    let mut cursor = Cursor::new(&mut result);
                    let mut encoder = $encoder::new(&mut cursor);
                    let pixels = match image.pixels.get_u8_vec_ref() {
                        Some(s) => s,
                        None => {
                            return ResultU8VecEncodeImageError::Err(EncodeImageError::InvalidData);
                        }
                    };

                    let swapped = bgr_to_rgb_swap(pixels.as_ref(), image.data_format);
                    let pixel_bytes = swapped.as_deref().unwrap_or(pixels.as_ref());

                    if let Err(e) = encoder.encode(
                        pixel_bytes,
                        width,
                        height,
                        translate_rawimage_colortype(image.data_format).into(),
                    ) {
                        return ResultU8VecEncodeImageError::Err(translate_image_error_encode(e));
                    }
                }

                ResultU8VecEncodeImageError::Ok(result.into())
            }

            #[cfg(not(feature = $feature))]
            #[must_use] pub const fn $func(image: &RawImage) -> ResultU8VecEncodeImageError {
                ResultU8VecEncodeImageError::Err(EncodeImageError::EncoderNotAvailable)
            }
        };
    }

    encode_func!(encode_bmp, BmpEncoder, "bmp");
    encode_func!(encode_tga, TgaEncoder, "tga");
    encode_func!(encode_tiff, TiffEncoder, "tiff");
    encode_func!(encode_gif, GifEncoder, "gif");
    encode_func!(encode_pnm, PnmEncoder, "pnm");

    #[cfg(feature = "png")]
    pub fn encode_png(image: &RawImage) -> ResultU8VecEncodeImageError {
        use image::ImageEncoder;

        let width = match u32::try_from(image.width) {
            Ok(w) => w,
            Err(_) => return ResultU8VecEncodeImageError::Err(EncodeImageError::DimensionError),
        };
        let height = match u32::try_from(image.height) {
            Ok(h) => h,
            Err(_) => return ResultU8VecEncodeImageError::Err(EncodeImageError::DimensionError),
        };
        let mut result = Vec::<u8>::new();

        {
            let mut cursor = Cursor::new(&mut result);
            let mut encoder = PngEncoder::new_with_quality(
                &mut cursor,
                image::codecs::png::CompressionType::Best,
                image::codecs::png::FilterType::Adaptive,
            );
            let pixels = match image.pixels.get_u8_vec_ref() {
                Some(s) => s,
                None => {
                    return ResultU8VecEncodeImageError::Err(EncodeImageError::InvalidData);
                }
            };

            let swapped = bgr_to_rgb_swap(pixels.as_ref(), image.data_format);
            let pixel_bytes = swapped.as_deref().unwrap_or(pixels.as_ref());

            if let Err(e) = encoder.write_image(
                pixel_bytes,
                width,
                height,
                translate_rawimage_colortype(image.data_format).into(),
            ) {
                return ResultU8VecEncodeImageError::Err(translate_image_error_encode(e));
            }
        }

        ResultU8VecEncodeImageError::Ok(result.into())
    }

    #[cfg(not(feature = "png"))]
    #[must_use] pub const fn encode_png(image: &RawImage) -> ResultU8VecEncodeImageError {
        ResultU8VecEncodeImageError::Err(EncodeImageError::EncoderNotAvailable)
    }

    #[cfg(feature = "jpeg")]
    pub fn encode_jpeg(image: &RawImage, quality: u8) -> ResultU8VecEncodeImageError {
        let width = match u32::try_from(image.width) {
            Ok(w) => w,
            Err(_) => return ResultU8VecEncodeImageError::Err(EncodeImageError::DimensionError),
        };
        let height = match u32::try_from(image.height) {
            Ok(h) => h,
            Err(_) => return ResultU8VecEncodeImageError::Err(EncodeImageError::DimensionError),
        };
        let mut result = Vec::<u8>::new();

        {
            let mut cursor = Cursor::new(&mut result);
            let mut encoder = JpegEncoder::new_with_quality(&mut cursor, quality);
            let pixels = match image.pixels.get_u8_vec_ref() {
                Some(s) => s,
                None => {
                    return ResultU8VecEncodeImageError::Err(EncodeImageError::InvalidData);
                }
            };

            let swapped = bgr_to_rgb_swap(pixels.as_ref(), image.data_format);
            let pixel_bytes = swapped.as_deref().unwrap_or(pixels.as_ref());

            if let Err(e) = encoder.encode(
                pixel_bytes,
                width,
                height,
                translate_rawimage_colortype(image.data_format).into(),
            ) {
                return ResultU8VecEncodeImageError::Err(translate_image_error_encode(e));
            }
        }

        ResultU8VecEncodeImageError::Ok(result.into())
    }

    #[cfg(not(feature = "jpeg"))]
    #[must_use] pub const fn encode_jpeg(image: &RawImage, quality: u8) -> ResultU8VecEncodeImageError {
        ResultU8VecEncodeImageError::Err(EncodeImageError::EncoderNotAvailable)
    }
}
