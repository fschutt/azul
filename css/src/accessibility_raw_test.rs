//! What each platform's raw answers mean as typed preferences: the iOS size categories, Android's
//! zero animation scale and its nonlinear font scale, the registry values of Windows, GNOME's
//! text scaling factor, and above all that a missing answer is Unknown and never Off.

use super::*;
use crate::accessibility::{
    ContrastPreference as Contrast, SettingState as State, TextSizeCategory as Category,
};

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn a_platform_that_answered_nothing_means_nothing() {
    assert!(RawIos::default().into_preferences().is_empty());
    assert!(RawAndroid::default().into_preferences().is_empty());
    assert!(RawMacos::default().into_preferences().is_empty());
    assert!(RawGnome::default().into_preferences().is_empty());
    assert!(RawKde::default().into_preferences().is_empty());
    // Windows' text scale slider at its default is an answer: no value, 100 %
    let windows = RawWindows::default().into_preferences();
    assert_eq!(windows.effective_text_scale(), 1.0);
    assert_eq!(windows.bold_text, State::Unknown, "Windows has no bold text: unknown, not off");
    assert_eq!(windows.reduced_motion, State::Unknown);
}

#[test]
fn state_of_keeps_unknown_apart_from_off() {
    assert_eq!(state_of(Some(true)), State::On);
    assert_eq!(state_of(Some(false)), State::Off);
    assert_eq!(state_of(None), State::Unknown);
    assert_eq!(contrast_of(Some(true)), Contrast::More);
    assert_eq!(contrast_of(Some(false)), Contrast::NoPreference);
    assert_eq!(contrast_of(None), Contrast::Unknown);
    assert_eq!(either(Some(false), Some(true)), Some(true));
    assert_eq!(either(None, Some(false)), Some(false));
    assert_eq!(either(None, None), None);
}

#[test]
fn ios_reads_the_size_category_and_every_flag() {
    let raw = RawIos {
        content_size_category: Some("UICTContentSizeCategoryAccessibilityXXXL".into()),
        bold_text: Some(true),
        darker_system_colors: Some(true),
        reduce_motion: Some(false),
        prefers_cross_fade: Some(true),
        reduce_transparency: Some(true),
        invert_colors: Some(false),
        differentiate_without_color: Some(true),
        button_shapes: Some(true),
        on_off_labels: Some(false),
        voice_over: Some(true),
        ..RawIos::default()
    };
    let preferences = raw.into_preferences();
    assert_eq!(preferences.text_size_category, Category::AccessibilityExtraExtraExtraLarge);
    assert!(near(preferences.effective_text_scale(), 53.0 / 17.0), "AX5 is 53/17");
    assert_eq!(preferences.bold_text, State::On);
    assert_eq!(preferences.contrast, Contrast::More);
    assert_eq!(preferences.reduced_motion, State::Off, "an answered false is Off");
    assert_eq!(preferences.cross_fade_transitions, State::On);
    assert_eq!(preferences.reduced_transparency, State::On);
    assert_eq!(preferences.invert_colors, State::Off);
    assert_eq!(preferences.differentiate_without_color, State::On);
    assert_eq!(preferences.button_shapes, State::On);
    assert_eq!(preferences.on_off_labels, State::Off);
    assert_eq!(preferences.screen_reader, State::On);
    assert_eq!(preferences.switch_access, State::Unknown, "not asked");
    assert_eq!(preferences.grayscale, State::Unknown, "not asked");
}

#[test]
fn ios_every_category_gives_its_body_factor() {
    for category in Category::ALL {
        let raw = RawIos {
            content_size_category: category.simctl_name().map(String::from),
            ..RawIos::default()
        };
        let preferences = raw.into_preferences();
        assert_eq!(preferences.text_size_category, category);
        let factor = category.scale_factor().unwrap();
        assert!(near(preferences.effective_text_scale(), factor), "{category:?}");
    }
}

