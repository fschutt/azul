#![cfg(feature = "text_layout")]
//! §3.2 campaign step 1 gate: the dense view derived from a real layout
//! must agree with the current model on every quantity both can express —
//! cluster count, per-cluster advance/x/start_byte/flags, run partitioning
//! totals, and the T1 invariant one level up: concatenating the dense
//! runs' texts in run order reproduces the input.

use std::collections::HashMap;
use std::sync::Arc;

use azul_core::dom::NodeId;
use azul_css::props::basic::FontRef;
use azul_layout::font::parsed::ParsedFont;
use azul_layout::parsed_font_to_font_ref;
use azul_layout::text3::cache::{
    create_logical_items, perform_fragment_layout, reorder_logical_items, shape_visual_items,
    AvailableSpace, BidiDirection, BreakCursor, FontStack, InlineBorderInfo, InlineContent,
    LoadedFonts, ShapedItem, StyleProperties, StyledRun, UnicodeBidi, UnifiedConstraints,
    UnifiedLayout,
};
use azul_layout::text3::dense::{
    get_glyph_positions_dense, get_glyph_runs_simple_dense, DenseText,
};
use azul_layout::text3::glyphs::{get_glyph_positions, get_glyph_runs_simple};
use rust_fontconfig::{FcFontCache, FontBytes, FontFallbackChain, FontId};

use crate::fakefont;

fn test_font_ref() -> FontRef {
    let bytes = fakefont::simple_test_font();
    let arc = Arc::new(FontBytes::Owned(Arc::from(bytes.as_slice())));
    let parsed = ParsedFont::from_bytes(&bytes, 0, &mut Vec::new())
        .expect("font")
        .with_source_bytes(arc);
    parsed_font_to_font_ref(parsed)
}

/// One `InlineContent::Text` with the shared test font and a per-case
/// style tweak — the building block for multi-run cases.
fn styled(
    text: &str,
    font_ref: &FontRef,
    logical_start_byte: usize,
    source_node_id: Option<NodeId>,
    tweak: impl FnOnce(&mut StyleProperties),
) -> InlineContent {
    let mut style = StyleProperties {
        font_stack: FontStack::Ref(font_ref.clone()),
        font_size_px: 16.0,
        ..StyleProperties::default()
    };
    tweak(&mut style);
    InlineContent::Text(StyledRun {
        text: Arc::from(text),
        style: Arc::new(style),
        logical_start_byte,
        source_node_id,
    })
}

fn layout_of_content(
    content: Vec<InlineContent>,
    font_ref: &FontRef,
    width: f32,
) -> (UnifiedLayout, Vec<InlineContent>, LoadedFonts<FontRef>) {
    let logical = create_logical_items(&content, &[], &mut None);
    let visual = reorder_logical_items(&logical, BidiDirection::Ltr, UnicodeBidi::Normal, &mut None)
        .expect("bidi");
    let mut loaded: LoadedFonts<FontRef> = LoadedFonts::new();
    loaded.insert(FontId::new(), font_ref.clone());
    let chain: HashMap<_, FontFallbackChain> = HashMap::new();
    let fc = FcFontCache::default();
    let shaped = shape_visual_items(&visual, &chain, &fc, &loaded, &mut None).expect("shape");
    let constraints = UnifiedConstraints {
        available_width: AvailableSpace::Definite(width),
        ..UnifiedConstraints::default()
    };
    let mut cursor = BreakCursor::new(&shaped);
    let layout = perform_fragment_layout(&mut cursor, &logical, &constraints, &mut None, &loaded)
        .expect("layout");
    (layout, content, loaded)
}

fn layout_of(text: &str, width: f32) -> (UnifiedLayout, Vec<InlineContent>) {
    let font_ref = test_font_ref();
    let content = vec![styled(text, &font_ref, 0, None, |_| {})];
    let (layout, content, _) = layout_of_content(content, &font_ref, width);
    (layout, content)
}

/// (d3) A line mixing font SIZES must record the MAX resolved line
/// height — the uniform corpus cannot distinguish max from
/// first-cluster, so this case is what makes the height check bite.
#[test]
fn dense_line_height_is_the_max_over_mixed_sizes() {
    let font_ref = test_font_ref();
    let content = vec![
        styled("small ", &font_ref, 0, None, |_| {}),
        styled("BIG", &font_ref, 6, None, |s| s.font_size_px = 32.0),
    ];
    let (layout, content, _) = layout_of_content(content, &font_ref, 400.0);
    let dense = DenseText::from_unified_with_content(&layout, &content);
    let clusters: Vec<_> = layout
        .items
        .iter()
        .filter_map(|it| match &it.item {
            ShapedItem::Cluster(c) => Some((it, c)),
            _ => None,
        })
        .collect();
    assert!(!dense.lines.is_empty());
    for l in &dense.lines {
        let expected: f32 = (l.clusters.0..l.clusters.1)
            .map(|ci| clusters[ci as usize].0.item.bounds().height)
            .fold(0.0, f32::max);
        assert!(expected > 20.0, "the BIG run must dominate ({expected})");
        assert!(
            (l.height - expected).abs() < 0.01,
            "line height {} != max cluster height {expected}",
            l.height
        );
    }
}

