//! A build with azul-layout's DEFAULT features decodes what `cargo add azul-layout` promises. The
//! dll turns the ten image formats on itself (`all_img_formats`); azul-layout's own default had
//! `image_decoding` and none of them, so a direct user decoded no picture at all.

use azul_core::resources::{RawImage, RawImageData, RawImageFormat};

use super::{
    decode::decode_raw_image_from_any_bytes,
    encode::{
        encode_bmp, encode_gif, encode_jpeg, encode_png, encode_pnm, encode_tga, encode_tiff,
        ResultU8VecEncodeImageError,
    },
};

/// A 4 x 3 picture of RGB triples that differ from pixel to pixel.
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

fn decoded_size(name: &str, encoded: ResultU8VecEncodeImageError) -> (usize, usize) {
    let bytes = encoded
        .into_result()
        .unwrap_or_else(|e| panic!("{name}: this build has no encoder ({e})"));
    let decoded = decode_raw_image_from_any_bytes(bytes.as_slice())
        .into_result()
        .unwrap_or_else(|e| panic!("{name}: this build cannot decode its own file ({e})"));
    (decoded.width, decoded.height)
}

#[test]
fn the_default_features_encode_and_decode_every_format_the_dll_turns_on() {
    let image = picture();
    for (name, encoded) in [
        ("png", encode_png(&image)),
        ("jpeg", encode_jpeg(&image, 90)),
        ("gif", encode_gif(&image)),
        ("bmp", encode_bmp(&image)),
        ("tga", encode_tga(&image)),
        ("tiff", encode_tiff(&image)),
        ("pnm", encode_pnm(&image)),
    ] {
        assert_eq!(decoded_size(name, encoded), (4, 3), "{name}");
    }
}