#[test]
fn ios_a_category_this_build_does_not_know_falls_back_to_what_uikit_scaled_the_body_to() {
    let raw = RawIos {
        content_size_category: Some("UICTContentSizeCategoryAccessibilityXXXXL".into()),
        scaled_body_points: Some(60.0),
        ..RawIos::default()
    };
    let preferences = raw.into_preferences();
    assert!(near(preferences.effective_text_scale(), 60.0 / 17.0));
    // `Unspecified` and nothing at all say nothing
    let unspecified = RawIos {
        content_size_category: Some("UICTContentSizeCategoryUnspecified".into()),
        ..RawIos::default()
    };
    assert_eq!(unspecified.into_preferences().text_scale, crate::OptionF32::None);
    // a nonsense number of points is no scale
    let nonsense = RawIos { scaled_body_points: Some(f32::NAN), ..RawIos::default() };
    assert!(nonsense.into_preferences().is_empty());
}

#[test]
fn android_takes_the_frameworks_own_scale_before_the_font_scale() {
    let linear = RawAndroid { font_scale: Some(1.3), ..RawAndroid::default() };
    assert!(near(linear.into_preferences().effective_text_scale(), 1.3));
    // Android 14: the font scale is 2.0 and a body `sp` becomes 1.6 dp per sp (nonlinear)
    let nonlinear = RawAndroid {
        font_scale: Some(2.0),
        sp_scale: Some(1.6),
        ..RawAndroid::default()
    };
    assert!(near(nonlinear.into_preferences().effective_text_scale(), 1.6));
    let broken = RawAndroid { font_scale: Some(0.0), sp_scale: Some(f32::NAN), ..RawAndroid::default() };
    assert!(broken.into_preferences().is_empty(), "no usable scale is no scale");
}

#[test]
fn android_bold_text_is_the_font_weight_adjustment() {
    let on = RawAndroid { font_weight_adjustment: Some(300), ..RawAndroid::default() }.into_preferences();
    assert_eq!(on.bold_text, State::On);
    assert_eq!(on.bold_weight_step(), 300);
    let off = RawAndroid { font_weight_adjustment: Some(0), ..RawAndroid::default() }.into_preferences();
    assert_eq!(off.bold_text, State::Off);
    assert_eq!(off.bold_weight_step(), 0);
    // FONT_WEIGHT_ADJUSTMENT_UNDEFINED (and an Android older than 12, with no such field)
    let undefined = RawAndroid {
        font_weight_adjustment: Some(ANDROID_FONT_WEIGHT_UNDEFINED),
        ..RawAndroid::default()
    }
    .into_preferences();
    assert_eq!(undefined.bold_text, State::Unknown);
    assert_eq!(RawAndroid::default().into_preferences().bold_text, State::Unknown);
}

#[test]
fn android_remove_animations_is_a_zero_animator_scale() {
    let off = RawAndroid { animator_duration_scale: Some(0.0), ..RawAndroid::default() }.into_preferences();
    assert_eq!(off.reduced_motion, State::On);
    assert!(off.reduces_motion());
    let normal = RawAndroid { animator_duration_scale: Some(1.0), ..RawAndroid::default() }.into_preferences();
    assert_eq!(normal.reduced_motion, State::Off);
    let slow = RawAndroid { animator_duration_scale: Some(5.0), ..RawAndroid::default() }.into_preferences();
    assert_eq!(slow.reduced_motion, State::Off, "a developer's slow motion is not reduced motion");
    assert_eq!(slow.animation_scale, crate::OptionF32::Some(5.0));
    let nothing = RawAndroid::default().into_preferences();
    assert_eq!(nothing.reduced_motion, State::Unknown);
}

