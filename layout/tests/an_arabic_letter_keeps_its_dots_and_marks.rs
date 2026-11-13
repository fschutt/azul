#![cfg(feature = "text_layout")]
//! What a real Arabic font does to its letters reaches the layout the way the
//! font means it: a dot that a one-to-many substitution splits off its letter
//! stays in that letter's cluster, and GPOS mark and cursive attachments put a
//! glyph where the font's anchors say, also once the line is drawn.
//!
//! The fonts are small subsets of Noto Sans Arabic and Amiri
//! (`tests/fonts/README.md`, sections 7 and 8). Both have 1000 units per em,
//! so at a font size of 1000px a pixel is a font unit; the expected values are
//! what `HarfBuzz` gives for the same font and text.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use azul_css::props::basic::FontRef;
use azul_layout::{
    font::parsed::ParsedFont,
    parsed_font_to_font_ref,
    text3::{
        cache::{
            create_logical_items, perform_fragment_layout, split_into_bidi_runs,
            shape_bidi_runs, AvailableSpace, BidiDirection, BreakCursor, FontChainKey,
            FontStack, Glyph, InlineContent, LoadedFonts, Point, StyleProperties, StyledRun,
            TextWrap, UnicodeBidi, UnifiedConstraints,
        },
        default::shape_text_for_parsed_font,
        glyphs::{get_glyph_positions, PositionedGlyph},
        script::{Language, Script},
    },
};
use rust_fontconfig::{FcFontCache, FontBytes, FontFallbackChain, FontId};

/// The font size at which one pixel is one unit of the 1000-unit fonts here.
const ONE_PIXEL_PER_UNIT: f32 = 1000.0;

fn font(name: &str) -> ParsedFont {
    let path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts")).join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let shared = Arc::new(FontBytes::Owned(Arc::from(bytes.as_slice())));
    let mut warnings = Vec::new();
    ParsedFont::from_bytes(&bytes, 0, &mut warnings)
        .unwrap_or_else(|| panic!("parse {name}"))
        .with_source_bytes(shared)
}

fn noto() -> ParsedFont {
    font("NotoSansArabic-DotsAndMarks.ttf")
}

fn amiri() -> ParsedFont {
    font("Amiri-CursiveSample.ttf")
}

/// `text` shaped right to left as Arabic, one pixel per font unit.
fn shape_arabic(font: &ParsedFont, text: &str) -> Vec<Glyph> {
    let style = StyleProperties {
        font_size_px: ONE_PIXEL_PER_UNIT,
        ..StyleProperties::default()
    };
    shape_text_for_parsed_font(
        font,
        text,
        Script::Arabic,
        Language::EnglishUS,
        BidiDirection::Rtl,
        &style,
    )
    .expect("Arabic text shapes")
}

/// The shaped glyphs in one line each, for the assert messages.
fn describe(glyphs: &[Glyph]) -> String {
    glyphs
        .iter()
        .map(|g| {
            format!(
                "gid {} cluster {} bytes {}+{} {:?} advance {} offset ({}, {})",
                g.glyph_id,
                g.cluster,
                g.logical_byte_index,
                g.logical_byte_len,
                g.codepoint,
                g.advance,
                g.offset.x,
                g.offset.y
            )
        })
        .collect::<Vec<_>>()
        .join("\n  ")
}

/// The glyphs that stand for no text of their own: the dots the font split
/// off their letters.
fn split_off(glyphs: &[Glyph]) -> Vec<&Glyph> {
    glyphs.iter().filter(|g| g.logical_byte_len == 0).collect()
}

#[test]
fn a_dot_split_off_its_letter_keeps_the_letters_cluster() {
    // The font draws the initial beh of "بالعالم" as a dotless beh and a
    // separate dot below it. The dot belongs to the beh at byte 0, not to
    // the alef at byte 2 that comes after it.
    let glyphs = shape_arabic(&noto(), "بالعالم");
    let dots = split_off(&glyphs);
    assert_eq!(
        dots.len(),
        1,
        "the beh's dot should be the one glyph without text of its own:\n  {}",
        describe(&glyphs)
    );
    let dot = dots[0];
    assert_eq!(
        dot.codepoint,
        'ب',
        "the dot should stand for the beh it was split off:\n  {}",
        describe(&glyphs)
    );
    assert_eq!(
        (dot.cluster, dot.logical_byte_index),
        (0, 0),
        "the beh's dot landed in the alef's cluster instead of the beh's cluster 0:\n  {}",
        describe(&glyphs)
    );
}

