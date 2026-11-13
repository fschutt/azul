//! The app's light / dark MODE (`CallbackInfo::set_mode`) through the real
//! shell pipeline (`HeadlessWindow`).
//!
//! What the layout-crate tests (`layout/tests/app_color_scheme_override.rs`)
//! cannot see: that the switch reaches the window's light / dark at once, that
//! it is a RESTYLE of the retained DOM (the app's `layout()` is not run again)
//! unless `layout()` read the mode, that the desktop's own light / dark is
//! remembered while the app pins one, and that windows opened afterwards start
//! in the app's mode.
//!
//! The desktop here is the macOS light preset whatever the host runs, and
//! `HeadlessWindow::set_system_theme` is the desktop flipping.

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
    callbacks::{LayoutCallback, LayoutCallbackInfo, LayoutCallbackType},
    dom::{Dom, DomId},
    events::ProcessEventResult,
    icon::{IconProviderHandle, SharedIconProvider},
    refany::{OptionRefAny, RefAny},
    resources::AppConfig,
    window::{OptionDarkLightMode, DarkLightMode},
};
use azul_css::props::basic::color::ColorU;
use azul_layout::{
    callbacks::CallbackChange, solver3::display_list::DisplayListItem,
    window_state::WindowCreateOptions,
};
use rust_fontconfig::FcFontCache;

const PIN_LIGHT: OptionDarkLightMode = OptionDarkLightMode::Some(DarkLightMode::Light);
const PIN_DARK: OptionDarkLightMode = OptionDarkLightMode::Some(DarkLightMode::Dark);
const FOLLOW: OptionDarkLightMode = OptionDarkLightMode::None;

/// The mode is APP-wide (every window of the App shares it), so the tests
/// of this binary take turns, and each starts from "follow the desktop".
static SERIAL: Mutex<()> = Mutex::new(());

fn fresh_app() -> MutexGuard<'static, ()> {
    let guard = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    azul_layout::window::set_app_mode(FOLLOW);
    guard
}

/// `AZ_MODE` outranks the app's mode; under it there is nothing to test.
fn env_pinned() -> bool {
    azul_css::dynamic_selector::mode_pinned_by_env().is_some()
}

#[derive(Clone)]
struct Model {
    layout_calls: Arc<AtomicU32>,
    /// What the last `layout()` read through `get_mode`: 0 nothing, 1
    /// light, 2 dark.
    seen: Arc<AtomicU32>,
}

impl Model {
    fn new() -> Self {
        Self {
            layout_calls: Arc::new(AtomicU32::new(0)),
            seen: Arc::new(AtomicU32::new(0)),
        }
    }

    fn calls(&self) -> u32 {
        self.layout_calls.load(Ordering::SeqCst)
    }

    fn seen(&self) -> u32 {
        self.seen.load(Ordering::SeqCst)
    }
}

/// A `layout()` that never looks at the mode: its DOM is the same in both,
/// every colour comes from the cascade.
extern "C" fn mode_blind_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    if let Some(model) = data.downcast_ref::<Model>() {
        model.layout_calls.fetch_add(1, Ordering::SeqCst);
    }
    Dom::create_body().with_child(Dom::create_p_with_text("ink"))
}

/// A `layout()` whose DOM depends on the mode: it asks (`get_mode`).
extern "C" fn mode_reading_layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let mode = info.get_mode();
    if let Some(model) = data.downcast_ref::<Model>() {
        model.layout_calls.fetch_add(1, Ordering::SeqCst);
        model.seen.store(
            match mode {
                DarkLightMode::Light => 1,
                DarkLightMode::Dark => 2,
            },
            Ordering::SeqCst,
        );
    }
    Dom::create_body().with_child(Dom::create_p_with_text(match mode {
        DarkLightMode::Light => "light",
        DarkLightMode::Dark => "dark",
    }))
}

fn make_window(model: Model, layout: LayoutCallbackType) -> HeadlessWindow {
    make_window_from(model, window_options(layout))
}

/// Hermetic: a light desktop whatever the host is in.
fn light_desktop() -> azul_css::system::SystemStyle {
    azul_css::system::defaults::macos_modern_light()
}

/// What `make_window` opens: the layout callback and nothing else.
fn window_options(layout: LayoutCallbackType) -> WindowCreateOptions {
    let mut options = WindowCreateOptions::default();
    options.window_state.layout_callback = LayoutCallback {
        cb: layout,
        ctx: OptionRefAny::None,
    };
    options
}