#[test]
fn android_contrast_comes_from_the_ui_mode_level_or_the_text_setting() {
    let level = |contrast: Option<f32>, text: Option<bool>| {
        RawAndroid { contrast, high_text_contrast: text, ..RawAndroid::default() }
            .into_preferences()
            .contrast
    };
    assert_eq!(level(Some(0.0), None), Contrast::NoPreference);
    assert_eq!(level(Some(0.5), None), Contrast::More, "medium");
    assert_eq!(level(Some(1.0), None), Contrast::More, "high");
    assert_eq!(level(Some(-1.0), None), Contrast::Less);
    assert_eq!(level(None, Some(true)), Contrast::More, "high contrast text");
    assert_eq!(level(Some(0.0), Some(true)), Contrast::More);
    assert_eq!(level(None, Some(false)), Contrast::NoPreference);
    assert_eq!(level(None, None), Contrast::Unknown);
}

#[test]
fn android_colour_filters_and_the_assistive_technology() {
    let raw = RawAndroid {
        invert_colors: Some(true),
        daltonizer_enabled: Some(true),
        daltonizer_type: Some(0),
        touch_exploration: Some(true),
        captions: Some(false),
        long_press_ms: Some(1500),
        action_timeout_ms: Some(-1),
        pointer_scale: Some(2.0),
        blur_disabled: Some(true),
        ..RawAndroid::default()
    };
    let p = raw.into_preferences();
    assert_eq!(p.invert_colors, State::On);
    assert_eq!(p.color_filter, State::On);
    assert_eq!(p.grayscale, State::On, "daltonizer type 0 is monochromacy");
    assert_eq!(p.screen_reader, State::On, "TalkBack is touch exploration");
    assert_eq!(p.captions, State::Off);
    assert_eq!(p.long_press_ms, crate::OptionU32::Some(1500));
    assert_eq!(p.action_timeout_ms, crate::OptionU32::None, "a negative timeout is unknown");
    assert_eq!(p.pointer_scale, crate::OptionF32::Some(2.0));
    assert_eq!(p.reduced_transparency, State::On, "blur effects off");
    assert_eq!(p.dim_flashing_lights, State::Unknown, "reduce bright colors cannot be read by an app");
    assert_eq!(p.button_shapes, State::Unknown, "Android has no such option");

    let coloured = RawAndroid {
        daltonizer_enabled: Some(true),
        daltonizer_type: Some(12),
        ..RawAndroid::default()
    }
    .into_preferences();
    assert_eq!(coloured.color_filter, State::On);
    assert_eq!(coloured.grayscale, State::Off, "deuteranomaly is a colour filter, not grayscale");
    let none = RawAndroid { daltonizer_enabled: Some(false), ..RawAndroid::default() }.into_preferences();
    assert_eq!(none.color_filter, State::Off);
    assert_eq!(none.grayscale, State::Off);
}

#[test]
fn windows_text_scale_is_the_slider_percentage() {
    let at = |percent: Option<u32>| {
        RawWindows { text_scale_percent: percent, ..RawWindows::default() }
            .into_preferences()
            .effective_text_scale()
    };
    assert_eq!(at(None), 1.0);
    assert_eq!(at(Some(100)), 1.0);
    assert!(near(at(Some(150)), 1.5));
    assert!(near(at(Some(225)), 2.25), "the slider's end");
    assert!(near(at(Some(40_000)), 4.0), "a value no slider has is capped");
    assert!(near(at(Some(0)), 0.5), "and so is a zero");
}

#[test]
fn windows_contrast_themes_are_forced_colours_and_more_contrast() {
    let on = RawWindows { high_contrast: Some(true), ..RawWindows::default() }.into_preferences();
    assert_eq!(on.forced_colors, State::On);
    assert_eq!(on.contrast, Contrast::More);
    assert!(on.high_contrast());
    let off = RawWindows { high_contrast: Some(false), ..RawWindows::default() }.into_preferences();
    assert_eq!(off.forced_colors, State::Off);
    assert_eq!(off.contrast, Contrast::NoPreference);
}

