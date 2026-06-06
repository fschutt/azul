//! Web/lifted layout-correctness REFERENCE.
//!
//! Builds the `flexbox-simple` reftest (`doc/working/flexbox-simple.xht`) as a
//! `Dom` with INLINE CSS — the *exact same* construction the web example
//! (`examples/c/web-flexbox-simple.c`) uses via `AzDom_withCss` — and runs it
//! through the SAME native solver entry the web backend's
//! `AzStartup_solveLayoutReal` calls:
//!   `StyledDom::create(dom, Css::empty())`
//!     → `LayoutWindow::layout_dom_recursive`
//!     → `get_node_position` / `get_node_size`.
//!
//! It prints the computed rects and writes them to
//! `scripts/m9_e2e/flexbox-ref.json`, which the JS gate
//! (`scripts/m9_e2e/layout-flexbox.js`) loads as the reference so it can assert
//! the LIFTED (ARM→wasm) rects == these native rects — proving the lift computes
//! identical geometry, with NO fonts/text involved.
//!
//! `flexbox-simple` is a pure-box layout with 0px difference to Chrome, so these
//! native rects are also the correct browser geometry.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::css::Css;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

// ----------------------------------------------------------------------------
// The shared inline-CSS spec. KEEP IN SYNC with examples/c/web-flexbox-simple.c
// (the `AzDom_withCss` strings). Both sides go through `Css::parse_inline`, so
// identical strings + identical tree => identical StyledDom after the
// `Css::empty()` cascade => identical layout.
// ----------------------------------------------------------------------------
const BODY_CSS: &str =
    "box-sizing:border-box; margin:0; padding:20px; width:800px; height:600px;";
const CONTAINER_CSS: &str = "box-sizing:border-box; margin:0; padding:0; display:flex; \
     width:100%; height:100px; border:5px solid #000000;";
const ITEM1_CSS: &str =
    "box-sizing:border-box; margin:0; padding:0; flex-grow:1; border:3px solid #880000;";
const ITEM2_CSS: &str =
    "box-sizing:border-box; margin:0; padding:0; flex-grow:2; border:3px solid #000088;";
const ITEM3_CSS: &str =
    "box-sizing:border-box; margin:0; padding:0; flex-grow:3; border:3px solid #008800;";

const VIEWPORT_W: f32 = 800.0;
const VIEWPORT_H: f32 = 600.0;

/// The shared DOM builder (inline CSS). Mirrors the C web example's callback.
fn build_flexbox_simple_dom() -> Dom {
    Dom::create_body().with_css(BODY_CSS).with_child(
        Dom::create_div()
            .with_css(CONTAINER_CSS)
            .with_child(Dom::create_div().with_css(ITEM1_CSS))
            .with_child(Dom::create_div().with_css(ITEM2_CSS))
            .with_child(Dom::create_div().with_css(ITEM3_CSS)),
    )
}

#[test]
fn web_flexbox_simple_reference() {
    // ---- mirror AzStartup_solveLayoutReal exactly ----
    let mut dom = build_flexbox_simple_dom();
    let styled = StyledDom::create(&mut dom, Css::empty());
    let node_count = styled.node_data.as_ref().len();
    assert_eq!(
        node_count, 5,
        "expected body + container + 3 items = 5 nodes, got {node_count}"
    );

    // Use the EXACT same font cache the web eventloop uses (embedded
    // SourceSerifPro fallback via with_memory_fonts) so this reference matches
    // the lifted path's font environment — not real system fonts. If the bare
    // fallback were insufficient, the lifted solveLayoutReal Err would reproduce
    // here too.
    const AZ_WEB_FALLBACK_FONT: &[u8] =
        include_bytes!("../../doc/fonts/SourceSerifPro-Regular.ttf");
    let fc_cache = FcFontCache::default();
    fc_cache.with_memory_fonts(vec![(
        rust_fontconfig::FcPattern {
            name: Some("serif sans-serif monospace".to_string()),
            family: Some("serif sans-serif monospace".to_string()),
            ..Default::default()
        },
        rust_fontconfig::FcFont {
            bytes: AZ_WEB_FALLBACK_FONT.to_vec(),
            font_index: 0,
            id: "az_web_fallback".to_string(),
        },
    )]);
    let mut lw = LayoutWindow::new(fc_cache).expect("LayoutWindow::new");
    lw.skip_gpu_sync = true;
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(VIEWPORT_W, VIEWPORT_H);
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = None;
    solver3::set_skip_display_list(true);
    lw.layout_dom_recursive(styled, &ws, &rr, &sc, &mut dbg)
        .expect("layout_dom_recursive should succeed");
    // Diagnostic: compare against the lifted env (which gets 0/0 → Text error).
    println!(
        "NATIVE font_chain_cache.len()={} parsed_fonts.len()={}",
        lw.font_manager.font_chain_cache.len(),
        lw.font_manager.parsed_fonts.lock().map(|m| m.len()).unwrap_or(999)
    );

    // ---- extract rects exactly like solveLayoutReal ----
    let mut rects: Vec<(i64, i64, i64, i64)> = Vec::new();
    for i in 0..node_count {
        let id = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i))),
        };
        let (x, y, w, h) = match (lw.get_node_position(id), lw.get_node_size(id)) {
            (Some(p), Some(s)) => (
                p.x.max(0.0).round() as i64,
                p.y.max(0.0).round() as i64,
                s.width.max(0.0).round() as i64,
                s.height.max(0.0).round() as i64,
            ),
            _ => (0, 0, 0, 0),
        };
        println!("REF rect[{i}] = {{x:{x}, y:{y}, w:{w}, h:{h}}}");
        rects.push((x, y, w, h));
    }

    // ---- write the JSON fixture the JS gate reads (BEFORE asserts, so it is
    // produced even when an invariant fails and reveals the real numbers) ----
    let body = rects
        .iter()
        .map(|(x, y, w, h)| format!("[{x},{y},{w},{h}]"))
        .collect::<Vec<_>>()
        .join(",");
    let json = format!(
        "{{\"test\":\"flexbox-simple\",\"viewport\":[{},{}],\"rects\":[{}]}}",
        VIEWPORT_W as i64, VIEWPORT_H as i64, body
    );
    let out = concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts/m9_e2e/flexbox-ref.json");
    match std::fs::write(out, &json) {
        Ok(()) => println!("wrote reference rects -> {out}"),
        Err(e) => println!("WARN: could not write {out}: {e}"),
    }
    println!("REF_JSON {json}");

    // ---- exact reference geometry (also the JSON the gate asserts against). ----
    assert_eq!(rects[0], (0, 0, 800, 600), "body border-box fills 800x600 at origin");
    assert_eq!(rects[1], (20, 20, 760, 100), "container: width:100% of 760 content, height:100, at padding origin");
    assert_eq!(rects[2], (25, 25, 128, 100), "item1 flex-grow:1");
    assert_eq!(rects[3], (153, 25, 250, 100), "item2 flex-grow:2");
    assert_eq!(rects[4], (403, 25, 372, 100), "item3 flex-grow:3");
    assert_eq!((128 - 6, 250 - 6, 372 - 6), (122, 244, 366), "extra width splits 1:2:3");
    assert_eq!(25 + 128 + 250 + 372, 775, "items fill container content width (750)");
}
