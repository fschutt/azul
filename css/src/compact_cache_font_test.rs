use super::*;
use crate::props::basic::{
    font::{StyleFontStyle, StyleFontWeight},
    length::FloatValue,
    StyleFontStretch,
};

// The font slots of the compact cache since `font-weight` left the tier-1 word:
//
//   * `CompactTextProps::font_weight`: the weight x 64 (`0` = undeclared = 400; two sentinels
//     at the top stand for `lighter` / `bolder` until the builder resolves them),
//   * `CompactTextProps::font_stretch`: the percentage x 64 (`0` = undeclared = 100%),
//   * `CompactTextProps::font_oblique`: the angle of an `oblique` style x 64, read only when
//     the 2-bit style code of tier 1 says oblique,
//   * six more `DOM_HAS_*` flags, which is why the flag word is a `u64` now.
//
// The cascade that fills these slots is tested in `azul-core`
// (`core/tests/font_properties_cascade.rs`); this file pins the codec and the getters.

/// `encode_font_weight_slot` is a `const fn`: a weight constant is encoded at compile time.
const BOLD_SLOT: u16 = encode_font_weight_slot(StyleFontWeight::BOLD);

/// The six flags of the rare font properties, in bits 32..=37 of the flag word.
const NEW_FONT_FLAGS: [(&str, u64); 6] = [
    ("font-variation-settings", DOM_HAS_FONT_VARIATION_SETTINGS),
    ("font-optical-sizing", DOM_HAS_FONT_OPTICAL_SIZING),
    ("font-kerning", DOM_HAS_FONT_KERNING),
    ("font-variant-ligatures", DOM_HAS_FONT_VARIANT_LIGATURES),
    ("font-variant-east-asian", DOM_HAS_FONT_VARIANT_EAST_ASIAN),
    ("font-synthesis", DOM_HAS_FONT_SYNTHESIS),
];

fn weight(w: f32) -> StyleFontWeight {
    StyleFontWeight::from_number(w)
}

fn stretch(percent: f32) -> StyleFontStretch {
    StyleFontStretch::from_percent(percent)
}

fn oblique(degrees: f32) -> StyleFontStyle {
    StyleFontStyle::Oblique(FloatValue::new(degrees))
}

fn weight_slot(w: f32) -> u16 {
    encode_font_weight_slot(weight(w))
}

/// A tier-1 word with every field at its CSS initial value except the font style.
fn tier1_with_style(font_style: StyleFontStyle) -> u64 {
    encode_tier1(
        LayoutDisplay::Block,
        LayoutPosition::Static,
        LayoutFloat::None,
        LayoutOverflow::Visible,
        LayoutOverflow::Visible,
        LayoutBoxSizing::ContentBox,
        LayoutFlexDirection::Row,
        LayoutFlexWrap::NoWrap,
        LayoutJustifyContent::FlexStart,
        LayoutAlignItems::Stretch,
        LayoutAlignContent::Stretch,
        LayoutWritingMode::HorizontalTb,
        LayoutClear::None,
        font_style,
        StyleTextAlign::Start,
        StyleVisibility::Visible,
        StyleWhiteSpace::Normal,
        StyleDirection::Ltr,
        StyleVerticalAlign::Baseline,
        StyleBorderCollapse::Separate,
    )
}

// ---------------------------------------------------------------------------
// The text tier
// ---------------------------------------------------------------------------

#[test]
fn the_text_tier_stays_32_bytes_with_the_three_font_slots_in_it() {
    assert_eq!(size_of::<CompactTextProps>(), 32);
    assert_eq!(align_of::<CompactTextProps>(), 8);

    // All nine fields by name, and the type of each font slot.
    let props = CompactTextProps {
        text_color: 1,
        font_weight: 2,
        font_stretch: 3,
        font_family_hash: 4,
        line_height: 5,
        font_oblique: 6,
        letter_spacing: 7,
        word_spacing: 8,
        text_indent: 9,
    };
    let weight_bits: u16 = props.font_weight;
    let stretch_bits: u16 = props.font_stretch;
    let oblique_bits: i16 = props.font_oblique;
    assert_eq!((weight_bits, stretch_bits, oblique_bits), (2, 3, 6));
}

