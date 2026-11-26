//! AzWriter's format bar on a phone: a row of 48 dp keys that scrolls sideways.
//!
//! The ribbon (a tab strip over a band of groups, its small keys 22 px tall) does not fit a phone: on the
//! 411 dp emulator the tab strip and the groups ran out of the window on its right and the keys were no touch
//! targets. In a window narrower than `PHONE_WIDTH` the app shows `phone_bar` in the ribbon's place: File, then
//! the keys people use while they write, every one the engine's Button (48 dp on Android), in a row that
//! scrolls sideways when it is longer than the screen.
//!
//! The app is an example crate this one cannot link, so this reads its SOURCE, the way
//! `azul_paint_toolbar_fits_a_phone.rs` does: the row's style is the constant of
//! `examples/azul-writer/src/ribbon.rs`, the keys are the engine's own Buttons.

use std::sync::Arc;

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::{system::defaults, AzString};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, widgets::button::Button, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `examples/azul-writer/src/ribbon.rs`, verbatim, at compile time.
const RIBBON: &str = include_str!("../../examples/azul-writer/src/ribbon.rs");

/// The value of `const <name>: &str = "...";` as the compiler reads it: a `\` at the end of a line
/// continues the literal on the next one without its indentation.
fn const_css(name: &str) -> String {
    let head = format!("const {name}: &str =");
    let start = RIBBON.find(&head).unwrap_or_else(|| panic!("ribbon.rs has `{head}`"));
    let rest = &RIBBON[start + head.len()..];
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

/// The icons of `fn phone_bar` after File (undo, redo, bold, italic, underline, bullets, numbering,
/// the three alignments, settings).
const KEYS: [&str; 11] = [
    "undo",
    "redo",
    "format_bold",
    "format_italic",
    "format_underlined",
    "format_list_bulleted",
    "format_list_numbered",
    "format_align_left",
    "format_align_center",
    "format_align_right",
    "settings",
];

/// body > [bar > [File, the keys], page] in a window `width` x 800 with the style of an Android phone.
fn lay_out(width: f32) -> LayoutWindow {
    let mut bar = Dom::create_div().with_css(&const_css("PHONE_BAR")).with_id("bar".into());
    bar.add_child(Button::create(AzString::from("File")).dom());
    for icon in KEYS {
        bar.add_child(Button::create(AzString::from("")).with_icon(AzString::from(icon)).dom());
    }
    let dom = Dom::create_body()
        .with_css("margin: 0px; width: 100%; height: 100%; display: flex; flex-direction: column;")
        .with_child(bar)
        .with_child(Dom::create_div().with_css("flex-grow: 1;"));
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    lw.set_system_style(Arc::new(defaults::android_material_light()));
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(width, 800.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("layout");
    lw
}

/// The box of every node under the bar (the node with the id `bar`): its children, each key's first.
fn keys_of_the_bar(w: &LayoutWindow) -> (LogicalRect, Vec<LogicalRect>) {
    let lr = w.layout_results.get(&DomId::ROOT_ID).expect("root layout");
    let rect_of = |node: NodeId| {
        let idx = *lr.layout_tree.dom_to_layout[&node].first().expect("laid out");
        let size = lr.layout_tree.get(idx).expect("layout node").used_size.expect("sized");
        LogicalRect { origin: lr.calculated_positions[idx.index()], size }
    };
    let bar = (0..lr.styled_dom.node_data.as_ref().len())
        .map(NodeId::new)
        .find(|n| {
            lr.styled_dom.node_data.as_container()[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| matches!(c.as_id(), Some(s) if s == "bar"))
        })
        .expect("the bar");
    let mut keys = Vec::new();
    let mut child = lr.styled_dom.node_hierarchy.as_container()[bar].first_child_id(bar);
    while let Some(key) = child {
        keys.push(rect_of(key));
        child = lr.styled_dom.node_hierarchy.as_container()[key].next_sibling_id();
    }
    (rect_of(bar), keys)
}

#[test]
fn the_bar_is_the_width_of_the_screen_and_every_key_is_48_dp() {
    for width in [360.0, 393.0, 411.0] {
        let w = lay_out(width);
        let (bar, keys) = keys_of_the_bar(&w);
        assert!(
            bar.origin.x >= -0.5 && bar.origin.x + bar.size.width <= width + 0.5,
            "{width} dp: the bar runs from {} to {}",
            bar.origin.x,
            bar.origin.x + bar.size.width
        );
        assert_eq!(keys.len(), KEYS.len() + 1, "{width} dp: File and the keys are the bar's children");
        for (index, key) in keys.iter().enumerate() {
            assert!(
                key.size.width >= 47.9 && key.size.height >= 47.9,
                "{width} dp: key {index} is {} x {}",
                key.size.width,
                key.size.height
            );
        }
        assert!(bar.size.height >= 48.0, "{width} dp: the bar is {} tall", bar.size.height);
    }
}

#[test]
fn the_first_keys_are_on_the_screen_without_scrolling() {
    for (width, at_least) in [(360.0, 5), (393.0, 6), (411.0, 7)] {
        let w = lay_out(width);
        let (_, keys) = keys_of_the_bar(&w);
        let visible = keys
            .iter()
            .filter(|key| key.origin.x >= -0.5 && key.origin.x + key.size.width <= width + 0.5)
            .count();
        assert!(
            visible >= at_least,
            "{width} dp: {visible} keys are fully on the screen, {at_least} are wanted: {keys:?}"
        );
    }
}

#[test]
fn the_keys_come_in_a_row_in_the_order_of_the_bar() {
    let w = lay_out(411.0);
    let (bar, keys) = keys_of_the_bar(&w);
    for pair in keys.windows(2) {
        assert!(
            pair[1].origin.x >= pair[0].origin.x + pair[0].size.width - 0.5,
            "one key after the other: {pair:?}"
        );
        assert!(
            (pair[1].origin.y - pair[0].origin.y).abs() <= 8.0,
            "all in one row: {pair:?}"
        );
    }
    assert!(keys[0].origin.y >= bar.origin.y - 0.5, "inside the bar");
}