/// Opens `options` on the light desktop.
fn make_window_from(model: Model, options: WindowCreateOptions) -> HeadlessWindow {
    let mut config = AppConfig::default();
    config.system_style = light_desktop();

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

fn mode_of(window: &HeadlessWindow) -> DarkLightMode {
    window.common.current_window_state().mode
}

fn set_mode(window: &mut HeadlessWindow, mode: OptionDarkLightMode) -> ProcessEventResult {
    window.apply_user_change(&CallbackChange::SetMode { mode })
}

/// `service_frame`'s routing, without the render: a regenerate-tier result
/// re-invokes the app's `layout()`, an incremental one re-lays the existing
/// tree.
fn honor(window: &mut HeadlessWindow, tier: ProcessEventResult) {
    if tier >= ProcessEventResult::ShouldRegenerateDomCurrentWindow {
        window
            .regenerate_layout()
            .expect("regenerate_layout after a rebuild request");
    } else if tier == ProcessEventResult::ShouldIncrementalRelayout {
        window.relayout_only().expect("relayout_only after a restyle");
    }
}

fn painted_text(window: &HeadlessWindow) -> Vec<ColorU> {
    window
        .common
        .layout_window
        .as_ref()
        .expect("a layout window")
        .layout_results
        .get(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { color, .. } => Some(*color),
            _ => None,
        })
        .collect()
}

fn is_light(c: &ColorU) -> bool {
    c.r >= 0xd0 && c.g >= 0xd0 && c.b >= 0xd0
}

#[test]
fn a_mode_switch_restyles_without_running_layout_again() {
    let _app = fresh_app();
    if env_pinned() {
        return;
    }
    let model = Model::new();
    let mut window = make_window(model.clone(), mode_blind_layout);
    window.regenerate_layout().expect("first layout");
    assert_eq!(mode_of(&window), DarkLightMode::Light, "premise: a light desktop");
    let calls = model.calls();

    let result = set_mode(&mut window, PIN_DARK);
    assert_eq!(
        mode_of(&window),
        DarkLightMode::Dark,
        "the window takes the app's dark pin at once"
    );
    assert_eq!(
        result,
        ProcessEventResult::ShouldIncrementalRelayout,
        "a layout() that never read the mode is RE-STYLED, not rebuilt"
    );
    honor(&mut window, result);

    assert_eq!(
        model.calls(),
        calls,
        "the mode switch must not run the app's layout() again"
    );
    let text = painted_text(&window);
    assert!(!text.is_empty(), "the text run must be painted");
    assert!(
        text.iter().all(is_light),
        "the retained DOM re-styled dark paints near-white text, got {text:?}"
    );
}

#[test]
fn a_layout_that_read_the_mode_is_rebuilt_and_sees_the_pin() {
    let _app = fresh_app();
    if env_pinned() {
        return;
    }
    let model = Model::new();
    let mut window = make_window(model.clone(), mode_reading_layout);
    window.regenerate_layout().expect("first layout");
    assert_eq!(model.seen(), 1, "premise: layout() saw light");
    let calls = model.calls();

    let result = set_mode(&mut window, PIN_DARK);
    assert_eq!(
        result,
        ProcessEventResult::ShouldRegenerateDomCurrentWindow,
        "layout() read the mode (get_mode), so its DOM depends on it"
    );
    honor(&mut window, result);
    assert!(model.calls() > calls, "layout() ran again");
    assert_eq!(model.seen(), 2, "and the rebuilt layout() saw the dark pin");
}

#[test]
fn a_desktop_flip_while_pinned_does_not_flip_the_window() {
    let _app = fresh_app();
    if env_pinned() {
        return;
    }
    let model = Model::new();
    let mut window = make_window(model, mode_blind_layout);
    window.regenerate_layout().expect("first layout");

    let result = set_mode(&mut window, PIN_LIGHT);
    assert_eq!(
        result,
        ProcessEventResult::DoNothing,
        "pinning the mode the window already shows costs nothing"
    );

    assert!(
        !window.set_system_theme(DarkLightMode::Dark),
        "the desktop went dark: the pinned-light window's mode must not move"
    );
    assert_eq!(mode_of(&window), DarkLightMode::Light);
}

