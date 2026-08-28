//! miniword ENGINE-ISSUE 5: four deterministic `position: absolute` sizing
//! anomalies inside a `position: relative; overflow: hidden` flex-grow
//! container (the Word page-canvas shape). Each case asserts the CSS 2.2
//! §10.3.7/§10.6.4 result.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn layout_dom(dom: Dom, css_str: &str) -> LayoutWindow {
    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(1280.0, 800.0);
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();
    layout_window
}

fn node(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

/// body(0) > canvas(1, relative flex-grow overflow:hidden 1280x676)
/// > abs(2) > sheet(3, 794x100)
fn shell(abs_css: &str) -> (Dom, String) {
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(vec![IdOrClass::Class("canvas".into())].into())
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(vec![IdOrClass::Class("abs".into())].into())
                    .with_child(
                        Dom::create_div()
                            .with_ids_and_classes(vec![IdOrClass::Class("sheet".into())].into()),
                    ),
            ),
    );
    let css = format!(
        r#"
        * {{ margin: 0; padding: 0; }}
        body {{ display: flex; flex-direction: column; width: 1280px; height: 100%; }}
        .canvas {{
            flex-grow: 1; min-height: 0px;
            position: relative; overflow: hidden;
            margin-top: 124px;
        }}
        .abs {{ position: absolute; {abs_css} }}
        .sheet {{ width: 794px; height: 100px; }}
    "#
    );
    (dom, css)
}

const CANVAS_W: f32 = 1280.0;
const CANVAS_H: f32 = 676.0; // 800 - 124

#[test]
fn a_four_zero_anchors_span_and_center_their_flex_child() {
    let (dom, css) = shell(
        "top: 0; left: 0; right: 0; bottom: 0; \
         display: flex; align-items: center; justify-content: center;",
    );
    let lw = layout_dom(dom, &css);
    let abs = lw.get_node_layout_rect(node(2)).expect("abs rect");
    let sheet = lw.get_node_layout_rect(node(3)).expect("sheet rect");
    assert!(
        (abs.size.width - CANVAS_W).abs() < 1.0 && (abs.size.height - CANVAS_H).abs() < 1.0,
        "four-zero anchors must span the padding box {CANVAS_W}x{CANVAS_H}, got {:?}",
        abs.size
    );
    // justify-content: center on the abs flex box must center the sheet.
    let want_x = (CANVAS_W - 794.0) / 2.0;
    assert!(
        (sheet.origin.x - want_x).abs() < 1.0,
        "sheet must be CENTERED at x={want_x}, got x={} (report: hugged x=0)",
        sheet.origin.x
    );
    let want_y = (CANVAS_H - 100.0) / 2.0;
    assert!(
        ((sheet.origin.y - abs.origin.y) - want_y).abs() < 1.0,
        "sheet must be vertically centered at dy={want_y}, got dy={}",
        sheet.origin.y - abs.origin.y
    );
}

#[test]
fn b_percent_size_resolves_against_the_positioned_ancestor() {
    let (dom, css) = shell("top: 0; left: 0; width: 100%; height: 100%;");
    let lw = layout_dom(dom, &css);
    let abs = lw.get_node_layout_rect(node(2)).expect("abs rect");
    let sheet = lw.get_node_layout_rect(node(3)).expect("sheet rect");
    assert!(
        (abs.size.width - CANVAS_W).abs() < 1.0 && (abs.size.height - CANVAS_H).abs() < 1.0,
        "width/height:100% must resolve against the relative ancestor \
         {CANVAS_W}x{CANVAS_H}, got {:?} (report: ZERO size)",
        abs.size
    );
    assert!(
        (sheet.size.width - 794.0).abs() < 1.0 && (sheet.size.height - 100.0).abs() < 1.0,
        "child must lay out inside, got {:?} (report: disappears)",
        sheet.size
    );
}

#[test]
fn c_auto_size_shrinks_to_fit_the_child() {
    let (dom, css) = shell("top: 0; left: 0;");
    let lw = layout_dom(dom, &css);
    let abs = lw.get_node_layout_rect(node(2)).expect("abs rect");
    let sheet = lw.get_node_layout_rect(node(3)).expect("sheet rect");
    assert!(
        (abs.size.width - 794.0).abs() < 1.0 && (abs.size.height - 100.0).abs() < 1.0,
        "auto-size abs box must shrink-to-fit its 794x100 child, got {:?} \
         (report: children disappear)",
        abs.size
    );
    assert!(
        (sheet.size.width - 794.0).abs() < 1.0,
        "child must be visible at content size, got {:?}",
        sheet.size
    );
}

#[test]
fn d_symmetric_left_right_insets_solve_the_width_equation() {
    // left:243 + width:auto + right:243 => width = 1280-243-243 = 794
    let (dom, css) = shell("top: 0; bottom: 0; left: 243px; right: 243px;");
    let lw = layout_dom(dom, &css);
    let abs = lw.get_node_layout_rect(node(2)).expect("abs rect");
    let sheet = lw.get_node_layout_rect(node(3)).expect("sheet rect");
    assert!(
        (abs.size.width - 794.0).abs() < 1.0,
        "css 2.2 10.3.7: width = 1280 - 243 - 243 = 794, got {} \
         (report: ~552 + black band)",
        abs.size.width
    );
    assert!(
        (abs.origin.x - 243.0).abs() < 1.0,
        "abs box must start at left inset x=243, got {}",
        abs.origin.x
    );
    assert!(
        (sheet.size.width - 794.0).abs() < 1.0,
        "block child fills the solved width, got {}",
        sheet.size.width
    );
}
