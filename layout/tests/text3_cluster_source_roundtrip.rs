#![cfg(feature = "text_layout")]
//! T1 SEED — cluster→source reproduction (SHAPED_TEXT_REFACTOR_PLAN §2.3,
//! property 2 of the T1 spec, the single invariant that AUTHORISES
//! deleting `ShapedCluster::text` later: concatenating every cluster's
//! source slice, ordered by `(source_run, start_byte_in_run)`, must
//! reproduce the input text EXACTLY — across bidi reordering, ligature
//! folding, soft hyphens, tabs and CJK. If a cluster's byte range ever
//! drifts from the text it claims to represent, selection, caret motion
//! and PDF extraction all silently corrupt; this pins the mapping at the
//! shaping level for a corpus of adversarial inputs.
//!
//! The remaining T1 properties (round-trip via byte_offset_to_cluster_id,
//! strict monotonicity per run, shaped_item_source equivalence,
//! source_node_id attribution) extend this file as the §3.2 refactor
//! reaches the fields they guard.

use std::collections::HashMap;
use std::sync::Arc;

use azul_css::props::basic::FontRef;
use azul_layout::font::parsed::ParsedFont;
use azul_layout::parsed_font_to_font_ref;
use azul_layout::text3::cache::{
    create_logical_items, reorder_logical_items, shape_visual_items, BidiDirection, FontStack,
    InlineContent, LoadedFonts, ShapedItem, StyleProperties, StyledRun, UnicodeBidi,
};
use rust_fontconfig::{FcFontCache, FontBytes, FontFallbackChain, FontId};

use crate::fakefont;

fn test_font() -> ParsedFont {
    let bytes = fakefont::simple_test_font();
    let arc = Arc::new(FontBytes::Owned(Arc::from(bytes.as_slice())));
    ParsedFont::from_bytes(&bytes, 0, &mut Vec::new())
        .expect("test font must parse")
        .with_source_bytes(arc)
}

fn shape(text: &str, dir: BidiDirection) -> Vec<ShapedItem> {
    let font_ref: FontRef = parsed_font_to_font_ref(test_font());
    let style = Arc::new(StyleProperties {
        font_stack: FontStack::Ref(font_ref.clone()),
        font_size_px: 16.0,
        ..StyleProperties::default()
    });
    let content = vec![InlineContent::Text(StyledRun {
        text: Arc::from(text),
        style,
        logical_start_byte: 0,
        source_node_id: None,
    })];
    let logical = create_logical_items(&content, &[], &mut None);
    let visual =
        reorder_logical_items(&logical, dir, UnicodeBidi::Normal, &mut None).expect("bidi");
    let mut loaded: LoadedFonts<FontRef> = LoadedFonts::new();
    loaded.insert(FontId::new(), font_ref);
    let chain: HashMap<_, FontFallbackChain> = HashMap::new();
    let fc = FcFontCache::default();
    shape_visual_items(&visual, &chain, &fc, &loaded, &mut None).expect("shape")
}

/// Property 2: source slices, in logical order, reproduce the input.
fn assert_source_roundtrip(input: &str, dir: BidiDirection) {
    let shaped = shape(input, dir);
    let mut spans: Vec<(u32, u32, String)> = shaped
        .iter()
        .filter_map(|it| match it {
            ShapedItem::Cluster(c) => Some((
                c.source_cluster_id.source_run,
                c.source_cluster_id.start_byte_in_run,
                c.text().to_string(),
            )),
            _ => None,
        })
        .collect();
    assert!(!spans.is_empty(), "input {input:?} shaped to zero clusters");
    spans.sort_by_key(|(run, byte, _)| (*run, *byte));
    // Byte-position agreement: each cluster's claimed start must equal the
    // running length of everything before it (no gaps, no overlaps)...
    let mut reconstructed = String::new();
    for (_, start, text) in &spans {
        assert_eq!(
            *start as usize,
            reconstructed.len(),
            "cluster {text:?} claims byte {start} but {} bytes precede it \
             (input {input:?})",
            reconstructed.len()
        );
        reconstructed.push_str(text);
    }
    // ...and the concatenation reproduces the input exactly.
    assert_eq!(
        reconstructed, input,
        "source slices do not reproduce the input (dir {dir:?})"
    );
}

/// Property 3: `source_cluster_id` values are strictly increasing within
/// a run and unique across the layout — bidi reordering must not collide
/// them (`VisualItem::run_byte_offset` exists precisely to prevent this).
fn assert_ids_monotonic_and_unique(input: &str, dir: BidiDirection) {
    let shaped = shape(input, dir);
    let mut seen = std::collections::HashSet::new();
    let mut per_run: HashMap<u32, u32> = HashMap::new();
    let mut in_logical_order: Vec<(u32, u32)> = Vec::new();
    for it in &shaped {
        if let ShapedItem::Cluster(c) = it {
            let key = (
                c.source_cluster_id.source_run,
                c.source_cluster_id.start_byte_in_run,
            );
            assert!(
                seen.insert(key),
                "duplicate source_cluster_id {key:?} (input {input:?})"
            );
            in_logical_order.push(key);
        }
    }
    in_logical_order.sort_unstable();
    for (run, byte) in in_logical_order {
        let prev = per_run.entry(run).or_insert(0);
        assert!(
            byte >= *prev,
            "non-monotonic byte {byte} after {prev} in run {run} (input {input:?})"
        );
        *prev = byte;
    }
}

#[test]
fn ids_unique_and_monotonic_across_corpus() {
    for (text, dir) in [
        ("hello world", BidiDirection::Ltr),
        ("abc \u{05d0}\u{05d1}\u{05d2} def", BidiDirection::Ltr),
        (
            "\u{05e9}\u{05dc}\u{05d5}\u{05dd} shalom",
            BidiDirection::Rtl,
        ),
        ("waffle office ffi", BidiDirection::Ltr),
        ("co\u{00ad}op\tend", BidiDirection::Ltr),
    ] {
        assert_ids_monotonic_and_unique(text, dir);
    }
}

#[test]
fn ascii_reproduces() {
    assert_source_roundtrip("hello world", BidiDirection::Ltr);
}

#[test]
fn accents_and_combining_marks_reproduce() {
    assert_source_roundtrip(
        "re\u{0301}sume\u{0301} \u{00e4}\u{00f6}\u{00fc}",
        BidiDirection::Ltr,
    );
}

#[test]
fn hebrew_rtl_reproduces_in_logical_order() {
    assert_source_roundtrip(
        "\u{05e9}\u{05dc}\u{05d5}\u{05dd} shalom",
        BidiDirection::Rtl,
    );
}

#[test]
fn mixed_bidi_reproduces() {
    assert_source_roundtrip("abc \u{05d0}\u{05d1}\u{05d2} def", BidiDirection::Ltr);
}

#[test]
fn ligature_text_reproduces() {
    // 'ffi' may fold to one glyph cluster; the SOURCE bytes must survive.
    assert_source_roundtrip("waffle office ffi", BidiDirection::Ltr);
}

#[test]
fn soft_hyphens_and_tabs_reproduce() {
    assert_source_roundtrip("co\u{00ad}op\tend", BidiDirection::Ltr);
}

#[test]
fn cjk_reproduces() {
    assert_source_roundtrip("\u{6c34}\u{5e73}\u{7dda} text", BidiDirection::Ltr);
}