#[test]
fn the_dots_of_the_last_letter_stay_inside_the_text() {
    // "تحدث" has a teh at byte 0 and a theh at byte 6, and the font splits
    // both into a dotless letter and their dots. Each letter's dots carry
    // that letter's cluster; the theh's must not point past the end of the
    // 8-byte text.
    let text = "تحدث";
    let glyphs = shape_arabic(&noto(), text);
    let dots: Vec<(char, u32)> = split_off(&glyphs)
        .iter()
        .map(|g| (g.codepoint, g.cluster))
        .collect();
    assert_eq!(
        dots,
        [('ت', 0), ('ث', 6)],
        "the dots should carry the clusters of the teh (0) and the theh (6) of a {}-byte \
         text, each dot got the cluster of whatever came after its letter:\n  {}",
        text.len(),
        describe(&glyphs)
    );
    let past_the_end: Vec<&Glyph> = glyphs
        .iter()
        .filter(|g| g.cluster as usize >= text.len())
        .collect();
    assert!(
        past_the_end.is_empty(),
        "no glyph should have a cluster past the end of the text:\n  {}",
        describe(&glyphs)
    );
}

/// Whether two `(x, y)` pairs are the same, give or take `tolerance` on each axis.
fn near(actual: (f32, f32), expected: (f32, f32), tolerance: f32) -> bool {
    (actual.0 - expected.0).abs() <= tolerance && (actual.1 - expected.1).abs() <= tolerance
}

/// Asserts that `glyph` sits at `expected` font units from its pen position,
/// give or take one unit.
fn assert_offset(glyph: &Glyph, expected: (f32, f32), what: &str, glyphs: &[Glyph]) {
    let actual = (glyph.offset.x, glyph.offset.y);
    assert!(
        near(actual, expected, 1.0),
        "{what} should sit at {expected:?} font units from its pen, as the font's anchors put \
         it, but sits at {actual:?}:\n  {}",
        describe(glyphs)
    );
}

#[test]
fn a_kasra_sits_on_its_beh_where_the_fonts_anchors_put_it() {
    // "بِ" is a dotless beh, its dot below and a kasra. The dot hangs on the
    // beh's bottom anchor, the kasra on the dot's own bottom anchor (mark to
    // mark), so the kasra ends up below the dot.
    let glyphs = shape_arabic(&noto(), "بِ");
    let dot = split_off(&glyphs)
        .first()
        .copied()
        .unwrap_or_else(|| panic!("a dot split off the beh:\n  {}", describe(&glyphs)));
    let kasra = glyphs
        .iter()
        .find(|g| g.codepoint == '\u{650}')
        .unwrap_or_else(|| panic!("a kasra glyph:\n  {}", describe(&glyphs)));
    assert_offset(dot, (410.0, -23.0), "the beh's dot", &glyphs);
    assert_offset(kasra, (344.0, -211.0), "the kasra", &glyphs);
}

#[test]
fn every_dot_sits_where_the_fonts_anchors_put_it() {
    let font = noto();

    let glyphs = shape_arabic(&font, "بالعالم");
    let dots = split_off(&glyphs);
    assert_eq!(dots.len(), 1, "one dot in \"بالعالم\":\n  {}", describe(&glyphs));
    assert_offset(dots[0], (31.0, -3.0), "the dot below the initial beh", &glyphs);

    let glyphs = shape_arabic(&font, "تحدث");
    let dots = split_off(&glyphs);
    assert_eq!(dots.len(), 2, "two sets of dots in \"تحدث\":\n  {}", describe(&glyphs));
    assert_offset(dots[0], (5.0, -59.0), "the teh's two dots", &glyphs);
    assert_offset(dots[1], (344.0, -165.0), "the theh's three dots", &glyphs);
}