#[test]
fn an_undeclared_text_node_has_zero_font_slots_that_read_as_the_initial_values() {
    let d = CompactTextProps::default();
    assert_eq!(d.font_weight, 0, "0 = undeclared = 400");
    assert_eq!(d.font_stretch, 0, "0 = undeclared = 100%");
    assert_eq!(d.font_oblique, 0);
    assert_eq!(
        decode_font_weight_slot(d.font_weight),
        StyleFontWeight::NORMAL
    );
    assert_eq!(
        decode_font_stretch_slot(d.font_stretch),
        StyleFontStretch::NORMAL
    );

    // The slots that were there keep their defaults.
    assert_eq!(d.line_height, I16_SENTINEL);
    assert_eq!(
        (
            d.text_color,
            d.font_family_hash,
            d.letter_spacing,
            d.word_spacing,
            d.text_indent
        ),
        (0, 0, 0, 0, 0)
    );

    // And a fresh cache holds that in every node.
    let c = CompactLayoutCache::with_capacity(3);
    assert!(c.tier2b_text.iter().all(|t| *t == d));
}

// ---------------------------------------------------------------------------
// font-weight slot
// ---------------------------------------------------------------------------

#[test]
fn every_integer_weight_from_1_to_1000_is_kept_exactly_in_its_slot() {
    for w in 1u16..=1000 {
        let expected = weight(f32::from(w));
        let slot = encode_font_weight_slot(expected);
        assert!(
            slot < U16_SENTINEL_THRESHOLD,
            "weight {w} encodes into the sentinel band: {slot:#06x}"
        );
        assert_eq!(
            decode_font_weight_slot(slot),
            expected,
            "weight {w} through slot {slot}"
        );
        if w == 400 {
            assert_eq!(slot, 0, "400 is the zero slot");
        } else {
            assert_eq!(slot, w * 64, "weight {w}: the slot is the weight times 64");
        }
    }
}

#[test]
fn every_eighth_of_a_weight_is_kept_exactly_in_its_slot() {
    // A multiple of 1/8 is exact as an f32, in thousandths (the `FloatValue` the weight holds)
    // and in 64ths (the slot).
    for eighths in 8u16..=8000 {
        let w = f32::from(eighths) / 8.0;
        let expected = weight(w);
        let slot = encode_font_weight_slot(expected);
        assert!(
            slot < U16_SENTINEL_THRESHOLD,
            "weight {w} encodes into the sentinel band: {slot:#06x}"
        );
        assert_eq!(
            decode_font_weight_slot(slot),
            expected,
            "weight {w} through slot {slot}"
        );
        if eighths == 3200 {
            assert_eq!(slot, 0, "400 is the zero slot");
        } else {
            assert_eq!(slot, eighths * 8, "weight {w}: the slot is the weight times 64");
        }
    }

    // The values a stylesheet is likely to have: the slot is the weight times 64.
    assert_eq!(weight_slot(350.5), 22_432);
    assert_eq!(weight_slot(12.25), 784);
    assert_eq!(weight_slot(100.125), 6_408);
    assert_eq!(weight_slot(650.0), 41_600);
    assert_eq!(weight_slot(1000.0), 64_000);
    assert_eq!(weight_slot(1.0), 64);
}

#[test]
fn a_weight_that_is_no_multiple_of_an_eighth_is_kept_to_about_a_64th() {
    // 87.0625 is 5572/64 but not a whole number of thousandths: the weight holds it rounded.
    for w in [
        1.0f32, 1.1, 12.3, 87.0625, 249.99, 350.1, 399.99, 400.01, 650.3, 999.9,
    ] {
        let back = decode_font_weight_slot(encode_font_weight_slot(weight(w))).number();
        assert!((back - w).abs() <= 0.02, "{w} came back as {back}");
    }
}

