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

    #[cfg(test)]
    mod autotest_generated {
        use std::collections::BTreeSet;

        use image::error::{
            DecodingError, ImageFormatHint, ParameterError, ParameterErrorKind, UnsupportedError,
            UnsupportedErrorKind,
        };

        use super::*;

        const ALL_ERRORS: [DecodeImageError; 4] = [
            DecodeImageError::InsufficientMemory,
            DecodeImageError::DimensionError,
            DecodeImageError::UnsupportedImageFormat,
            DecodeImageError::Unknown,
        ];

        fn err_of(r: &ResultRawImageDecodeImageError) -> DecodeImageError {
            match r.as_result() {
                Ok(_) => panic!("expected Err, got Ok"),
                Err(e) => *e,
            }
        }

        // --- DecodeImageError::fmt (serializer) ---------------------------------

        #[test]
        fn display_every_variant_is_non_empty_and_unique() {
            let rendered = ALL_ERRORS.iter().map(|e| e.to_string()).collect::<Vec<_>>();
            for (e, s) in ALL_ERRORS.iter().zip(rendered.iter()) {
                assert!(!s.is_empty(), "empty Display for {e:?}");
                assert!(
                    s.starts_with("Error decoding image: "),
                    "unexpected Display prefix for {e:?}: {s}"
                );
            }
            let unique = rendered.iter().collect::<BTreeSet<_>>();
            assert_eq!(unique.len(), ALL_ERRORS.len(), "Display collides: {rendered:?}");
        }

        #[test]
        fn display_text_is_stable() {
            assert_eq!(
                DecodeImageError::DimensionError.to_string(),
                "Error decoding image: Wrong dimensions"
            );
            assert_eq!(
                DecodeImageError::UnsupportedImageFormat.to_string(),
                "Error decoding image: Invalid data format"
            );
            assert_eq!(
                DecodeImageError::Unknown.to_string(),
                "Error decoding image: Unknown error"
            );
            // NOTE: the InsufficientMemory arm says "encoding operation" inside a *decode*
            // error. Pinned here because it is user-visible text, not because it is right.
            assert_eq!(
                DecodeImageError::InsufficientMemory.to_string(),
                "Error decoding image: Not enough memory available to perform encoding operation"
            );
        }

        #[test]
        fn display_ignores_width_and_precision_specs_without_panicking() {
            for e in ALL_ERRORS {
                // The impl writes straight to the formatter, so padding/precision are no-ops.
                let plain = format!("{e}");
                assert_eq!(format!("{e:>200}"), plain);
                assert_eq!(format!("{e:.1}"), plain);
                assert_eq!(format!("{e:^0}"), plain);
                assert!(!format!("{e:?}").is_empty());
            }
        }

        #[test]
        fn derived_ord_is_total_and_matches_declaration_order() {
            for (i, a) in ALL_ERRORS.iter().enumerate() {
                for (j, b) in ALL_ERRORS.iter().enumerate() {
                    assert_eq!(a.cmp(b), i.cmp(&j), "Ord disagrees for {a:?} vs {b:?}");
                    assert_eq!(a.partial_cmp(b), Some(a.cmp(b)));
                    assert_eq!(a == b, i == j);
                }
            }
        }

        // --- translate_image_error_decode (other) -------------------------------

        #[test]
        fn translate_maps_limit_kinds() {
            assert_eq!(
                translate_image_error_decode(ImageError::Limits(LimitError::from_kind(
                    LimitErrorKind::InsufficientMemory
                ))),
                DecodeImageError::InsufficientMemory
            );
            assert_eq!(
                translate_image_error_decode(ImageError::Limits(LimitError::from_kind(
                    LimitErrorKind::DimensionError
                ))),
                DecodeImageError::DimensionError
            );
            // The catch-all arm of the (non_exhaustive) LimitErrorKind match.
            assert_eq!(
                translate_image_error_decode(ImageError::Limits(LimitError::from_kind(
                    LimitErrorKind::Unsupported {
                        limits: image::Limits::default(),
                        supported: image::LimitSupport::default(),
                    }
                ))),
                DecodeImageError::Unknown
            );
        }

        #[test]
        fn translate_maps_every_other_variant_to_unknown() {
            let cases = vec![
                ImageError::IoError(std::io::Error::other("boom")),
                ImageError::Decoding(DecodingError::new(ImageFormatHint::Unknown, "bad chunk")),
                ImageError::Unsupported(UnsupportedError::from_format_and_kind(
                    ImageFormatHint::Unknown,
                    UnsupportedErrorKind::Format(ImageFormatHint::Unknown),
                )),
                // NOTE: a *parameter* dimension mismatch is NOT mapped to DimensionError --
                // only `Limits` errors are inspected. Pinning the lossy mapping.
                ImageError::Parameter(ParameterError::from_kind(
                    ParameterErrorKind::DimensionMismatch,
                )),
                ImageError::Parameter(ParameterError::from_kind(ParameterErrorKind::NoMoreData)),
            ];
            for c in cases {
                let debug = format!("{c:?}");
                assert_eq!(
                    translate_image_error_decode(c),
                    DecodeImageError::Unknown,
                    "unexpected mapping for {debug}"
                );
            }
        }

        // --- decode_raw_image_from_any_bytes (other / parser-ish) ---------------

        #[test]
        fn decode_empty_input_errors_without_panicking() {
            let r = decode_raw_image_from_any_bytes(&[]);
            assert!(r.is_err());
            assert_eq!(err_of(&r), DecodeImageError::Unknown);
        }

        #[test]
        fn decode_whitespace_and_text_bytes_error() {
            for input in [
                &b"   "[..],
                &b"\t\n\r\n"[..],
                &b"not an image at all"[..],
                "\u{1F600} combining a\u{0301}\u{0308}".as_bytes(),
                "\u{FEFF}\u{202E}\u{0000}".as_bytes(),
            ] {
                let r = decode_raw_image_from_any_bytes(input);
                assert!(r.is_err(), "unexpectedly decoded {input:?}");
                assert_eq!(err_of(&r), DecodeImageError::Unknown);
            }
        }

        #[test]
        fn decode_invalid_utf8_and_garbage_bytes_error() {
            for input in [
                &[0xFF, 0xFE, 0x00][..],
                &[0xFF][..],
                &[0x00, 0x00, 0x00, 0x00][..],
                &[0xC0, 0x80, 0xED, 0xA0, 0x80][..],
            ] {
                let r = decode_raw_image_from_any_bytes(input);
                assert!(r.is_err(), "unexpectedly decoded {input:?}");
                assert_eq!(err_of(&r), DecodeImageError::Unknown);
            }
        }

        #[test]
        fn decode_every_single_byte_input_errors() {
            for b in 0u8..=255 {
                let r = decode_raw_image_from_any_bytes(&[b]);
                assert!(r.is_err(), "single byte {b:#04X} decoded to an image");
            }
        }

        #[test]
        fn decode_extremely_long_garbage_does_not_hang_or_panic() {
            let huge = vec![0xAAu8; 1_000_000];
            let r = decode_raw_image_from_any_bytes(&huge);
            assert!(r.is_err());
            assert_eq!(err_of(&r), DecodeImageError::Unknown);
        }

        /// Valid magic bytes with a truncated / missing body: `guess_format` succeeds,
        /// the actual decode must then fail cleanly (never panic), regardless of which
        /// codec features are compiled in.
        #[test]
        fn decode_valid_magic_with_no_payload_errors() {
            let png_magic = b"\x89PNG\r\n\x1a\n";
            let gif_magic = b"GIF89a";
            let bmp_magic = b"BM";
            let jpeg_magic = b"\xFF\xD8\xFF";

            for magic in [&png_magic[..], &gif_magic[..], &bmp_magic[..], &jpeg_magic[..]] {
                let mut bytes = magic.to_vec();
                let r = decode_raw_image_from_any_bytes(&bytes);
                assert!(r.is_err(), "header-only input decoded: {magic:?}");

                // ... and with a garbage payload appended.
                bytes.extend(core::iter::repeat_n(0x7Fu8, 512));
                let r = decode_raw_image_from_any_bytes(&bytes);
                assert!(r.is_err(), "header + garbage decoded: {magic:?}");
            }
        }

        #[cfg(feature = "png")]
        fn png_of_2x2_rgba8(fill: u8) -> Vec<u8> {
            use azul_core::resources::RawImageData;

            let img = RawImage {
                pixels: RawImageData::U8(vec![fill; 16].into()),
                width: 2,
                height: 2,
                premultiplied_alpha: false,
                data_format: RawImageFormat::RGBA8,
                tag: Vec::<u8>::new().into(),
            };
            crate::image::encode::encode_png(&img)
                .into_result()
                .expect("png encode of a well-formed 2x2 RGBA8 image failed")
                .as_slice()
                .to_vec()
        }

        #[cfg(feature = "png")]
        #[test]
        fn decode_of_every_truncation_of_a_real_png_never_panics() {
            let bytes = png_of_2x2_rgba8(1);
            assert!(bytes.len() > 33, "PNG shorter than signature + IHDR");

            for len in 0..bytes.len() {
                let r = decode_raw_image_from_any_bytes(&bytes[..len]);
                match r.as_result() {
                    // A truncation that still decodes (the trailing IEND chunk is not load
                    // bearing for every decoder) must at least not invent geometry or pixels.
                    Ok(img) => {
                        assert_eq!(
                            (img.width, img.height),
                            (2, 2),
                            "truncation to {len} bytes resized the image"
                        );
                        assert_eq!(img.pixels.get_u8_vec_ref().expect("8-bit data").len(), 16);
                    }
                    // Anything cut off before the header is complete cannot possibly decode.
                    Err(e) => {
                        assert_eq!(*e, DecodeImageError::Unknown, "truncation to {len} bytes");
                    }
                }
                if len <= 33 {
                    assert!(r.is_err(), "a {len}-byte PNG prefix decoded to an image");
                }
            }
        }

        #[cfg(feature = "png")]
        #[test]
        fn decode_of_a_corrupted_png_never_panics_or_invents_geometry() {
            let bytes = png_of_2x2_rgba8(9);

            // Flip every byte of the stream in turn: CRC/zlib checks must reject, not panic.
            for i in 0..bytes.len() {
                let mut corrupted = bytes.clone();
                corrupted[i] ^= 0xFF;
                let r = decode_raw_image_from_any_bytes(&corrupted);
                // Ok is acceptable in principle (e.g. a flipped byte in a padding field),
                // the invariant under test is "no panic, and dimensions stay sane".
                if let Ok(img) = r.as_result() {
                    assert!(img.width <= 2 && img.height <= 2, "corrupt byte {i} grew the image");
                }
            }
        }
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

    #[cfg(test)]
    mod autotest_generated {
        use std::collections::BTreeSet;

        use azul_core::resources::RawImageData;
        use image::error::{
            DecodingError, EncodingError, ImageFormatHint, ParameterError, ParameterErrorKind,
            UnsupportedError, UnsupportedErrorKind,
        };

        use super::*;

        const ALL_ERRORS: [EncodeImageError; 5] = [
            EncodeImageError::EncoderNotAvailable,
            EncodeImageError::InsufficientMemory,
            EncodeImageError::DimensionError,
            EncodeImageError::InvalidData,
            EncodeImageError::Unknown,
        ];

        const ALL_FORMATS: [RawImageFormat; 12] = [
            RawImageFormat::R8,
            RawImageFormat::RG8,
            RawImageFormat::RGB8,
            RawImageFormat::RGBA8,
            RawImageFormat::R16,
            RawImageFormat::RG16,
            RawImageFormat::RGB16,
            RawImageFormat::RGBA16,
            RawImageFormat::BGR8,
            RawImageFormat::BGRA8,
            RawImageFormat::RGBF32,
            RawImageFormat::RGBAF32,
        ];

        fn raw_u8(
            width: usize,
            height: usize,
            data_format: RawImageFormat,
            pixels: Vec<u8>,
        ) -> RawImage {
            RawImage {
                pixels: RawImageData::U8(pixels.into()),
                width,
                height,
                premultiplied_alpha: false,
                data_format,
                tag: Vec::<u8>::new().into(),
            }
        }

        #[allow(dead_code)]
        fn err_of(r: &ResultU8VecEncodeImageError) -> EncodeImageError {
            match r.as_result() {
                Ok(_) => panic!("expected Err, got Ok"),
                Err(e) => *e,
            }
        }

        // --- EncodeImageError::fmt (serializer) ---------------------------------

        #[test]
        fn display_every_variant_is_non_empty_and_unique() {
            let rendered = ALL_ERRORS.iter().map(|e| e.to_string()).collect::<Vec<_>>();
            for (e, s) in ALL_ERRORS.iter().zip(rendered.iter()) {
                assert!(!s.is_empty(), "empty Display for {e:?}");
            }
            let unique = rendered.iter().collect::<BTreeSet<_>>();
            assert_eq!(unique.len(), ALL_ERRORS.len(), "Display collides: {rendered:?}");
        }

        #[test]
        fn display_text_is_stable() {
            // The odd one out: no "Error encoding image" prefix.
            assert_eq!(
                EncodeImageError::EncoderNotAvailable.to_string(),
                "Missing encoder (library was not compiled with given codec)"
            );
            assert_eq!(
                EncodeImageError::InsufficientMemory.to_string(),
                "Error encoding image: Not enough memory available to perform encoding operation"
            );
            assert_eq!(
                EncodeImageError::DimensionError.to_string(),
                "Error encoding image: Wrong dimensions"
            );
            assert_eq!(
                EncodeImageError::InvalidData.to_string(),
                "Error encoding image: Invalid data format"
            );
            assert_eq!(
                EncodeImageError::Unknown.to_string(),
                "Error encoding image: Unknown error"
            );
        }

        #[test]
        fn display_ignores_width_and_precision_specs_without_panicking() {
            for e in ALL_ERRORS {
                let plain = format!("{e}");
                assert_eq!(format!("{e:>512}"), plain);
                assert_eq!(format!("{e:.0}"), plain);
                assert!(!format!("{e:?}").is_empty());
            }
        }

        #[test]
        fn derived_ord_is_total_and_matches_declaration_order() {
            for (i, a) in ALL_ERRORS.iter().enumerate() {
                for (j, b) in ALL_ERRORS.iter().enumerate() {
                    assert_eq!(a.cmp(b), i.cmp(&j), "Ord disagrees for {a:?} vs {b:?}");
                    assert_eq!(a.partial_cmp(b), Some(a.cmp(b)));
                }
            }
        }

        // --- translate_rawimage_colortype (other) -------------------------------

        #[test]
        fn colortype_mapping_is_exhaustive_and_stable() {
            use image::ColorType::{L16, L8, La16, La8, Rgb16, Rgb32F, Rgb8, Rgba16, Rgba32F, Rgba8};

            let expected = [
                (RawImageFormat::R8, L8),
                (RawImageFormat::RG8, La8),
                (RawImageFormat::RGB8, Rgb8),
                (RawImageFormat::RGBA8, Rgba8),
                (RawImageFormat::R16, L16),
                (RawImageFormat::RG16, La16),
                (RawImageFormat::RGB16, Rgb16),
                (RawImageFormat::RGBA16, Rgba16),
                (RawImageFormat::BGR8, Rgb8),
                (RawImageFormat::BGRA8, Rgba8),
                (RawImageFormat::RGBF32, Rgb32F),
                (RawImageFormat::RGBAF32, Rgba32F),
            ];
            assert_eq!(expected.len(), ALL_FORMATS.len(), "a RawImageFormat variant is untested");
            for (format, want) in expected {
                assert_eq!(
                    translate_rawimage_colortype(format),
                    want,
                    "wrong ColorType for {format:?}"
                );
            }
        }

        #[test]
        fn colortype_maps_bgr_onto_its_rgb_counterpart() {
            // The BGR formats intentionally alias their RGB counterparts; the byte swap is
            // handled separately by `bgr_to_rgb_swap`.
            assert_eq!(
                translate_rawimage_colortype(RawImageFormat::BGR8),
                translate_rawimage_colortype(RawImageFormat::RGB8)
            );
            assert_eq!(
                translate_rawimage_colortype(RawImageFormat::BGRA8),
                translate_rawimage_colortype(RawImageFormat::RGBA8)
            );
        }

        #[test]
        fn colortype_is_usable_in_const_context() {
            const C: image::ColorType = translate_rawimage_colortype(RawImageFormat::RGBA16);
            assert_eq!(C, image::ColorType::Rgba16);
        }

        #[test]
        fn colortype_channel_count_matches_the_source_format() {
            for f in ALL_FORMATS {
                let channels = translate_rawimage_colortype(f).channel_count();
                let want = match f {
                    RawImageFormat::R8 | RawImageFormat::R16 => 1,
                    RawImageFormat::RG8 | RawImageFormat::RG16 => 2,
                    RawImageFormat::RGB8
                    | RawImageFormat::BGR8
                    | RawImageFormat::RGB16
                    | RawImageFormat::RGBF32 => 3,
                    RawImageFormat::RGBA8
                    | RawImageFormat::BGRA8
                    | RawImageFormat::RGBA16
                    | RawImageFormat::RGBAF32 => 4,
                };
                assert_eq!(channels, want, "channel count mismatch for {f:?}");
            }
        }

        // --- bgr_to_rgb_swap (parser) -------------------------------------------

        #[test]
        fn swap_returns_none_for_every_non_bgr_format() {
            for f in ALL_FORMATS {
                if matches!(f, RawImageFormat::BGR8 | RawImageFormat::BGRA8) {
                    continue;
                }
                assert_eq!(bgr_to_rgb_swap(&[1, 2, 3, 4, 5, 6], f), None, "{f:?} should not swap");
                assert_eq!(bgr_to_rgb_swap(&[], f), None, "{f:?} should not swap (empty)");
            }
        }

        #[test]
        fn swap_of_empty_input_is_some_empty_not_none() {
            // Empty input is *not* an error here: the format decides Some/None.
            assert_eq!(bgr_to_rgb_swap(&[], RawImageFormat::BGR8), Some(Vec::new()));
            assert_eq!(bgr_to_rgb_swap(&[], RawImageFormat::BGRA8), Some(Vec::new()));
        }

        #[test]
        fn swap_bgr8_swaps_red_and_blue_only() {
            assert_eq!(
                bgr_to_rgb_swap(&[1, 2, 3, 4, 5, 6], RawImageFormat::BGR8),
                Some(vec![3, 2, 1, 6, 5, 4])
            );
        }

        #[test]
        fn swap_bgra8_preserves_the_alpha_channel() {
            assert_eq!(
                bgr_to_rgb_swap(&[1, 2, 3, 200, 4, 5, 6, 201], RawImageFormat::BGRA8),
                Some(vec![3, 2, 1, 200, 6, 5, 4, 201])
            );
        }

        /// A pixel buffer whose length is not a whole number of pixels: `chunks_exact_mut`
        /// silently drops the remainder, so the trailing bytes pass through unswapped.
        #[test]
        fn swap_leaves_a_trailing_partial_pixel_untouched() {
            assert_eq!(bgr_to_rgb_swap(&[1], RawImageFormat::BGR8), Some(vec![1]));
            assert_eq!(bgr_to_rgb_swap(&[1, 2], RawImageFormat::BGR8), Some(vec![1, 2]));
            assert_eq!(
                bgr_to_rgb_swap(&[1, 2, 3, 4], RawImageFormat::BGR8),
                Some(vec![3, 2, 1, 4])
            );
            for len in 0..4usize {
                let input = (0..len as u8).collect::<Vec<_>>();
                assert_eq!(
                    bgr_to_rgb_swap(&input, RawImageFormat::BGRA8),
                    Some(input.clone()),
                    "a partial BGRA8 pixel of {len} byte(s) must pass through unchanged"
                );
            }
            assert_eq!(
                bgr_to_rgb_swap(&[1, 2, 3, 4, 5, 6, 7], RawImageFormat::BGRA8),
                Some(vec![3, 2, 1, 4, 5, 6, 7])
            );
        }

        #[test]
        fn swap_never_changes_the_length_and_is_its_own_inverse() {
            for (format, stride) in [(RawImageFormat::BGR8, 3usize), (RawImageFormat::BGRA8, 4)] {
                for len in 0..64usize {
                    let input = (0..len).map(|i| (i % 251) as u8).collect::<Vec<_>>();
                    let once = bgr_to_rgb_swap(&input, format).expect("BGR format must swap");
                    assert_eq!(once.len(), input.len(), "length changed for {format:?}, len {len}");
                    let twice = bgr_to_rgb_swap(&once, format).expect("BGR format must swap");
                    assert_eq!(twice, input, "swap is not an involution for {format:?}, len {len}");
                    // Only whole pixels move; the tail is untouched.
                    let tail = len - (len / stride) * stride;
                    assert_eq!(once[len - tail..], input[len - tail..]);
                }
            }
        }

        #[test]
        fn swap_handles_arbitrary_non_utf8_and_unicode_bytes() {
            assert_eq!(
                bgr_to_rgb_swap(&[0xFF, 0xFE, 0x00], RawImageFormat::BGR8),
                Some(vec![0x00, 0xFE, 0xFF])
            );
            // Bytes of "\u{1F600}" (F0 9F 98 80) reinterpreted as one BGRA8 pixel.
            assert_eq!(
                bgr_to_rgb_swap("\u{1F600}".as_bytes(), RawImageFormat::BGRA8),
                Some(vec![0x98, 0x9F, 0xF0, 0x80])
            );
            assert_eq!(
                bgr_to_rgb_swap(b"   \t\n", RawImageFormat::BGR8),
                Some(vec![b' ', b' ', b' ', b'\t', b'\n'])
            );
            assert_eq!(
                bgr_to_rgb_swap(&[0, 0, 0, 255, 255, 255], RawImageFormat::BGR8),
                Some(vec![0, 0, 0, 255, 255, 255])
            );
        }

        #[test]
        fn swap_of_a_very_large_buffer_does_not_hang() {
            const LEN: usize = 3 * 333_333 + 2; // ~1 MB, deliberately not pixel-aligned
            let input = (0..LEN).map(|i| (i % 256) as u8).collect::<Vec<_>>();
            let out = bgr_to_rgb_swap(&input, RawImageFormat::BGR8).expect("BGR8 must swap");
            assert_eq!(out.len(), LEN);
            assert_eq!(out[0], input[2]);
            assert_eq!(out[2], input[0]);
            assert_eq!(out[LEN - 1], input[LEN - 1]); // partial trailing pixel kept as-is
            assert_eq!(out[LEN - 2], input[LEN - 2]);
        }

        // --- translate_image_error_encode (other) --------------------------------

        #[test]
        fn translate_maps_limit_kinds() {
            assert_eq!(
                translate_image_error_encode(ImageError::Limits(LimitError::from_kind(
                    LimitErrorKind::InsufficientMemory
                ))),
                EncodeImageError::InsufficientMemory
            );
            assert_eq!(
                translate_image_error_encode(ImageError::Limits(LimitError::from_kind(
                    LimitErrorKind::DimensionError
                ))),
                EncodeImageError::DimensionError
            );
            assert_eq!(
                translate_image_error_encode(ImageError::Limits(LimitError::from_kind(
                    LimitErrorKind::Unsupported {
                        limits: image::Limits::default(),
                        supported: image::LimitSupport::default(),
                    }
                ))),
                EncodeImageError::Unknown
            );
        }

        #[test]
        fn translate_maps_every_other_variant_to_unknown() {
            let cases = vec![
                ImageError::IoError(std::io::Error::other("boom")),
                ImageError::Encoding(EncodingError::new(ImageFormatHint::Unknown, "bad pixel")),
                ImageError::Decoding(DecodingError::new(ImageFormatHint::Unknown, "bad chunk")),
                // An unsupported *color type* (what the JPEG encoder returns for RGBA8) is
                // flattened to `Unknown`, never to `InvalidData`.
                ImageError::Unsupported(UnsupportedError::from_format_and_kind(
                    ImageFormatHint::Unknown,
                    UnsupportedErrorKind::Format(ImageFormatHint::Unknown),
                )),
                // ... and a parameter dimension mismatch is NOT mapped to DimensionError.
                ImageError::Parameter(ParameterError::from_kind(
                    ParameterErrorKind::DimensionMismatch,
                )),
            ];
            for c in cases {
                let debug = format!("{c:?}");
                assert_eq!(
                    translate_image_error_encode(c),
                    EncodeImageError::Unknown,
                    "unexpected mapping for {debug}"
                );
            }
        }

        // --- encode_png ----------------------------------------------------------

        #[cfg(feature = "png")]
        #[test]
        fn encode_png_round_trips_through_the_decoder() {
            use crate::image::decode::decode_raw_image_from_any_bytes;

            for (format, bpp) in [
                (RawImageFormat::R8, 1usize),
                (RawImageFormat::RGB8, 3),
                (RawImageFormat::RGBA8, 4),
            ] {
                let pixels = (0..(2 * 2 * bpp)).map(|i| (i * 7 + 1) as u8).collect::<Vec<_>>();
                let encoded = encode_png(&raw_u8(2, 2, format, pixels.clone()))
                    .into_result()
                    .unwrap_or_else(|e| panic!("encode_png({format:?}) failed: {e}"));
                let bytes = encoded.as_slice();
                assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "missing PNG signature");

                let decoded = decode_raw_image_from_any_bytes(bytes)
                    .into_result()
                    .unwrap_or_else(|e| panic!("decode of our own PNG ({format:?}) failed: {e}"));
                assert_eq!(decoded.width, 2);
                assert_eq!(decoded.height, 2);
                assert_eq!(decoded.data_format, format, "format changed across the round trip");
                assert!(!decoded.premultiplied_alpha);
                assert_eq!(
                    decoded.pixels.get_u8_vec_ref().expect("8-bit data").as_slice(),
                    pixels.as_slice(),
                    "pixels changed across the round trip ({format:?})"
                );
            }
        }

        /// BGR input must come back out of the decoder as RGB with the channels swapped
        /// (PNG has no BGR color type -- `bgr_to_rgb_swap` is what makes this correct).
        #[cfg(feature = "png")]
        #[test]
        fn encode_png_swaps_bgr_channels_before_writing() {
            use crate::image::decode::decode_raw_image_from_any_bytes;

            let bgra = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160];
            let encoded = encode_png(&raw_u8(2, 2, RawImageFormat::BGRA8, bgra.clone()))
                .into_result()
                .expect("encode_png(BGRA8) failed");
            let decoded = decode_raw_image_from_any_bytes(encoded.as_slice())
                .into_result()
                .expect("decode of our own BGRA8 PNG failed");
            assert_eq!(decoded.data_format, RawImageFormat::RGBA8);
            let want = bgr_to_rgb_swap(&bgra, RawImageFormat::BGRA8).expect("BGRA8 must swap");
            assert_eq!(
                decoded.pixels.get_u8_vec_ref().expect("8-bit data").as_slice(),
                want.as_slice()
            );

            let bgr = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120];
            let encoded = encode_png(&raw_u8(2, 2, RawImageFormat::BGR8, bgr.clone()))
                .into_result()
                .expect("encode_png(BGR8) failed");
            let decoded = decode_raw_image_from_any_bytes(encoded.as_slice())
                .into_result()
                .expect("decode of our own BGR8 PNG failed");
            assert_eq!(decoded.data_format, RawImageFormat::RGB8);
            let want = bgr_to_rgb_swap(&bgr, RawImageFormat::BGR8).expect("BGR8 must swap");
            assert_eq!(
                decoded.pixels.get_u8_vec_ref().expect("8-bit data").as_slice(),
                want.as_slice()
            );
        }

        /// Non-8-bit payloads have no `get_u8_vec_ref()`, so they must be rejected as
        /// `InvalidData` -- even though `translate_rawimage_colortype` happily maps them.
        #[cfg(feature = "png")]
        #[test]
        fn encode_png_rejects_16_bit_and_float_payloads() {
            let u16_img = RawImage {
                pixels: RawImageData::U16(vec![0u16; 4].into()),
                width: 2,
                height: 2,
                premultiplied_alpha: false,
                data_format: RawImageFormat::R16,
                tag: Vec::<u8>::new().into(),
            };
            assert_eq!(err_of(&encode_png(&u16_img)), EncodeImageError::InvalidData);

            let f32_img = RawImage {
                pixels: RawImageData::F32(vec![0.0f32, f32::NAN, f32::INFINITY, f32::MIN].into()),
                width: 2,
                height: 2,
                premultiplied_alpha: false,
                data_format: RawImageFormat::RGBAF32,
                tag: Vec::<u8>::new().into(),
            };
            assert_eq!(err_of(&encode_png(&f32_img)), EncodeImageError::InvalidData);
        }

        #[cfg(all(feature = "png", target_pointer_width = "64"))]
        #[test]
        fn encode_png_rejects_dimensions_that_overflow_u32() {
            let too_wide = raw_u8(usize::MAX, 1, RawImageFormat::RGBA8, vec![0; 4]);
            assert_eq!(err_of(&encode_png(&too_wide)), EncodeImageError::DimensionError);

            let too_tall = raw_u8(1, (u32::MAX as usize) + 1, RawImageFormat::RGBA8, vec![0; 4]);
            assert_eq!(err_of(&encode_png(&too_tall)), EncodeImageError::DimensionError);
        }

        #[cfg(feature = "png")]
        #[test]
        fn encode_png_rejects_zero_dimensions() {
            let zero = raw_u8(0, 0, RawImageFormat::RGBA8, Vec::new());
            assert!(encode_png(&zero).is_err(), "a 0x0 PNG is not a valid image");

            let zero_width = raw_u8(0, 4, RawImageFormat::RGBA8, Vec::new());
            assert!(encode_png(&zero_width).is_err());
        }

        /// ADVERSARIAL: `image`'s `PngEncoder::write_image` *asserts* that the buffer length
        /// matches `width * height * bpp`, and `encode_png` forwards an unvalidated buffer.
        /// A short/long `RawImage` therefore aborts the process instead of returning `Err`.
        /// The invariant asserted here is the weaker one that always holds: it must never
        /// report success. See the report for the underlying bug.
        #[cfg(feature = "png")]
        #[test]
        fn encode_png_never_succeeds_on_a_buffer_of_the_wrong_length() {
            for (w, h, format, len) in [
                (4usize, 4usize, RawImageFormat::RGBA8, 3usize), // far too short
                (2, 2, RawImageFormat::RGBA8, 15),               // one byte short
                (2, 2, RawImageFormat::RGBA8, 17),               // one byte long
                (2, 2, RawImageFormat::R8, 0),                   // empty, non-zero dimensions
            ] {
                let outcome = std::panic::catch_unwind(move || {
                    encode_png(&raw_u8(w, h, format, vec![0u8; len])).is_ok()
                });
                assert!(
                    !matches!(outcome, Ok(true)),
                    "encode_png claimed success for {w}x{h} {format:?} with {len} byte(s)"
                );
            }
        }

        #[cfg(not(feature = "png"))]
        #[test]
        fn encode_png_without_the_feature_always_reports_encoder_not_available() {
            for img in [
                raw_u8(2, 2, RawImageFormat::RGBA8, vec![0; 16]),
                raw_u8(0, 0, RawImageFormat::RGBA8, Vec::new()),
                raw_u8(usize::MAX, usize::MAX, RawImageFormat::BGRA8, vec![0; 3]),
            ] {
                assert_eq!(err_of(&encode_png(&img)), EncodeImageError::EncoderNotAvailable);
            }
        }

        // --- encode_jpeg (numeric: `quality`) ------------------------------------

        /// `JpegEncoder::new_with_quality` clamps the quality to 1..=100 internally, so the
        /// out-of-range ends must not panic and must be indistinguishable from the clamped value.
        #[cfg(feature = "jpeg")]
        #[test]
        fn encode_jpeg_clamps_quality_at_both_ends() {
            let pixels = (0..(4 * 4 * 3)).map(|i| (i * 5) as u8).collect::<Vec<_>>();
            let at = |q: u8| {
                encode_jpeg(&raw_u8(4, 4, RawImageFormat::RGB8, pixels.clone()), q)
                    .into_result()
                    .unwrap_or_else(|e| panic!("encode_jpeg(quality = {q}) failed: {e}"))
                    .as_slice()
                    .to_vec()
            };

            assert_eq!(at(u8::MIN), at(1), "quality 0 must clamp to 1");
            assert_eq!(at(u8::MAX), at(100), "quality 255 must clamp to 100");
            assert_ne!(at(1), at(100), "quality must actually affect the output");

            for q in [0u8, 1, 50, 100, 101, 254, 255] {
                let bytes = at(q);
                assert_eq!(&bytes[..2], &[0xFF, 0xD8], "missing JPEG SOI marker (quality {q})");
                assert_eq!(
                    &bytes[bytes.len() - 2..],
                    &[0xFF, 0xD9],
                    "missing JPEG EOI marker (quality {q})"
                );
            }
        }

        #[cfg(feature = "jpeg")]
        #[test]
        fn encode_jpeg_round_trips_dimensions_through_the_decoder() {
            use crate::image::decode::decode_raw_image_from_any_bytes;

            let pixels = (0..(8 * 8 * 3)).map(|i| (i % 256) as u8).collect::<Vec<_>>();
            let encoded = encode_jpeg(&raw_u8(8, 8, RawImageFormat::RGB8, pixels), 90)
                .into_result()
                .expect("encode_jpeg(RGB8) failed");
            // JPEG is lossy, so only the geometry/format survives -- not the exact pixels.
            let decoded = decode_raw_image_from_any_bytes(encoded.as_slice())
                .into_result()
                .expect("decode of our own JPEG failed");
            assert_eq!(decoded.width, 8);
            assert_eq!(decoded.height, 8);
            assert_eq!(decoded.data_format, RawImageFormat::RGB8);
            assert_eq!(decoded.pixels.get_u8_vec_ref().expect("8-bit data").len(), 8 * 8 * 3);
        }

        /// The JPEG encoder only supports L8 and Rgb8. Everything else comes back as an
        /// `Unsupported` image error, which the wrapper flattens to `Unknown`.
        #[cfg(feature = "jpeg")]
        #[test]
        fn encode_jpeg_rejects_color_types_it_cannot_write() {
            for (format, bpp) in [
                (RawImageFormat::RGBA8, 4usize),
                (RawImageFormat::BGRA8, 4),
                (RawImageFormat::RG8, 2),
            ] {
                let img = raw_u8(2, 2, format, vec![7u8; 2 * 2 * bpp]);
                assert_eq!(
                    err_of(&encode_jpeg(&img, 80)),
                    EncodeImageError::Unknown,
                    "unexpected error for {format:?}"
                );
            }
            // ... while the two supported ones do encode.
            assert!(encode_jpeg(&raw_u8(2, 2, RawImageFormat::R8, vec![1; 4]), 80).is_ok());
            assert!(encode_jpeg(&raw_u8(2, 2, RawImageFormat::RGB8, vec![1; 12]), 80).is_ok());
        }

        #[cfg(feature = "jpeg")]
        #[test]
        fn encode_jpeg_rejects_zero_and_u16_max_plus_one_dimensions() {
            // JPEG dimensions are u16 in the frame header: 0 and > 65535 must both fail,
            // and must fail as an error rather than a panic.
            assert!(encode_jpeg(&raw_u8(0, 0, RawImageFormat::RGB8, Vec::new()), 75).is_err());
            assert!(encode_jpeg(&raw_u8(0, 4, RawImageFormat::RGB8, Vec::new()), 75).is_err());

            let too_wide = raw_u8(65_536, 1, RawImageFormat::RGB8, vec![0u8; 65_536 * 3]);
            // NOTE: reported as `Unknown`, not `DimensionError` -- the wrapper only maps
            // `ImageError::Limits`, and the JPEG encoder raises an encoding error instead.
            assert_eq!(err_of(&encode_jpeg(&too_wide, 75)), EncodeImageError::Unknown);
        }

        #[cfg(all(feature = "jpeg", target_pointer_width = "64"))]
        #[test]
        fn encode_jpeg_rejects_dimensions_that_overflow_u32() {
            let too_wide = raw_u8(usize::MAX, 1, RawImageFormat::RGB8, vec![0; 3]);
            assert_eq!(err_of(&encode_jpeg(&too_wide, 75)), EncodeImageError::DimensionError);

            let too_tall = raw_u8(1, (u32::MAX as usize) + 1, RawImageFormat::RGB8, vec![0; 3]);
            assert_eq!(err_of(&encode_jpeg(&too_tall, 0)), EncodeImageError::DimensionError);
        }

        #[cfg(feature = "jpeg")]
        #[test]
        fn encode_jpeg_rejects_16_bit_payloads_before_looking_at_quality() {
            let u16_img = RawImage {
                pixels: RawImageData::U16(vec![u16::MAX; 12].into()),
                width: 2,
                height: 2,
                premultiplied_alpha: false,
                data_format: RawImageFormat::RGB16,
                tag: Vec::<u8>::new().into(),
            };
            for q in [0u8, 50, 255] {
                assert_eq!(err_of(&encode_jpeg(&u16_img, q)), EncodeImageError::InvalidData);
            }
        }

        /// Same unvalidated-buffer hazard as `encode_png`, at every quality boundary.
        #[cfg(feature = "jpeg")]
        #[test]
        fn encode_jpeg_never_succeeds_on_a_buffer_of_the_wrong_length() {
            for q in [0u8, 1, 100, 255] {
                for (w, h, len) in [(4usize, 4usize, 3usize), (2, 2, 11), (2, 2, 13)] {
                    let outcome = std::panic::catch_unwind(move || {
                        encode_jpeg(&raw_u8(w, h, RawImageFormat::RGB8, vec![0u8; len]), q).is_ok()
                    });
                    assert!(
                        !matches!(outcome, Ok(true)),
                        "encode_jpeg claimed success for {w}x{h} RGB8, {len} byte(s), quality {q}"
                    );
                }
            }
        }

        #[cfg(not(feature = "jpeg"))]
        #[test]
        fn encode_jpeg_without_the_feature_always_reports_encoder_not_available() {
            // Every quality boundary, including the ones the real encoder would clamp.
            for q in [u8::MIN, 1, 50, 100, 101, 254, u8::MAX] {
                let img = raw_u8(2, 2, RawImageFormat::RGB8, vec![0; 12]);
                assert_eq!(err_of(&encode_jpeg(&img, q)), EncodeImageError::EncoderNotAvailable);
            }
            // ... and for images that the real encoder would reject outright.
            let broken = raw_u8(usize::MAX, 0, RawImageFormat::RGBAF32, Vec::new());
            assert_eq!(err_of(&encode_jpeg(&broken, 0)), EncodeImageError::EncoderNotAvailable);
        }
    }
}
