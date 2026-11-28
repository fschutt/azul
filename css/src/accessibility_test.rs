//! The typed accessibility preferences: what each word means, what the categories scale to, how an
//! override layers on what the platform reported, and what the environment pins.

use alloc::string::ToString;

use super::*;
use crate::env::{MapEnv, NoEnv};

#[test]
fn an_option_nobody_answered_is_unknown_and_never_off() {
    let prefs = AccessibilityPreferences::default();
    assert!(prefs.is_empty());
    assert_eq!(prefs.bold_text, SettingState::Unknown);
    assert!(!prefs.bold_text.is_on());
    assert!(!prefs.bold_text.is_off(), "Unknown is not Off: the platform did not answer");
    assert!(!prefs.bold_text.is_known());
    assert_eq!(prefs.contrast, ContrastPreference::Unknown);
    assert_eq!(prefs.text_scale, OptionF32::None);
    assert_eq!(prefs.effective_text_scale(), 1.0, "unknown text size is the default size");
}

#[test]
fn a_setting_state_reads_the_words_every_tool_uses() {
    for on in ["1", "true", "on", "YES", "enabled", "active", "reduce", "more", "bold", "inverted"] {
        assert_eq!(SettingState::parse(on), Some(SettingState::On), "{on}");
    }
    for off in ["0", "false", "off", "No", "disabled", "none", "no-preference", "normal"] {
        assert_eq!(SettingState::parse(off), Some(SettingState::Off), "{off}");
    }
    for unknown in ["", "unknown", "auto", "default", "unset", "  "] {
        assert_eq!(SettingState::parse(unknown), Some(SettingState::Unknown), "{unknown:?}");
    }
    assert_eq!(SettingState::parse("maybe"), None);
    assert_eq!(SettingState::from_bool(true), SettingState::On);
    assert_eq!(SettingState::from_bool(false), SettingState::Off);
    assert_eq!(SettingState::On.or(SettingState::Off), SettingState::On);
    assert_eq!(SettingState::Unknown.or(SettingState::Off), SettingState::Off);
    assert_eq!(SettingState::Off.or(SettingState::On), SettingState::Off, "an answer is kept");
    assert_eq!(SettingState::On.as_condition(), BoolCondition::True);
    assert_eq!(SettingState::Off.as_condition(), BoolCondition::False);
    assert_eq!(SettingState::Unknown.as_condition(), BoolCondition::False);
}

#[test]
fn the_dynamic_type_categories_scale_by_their_body_size() {
    // Apple's table: Body text at each category, against the default (Large, 17 pt).
    let expected: [(TextSizeCategory, u8); 12] = [
        (TextSizeCategory::ExtraSmall, 14),
        (TextSizeCategory::Small, 15),
        (TextSizeCategory::Medium, 16),
        (TextSizeCategory::Large, 17),
        (TextSizeCategory::ExtraLarge, 19),
        (TextSizeCategory::ExtraExtraLarge, 21),
        (TextSizeCategory::ExtraExtraExtraLarge, 23),
        (TextSizeCategory::AccessibilityMedium, 28),
        (TextSizeCategory::AccessibilityLarge, 33),
        (TextSizeCategory::AccessibilityExtraLarge, 40),
        (TextSizeCategory::AccessibilityExtraExtraLarge, 47),
        (TextSizeCategory::AccessibilityExtraExtraExtraLarge, 53),
    ];
    for (category, points) in expected {
        assert_eq!(category.body_points(), Some(points), "{category:?}");
        let factor = category.scale_factor().expect("a known category has a factor");
        assert!((factor - f32::from(points) / 17.0).abs() < 1e-6, "{category:?} {factor}");
    }
    assert_eq!(TextSizeCategory::Large.scale_factor(), Some(1.0), "the default size is 1.0");
    let ax5 = TextSizeCategory::AccessibilityExtraExtraExtraLarge.scale_factor().unwrap();
    assert!((ax5 - 3.1176).abs() < 1e-3, "AX5 is about 3.12: {ax5}");
    let xs = TextSizeCategory::ExtraSmall.scale_factor().unwrap();
    assert!((xs - 0.8235).abs() < 1e-3, "the smallest is about 0.82: {xs}");
    assert_eq!(TextSizeCategory::Unknown.scale_factor(), None);
    assert!(TextSizeCategory::AccessibilityMedium.is_accessibility());
    assert!(!TextSizeCategory::ExtraExtraExtraLarge.is_accessibility());
    // The table grows with every step.
    let mut previous = 0.0_f32;
    for category in TextSizeCategory::ALL {
        let factor = category.scale_factor().unwrap();
        assert!(factor > previous, "{category:?}");
        previous = factor;
    }
}