#[test]
fn a_cursive_join_moves_the_letter_whose_exit_meets_the_next_letters_entry() {
    // In "مجد" the initial meem's exit anchor is at (0, 254) and the medial
    // jeem's entry anchor at (55, 235). The font's cursive lookup is flagged
    // right to left, which makes the first glyph the one that moves: the meem
    // goes 19 units down, so its exit meets the jeem's entry. The anchors' x
    // already match the advances, so no advance changes.
    let glyphs = shape_arabic(&amiri(), "مجد");
    let letters: String = glyphs.iter().map(|g| g.codepoint).collect();
    assert_eq!(letters, "مجد", "one glyph per letter:\n  {}", describe(&glyphs));
    let expected = [
        ("the meem", (0.0, -19.0), 550.0),
        ("the jeem", (0.0, 0.0), 55.0),
        ("the dal", (0.0, 0.0), 471.0),
    ];
    for (glyph, (what, offset, advance)) in glyphs.iter().zip(expected) {
        assert_offset(glyph, offset, what, &glyphs);
        assert!(
            (glyph.advance - advance).abs() <= 1.0,
            "{what} should keep its advance of {advance} font units, the cursive join \
             changed it:\n  {}",
            describe(&glyphs)
        );
    }
}

/// `text` laid out on one right-to-left line at `font_size_px`, and where
/// each of its glyphs is drawn.
fn draw_right_to_left(parsed: ParsedFont, text: &str, font_size_px: f32) -> Vec<PositionedGlyph> {
    draw_right_to_left_wrapping(parsed, text, font_size_px, TextWrap::Wrap)
}

/// [`draw_right_to_left`], with the lines broken by `text_wrap`.
fn draw_right_to_left_wrapping(
    parsed: ParsedFont,
    text: &str,
    font_size_px: f32,
    text_wrap: TextWrap,
) -> Vec<PositionedGlyph> {
    let font_ref = parsed_font_to_font_ref(parsed);
    let style = Arc::new(StyleProperties {
        font_stack: FontStack::Ref(font_ref.clone()),
        font_size_px,
        ..StyleProperties::default()
    });
    let content = vec![InlineContent::Text(StyledRun {
        text: Arc::from(text),
        style,
        logical_start_byte: 0,
        source_node_id: None,
    })];
    let mut loaded: LoadedFonts<FontRef> = LoadedFonts::new();
    loaded.insert(FontId::new(), font_ref);
    let logical = create_logical_items(&content, &[], &mut None);
    let visual =
        split_into_bidi_runs(&logical, BidiDirection::Rtl, UnicodeBidi::Normal, &mut None)
            .expect("bidi reorder");
    let chains: HashMap<FontChainKey, FontFallbackChain> = HashMap::new();
    let shaped = shape_bidi_runs(&visual, &chains, &FcFontCache::default(), &loaded, &mut None)
        .expect("shaping");
    let mut cursor = BreakCursor::new(&shaped);
    let constraints = UnifiedConstraints {
        available_width: AvailableSpace::MaxContent,
        direction: Some(BidiDirection::Rtl),
        text_wrap,
        ..UnifiedConstraints::default()
    };
    let layout = perform_fragment_layout(&mut cursor, &logical, &constraints, &mut None, &loaded)
        .expect("line layout");
    get_glyph_positions(&layout)
}

