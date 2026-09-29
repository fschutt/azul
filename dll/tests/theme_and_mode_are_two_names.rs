//! "Theme" and "mode" are two names for two things (user ruling 2026-09-29).
//!
//! - The THEME is the app theme: `"flat"`, `"flora"`, later `"native"` and user themes.
//!   `AppConfig::with_theme`, `CallbackInfo::set_theme` / `get_theme`, and in `layout()`
//!   `LayoutCallbackInfo::get_theme`. A switch recreates every window's DOM
//!   (`RelayoutReason::ThemeChange`).
//! - The MODE is light / dark / system: `AppConfig::with_mode`, `CallbackInfo::set_mode` /
//!   `get_mode` / `get_resolved_mode`, and in `layout()` `LayoutCallbackInfo::get_mode`. A switch
//!   repaints, and re-runs only a `layout()` that read it (`RelayoutReason::ModeChange`).
//!
//! Before the rename the same call meant both: `LayoutCallbackInfo::get_theme` was light / dark
//! while `CallbackInfo::get_theme` was the app theme, the layout-side app-theme getter had to be
//! called `get_theme_name`, and the light / dark API was `*_color_scheme`. This file pins the
//! shape: it does not compile against the old names.
//!
//! The desktop here is the macOS light preset whatever the host runs.

use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex, MutexGuard,
    },
};

use azul::desktop::shell2::{
    common::{event::SharedUndoManager, PlatformWindow},
    headless::HeadlessWindow,
};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo, RelayoutReason, Update},
    dom::{Dom, NodeData},
    events::{ComponentEventFilter, EventFilter},
    icon::{IconProviderHandle, SharedIconProvider},
    refany::{OptionRefAny, RefAny},
    resources::AppConfig,
    window::{OptionWindowTheme, WindowTheme},
};
use azul_css::AzString;
use azul_layout::{
    callbacks::{Callback, CallbackChange, CallbackInfo},
    window_state::WindowCreateOptions,
};
use rust_fontconfig::FcFontCache;

const PIN_DARK: OptionWindowTheme = OptionWindowTheme::Some(WindowTheme::DarkMode);
const FOLLOW: OptionWindowTheme = OptionWindowTheme::None;

/// The mode and the theme are APP-wide (process globals), so the tests of this binary take
/// turns, and each starts from "follow the desktop" in the default theme.
static SERIAL: Mutex<()> = Mutex::new(());

fn fresh_app() -> MutexGuard<'static, ()> {
    let guard = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    azul_layout::window::set_app_mode(FOLLOW);
    azul_core::app_theme::set_app_theme(azul_css::dynamic_selector::DEFAULT_APP_THEME);
    guard
}

/// `AZ_THEME` pins light / dark over the app's mode; under it there is nothing to test.
fn env_pinned() -> bool {
    azul_css::dynamic_selector::mode_pinned_by_env().is_some()
}

#[derive(Clone)]
struct Model {
    /// `layout()` calls so far; frame 0 builds an empty body, later frames mount the probe.
    frame: Arc<AtomicU32>,
    /// What the last `layout()` read through `LayoutCallbackInfo::get_mode`: 0 nothing,
    /// 1 light, 2 dark.
    mode_seen: Arc<AtomicU32>,
    /// What the last `layout()` read through `LayoutCallbackInfo::get_theme`.
    theme_seen: Arc<Mutex<String>>,
    /// What the probe's callback read: `CallbackInfo::get_mode` (the choice) and
    /// `CallbackInfo::get_resolved_mode` (what the window shows).
    callback_seen: Arc<Mutex<Option<(OptionWindowTheme, WindowTheme)>>>,
}

impl Model {
    fn new() -> Self {
        Self {
            frame: Arc::new(AtomicU32::new(0)),
            mode_seen: Arc::new(AtomicU32::new(0)),
            theme_seen: Arc::new(Mutex::new(String::new())),
            callback_seen: Arc::new(Mutex::new(None)),
        }
    }

