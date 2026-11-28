//! "Reduce motion" on the glides of the text editor: the caret and the selection jump, the focus ring
//! does not glide, and the view follows the caret without the scroll spring. The app's interpolators are
//! kept (they are what the glides are drawn with when the user has not asked for less).

use std::sync::Arc;

use azul_css::{
    accessibility::{AccessibilityPreferences, SettingState},
    system::SystemStyle,
};

use super::Runner;

#[test]
fn reduced_motion_makes_the_caret_and_the_selection_jump_and_the_view_follow_without_a_spring() {
    // `true`: the system animations of an app (a 60 ms glide of the caret and the selection)
    let mut runner = Runner::new(400.0, 200.0, 96, true);
    let normal = runner.layout_window.effective_system_animations().into_owned();
    assert_eq!(normal.caret_tween_duration_ms, 60, "premise: the caret glides");
    assert_eq!(normal.selection_tween_duration_ms, 60, "premise: the selection glides");
    assert!(normal.caret_scroll_glide, "premise: the view follows with a spring");

    let mut style = SystemStyle::default();
    style.set_preferences(AccessibilityPreferences {
        reduced_motion: SettingState::On,
        ..AccessibilityPreferences::default()
    });
    runner.layout_window.set_system_style(Arc::new(style));
    let calm = runner.layout_window.effective_system_animations().into_owned();
    assert_eq!(calm.caret_tween_duration_ms, 0);
    assert_eq!(calm.selection_tween_duration_ms, 0);
    assert_eq!(calm.focus_ring_duration_ms, 0);
    assert!(!calm.caret_scroll_glide);

    // and back: the window that is no longer asked for less glides again
    runner
        .layout_window
        .set_system_style(Arc::new(SystemStyle::default()));
    let again = runner.layout_window.effective_system_animations().into_owned();
    assert_eq!(again.caret_tween_duration_ms, 60);
}
