//! Bitmaps and the options for writing them.
//!
//! The types here are available without any feature. Decoding image files and encoding them
//! again needs the `images` feature, plus one feature per file format (`png`, `jpeg`, `gif`,
//! `bmp`, `tiff`, `webp`, ...). Each format feature enables `images`.
//!
//! # How a PDF stores an image
//!
//! A bitmap is an *image XObject* (see [`xobject`](crate::xobject)): a stream of samples and a
//! dictionary that says how to read them (ISO 32000-1, 8.9.5):
//!
//! - `/Width` and `/Height` in pixels,
//! - `/ColorSpace`, here `DeviceGray` or `DeviceRGB`, and `/BitsPerComponent` (1, 8 or 16),
//! - `/Filter`, how the stream bytes are compressed: `FlateDecode` (lossless, zlib) or
//!   `DCTDecode` (JPEG, lossy).
//!
//! The pixels have no alpha channel. Transparency is a second, greyscale image attached as a
//! *soft mask* (`/SMask`, ISO 32000-1, 11.6.5.3).
//!
//! # How this crate models it
//!
//! A [`RawImage`] holds decoded pixels: a [`RawImageData`] buffer, the size and a
//! [`RawImageFormat`] that names the channel layout. Make one with
//! [`RawImage::decode_from_bytes`] or fill the struct yourself. Register it with
//! [`PdfDocument::add_image`](crate::PdfDocument::add_image), place it with
//! [`Op::UseXobject`](crate::Op::UseXobject).
//!
//! The pixels are encoded when the document is saved: the alpha channel becomes a soft mask and
//! [`PdfSaveOptions::image_optimization`](crate::PdfSaveOptions::image_optimization)
//! ([`ImageOptimizationOptions`]) chooses the filter and whether the image is shrunk. Without
//! the `images` feature, pixels are written as 8-bit samples with `FlateDecode`.
//! `optimize_images` applies the same encoding to a finished PDF.

use serde_derive::{Deserialize, Serialize};

/// How images are encoded when a document is saved: compression, quality and shrinking.
///
/// [`PdfSaveOptions::default`](crate::PdfSaveOptions) uses the defaults of this type, which are
/// lossy: color images are written as JPEG at quality 0.85 (`jpeg` feature) and an image whose
/// uncompressed size exceeds 2 MB is scaled down. For lossless output set `format` to
/// [`ImageCompression::Flate`] and `max_image_size` to `None`. Without options
/// (`image_optimization: None`) the pixels are written unfiltered.
///
/// A field that is missing in JSON takes its default, and a `None` field is left out of the JSON.
#[derive(Debug, Clone, Serialize, PartialOrd, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageOptimizationOptions {
    /// JPEG quality from 0.0 (smallest) to 1.0 (best), default 0.85. Only used for JPEG.
    #[serde(default = "default_quality", skip_serializing_if = "Option::is_none")]
    pub quality: Option<f32>,
    /// Largest uncompressed size of one image, as text: a number and a unit of `b`, `kb`, `mb`
    /// or `gb` (1024 based), such as `"300kb"`. Default `"2MB"`. A larger image is scaled down,
    /// keeping its aspect ratio, with nearest-neighbour sampling. Text that does not parse
    /// disables the limit.
    #[serde(
        default = "default_max_img_size",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_image_size: Option<String>,
    /// Turns greyscale images into pure black and white with Floyd-Steinberg dithering, so they
    /// are written with 1 bit per pixel. For scans of line art. Default `None` (off).
    #[serde(
        default = "default_dither_greyscale",
        skip_serializing_if = "Option::is_none"
    )]
    pub dither_greyscale: Option<bool>,
    /// Converts color images to greyscale. Only has an effect when `auto_optimize` is true.
    /// Default `Some(false)`.
    #[serde(
        default = "default_convert_to_greyscale",
        skip_serializing_if = "Option::is_none"
    )]
    pub convert_to_greyscale: Option<bool>,
    /// Drops the alpha channel of fully opaque images and converts color images whose channels
    /// differ by at most 3 of 255 (about 1%) everywhere to greyscale. Default `Some(true)`.
    #[serde(
        default = "default_auto_optimize",
        skip_serializing_if = "Option::is_none"
    )]
    pub auto_optimize: Option<bool>,
    /// Compression to use, default [`ImageCompression::Auto`].
    #[serde(default = "default_format", skip_serializing_if = "Option::is_none")]
    pub format: Option<ImageCompression>,
}

