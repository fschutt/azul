//! A text in a generic family is drawn in a face that has its letters.
//!
//! `sans-serif` names no installed family: it stands for the faces the
//! platform sets sans-serif text in, and a text in it is drawn in the first
//! of them that has its letters, as the DOM's text is. The text painters that
//! draw into images (a text image, an SVG's `<text>`) looked the family up as
//! if it were a family name, matched nothing, and fell back to a face chosen
//! with no regard to the text: the narrowest face installed. On a Mac that is
//! a face without Latin letters, and a chart's labels came out as rows of
//! empty boxes, the face's missing-glyph shape.
//!
//! The font cache here holds the mono mock face, which has the ASCII letters,
//! and a face with six Arabic letters, the narrower one: a text in
//! `sans-serif` comes out as the mono face draws it.

use azul_core::resources::{RawImage, RawImageData};
use azul_css::{props::basic::ColorU, AzString};
use azul_layout::{
    cpurender::{render_svg_to_raw_image_with, text_image_with, TextRasterStyle},
    text3::mock_fonts::MOCK_MONO_TTF,
};
use rust_fontconfig::{FcFont, FcFontCache, FcPattern};

const MONO: &str = "Azul Mock Mono";

/// A font cache of the mono mock face and of a face with Arabic letters only.
fn mono_and_arabic() -> FcFontCache {
    let path = format!("{}/tests/fonts/azul-mock-arabic.ttf", env!("CARGO_MANIFEST_DIR"));
    let arabic = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let face = |family: &str, bytes: Vec<u8>, id: &str| {
        (
            FcPattern {
                family: Some(family.to_string()),
                ..Default::default()
            },
            FcFont {
                bytes,
                font_index: 0,
                id: id.to_string(),
            },
        )
    };
    let cache = FcFontCache::default();
    cache.with_memory_fonts(vec![
        face("Azul Mock Arabic", arabic, "generic-family-arabic"),
        face(MONO, MOCK_MONO_TTF.to_vec(), "generic-family-mono"),
    ]);
    cache
}

/// The size and the pixels of `image`.
fn pixels(image: &RawImage) -> (usize, usize, Vec<u8>) {
    let RawImageData::U8(bytes) = &image.pixels else {
        panic!("an 8-bit image");
    };
    (image.width, image.height, bytes.as_ref().to_vec())
}

/// `text` set at 20 px in `family`, as a text image.
fn set_in(fc: &FcFontCache, text: &str, family: &str) -> (usize, usize, Vec<u8>) {
    let style = TextRasterStyle::create(
        AzString::from(family),
        20.0,
        ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
    );
    pixels(&text_image_with(fc, text, &style).expect("the text is set"))
}

#[test]
fn a_text_image_in_sans_serif_is_drawn_in_the_face_that_has_its_letters() {
    let fc = mono_and_arabic();
    let mono = set_in(&fc, "Hi", MONO);
    assert!(
        mono.2.chunks_exact(4).any(|px| px[3] > 0),
        "harness: the mono face inks the text"
    );
    assert_eq!(
        set_in(&fc, "Hi", "sans-serif"),
        mono,
        "`sans-serif` draws `Hi` in the face that has `H` and `i`"
    );
}

/// An SVG whose one `<text>` says `Hi` in `family`, rendered with the fonts
/// of `fc` on a transparent backdrop.
fn svg_text_in(fc: &FcFontCache, family: &str) -> (usize, usize, Vec<u8>) {
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"60\" height=\"30\" \
         viewBox=\"0 0 60 30\"><text x=\"5\" y=\"20\" font-family=\"{family}\" \
         font-size=\"20\">Hi</text></svg>"
    );
    pixels(
        &render_svg_to_raw_image_with(fc, svg.as_bytes(), 60, 30, None)
            .expect("the SVG renders"),
    )
}

/// The labels of a chart drawn as an SVG: the `<text>` in `sans-serif` is
/// drawn in the face that has its letters, and inks the image.
#[test]
fn an_svg_text_in_sans_serif_is_drawn_in_the_face_that_has_its_letters() {
    let fc = mono_and_arabic();
    let mono = svg_text_in(&fc, MONO);
    assert!(
        mono.2.chunks_exact(4).any(|px| px[3] > 0),
        "the SVG text inks the image"
    );
    assert_eq!(
        svg_text_in(&fc, "sans-serif"),
        mono,
        "`sans-serif` draws `Hi` in the face that has `H` and `i`"
    );
}