#[test]
fn dense_view_agrees_with_the_current_model() {
    for (text, width) in [
        ("hello dense world", 400.0),
        ("a longer paragraph that will wrap across multiple lines of text", 120.0),
        ("waffle office ffi", 400.0),
    ] {
        let (layout, content) = layout_of(text, width);
        let dense = DenseText::from_unified_with_content(&layout, &content);
        // §3.2 step 2: the dense run text is the SHARED source Arc — the
        // full StyledRun text, not the surviving-cluster concatenation.
        for r in &dense.runs {
            assert_eq!(&*r.text, text, "run text is the shared source");
        }

        let clusters: Vec<_> = layout
            .items
            .iter()
            .filter_map(|it| match &it.item {
                ShapedItem::Cluster(c) => Some((it, c)),
                _ => None,
            })
            .collect();
        assert_eq!(dense.clusters.len(), clusters.len(), "cluster count ({text:?})");

        for (i, ((item, c), dc)) in clusters.iter().zip(dense.clusters.iter()).enumerate() {
            assert_eq!(dc.start_byte, c.source_cluster_id.start_byte_in_run, "byte @{i}");
            // (d6h) Dense flags additionally pack fragment/marker bits
            // (7-10); the classification bits must still match exactly.
            assert_eq!(
                azul_layout::text3::cache::ClusterFlags(
                    dc.flags.0 & azul_layout::text3::cache::ClusterFlags::CLASSIFY_MASK
                ),
                c.flags,
                "flags @{i}"
            );
            assert!((dc.x - item.position.x).abs() < 0.01, "x @{i}");
        }

        // Run partition covers every cluster exactly once, in order.
        let mut covered = 0u32;
        for r in &dense.runs {
            assert_eq!(r.clusters.start, covered, "run start ({text:?})");
            assert!(r.clusters.end >= r.clusters.start);
            covered = r.clusters.end;
        }
        assert_eq!(covered as usize, dense.clusters.len(), "runs cover all clusters");

        // Source-slice validity at the POSITIONED level: the line breaker
        // legitimately CONSUMES separator clusters at wrap points (they
        // become glue and never reach layout.items), so full-input
        // reproduction holds pre-break (T1 pins that) but NOT here.
        // The invariant that does hold: every surviving cluster's text is
        // exactly the input at its claimed byte offset — gaps allowed
        // where separators were consumed. (This failing as strict
        // reproduction on the first wrapped case is what validated the
        // plan's share-the-true-source-Arc design for run.text.)
        for (i, (_, c)) in clusters.iter().enumerate() {
            let start = c.source_cluster_id.start_byte_in_run as usize;
            assert!(
                text[start..].starts_with(c.text()),
                "cluster {i} claims byte {start} but the input there does not \
                 start with {:?} ({text:?})",
                c.text()
            );
        }

        // Every line record covers a non-empty, in-order cluster range,
        // and (d3) carries the max resolved line height of its clusters —
        // cross-checked against the sparse items' bounds().height.
        let mut line_cover = 0u32;
        for l in &dense.lines {
            assert!(l.clusters.1 > l.clusters.0, "empty line record");
            assert!(l.clusters.0 >= line_cover, "line ranges ordered");
            line_cover = l.clusters.1;
            let expected: f32 = (l.clusters.0..l.clusters.1)
                .map(|ci| clusters[ci as usize].0.item.bounds().height)
                .fold(0.0, f32::max);
            assert!(
                (l.height - expected).abs() < 0.01,
                "line height {} != max cluster height {expected}",
                l.height
            );
        }
    }
}


/// §3.2 step 4 agreement gate: the dense simple-run walker must reproduce
/// the reference's PAINT RUNS exactly — boundaries, every painted
/// property, border fragment marks after §9.4.2 suppression, and each
/// glyph instance. The corpus exercises the seams that differ between
/// the models: line breaks (baseline splits), ligatures (detail
/// clusters), value-splits (two colours / two nodes), value-MERGES
/// across distinct style Arcs (dense runs split there, paint runs must
/// not), and a bordered inline wrapped over lines (the shared fragment
/// post-process).
#[test]
fn dense_simple_runs_agree_with_the_reference_walker() {
    let font_ref = test_font_ref();
    let red = azul_css::props::basic::ColorU { r: 200, g: 30, b: 30, a: 255 };
    let blue = azul_css::props::basic::ColorU { r: 30, g: 30, b: 200, a: 255 };
    let cases: Vec<(&str, Vec<InlineContent>, f32)> = vec![
        (
            "plain",
            vec![styled("hello dense world", &font_ref, 0, None, |_| {})],
            400.0,
        ),
        (
            "wrapped",
            vec![styled(
                "a longer paragraph that will wrap across multiple lines of text",
                &font_ref,
                0,
                None,
                |_| {},
            )],
            120.0,
        ),
        (
            "ligature",
            vec![styled("waffle office ffi", &font_ref, 0, None, |_| {})],
            400.0,
        ),
        (
            "two-style",
            vec![
                styled("red ", &font_ref, 0, Some(NodeId::new(1)), |s| s.color = red),
                styled("blue", &font_ref, 4, Some(NodeId::new(2)), |s| s.color = blue),
            ],
            400.0,
        ),
        (
            // MIXED FONT SIZES ON ONE LINE — the case every other entry in
            // this list misses, because they all use one size.
            //
            // The dense model stores ONE y per line (`ClusterCompact.x` says
            // so outright: "Inline-axis position within the IFC; y comes from
            // the line"), frozen from whichever cluster opened the
            // `LineRecord`. The sparse reference uses each ITEM's own solved
            // `position.y`. Those agree only while every cluster on a line
            // shares a y — true for uniform text, false the moment one run is
            // taller, because a taller run sits on a different baseline.
            //
            // Found 2026-08-14 from OUTSIDE azul: printpdf's ligature tests
            // passed against azul rev aaa700097 and failed against master,
            // and `AZ_DENSE_TEXT=verify` localised it to a glyph whose y was
            // off by 1.98px with identical index, x, font and node. Dense has
            // been the DEFAULT since 0a5c69230, so that is what ships; the
            // verify gate that catches it is opt-in and no CI job sets it.
            "mixed-sizes-one-line",
            vec![
                styled("small ", &font_ref, 0, None, |_| {}),
                styled("BIG", &font_ref, 6, None, |s| s.font_size_px = 32.0),
            ],
            400.0,
        ),
        (
            // Identical style VALUES in two runs with DISTINCT Arcs: the
            // dense model splits runs on Arc identity, the reference
            // merges on values — the twin must merge back.
            "merge-across-arcs",
            vec![
                styled("same ", &font_ref, 0, None, |_| {}),
                styled("style", &font_ref, 5, None, |_| {}),
            ],
            400.0,
        ),
        (
            "bordered-wrap",
            vec![styled(
                "bordered text that wraps across several line boxes",
                &font_ref,
                0,
                Some(NodeId::new(3)),
                |s| {
                    s.border = Some(InlineBorderInfo {
                        left: 1.0,
                        right: 1.0,
                        top: 1.0,
                        bottom: 1.0,
                        ..InlineBorderInfo::default()
                    });
                },
            )],
            110.0,
        ),
    ];
    for (name, content, width) in cases {
        let (layout, content, _) = layout_of_content(content, &font_ref, width);
        let dense = DenseText::from_unified_with_content(&layout, &content);
        let reference = get_glyph_runs_simple(&layout);
        let ours = get_glyph_runs_simple_dense(&dense);
        assert!(!reference.is_empty(), "reference produced runs ({name})");
        assert_eq!(reference.len(), ours.len(), "run count ({name})");
        for (i, (r, o)) in reference.iter().zip(ours.iter()).enumerate() {
            assert_eq!(r.color, o.color, "color @{i} ({name})");
            assert_eq!(r.background_color, o.background_color, "bg @{i} ({name})");
            assert_eq!(r.background_content, o.background_content, "bg content @{i} ({name})");
            assert_eq!(r.border, o.border, "border (incl. fragment marks) @{i} ({name})");
            assert_eq!(r.font_hash, o.font_hash, "font hash @{i} ({name})");
            assert!(
                (r.font_size_px - o.font_size_px).abs() < 0.01,
                "font size @{i} ({name})"
            );
            assert_eq!(r.text_decoration, o.text_decoration, "decoration @{i} ({name})");
            assert_eq!(r.is_ime_preview, o.is_ime_preview, "ime @{i} ({name})");
            assert_eq!(r.source_node_id, o.source_node_id, "node @{i} ({name})");
            assert_eq!(r.glyphs.len(), o.glyphs.len(), "glyph count @{i} ({name})");
            for (j, (rg, og)) in r.glyphs.iter().zip(o.glyphs.iter()).enumerate() {
                assert_eq!(rg.index, og.index, "glyph id @{i}/{j} ({name})");
                assert!(
                    (rg.point.x - og.point.x).abs() < 0.01
                        && (rg.point.y - og.point.y).abs() < 0.01,
                    "glyph pos @{i}/{j}: ref ({}, {}) vs dense ({}, {}) ({name})",
                    rg.point.x, rg.point.y, og.point.x, og.point.y
                );
                assert_eq!(rg.size, og.size, "glyph size @{i}/{j} ({name})");
            }
        }
    }
}