#[test]
fn a_category_is_read_from_every_spelling_it_has() {
    use TextSizeCategory as C;
    // xcrun simctl ui <device> content_size
    for category in C::ALL {
        let name = category.simctl_name().unwrap();
        assert_eq!(C::parse(name), Some(category), "simctl name {name}");
    }
    // UIKit constants and the raw strings preferredContentSizeCategory answers.
    assert_eq!(C::parse("UIContentSizeCategoryExtraSmall"), Some(C::ExtraSmall));
    assert_eq!(C::parse("UICTContentSizeCategoryXS"), Some(C::ExtraSmall));
    assert_eq!(C::parse("UICTContentSizeCategoryS"), Some(C::Small));
    assert_eq!(C::parse("UICTContentSizeCategoryM"), Some(C::Medium));
    assert_eq!(C::parse("UICTContentSizeCategoryL"), Some(C::Large));
    assert_eq!(C::parse("UICTContentSizeCategoryXL"), Some(C::ExtraLarge));
    assert_eq!(C::parse("UICTContentSizeCategoryXXL"), Some(C::ExtraExtraLarge));
    assert_eq!(C::parse("UICTContentSizeCategoryXXXL"), Some(C::ExtraExtraExtraLarge));
    assert_eq!(C::parse("UICTContentSizeCategoryAccessibilityM"), Some(C::AccessibilityMedium));
    assert_eq!(C::parse("UICTContentSizeCategoryAccessibilityL"), Some(C::AccessibilityLarge));
    assert_eq!(C::parse("UICTContentSizeCategoryAccessibilityXL"), Some(C::AccessibilityExtraLarge));
    assert_eq!(
        C::parse("UICTContentSizeCategoryAccessibilityXXL"),
        Some(C::AccessibilityExtraExtraLarge)
    );
    assert_eq!(
        C::parse("UICTContentSizeCategoryAccessibilityXXXL"),
        Some(C::AccessibilityExtraExtraExtraLarge)
    );
    assert_eq!(
        C::parse("UIContentSizeCategoryAccessibilityExtraExtraExtraLarge"),
        Some(C::AccessibilityExtraExtraExtraLarge)
    );
    // short forms of the e2e ops
    assert_eq!(C::parse("ax1"), Some(C::AccessibilityMedium));
    assert_eq!(C::parse("AX5"), Some(C::AccessibilityExtraExtraExtraLarge));
    assert_eq!(C::parse("xxxl"), Some(C::ExtraExtraExtraLarge));
    assert_eq!(C::parse("extra_extra_large"), Some(C::ExtraExtraLarge));
    assert_eq!(C::parse("UICTContentSizeCategoryUnspecified"), None, "not a size: left to the caller");
    assert_eq!(C::parse("unspecified"), Some(C::Unknown));
    assert_eq!(C::parse("huge"), None);
    assert_eq!(C::parse("1.5"), None, "a factor is not a category");
}

#[test]
fn the_nearest_category_of_a_factor() {
    use TextSizeCategory as C;
    assert_eq!(C::nearest(1.0), C::Large);
    assert_eq!(C::nearest(0.85), C::ExtraSmall, "0.82 is nearer to 0.85 than 0.88");
    assert_eq!(C::nearest(0.88), C::Small);
    assert_eq!(C::nearest(0.1), C::ExtraSmall);
    assert_eq!(C::nearest(1.3), C::ExtraExtraExtraLarge, "Android's largest step, 1.3");
    assert_eq!(C::nearest(2.0), C::AccessibilityLarge);
    assert_eq!(C::nearest(3.0), C::AccessibilityExtraExtraExtraLarge);
    assert_eq!(C::nearest(40.0), C::AccessibilityExtraExtraExtraLarge, "a larger factor is the largest");
    assert_eq!(C::nearest(f32::NAN), C::Unknown);
    assert_eq!(C::nearest(f32::INFINITY), C::Unknown);
}

