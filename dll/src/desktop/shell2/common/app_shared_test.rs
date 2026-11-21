use super::*;

fn pinned(mode: DarkLightMode) -> AppConfig {
    let mut config = AppConfig::default();
    config.mode = OptionDarkLightMode::Some(mode);
    config
}

/// An app's state starts from its config alone: the mode it pins, the two flags, and nothing
/// chosen, reported or hooked yet.
#[test]
fn a_state_made_from_a_config_holds_what_the_config_says_and_nothing_else() {
    let shared = AppShared::new(&pinned(DarkLightMode::Dark));
    assert_eq!(shared.mode(), OptionDarkLightMode::Some(DarkLightMode::Dark));
    assert_eq!(shared.locale(), None);
    assert_eq!(shared.system_natural_scroll(), None);
    assert!(shared.gpu_provision.is_none(), "a test's windows never get the elevated repair");

    let mut config = AppConfig::default();
    config.synthesize_pinch_from_ctrl_wheel = false;
    config.expose_system_media_controls = true;
    let shared = AppShared::new(&config);
    assert_eq!(shared.mode(), OptionDarkLightMode::None, "an unpinned app follows the desktop");
    assert!(!shared.synthesize_pinch_from_ctrl_wheel());
    assert!(shared.expose_system_media_controls());
}

/// The platform's natural-scrolling preference is read for a RUNNING app only, and only when the
/// config asks for it: a config alone never reads the machine it runs on.
#[test]
fn only_a_running_app_reads_the_platform_and_gets_the_hooks() {
    let mut config = AppConfig::default();
    config.natural_scroll = NaturalScroll::System;
    assert_eq!(AppShared::new(&config).system_natural_scroll(), None);

    config.natural_scroll = NaturalScroll::Enabled;
    let running = AppShared::for_app(&config);
    assert_eq!(running.system_natural_scroll(), None, "only System asks the platform");
    assert!(running.gpu_provision.is_some(), "a running app's dialogs can provision the GPU");
}

/// Two apps - or two tests - share nothing: a choice made in one is never seen by the other,
/// which is what the process-wide statics this replaces could not give.
#[test]
fn what_one_app_chooses_the_other_never_sees() {
    let (a, b) = (AppShared::new(&AppConfig::default()), AppShared::new(&AppConfig::default()));
    a.set_mode(OptionDarkLightMode::Some(DarkLightMode::Dark));
    a.set_locale(Some("de-DE"));
    a.set_system_natural_scroll(Some(true));

    assert_eq!(a.mode(), OptionDarkLightMode::Some(DarkLightMode::Dark));
    assert_eq!(a.locale().as_deref(), Some("de-DE"));
    assert_eq!(a.system_natural_scroll(), Some(true));
    assert_eq!(b.mode(), OptionDarkLightMode::None);
    assert_eq!(b.locale(), None);
    assert_eq!(b.system_natural_scroll(), None);
}

/// The language the app chose is kept as a tag without its padding, and an empty one (or none)
/// goes back to the system's.
#[test]
fn an_empty_language_goes_back_to_the_system() {
    let shared = AppShared::new(&AppConfig::default());
    shared.set_locale(Some("  fr-FR "));
    assert_eq!(shared.locale().as_deref(), Some("fr-FR"));
    shared.set_locale(Some("   "));
    assert_eq!(shared.locale(), None);
    shared.set_locale(Some("ja-JP"));
    shared.set_locale(None);
    assert_eq!(shared.locale(), None);
}

/// Every mode, and every reading of the preference, comes back as it went in.
#[test]
fn the_mode_and_the_preference_round_trip() {
    let shared = AppShared::new(&AppConfig::default());
    for mode in [
        OptionDarkLightMode::Some(DarkLightMode::Light),
        OptionDarkLightMode::Some(DarkLightMode::Dark),
        OptionDarkLightMode::None,
    ] {
        shared.set_mode(mode);
        assert_eq!(shared.mode(), mode);
    }
    for natural in [Some(true), Some(false), None] {
        shared.set_system_natural_scroll(natural);
        assert_eq!(shared.system_natural_scroll(), natural);
    }
}

/// A window built from an app takes the app's choices as they stand NOW - the mode, the language
/// and the natural-scrolling preference - and the settings of the config; a window of another
/// app takes none of them.
#[test]
fn a_seeded_window_starts_in_what_its_app_chose() {
    let config = pinned(DarkLightMode::Light);
    let shared = AppShared::new(&config);
    shared.set_mode(OptionDarkLightMode::Some(DarkLightMode::Dark));
    shared.set_locale(Some("de-DE"));
    shared.set_system_natural_scroll(Some(true));

    let mut window =
        LayoutWindow::new(rust_fontconfig::FcFontCache::default()).expect("a layout window");
    shared.seed_window(&mut window, &config);
    assert_eq!(window.mode, OptionDarkLightMode::Some(DarkLightMode::Dark));
    assert_eq!(window.active_language().id.as_str(), "de-DE");
    assert_eq!(window.scroll_manager.system_natural_scroll(), Some(true));

    let other = AppShared::new(&config);
    let mut stranger =
        LayoutWindow::new(rust_fontconfig::FcFontCache::default()).expect("a layout window");
    other.seed_window(&mut stranger, &config);
    assert_eq!(stranger.mode, OptionDarkLightMode::Some(DarkLightMode::Light));
    assert_ne!(stranger.active_language().id.as_str(), "de-DE");
    assert_eq!(stranger.scroll_manager.system_natural_scroll(), None);
}