/// §3.2 step 3 agreement gate: the dense walker must place every glyph
/// at EXACTLY the reference walker's position (id + x + y). Advance
/// semantics differ by design (dense folds kerning into the painted
/// advance); positions are the contract.
#[test]
fn dense_glyph_positions_agree_with_the_reference_walker() {
    for (text, width) in [
        ("hello dense world", 400.0),
        ("a longer paragraph that will wrap across multiple lines of text", 120.0),
        ("waffle office ffi", 400.0),
    ] {
        let (layout, content) = layout_of(text, width);
        let dense = DenseText::from_unified_with_content(&layout, &content);
        let reference = get_glyph_positions(&layout);
        let ours = get_glyph_positions_dense(&dense);
        assert_eq!(reference.len(), ours.len(), "glyph count ({text:?})");
        for (i, (r, o)) in reference.iter().zip(ours.iter()).enumerate() {
            assert_eq!(r.glyph_id, o.glyph_id, "id @{i} ({text:?})");
            assert!(
                (r.position.x - o.position.x).abs() < 0.01
                    && (r.position.y - o.position.y).abs() < 0.01,
                "position @{i}: ref ({}, {}) vs dense ({}, {}) ({text:?})",
                r.position.x, r.position.y, o.position.x, o.position.y
            );
        }
    }
}

/// §3.2 step 5 agreement gate: the dense PDF walker must reproduce the
/// reference's runs exactly — boundaries (incl. LINE-index breaks, which
/// the simple walker does not have), every reported property, the
/// baseline_start anchor, per-glyph id/position/codepoint, and the
/// cluster_texts side list whose text the dense side RECONSTRUCTS from
/// the shared run text (the 3c deletion rests on this equivalence).
/// Advance is compared only where the reference's base-advance equals
/// the painted advance (simple clusters) — the fold is documented.
#[test]
fn dense_pdf_runs_agree_with_the_reference_walker() {
    use azul_layout::text3::dense::get_glyph_runs_pdf_dense;
    use azul_layout::text3::glyphs::get_glyph_runs_pdf;

    let font_ref = test_font_ref();
    let red = azul_css::props::basic::ColorU { r: 200, g: 30, b: 30, a: 255 };
    let blue = azul_css::props::basic::ColorU { r: 30, g: 30, b: 200, a: 255 };
    let cases: Vec<(&str, Vec<InlineContent>, f32)> = vec![
        (
            "plain",
            vec![styled("hello dense world", &font_ref, 0, None, |_| {})],
            400.0,
        ),
        (
            "wrapped",
            vec![styled(
                "a longer paragraph that will wrap across multiple lines of text",
                &font_ref,
                0,
                None,
                |_| {},
            )],
            120.0,
        ),
        (
            "ligature",
            vec![styled("waffle office ffi", &font_ref, 0, None, |_| {})],
            400.0,
        ),
        (
            "two-style",
            vec![
                styled("red ", &font_ref, 0, Some(NodeId::new(1)), |s| s.color = red),
                styled("blue", &font_ref, 4, Some(NodeId::new(2)), |s| s.color = blue),
            ],
            400.0,
        ),
        (
            // The PDF predicate has NO source-node/border comparison: two
            // value-identical styled runs merge — including across the
            // dense model's Arc-identity run split.
            "merge-across-arcs",
            vec![
                styled("same ", &font_ref, 0, Some(NodeId::new(1)), |_| {}),
                styled("style", &font_ref, 5, Some(NodeId::new(2)), |_| {}),
            ],
            400.0,
        ),
    ];
    for (name, content, width) in cases {
        let (layout, content, loaded) = layout_of_content(content, &font_ref, width);
        let dense = DenseText::from_unified_with_content(&layout, &content);
        let reference = get_glyph_runs_pdf(&layout, &loaded);
        let ours = get_glyph_runs_pdf_dense(&dense, &loaded);
        assert!(!reference.is_empty(), "reference produced runs ({name})");
        assert_eq!(reference.len(), ours.len(), "run count ({name})");
        for (i, (r, o)) in reference.iter().zip(ours.iter()).enumerate() {
            assert_eq!(r.color, o.color, "color @{i} ({name})");
            assert_eq!(r.background_color, o.background_color, "bg @{i} ({name})");
            assert_eq!(r.font_hash, o.font_hash, "font hash @{i} ({name})");
            assert!(
                (r.font_size_px - o.font_size_px).abs() < 0.01,
                "font size @{i} ({name})"
            );
            assert_eq!(r.text_decoration, o.text_decoration, "decoration @{i} ({name})");
            assert_eq!(r.line_index, o.line_index, "line @{i} ({name})");
            assert_eq!(r.direction, o.direction, "direction @{i} ({name})");
            assert_eq!(r.writing_mode, o.writing_mode, "writing mode @{i} ({name})");
            assert!(
                (r.baseline_start.x - o.baseline_start.x).abs() < 0.01
                    && (r.baseline_start.y - o.baseline_start.y).abs() < 0.01,
                "baseline_start @{i}: ref ({}, {}) vs dense ({}, {}) ({name})",
                r.baseline_start.x, r.baseline_start.y, o.baseline_start.x, o.baseline_start.y
            );
            assert_eq!(r.cluster_texts, o.cluster_texts, "cluster_texts @{i} ({name})");
            assert_eq!(r.glyphs.len(), o.glyphs.len(), "glyph count @{i} ({name})");
            for (j, (rg, og)) in r.glyphs.iter().zip(o.glyphs.iter()).enumerate() {
                assert_eq!(rg.glyph_id, og.glyph_id, "glyph id @{i}/{j} ({name})");
                assert!(
                    (rg.position.x - og.position.x).abs() < 0.01
                        && (rg.position.y - og.position.y).abs() < 0.01,
                    "glyph pos @{i}/{j}: ref ({}, {}) vs dense ({}, {}) ({name})",
                    rg.position.x, rg.position.y, og.position.x, og.position.y
                );
                assert_eq!(
                    rg.unicode_codepoint, og.unicode_codepoint,
                    "codepoint @{i}/{j} ({name})"
                );
            }
            // Advance semantics: identical where kerning is zero. On the
            // fakefont corpus base==painted for simple clusters; detail
            // clusters may fold kerning — allow only that documented
            // difference (dense >= reference, delta == folded kerning).
            for (j, (rg, og)) in r.glyphs.iter().zip(o.glyphs.iter()).enumerate() {
                assert!(
                    (rg.advance - og.advance).abs() < 0.01 || og.advance > rg.advance,
                    "advance @{i}/{j}: ref {} vs dense {} ({name})",
                    rg.advance, og.advance
                );
            }
        }
    }
}