    fn mode_seen(&self) -> u32 {
        self.mode_seen.load(Ordering::SeqCst)
    }

    fn theme_seen(&self) -> String {
        self.theme_seen.lock().expect("theme_seen").clone()
    }

    fn callback_seen(&self) -> Option<(OptionWindowTheme, WindowTheme)> {
        *self.callback_seen.lock().expect("callback_seen")
    }
}

/// A callback reads the mode (choice and result), then pins the app dark.
extern "C" fn pin_dark_on_mount(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let choice: OptionWindowTheme = info.get_mode();
    let shown: WindowTheme = info.get_resolved_mode();
    if let Some(model) = data.downcast_ref::<Model>() {
        *model.callback_seen.lock().expect("callback_seen") = Some((choice, shown));
    }
    info.set_mode(PIN_DARK);
    Update::DoNothing
}

/// A `layout()` that reads both: the mode (a `WindowTheme`) and the app theme (a name).
extern "C" fn mode_and_theme_layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let mode: WindowTheme = info.get_mode();
    let theme: AzString = info.get_theme();
    let model = match data.downcast_ref::<Model>() {
        Some(m) => m.clone(),
        None => return Dom::create_body(),
    };
    model.mode_seen.store(
        match mode {
            WindowTheme::LightMode => 1,
            WindowTheme::DarkMode => 2,
        },
        Ordering::SeqCst,
    );
    *model.theme_seen.lock().expect("theme_seen") = theme.as_str().to_string();

    // Frame 0 is empty, so frame 1 MOUNTS the probe and its `AfterMount` callback runs.
    if model.frame.fetch_add(1, Ordering::SeqCst) == 0 {
        return Dom::create_body();
    }
    let on_mount = Callback {
        cb: pin_dark_on_mount,
        ctx: OptionRefAny::None,
    }
    .to_core();
    let mut probe = NodeData::create_text_do_not_use_without_block_level_wrapper("mode probe");
    probe.add_callback(
        EventFilter::Component(ComponentEventFilter::AfterMount),
        RefAny::new(model),
        on_mount,
    );
    Dom::create_body().with_child(Dom::create_from_data(probe))
}

fn make_window(model: Model) -> HeadlessWindow {
    let mut config = AppConfig::default().with_mode(FOLLOW);
    // Hermetic: a light desktop whatever the host is in.
    config.system_style = azul_css::system::defaults::macos_modern_light();

    let mut options = WindowCreateOptions::default();
    options.window_state.layout_callback = LayoutCallback {
        cb: mode_and_theme_layout,
        ctx: OptionRefAny::None,
    };

    HeadlessWindow::new(
        options,
        Arc::new(RefCell::new(RefAny::new(model))),
        SharedUndoManager::new(),
        config,
        SharedIconProvider::from_handle(IconProviderHandle::default()),
        Arc::new(FcFontCache::default()),
        None,
    )
    .expect("HeadlessWindow construction must succeed")
}

fn shown_mode(window: &HeadlessWindow) -> WindowTheme {
    window.common.current_window_state().theme
}

/// The names ARE the contract: each binding generates its method names from them.
#[test]
fn the_signatures_say_which_call_is_the_mode_and_which_the_theme() {
    // In `layout()`: the mode is a light / dark, the theme a name.
    let _: fn(&LayoutCallbackInfo) -> WindowTheme = LayoutCallbackInfo::get_mode;
    let _: fn(&LayoutCallbackInfo) -> AzString = LayoutCallbackInfo::get_theme;
    // In an event callback: the same split.
    let _: fn(&mut CallbackInfo, OptionWindowTheme) = CallbackInfo::set_mode;
    let _: fn(&CallbackInfo) -> OptionWindowTheme = CallbackInfo::get_mode;
    let _: fn(&CallbackInfo) -> WindowTheme = CallbackInfo::get_resolved_mode;
    let _: fn(&mut CallbackInfo, AzString) = CallbackInfo::set_theme;
    let _: fn(&CallbackInfo) -> AzString = CallbackInfo::get_theme;
    // At startup.
    let _: fn(AppConfig, OptionWindowTheme) -> AppConfig = AppConfig::with_mode;
    let _: fn(&mut AppConfig, OptionWindowTheme) = AppConfig::set_mode;
    let _: fn(AppConfig, AzString) -> AppConfig = AppConfig::with_theme;
    // The queued change carries a mode.
    let change = CallbackChange::SetMode { mode: PIN_DARK };
    assert!(matches!(change, CallbackChange::SetMode { mode } if mode == PIN_DARK));
}