const fn default_quality() -> Option<f32> {
    Some(0.85)
}

fn default_max_img_size() -> Option<String> {
    Some("2MB".to_string())
}

const fn default_convert_to_greyscale() -> Option<bool> {
    Some(false)
}

const fn default_dither_greyscale() -> Option<bool> {
    None
}

const fn default_auto_optimize() -> Option<bool> {
    Some(true)
}

const fn default_format() -> Option<ImageCompression> {
    Some(ImageCompression::Auto)
}

impl Default for ImageOptimizationOptions {
    fn default() -> Self {
        ImageOptimizationOptions {
            quality: default_quality(),
            convert_to_greyscale: default_convert_to_greyscale(),
            max_image_size: default_max_img_size(),
            dither_greyscale: default_dither_greyscale(),
            auto_optimize: default_auto_optimize(),
            format: default_format(),
        }
    }
}

/// The compression of an image stream. The soft mask (alpha channel) of an image is always
/// written with Flate.
#[derive(Debug, Copy, Clone, Serialize, PartialOrd, PartialEq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageCompression {
    /// Lossy JPEG for color images (needs the `jpeg` feature, else Flate), lossless Flate for
    /// greyscale images.
    #[default]
    Auto,
    /// Lossy JPEG (`DCTDecode`). Needs the `jpeg` feature, else Flate is used.
    Jpeg,
    /// Written as JPEG (`DCTDecode`), as there is no JPEG 2000 encoder.
    Jpeg2000,
    /// Lossless zlib compression (`FlateDecode`).
    Flate,
    /// Written as Flate (`FlateDecode`), as there is no LZW encoder.
    Lzw,
    /// Written as Flate (`FlateDecode`), as there is no run-length encoder.
    RunLength,
    /// No filter: the samples are stored as they are.
    None,
}

/// An image file format that [`RawImage::encode_to_bytes`] can write.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputImageFormat {
    /// PNG, lossless.
    Png,
    /// JPEG, lossy.
    Jpeg,
    /// WebP.
    Webp,
    /// AVIF.
    Avif,
    /// GIF, at most 256 colors.
    Gif,
    /// BMP, uncompressed.
    Bmp,
    /// TIFF.
    Tiff,
    /// TGA.
    Tga,
    /// PNM (portable anymap).
    Pnm,
}

impl OutputImageFormat {
    /// The MIME type of the format, such as `"image/png"`.
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
            Self::Avif => "image/avif",
            Self::Gif => "image/gif",
            Self::Bmp => "image/bmp",
            Self::Tiff => "image/tiff",
            Self::Tga => "image/x-tga",
            Self::Pnm => "image/x-portable-anymap",
        }
    }
}

/// The channel layout and sample type of a [`RawImage`]. Samples of a pixel are stored next to
/// each other, pixels row by row from the top left corner, with no padding between rows.
///
/// The sample type must match [`RawImageData`]: 8-bit formats use `U8`, 16-bit formats `U16`,
/// float formats `F32` (0.0 to 1.0).
#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[serde(rename_all = "lowercase")]
pub enum RawImageFormat {
    /// One grey channel, 8 bits.
    R8,
    /// Grey and alpha (not red and green), 8 bits each.
    RG8,
    /// Red, green, blue, 8 bits each.
    RGB8,
    /// Red, green, blue, alpha, 8 bits each.
    RGBA8,
    /// One grey channel, 16 bits.
    R16,
    /// Grey and alpha, 16 bits each.
    RG16,
    /// Red, green, blue, 16 bits each.
    RGB16,
    /// Red, green, blue, alpha, 16 bits each.
    RGBA16,
    /// Blue, green, red, 8 bits each.
    BGR8,
    /// Blue, green, red, alpha, 8 bits each.
    BGRA8,
    /// Red, green, blue as 32-bit floats.
    RGBF32,
    /// Red, green, blue, alpha as 32-bit floats.
    RGBAF32,
}