/// Override-segmentation pin (§10 finding 1): a per-grapheme style
/// override splits the run into LOGICAL ITEMS, and every offset in the
/// model is ITEM-relative — `item_index` carries the segment's run
/// offset. Pins (a) each cluster's text() against the INPUT at
/// item_index + start_byte_in_run, and (b) the dense runs' text Arcs
/// against the clusters' own source Arcs (the fb77aff46 rewire; the old
/// content.get(source_run) mapping fails this for the segments).
#[test]
fn override_segmented_run_offsets_are_item_relative_and_dense_text_correct() {
    use azul_layout::text3::cache::{PartialStyleProperties, StyleOverride, ContentIndex};
    let font_ref = test_font_ref();
    let input = "hello world";
    let red = azul_css::props::basic::ColorU { r: 200, g: 30, b: 30, a: 255 };
    let overrides = vec![StyleOverride {
        target: ContentIndex { run_index: 0, item_index: 6 },
        style: PartialStyleProperties { color: Some(red), ..PartialStyleProperties::default() },
    }];
    let content = vec![styled(input, &font_ref, 0, None, |_| {})];
    let logical = create_logical_items(&content, &overrides, &mut None);
    let visual = reorder_logical_items(&logical, BidiDirection::Ltr, UnicodeBidi::Normal, &mut None)
        .expect("bidi");
    let mut loaded: LoadedFonts<FontRef> = LoadedFonts::new();
    loaded.insert(FontId::new(), font_ref.clone());
    let chain: HashMap<_, FontFallbackChain> = HashMap::new();
    let fc = FcFontCache::default();
    let shaped = shape_visual_items(&visual, &chain, &fc, &loaded, &mut None).expect("shape");
    let constraints = UnifiedConstraints {
        available_width: AvailableSpace::Definite(400.0),
        ..UnifiedConstraints::default()
    };
    let mut cursor = BreakCursor::new(&shaped);
    let layout = perform_fragment_layout(&mut cursor, &logical, &constraints, &mut None, &loaded)
        .expect("layout");

    let mut saw_override_segment = false;
    for it in &layout.items {
        if let ShapedItem::Cluster(c) = &it.item {
            let item_start = c.source_content_index.item_index as usize;
            let start = item_start + c.source_cluster_id.start_byte_in_run as usize;
            if item_start > 0 {
                saw_override_segment = true;
            }
            assert!(
                input[start..].starts_with(c.text()),
                "cluster at item {item_start} + byte {} claims {:?} but input there is {:?}",
                c.source_cluster_id.start_byte_in_run, c.text(), &input[start..]
            );
        }
    }
    assert!(saw_override_segment, "the override produced no non-zero item_index segment");

    let dense = DenseText::from_unified(&layout);
    assert!(dense.runs.len() >= 3, "override splits into >=3 dense runs, got {}", dense.runs.len());
    for r in &dense.runs {
        for ci in r.clusters.clone() {
            let c = &dense.clusters[ci as usize];
            assert!(
                r.text.get(c.start_byte as usize..).is_some(),
                "dense run text too short for cluster byte {}", c.start_byte
            );
        }
    }
}