#[test]
fn a_text_scale_is_capped_with_a_diagnostic_and_never_crashes() {
    assert_eq!(clamp_text_scale(1.0), Some((1.0, false)));
    assert_eq!(clamp_text_scale(1.3), Some((1.3, false)));
    assert_eq!(clamp_text_scale(0.1), Some((MIN_TEXT_SCALE, true)));
    assert_eq!(clamp_text_scale(100.0), Some((MAX_TEXT_SCALE, true)));
    assert_eq!(clamp_text_scale(f32::INFINITY), Some((MAX_TEXT_SCALE, true)));
    assert_eq!(clamp_text_scale(f32::NAN), None, "not a number: no scale at all");

    let mut prefs = AccessibilityPreferences::default();
    assert!(prefs.set_text_scale(1.5).is_none(), "in range: no diagnostic");
    assert_eq!(prefs.effective_text_scale(), 1.5);
    assert!(
        matches!(
            prefs.text_size_category,
            TextSizeCategory::ExtraExtraExtraLarge | TextSizeCategory::AccessibilityMedium
        ),
        "1.5 sits between XXXL and AX1: {:?}",
        prefs.text_size_category
    );
    let note = prefs.set_text_scale(99.0).expect("capped");
    assert!(note.contains("99") && note.contains("4"), "{note}");
    assert_eq!(prefs.effective_text_scale(), MAX_TEXT_SCALE);
    let note = prefs.set_text_scale(f32::NAN).expect("not a number");
    assert!(note.contains("not a number"), "{note}");
    assert_eq!(prefs.text_scale, OptionF32::None);
    assert_eq!(prefs.effective_text_scale(), 1.0);

    // Whatever a platform reports, the engine reads a scale in range.
    for reported in [f32::NEG_INFINITY, -3.0, 0.0, 1e-9, 1e9, f32::MAX] {
        let prefs = AccessibilityPreferences {
            text_scale: OptionF32::Some(reported),
            ..Default::default()
        };
        let scale = prefs.effective_text_scale();
        assert!((MIN_TEXT_SCALE..=MAX_TEXT_SCALE).contains(&scale), "{reported} -> {scale}");
    }
}

#[test]
fn the_text_scale_is_active_only_off_the_default() {
    let mut prefs = AccessibilityPreferences::default();
    assert!(!prefs.text_scale_is_active());
    prefs.set_text_scale(1.0);
    assert!(!prefs.text_scale_is_active());
    prefs.set_text_scale(1.3);
    assert!(prefs.text_scale_is_active());
    prefs.set_text_scale(0.85);
    assert!(prefs.text_scale_is_active(), "smaller text is a scale too");
}

#[test]
fn an_override_replaces_what_it_knows_and_leaves_the_rest() {
    let mut detected = AccessibilityPreferences::default();
    detected.set_text_scale(1.15);
    detected.bold_text = SettingState::Off;
    detected.contrast = ContrastPreference::NoPreference;
    detected.reduced_motion = SettingState::On;
    detected.long_press_ms = OptionU32::Some(400);

    let mut pin = AccessibilityPreferences::default();
    pin.set_text_scale(2.0);
    pin.bold_text = SettingState::On;
    pin.contrast = ContrastPreference::More;

    let effective = detected.overlaid(&pin);
    assert_eq!(effective.effective_text_scale(), 2.0, "the pin's scale");
    assert_eq!(effective.bold_text, SettingState::On, "the pin's bold text");
    assert_eq!(effective.contrast, ContrastPreference::More);
    assert_eq!(effective.reduced_motion, SettingState::On, "unpinned: what the system said");
    assert_eq!(effective.long_press_ms, OptionU32::Some(400), "unpinned numbers stay too");
    // an empty override is the identity, and the override of an empty set is the override
    assert_eq!(detected.overlaid(&AccessibilityPreferences::default()), detected);
    assert_eq!(AccessibilityPreferences::default().overlaid(&pin), pin);
    // an explicit Off in the pin is an answer: it replaces an On
    let mut off = AccessibilityPreferences::default();
    off.reduced_motion = SettingState::Off;
    assert_eq!(detected.overlaid(&off).reduced_motion, SettingState::Off);
}

