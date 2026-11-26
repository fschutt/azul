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

/// The JPEG decoder (`zune-jpeg`, through `image`'s `jpeg`) runs its x86 and NEON SIMD paths: cargo
/// unifies `image`'s own dependency on it, which keeps the crate's default features, with the
/// `default-features = false` of this manifest. Those paths did not build before 0.5.8 (SIMD calls
/// outside `unsafe` blocks, image-rs/image#2705, fixed in zune-jpeg 0.5.8), so the manifest must
/// not ask for less than 0.5.8, and the lock holds at least that.
#[test]
fn the_jpeg_decoder_is_a_version_whose_simd_builds() {
    fn version_of(text: &str) -> (u32, u32, u32) {
        let mut parts = text.split('.').map(|p| p.trim().parse::<u32>().unwrap_or(0));
        (
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
        )
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("layout/Cargo.toml");
    let line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("zune-jpeg") && l.contains("version"))
        .expect("the zune-jpeg dependency");
    let required = line
        .split("version = \"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("its version requirement");
    assert!(
        version_of(required) >= (0, 5, 8),
        "layout/Cargo.toml asks for zune-jpeg {required}: SIMD that does not build is below 0.5.8"
    );
    let lock = std::fs::read_to_string(root.join("..").join("Cargo.lock")).expect("Cargo.lock");
    let locked = lock
        .split("[[package]]")
        .find_map(|block| {
            block
                .contains("name = \"zune-jpeg\"")
                .then(|| block.split("version = \"").nth(1)?.split('"').next().map(str::to_string))
                .flatten()
        })
        .expect("zune-jpeg is in Cargo.lock");
    assert!(version_of(&locked) >= (0, 5, 8), "Cargo.lock holds zune-jpeg {locked}");
}
