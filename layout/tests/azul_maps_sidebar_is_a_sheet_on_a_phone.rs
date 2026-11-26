//! The AzMaps sidebar is a sheet over the top of the map on a portrait phone.
//!
//! The sidebar was `position: absolute; top: 44px; bottom: 26px; width: 290px` on every screen: on a 411 dp
//! phone it ran from the title to the bottom and covered the whole map, which is what the boot picture of
//! the Android emulator showed (a grey card and "no tiles": the tiles were there, behind it - they show
//! in the landscape picture of the same run). On a phone the sidebar is a sheet: as wide as the screen less
//! 8 px each side, as tall as its content up to 62 % of the screen, and it starts closed there.
//!
//! The app is an example crate this one cannot link, so this reads its SOURCE, the way
//! `azul_widgets_video_card_fits_a_phone.rs` does: the styles are taken from
//! `examples/azul-maps/src/lib.rs` and laid out in the map's own root with a list too long for the screen
//! in it, inside the insets of the emulator's bars.

use azul_core::{
    dom::{Dom, DomId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::{
    props::basic::pixel::{OptionPixelValue, PixelValue},
    system::SafeAreaInsets,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `examples/azul-maps/src/lib.rs`, verbatim, at compile time.
const MAPS: &str = include_str!("../../examples/azul-maps/src/lib.rs");

/// The value of `const <name>: &str = "...";` as the compiler reads it: a `\` at the end of a line
/// continues the literal on the next one without its indentation.
fn const_css(name: &str) -> String {
    let head = format!("const {name}: &str =");
    let start = MAPS.find(&head).unwrap_or_else(|| panic!("lib.rs has `{head}`"));
    let rest = &MAPS[start + head.len()..];
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

/// The map's root with its map area and the sidebar `css` with `rows` rows of 48 px in it. The
/// body is the safe rect of the window (the engine insets the root by the bars).
fn lay_out(width: f32, height: f32, top: f32, bottom: f32, sidebar_css: &str, rows: usize) -> LayoutWindow {
    let mut sidebar = Dom::create_div().with_css(sidebar_css);
    for _ in 0..rows {
        sidebar.add_child(Dom::create_div().with_css("height: 48px; flex-shrink: 0;"));
    }
    let root = Dom::create_div()
        .with_css(&const_css("ROOT"))
        .with_child(Dom::create_div().with_css(&const_css("MAP_AREA")))
        .with_child(sidebar);
    let mut dom = Dom::create_body()
        .with_css("margin: 0px; width: 100%; height: 100%; display: flex; flex-direction: column;")
        .with_child(root);
    let (css, _) = azul_css::parser2::new_from_str("");
    let styled_dom = StyledDom::create(&mut dom, css);

    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    let px = |v: f32| OptionPixelValue::Some(PixelValue::px(v));
    layout_window.safe_area_insets =
        SafeAreaInsets { top: px(top), bottom: px(bottom), ..SafeAreaInsets::default() };
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(width, height);
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

/// The laid-out rect of the node with DOM index `index` (body 0, root 1, map area 2, sidebar 3).
fn rect_of(w: &LayoutWindow, index: usize) -> LogicalRect {
    let lr = w.layout_results.get(&DomId::ROOT_ID).expect("root layout");
    let (_, indices) = lr
        .layout_tree
        .dom_to_layout
        .iter()
        .find(|(dom_node, _)| dom_node.index() == index)
        .unwrap_or_else(|| panic!("node {index} was laid out"));
    let idx = indices[0];
    let hot = lr.layout_tree.get(idx).expect("layout node");
    LogicalRect {
        origin: lr.calculated_positions[idx.index()],
        size: hot.used_size.expect("sized"),
    }
}

const COMPACT_WIDTH: f32 = 560.0;

#[test]
fn the_compact_width_is_the_one_the_app_uses() {
    assert!(
        MAPS.contains(&format!("const COMPACT_WIDTH: f32 = {COMPACT_WIDTH:.1};")),
        "the app's width under which the sidebar is a sheet changed: update this test"
    );
}

#[test]
fn on_a_portrait_phone_the_sheet_leaves_the_map_visible_below_it() {
    // the Android emulator of the QA runs: 411 x 914 dp, bars of 48.8 and 48 dp
    let w = lay_out(411.0, 914.0, 48.8, 48.0, &const_css("SIDEBAR_SHEET"), 14);
    let root = rect_of(&w, 1);
    let sheet = rect_of(&w, 3);
    assert!((root.size.height - (914.0 - 48.8 - 48.0)).abs() < 1.0, "the root is the safe area");
    assert!((sheet.origin.x - 8.0).abs() <= 1.0, "8 px from the left edge: {}", sheet.origin.x);
    assert!(
        (sheet.size.width - (411.0 - 16.0)).abs() <= 1.0,
        "as wide as the screen less 8 px each side: {}",
        sheet.size.width
    );
    assert!(
        (sheet.origin.y - (root.origin.y + 60.0)).abs() <= 1.0,
        "below the title row (56 dp, the app bar of the 48 dp keys), inside the bars: {}",
        sheet.origin.y
    );
    // `max-height` caps the content box: the sheet's 12 px of padding come on top of it.
    assert!(
        sheet.size.height <= 0.62 * root.size.height + 24.0 + 1.0,
        "14 rows of 48 px are 672 px and the sheet is capped at 62 % of the screen: {} of {}",
        sheet.size.height,
        root.size.height
    );
    assert!(
        sheet.size.height >= 0.5 * root.size.height,
        "the cap, not the content, ended it: {} of {}",
        sheet.size.height,
        root.size.height
    );
    assert!(
        sheet.origin.y + sheet.size.height <= root.origin.y + root.size.height - 200.0,
        "at least 200 px of map stay free below the sheet"
    );
}

#[test]
fn a_short_sidebar_is_as_tall_as_its_content_on_a_phone() {
    let w = lay_out(411.0, 914.0, 48.8, 48.0, &const_css("SIDEBAR_SHEET"), 2);
    let sheet = rect_of(&w, 3);
    // 2 rows of 48 px, a 12 px gap between them, 12 px padding above and below
    assert!(
        sheet.size.height < 200.0,
        "two rows do not fill the screen (the column ran to the bottom whatever it held): {}",
        sheet.size.height
    );
}

#[test]
fn on_a_desktop_window_the_sidebar_is_still_the_column_along_the_window() {
    let w = lay_out(1024.0, 720.0, 0.0, 0.0, &const_css("SIDEBAR"), 3);
    let root = rect_of(&w, 1);
    let column = rect_of(&w, 3);
    assert!(
        (column.size.height - (root.size.height - 44.0 - 26.0)).abs() <= 1.0,
        "title row above, 26 px below: {} of {}",
        column.size.height,
        root.size.height
    );
    assert!(column.size.width < 340.0, "the column is 290 px of content: {}", column.size.width);
}
