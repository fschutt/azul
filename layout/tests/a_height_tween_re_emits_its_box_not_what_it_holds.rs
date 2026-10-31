//! A frame of a `height` tween re-emits the display list items of the box
//! that grows (and of the boxes its growth resizes), not of everything inside
//! it: `height` is not inherited, so the children paint as they did and only
//! move or keep their place - the patch copies their items.
//!
//! AzDrive's ribbon opens by a height tween on its content panel: every frame
//! re-emitted the whole ribbon, ~120 labels and buttons, each one reading its
//! backgrounds and borders through the cascade again.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::{layout::LayoutHeight, property::CssProperty};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, overlay::ContentChange, probe::Probe,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

use crate::a_one_box_slide_does_not_re_lay_out_the_page::restore_ambient_recording;

const ITEMS: usize = 20;

/// A panel of twenty coloured labels whose height tweens, above a footer.
fn window() -> (LayoutWindow, NodeId) {
    let mut panel = Dom::create_div().with_class("panel".into()).with_css(
        "height: 40px; overflow: hidden; display: flex; flex-direction: row; flex-wrap: wrap; \
         background: #dde4ee; animation: height 200ms linear;",
    );
    for i in 0..ITEMS {
        let color = if i % 2 == 0 { "#c8d4e8" } else { "#e8eef8" };
        panel = panel.with_child(
            Dom::create_div_with_text(format!("item {i}")).with_css(&format!(
                "width: 60px; height: 16px; margin: 2px; background: {color}; \
                 border: 1px solid #8899aa;"
            )),
        );
    }
    let mut dom = Dom::create_body()
        .with_css("margin: 0; background: #ffffff;")
        .with_child(panel)
        .with_child(Dom::create_div_with_text("footer").with_css("height: 30px; background: #eeeeee;"));
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .expect("the page lays out");
    let panel = {
        let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
        let node_data = sd.node_data.as_container();
        (0..node_data.len())
            .map(NodeId::new)
            .find(|n| {
                node_data[*n]
                    .get_ids_and_classes()
                    .iter()
                    .any(|c| matches!(c.as_class(), Some(s) if s == "panel"))
            })
            .expect("the page has a panel")
    };
    (lw, panel)
}

fn relayout(lw: &mut LayoutWindow) {
    let result = lw.layout_results.remove(&DomId::ROOT_ID).expect("laid out");
    let window_state = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        result.styled_dom,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .expect("the relayout runs");
}

#[test]
fn a_height_tween_frame_copies_the_items_of_what_its_box_holds() {
    let _serialised = crate::probe_lock();
    if !Probe::enabled() {
        eprintln!("the probe is compiled out: nothing to count");
        return;
    }
    let (mut lw, panel) = window();
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: panel,
        props: vec![CssProperty::const_height(LayoutHeight::const_px(120))],
        override_only: false,
    });
    assert!(
        lw.css_transitions.iter().any(|t| t.node == panel),
        "harness: the panel's height write seeds its tween"
    );
    for frame in 0..3 {
        let _ = lw.tick_animations(0.016);
        assert!(
            lw.take_transition_relayout(),
            "frame {frame}: harness - a height step owes a relayout"
        );
        Probe::set_recording(true);
        let _ = Probe::drain();
        relayout(&mut lw);
        let events = Probe::drain();
        restore_ambient_recording();
        let count = |name: &str| events.iter().filter(|e| e.name == name).count();
        assert!(
            lw.frame_report.last_dl_build_patched,
            "frame {frame}: harness - the tween frame patches its display list"
        );
        // Every box is visited for its backgrounds and for its content; a
        // visit that copies its items from the last list says so.
        let visits = count("dl_bg_border") + count("dl_content");
        let copies = count("dl_patch_copy") + count("dl_patch_empty");
        let re_emitted = visits.saturating_sub(copies);
        assert!(
            re_emitted <= 8,
            "frame {frame}: {re_emitted} of {visits} box visits painted their items again - the \
             panel's {ITEMS} labels only kept their place and must be copied"
        );
        let mismatches = lw.verify_patched_display_list(DomId::ROOT_ID);
        assert!(
            mismatches.is_empty(),
            "frame {frame}: the patched list must equal a wholesale build: {mismatches:?}"
        );
    }
}
