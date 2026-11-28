//! A segmented control's choices are touch targets on a phone: at least 48 dp on Android and 44 pt on iOS in
//! each direction, like the `Button` (see `a_button_is_a_touch_target_on_a_phone.rs`).
//!
//! A desktop choice is 28 px tall (6 px of padding round a 14 px line, and its border): the four places of
//! AzClock's old mode row measured 28 dp tall on the Android emulator (QA run 46121386177), which a finger
//! misses. The floor is a property of the segment the themes put on it for those two platforms only: a desktop
//! control keeps its size.

use std::sync::Arc;

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::{
    system::{defaults, SystemStyle},
    AzString, StringVec,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{segmented::Segmented, themes::UiTheme},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const ITEM_CLASS: &str = "__azul-native-segmented-item";
const THEMES: [UiTheme; 2] = [UiTheme::Flat, UiTheme::Flora];

fn choices() -> Segmented {
    let labels = ["World", "Alarms", "Timer", "Stopwatch"];
    Segmented::create(StringVec::from_vec(labels.iter().map(|l| AzString::from(*l)).collect()))
        .with_selected_index(1)
}

/// The size of every choice of `segmented` laid out in a 411 x 914 window of `style`.
fn sizes(style: SystemStyle, segmented: Segmented) -> Vec<(f32, f32)> {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.set_system_style(Arc::new(style));
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(411.0, 914.0);
    lw.current_window_state = ws.clone();
    let dom = Dom::create_body()
        .with_css("margin: 0; display: flex; flex-direction: column;")
        .with_child(segmented.dom());
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("layout");
    let lr = &lw.layout_results[&DomId::ROOT_ID];
    let items: Vec<(f32, f32)> = (0..lr.styled_dom.node_data.as_ref().len())
        .map(NodeId::new)
        .filter(|n| {
            lr.styled_dom.node_data.as_container()[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| matches!(c.as_class(), Some(s) if s == ITEM_CLASS))
        })
        .map(|node| {
            let idx = *lr.layout_tree.dom_to_layout[&node].first().expect("laid out");
            let size = lr.layout_tree.get(idx).expect("layout node").used_size.expect("sized");
            (size.width, size.height)
        })
        .collect();
    assert_eq!(items.len(), 4, "one node a choice");
    items
}

#[test]
fn a_choice_is_48_dp_in_each_direction_on_android_in_every_theme() {
    for theme in THEMES {
        for (i, (w, h)) in sizes(defaults::android_material_light(), choices().with_theme(theme)).into_iter().enumerate() {
            assert!(w >= 47.9 && h >= 47.9, "{theme:?} choice {i}: {w} x {h} dp on Android");
        }
    }
}

#[test]
fn a_choice_is_44_pt_in_each_direction_on_ios_in_every_theme() {
    for theme in THEMES {
        for (i, (w, h)) in sizes(defaults::ios_light(), choices().with_theme(theme)).into_iter().enumerate() {
            assert!(w >= 43.9 && h >= 43.9, "{theme:?} choice {i}: {w} x {h} pt on iOS");
        }
    }
}

#[test]
fn a_desktop_choice_keeps_its_size() {
    for theme in THEMES {
        for style in [defaults::macos_modern_light(), defaults::windows_11_light()] {
            for (i, (_, h)) in sizes(style, choices().with_theme(theme)).into_iter().enumerate() {
                assert!(h < 40.0, "{theme:?} choice {i}: a desktop choice does not grow to a finger: {h}");
            }
        }
    }
}
