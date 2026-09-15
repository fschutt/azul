//! Regression test: a child of a replaced element (`<img>`) is painted ONCE,
//! at the position its own margin puts it.
//!
//! Found in the frontpage `opengl` screenshot 2026-09-08: the example composits
//! a `Button` over a callback image (`body > img > button`, the button carrying
//! `margin-top: 50px; margin-left: 50px`) and the button's box appeared TWICE —
//! once at the image's content origin and once 50 px down-right, with the label
//! text painted inside only one of them. The display list showed both copies
//! carrying the same hit-test tag, so it is one node emitted by two different
//! paint paths that disagree about the child's origin.
//!
//! The assertions are deliberately about the DISPLAY LIST, not about layout:
//! layout resolves one rect for the node, and it is the painting of a replaced
//! element's children that duplicated it.

use azul_core::{
    dom::Dom,
    geom::LogicalSize,
    resources::{ImageRef, RawImageFormat, RendererResources},
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A callback-style image: no intrinsic size, exactly like the `opengl`
/// example's `RenderImageCallback` surface.
fn sizeless_image() -> ImageRef {
    ImageRef::null_image(0, 0, RawImageFormat::RGBA8, Vec::new())
}

fn layout_dom(dom: Dom, css_str: &str, width: f32, height: f32) -> LayoutWindow {
    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);

    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(width, height);
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

/// Every opaque red rect in the display list, as (x, y, w, h) — the marker
/// colour the child paints itself with.
fn red_rects(lw: &LayoutWindow) -> Vec<(f32, f32, f32, f32)> {
    let mut out = Vec::new();
    for result in lw.layout_results.values() {
        for item in &result.display_list.items {
            if let DisplayListItem::Rect { bounds, color, .. } = item {
                if color.r > 200 && color.g < 60 && color.b < 60 && color.a > 0 {
                    let o = bounds.origin();
                    let s = bounds.size();
                    out.push((o.x, o.y, s.width, s.height));
                }
            }
        }
    }
    out
}

#[test]
fn a_child_of_an_image_is_painted_once_at_its_margin_offset() {
    // `body > img > div`: the image has a 5px border, so its content origin is
    // (5, 5); the div's 50px margins put the div at (55, 55) — and nowhere else.
    // A BLOCK child deliberately: an atomic inline over an image is the sibling
    // test's subject, and it takes an entirely different paint path (its parent
    // paints it from an `InlineShape`). What this one pins is that the interior
    // layout run of a replaced element writes its children's `relative_position`,
    // without which the child collapses onto the image's content origin.
    let dom = Dom::create_body().with_child(
        Dom::create_image(sizeless_image()).with_child(
            Dom::create_div()
                .with_ids_and_classes(
                    vec![azul_core::dom::IdOrClass::Class("marked".into())].into(),
                )
                .with_child(Dom::create_p_with_text("label")),
        ),
    );

    let lw = layout_dom(
        dom,
        "body { margin: 0; padding: 0; }
         img { width: 400px; height: 300px; border: 5px solid #00ff00;
               border-radius: 50px; box-sizing: border-box; }
         .marked { width: 100px; height: 20px;
                   margin-top: 50px; margin-left: 50px; background: #ff0000; }
         .marked p { margin: 0; }",
        800.0,
        600.0,
    );

    let boxes = red_rects(&lw);

    assert_eq!(
        boxes.len(),
        1,
        "the image's child was painted {} time(s), at {:?} — a replaced element's children must \
         be emitted by exactly one paint path",
        boxes.len(),
        boxes
    );

    let (x, y, _, _) = boxes[0];
    assert!(
        (x - 55.0).abs() < 0.5 && (y - 55.0).abs() < 0.5,
        "the image's child painted at ({x}, {y}); the image's 5px border plus the child's 50px \
         margins put it at (55, 55). Painting it at the image's content origin means its \
         `relative_position` was never written."
    );
}

#[test]
#[ignore = "diagnostic dump, run with --ignored when investigating"]
fn dbg_image_child_tree() {
    let dom = Dom::create_body().with_child(
        Dom::create_image(sizeless_image()).with_child(
            Dom::create_div()
                .with_ids_and_classes(
                    vec![azul_core::dom::IdOrClass::Class("marked".into())].into(),
                )
                .with_child(Dom::create_p_with_text("label")),
        ),
    );
    let lw = layout_dom(
        dom,
        "body { margin: 0; padding: 0; }
         img { width: 400px; height: 300px; }
         .marked { width: 100px; height: 20px; margin-top: 50px; margin-left: 50px;
                   background: #ff0000; }",
        800.0,
        600.0,
    );
    for result in lw.layout_results.values() {
        let tree = &result.layout_tree;
        for (i, n) in tree.nodes.iter().enumerate() {
            println!(
                "[{i}] dom={:?} parent={:?} children={:?} fc={:?} size={:?} rel={:?} abs={:?}",
                n.dom_node_id.map(|d| d.index()),
                n.parent,
                tree.children(i).to_vec(),
                n.formatting_context,
                n.used_size.map(|s| (s.width, s.height)),
                tree.warm(azul_layout::solver3::LayoutNodeId::new(i))
                    .and_then(|w| w.relative_position)
                    .map(|p| (p.x, p.y)),
                result.calculated_positions.get(i).map(|p| (p.x, p.y)),
            );
        }
    }
}

/// The frontpage `opengl` example's exact shape: a real `Button` composited
/// over a callback image inside a flex column. Its box was painted TWICE —
/// once at the image's content origin and once at the margin-correct spot,
/// with the label text inside only one of them.
#[test]
fn a_button_over_an_image_is_painted_once_and_maps_to_its_own_node() {
    use azul_layout::widgets::button::Button;

    let mut button = Button::create("Button composited over OpenGL content!".into());
    button.set_button_type(azul_layout::widgets::button::ButtonType::Default);

    let dom = Dom::create_body().with_child(
        Dom::create_image(sizeless_image()).with_child(
            button
                .dom()
                .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("mk".into())].into()),
        ),
    );

    let lw = layout_dom(
        dom,
        "body { display: flex; flex-direction: column; padding: 10px;
                width: 100%; height: 100%; box-sizing: border-box; }
         img { flex-grow: 1; width: 100%; border: 5px solid #00ff00;
               border-radius: 50px; box-sizing: border-box; }
         .mk { margin-top: 50px; margin-left: 50px; }",
        1466.0,
        833.0,
    );

    // The DL ↔ DOM identity invariant: a stale or shifted mapping does not
    // fail loudly, it describes the WRONG node (see `validate_node_mapping`).
    for result in lw.layout_results.values() {
        if let Err(e) = result
            .display_list
            .validate_node_mapping(&result.styled_dom)
        {
            panic!("display list ↔ DOM mapping broken: {e}");
        }
    }

    // The button's own background: `Default` is #f8f9fa.
    let mut boxes = Vec::new();
    for result in lw.layout_results.values() {
        let dl = &result.display_list;
        for (i, item) in dl.items.iter().enumerate() {
            if let DisplayListItem::Rect { bounds, color, .. } = item {
                if color.r == 0xf8 && color.g == 0xf9 && color.b == 0xfa && color.a > 0 {
                    let o = bounds.origin();
                    println!(
                        "  item {i}: bg at ({}, {})  dom_node={:?}  emit={:?}",
                        o.x,
                        o.y,
                        dl.node_mapping.get(i).and_then(|n| *n).map(|n| n.index()),
                        dl.layout_node_mapping.get(i).and_then(|m| *m),
                    );
                    boxes.push((o.x, o.y));
                }
            }
        }
    }
    assert_eq!(
        boxes.len(),
        1,
        "the button's background was painted {} time(s), at {:?}",
        boxes.len(),
        boxes
    );
}
