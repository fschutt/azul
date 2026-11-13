//! Text set into a picture wraps at a width, aligns its lines and runs
//! right to left (T2: the text tool of a photo or paint app sets a caption in
//! a box; the set text was one LTR line per `\n`, left-aligned).
//!
//! - `with_max_width`: lines break at spaces so none is wider (a word wider
//!   than the box is broken between its letters); the image is the box's
//!   width. The spaces at a break hang: they take no room.
//! - `with_align`: each line's place in the box (the widest line's width
//!   when there is no box). `Start` / `End` follow the direction.
//! - `with_direction`: `Auto` takes the paragraph's first strong letter
//!   (Unicode bidi P2), `Rtl` puts the start on the right; mixed runs are
//!   ordered as the bidi algorithm orders them.
//!
//! The font is `Azul Mock Mono` as in a_text_rasterises_into_a_raw_image.rs:
//! at 20 px every glyph advances 10 px, the line box is 20 px, a glyph's
//! frame is inked at x 1..9 of its cell.

use azul_core::resources::{RawImage, RawImageData};
use azul_css::{props::basic::ColorU, AzString};
use azul_layout::cpurender::{
    text_image_with, TextRasterAlign, TextRasterDirection, TextRasterStyle,
};
use rust_fontconfig::{FcFont, FcFontCache, FcPattern};

const FAMILY: &str = "Azul Mock Mono";

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
            id: "text-raster-layout-mock-mono".to_string(),
        },
    )]);
    cache
}

fn style() -> TextRasterStyle {
    TextRasterStyle::create(
        AzString::from(FAMILY),
        20.0,
        ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
    )
}

fn alpha(img: &RawImage, x: usize, y: usize) -> u8 {
    let RawImageData::U8(ref px) = img.pixels else {
        panic!("an 8-bit image");
    };
    px.as_ref()[(y * img.width + x) * 4 + 3]
}

/// Whether the glyph cell starting at x `cell_x` on line `line` is inked
/// (its frame's left bar).
fn inked(img: &RawImage, cell_x: usize, line: usize) -> bool {
    alpha(img, cell_x + 1, line * 20 + 9) >= 200
}

#[test]
fn lines_break_at_spaces_to_fit_the_width() {
    let img = text_image_with(&mock_cache(), "AA BB CC", &style().with_max_width(50.0))
        .expect("the text sets");
    assert_eq!(
        (img.width, img.height),
        (50, 40),
        "\"AA BB\" fills the 50 px box, \"CC\" goes to the second line"
    );
    assert!(inked(&img, 40, 0), "the second B ends the first line");
    assert!(inked(&img, 0, 1) && inked(&img, 10, 1), "CC on the second line");
    assert!(!inked(&img, 20, 1), "and nothing after it");
}

#[test]
fn a_word_wider_than_the_box_breaks_between_its_letters() {
    let img = text_image_with(&mock_cache(), "AAAAA", &style().with_max_width(30.0))
        .expect("the text sets");
    assert_eq!((img.width, img.height), (30, 40), "AAA, then AA");
    assert!(inked(&img, 20, 0) && inked(&img, 10, 1) && !inked(&img, 20, 1));
}

#[test]
fn a_centred_line_sits_in_the_middle_of_its_box() {
    let img = text_image_with(
        &mock_cache(),
        "AA",
        &style().with_max_width(60.0).with_align(TextRasterAlign::Center),
    )
    .expect("the text sets");
    assert_eq!(img.width, 60);
    assert!(!inked(&img, 0, 0) && inked(&img, 20, 0) && inked(&img, 30, 0));
}

#[test]
fn an_end_aligned_line_ends_at_the_right_of_its_box() {
    let img = text_image_with(
        &mock_cache(),
        "AA",
        &style().with_max_width(60.0).with_align(TextRasterAlign::End),
    )
    .expect("the text sets");
    assert!(!inked(&img, 30, 0) && inked(&img, 40, 0) && inked(&img, 50, 0));
}

#[test]
fn without_a_box_lines_align_in_the_widest_one() {
    let img = text_image_with(
        &mock_cache(),
        "A\nBBB",
        &style().with_align(TextRasterAlign::Center),
    )
    .expect("the text sets");
    assert_eq!((img.width, img.height), (30, 40));
    assert!(!inked(&img, 0, 0) && inked(&img, 10, 0), "A in the middle of 30 px");
}

#[test]
fn right_to_left_text_starts_on_the_right() {
    let img = text_image_with(
        &mock_cache(),
        "AA",
        &style()
            .with_max_width(60.0)
            .with_direction(TextRasterDirection::Rtl),
    )
    .expect("the text sets");
    assert!(
        !inked(&img, 30, 0) && inked(&img, 40, 0) && inked(&img, 50, 0),
        "the start of a right-to-left line is its right edge"
    );
}

#[test]
fn a_paragraph_whose_first_letter_is_hebrew_runs_right_to_left_by_itself() {
    // U+05D0 HEBREW LETTER ALEF (bidi class R) first, then a Latin word: the
    // paragraph is right to left, so the alef is the rightmost cell and the
    // Latin run stands left of it, the line right-aligned in its 60 px box:
    // [ ][AAA][ ][alef] - left to right, the line laid as written would be
    // [alef][ ][AAA][ ].
    let img = text_image_with(&mock_cache(), "\u{05D0} AAA", &style().with_max_width(60.0))
        .expect("the text sets");
    assert!(
        !inked(&img, 0, 0) && inked(&img, 10, 0) && inked(&img, 30, 0) && !inked(&img, 40, 0),
        "Auto takes the first strong letter's direction and orders the runs by it"
    );
}
