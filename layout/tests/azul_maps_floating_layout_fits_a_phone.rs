//! What floats on AzMaps' map fits a phone upright: the search bar with its chips on top, the keys at the right, the sheet from the
//! bottom.
//!
//! (It replaces the test of the planner sidebar, which was a sheet over the TOP of the map on a phone: the map now has the search
//! bar and the chips there and the engine's BottomSheet at the bottom, `examples/azul-maps/src/ui.rs`.) The 290 px column that ran
//! from the title to the bottom of every screen had covered the whole map of a 411 dp phone; what is checked here is that nothing of
//! the new layout does that again: on the phones the QA runs use (320 x 640 up to 411 x 914 dp, inside the bars' insets) the bar
//! and the chips stay between 12 px margins whatever the chips hold (the row scrolls sideways, it does not widen the page), the keys
//! stand above the sheet and below the chips, the sheet spans the window from its lower edge, and at the peek the map keeps most of
//! the screen.
//!
//! The app is an example crate this one cannot link, so this reads its SOURCE, the way `azul_widgets_video_card_fits_a_phone.rs`
//! does: the styles are the constants of `examples/azul-maps/src/ui.rs` and `lib.rs`, the sheet is the engine's own widget with the
//! numbers `ui::sheet` gives it, the keys are the engine's buttons, the root is the map's own root inside the bars. The window's
//! root is inset by the engine (`LayoutWindow::root_insets`), so the bar starts 8 px under the status bar's edge.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::{
    props::basic::pixel::{OptionPixelValue, PixelValue},
    system::SafeAreaInsets,
    AzString,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{
        bottom_sheet::{BottomSheet, SheetDetent},
        button::Button,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `examples/azul-maps/src/ui.rs`, `lib.rs` and `flow.rs`, verbatim, at compile time.
const UI: &str = include_str!("../../examples/azul-maps/src/ui.rs");
const MAPS: &str = include_str!("../../examples/azul-maps/src/lib.rs");
const FLOW: &str = include_str!("../../examples/azul-maps/src/flow.rs");

/// The value of `const <name>: &str = "...";` in `source` as the compiler reads it: a `\` at the end of a line continues the literal
/// on the next one without its indentation.
fn const_css(source: &str, name: &str) -> String {
    let head = format!("const {name}: &str =");
    let start = source.find(&head).unwrap_or_else(|| panic!("the source has `{head}`"));
    let rest = &source[start + head.len()..];
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

/// The value of `const <name>: f32 = <number>;` in `source`.
fn const_f32(source: &str, name: &str) -> f32 {
    let head = format!("const {name}: f32 =");
    let start = source.find(&head).unwrap_or_else(|| panic!("the source has `{head}`"));
    let rest = &source[start + head.len()..];
    let end = rest.find(';').expect("the constant ends");
    rest[..end].trim().parse().unwrap_or_else(|_| panic!("{name} is a plain number: {}", &rest[..end]))
}

/// The labels of the chips, the longest ones the app has in German.
const CHIP_LABELS: [&str; 5] = ["Essen und Trinken", "Cafe", "Tankstellen", "Parkplaetze", "Hotels und Unterkuenfte"];

struct Phone {
    window: LayoutWindow,
    /// The sheet's height at the peek, px.
    peek: f32,
}

/// body > root > [map area, top > [bar > [field, settings key], chips > [chip x 5]], keys > [zoom pair, locate, layers], sheet] in a
/// window `width` x `height` dp with a status bar of `top` and a navigation bar / home indicator of `bottom` dp, the keyboard
/// covering `keyboard` dp (none: 0) of the root's bottom.
fn lay_out(width: f32, height: f32, top: f32, bottom: f32, keyboard: f32) -> Phone {
    let bar_top = 8.0;
    let peek = const_f32(UI, "PEEK_HEIGHT");
    let float = const_css(UI, "FLOAT");

    let mut bar = Dom::create_div()
        .with_css(&format!("{} {float}", const_css(UI, "SEARCH_BAR")))
        .with_id("bar".into())
        .with_child(
            Dom::create_div()
                .with_css(&const_css(UI, "FIELD"))
                .with_child(Dom::create_div().with_css("height: 40px;")),
        );
    bar.add_child(Button::create(AzString::from("")).with_icon(AzString::from("settings")).dom());
    let mut chips = Dom::create_div().with_css(&const_css(UI, "CHIPS")).with_id("chips".into());
    for label in CHIP_LABELS {
        chips.add_child(
            Dom::create_div()
                .with_css(&const_css(UI, "CHIP"))
                .with_child(Button::create(AzString::from(label)).with_icon(AzString::from("restaurant")).dom()),
        );
    }
    let top_box = Dom::create_div()
        .with_css(&format!("{} top: {bar_top}px;", const_css(UI, "TOP")))
        .with_id("top".into())
        .with_child(bar)
        .with_child(chips);

    // The sheet: the engine's widget with the numbers of `ui::sheet` (the peek, a top margin that leaves the bar in sight, the keyboard).
    let root_height = height - top - bottom;
    // what the full sheet leaves above itself: the bar's height and the gaps over and under it (`ui::TOP_ROOM`)
    let top_room = const_f32(UI, "BAR_HEIGHT") + 2.0 * const_f32(UI, "BAR_GAP");
    let mut content = Dom::create_div().with_css("display: flex; flex-direction: column;");
    for _ in 0..14 {
        content.add_child(Dom::create_div().with_css("height: 48px; flex-shrink: 0;"));
    }
    let sheet = BottomSheet::create(content)
        .with_label(AzString::from("Place and directions"))
        .with_detent(SheetDetent::Peek)
        .with_lowest(SheetDetent::Peek)
        .with_highest(SheetDetent::Full)
        .with_container_height(root_height)
        .with_peek_height(peek)
        .with_top_margin(bar_top + top_room)
        .with_bottom_inset(keyboard);

    // The keys at the right stand above the sheet (what `controls_bottom` does).
    let keys_bottom = sheet.height_of(SheetDetent::Peek) + keyboard + const_f32(UI, "ABOVE_SHEET");
    let group = format!("{} {float}", const_css(UI, "GROUP"));
    let keys = Dom::create_div()
        .with_css(&format!("{} bottom: {keys_bottom}px;", const_css(UI, "CONTROLS")))
        .with_id("keys".into())
        .with_child(
            Dom::create_div()
                .with_css(&group)
                .with_child(Button::create(AzString::from("")).with_icon(AzString::from("add")).dom())
                .with_child(Button::create(AzString::from("")).with_icon(AzString::from("remove")).dom()),
        )
        .with_child(
            Dom::create_div()
                .with_css(&group)
                .with_child(Button::create(AzString::from("")).with_icon(AzString::from("my_location")).dom()),
        )
        .with_child(
            Dom::create_div()
                .with_css(&group)
                .with_child(Button::create(AzString::from("")).with_icon(AzString::from("layers")).dom()),
        );

    let root = Dom::create_div()
        .with_css(&const_css(MAPS, "ROOT"))
        .with_id("root".into())
        .with_child(Dom::create_div().with_css(&const_css(MAPS, "MAP_AREA")))
        .with_child(top_box)
        .with_child(keys)
        .with_child(sheet.dom().with_id("sheet".into()));
    let mut dom = Dom::create_body()
        .with_css("margin: 0px; width: 100%; height: 100%; display: flex; flex-direction: column;")
        .with_child(root);
    let (css, _) = azul_css::parser2::new_from_str("");
    let styled_dom = StyledDom::create(&mut dom, css);

    let mut window = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let px = |v: f32| OptionPixelValue::Some(PixelValue::px(v));
    window.safe_area_insets =
        SafeAreaInsets { top: px(top), bottom: px(bottom), ..SafeAreaInsets::default() };
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(width, height);
    window.current_window_state = window_state.clone();
    let mut debug_messages = Some(Vec::new());
    window
        .layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut debug_messages,
        )
        .expect("layout");
    Phone { window, peek }
}

/// The box of the node whose id is `id`, in window coordinates.
fn rect_of_id(w: &LayoutWindow, id: &str) -> LogicalRect {
    let lr = w.layout_results.get(&DomId::ROOT_ID).expect("root layout");
    let node = (0..lr.styled_dom.node_data.as_ref().len())
        .map(NodeId::new)
        .find(|n| {
            lr.styled_dom.node_data.as_container()[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| matches!(c.as_id(), Some(s) if s == id))
        })
        .unwrap_or_else(|| panic!("a node with the id {id}"));
    let idx = *lr.layout_tree.dom_to_layout[&node].first().expect("laid out");
    let size = lr.layout_tree.get(idx).expect("layout node").used_size.expect("sized");
    LogicalRect { origin: lr.calculated_positions[idx.index()], size }
}

/// Every laid-out node of the root document, in window coordinates.
fn every_rect(w: &LayoutWindow) -> Vec<(usize, LogicalRect)> {
    let lr = w.layout_results.get(&DomId::ROOT_ID).expect("root layout");
    let mut out = Vec::new();
    for (dom_node, indices) in lr.layout_tree.dom_to_layout.iter() {
        for idx in indices {
            let hot = lr.layout_tree.get(*idx).expect("layout node");
            if let Some(size) = hot.used_size {
                out.push((dom_node.index(), LogicalRect { origin: lr.calculated_positions[idx.index()], size }));
            }
        }
    }
    out
}

/// The phones of the QA runs: (width, height, status bar, navigation bar or home indicator), dp.
const PHONES: [(f32, f32, f32, f32); 4] =
    [(320.0, 640.0, 24.0, 0.0), (360.0, 800.0, 24.0, 24.0), (393.0, 852.0, 59.0, 34.0), (411.0, 914.0, 48.8, 48.0)];

#[test]
fn the_width_under_which_the_screen_is_a_phones_is_the_one_the_tests_of_the_app_use() {
    assert!(
        FLOW.contains("pub const WIDE_WIDTH: f32 = 560.0;"),
        "the width from which the panel is at the left changed: update this test and azul_maps_drive_banner_fits_a_phone.rs"
    );
}

#[test]
fn on_a_phone_the_bar_and_the_chips_are_the_screen_less_12_px_each_side_and_the_chips_scroll_instead_of_widening_the_page() {
    for (width, height, top, bottom) in PHONES {
        let p = lay_out(width, height, top, bottom, 0.0);
        let root = rect_of_id(&p.window, "root");
        assert!(
            (root.size.height - (height - top - bottom)).abs() < 1.0,
            "{width} dp: the root is the safe area, {} of {}",
            root.size.height,
            height - top - bottom
        );
        for id in ["bar", "chips"] {
            let r = rect_of_id(&p.window, id);
            assert!(
                (r.origin.x - 12.0).abs() <= 1.0 && (r.size.width - (width - 24.0)).abs() <= 1.0,
                "{width} dp: the {id} runs from {} to {}",
                r.origin.x,
                r.origin.x + r.size.width
            );
        }
        let bar = rect_of_id(&p.window, "bar");
        assert!(
            (bar.origin.y - (root.origin.y + 8.0)).abs() <= 1.0 && bar.size.height >= 56.0 - 0.5,
            "{width} dp: the bar is 56 dp tall, 8 px under the root's top edge: {bar:?}"
        );
        // five chips with long German words are wider than any phone: the page must not scroll sideways
        for (node, r) in every_rect(&p.window) {
            let inside_chips = {
                let chips = rect_of_id(&p.window, "chips");
                r.origin.x >= chips.origin.x - 0.5 && r.origin.y >= chips.origin.y - 0.5 && r.origin.y <= chips.origin.y + chips.size.height
            };
            if inside_chips {
                continue; // a chip may lie past the row's edge: the row scrolls
            }
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
fn on_a_phone_the_sheet_spans_the_window_from_its_lower_edge_and_the_map_keeps_most_of_the_screen_at_the_peek() {
    for (width, height, top, bottom) in PHONES {
        let p = lay_out(width, height, top, bottom, 0.0);
        let root = rect_of_id(&p.window, "root");
        let sheet = rect_of_id(&p.window, "sheet");
        assert!(sheet.origin.x.abs() <= 1.0 && (sheet.size.width - width).abs() <= 1.0, "{width} dp: the sheet is as wide as the window: {sheet:?}");
        assert!(
            (sheet.origin.y + sheet.size.height - (root.origin.y + root.size.height)).abs() <= 1.0,
            "{width} dp: the sheet's lower edge is the root's: {sheet:?} in {root:?}"
        );
        assert!((sheet.size.height - p.peek).abs() <= 1.0, "{width} dp: at its peek the sheet is {} px tall, not {}", sheet.size.height, p.peek);
        assert!(
            sheet.size.height <= 0.35 * root.size.height,
            "{width} dp: at the peek the map keeps most of the screen: {} of {}",
            sheet.size.height,
            root.size.height
        );
    }
}

#[test]
fn on_a_phone_the_keys_stand_above_the_sheet_and_below_the_chips_at_the_right() {
    for (width, height, top, bottom) in PHONES {
        let p = lay_out(width, height, top, bottom, 0.0);
        let keys = rect_of_id(&p.window, "keys");
        let sheet = rect_of_id(&p.window, "sheet");
        let chips = rect_of_id(&p.window, "chips");
        assert!(
            keys.origin.x + keys.size.width <= width - 12.0 + 1.0 && keys.origin.x >= 0.0,
            "{width} dp: the keys' column runs from {} to {}, the right margin is 12 px",
            keys.origin.x,
            keys.origin.x + keys.size.width
        );
        assert!(
            keys.origin.y + keys.size.height <= sheet.origin.y - 16.0 + 1.0,
            "{width} dp: the keys end at {}, 16 px above the sheet's top {}",
            keys.origin.y + keys.size.height,
            sheet.origin.y
        );
        assert!(
            keys.origin.y >= chips.origin.y + chips.size.height - 1.0,
            "{width} dp: the keys' column starts at {}, under the chips that end at {}",
            keys.origin.y,
            chips.origin.y + chips.size.height
        );
    }
}

#[test]
fn with_the_keyboard_up_the_sheet_lifts_above_it_and_the_keys_with_it() {
    let (width, height, top, bottom) = (411.0, 914.0, 48.8, 48.0);
    let down = lay_out(width, height, top, bottom, 0.0);
    let up = lay_out(width, height, top, bottom, 300.0);
    let root = rect_of_id(&up.window, "root");
    let sheet_up = rect_of_id(&up.window, "sheet");
    let sheet_down = rect_of_id(&down.window, "sheet");
    assert!(
        (sheet_up.origin.y + sheet_up.size.height - (root.origin.y + root.size.height - 300.0)).abs() <= 1.0,
        "the sheet's lower edge lies on the keyboard's top: {sheet_up:?} in {root:?}"
    );
    assert!((sheet_up.size.height - sheet_down.size.height).abs() <= 1.0, "the peek is the same height up or down");
    let keys_up = rect_of_id(&up.window, "keys");
    assert!(keys_up.origin.y + keys_up.size.height <= sheet_up.origin.y - 16.0 + 1.0, "the keys rise with the sheet");
}