#[test]
fn the_derived_questions_follow_the_options() {
    let mut prefs = AccessibilityPreferences::default();
    assert!(!prefs.reduces_motion());
    assert!(!prefs.high_contrast());
    assert_eq!(prefs.bold_weight_step(), 0);

    prefs.animation_scale = OptionF32::Some(0.0);
    assert!(prefs.reduces_motion(), "the system's animation speed of zero is reduced motion");
    prefs.animation_scale = OptionF32::Some(1.0);
    assert!(!prefs.reduces_motion());
    prefs.reduced_motion = SettingState::On;
    assert!(prefs.reduces_motion());

    prefs.contrast = ContrastPreference::More;
    assert!(prefs.high_contrast());
    prefs.contrast = ContrastPreference::NoPreference;
    assert!(!prefs.high_contrast());
    prefs.forced_colors = SettingState::On;
    assert!(prefs.high_contrast(), "a contrast theme is high contrast");

    prefs.bold_text = SettingState::On;
    assert_eq!(prefs.bold_weight_step(), BOLD_TEXT_WEIGHT_STEP, "only 'on': the engine's step");
    prefs.bold_weight_adjustment = OptionI16::Some(300);
    assert_eq!(prefs.bold_weight_step(), 300, "the platform's own adjustment");
    prefs.bold_weight_adjustment = OptionI16::Some(5000);
    assert_eq!(prefs.bold_weight_step(), 1000, "capped");
    prefs.bold_weight_adjustment = OptionI16::Some(0);
    assert_eq!(prefs.bold_weight_step(), BOLD_TEXT_WEIGHT_STEP, "0 with bold on: the platform did not say");
    prefs.bold_text = SettingState::Off;
    prefs.bold_weight_adjustment = OptionI16::None;
    assert_eq!(prefs.bold_weight_step(), 0);
}

#[test]
fn every_field_is_set_from_text_by_its_own_name() {
    for field in PREFERENCE_FIELDS {
        let mut prefs = AccessibilityPreferences::default();
        let value = match field {
            "text_scale" => "1.5",
            "text_size_category" => "accessibility-large",
            "contrast" => "more",
            "bold_weight_adjustment" => "300",
            "animation_scale" | "pointer_scale" => "2",
            "long_press_ms" | "action_timeout_ms" => "1500",
            _ => "on",
        };
        let result = prefs.set_from_text(field, value);
        assert!(result.is_ok(), "{field}: {result:?}");
        assert!(!prefs.is_empty(), "{field} set nothing");
        // and "unknown" puts it back
        let back = prefs.set_from_text(field, "unknown");
        assert!(back.is_ok(), "{field}: {back:?}");
        assert!(prefs.is_empty(), "{field} did not reset: {prefs:?}");
        // the e2e key is the same name in any case
        let mut upper = AccessibilityPreferences::default();
        assert!(upper.set_from_text(&field.to_ascii_uppercase(), value).is_ok(), "{field}");
        assert_eq!(upper, {
            let mut again = AccessibilityPreferences::default();
            again.set_from_text(field, value).unwrap();
            again
        });
    }
    let mut prefs = AccessibilityPreferences::default();
    let err = prefs.set_from_text("sparkles", "on").expect_err("no such option");
    assert!(err.contains("sparkles") && err.contains("text_scale"), "{err}");
    assert!(prefs.is_empty());
}

