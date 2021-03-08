#![cfg(feature = "image_loading")]

#[cfg(feature = "std")]
pub mod decode {

    #[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
    #[repr(C)]
    pub enum DecodeImageError {
        InsufficientMemory,
        DimensionError,
        UnsupportedImageFormat,
        Unknown,
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

    impl_result!(U8Vec, DecodeImageError, ResultU8VecDecodeImageError, copy = false, [Debug, Clone]);

    pub fn decode_raw_image_from_any_bytes(image_bytes: &[u8]) -> ResultU8VecDecodeImageError {
        let image_format = match image_crate::guess_format(image_data) {
            Ok(o) => o,
            Err(e) => { return ResultU8VecDecodeImageError::Err(translate_image_error_decode(e)); },
        };

        let decoded = match image_crate::load_from_memory_with_format(image_data, image_format) {
            Ok(o) => o,
            Err(e) => { return ResultU8VecDecodeImageError::Err(translate_image_error_decode(e)); },
        };

        ResultU8VecDecodeImageError::Ok(decoded.into())
    }
}

#[cfg(feature = "std")]
pub mod encode {

    use alloc::vec::Vec;

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