#[test]
fn switching_back_to_system_follows_the_desktop_immediately() {
    let _app = fresh_app();
    if env_pinned() {
        return;
    }
    let model = Model::new();
    let mut window = make_window(model, mode_blind_layout);
    window.regenerate_layout().expect("first layout");

    let _ = set_mode(&mut window, PIN_LIGHT);
    // The desktop goes dark while the app is pinned light: remembered, not shown.
    let _ = window.set_system_theme(DarkLightMode::Dark);
    assert_eq!(mode_of(&window), DarkLightMode::Light, "premise: still pinned");

    let result = set_mode(&mut window, FOLLOW);
    assert_eq!(
        mode_of(&window),
        DarkLightMode::Dark,
        "back on System the window takes the desktop's CURRENT mode at once - no desktop \
         event is needed"
    );
    assert_eq!(result, ProcessEventResult::ShouldIncrementalRelayout);
}

#[test]
fn a_window_opened_after_the_switch_starts_in_the_apps_mode() {
    let _app = fresh_app();
    if env_pinned() {
        return;
    }
    let mut first = make_window(Model::new(), mode_blind_layout);
    first.regenerate_layout().expect("first layout");
    let _ = set_mode(&mut first, PIN_DARK);

    let second = make_window(Model::new(), mode_blind_layout);
    assert_eq!(
        mode_of(&second),
        DarkLightMode::Dark,
        "a window created after the app pinned dark starts dark on the light desktop"
    );
    assert_eq!(
        second
            .common
            .layout_window
            .as_ref()
            .expect("a layout window")
            .mode,
        PIN_DARK,
        "and holds the app's mode"
    );
}

/// An app that switches the mode through `modify_window_state` gets it. The
/// handler compared `theme`, asked for a `ModeChange` rebuild, and never
/// WROTE the theme: the rebuilt `layout()` still saw the old mode.
#[test]
fn modify_window_state_with_a_new_mode_switches_the_window() {
    let _app = fresh_app();
    if env_pinned() {
        return;
    }
    let model = Model::new();
    let mut window = make_window(model.clone(), mode_reading_layout);
    window.regenerate_layout().expect("first layout");
    assert_eq!(mode_of(&window), DarkLightMode::Light, "premise: a light desktop");

    let mut state = window.get_current_window_state().clone();
    state.mode = DarkLightMode::Dark;
    let result = window.apply_user_change(&CallbackChange::ModifyWindowState { state });
    assert_eq!(
        mode_of(&window),
        DarkLightMode::Dark,
        "the pushed light / dark is the window's mode now"
    );
    honor(&mut window, result);
    assert_eq!(model.seen(), 2, "and the rebuilt layout() sees it");
}

/// A light / dark pushed through `modify_window_state` is the WINDOW's own
/// choice, so the app's mode still outranks it (AZ_MODE > app > window >
/// desktop).
#[test]
fn modify_window_state_does_not_override_the_apps_pin() {
    let _app = fresh_app();
    if env_pinned() {
        return;
    }
    let mut window = make_window(Model::new(), mode_blind_layout);
    window.regenerate_layout().expect("first layout");
    let _ = set_mode(&mut window, PIN_LIGHT);

    let mut state = window.get_current_window_state().clone();
    state.mode = DarkLightMode::Dark;
    let _ = window.apply_user_change(&CallbackChange::ModifyWindowState { state });
    assert_eq!(mode_of(&window), DarkLightMode::Light);
}

// ---------------------------------------------------------------------------
// The window's clear colour follows the MODE the window shows (W4 item 6.3)
// ---------------------------------------------------------------------------

/// A window as a DESKTOP shell opens it: the background is seeded from the
/// scheme at creation (`resolve_initial_background_color`, which every shell
/// calls before its `CommonWindowState`), and the CPU canvas follows the
/// system background (every shell's `CpuBackend`; the headless one keeps a
/// fixed canvas so offscreen output does not change with the machine).
fn make_desktop_window_from(model: Model, mut options: WindowCreateOptions) -> HeadlessWindow {
    azul::desktop::shell2::common::resolve_initial_background_color(
        &mut options,
        &light_desktop(),
    );
    let mut window = make_window_from(model, options);
    window.cpu_backend.follow_system_background = true;
    window
}

fn make_desktop_window(model: Model, layout: LayoutCallbackType) -> HeadlessWindow {
    make_desktop_window_from(model, window_options(layout))
}

/// Paints one frame on the CPU backend - the canvas macOS, X11 and Wayland
/// share - and answers the colour it cleared the window to: what shows
/// wherever the app's body does not paint.
fn paint_and_read_clear_color(window: &mut HeadlessWindow) -> [u8; 4] {
    let (width, height, dpi) = {
        let ws = window.common.current_window_state();
        (
            ws.size.dimensions.width,
            ws.size.dimensions.height,
            ws.size.dpi as f32 / 96.0,
        )
    };
    let lw = window
        .common
        .layout_window
        .as_ref()
        .expect("a layout window");
    let _ = window.cpu_backend.render_frame(
        lw,
        &window.common.renderer_resources,
        width,
        height,
        dpi,
    );
    window
        .cpu_backend
        .last_clear_color
        .expect("a painted frame records its clear colour")
}