#[test]
fn the_beh_its_dot_and_its_kasra_are_drawn_where_the_font_puts_them() {
    // A right-to-left line is drawn in visual order and a glyph's offset
    // counts from the pen where that glyph is drawn. The dot and the kasra
    // come before the beh visually and have no advance, so both are drawn at
    // the beh's origin plus their offset - at 100px, a tenth of the font
    // units, with y pointing down on screen.
    let text = "بِ";
    let shaped = shape_arabic(&noto(), text);
    let gid = |what: &str, pick: fn(&Glyph) -> bool| {
        shaped
            .iter()
            .find(|g| pick(g))
            .map(|g| g.glyph_id)
            .unwrap_or_else(|| panic!("{what} glyph:\n  {}", describe(&shaped)))
    };
    let beh = gid("the beh's", |g| g.advance > 0.0);
    let dot = gid("the dot's", |g| g.logical_byte_len == 0);
    let kasra = gid("the kasra's", |g| g.codepoint == '\u{650}');

    let drawn = draw_right_to_left(noto(), text, 100.0);
    let at = |what: &str, glyph_id: u16| -> Point {
        drawn
            .iter()
            .find(|p| p.glyph_id == glyph_id)
            .map(|p| p.position)
            .unwrap_or_else(|| panic!("{what} is drawn: {drawn:?}"))
    };
    let beh_at = at("the beh", beh);
    for (what, glyph_id, expected) in [
        ("the beh's dot", dot, (41.0, 2.3)),
        ("the kasra", kasra, (34.4, 21.1)),
    ] {
        let glyph_at = at(what, glyph_id);
        let from_beh = (glyph_at.x - beh_at.x, glyph_at.y - beh_at.y);
        assert!(
            near(from_beh, expected, 0.11),
            "{what} should be drawn at {expected:?} px from the beh's origin, it is drawn at \
             {from_beh:?}: {drawn:?}"
        );
    }
}

#[test]
fn a_balanced_line_draws_a_letter_from_where_its_pen_stands() {
    // `text-wrap: balance` breaks and places its lines on its own. A line of
    // one cluster starts at x = 0 on the line's baseline, and the beh has no
    // offset of its own, so it is drawn there like the dotless hah of a line
    // of its own; the dot's offset moves only the dot.
    let first_glyph_drawn = |text: &str, pick: fn(&Glyph) -> bool| -> Point {
        let shaped = shape_arabic(&noto(), text);
        let glyph_id = shaped
            .iter()
            .find(|g| pick(g))
            .map(|g| g.glyph_id)
            .unwrap_or_else(|| panic!("the letter of {text:?}:\n  {}", describe(&shaped)));
        let drawn = draw_right_to_left_wrapping(noto(), text, 100.0, TextWrap::Balance);
        drawn
            .iter()
            .find(|p| p.glyph_id == glyph_id)
            .map(|p| p.position)
            .unwrap_or_else(|| panic!("the letter of {text:?} is drawn: {drawn:?}"))
    };
    let hah_at = first_glyph_drawn("ح", |g| g.advance > 0.0);
    let beh_at = first_glyph_drawn("بِ", |g| g.advance > 0.0);
    assert!(
        hah_at.x.abs() <= 0.01,
        "a lone hah should be drawn at the start of its balanced line, it is drawn at {hah_at:?}"
    );
    assert!(
        near((beh_at.x, beh_at.y), (hah_at.x, hah_at.y), 0.01),
        "the beh should be drawn where the hah is, at the pen on the baseline, it is drawn at \
         {beh_at:?}: the balanced line moved the beh's whole cluster by the offset of its dot"
    );
}

#[test]
fn a_balanced_right_to_left_line_is_drawn_like_a_wrapped_one() {
    // A line of one word does not break either way, so `text-wrap: balance`
    // must draw it as the greedy line breaker does: its letters right to
    // left, the first one, the beh, rightmost.
    let text = "بالعالم";
    let wrapped = draw_right_to_left_wrapping(noto(), text, 100.0, TextWrap::Wrap);
    let balanced = draw_right_to_left_wrapping(noto(), text, 100.0, TextWrap::Balance);
    let from_the_first = |drawn: &[PositionedGlyph]| -> Vec<(u16, f32)> {
        let left = drawn.first().map_or(0.0, |p| p.position.x);
        drawn.iter().map(|p| (p.glyph_id, p.position.x - left)).collect()
    };
    let (wrapped, balanced) = (from_the_first(&wrapped), from_the_first(&balanced));
    let same = wrapped.len() == balanced.len()
        && wrapped
            .iter()
            .zip(&balanced)
            .all(|(w, b)| w.0 == b.0 && (w.1 - b.1).abs() <= 0.01);
    assert!(
        same,
        "the balanced line should draw the glyphs (id, x from the leftmost) as the wrapped one \
         does, right to left:\n  wrapped  {wrapped:?}\n  balanced {balanced:?}"
    );
}