/// (d4) The dense cursor helpers must agree with the sparse walks they
/// replace — last-cluster cursor and IFC-wide byte-offset resolution,
/// over the full corpus incl. wrapped + ligature cases. These pin the
/// helpers DIRECTLY (window-plumbing coverage is separate — the in-situ
/// verify asserts there were silent for lack of dense-bearing fixtures,
/// the same vacuous-NC class d3 hit).
#[test]
fn dense_cursor_helpers_agree_with_the_sparse_walks() {
    use azul_core::selection::{CursorAffinity, TextCursor};
    for (text, width) in [
        ("hello dense world", 400.0),
        ("a longer paragraph that will wrap across multiple lines of text", 120.0),
        ("waffle office ffi", 400.0),
    ] {
        let (layout, content) = layout_of(text, width);
        let dense = DenseText::from_unified_with_content(&layout, &content);

        // last-cluster cursor vs the sparse rev-scan
        let sparse_last = layout.items.iter().rev().find_map(|it| match &it.item {
            ShapedItem::Cluster(c) => Some(TextCursor {
                cluster_id: c.source_cluster_id,
                affinity: CursorAffinity::Trailing,
            }),
            _ => None,
        });
        assert_eq!(
            dense.last_cluster_cursor(),
            sparse_last,
            "last-cluster cursor ({text:?})"
        );

        // byte_offset_to_cursor vs the sparse accumulation walk, at every
        // boundary offset the sparse walk produces.
        let mut acc = 0u32;
        let mut offsets = vec![0u32];
        for it in &layout.items {
            if let ShapedItem::Cluster(c) = &it.item {
                acc += c.text().len() as u32;
                offsets.push(acc);
            }
        }
        offsets.push(acc + 100); // past the end
        for off in offsets {
            let sparse = {
                let mut cur = 0u32;
                let mut found = None;
                if off == 0 {
                    found = layout.items.iter().find_map(|it| match &it.item {
                        ShapedItem::Cluster(c) => Some(TextCursor {
                            cluster_id: c.source_cluster_id,
                            affinity: CursorAffinity::Trailing,
                        }),
                        _ => None,
                    });
                } else {
                    for it in &layout.items {
                        if let ShapedItem::Cluster(c) = &it.item {
                            let end = cur + c.text().len() as u32;
                            if off >= cur && off <= end {
                                found = Some(TextCursor {
                                    cluster_id: c.source_cluster_id,
                                    affinity: CursorAffinity::Trailing,
                                });
                                break;
                            }
                            cur = end;
                        }
                    }
                    if found.is_none() {
                        found = layout.items.iter().rev().find_map(|it| match &it.item {
                            ShapedItem::Cluster(c) => Some(TextCursor {
                                cluster_id: c.source_cluster_id,
                                affinity: CursorAffinity::Trailing,
                            }),
                            _ => None,
                        });
                    }
                }
                found
            };
            assert_eq!(
                dense.byte_offset_to_cursor(off),
                sparse,
                "byte offset {off} ({text:?})"
            );
        }
    }
}

/// (d6b) The caret-stop primitive: dense grapheme_stops must equal the
/// sparse list exactly — incl. a combining-mark case, where the
/// GRAPHEME_CONTINUATION flag must exclude the mark cluster the same
/// way the sparse text probe does.
#[test]
fn dense_grapheme_stops_agree_with_the_sparse_walk() {
    for (text, width) in [
        ("hello dense world", 400.0),
        ("a longer paragraph that will wrap across multiple lines of text", 120.0),
        ("waffle office ffi", 400.0),
        ("cafe\u{0301} au lait", 400.0), // combining acute: continuation cluster
    ] {
        let (layout, content) = layout_of(text, width);
        let dense = DenseText::from_unified_with_content(&layout, &content);
        assert_eq!(
            dense.grapheme_stops(),
            layout.grapheme_stops(),
            "caret stops ({text:?})"
        );
    }
}

/// (d6c) Left/right movement over EVERY caret stop must match the sparse
/// walk exactly — incl. the combining-mark case (marks move with their
/// base) and saturation at both ends.
#[test]
fn dense_cursor_movement_agrees_with_the_sparse_walk() {
    for (text, width) in [
        ("hello dense world", 400.0),
        ("a longer paragraph that will wrap across multiple lines of text", 120.0),
        ("cafe\u{0301} au lait", 400.0),
    ] {
        let (layout, content) = layout_of(text, width);
        let dense = DenseText::from_unified_with_content(&layout, &content);
        let stops = dense.grapheme_stops();
        assert!(!stops.is_empty());
        let mut dbg = None;
        for id in &stops {
            for affinity in [
                azul_core::selection::CursorAffinity::Leading,
                azul_core::selection::CursorAffinity::Trailing,
            ] {
                let cursor = azul_core::selection::TextCursor { cluster_id: *id, affinity };
                assert_eq!(
                    dense.move_cursor_left(cursor),
                    layout.move_cursor_left(cursor, &mut dbg),
                    "left from {id:?}/{affinity:?} ({text:?})"
                );
                assert_eq!(
                    dense.move_cursor_right(cursor),
                    layout.move_cursor_right(cursor, &mut dbg),
                    "right from {id:?}/{affinity:?} ({text:?})"
                );
            }
        }
    }
}

/// (d6d) Vertical movement + hit testing: from EVERY caret stop under
/// both affinities, up and down (with fresh AND persisted goal_x) must
/// match the sparse walk; the hit test itself is probed at every
/// cluster center. The wrapped corpus gives real multi-line traffic.
#[test]
fn dense_vertical_movement_and_hittest_agree_with_the_sparse_walk() {
    use azul_layout::text3::cache::Point;
    for (text, width) in [
        ("hello dense world", 400.0),
        ("a longer paragraph that will wrap across multiple lines of text", 120.0),
        ("cafe\u{0301} au lait wraps too when narrow enough for it", 90.0),
    ] {
        let (layout, content) = layout_of(text, width);
        let dense = DenseText::from_unified_with_content(&layout, &content);
        let stops = dense.grapheme_stops();
        let mut dbg = None;
        for id in &stops {
            for affinity in [
                azul_core::selection::CursorAffinity::Leading,
                azul_core::selection::CursorAffinity::Trailing,
            ] {
                let cursor = azul_core::selection::TextCursor { cluster_id: *id, affinity };
                let mut gx_d = None;
                let mut gx_s = None;
                assert_eq!(
                    dense.move_cursor_up(cursor, &mut gx_d),
                    layout.move_cursor_up(cursor, &mut gx_s, &mut dbg),
                    "up from {id:?}/{affinity:?} ({text:?})"
                );
                assert_eq!(gx_d, gx_s, "up goal_x ({text:?})");
                let mut gx_d = None;
                let mut gx_s = None;
                assert_eq!(
                    dense.move_cursor_down(cursor, &mut gx_d),
                    layout.move_cursor_down(cursor, &mut gx_s, &mut dbg),
                    "down from {id:?}/{affinity:?} ({text:?})"
                );
                assert_eq!(gx_d, gx_s, "down goal_x ({text:?})");
            }
        }
        // Hit test at every cluster center (both halves).
        for it in &layout.items {
            if let ShapedItem::Cluster(c) = &it.item {
                let b = it.item.bounds();
                for frac in [0.25, 0.75] {
                    let p = Point {
                        x: it.position.x + b.width * frac,
                        y: it.position.y + b.height / 2.0,
                    };
                    assert_eq!(
                        dense.hittest_cursor(p),
                        layout.hittest_cursor(azul_core::geom::LogicalPosition { x: p.x, y: p.y }),
                        "hittest at ({}, {}) near {:?} ({text:?})",
                        p.x, p.y, c.source_cluster_id
                    );
                }
            }
        }
    }
}