/// The samples of a [`RawImage`], in the order [`RawImageFormat`] describes.
///
/// Serialized as `{"u8b64": "<base64>"}`, `{"u16b64": ...}` or `{"f32b64": ...}`, with 16-bit and
/// float samples packed little-endian. The array forms `{"u8": [1, 2, 3]}`, `{"u16": [...]}` and
/// `{"f32": [...]}` are accepted when reading.
#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "RawImageDataWire", into = "RawImageDataWire")]
pub enum RawImageData {
    /// 8-bit samples.
    U8(Vec<u8>),
    /// 16-bit samples.
    U16(Vec<u16>),
    /// 32-bit float samples.
    F32(Vec<f32>),
}

/// Wire format for [`RawImageData`]. Base64 is smaller and much faster to parse than a JSON
/// array of numbers.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RawImageDataWire {
    U8(Vec<u8>),
    U16(Vec<u16>),
    F32(Vec<f32>),
    U8B64(String),
    U16B64(String),
    F32B64(String),
}

impl From<RawImageData> for RawImageDataWire {
    fn from(d: RawImageData) -> Self {
        use base64::Engine;
        let b64 = |bytes: &[u8]| base64::prelude::BASE64_STANDARD.encode(bytes);
        match d {
            RawImageData::U8(v) => RawImageDataWire::U8B64(b64(&v)),
            RawImageData::U16(v) => {
                let bytes: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
                RawImageDataWire::U16B64(b64(&bytes))
            }
            RawImageData::F32(v) => {
                let bytes: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
                RawImageDataWire::F32B64(b64(&bytes))
            }
        }
    }
}

impl TryFrom<RawImageDataWire> for RawImageData {
    type Error = String;
    fn try_from(w: RawImageDataWire) -> Result<Self, String> {
        use base64::Engine;
        let dec = |s: &str| {
            base64::prelude::BASE64_STANDARD
                .decode(s)
                .map_err(|e| format!("pixel data base64: {e}"))
        };
        Ok(match w {
            RawImageDataWire::U8(v) => RawImageData::U8(v),
            RawImageDataWire::U16(v) => RawImageData::U16(v),
            RawImageDataWire::F32(v) => RawImageData::F32(v),
            RawImageDataWire::U8B64(s) => RawImageData::U8(dec(&s)?),
            RawImageDataWire::U16B64(s) => {
                let b = dec(&s)?;
                if b.len() % 2 != 0 {
                    return Err("u16b64 pixel data has odd byte length".to_string());
                }
                RawImageData::U16(
                    b.as_chunks::<2>().0.iter()
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect(),
                )
            }
            RawImageDataWire::F32B64(s) => {
                let b = dec(&s)?;
                if b.len() % 4 != 0 {
                    return Err("f32b64 pixel data length not a multiple of 4".to_string());
                }
                RawImageData::F32(
                    b.as_chunks::<4>().0.iter()
                        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                        .collect(),
                )
            }
        })
    }
}

#[cfg(test)]
mod pixel_wire_tests {
    use super::*;

