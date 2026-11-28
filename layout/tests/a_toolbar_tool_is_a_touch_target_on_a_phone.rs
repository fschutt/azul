//! A toolbar's tool is a touch target on a phone: at least 48 dp on Android and 44 pt on iOS in each
//! direction, like the `Button` it is built from (see `a_button_is_a_touch_target_on_a_phone.rs`).
//!
//! A desktop toolbar tool is 28 px tall (an icon of 18 px in a row of 3 px of padding and a 1 px border):
//! AzReader's phone toolbar (back, smaller, larger, bookmark, Settings) measured 28 dp tall on the Android
//! emulator, which a finger misses. The floor is a property the tool takes for those two platforms only:
//! a desktop toolbar keeps its size, and the separator, the spacer and an app's own control (a text field
//! of the toolbar's, which has its own floor) are not tools.

use std::sync::Arc;

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::{
    system::{defaults, SystemStyle},
    AzString,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{
        themes::UiTheme,
        toolbar::{Toolbar, ToolbarItem, ToolbarItemVec},
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The icon-only tools of AzReader's phone bar, a toggle among them, in `theme`.
fn bar(theme: UiTheme) -> Toolbar {
    let tools = vec![
        ToolbarItem::create_button(AzString::from("back"), AzString::from("Library"), AzString::from("arrow_back")),
        ToolbarItem::create_spacer(),
        ToolbarItem::create_button(AzString::from("smaller"), AzString::from("Smaller text"), AzString::from("remove")),
        ToolbarItem::create_button(AzString::from("larger"), AzString::from("Larger text"), AzString::from("add")),
        ToolbarItem::create_toggle(
            AzString::from("mark"),
            AzString::from("Bookmark"),
            AzString::from("bookmark_border"),
            false,
        ),
    ];
    Toolbar::create(AzString::from("Reader tools"))
        .with_items(ToolbarItemVec::from_vec(tools))
        .with_theme(theme)
}

/// The size of the node with the id `id` in `toolbar` laid out in a 411 x 914 window of `style`.
fn size_of(style: SystemStyle, toolbar: Toolbar, id: &str) -> (f32, f32) {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.set_system_style(Arc::new(style));
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(411.0, 914.0);
    lw.current_window_state = ws.clone();
    let dom = Dom::create_body()
        .with_css("margin: 0; display: flex; flex-direction: column;")
        .with_child(toolbar.dom());
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("layout");
    let lr = &lw.layout_results[&DomId::ROOT_ID];
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
    (size.width, size.height)
}

const THEMES: [UiTheme; 2] = [UiTheme::Flat, UiTheme::Flora];
const TOOLS: [&str; 4] = ["back", "smaller", "larger", "mark"];

#[test]
fn a_tool_is_48_dp_in_each_direction_on_android_in_every_theme() {
    for theme in THEMES {
        for id in TOOLS {
            let (w, h) = size_of(defaults::android_material_light(), bar(theme), id);
            assert!(w >= 47.9 && h >= 47.9, "{theme:?} {id}: {w} x {h} dp on Android");
        }
    }
}

#[test]
fn a_tool_is_44_pt_in_each_direction_on_ios_in_every_theme() {
    for theme in THEMES {
        for id in TOOLS {
            let (w, h) = size_of(defaults::ios_light(), bar(theme), id);
            assert!(w >= 43.9 && h >= 43.9, "{theme:?} {id}: {w} x {h} pt on iOS");
        }
    }
}

#[test]
fn a_desktop_tool_keeps_its_size() {
    for theme in THEMES {
        for style in [defaults::macos_modern_light(), defaults::windows_11_light()] {
            for id in TOOLS {
                let (_, h) = size_of(style.clone(), bar(theme), id);
                assert!(h < 40.0, "{theme:?} {id}: a desktop tool does not grow to a finger: {h}");
            }
        }
    }
}