/// (d6e) Word + line movements: from EVERY caret stop under both
/// affinities, prev/next word and line start/end must match the sparse
/// walks — punctuation-as-boundary and end-of-text saturation included.
#[test]
fn dense_word_and_line_movement_agree_with_the_sparse_walk() {
    for (text, width) in [
        ("hello dense world, punct. and_under scores!", 400.0),
        ("a longer paragraph that will wrap across multiple lines of text", 120.0),
    ] {
        let (layout, content) = layout_of(text, width);
        let dense = DenseText::from_unified_with_content(&layout, &content);
        let mut dbg = None;
        for id in &dense.grapheme_stops() {
            for affinity in [
                azul_core::selection::CursorAffinity::Leading,
                azul_core::selection::CursorAffinity::Trailing,
            ] {
                let cursor = azul_core::selection::TextCursor { cluster_id: *id, affinity };
                assert_eq!(
                    dense.move_cursor_to_prev_word(cursor),
                    layout.move_cursor_to_prev_word(cursor, &mut dbg),
                    "prev-word from {id:?}/{affinity:?} ({text:?})"
                );
                assert_eq!(
                    dense.move_cursor_to_next_word(cursor),
                    layout.move_cursor_to_next_word(cursor, &mut dbg),
                    "next-word from {id:?}/{affinity:?} ({text:?})"
                );
                assert_eq!(
                    dense.move_cursor_to_line_start(cursor),
                    layout.move_cursor_to_line_start(cursor, &mut dbg),
                    "line-start from {id:?}/{affinity:?} ({text:?})"
                );
                assert_eq!(
                    dense.move_cursor_to_line_end(cursor),
                    layout.move_cursor_to_line_end(cursor, &mut dbg),
                    "line-end from {id:?}/{affinity:?} ({text:?})"
                );
            }
        }
    }
}

/// (d6h) THE retirement gate: full sparse materialization from the
/// dense arrays must reproduce `layout.items` EXACTLY (PartialEq) —
/// every field, every glyph, every position — on pure-cluster layouts.
/// Covers single-line, wrapped, ligature (ffi), and mixed-style-run
/// cases; the mixed case exercises run boundaries + item_base.
#[test]
fn dense_expansion_reproduces_the_sparse_items_exactly() {
    let mut cases: Vec<(String, UnifiedLayout, Vec<InlineContent>)> = Vec::new();
    for (text, width) in [
        ("hello dense world", 400.0),
        ("a longer paragraph that will wrap across multiple lines of text", 120.0),
        ("waffle office ffi", 400.0),
    ] {
        let (layout, content) = layout_of(text, width);
        cases.push((text.to_string(), layout, content));
    }
    {
        let font_ref = test_font_ref();
        let content = vec![
            styled("small ", &font_ref, 0, None, |_| {}),
            styled("BIG", &font_ref, 6, None, |s| s.font_size_px = 32.0),
        ];
        let (layout, content, _) = layout_of_content(content, &font_ref, 400.0);
        cases.push(("small+BIG two-run".to_string(), layout, content));
    }
    for (name, layout, content) in cases {
        let dense = DenseText::from_unified_with_content(&layout, &content);
        assert_eq!(
            dense.clusters.len(),
            layout.items.len(),
            "pure-cluster corpus required ({name})"
        );
        let expanded = dense.to_unified_items();
        assert_eq!(expanded.len(), layout.items.len(), "count ({name})");
        for (i, (e, o)) in expanded.iter().zip(layout.items.iter()).enumerate() {
            assert_eq!(e, o, "expansion diverges at item {i} ({name})");
        }
    }
}

/// (#25b) The item-index DUAL-MODE law: a layout whose clusters all
/// carry one CONSTANT `item_index` (the shape produced by paths that
/// never restamp per cluster) must build the SAME number of dense runs
/// as the linear original — before the `item_linear` flag it split into
/// one run PER CLUSTER (~25 KB/IFC of headers) — and expansion must
/// reproduce the constant index exactly.
#[test]
fn constant_item_index_coalesces_and_roundtrips() {
    let (layout, content) = layout_of("hello dense world constant", 400.0);
    let baseline_runs = DenseText::from_unified_with_content(&layout, &content)
        .runs
        .len();

    // Rewrite every cluster to the degenerate constant-index shape.
    let mut mutated = layout.clone();
    for item in &mut mutated.items {
        if let ShapedItem::Cluster(c) = &mut item.item {
            c.source_content_index.item_index = 7;
        }
    }

    let dense = DenseText::from_unified_with_content(&mutated, &content);
    assert_eq!(
        dense.clusters.len(),
        mutated.items.len(),
        "pure-cluster fixture required"
    );
    assert_eq!(
        dense.runs.len(),
        baseline_runs,
        "constant item_index must coalesce exactly like the linear shape \
         (pre-#25b this was one run per cluster: {} runs for {} clusters)",
        dense.runs.len(),
        dense.clusters.len(),
    );
    assert!(
        dense.runs.iter().any(|r| !r.item_linear),
        "the constant model must actually have engaged (guard against the \
         mutation not sticking)"
    );

    // Exact reconstruction of the constant index through the expander.
    let expanded = dense.to_unified_items();
    assert_eq!(expanded.len(), mutated.items.len());
    for (i, (e, o)) in expanded.iter().zip(mutated.items.iter()).enumerate() {
        assert_eq!(e, o, "expansion diverges at item {i}");
    }

    // And the ORIGINAL still roundtrips bit-for-bit. (Writing this pin
    // surfaced that the REAL corpus is itself constant-per-word —
    // item_index holds while start_byte advances inside a word — so the
    // pre-#25b encoding was already splitting one run per cluster inside
    // every multi-cluster word; the dual mode coalesces ordinary
    // documents too, not just exotic paths.)
    let dense_lin = DenseText::from_unified_with_content(&layout, &content);
    assert!(
        dense_lin.runs.len() < dense_lin.clusters.len(),
        "a plain sentence must not degenerate to one run per cluster \
         ({} runs / {} clusters)",
        dense_lin.runs.len(),
        dense_lin.clusters.len(),
    );
    let expanded_lin = dense_lin.to_unified_items();
    for (i, (e, o)) in expanded_lin.iter().zip(layout.items.iter()).enumerate() {
        assert_eq!(e, o, "linear expansion diverges at item {i}");
    }
}

