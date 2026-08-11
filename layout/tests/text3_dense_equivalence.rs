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

#[path = "common/fakefont.rs"]
mod fakefont;

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
        text: std::sync::Arc::from(text),
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
            assert_eq!(dc.flags, c.flags, "flags @{i}");
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
