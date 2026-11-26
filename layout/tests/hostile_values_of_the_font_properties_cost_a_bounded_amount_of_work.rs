//! A hostile value of a font property costs a bounded amount of work: a `font-feature-settings`
//! list of fifty thousand tags keeps its first sixty-four (and says so in the stylesheet parser),
//! a style that is made by hand with a hundred thousand feature entries shapes in the time of the
//! first hundred and twenty-eight, and the properties whose grammar bounds them (the ligature
//! groups, the East Asian forms, the synthesis switches) read a value of any length in linear
//! time.
//!
//! Each test is a loop that used to be quadratic or that could be: the old reader of
//! `font-feature-settings` scanned everything it had kept for every tag (fifty thousand tags took
//! seconds), as `font-variation-settings` did (21 s).

use std::{sync::Arc, time::{Duration, Instant}};

use azul_layout::{
    font::parsed::ParsedFont,
    text3::{
        cache::{BidiDirection, StyleProperties},
        default::shape_text_for_parsed_font,
        script::{Language, Script},
    },
};
use rust_fontconfig::FontBytes;

use crate::text_rig::{export_page, face_bytes, laid_out, ids, FEATURES, MONO};

/// `count` distinct four-character tags: `zaaa`, `zaab`, ...
fn tags(count: u32) -> Vec<String> {
    let letter = |n: u32| {
        char::from(if n < 26 {
            b'a' + u8::try_from(n).unwrap()
        } else {
            b'A' + u8::try_from(n - 26).unwrap()
        })
    };
    (0..count)
        .map(|index| {
            let (a, b, c) = (index / 2704, (index / 52) % 52, index % 52);
            format!("z{}{}{}", letter(a), letter(b), letter(c))
        })
        .collect()
}

fn within(limit: Duration, what: &str, work: impl FnOnce()) {
    let start = Instant::now();
    work();
    let took = start.elapsed();
    assert!(took < limit, "{what} took {took:?}");
}

#[test]
fn fifty_thousand_feature_settings_lay_out_in_bounded_time() {
    let list = tags(50_000)
        .iter()
        .map(|tag| format!("\"{tag}\" 1"))
        .collect::<Vec<_>>()
        .join(", ");
    let css = format!("font-feature-settings: {list}");
    within(Duration::from_secs(10), "50,000 feature settings", || {
        let (_lw, dl) = laid_out(FEATURES, &css, "fi fi fi", 800.0);
        assert!(!ids(&dl).is_empty(), "the text is drawn");
    });
}

/// A style that does not come from the CSS parser has no cap before the shaper: the entries it
/// reads are the first `MAX_FEATURE_ENTRIES`.
#[test]
fn a_style_with_a_hundred_thousand_feature_entries_shapes_in_bounded_time() {
    let bytes = face_bytes("azul-mock-features.ttf");
    let shared = Arc::new(FontBytes::Owned(Arc::from(bytes.as_slice())));
    let font = ParsedFont::from_bytes(&bytes, 0, &mut Vec::new())
        .expect("the face parses")
        .with_source_bytes(shared);
    let style = StyleProperties {
        font_size_px: 20.0,
        font_features: tags(100_000).iter().map(|tag| format!("{tag}=1")).collect(),
        ..StyleProperties::default()
    };
    within(Duration::from_secs(10), "100,000 feature entries", || {
        let glyphs = shape_text_for_parsed_font(
            &font,
            "fi fi fi",
            Script::Latin,
            Language::EnglishUS,
            BidiDirection::Ltr,
            &style,
        )
        .expect("shaping succeeds");
        assert!(!glyphs.is_empty());
    });
}

/// The values the grammar bounds are read in time proportional to their length: a value of a
/// hundred thousand keywords is no value, and costs a few lookups.
#[test]
fn long_values_of_the_bounded_properties_are_read_in_bounded_time() {
    let ligatures = "no-common-ligatures ".repeat(50_000);
    let east_asian = "jis78 ".repeat(50_000);
    let synthesis = "weight ".repeat(50_000);
    within(Duration::from_secs(10), "long values", || {
        for css in [
            format!("font-variant-ligatures: {ligatures}"),
            format!("font-variant-east-asian: {east_asian}"),
            format!("font-synthesis: {synthesis}"),
            format!("font-kerning: {}", "none ".repeat(50_000)),
        ] {
            let (_lw, dl) = laid_out(FEATURES, &css, "fi", 800.0);
            // an invalid value is dropped: the text is set as it is
            assert_eq!(ids(&dl), crate::text_rig::ids_of(FEATURES, "", "fi"), "{}", &css[..40]);
        }
    });
}

/// Ten thousand blocks of combined digits are laid out in time proportional to their number.
#[test]
fn ten_thousand_combined_blocks_are_laid_out_in_bounded_time() {
    let digits = "20".repeat(10_000);
    within(Duration::from_secs(20), "10,000 combined blocks", || {
        let exported = export_page(
            MONO,
            "writing-mode: vertical-rl; text-combine-upright: digits 2",
            &digits,
        );
        assert!(!exported.pdf_runs().is_empty());
    });
}

/// A tab asks where its line has got to: five thousand tabs on one line are read once each, not
/// each against all that stands before it.
#[test]
fn five_thousand_tabs_on_one_line_are_laid_out_in_bounded_time() {
    let text = "a\t".repeat(5_000);
    within(Duration::from_secs(20), "5,000 tabs", || {
        let (_lw, dl) = laid_out(MONO, "white-space: pre; tab-size: 2", &text, 400.0);
        assert!(!ids(&dl).is_empty(), "the text is drawn");
    });
}