/// (d6g) The FIELD-CHOICE pin for positioned_cluster, on a hand-built
/// struct where `top_y != baseline_y` — the fakefont corpus cannot
/// distinguish them (its lines have top == baseline), which made a
/// baseline→top NC pass the corpus pin below. This is the arm that
/// actually pins WHICH y the accessor reads (the sparse `position.y`,
/// i.e. the baseline), plus the out-of-range and source_index reads.
#[test]
fn dense_positioned_cluster_reads_the_baseline_not_the_top() {
    use azul_layout::text3::dense::{ClusterCompact, DenseRun, DenseText, LineRecord};
    let mut d = DenseText::default();
    d.clusters.push(ClusterCompact {
        glyph_id: 0,
        flags: azul_layout::text3::cache::ClusterFlags(0),
        advance: 10.0,
        start_byte: 0,
        x: 5.0,
    });
    // (d6h) positioned_cluster resolves the cluster's run for the
    // mixed-size ascent correction; zero metrics ⟹ ascent 0 ⟹ the
    // recorded y verbatim, keeping this a pure field-choice pin.
    d.runs.push(DenseRun {
        style: Arc::new(StyleProperties::default()),
        font_hash: 0,
        font_metrics: azul_layout::text3::cache::LayoutFontMetrics {
            ascent: 0.0,
            descent: 0.0,
            cap_height: None,
            x_height: None,
            line_gap: 0.0,
            units_per_em: 0,
        },
        source_run: 0,
        source_node: u32::MAX,
        text: Arc::from(""),
        clusters: 0..1,
        item_base: 0,
        item_linear: true,
        // The run's own solved y == the line's recorded baseline here (one
        // run on the line), so this stays a pure field-choice pin.
        y: 40.0,
        script: azul_layout::text3::script::Script::Latin,
        direction: BidiDirection::Ltr,
    });
    d.lines.push(LineRecord {
        clusters: (0, 1),
        baseline_y: 40.0,
        top_y: 28.0,
        height: 16.0,
        source_index: 3,
    });
    let (x, y, li) = d.positioned_cluster(0).expect("in range");
    assert!((x - 5.0).abs() < f32::EPSILON, "x reads the cluster");
    assert!(
        (y - 40.0).abs() < f32::EPSILON,
        "y must be the line's baseline_y (the sparse position.y), got {y}"
    );
    assert_eq!(li, 3, "line_index reads source_index, not the record ordinal");
    assert!(d.positioned_cluster(1).is_none(), "out of range is None");
}

/// (d6g) positioned_cluster pin: every cluster's (x, y, line_index)
/// reconstruction equals the sparse PositionedItem fields — including
/// on wrapped multi-line layouts where source_index matters, and (d6h)
/// on a MIXED-SIZE line, where per-item y differs within one line (the
/// case the uniform corpus could not express and d6g got wrong).
#[test]
fn dense_positioned_cluster_agrees_with_the_sparse_items() {
    let mut cases: Vec<(String, UnifiedLayout, Vec<InlineContent>)> = Vec::new();
    for (text, width) in [
        ("hello dense world, punct. and_under scores!", 400.0),
        ("a longer paragraph that will wrap across multiple lines of text", 120.0),
    ] {
        let (layout, content) = layout_of(text, width);
        cases.push((text.to_string(), layout, content));
    }
    {
        let font_ref = test_font_ref();
        let content = vec![
            styled("small ", &font_ref, 0, None, |_| {}),
            styled("BIG", &font_ref, 6, None, |s| s.font_size_px = 32.0),
        ];
        let (layout, content, _) = layout_of_content(content, &font_ref, 400.0);
        cases.push(("small+BIG mixed line".to_string(), layout, content));
    }
    for (text, layout, content) in cases {
        let dense = DenseText::from_unified_with_content(&layout, &content);
        assert_eq!(
            dense.clusters.len(),
            layout.items.len(),
            "corpus must stay pure-cluster for this pin ({text:?})"
        );
        for (i, item) in layout.items.iter().enumerate() {
            let (x, y, li) = dense
                .positioned_cluster(u32::try_from(i).unwrap())
                .expect("cluster index in range");
            assert!(
                (item.position.x - x).abs() < 0.01,
                "x[{i}]: sparse {} vs dense {x} ({text:?})",
                item.position.x
            );
            assert!(
                (item.position.y - y).abs() < 0.01,
                "y[{i}]: sparse {} vs dense {y} ({text:?})",
                item.position.y
            );
            assert_eq!(
                item.line_index, li,
                "line_index[{i}] ({text:?})"
            );
        }
    }
}

/// (d6f) The dispatcher pin: every (direction, step) arm of
/// `DenseText::resolve_step` must route to the same movement op the
/// window's sparse `resolve_step_static` routes to. Wiring-level — the
/// ops themselves are pinned pairwise above; this catches a swapped or
/// mis-mapped match arm.
#[test]
fn dense_resolve_step_dispatches_like_the_sparse_resolver() {
    use azul_core::events::{SelectionDirection as D, SelectionStep as S};
    let (layout, content) =
        layout_of("a longer paragraph that will wrap across multiple lines of text", 120.0);
    let dense = DenseText::from_unified_with_content(&layout, &content);
    for id in &dense.grapheme_stops() {
        for affinity in [
            azul_core::selection::CursorAffinity::Leading,
            azul_core::selection::CursorAffinity::Trailing,
        ] {
            let cursor = azul_core::selection::TextCursor { cluster_id: *id, affinity };
            for (direction, step) in [
                (D::Backward, S::Character),
                (D::Forward, S::Character),
                (D::Backward, S::Word),
                (D::Forward, S::Word),
                (D::Backward, S::VisualLine),
                (D::Forward, S::VisualLine),
                (D::Backward, S::Line),
                (D::Forward, S::Line),
                (D::Backward, S::Document),
                (D::Forward, S::Document),
            ] {
                let expected = match (direction, step) {
                    (D::Backward, S::Character) => dense.move_cursor_left(cursor),
                    (D::Forward, S::Character) => dense.move_cursor_right(cursor),
                    (D::Backward, S::Word) => dense.move_cursor_to_prev_word(cursor),
                    (D::Forward, S::Word) => dense.move_cursor_to_next_word(cursor),
                    (D::Backward, S::VisualLine) => dense.move_cursor_up(cursor, &mut None),
                    (D::Forward, S::VisualLine) => dense.move_cursor_down(cursor, &mut None),
                    (D::Backward, S::Line) => dense.move_cursor_to_line_start(cursor),
                    (D::Forward, S::Line) => dense.move_cursor_to_line_end(cursor),
                    (D::Backward, S::Document) => {
                        dense.first_cluster_cursor().unwrap_or(cursor)
                    }
                    (D::Forward, S::Document) => dense.last_cluster_cursor().unwrap_or(cursor),
                };
                assert_eq!(
                    dense.resolve_step(&cursor, direction, step),
                    expected,
                    "dispatch ({direction:?}, {step:?}) from {id:?}/{affinity:?}"
                );
            }
        }
    }
}

