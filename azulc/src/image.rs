#![cfg(feature = "image_loading")]

#[cfg(feature = "std")]
pub mod decode {

    use azul_css::U8Vec;
    use image_crate::error::ImageError;
    use image_crate::error::LimitError;
    use image_crate::error::LimitErrorKind;
    use image_crate::DynamicImage;
    use azul_core::app_resources::{RawImage, RawImageFormat};
    use core::fmt;

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
            use self::DecodeImageError::*;
            match self {
                InsufficientMemory => write!(f, "Error decoding image: Not enough memory available to perform encoding operation"),
                DimensionError => write!(f, "Error decoding image: Wrong dimensions"),
                InvalidData => write!(f, "Error decoding image: Invalid data format"),
                Unknown => write!(f, "Error decoding image: Unknown error"),
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

    impl_result!(RawImage, DecodeImageError, ResultRawImageDecodeImageError, copy = false, [Debug, Clone]);

    pub fn decode_raw_image_from_any_bytes(image_bytes: &[u8]) -> ResultRawImageDecodeImageError {

        use azul_core::app_resources::RawImageData;

        let image_format = match image_crate::guess_format(image_bytes) {
            Ok(o) => o,
            Err(e) => { return ResultRawImageDecodeImageError::Err(translate_image_error_decode(e)); },
        };

        let decoded = match image_crate::load_from_memory_with_format(image_bytes, image_format) {
            Ok(o) => o,
            Err(e) => { return ResultRawImageDecodeImageError::Err(translate_image_error_decode(e)); },
        };

        let ((width, height), data_format, pixels) = match decoded {
            DynamicImage::ImageLuma8(i) => {
                (i.dimensions(), RawImageFormat::R8, RawImageData::U8(i.into_vec().into()))
            },
            DynamicImage::ImageLumaA8(i) => {
                (i.dimensions(), RawImageFormat::RG8, RawImageData::U8(i.into_vec().into()))
            },
            DynamicImage::ImageRgb8(i) => {
                (i.dimensions(), RawImageFormat::RGB8, RawImageData::U8(i.into_vec().into()))
            },
            DynamicImage::ImageRgba8(i) => {
                (i.dimensions(), RawImageFormat::RGBA8, RawImageData::U8(i.into_vec().into()))
            },
            DynamicImage::ImageBgr8(i) => {
                (i.dimensions(), RawImageFormat::BGR8, RawImageData::U8(i.into_vec().into()))
            },
            DynamicImage::ImageBgra8(i) => {
                (i.dimensions(), RawImageFormat::BGRA8, RawImageData::U8(i.into_vec().into()))
            },
            DynamicImage::ImageLuma16(i) => {
                (i.dimensions(), RawImageFormat::R16, RawImageData::U16(i.into_vec().into()))
            },
            DynamicImage::ImageLumaA16(i) => {
                (i.dimensions(), RawImageFormat::RG16, RawImageData::U16(i.into_vec().into()))
            },
            DynamicImage::ImageRgb16(i) => {
                (i.dimensions(), RawImageFormat::RGB16, RawImageData::U16(i.into_vec().into()))
            },
            DynamicImage::ImageRgba16(i) => {
                (i.dimensions(), RawImageFormat::RGBA16, RawImageData::U16(i.into_vec().into()))
            },
        };

        ResultRawImageDecodeImageError::Ok(RawImage {
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
    use azul_core::app_resources::RawImageFormat;
    use core::fmt;

    #[cfg(feature = "bmp")]
    use image_crate::codecs::bmp::BmpEncoder;
    #[cfg(feature = "png")]
    use image_crate::codecs::png::PngEncoder;
    #[cfg(feature = "jpeg")]
    use image_crate::codecs::jpeg::JpegEncoder;
    #[cfg(feature = "gif")]
    use image_crate::codecs::gif::GifEncoder;
    #[cfg(feature = "tiff")]
    use image_crate::codecs::tiff::TiffEncoder;
    #[cfg(feature = "tga")]
    use image_crate::codecs::tga::TgaEncoder;
    #[cfg(feature = "dxt")]
    use image_crate::codecs::dxt::DxtEncoder;
    #[cfg(feature = "pnm")]
    use image_crate::codecs::pnm::PnmEncoder;
    #[cfg(feature = "hdr")]
    use image_crate::codecs::hdr::HdrEncoder;

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
        InvalidData,
        Unknown,
    }

    impl fmt::Display for EncodeImageError {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            use self::EncodeImageError::*;
            match self {
                InsufficientMemory => write!(f, "Error encoding image: Not enough memory available to perform encoding operation"),
                DimensionError => write!(f, "Error encoding image: Wrong dimensions"),
                InvalidData => write!(f, "Error encoding image: Invalid data format"),
                Unknown => write!(f, "Error encoding image: Unknown error"),
            }
        }
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

    macro_rules! encode_func {($func:ident, $encoder:ident, $get_fn:ident, $feature:expr) => (
        #[cfg(feature = $feature)]
        pub fn $func(image: &RawImage) -> ResultU8VecEncodeImageError {
            let mut result = Vec::<u8>::new();

            {
                let mut cursor = Cursor::new(&mut result);
                let mut encoder = $encoder::new(&mut cursor);
                let pixels = match image.pixels.$get_fn() {
                    Some(s) => s,
                    None => { return ResultU8VecEncodeImageError::Err(EncodeImageError::InvalidData); },
                };

                if let Err(e) = encoder.encode(
                    pixels.as_ref(),
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

    encode_func!(encode_bmp, BmpEncoder, get_u8_vec_ref, "bmp");
    encode_func!(encode_png, PngEncoder, get_u8_vec_ref, "png");
    encode_func!(encode_jpeg, JpegEncoder, get_u8_vec_ref, "jpeg");
    encode_func!(encode_tga, TgaEncoder, get_u8_vec_ref, "tga");
    encode_func!(encode_tiff, TiffEncoder, get_u8_vec_ref, "tiff");
    encode_func!(encode_gif, GifEncoder, get_u8_vec_ref, "gif");
    encode_func!(encode_pnm, PnmEncoder, get_u8_vec_ref, "pnm");
}