#[test]
fn app_config_with_mode_sets_the_mode_and_leaves_the_theme_alone() {
    let config = AppConfig::default().with_mode(PIN_DARK);
    assert_eq!(config.mode, PIN_DARK);
    assert_eq!(
        config.theme.as_str(),
        azul_css::dynamic_selector::DEFAULT_APP_THEME,
        "a mode is not a theme: with_mode must not touch the app theme"
    );

    let mut config = AppConfig::default();
    assert_eq!(config.mode, FOLLOW, "the default mode follows the desktop");
    config.set_mode(PIN_DARK);
    assert_eq!(config.mode, PIN_DARK);
}

/// A binding sees the variants as numbers: the renames keep every value. `ModeChange` is the
/// old light / dark `ThemeChange` (3); `ThemeChange` is the old `AppThemeChange` (6), still
/// appended after `Other`.
#[test]
fn the_relayout_reasons_keep_their_values_across_the_rename() {
    assert_eq!(RelayoutReason::Initial as u32, 0);
    assert_eq!(RelayoutReason::RefreshDom as u32, 1);
    assert_eq!(RelayoutReason::Resize as u32, 2);
    assert_eq!(RelayoutReason::ModeChange as u32, 3);
    assert_eq!(RelayoutReason::RouteChange as u32, 4);
    assert_eq!(RelayoutReason::Other as u32, 5);
    assert_eq!(RelayoutReason::ThemeChange as u32, 6);
    assert!(
        !RelayoutReason::ModeChange.animates_moves() && !RelayoutReason::ThemeChange.animates_moves(),
        "an environment change reflows in place, the mode's and the theme's alike"
    );
}

#[test]
fn layout_reads_the_mode_and_the_theme_and_a_callback_sets_the_mode() {
    let _app = fresh_app();
    if env_pinned() {
        return;
    }
    let model = Model::new();
    let mut window = make_window(model.clone());

    window.regenerate_layout().expect("frame 0");
    assert_eq!(model.mode_seen(), 1, "layout() read the light mode through get_mode");
    assert_eq!(
        model.theme_seen(),
        azul_css::dynamic_selector::DEFAULT_APP_THEME,
        "layout() read the app theme's NAME through get_theme"
    );

    // Frame 1 mounts the probe; its callback reads the mode and pins dark.
    window.regenerate_layout().expect("frame 1");
    assert_eq!(
        model.callback_seen(),
        Some((FOLLOW, WindowTheme::LightMode)),
        "the callback read the choice (get_mode: follow the desktop) and what it gives \
         (get_resolved_mode: light)"
    );
    assert_eq!(
        shown_mode(&window),
        WindowTheme::DarkMode,
        "set_mode pinned the window dark"
    );
    assert_eq!(
        window
            .common
            .layout_window
            .as_ref()
            .expect("a layout window")
            .mode,
        PIN_DARK,
        "and the window holds the app's mode"
    );

    // layout() read the mode, so the switch rebuilds it; the rebuilt DOM sees dark, same theme.
    window.regenerate_layout().expect("frame 2");
    assert_eq!(model.mode_seen(), 2, "the rebuilt layout() reads the dark mode");
    assert_eq!(
        model.theme_seen(),
        azul_css::dynamic_selector::DEFAULT_APP_THEME,
        "a mode switch leaves the app theme alone"
    );
}
