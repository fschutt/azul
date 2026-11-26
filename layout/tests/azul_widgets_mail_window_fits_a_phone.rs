//! The AzWidgets mail window fits a phone.
//!
//! The Mail card of the widget showcase puts three panes in a row: the message list (300 px), the reading pane
//! (what is left) and the to-do bar (240 px). Side by side they are 540 px and 24 px of gaps in a card that leaves a
//! 411 dp phone 327 px: the panes ran out of the right edge of the screen with every node in them. In a window
//! narrower than `COMPACT_WIDTH` the panes stand one over the other, each as wide as the card; a wide window keeps
//! the row.
//!
//! The demo is an example crate this one cannot link, so this reads its SOURCE, the way
//! `azul_widgets_video_card_fits_a_phone.rs` does: the styles are the constants of
//! `examples/azul-widgets/src/mail.rs`, laid out in a page with the demo's own paddings.

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

/// `examples/azul-widgets/src/mail.rs`, verbatim, at compile time.
const MAIL: &str = include_str!("../../examples/azul-widgets/src/mail.rs");

/// The page's own padding (`lib.rs`) and the card's (`section`).
const PAGE_PADDING: f32 = 24.0;
const CARD_PADDING: f32 = 18.0;

/// The value of `const <name>: &str = "...";` as the compiler reads it: a `\` at the end of a line continues the
/// literal on the next one without its indentation.
fn const_css(name: &str) -> String {
    let head = format!("const {name}: &str =");
    let start = MAIL.find(&head).unwrap_or_else(|| panic!("mail.rs has `{head}`"));
    let rest = &MAIL[start + head.len()..];
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

/// body > page > card > window > [list pane, reading pane, to-do bar], the way `mail_section` builds them.
fn lay_out(width: f32) -> LayoutWindow {
    let compact = width < 560.0;
    let pane = const_css("PANE_CSS");
    let window_css = const_css(if compact { "WINDOW_CSS_PHONE" } else { "WINDOW_CSS" });
    let list_width = const_css(if compact { "FULL_WIDTH" } else { "LIST_WIDTH" });
    let todo_width = const_css(if compact { "FULL_WIDTH" } else { "TODO_WIDTH" });
    let window = Dom::create_div()
        .with_css(&window_css)
        .with_child(
            Dom::create_div()
                .with_css(&format!("{pane} {list_width}"))
                .with_child(Dom::create_div().with_css("height: 20px;")),
        )
        .with_child(
            Dom::create_div()
                .with_css(&format!("{pane} flex-grow: 1; min-width: 0px;"))
                .with_child(Dom::create_div().with_css("height: 20px;")),
        )
        .with_child(
            Dom::create_div()
                .with_css(&format!("{pane} {todo_width}"))
                .with_child(Dom::create_div().with_css("height: 20px;")),
        );
    let card = Dom::create_div()
        .with_css(&format!("display: flex; flex-direction: column; padding: {CARD_PADDING}px;"))
        .with_child(window);
    let page = Dom::create_div()
        .with_css(&format!(
            "display: flex; flex-direction: column; padding: {PAGE_PADDING}px; flex-grow: 1;"
        ))
        .with_child(card);
    let mut dom = Dom::create_body()
        .with_css("margin: 0px; width: 100%; height: 100%; display: flex; flex-direction: column;")
        .with_child(page);
    let (css, _) = azul_css::parser2::new_from_str("");
    let styled_dom = StyledDom::create(&mut dom, css);

    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(width, 1200.0);
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

/// The three panes' boxes: body 0, page 1, card 2, window 3, then each pane and its text (4 and 5, 6 and 7, 8 and 9).
fn panes(w: &LayoutWindow) -> [LogicalRect; 3] {
    let all = every_rect(w);
    let at = |index: usize| all.iter().find(|(node, _)| *node == index).expect("the pane").1;
    [at(4), at(6), at(8)]
}

#[test]
fn the_compact_width_is_the_one_the_app_uses() {
    assert!(
        MAIL.contains("pub(crate) const COMPACT_WIDTH: f32 = 560.0;"),
        "the app's width under which the screen is a phone's changed: update this test"
    );
}

#[test]
fn on_a_phone_the_three_panes_stand_one_over_the_other_inside_the_card() {
    for width in [320.0, 360.0, 411.0] {
        let w = lay_out(width);
        let card = width - 2.0 * PAGE_PADDING - 2.0 * CARD_PADDING;
        let [list, reading, todo] = panes(&w);
        for (what, pane) in [("list", list), ("reading", reading), ("to-do", todo)] {
            assert!(
                (pane.size.width - card).abs() <= 1.0,
                "{width} dp: the {what} pane is {} wide, the card leaves {card}",
                pane.size.width
            );
        }
        assert!(list.origin.y + list.size.height <= reading.origin.y + 0.5, "{width} dp: list over reading");
        assert!(reading.origin.y + reading.size.height <= todo.origin.y + 0.5, "{width} dp: reading over to-do");
        for (node, r) in every_rect(&w) {
            assert!(
                r.origin.x >= -0.5 && r.origin.x + r.size.width <= width + 0.5,
                "{width} dp: node {node} runs from {} to {}, past the screen's edge",
                r.origin.x,
                r.origin.x + r.size.width
            );
        }
    }
}

#[test]
fn a_wide_window_keeps_the_row_of_300_and_240_px_panes() {
    let w = lay_out(1024.0);
    let [list, reading, todo] = panes(&w);
    assert!((list.size.width - 300.0).abs() <= 1.0, "the list is {}", list.size.width);
    assert!((todo.size.width - 240.0).abs() <= 1.0, "the to-do bar is {}", todo.size.width);
    assert!(reading.size.width > 100.0, "the reading pane has the rest: {}", reading.size.width);
    assert!(
        (list.origin.y - reading.origin.y).abs() <= 1.0 && (reading.origin.y - todo.origin.y).abs() <= 1.0,
        "side by side"
    );
    assert!(list.origin.x < reading.origin.x && reading.origin.x < todo.origin.x, "in this order");
}