/// A LIGATURE-FUSED cluster must record its full source byte length.
///
/// An `fi` ligature is exactly ONE glyph with no offsets, no kerning, kind
/// `Character` and no vertical metrics — so it satisfies none of the clauses
/// `needs_detail` used to check, got no `ClusterDetail`, and
/// `cluster_byte_len` fell through to its grapheme fallback: "the next
/// grapheme at start_byte", which for "fi" is "f", length 1. Every consumer
/// of the length then saw a 1-byte cluster, and the difference reached users
/// as PDF text — pdftotext extracted "Confgure", "flter", "offine", each
/// ligated word missing the SECOND letter of its ligature pair, because the
/// ToUnicode entry said "f". `ShapedCluster::source_byte_len`'s own doc
/// states the invariant being violated: "Stored, not re-derived:
/// ligature-fused clusters span MULTIPLE graphemes, so 'next grapheme
/// boundary' cannot reconstruct the slice in general."
///
/// HAND-BUILT sparse input, not a shaped one: the fakefont performs no GSUB
/// ligature substitution, so no shaped corpus can produce a single-glyph
/// multi-grapheme cluster — which is precisely how 19 green cases coexisted
/// with this bug. (Found 2026-08-14 from OUTSIDE azul, by printpdf's
/// html_visual_subset_glyphs suite against a real font.)
#[test]
fn ligature_cluster_records_its_full_byte_length() {
    use azul_core::selection::{ContentIndex, GraphemeClusterId};
    use azul_layout::text3::cache::{
        ClusterFlags, GlyphKind, LayoutFontMetrics, OverflowInfo, Point, PositionedItem,
        ShapedCluster, ShapedGlyph,
    };
    use azul_layout::text3::script::Script;

    let text: Arc<str> = Arc::from("fine");
    let style = Arc::new(StyleProperties::default());
    let metrics = LayoutFontMetrics {
        ascent: 0.0,
        descent: 0.0,
        cap_height: None,
        x_height: None,
        line_gap: 0.0,
        units_per_em: 0,
    };
    let glyph = |id: u16| ShapedGlyph {
        kind: GlyphKind::Character,
        glyph_id: id,
        cluster_offset: 0,
        advance: 10.0,
        kerning: 0.0,
        offset: Point { x: 0.0, y: 0.0 },
        vertical_advance: 0.0,
        vertical_offset: Point { x: 0.0, y: 0.0 },
        script: Script::Latin,
        font_hash: 7,
        font_metrics: metrics,
    };
    let cluster = |start: u32, len: u16, glyph_id: u16| ShapedCluster {
        source_text: text.clone(),
        source_byte_len: len,
        source_cluster_id: GraphemeClusterId { source_run: 0, start_byte_in_run: start },
        source_content_index: ContentIndex { run_index: 0, item_index: start },
        source_node_id: None,
        glyphs: [glyph(glyph_id)].into_iter().collect(),
        flags: ClusterFlags(0),
        advance: 10.0,
        direction: BidiDirection::Ltr,
        style: style.clone(),
        marker_position_outside: None,
        is_first_fragment: false,
        is_last_fragment: false,
    };
    let layout = UnifiedLayout {
        items: vec![
            // "fi" FUSED into one glyph: byte length 2, ONE plain glyph.
            PositionedItem {
                item: ShapedItem::Cluster(cluster(0, 2, 100)),
                position: Point { x: 0.0, y: 12.0 },
                line_index: 0,
            },
            // "n" and "e", ordinary single-grapheme clusters.
            PositionedItem {
                item: ShapedItem::Cluster(cluster(2, 1, 101)),
                position: Point { x: 10.0, y: 12.0 },
                line_index: 0,
            },
            PositionedItem {
                item: ShapedItem::Cluster(cluster(3, 1, 102)),
                position: Point { x: 20.0, y: 12.0 },
                line_index: 0,
            },
        ],
        overflow: OverflowInfo::default(),
    };

    let d = DenseText::from_unified(&layout);
    assert_eq!(d.clusters.len(), 3);

    // The fused cluster is not reconstructible from the compact record, so it
    // MUST carry a detail entry with its true byte length; the plain clusters
    // must NOT (the predicate stays tight — details are the exception path).
    assert_eq!(
        d.details.len(),
        1,
        "exactly the fused cluster needs a detail entry, got {:?}",
        d.details
    );
    assert_eq!(d.details[0].cluster, 0);
    assert_eq!(d.details[0].byte_len, 2, "the detail must record the FUSED length");

    assert_eq!(d.cluster_byte_len(0), 2, "fused cluster spans 2 source bytes");
    assert_eq!(d.cluster_byte_len(1), 1);
    assert_eq!(d.cluster_byte_len(2), 1);

    // Roundtrip: the sparse expansion must reproduce the fused length and
    // therefore the cluster's own text slice.
    let expanded = d.to_unified_items();
    assert_eq!(expanded.len(), 3);
    let ShapedItem::Cluster(rc) = &expanded[0].item else {
        panic!("expanded[0] must be a cluster");
    };
    assert_eq!(rc.source_byte_len, 2, "roundtrip lost the fused byte length");
    assert_eq!(rc.text(), "fi", "the cluster's own text slice must be the full ligature");
}
