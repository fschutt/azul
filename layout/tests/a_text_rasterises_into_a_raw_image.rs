//! Text becomes pixels in a `RawImage`: AzPhoto's text tool (and any app that
//! draws a caption into a picture, a watermark, a chart label) needs the
//! glyphs as RGBA8 it can composite into its own raster - not a text node in
//! a DOM. azul had no such API (the glyph raster was internal to cpurender).
//!
//! `RawImage::from_text` sets a string in a font and answers a straight-alpha
//! RGBA8 image exactly as big as its lines' boxes; `RawImage::draw_text`
//! composites the same pixels into an image at a position. Both shape with
//! the text engine (`shape_text_for_parsed_font`) and rasterise with the CPU
//! renderer's glyph path (grayscale coverage - a transparent target has no
//! background for LCD subpixels).
//!
//! The font is `Azul Mock Mono` registered as a memory font, so the numbers
//! are exact: at 20 px every glyph advances 10 px, the line box is 20 px
//! (ascent 16, descent 4), and a glyph's ink is the box x 1..9, y 2..16 of
//! its cell (a 1 px frame on every side, its interior keyed off the
//! codepoint).

use azul_core::resources::{RawImage, RawImageData, RawImageFormat};
use azul_css::{props::basic::ColorU, AzString, U8Vec};
use azul_layout::cpurender::{draw_text_with, text_image_with, TextRasterStyle};
use rust_fontconfig::{FcFont, FcFontCache, FcPattern};

const FAMILY: &str = "Azul Mock Mono";

/// A font cache that knows only the mock font.
fn mock_cache() -> FcFontCache {
    let cache = FcFontCache::default();
    cache.with_memory_fonts(vec![(
        FcPattern {
            family: Some(FAMILY.to_string()),
            ..Default::default()
        },
        FcFont {
            bytes: azul_layout::text3::mock_fonts::MOCK_MONO_TTF.to_vec(),
            font_index: 0,
            id: "text-raster-mock-mono".to_string(),
        },
    )]);
    cache
}

fn style(color: ColorU) -> TextRasterStyle {
    TextRasterStyle::create(AzString::from(FAMILY), 20.0, color)
}

const BLACK: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 0,
    a: 255,
};
const RED: ColorU = ColorU {
    r: 220,
    g: 20,
    b: 20,
    a: 255,
};

fn pixel(img: &RawImage, x: usize, y: usize) -> [u8; 4] {
    let RawImageData::U8(ref px) = img.pixels else {
        panic!("an 8-bit image");
    };
    let i = (y * img.width + x) * 4;
    let p = px.as_ref();
    [p[i], p[i + 1], p[i + 2], p[i + 3]]
}

fn white(width: u32, height: u32) -> RawImage {
    RawImage::create_rgba8(
        width,
        height,
        U8Vec::from_vec(vec![255; (width * height * 4) as usize]),
        false,
    )
}

#[test]
fn a_set_text_is_as_big_as_its_line_box_and_inks_its_glyphs() {
    let img = text_image_with(&mock_cache(), "Hi", &style(BLACK)).expect("the mock font sets text");
    assert_eq!(
        (img.width, img.height),
        (20, 20),
        "two 10 px glyphs on one 20 px line"
    );
    assert_eq!(img.data_format, RawImageFormat::RGBA8);
    assert!(
        !img.premultiplied_alpha,
        "straight alpha, like every decoded image"
    );

    // The left side bearing and the space above the ink are transparent.
    for y in 0..20 {
        assert_eq!(pixel(&img, 0, y)[3], 0, "x 0 is the side bearing (y {y})");
    }
    for x in 0..20 {
        assert_eq!(pixel(&img, x, 0)[3], 0, "y 0 is above the ink (x {x})");
        assert_eq!(
            pixel(&img, x, 18)[3],
            0,
            "y 18 is the descent, no ink (x {x})"
        );
    }
    // The frame's left bar of 'H' covers x 1..2 from y 2 to the baseline.
    let ink = pixel(&img, 1, 9);
    assert!(ink[3] >= 200, "the glyph frame is inked: {ink:?}");
    assert_eq!(&ink[..3], &[0, 0, 0], "in the text colour");
    // ... and so does the second glyph's, one advance further.
    assert!(
        pixel(&img, 11, 9)[3] >= 200,
        "the second glyph starts at x 10"
    );
}

