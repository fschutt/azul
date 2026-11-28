//! The arrows of a time picker and its AM/PM toggle are touch targets on a phone: at least 48 dp on Android and 44 pt
//! on iOS in each direction, like the `Button` (see `a_button_is_a_touch_target_on_a_phone.rs`).
//!
//! A desktop arrow is a 16 px cell (40 px wide) with an 11 px glyph in it: the arrows of AzClock's alarm editor measured
//! 40 x 16 dp on the Android emulator (the picture of run 46129541989), which a finger misses by a third. The floor is a
//! property of the cells the picker builds, for those two platforms only: a desktop picker keeps its size.

use std::sync::Arc;

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::system::{defaults, SystemStyle};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{themes::UiTheme, time_picker::TimePicker},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const ARROW_CLASS: &str = "__azul-native-time-picker-arrow";
const AMPM_CLASS: &str = "__azul-native-time-picker-ampm";
const THEMES: [UiTheme; 2] = [UiTheme::Flat, UiTheme::Flora];

/// The `(width, height)` of every node with `class` in a 12 hour picker laid out in a 411 x 914 window of `style`.
fn sizes(style: SystemStyle, theme: UiTheme, class: &str) -> Vec<(f32, f32)> {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.set_system_style(Arc::new(style));
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(411.0, 914.0);
    lw.current_window_state = ws.clone();
    let dom = Dom::create_body()
        .with_css("margin: 0; display: flex; flex-direction: column;")
        .with_child(TimePicker::create(7, 30).with_24h(false).with_theme(theme).dom());
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("layout");
    let lr = &lw.layout_results[&DomId::ROOT_ID];
    (0..lr.styled_dom.node_data.as_ref().len())
        .map(NodeId::new)
        .filter(|n| {
            lr.styled_dom.node_data.as_container()[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| matches!(c.as_class(), Some(s) if s == class))
        })
        .map(|node| {
            let idx = *lr.layout_tree.dom_to_layout[&node].first().expect("laid out");
            let size = lr.layout_tree.get(idx).expect("layout node").used_size.expect("sized");
            (size.width, size.height)
        })
        .collect()
}

#[test]
fn an_arrow_and_the_ampm_toggle_are_48_dp_in_each_direction_on_android_in_every_theme() {
    for theme in THEMES {
        for class in [ARROW_CLASS, AMPM_CLASS] {
            let cells = sizes(defaults::android_material_light(), theme, class);
            assert!(!cells.is_empty(), "{theme:?}: a cell of class {class}");
            for (i, (w, h)) in cells.into_iter().enumerate() {
                assert!(w >= 47.9 && h >= 47.9, "{theme:?} {class} {i}: {w} x {h} dp on Android");
            }
        }
    }
}

#[test]
fn an_arrow_and_the_ampm_toggle_are_44_pt_in_each_direction_on_ios_in_every_theme() {
    for theme in THEMES {
        for class in [ARROW_CLASS, AMPM_CLASS] {
            for (i, (w, h)) in sizes(defaults::ios_light(), theme, class).into_iter().enumerate() {
                assert!(w >= 43.9 && h >= 43.9, "{theme:?} {class} {i}: {w} x {h} pt on iOS");
            }
        }
    }
}

#[test]
fn a_desktop_arrow_keeps_its_size() {
    for theme in THEMES {
        for style in [defaults::macos_modern_light(), defaults::windows_11_light()] {
            for (i, (_, h)) in sizes(style, theme, ARROW_CLASS).into_iter().enumerate() {
                assert!(h < 30.0, "{theme:?} arrow {i}: a desktop arrow does not grow to a finger: {h}");
            }
        }
    }
}
