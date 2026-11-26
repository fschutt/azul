//! The AzWidgets video card fits a phone.
//!
//! The Video card was a fixed 480 x 270 box (the stage, its overlay button and the row of the seek bar
//! under it: `width: 480px`) in a page whose padding leaves a 411 dp phone 363 dp: the card ran out of
//! the right edge of the screen and every node in it (the overlay button, the seek bar, the play badge)
//! with it - 150 nodes crossed the edge in the e2e layout check of the QA run 45778714528.
//!
//! The demo is an example crate this one cannot link, so this reads its SOURCE, the way
//! `azul_widgets_toolbar_wraps_on_a_narrow_screen.rs` does: the three styles of the card are taken
//! from `examples/azul-widgets/src/video.rs` and laid out in a page with the demo's own 24 px padding
//! at the widths of a small phone, a phone, a tablet and a desktop window. No node may cross the
//! window's right edge, the stage is as wide as the page leaves it up to the clip's own 480 px, and it
//! keeps the clip's 16:9.

use azul_core::{
    dom::{Dom, DomId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `examples/azul-widgets/src/video.rs`, verbatim, at compile time.
const VIDEO: &str = include_str!("../../examples/azul-widgets/src/video.rs");

/// The page's own padding (`lib.rs`: the scrolling column the cards sit in).
const PAGE_PADDING: f32 = 24.0;

/// The value of `const <name>: &str = "...";` as the compiler reads it: a `\` at the end of a line
/// continues the literal on the next one without its indentation.
fn const_css(name: &str) -> String {
    let head = format!("const {name}: &str =");
    let start = VIDEO.find(&head).unwrap_or_else(|| panic!("video.rs has `{head}`"));
    let rest = &VIDEO[start + head.len()..];
    let open = rest.find('"').expect("the constant is a string literal");
    let mut out = String::new();
    let mut chars = rest[open + 1..].chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => return out,
            '\\' => match chars.next() {
                Some('\n') => {
                    while chars.peek().is_some_and(|n| n.is_whitespace()) {
                        chars.next();
                    }
                }
                Some(other) => out.push(other),
                None => break,
            },
            _ => out.push(c),
        }
    }
    panic!("the literal of {name} does not end");
}

/// The card in the demo's page: body > page > [stage > overlay, controls > three boxes].
fn lay_out(window_width: f32) -> LayoutWindow {
    let overlay = Dom::create_div().with_css(&const_css("OVERLAY_CSS"));
    let stage = Dom::create_div().with_css(&const_css("STAGE_CSS")).with_child(overlay);
    let controls = Dom::create_div()
        .with_css(&const_css("CONTROLS_CSS"))
        .with_child(Dom::create_div().with_css(&const_css("TOGGLE_CSS")))
        .with_child(Dom::create_div().with_css("width: 40px; height: 16px;"))
        .with_child(Dom::create_div().with_css(&const_css("SEEK_CSS")));
    let page = Dom::create_div()
        .with_css(&format!(
            "display: flex; flex-direction: column; padding: {PAGE_PADDING}px; flex-grow: 1;"
        ))
        .with_child(stage)
        .with_child(controls);
    let mut dom = Dom::create_body()
        .with_css("margin: 0px; width: 100%; height: 100%; display: flex; flex-direction: column;")
        .with_child(page);
    let (css, _) = azul_css::parser2::new_from_str("");
    let styled_dom = StyledDom::create(&mut dom, css);

    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(window_width, 800.0);
    let mut debug_messages = Some(Vec::new());
    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut debug_messages,
        )
        .unwrap();
    layout_window
}

/// Every laid-out node of the root document, by its index in the DOM, in window coordinates.
fn every_rect(w: &LayoutWindow) -> Vec<(usize, LogicalRect)> {
    let lr = w.layout_results.get(&DomId::ROOT_ID).expect("root layout");
    let mut out = Vec::new();
    for (dom_node, indices) in lr.layout_tree.dom_to_layout.iter() {
        for idx in indices {
            let hot = lr.layout_tree.get(*idx).expect("layout node");
            if let Some(size) = hot.used_size {
                out.push((
                    dom_node.index(),
                    LogicalRect { origin: lr.calculated_positions[idx.index()], size },
                ));
            }
        }
    }
    out.sort_by_key(|(node, _)| *node);
    out
}

const WIDTHS: [f32; 4] = [320.0, 360.0, 411.0, 1024.0];

#[test]
fn no_node_of_the_video_card_crosses_the_right_edge_of_a_phone() {
    for width in WIDTHS {
        let rects = every_rect(&lay_out(width));
        assert!(rects.len() >= 7, "the card's nodes were laid out: {}", rects.len());
        for (node, r) in rects {
            assert!(
                r.origin.x + r.size.width <= width + 0.5,
                "{width} dp wide: node {node} runs from {} to {} px, past the window's edge",
                r.origin.x,
                r.origin.x + r.size.width
            );
        }
    }
}

#[test]
fn the_stage_is_as_wide_as_the_page_leaves_it_up_to_the_clips_480_and_keeps_16_to_9() {
    for width in WIDTHS {
        let rects = every_rect(&lay_out(width));
        // body 0, page 1, stage 2, overlay 3
        let stage = rects.iter().find(|(node, _)| *node == 2).expect("the stage").1;
        let overlay = rects.iter().find(|(node, _)| *node == 3).expect("the overlay").1;
        let wanted = (width - 2.0 * PAGE_PADDING).min(480.0);
        assert!(
            (stage.size.width - wanted).abs() <= 1.0,
            "{width} dp: the stage is {} wide, the page leaves {wanted}",
            stage.size.width
        );
        assert!(
            (stage.size.height - wanted * 9.0 / 16.0).abs() <= 1.0,
            "{width} dp: the stage is {} tall for {} wide: not 16:9",
            stage.size.height,
            stage.size.width
        );
        assert!(
            (overlay.size.width - stage.size.width).abs() <= 1.0
                && (overlay.size.height - stage.size.height).abs() <= 1.0,
            "{width} dp: the overlay button covers the whole video, as the stage is"
        );
    }
}