fn is_dark_rgba(c: [u8; 4]) -> bool {
    u32::from(c[0]) + u32::from(c[1]) + u32::from(c[2]) < 3 * 128
}

fn rgba(c: ColorU) -> [u8; 4] {
    [c.r, c.g, c.b, c.a]
}

/// The clear colour follows the scheme the window SHOWS. The background a
/// shell seeds at creation came from the DESKTOP's palette and nothing moved
/// it, so a dark pin on a light desktop painted dark widgets on a light
/// canvas wherever the body did not reach.
#[test]
fn a_dark_pin_clears_the_window_dark_on_a_light_desktop() {
    let _app = fresh_app();
    if env_pinned() {
        return;
    }
    let mut window = make_desktop_window(Model::new(), mode_blind_layout);
    window.regenerate_layout().expect("first layout");
    assert!(
        !is_dark_rgba(paint_and_read_clear_color(&mut window)),
        "premise: a light desktop clears light"
    );

    let result = set_mode(&mut window, PIN_DARK);
    honor(&mut window, result);
    let clear = paint_and_read_clear_color(&mut window);
    assert!(is_dark_rgba(clear), "the dark pin clears dark, got {clear:?}");
}

/// A window opened while the app is pinned dark starts on a dark canvas: the
/// creation-time background was chosen from the DESKTOP's theme.
#[test]
fn a_window_opened_under_a_dark_pin_starts_on_a_dark_canvas() {
    let _app = fresh_app();
    if env_pinned() {
        return;
    }
    azul_layout::window::set_app_mode(PIN_DARK);
    let mut window = make_desktop_window(Model::new(), mode_blind_layout);
    window.regenerate_layout().expect("first layout");
    let clear = paint_and_read_clear_color(&mut window);
    assert!(is_dark_rgba(clear), "a dark-pinned window clears dark, got {clear:?}");
}

/// A canvas nobody seeded takes the system window background - of the mode
/// the window SHOWS, not of the desktop's palette.
#[test]
fn an_unseeded_canvas_takes_the_system_background_of_the_mode_the_window_shows() {
    let _app = fresh_app();
    if env_pinned() {
        return;
    }
    let mut window = make_window(Model::new(), mode_blind_layout);
    window.cpu_backend.follow_system_background = true;
    window.regenerate_layout().expect("first layout");
    assert!(
        !is_dark_rgba(paint_and_read_clear_color(&mut window)),
        "premise: a light desktop clears light"
    );

    let result = set_mode(&mut window, PIN_DARK);
    honor(&mut window, result);
    let clear = paint_and_read_clear_color(&mut window);
    assert!(is_dark_rgba(clear), "the dark pin clears dark, got {clear:?}");
}

/// The app's own per-mode background (`WindowCreateOptions::
/// background_color_dark`) is the canvas of the dark mode, whoever switched
/// to it.
#[test]
fn a_per_mode_background_follows_the_pin() {
    let _app = fresh_app();
    if env_pinned() {
        return;
    }
    let navy = ColorU::new_rgb(0x10, 0x18, 0x40);
    let mut options = window_options(mode_blind_layout);
    options.background_color_dark = azul_css::props::basic::color::OptionColorU::Some(navy);
    let mut window = make_desktop_window_from(Model::new(), options);
    window.regenerate_layout().expect("first layout");
    assert!(
        !is_dark_rgba(paint_and_read_clear_color(&mut window)),
        "premise: the light mode has no background of the app's, the desktop's is light"
    );

    let result = set_mode(&mut window, PIN_DARK);
    honor(&mut window, result);
    assert_eq!(
        paint_and_read_clear_color(&mut window),
        rgba(navy),
        "the dark mode clears to the app's dark background"
    );
}

