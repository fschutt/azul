//! Image files through azul's codecs: decode any format azul reads (PNG,
//! JPEG, WebP, GIF, BMP, TIFF, ...) into straight RGBA8, encode PNG and JPEG.

use azul::{
    error::{ResultRawImageDecodeImageError, ResultU8VecEncodeImageError},
    image::{ImageRef, RawImage, RawImageData, RawImageFormat},
    vec::U8VecRef,
};

/// Decode an image file to (`width`, `height`, straight RGBA8 rows).
pub fn decode(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    match RawImage::decode_image_bytes_any(U8VecRef::from(bytes)) {
        ResultRawImageDecodeImageError::Ok(image) => {
            to_rgba8(&image).ok_or_else(|| "the image's pixel format is not supported".to_string())
        }
        ResultRawImageDecodeImageError::Err(e) => Err(format!("the image could not be read ({e:?})")),
    }
}

/// Any decoded `RawImage` as straight RGBA8 rows.
#[must_use]
pub fn to_rgba8(image: &RawImage) -> Option<(u32, u32, Vec<u8>)> {
    let (w, h) = (image.width, image.height);
    let n = w.checked_mul(h)?;
    let format = image.data_format;
    let mut out = vec![0u8; n.checked_mul(4)?];
    match &image.pixels {
        RawImageData::U8(bytes) => {
            let src: &[u8] = bytes.as_ref();
            let channels = match format {
                RawImageFormat::R8 => 1,
                RawImageFormat::RG8 => 2,
                RawImageFormat::RGB8 | RawImageFormat::BGR8 => 3,
                RawImageFormat::RGBA8 | RawImageFormat::BGRA8 => 4,
                _ => return None,
            };
            if src.len() < n * channels {
                return None;
            }
            for i in 0..n {
                let s = &src[i * channels..(i + 1) * channels];
                let px = match format {
                    RawImageFormat::R8 => [s[0], s[0], s[0], 255],
                    RawImageFormat::RG8 => [s[0], s[0], s[0], s[1]],
                    RawImageFormat::RGB8 => [s[0], s[1], s[2], 255],
                    RawImageFormat::BGR8 => [s[2], s[1], s[0], 255],
                    RawImageFormat::RGBA8 => [s[0], s[1], s[2], s[3]],
                    _ => [s[2], s[1], s[0], s[3]],
                };
                out[i * 4..i * 4 + 4].copy_from_slice(&px);
            }
        }
        RawImageData::U16(words) => {
            let src: &[u16] = words.as_ref();
            let channels = match format {
                RawImageFormat::R16 => 1,
                RawImageFormat::RG16 => 2,
                RawImageFormat::RGB16 => 3,
                RawImageFormat::RGBA16 => 4,
                _ => return None,
            };
            if src.len() < n * channels {
                return None;
            }
            let b = |v: u16| (v >> 8) as u8;
            for i in 0..n {
                let s = &src[i * channels..(i + 1) * channels];
                let px = match channels {
                    1 => [b(s[0]), b(s[0]), b(s[0]), 255],
                    2 => [b(s[0]), b(s[0]), b(s[0]), b(s[1])],
                    3 => [b(s[0]), b(s[1]), b(s[2]), 255],
                    _ => [b(s[0]), b(s[1]), b(s[2]), b(s[3])],
                };
                out[i * 4..i * 4 + 4].copy_from_slice(&px);
            }
        }
        RawImageData::F32(floats) => {
            let src: &[f32] = floats.as_ref();
            let channels = match format {
                RawImageFormat::RGBF32 => 3,
                RawImageFormat::RGBAF32 => 4,
                _ => return None,
            };
            if src.len() < n * channels {
                return None;
            }
            let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            for i in 0..n {
                let s = &src[i * channels..(i + 1) * channels];
                let a = if channels == 4 { b(s[3]) } else { 255 };
                out[i * 4..i * 4 + 4].copy_from_slice(&[b(s[0]), b(s[1]), b(s[2]), a]);
            }
        }
    }
    if image.premultiplied_alpha {
        for px in out.chunks_exact_mut(4) {
            let a = u32::from(px[3]);
            if a > 0 && a < 255 {
                for c in &mut px[..3] {
                    *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
    }
    Some((w as u32, h as u32, out))
}

/// A straight RGBA8 `RawImage`.
fn raw_rgba(w: u32, h: u32, rgba: Vec<u8>) -> RawImage {
    RawImage::create_rgba8(w, h, rgba.into(), false)
}

fn encoded(result: ResultU8VecEncodeImageError) -> Result<Vec<u8>, String> {
    match result {
        ResultU8VecEncodeImageError::Ok(bytes) => Ok(bytes.as_ref().to_vec()),
        ResultU8VecEncodeImageError::Err(e) => Err(format!("the image could not be written ({e:?})")),
    }
}

/// PNG bytes of straight RGBA8 rows.
pub fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    encoded(raw_rgba(w, h, rgba.to_vec()).encode_png())
}

/// JPEG bytes of straight RGBA8 rows at `quality` (1..100), composited on
/// white (JPEG has no alpha).
pub fn encode_jpeg(w: u32, h: u32, rgba: &[u8], quality: u8) -> Result<Vec<u8>, String> {
    let mut flat = rgba.to_vec();
    for px in flat.chunks_exact_mut(4) {
        let a = u32::from(px[3]);
        for c in &mut px[..3] {
            *c = ((u32::from(*c) * a + 255 * (255 - a) + 127) / 255) as u8;
        }
        px[3] = 255;
    }
    encoded(raw_rgba(w, h, flat).encode_jpeg(quality.clamp(1, 100)))
}

/// The view buffer (BGRA8, opaque) as an image for the canvas node.
#[must_use]
pub fn view_image(w: u32, h: u32, bgra: &[u8]) -> Option<ImageRef> {
    ImageRef::create_rawimage(RawImage {
        pixels: RawImageData::U8(bgra.to_vec().into()),
        width: w as usize,
        height: h as usize,
        premultiplied_alpha: true,
        data_format: RawImageFormat::BGRA8,
        tag: Vec::new().into(),
    })
    .into_option()
}
