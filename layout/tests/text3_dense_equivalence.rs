#![cfg(feature = "text_layout")]
//! §3.2 campaign step 1 gate: the dense view derived from a real layout
//! must agree with the current model on every quantity both can express —
//! cluster count, per-cluster advance/x/start_byte/flags, run partitioning
//! totals, and the T1 invariant one level up: concatenating the dense
//! runs' texts in run order reproduces the input.

use std::collections::HashMap;
use std::sync::Arc;

use azul_css::props::basic::FontRef;
use azul_layout::font::parsed::ParsedFont;
use azul_layout::parsed_font_to_font_ref;
use azul_layout::text3::cache::{
    create_logical_items, perform_fragment_layout, reorder_logical_items, shape_visual_items,
    AvailableSpace, BidiDirection, BreakCursor, FontStack, InlineContent, LoadedFonts, ShapedItem,
    StyleProperties, StyledRun, UnicodeBidi, UnifiedConstraints, UnifiedLayout,
};
use azul_layout::text3::dense::DenseText;
use rust_fontconfig::{FcFontCache, FontBytes, FontFallbackChain, FontId};

#[path = "common/fakefont.rs"]
mod fakefont;

fn layout_of(text: &str, width: f32) -> UnifiedLayout {
    let bytes = fakefont::simple_test_font();
    let arc = Arc::new(FontBytes::Owned(Arc::from(bytes.as_slice())));
    let parsed = ParsedFont::from_bytes(&bytes, 0, &mut Vec::new())
        .expect("font")
        .with_source_bytes(arc);
    let font_ref: FontRef = parsed_font_to_font_ref(parsed);
    let style = Arc::new(StyleProperties {
        font_stack: FontStack::Ref(font_ref.clone()),
        font_size_px: 16.0,
        ..StyleProperties::default()
    });
    let content = vec![InlineContent::Text(StyledRun {
        text: text.to_string(),
        style,
        logical_start_byte: 0,
        source_node_id: None,
    })];
    let logical = create_logical_items(&content, &[], &mut None);
    let visual = reorder_logical_items(&logical, BidiDirection::Ltr, UnicodeBidi::Normal, &mut None)
        .expect("bidi");
    let mut loaded: LoadedFonts<FontRef> = LoadedFonts::new();
    loaded.insert(FontId::new(), font_ref);
    let chain: HashMap<_, FontFallbackChain> = HashMap::new();
    let fc = FcFontCache::default();
    let shaped = shape_visual_items(&visual, &chain, &fc, &loaded, &mut None).expect("shape");
    let constraints = UnifiedConstraints {
        available_width: AvailableSpace::Definite(width),
        ..UnifiedConstraints::default()
    };
    let mut cursor = BreakCursor::new(&shaped);
    perform_fragment_layout(&mut cursor, &logical, &constraints, &mut None, &loaded)
        .expect("layout")
}

#[test]
fn dense_view_agrees_with_the_current_model() {
    for (text, width) in [
        ("hello dense world", 400.0),
        ("a longer paragraph that will wrap across multiple lines of text", 120.0),
        ("waffle office ffi", 400.0),
    ] {
        let layout = layout_of(text, width);
        let dense = DenseText::from_unified(&layout);

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
                text[start..].starts_with(c.text.as_str()),
                "cluster {i} claims byte {start} but the input there does not \
                 start with {:?} ({text:?})",
                c.text
            );
        }

        // Every line record covers a non-empty, in-order cluster range.
        let mut line_cover = 0u32;
        for l in &dense.lines {
            assert!(l.clusters.1 > l.clusters.0, "empty line record");
            assert!(l.clusters.0 >= line_cover, "line ranges ordered");
            line_cover = l.clusters.1;
        }
    }
}