#[test]
fn windows_effects_toggles_are_reduced_motion_and_reduced_transparency_the_other_way_round() {
    let raw = RawWindows {
        client_area_animation: Some(false),
        transparency_effects: Some(false),
        ..RawWindows::default()
    }
    .into_preferences();
    assert_eq!(raw.reduced_motion, State::On, "animation effects off");
    assert_eq!(raw.reduced_transparency, State::On, "transparency effects off");
    let normal = RawWindows {
        client_area_animation: Some(true),
        transparency_effects: Some(true),
        ..RawWindows::default()
    }
    .into_preferences();
    assert_eq!(normal.reduced_motion, State::Off);
    assert_eq!(normal.reduced_transparency, State::Off);
}

#[test]
fn windows_pointer_colour_filters_notifications_and_mono_audio() {
    let raw = RawWindows {
        cursor_base_size: Some(96),
        color_filter_active: Some(true),
        color_filter_type: Some(1),
        screen_reader: Some(true),
        message_duration_s: Some(30),
        mono_audio: Some(true),
        magnifier_running: Some(true),
        ..RawWindows::default()
    }
    .into_preferences();
    assert_eq!(raw.magnifier, State::On);
    assert_eq!(raw.pointer_scale, crate::OptionF32::Some(3.0));
    assert_eq!(raw.color_filter, State::On);
    assert_eq!(raw.invert_colors, State::On);
    assert_eq!(raw.grayscale, State::Off);
    assert_eq!(raw.screen_reader, State::On);
    assert_eq!(raw.action_timeout_ms, crate::OptionU32::Some(30_000));
    assert_eq!(raw.mono_audio, State::On);

    let both = RawWindows {
        color_filter_active: Some(true),
        color_filter_type: Some(2),
        ..RawWindows::default()
    }
    .into_preferences();
    assert_eq!((both.invert_colors, both.grayscale), (State::On, State::On), "grayscale inverted");
    let gray = RawWindows {
        color_filter_active: Some(true),
        color_filter_type: Some(0),
        ..RawWindows::default()
    }
    .into_preferences();
    assert_eq!((gray.invert_colors, gray.grayscale), (State::Off, State::On));
    let inactive = RawWindows { color_filter_active: Some(false), ..RawWindows::default() }.into_preferences();
    assert_eq!((inactive.color_filter, inactive.grayscale), (State::Off, State::Off));
}

#[test]
fn macos_maps_the_workspace_flags() {
    let p = RawMacos {
        reduce_motion: Some(true),
        increase_contrast: Some(false),
        reduce_transparency: Some(true),
        differentiate_without_color: Some(true),
        voice_over: Some(false),
        ..RawMacos::default()
    }
    .into_preferences();
    assert_eq!(p.reduced_motion, State::On);
    assert_eq!(p.contrast, Contrast::NoPreference);
    assert_eq!(p.reduced_transparency, State::On);
    assert_eq!(p.differentiate_without_color, State::On);
    assert_eq!(p.screen_reader, State::Off);
    assert_eq!(p.text_scale, crate::OptionF32::None, "macOS has no system text size an app can read");
    assert_eq!(p.bold_text, State::Unknown);
}

#[test]
fn gnome_text_scaling_animations_contrast_and_pointer() {
    let p = RawGnome {
        text_scaling_factor: Some(1.25),
        enable_animations: Some(false),
        high_contrast: Some(false),
        cursor_size: Some(48),
        screen_reader: Some(true),
        ..RawGnome::default()
    }
    .into_preferences();
    assert!(near(p.effective_text_scale(), 1.25));
    assert_eq!(p.reduced_motion, State::On, "animations off is reduced motion");
    assert_eq!(p.contrast, Contrast::NoPreference);
    assert_eq!(p.pointer_scale, crate::OptionF32::Some(2.0));
    assert_eq!(p.screen_reader, State::On);

    // the settings portal says it too, and either source is enough
    let portal = RawGnome {
        enable_animations: Some(true),
        portal_reduced_motion: Some(1),
        portal_contrast: Some(1),
        ..RawGnome::default()
    }
    .into_preferences();
    assert_eq!(portal.reduced_motion, State::On);
    assert_eq!(portal.contrast, Contrast::More);
    let quiet = RawGnome {
        enable_animations: Some(true),
        portal_reduced_motion: Some(0),
        portal_contrast: Some(0),
        high_contrast: Some(false),
        ..RawGnome::default()
    }
    .into_preferences();
    assert_eq!(quiet.reduced_motion, State::Off);
    assert_eq!(quiet.contrast, Contrast::NoPreference);
}

