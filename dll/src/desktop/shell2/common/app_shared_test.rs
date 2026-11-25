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
    let running = AppShared::for_app(&config, AppSettings::default());
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

/// What the environment asked of the layout engine reaches every window of the app through the
/// ONE place that seeds a window (`seed_window`): the layout passes' options, the window's own
/// copy and its font manager's text settings. A window of an app that asked for nothing has the
/// defaults, which is the behaviour of an empty environment.
#[test]
fn a_window_is_seeded_with_the_layout_settings_its_app_read_from_the_environment() {
    use azul_css::env::MapEnv;
    use azul_layout::settings::{DenseTextMode, LayoutSettings};

    let asked = AppSettings::from_lookup(
        &MapEnv::new()
            .with("AZ_NO_DL_PATCH", "1")
            .with("AZ_DENSE_TEXT", "verify")
            .with("AZ_TEXT_HINTING", "0")
            .with("AZ_FONT_GC_TRACE", "1"),
    );
    assert_ne!(asked.layout, LayoutSettings::default());
    let config = AppConfig::default();

    let mut window = LayoutWindow::new(rust_fontconfig::FcFontCache::default()).expect("a window");
    assert!(window.display_list_options.patching, "an unseeded window patches its lists");
    AppShared::with_settings(&config, asked.clone()).seed_window(&mut window, &config);
    assert_eq!(window.settings, asked.layout);
    assert!(!window.display_list_options.patching, "AZ_NO_DL_PATCH reached the layout passes");
    assert_eq!(window.display_list_options.dense_text, DenseTextMode::Verify);
    assert!(!window.font_manager.settings.hinting, "AZ_TEXT_HINTING=0 reached the font manager");
    assert!(window.font_manager.settings.trace_font_gc, "so did the font trace");

    // the window of an app that asked for nothing, and of one made for a test
    for shared in [AppShared::new(&config), AppShared::for_app(&config, AppSettings::default())] {
        let mut other =
            LayoutWindow::new(rust_fontconfig::FcFontCache::default()).expect("a window");
        shared.seed_window(&mut other, &config);
        assert_eq!(other.settings, LayoutSettings::default());
        assert!(other.display_list_options.patching);
    }
}

/// An app that never read the process holds the empty environment's settings; a running app holds
/// what its environment asked, and a window reads it through the app it was built for.
#[test]
fn the_app_owns_what_its_environment_asked_and_a_test_app_holds_the_empty_one() {
    use azul_css::env::MapEnv;

    assert_eq!(*AppShared::new(&AppConfig::default()).settings(), AppSettings::default());

    let asked = AppSettings::from_lookup(&MapEnv::new().with("AZ_HIT_DEBUG", "1"));
    let running = AppShared::for_app(&AppConfig::default(), asked.clone());
    assert!(running.settings().shell.hit_debug);
    assert_eq!(*running.settings(), asked);

    // two apps share no settings either
    assert!(!AppShared::new(&AppConfig::default()).settings().shell.hit_debug);
    let tested = AppShared::with_settings(&AppConfig::default(), asked);
    assert!(tested.settings().shell.hit_debug);
}

/// A window is handed the layout settings of the app that builds it, whatever it started with
/// (`LayoutWindow::set_settings`): the e2e machinery of the window (`AZ_E2E_SHOT_DIR`,
/// `AZ_E2E_RECORD`, ...) and the solver's switches read them from there, and a window nobody
/// seeded has the empty environment's.
#[test]
fn a_seeded_window_holds_the_layout_settings_of_its_app() {
    let config = AppConfig::default();
    let mut settings = AppSettings::default();
    settings.layout.e2e.record_baselines = true;
    settings.layout.e2e.shot_dir = Some(std::path::PathBuf::from("/tmp/azul-shots"));
    settings.layout.solver.patching = false;
    let app = AppShared::with_settings(&config, settings.clone());

    let mut window =
        LayoutWindow::new(rust_fontconfig::FcFontCache::default()).expect("a layout window");
    assert_eq!(
        window.settings,
        azul_layout::settings::LayoutSettings::default(),
        "a window nobody seeded has the empty environment's"
    );
    app.seed_window(&mut window, &config);
    assert_eq!(window.settings, settings.layout);
    assert!(
        !window.display_list_options.patching,
        "the solver group is what the window's passes run under"
    );

    // another app's window sees its own app's settings, not this one's
    let other = AppShared::new(&config);
    let mut other_window =
        LayoutWindow::new(rust_fontconfig::FcFontCache::default()).expect("a layout window");
    other.seed_window(&mut other_window, &config);
    assert!(!other_window.settings.e2e.record_baselines);
    assert!(other_window.display_list_options.patching);
}
