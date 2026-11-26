//! A TGA file has no magic number, so it is recognised by elimination: what nothing else claims
//! is read as TGA, and a file that is not one is still an error.

use azul_core::resources::{RawImage, RawImageData, RawImageFormat};

use super::decode::decode_raw_image_from_any_bytes;
#[cfg(feature = "tga")]
use super::encode::encode_tga;

/// A 4 x 3 picture of RGB triples that differ from pixel to pixel.
#[cfg(feature = "tga")]
fn picture() -> RawImage {
    let pixels: Vec<u8> = (0..4 * 3 * 3).map(|i| (i * 9 + 5) as u8).collect();
    RawImage {
        pixels: RawImageData::U8(pixels.into()),
        width: 4,
        height: 3,
        premultiplied_alpha: false,
        data_format: RawImageFormat::RGB8,
        tag: Vec::<u8>::new().into(),
    }
}

#[cfg(feature = "tga")]
#[test]
fn a_tga_file_decodes_to_the_picture_that_was_encoded() {
    let image = picture();
    let bytes = encode_tga(&image)
        .into_result()
        .expect("a build with the tga feature encodes TGA");
    let decoded = decode_raw_image_from_any_bytes(bytes.as_slice())
        .into_result()
        .unwrap_or_else(|e| panic!("the TGA file does not decode: {e}"));
    assert_eq!((decoded.width, decoded.height), (4, 3));
    assert_eq!(decoded.data_format, RawImageFormat::RGB8);
    let (RawImageData::U8(got), RawImageData::U8(sent)) = (&decoded.pixels, &image.pixels) else {
        panic!("8-bit samples in and out");
    };
    assert_eq!(got.as_ref(), sent.as_ref(), "every sample comes back");
}

#[test]
fn bytes_that_are_no_picture_are_an_error_and_never_a_tga() {
    let zeros = [0u8; 64];
    let ones = [0xffu8; 300];
    for bytes in [
        &b""[..],
        &b"hello, this is text and not a picture"[..],
        &zeros[..],
        &ones[..],
    ] {
        assert!(
            decode_raw_image_from_any_bytes(bytes)
                .into_result()
                .is_err(),
            "{} bytes are no picture",
            bytes.len()
        );
    }
}
