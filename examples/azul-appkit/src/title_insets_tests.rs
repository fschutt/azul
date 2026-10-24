//! The room a top strip keeps for the window's controls, checked against the engine's own
//! title bar: the strip must agree with what azul's `TabsInTitlebar` and `Titlebar` compute
//! from the same metrics, or an app's buttons end up under the controls.

use azul::{
    css::{SystemStyle, TitlebarMetrics},
    widgets::{TabsInTitlebar, Titlebar},
};

use crate::ui::{title_insets_of, TitleInsets};

fn style_with(metrics: TitlebarMetrics) -> SystemStyle {
    let mut style = SystemStyle::create_default();
    style.metrics.titlebar = metrics;
    style
}

#[test]
fn a_top_strip_keeps_the_room_the_engine_gives_the_window_controls() {
    let style = style_with(TitlebarMetrics::macos());
    let tabs = TabsInTitlebar::from_system_style(style.clone());
    let bar = Titlebar::from_system_style("", style.clone());
    let insets = title_insets_of(style);
    assert_eq!(
        insets,
        TitleInsets {
            left: tabs.left,
            right: tabs.right,
            height: bar.height,
        }
    );
    assert_eq!(insets.height, 28.0);
}

#[test]
fn the_traffic_lights_and_their_padding_are_kept_free_at_the_left_on_macos() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let insets = title_insets_of(style_with(TitlebarMetrics::macos()));
    assert_eq!(insets.left, 78.0 + 8.0);
    assert_eq!(insets.right, 0.0);
}

#[test]
fn a_title_bar_with_no_reported_height_takes_the_platforms_own() {
    let insets = title_insets_of(SystemStyle::create_default());
    let bar = Titlebar::from_system_style("", SystemStyle::create_default());
    assert!(insets.height > 0.0);
    assert_eq!(insets.height, bar.height);
}