#[test]
fn normal_is_the_zero_slot_in_both_directions() {
    assert_eq!(encode_font_weight_slot(StyleFontWeight::NORMAL), 0);
    assert_eq!(weight_slot(400.0), 0);
    assert_eq!(decode_font_weight_slot(0), StyleFontWeight::NORMAL);
    assert_eq!(BOLD_SLOT, 700 * 64);
    // Every other weight is a slot of its own: nothing but 400 reads as undeclared.
    for w in [1.0, 100.0, 399.0, 401.0, 500.0, 700.0, 1000.0] {
        assert_ne!(weight_slot(w), 0, "weight {w} must not read as undeclared");
    }
}

#[test]
fn a_weight_outside_1_to_1000_clamps_to_the_nearest_end() {
    let lowest = weight_slot(1.0);
    let highest = weight_slot(1000.0);
    assert_eq!((lowest, highest), (64, 64_000));
    // A number the parser would refuse can still be built by hand (`Number(FloatValue::new(..))`).
    for w in [0.0f32, -5.0, 0.5, 0.999, -100_000.0] {
        assert_eq!(
            encode_font_weight_slot(StyleFontWeight::Number(FloatValue::new(w))),
            lowest,
            "weight {w} clamps to 1"
        );
    }
    // 65535 x 64 would wrap a u16 if it were scaled before it was clamped.
    for w in [1000.001f32, 1000.5, 2000.0, 65_535.0, 100_000.0] {
        assert_eq!(
            encode_font_weight_slot(StyleFontWeight::Number(FloatValue::new(w))),
            highest,
            "weight {w} clamps to 1000"
        );
    }
}

#[test]
fn lighter_and_bolder_are_the_two_sentinel_slots_and_no_number_reaches_them() {
    assert_eq!(FONT_WEIGHT_SLOT_LIGHTER, 0xFFFD);
    assert_eq!(FONT_WEIGHT_SLOT_BOLDER, 0xFFFE);
    assert_eq!(
        encode_font_weight_slot(StyleFontWeight::Lighter),
        FONT_WEIGHT_SLOT_LIGHTER
    );
    assert_eq!(
        encode_font_weight_slot(StyleFontWeight::Bolder),
        FONT_WEIGHT_SLOT_BOLDER
    );
    assert_eq!(
        decode_font_weight_slot(FONT_WEIGHT_SLOT_LIGHTER),
        StyleFontWeight::Lighter
    );
    assert_eq!(
        decode_font_weight_slot(FONT_WEIGHT_SLOT_BOLDER),
        StyleFontWeight::Bolder
    );
    for sentinel in [FONT_WEIGHT_SLOT_LIGHTER, FONT_WEIGHT_SLOT_BOLDER] {
        assert!(
            sentinel >= U16_SENTINEL_THRESHOLD,
            "{sentinel:#06x} is below the sentinel band"
        );
    }

    // The highest slot a number reaches is weight 1000, 64000: below the band.
    let highest = weight_slot(1000.0);
    assert!(
        highest < U16_SENTINEL_THRESHOLD,
        "weight 1000 encodes to {highest}"
    );
}