#[test]
fn lines_stack_at_the_line_height_and_the_widest_sets_the_width() {
    let cache = mock_cache();
    let img = text_image_with(&cache, "A\nBBB", &style(BLACK)).expect("two lines");
    assert_eq!(
        (img.width, img.height),
        (30, 40),
        "three glyphs wide, two lines tall"
    );
    assert!(
        pixel(&img, 21, 29)[3] >= 200,
        "the second line's third glyph"
    );
    assert_eq!(
        pixel(&img, 21, 9)[3],
        0,
        "the first line has one glyph only"
    );

    let loose =
        text_image_with(&cache, "A\nB", &style(BLACK).with_line_height(1.5)).expect("two lines");
    assert_eq!(loose.height, 50, "the second line's box starts 30 px down");
}

#[test]
fn the_colours_alpha_scales_the_ink() {
    let half = ColorU {
        r: 0,
        g: 0,
        b: 255,
        a: 128,
    };
    let img = text_image_with(&mock_cache(), "H", &style(half)).expect("text");
    let p = pixel(&img, 1, 9);
    assert!((118..=138).contains(&p[3]), "half-transparent ink: {p:?}");
    assert!(
        p[2] >= 240 && p[0] <= 10,
        "straight colour, not darkened by the alpha: {p:?}"
    );
}

#[test]
fn drawn_text_composites_over_the_image_at_its_position() {
    let mut img = white(40, 30);
    assert!(draw_text_with(
        &mock_cache(),
        &mut img,
        "H",
        &style(RED),
        10.0,
        5.0
    ));
    let p = pixel(&img, 11, 14);
    assert!(
        p[0] >= 200 && p[1] <= 60 && p[2] <= 60,
        "red ink at the glyph's frame: {p:?}"
    );
    assert_eq!(p[3], 255, "an opaque image stays opaque");
    assert_eq!(
        pixel(&img, 10, 14),
        [255, 255, 255, 255],
        "the side bearing leaves the white"
    );
    assert_eq!(
        pixel(&img, 5, 14),
        [255, 255, 255, 255],
        "left of the text nothing changed"
    );
    assert_eq!(
        pixel(&img, 11, 6),
        [255, 255, 255, 255],
        "above the ink nothing changed"
    );
}

#[test]
fn text_that_cannot_be_set_is_nothing_and_draws_nothing() {
    let cache = mock_cache();
    assert!(
        text_image_with(&FcFontCache::default(), "Hi", &style(BLACK)).is_none(),
        "no font at all"
    );
    assert!(
        text_image_with(&cache, "", &style(BLACK)).is_none(),
        "no text"
    );
    for size in [0.0, -4.0, f32::NAN, f32::INFINITY] {
        let s = TextRasterStyle::create(AzString::from(FAMILY), size, BLACK);
        assert!(text_image_with(&cache, "Hi", &s).is_none(), "size {size}");
    }
    // A format the compositor does not write is left alone.
    let mut grey = RawImage {
        pixels: RawImageData::U8(U8Vec::from_vec(vec![7; 40 * 30])),
        width: 40,
        height: 30,
        premultiplied_alpha: false,
        data_format: RawImageFormat::R8,
        tag: U8Vec::from_vec(Vec::new()),
    };
    assert!(!draw_text_with(
        &cache,
        &mut grey,
        "H",
        &style(RED),
        10.0,
        5.0
    ));
    let RawImageData::U8(ref px) = grey.pixels else {
        unreachable!()
    };
    assert!(px.as_ref().iter().all(|b| *b == 7), "untouched");
}

#[test]
fn an_unknown_family_falls_back_to_a_font_that_exists_and_bold_to_the_regular_face() {
    let cache = mock_cache();
    let bold = style(BLACK).with_bold(true).with_italic(true);
    let img = text_image_with(&cache, "Hi", &bold).expect("the regular face stands in");
    assert_eq!((img.width, img.height), (20, 20));
    let other = TextRasterStyle::create(AzString::from("No Such Family"), 20.0, BLACK);
    let img = text_image_with(&cache, "Hi", &other).expect("any font that covers the text");
    assert_eq!((img.width, img.height), (20, 20));
}