    #[test]
    fn pixels_serialize_as_base64_and_accept_legacy_arrays() {
        let d = RawImageData::U8(vec![1, 2, 3, 255]);
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("u8b64"), "new format must be base64: {json}");
        assert_eq!(serde_json::from_str::<RawImageData>(&json).unwrap(), d);
        // the array form is accepted when reading
        let legacy: RawImageData = serde_json::from_str(r#"{"u8":[1,2,3,255]}"#).unwrap();
        assert_eq!(legacy, d);

        let d16 = RawImageData::U16(vec![0x0102, 0xFFEE]);
        let json16 = serde_json::to_string(&d16).unwrap();
        assert_eq!(serde_json::from_str::<RawImageData>(&json16).unwrap(), d16);
    }
}

/// A decoded bitmap: pixels, size and channel layout. Available without the `images` feature.
///
/// `pixels` must hold `width * height * channels` samples, or encoding the image fails or
/// produces garbage.
///
/// # Examples
///
/// Build an image, register it with the document and paint it on a page. Reading an image file
/// instead needs the `images` feature and the feature of its format: see
/// [`RawImage::decode_from_bytes`].
///
/// ```
/// use printpdf::*;
///
/// // 2 x 2 pixels, RGB: red, green / blue, yellow.
/// let image = RawImage {
///     pixels: RawImageData::U8(vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0]),
///     width: 2,
///     height: 2,
///     data_format: RawImageFormat::RGB8,
///     tag: Vec::new(),
/// };
/// let mut doc = PdfDocument::new("Image");
/// let id = doc.add_image(&image);
/// let ops = vec![Op::UseXobject {
///     id,
///     transform: XObjectTransform {
///         dpi: Some(1.44), // 2 px at 1.44 dpi are 100 pt
///         translate_x: Some(Pt(50.0)),
///         translate_y: Some(Pt(600.0)),
///         ..Default::default()
///     },
/// }];
/// let page = PdfPage::new(Mm(210.0), Mm(297.0), ops);
/// let bytes = doc.with_pages(vec![page]).save(&PdfSaveOptions::default(), &mut Vec::new());
/// assert!(bytes.starts_with(b"%PDF"));
/// ```
#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct RawImage {
    /// The samples, row by row from the top left corner.
    pub pixels: RawImageData,
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// The channel layout of `pixels`.
    pub data_format: RawImageFormat,
    /// Free bytes for the caller to label the image. Not written to the PDF.
    pub tag: Vec<u8>,
}

impl RawImage {
    /// Without the `images` feature: always returns an error.
    #[cfg(not(feature = "images"))]
    pub fn decode_from_bytes(_bytes: &[u8], _warnings: &mut Vec<crate::PdfWarnMsg>) -> Result<Self, String> {
        Err("RawImage::decode_from_bytes requires the 'images' feature".to_string())
    }

    /// Without the `images` feature: always returns an error.
    #[cfg(not(feature = "images"))]
    pub async fn decode_from_bytes_async(_bytes: &[u8], _warnings: &mut Vec<crate::PdfWarnMsg>) -> Result<Self, String> {
        Err("RawImage::decode_from_bytes_async requires the 'images' feature".to_string())
    }

    /// Without the `images` feature: returns a 1 x 1 transparent PNG whatever the image is.
    #[cfg(not(feature = "images"))]
    pub fn encode_to_bytes(&self, formats: &[OutputImageFormat]) -> Result<(Vec<u8>, OutputImageFormat), String> {
        let png_data = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1x1 dimensions
            0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89,
            0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, // IDAT chunk
            0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01,
            0x0D, 0x0A, 0x2D, 0xB4,
            0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, // IEND chunk
            0xAE, 0x42, 0x60, 0x82,
        ];
        let format = formats.first().copied().unwrap_or(OutputImageFormat::Png);
        Ok((png_data, format))
    }

    /// Without the `images` feature: same as [`RawImage::encode_to_bytes`].
    #[cfg(not(feature = "images"))]
    pub async fn encode_to_bytes_async(&self, formats: &[OutputImageFormat]) -> Result<(Vec<u8>, OutputImageFormat), String> {
        self.encode_to_bytes(formats)
    }
}