/// A background the APP set is the app's decision: a mode change moves only
/// a background the scheme derived.
#[test]
fn a_background_the_app_set_survives_a_mode_change() {
    let _app = fresh_app();
    if env_pinned() {
        return;
    }
    let brand = ColorU::new_rgb(0xc0, 0x30, 0x20);
    let mut options = window_options(mode_blind_layout);
    options.window_state.background_color =
        azul_css::props::basic::color::OptionColorU::Some(brand);
    let mut window = make_desktop_window_from(Model::new(), options);
    window.regenerate_layout().expect("first layout");
    assert_eq!(paint_and_read_clear_color(&mut window), rgba(brand), "premise");

    let result = set_mode(&mut window, PIN_DARK);
    honor(&mut window, result);
    assert_eq!(
        paint_and_read_clear_color(&mut window),
        rgba(brand),
        "the dark pin keeps the app's own background"
    );

    let result = set_mode(&mut window, FOLLOW);
    honor(&mut window, result);
    assert_eq!(
        paint_and_read_clear_color(&mut window),
        rgba(brand),
        "and so does following the desktop again"
    );
}

/// Pinning and un-pinning is a round trip: back on System the canvas is the
/// desktop's own background again, exactly.
#[test]
fn switching_back_to_system_returns_the_desktop_background() {
    let _app = fresh_app();
    if env_pinned() {
        return;
    }
    let mut window = make_desktop_window(Model::new(), mode_blind_layout);
    window.regenerate_layout().expect("first layout");
    let desktop = paint_and_read_clear_color(&mut window);

    let result = set_mode(&mut window, PIN_DARK);
    honor(&mut window, result);
    let result = set_mode(&mut window, FOLLOW);
    honor(&mut window, result);
    assert_eq!(paint_and_read_clear_color(&mut window), desktop);
}

/// A theme pushed through `modify_window_state` moves the scheme-derived
/// canvas too - the app's state carries the background it read, which is the
/// one the scheme seeded.
#[test]
fn modify_window_state_with_a_new_theme_moves_the_seeded_background() {
    let _app = fresh_app();
    if env_pinned() {
        return;
    }
    let mut window = make_desktop_window(Model::new(), mode_blind_layout);
    window.regenerate_layout().expect("first layout");
    assert!(!is_dark_rgba(paint_and_read_clear_color(&mut window)), "premise");

    let mut state = window.get_current_window_state().clone();
    state.mode = DarkLightMode::Dark;
    let result = window.apply_user_change(&CallbackChange::ModifyWindowState { state });
    honor(&mut window, result);
    let clear = paint_and_read_clear_color(&mut window);
    assert!(is_dark_rgba(clear), "the pushed dark theme clears dark, got {clear:?}");
}

// ---------------------------------------------------------------------------
// The native chrome follows the MODE too (W4 item 6.4)
// ---------------------------------------------------------------------------

/// Under a pin the native titlebar (macOS: `NSWindow.appearance`) is forced
/// into the window's mode; while the window follows the desktop it inherits
/// the desktop's. Nothing forced it, so a dark-pinned window kept a light
/// titlebar on a light desktop.
#[test]
fn the_native_chrome_is_forced_into_a_pinned_mode_and_inherits_otherwise() {
    let _app = fresh_app();
    if env_pinned() {
        return;
    }
    let mut window = make_window(Model::new(), mode_blind_layout);
    window.regenerate_layout().expect("first layout");
    assert_eq!(
        window.common.native_chrome_mode(),
        None,
        "following the desktop, the chrome inherits it"
    );

    let result = set_mode(&mut window, PIN_DARK);
    honor(&mut window, result);
    assert_eq!(
        window.common.native_chrome_mode(),
        Some(DarkLightMode::Dark),
        "a dark pin on a light desktop forces a dark titlebar"
    );

    let result = set_mode(&mut window, FOLLOW);
    honor(&mut window, result);
    assert_eq!(
        window.common.native_chrome_mode(),
        None,
        "back on System the chrome inherits the desktop again"
    );
}

/// A pin that agrees with the desktop still forces the chrome: the desktop
/// can flip under it later, and the window - titlebar included - must stay.
#[test]
fn a_pin_that_matches_the_desktop_still_holds_the_chrome_when_the_desktop_flips() {
    let _app = fresh_app();
    if env_pinned() {
        return;
    }
    let mut window = make_window(Model::new(), mode_blind_layout);
    window.regenerate_layout().expect("first layout");

    let _ = set_mode(&mut window, PIN_LIGHT);
    assert_eq!(
        window.common.native_chrome_mode(),
        Some(DarkLightMode::Light),
        "pinned: forced, even where the desktop agrees"
    );

    let _ = window.set_system_theme(DarkLightMode::Dark);
    assert_eq!(
        window.common.native_chrome_mode(),
        Some(DarkLightMode::Light),
        "the desktop went dark under a light pin: the chrome stays light"
    );
}