#[test]
fn a_bad_value_leaves_the_field_as_it_was() {
    let mut prefs = AccessibilityPreferences::default();
    prefs.set_from_text("bold_text", "on").unwrap();
    assert!(prefs.set_from_text("bold_text", "perhaps").is_err());
    assert_eq!(prefs.bold_text, SettingState::On);
    assert!(prefs.set_from_text("text_scale", "huge").is_err());
    assert!(prefs.set_from_text("text_scale", "nan").is_err());
    assert_eq!(prefs.text_scale, OptionF32::None);
    assert!(prefs.set_from_text("long_press_ms", "-4").is_err());
    assert!(prefs.set_from_text("animation_scale", "-1").is_err());
    assert!(prefs.set_from_text("contrast", "loud").is_err());
    // numbers are capped, not refused
    prefs.set_from_text("long_press_ms", "99999999").unwrap();
    assert_eq!(prefs.long_press_ms, OptionU32::Some(MAX_TIMEOUT_MS));
    prefs.set_from_text("animation_scale", "500").unwrap();
    assert_eq!(prefs.animation_scale, OptionF32::Some(MAX_ANIMATION_SCALE));
    let note = prefs.set_from_text("text_scale", "7").unwrap();
    assert!(note.is_some(), "a scale past the range is capped with a diagnostic");
    assert_eq!(prefs.effective_text_scale(), MAX_TEXT_SCALE);
}

#[test]
fn the_environment_pins_what_it_names() {
    let (empty, notes) = AccessibilityPreferences::from_lookup(&NoEnv);
    assert!(empty.is_empty());
    assert!(notes.is_empty());

    let env = MapEnv::new()
        .with("AZ_A11Y_TEXT_SCALE", "AX5")
        .with("AZ_A11Y_BOLD_TEXT", "on")
        .with("AZ_A11Y_CONTRAST", "more")
        .with("AZ_A11Y_REDUCED_MOTION", "1")
        .with("AZ_A11Y_REDUCED_TRANSPARENCY", "off")
        .with("AZ_A11Y_LONG_PRESS_MS", "1500")
        .with("AZ_MODE", "dark");
    let (pin, notes) = AccessibilityPreferences::from_lookup(&env);
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(pin.text_size_category, TextSizeCategory::AccessibilityExtraExtraExtraLarge);
    assert!((pin.effective_text_scale() - 3.1176).abs() < 1e-3);
    assert_eq!(pin.bold_text, SettingState::On);
    assert_eq!(pin.contrast, ContrastPreference::More);
    assert_eq!(pin.reduced_motion, SettingState::On);
    assert_eq!(pin.reduced_transparency, SettingState::Off, "an explicit off is a pin");
    assert_eq!(pin.long_press_ms, OptionU32::Some(1500));
    assert_eq!(pin.screen_reader, SettingState::Unknown, "not named: not pinned");

    let env = MapEnv::new()
        .with("AZ_A11Y_BOLD_TEXT", "maybe")
        .with("AZ_A11Y_TEXT_SCALE", "50");
    let (pin, notes) = AccessibilityPreferences::from_lookup(&env);
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("AZ_A11Y_BOLD_TEXT") && n.contains("maybe")), "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("AZ_A11Y_TEXT_SCALE")), "{notes:?}");
    assert_eq!(pin.bold_text, SettingState::Unknown, "a bad value pins nothing");
    assert_eq!(pin.effective_text_scale(), MAX_TEXT_SCALE, "a scale out of range is capped");
}

#[test]
fn the_json_names_every_option_once() {
    let mut prefs = AccessibilityPreferences::default();
    prefs.set_text_scale(1.3);
    prefs.bold_text = SettingState::On;
    prefs.contrast = ContrastPreference::More;
    prefs.long_press_ms = OptionU32::Some(1000);
    let json = prefs.to_json();
    assert!(json.starts_with('{') && json.ends_with('}'), "{json}");
    for field in PREFERENCE_FIELDS {
        let key = alloc::format!("\"{field}\":");
        assert_eq!(json.matches(key.as_str()).count(), 1, "{field} in {json}");
    }
    assert!(json.contains("\"text_scale\": 1.300"), "{json}");
    assert!(json.contains("\"bold_text\": \"on\""), "{json}");
    assert!(json.contains("\"contrast\": \"more\""), "{json}");
    assert!(json.contains("\"long_press_ms\": 1000"), "{json}");
    assert!(json.contains("\"screen_reader\": \"unknown\""), "{json}");
    assert!(json.contains("\"pointer_scale\": null"), "{json}");
    assert_eq!(json.to_string(), prefs.to_json(), "deterministic");
}