#[test]
fn decoding_a_weight_slot_is_total_and_a_number_slot_decodes_to_a_number() {
    for slot in 0..=u16::MAX {
        let _ = decode_font_weight_slot(slot);
    }
    for slot in (64..=64_000u16).step_by(8) {
        let decoded = decode_font_weight_slot(slot);
        assert!(!decoded.is_relative(), "slot {slot} decoded to a keyword");
        assert!(
            (1.0..=1000.0).contains(&decoded.number()),
            "slot {slot} decoded to {decoded:?}"
        );
        // 400 is slot 0, so its own slot number (25600) comes back as 0.
        let again = encode_font_weight_slot(decoded);
        assert_eq!(
            again,
            if slot == 25_600 { 0 } else { slot },
            "slot {slot} through {decoded:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// font-stretch slot
// ---------------------------------------------------------------------------

#[test]
fn the_stretch_slot_is_percent_times_64_with_100_percent_as_zero() {
    let table: [(f32, u16); 9] = [
        (50.0, 3_200),
        (62.5, 4_000),
        (75.0, 4_800),
        (87.5, 5_600),
        (100.0, 0),
        (112.5, 7_200),
        (125.0, 8_000),
        (150.0, 9_600),
        (200.0, 12_800),
    ];
    for (percent, slot) in table {
        assert_eq!(
            encode_font_stretch_slot(stretch(percent)),
            slot,
            "{percent}% encodes to its slot"
        );
        assert_eq!(
            decode_font_stretch_slot(slot),
            stretch(percent),
            "slot {slot} decodes to {percent}%"
        );
    }
    assert_eq!(StyleFontStretch::default(), StyleFontStretch::NORMAL);
    assert_eq!(encode_font_stretch_slot(StyleFontStretch::NORMAL), 0);
    assert_eq!(decode_font_stretch_slot(0), StyleFontStretch::NORMAL);
}

#[test]
fn every_eighth_of_a_percent_stretch_is_kept_exactly_in_its_slot() {
    for slot in (8..=0xFFF0u16).step_by(8) {
        let decoded = decode_font_stretch_slot(slot);
        let again = encode_font_stretch_slot(decoded);
        // 100% (slot 6400) is stored as 0.
        assert_eq!(
            again,
            if slot == 6_400 { 0 } else { slot },
            "slot {slot} decoded to {decoded:?}"
        );
    }
}

#[test]
fn a_declared_zero_percent_stretch_is_slot_1_not_the_undeclared_zero() {
    for percent in [0.0f32, 0.01, -10.0] {
        let slot = encode_font_stretch_slot(stretch(percent));
        assert_eq!(slot, 1, "{percent}% is declared: it must not read as undeclared");
    }
    let back = decode_font_stretch_slot(1);
    assert_ne!(back, StyleFontStretch::NORMAL);
    assert!(
        back.percent() > 0.0 && back.percent() < 0.1,
        "slot 1 is a 64th of a percent, got {}%",
        back.percent()
    );
}

#[test]
fn a_stretch_beyond_1023_percent_saturates_instead_of_wrapping() {
    for percent in [1023.75f32, 1024.0, 2000.0, 5000.0, 100_000.0] {
        assert_eq!(
            encode_font_stretch_slot(stretch(percent)),
            0xFFF0,
            "{percent}% saturates at the top slot"
        );
    }
}

#[test]
fn a_stretch_that_is_no_multiple_of_an_eighth_is_kept_to_about_a_64th() {
    for percent in [33.3f32, 91.7, 99.99, 100.01, 133.37, 777.7] {
        let back = decode_font_stretch_slot(encode_font_stretch_slot(stretch(percent))).percent();
        assert!((back - percent).abs() <= 0.02, "{percent}% came back as {back}%");
    }
}

// ---------------------------------------------------------------------------
// font-style: the 2-bit code of tier 1 and the angle in the text tier
// ---------------------------------------------------------------------------

#[test]
fn the_tier1_style_code_is_two_bits_and_the_angle_is_not_in_it() {
    assert_eq!(FONT_STYLE_MASK, 0x03);
    assert_eq!(style_font_style_to_u8(StyleFontStyle::Normal), 0);
    assert_eq!(style_font_style_to_u8(StyleFontStyle::Italic), 1);
    assert_eq!(style_font_style_to_u8(StyleFontStyle::OBLIQUE), 2);
    for degrees in [-90.0f32, -10.0, 0.0, 14.0, 90.0] {
        assert_eq!(
            style_font_style_to_u8(oblique(degrees)),
            2,
            "oblique {degrees}deg is code 2 whatever its angle"
        );
    }

    assert_eq!(style_font_style_from_u8(0), StyleFontStyle::Normal);
    assert_eq!(style_font_style_from_u8(1), StyleFontStyle::Italic);
    assert_eq!(
        style_font_style_from_u8(2),
        StyleFontStyle::OBLIQUE,
        "the code alone is the bare oblique, 14deg"
    );
    for code in 3..=u8::MAX {
        assert_eq!(
            style_font_style_from_u8(code),
            StyleFontStyle::Normal,
            "code {code} has no style: the initial one"
        );
    }

    // One tier-1 word for every angle: the angle is in the text tier.
    let bare = tier1_with_style(StyleFontStyle::OBLIQUE);
    for degrees in [-90.0f32, -10.0, 0.0, 10.0, 90.0] {
        assert_eq!(
            tier1_with_style(oblique(degrees)),
            bare,
            "oblique {degrees}deg has its angle outside tier 1"
        );
    }
    assert_eq!(decode_font_style(bare), StyleFontStyle::OBLIQUE);
}

#[test]
fn the_style_code_is_the_only_tier1_bits_a_font_style_touches() {
    // Every other field at its initial value encodes to zero: the word is the populated bit.
    assert_eq!(
        tier1_with_style(StyleFontStyle::Normal),
        TIER1_POPULATED_BIT
    );
    for style in [
        StyleFontStyle::Italic,
        StyleFontStyle::OBLIQUE,
        oblique(-10.0),
        oblique(90.0),
    ] {
        let t1 = tier1_with_style(style);
        assert_eq!(
            t1 & !(FONT_STYLE_MASK << FONT_STYLE_SHIFT),
            TIER1_POPULATED_BIT,
            "{style:?} wrote outside its style bits"
        );
        assert_eq!(
            (t1 >> FONT_STYLE_SHIFT) & FONT_STYLE_MASK,
            u64::from(style_font_style_to_u8(style)),
            "{style:?} is not at FONT_STYLE_SHIFT"
        );
    }
}

#[test]
fn the_oblique_slot_is_the_angle_times_64_in_an_i16() {
    assert_eq!(
        encode_font_oblique_slot(StyleFontStyle::OBLIQUE),
        896,
        "a bare `oblique` is 14deg"
    );
    assert_eq!(encode_font_oblique_slot(oblique(-10.0)), -640);
    assert_eq!(encode_font_oblique_slot(oblique(10.0)), 640);
    assert_eq!(encode_font_oblique_slot(oblique(90.0)), 5_760);
    assert_eq!(encode_font_oblique_slot(oblique(-90.0)), -5_760);
    assert_eq!(encode_font_oblique_slot(oblique(0.0)), 0);
    assert_eq!(encode_font_oblique_slot(StyleFontStyle::Normal), 0);
    assert_eq!(encode_font_oblique_slot(StyleFontStyle::Italic), 0);

    // An angle beyond the range of a font style cannot wrap round the i16.
    let top = encode_font_oblique_slot(oblique(90.0));
    let bottom = encode_font_oblique_slot(oblique(-90.0));
    assert!(encode_font_oblique_slot(oblique(1000.0)) >= top);
    assert!(encode_font_oblique_slot(oblique(-1000.0)) <= bottom);
}

#[test]
fn every_eighth_of_a_degree_from_minus_90_to_90_is_kept_exactly() {
    for eighths in -720i16..=720 {
        let degrees = f32::from(eighths) / 8.0;
        let style = oblique(degrees);
        let slot = encode_font_oblique_slot(style);
        assert_eq!(slot, eighths * 8, "{degrees}deg: the slot is the angle times 64");
        assert_eq!(
            style_font_style_from_slots(2, slot),
            style,
            "{degrees}deg through slot {slot}"
        );
    }
}

#[test]
fn the_style_code_decides_and_the_oblique_slot_only_gives_the_angle() {
    // The slot is read for code 2 only: a stale angle under `normal` or `italic` is ignored.
    assert_eq!(style_font_style_from_slots(0, 896), StyleFontStyle::Normal);
    assert_eq!(style_font_style_from_slots(0, 0), StyleFontStyle::Normal);
    assert_eq!(style_font_style_from_slots(1, 896), StyleFontStyle::Italic);
    assert_eq!(style_font_style_from_slots(1, 0), StyleFontStyle::Italic);
    assert_eq!(style_font_style_from_slots(2, 896), StyleFontStyle::OBLIQUE);
    assert_eq!(style_font_style_from_slots(2, -640), oblique(-10.0));
    // `oblique 0deg` is an oblique style with no slant: code 2 with the slot 0 is not `normal`.
    assert_eq!(style_font_style_from_slots(2, 0), oblique(0.0));
    assert_ne!(style_font_style_from_slots(2, 0), StyleFontStyle::Normal);

    // Total: no code and no slot panics.
    for code in 0u8..=4 {
        for slot in (i16::MIN..=i16::MAX).step_by(7) {
            let _ = style_font_style_from_slots(code, slot);
        }
    }
}

// ---------------------------------------------------------------------------
// tier 1 without the weight
// ---------------------------------------------------------------------------

#[test]
fn a_fully_populated_tier1_word_round_trips_through_every_decoder() {
    let t1 = encode_tier1(
        LayoutDisplay::Contents,
        LayoutPosition::Sticky,
        LayoutFloat::Right,
        LayoutOverflow::Clip,
        LayoutOverflow::Scroll,
        LayoutBoxSizing::BorderBox,
        LayoutFlexDirection::ColumnReverse,
        LayoutFlexWrap::WrapReverse,
        LayoutJustifyContent::SpaceEvenly,
        LayoutAlignItems::Baseline,
        LayoutAlignContent::SpaceAround,
        LayoutWritingMode::VerticalLr,
        LayoutClear::Both,
        StyleFontStyle::OBLIQUE,
        StyleTextAlign::End,
        StyleVisibility::Collapse,
        StyleWhiteSpace::BreakSpaces,
        StyleDirection::Rtl,
        StyleVerticalAlign::TextBottom,
        StyleBorderCollapse::Collapse,
    );

    assert!(tier1_is_populated(t1));
    assert_eq!(decode_display(t1), LayoutDisplay::Contents);
    assert_eq!(decode_position(t1), LayoutPosition::Sticky);
    assert_eq!(decode_float(t1), LayoutFloat::Right);
    assert_eq!(decode_overflow_x(t1), LayoutOverflow::Clip);
    assert_eq!(decode_overflow_y(t1), LayoutOverflow::Scroll);
    assert_eq!(decode_box_sizing(t1), LayoutBoxSizing::BorderBox);
    assert_eq!(decode_flex_direction(t1), LayoutFlexDirection::ColumnReverse);
    assert_eq!(decode_flex_wrap(t1), LayoutFlexWrap::WrapReverse);
    assert_eq!(
        decode_justify_content(t1),
        LayoutJustifyContent::SpaceEvenly
    );
    assert_eq!(decode_align_items(t1), LayoutAlignItems::Baseline);
    assert_eq!(decode_align_content(t1), LayoutAlignContent::SpaceAround);
    assert_eq!(decode_writing_mode(t1), LayoutWritingMode::VerticalLr);
    assert_eq!(decode_clear(t1), LayoutClear::Both);
    assert_eq!(decode_font_style(t1), StyleFontStyle::OBLIQUE);
    assert_eq!(decode_text_align(t1), StyleTextAlign::End);
    assert_eq!(decode_visibility(t1), StyleVisibility::Collapse);
    assert_eq!(decode_white_space(t1), StyleWhiteSpace::BreakSpaces);
    assert_eq!(decode_direction(t1), StyleDirection::Rtl);
    assert_eq!(decode_vertical_align(t1), StyleVerticalAlign::TextBottom);
    assert_eq!(decode_border_collapse(t1), StyleBorderCollapse::Collapse);
}

#[test]
fn no_single_tier1_bit_moves_the_weight_or_the_stretch() {
    let mut c = CompactLayoutCache::with_capacity(1);
    for bit in 0..u64::BITS {
        c.tier1_enums[0] = 1u64 << bit;
        assert_eq!(
            c.get_font_weight(0),
            StyleFontWeight::NORMAL,
            "tier-1 bit {bit} moved the weight"
        );
        assert_eq!(
            c.get_font_stretch(0),
            StyleFontStretch::NORMAL,
            "tier-1 bit {bit} moved the stretch"
        );
    }
}

// ---------------------------------------------------------------------------
// the cache getters read the text tier
// ---------------------------------------------------------------------------

#[test]
fn the_cache_getters_read_the_weight_stretch_and_angle_from_the_text_tier() {
    let mut c = CompactLayoutCache::with_capacity(3);
    let slanted = oblique(10.0);
    c.tier2b_text[1].font_weight = encode_font_weight_slot(weight(350.5));
    c.tier2b_text[1].font_stretch = encode_font_stretch_slot(stretch(87.5));
    c.tier2b_text[1].font_oblique = encode_font_oblique_slot(slanted);
    c.tier1_enums[1] = u64::from(style_font_style_to_u8(slanted)) << FONT_STYLE_SHIFT;

    assert_eq!(c.get_font_weight(1), weight(350.5));
    assert_eq!(c.get_font_stretch(1), stretch(87.5));
    assert_eq!(c.get_font_style(1), slanted);

    // The neighbours hold the initial values.
    for node in [0, 2] {
        assert_eq!(c.get_font_weight(node), StyleFontWeight::NORMAL, "node {node}");
        assert_eq!(
            c.get_font_stretch(node),
            StyleFontStretch::NORMAL,
            "node {node}"
        );
        assert_eq!(c.get_font_style(node), StyleFontStyle::Normal, "node {node}");
    }
}

#[test]
fn the_oblique_slot_is_read_only_where_the_style_code_says_oblique() {
    let mut c = CompactLayoutCache::with_capacity(4);
    // Every node holds the slot of 10deg; only the style code differs.
    for text in &mut c.tier2b_text {
        text.font_oblique = 640;
    }
    c.tier1_enums[0] = 0;
    c.tier1_enums[1] = 1u64 << FONT_STYLE_SHIFT;
    c.tier1_enums[2] = 2u64 << FONT_STYLE_SHIFT;
    c.tier1_enums[3] = 2u64 << FONT_STYLE_SHIFT;
    c.tier2b_text[3].font_oblique = 0;

    assert_eq!(c.get_font_style(0), StyleFontStyle::Normal);
    assert_eq!(c.get_font_style(1), StyleFontStyle::Italic);
    assert_eq!(c.get_font_style(2), oblique(10.0));
    assert_eq!(
        c.get_font_style(3),
        oblique(0.0),
        "an `oblique 0deg` is still an oblique"
    );
}

#[test]
fn the_style_code_and_the_text_slots_do_not_read_each_other() {
    let mut c = CompactLayoutCache::with_capacity(1);
    c.tier2b_text[0].font_weight = weight_slot(900.0);
    c.tier2b_text[0].font_stretch = encode_font_stretch_slot(stretch(50.0));
    c.tier2b_text[0].font_oblique = 5_760;
    c.tier1_enums[0] = TIER1_POPULATED_BIT;
    // A word with no style code is `normal` however full the text tier is.
    assert_eq!(c.get_font_style(0), StyleFontStyle::Normal);
    assert_eq!(c.get_font_weight(0), weight(900.0));
    assert_eq!(c.get_font_stretch(0), stretch(50.0));
}

// ---------------------------------------------------------------------------
// the DOM flag word
// ---------------------------------------------------------------------------

#[test]
fn the_dom_flags_are_a_u64_and_the_six_new_ones_fill_bits_32_to_37() {
    // The flags that were there keep a bit of the low half of the wider word.
    let low: [u64; 3] = [
        DOM_HAS_SHAPE_INSIDE,
        DOM_HAS_VERTICAL_ALIGN,
        DOM_HAS_ZOOM_OR_ASPECT_RATIO,
    ];
    for flag in low {
        assert_eq!(flag.count_ones(), 1, "{flag:#x}");
        assert!(flag < 1u64 << 32, "{flag:#x} is not in the low half");
    }

    let mut seen = 0u64;
    for (name, flag) in NEW_FONT_FLAGS {
        assert_eq!(flag.count_ones(), 1, "{name}: {flag:#x} is not a single bit");
        assert!(
            (32..=37).contains(&flag.trailing_zeros()),
            "{name}: {flag:#x} is not in bits 32..=37"
        );
        assert_eq!(
            seen & flag,
            0,
            "{name}: {flag:#x} shares its bit with another new flag"
        );
        seen |= flag;
    }
    assert_eq!(seen, 0x3F << 32, "six flags fill the six bits 32..=37");
}

#[test]
fn dom_declared_answers_the_wide_flag_word() {
    let mut c = CompactLayoutCache::empty();
    let word: u64 = c.dom_declared_flags;
    assert_eq!(word, 0);
    for (name, flag) in NEW_FONT_FLAGS {
        assert!(!c.dom_declared(flag), "{name} is declared in an empty cache");
    }
    assert!(!c.dom_declared(0), "an empty query is never declared");

    // One new flag declared: only that query is true, and an old flag does not answer for it.
    for (declared_name, declared) in NEW_FONT_FLAGS {
        c.dom_declared_flags = declared;
        for (asked_name, asked) in NEW_FONT_FLAGS {
            assert_eq!(
                c.dom_declared(asked),
                asked == declared,
                "{asked_name} asked with only {declared_name} declared"
            );
        }
        assert!(
            !c.dom_declared(DOM_HAS_LINE_HEIGHT),
            "{declared_name} answered for a flag of the low half"
        );
        assert!(
            c.dom_declared(declared | DOM_HAS_LINE_HEIGHT),
            "a mask is declared when any of its bits is"
        );
    }

    // An old flag declared does not answer for a new one.
    c.dom_declared_flags = DOM_HAS_LINE_HEIGHT | DOM_HAS_ZOOM_OR_ASPECT_RATIO;
    for (name, flag) in NEW_FONT_FLAGS {
        assert!(!c.dom_declared(flag), "{name} answered for the low half");
    }
    assert!(c.dom_declared(DOM_HAS_ZOOM_OR_ASPECT_RATIO));

    // Bit 37 is the last one the word holds: it is not cut off.
    c.dom_declared_flags = 1u64 << 37;
    let answering = NEW_FONT_FLAGS
        .iter()
        .filter(|(_, flag)| c.dom_declared(*flag))
        .count();
    assert_eq!(answering, 1, "exactly the flag of bit 37 answers");

    // Everything declared: every query is true.
    c.dom_declared_flags = u64::MAX;
    for (name, flag) in NEW_FONT_FLAGS {
        assert!(c.dom_declared(flag), "{name}");
    }
    assert!(!c.dom_declared(0));
}

#[test]
fn a_fresh_cache_has_no_flag_and_with_capacity_matches_empty() {
    for n in [0usize, 1, 5] {
        let c = CompactLayoutCache::with_capacity(n);
        let word: u64 = c.dom_declared_flags;
        assert_eq!(word, 0, "{n} nodes");
    }
    assert_eq!(
        CompactLayoutCache::with_capacity(0),
        CompactLayoutCache::empty()
    );
}