/// Encodes a `RawImage` as an image XObject stream without an image codec: 8-bit samples with
/// `FlateDecode`, so documents with images can be saved when the `images` feature is off.
/// Alpha becomes a greyscale `/SMask`. 16-bit and float samples are reduced to 8 bits.
#[cfg(not(feature = "images"))]
pub(crate) fn raw_image_to_basic_stream(
    im: &RawImage,
    doc: &mut lopdf::Document,
) -> lopdf::Stream {
    use lopdf::Object::{Integer, Name, Reference};

    fn to_u8(px: &RawImageData) -> Vec<u8> {
        match px {
            RawImageData::U8(v) => v.clone(),
            RawImageData::U16(v) => v.iter().map(|x| (x >> 8) as u8).collect(),
            RawImageData::F32(v) => v
                .iter()
                .map(|x| (x.clamp(0.0, 1.0) * 255.0) as u8)
                .collect(),
        }
    }

    let raw = to_u8(&im.pixels);

    // (color bytes per pixel, alpha present, needs BGR swizzle, colorspace)
    let (ncolor, has_alpha, bgr, cs) = match im.data_format {
        RawImageFormat::R8 | RawImageFormat::R16 => (1, false, false, "DeviceGray"),
        RawImageFormat::RG8 | RawImageFormat::RG16 => (1, true, false, "DeviceGray"),
        RawImageFormat::RGB8 | RawImageFormat::RGB16 | RawImageFormat::RGBF32 => {
            (3, false, false, "DeviceRGB")
        }
        RawImageFormat::RGBA8 | RawImageFormat::RGBA16 | RawImageFormat::RGBAF32 => {
            (3, true, false, "DeviceRGB")
        }
        RawImageFormat::BGR8 => (3, false, true, "DeviceRGB"),
        RawImageFormat::BGRA8 => (3, true, true, "DeviceRGB"),
    };

    let stride = ncolor + usize::from(has_alpha);
    // The capacities come from the samples, not from `width` and `height`: they need not agree.
    let pixel_count = raw.len() / stride;
    let mut color = Vec::with_capacity(pixel_count * ncolor);
    let mut alpha = if has_alpha {
        Vec::with_capacity(pixel_count)
    } else {
        Vec::new()
    };
    for px in raw.chunks_exact(stride) {
        match (ncolor, bgr, px) {
            (3, false, [r, g, b, ..]) => color.extend_from_slice(&[*r, *g, *b]),
            (3, true, [b, g, r, ..]) => color.extend_from_slice(&[*r, *g, *b]),
            (_, _, [grey, ..]) => color.push(*grey),
            _ => {}
        }
        if has_alpha {
            if let Some(&a) = px.last() {
                alpha.push(a);
            }
        }
    }

    fn flate(data: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut e =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        let _ = e.write_all(data);
        e.finish().unwrap_or_default()
    }

    let mut dict = lopdf::Dictionary::from_iter(vec![
        ("Type", Name("XObject".into())),
        ("Subtype", Name("Image".into())),
        ("Width", Integer(im.width as i64)),
        ("Height", Integer(im.height as i64)),
        ("BitsPerComponent", Integer(8)),
        ("ColorSpace", Name(cs.into())),
        ("Filter", Name("FlateDecode".into())),
    ]);

    if has_alpha {
        let smask_dict = lopdf::Dictionary::from_iter(vec![
            ("Type", Name("XObject".into())),
            ("Subtype", Name("Image".into())),
            ("Width", Integer(im.width as i64)),
            ("Height", Integer(im.height as i64)),
            ("BitsPerComponent", Integer(8)),
            ("ColorSpace", Name("DeviceGray".into())),
            ("Filter", Name("FlateDecode".into())),
        ]);
        let smask_id = doc.add_object(lopdf::Stream::new(smask_dict, flate(&alpha)));
        dict.set("SMask", Reference(smask_id));
    }

    lopdf::Stream::new(dict, flate(&color)).with_compression(false)
}