#[test]
fn gnome_reduced_motion_has_three_eras_and_status_shapes_are_two_options() {
    // GNOME 50's own key, while the animations switch of the older Settings is still on
    let reduce = RawGnome {
        enable_animations: Some(true),
        reduced_motion_key: Some(true),
        ..RawGnome::default()
    }
    .into_preferences();
    assert_eq!(reduce.reduced_motion, State::On);
    // 'no-preference' with animations on: off, known
    let calm = RawGnome {
        enable_animations: Some(true),
        reduced_motion_key: Some(false),
        ..RawGnome::default()
    }
    .into_preferences();
    assert_eq!(calm.reduced_motion, State::Off);
    // nothing answered: unknown, not off
    assert_eq!(RawGnome::default().into_preferences().reduced_motion, State::Unknown);

    // "On/Off Shapes" is the one setting behind two options
    let shapes = RawGnome { show_status_shapes: Some(true), ..RawGnome::default() }.into_preferences();
    assert_eq!(shapes.differentiate_without_color, State::On);
    assert_eq!(shapes.on_off_labels, State::On);
    let plain = RawGnome { show_status_shapes: Some(false), ..RawGnome::default() }.into_preferences();
    assert_eq!(plain.differentiate_without_color, State::Off);
    assert_eq!(RawGnome::default().into_preferences().on_off_labels, State::Unknown);
}

#[test]
fn kde_animation_factor_and_font_dpi() {
    let p = RawKde {
        animation_duration_factor: Some(0.0),
        force_font_dpi: Some(144.0),
    }
    .into_preferences();
    assert_eq!(p.reduced_motion, State::On);
    assert_eq!(p.animation_scale, crate::OptionF32::Some(0.0));
    assert!(near(p.effective_text_scale(), 1.5), "144 dpi over 96");
    let slow = RawKde { animation_duration_factor: Some(2.0), force_font_dpi: None }.into_preferences();
    assert_eq!(slow.reduced_motion, State::Off);
    assert_eq!(slow.text_scale, crate::OptionF32::None);
}

#[test]
fn windows_narrator_does_not_set_the_screen_reader_flag_but_its_running_state_counts() {
    let narrator = RawWindows {
        screen_reader: Some(false),
        narrator_running: Some(true),
        ..RawWindows::default()
    }
    .into_preferences();
    assert_eq!(narrator.screen_reader, State::On, "either source says on");
    let none = RawWindows {
        screen_reader: Some(false),
        narrator_running: Some(false),
        ..RawWindows::default()
    }
    .into_preferences();
    assert_eq!(none.screen_reader, State::Off);
    let unknown = RawWindows::default().into_preferences();
    assert_eq!(unknown.screen_reader, State::Unknown);
}

#[test]
fn ios_the_newer_settings_come_from_the_accessibility_and_media_frameworks() {
    let p = RawIos {
        dim_flashing_lights: Some(true),
        animated_images: Some(false),
        button_shapes: Some(true),
        ..RawIos::default()
    }
    .into_preferences();
    assert_eq!(p.dim_flashing_lights, State::On);
    assert_eq!(p.autoplay_animated_images, State::Off);
    assert_eq!(p.button_shapes, State::On);
    assert_eq!(p.magnifier, State::Unknown, "iOS has no public flag for Zoom");
}
