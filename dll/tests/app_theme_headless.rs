//! The app theme (`CallbackInfo::set_theme`) through the real shell pipeline (`HeadlessWindow`).
//!
//! A theme is not a colour scheme. The colour scheme only repaints (`ThemeChange`, a restyle
//! unless `layout()` read it); a THEME may change a widget's DOM STRUCTURE - flora wraps nodes
//! flat does not - so a switch always RECREATES the DOM: `layout()` runs again, tagged
//! `RelayoutReason::AppThemeChange`, and builds for the new theme (`get_theme_name` in
//! `layout()`, `azul_core::app_theme::current_theme` in a widget's `dom()`).
//!
//! The choice is APP-wide: every window rebuilds under it, and a window opened later starts in
//! it. Headless has no window registry, so the fan-out's rebuild REQUEST cannot reach the other
//! window here; what is pinned is that the other window's next pass rebuilds under the new
//! theme and says why.

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
    callbacks::{LayoutCallback, LayoutCallbackInfo, LayoutCallbackType, RelayoutReason},
    dom::{Dom, DomId},
    events::ProcessEventResult,
    icon::{IconProviderHandle, SharedIconProvider},
    refany::{OptionRefAny, RefAny},
    resources::AppConfig,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::color::ColorU,
        layout::{LayoutHeight, LayoutWidth},
        property::CssProperty,
        style::{StyleBackgroundContent, StyleBackgroundContentVec},
    },
    AzString,
};
use azul_layout::{
    callbacks::CallbackChange, solver3::display_list::DisplayListItem,
    window_state::WindowCreateOptions,
};
use rust_fontconfig::FcFontCache;

const FLAT: ColorU = ColorU::rgb(0x00, 0x00, 0xff);
const FLORA: ColorU = ColorU::rgb(0xff, 0x00, 0x00);

const BG_FLAT: CssProperty = CssProperty::const_background_content(
    StyleBackgroundContentVec::from_const_slice(&[StyleBackgroundContent::Color(FLAT)]),
);
const BG_FLORA: CssProperty = CssProperty::const_background_content(
    StyleBackgroundContentVec::from_const_slice(&[StyleBackgroundContent::Color(FLORA)]),
);

/// A migrated widget's box: both themes' blocks, the matcher picks.
static BOX_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(40))),
    CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(20))),
    CssPropertyWithConditions::with_single_condition(BG_FLAT, azul_css::theme_conditions!("flat")),
    CssPropertyWithConditions::with_single_condition(
        BG_FLORA,
        azul_css::theme_conditions!("flora"),
    ),
];

/// The choice is APP-wide (a process-global, like the colour scheme's), so the tests of this
/// binary take turns, and each starts from the default theme.
static SERIAL: Mutex<()> = Mutex::new(());

fn fresh_app() -> MutexGuard<'static, ()> {
    let guard = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    azul_core::app_theme::set_app_theme("flat");
    guard
}

/// What one `layout()` call saw.
#[derive(Debug, Clone, PartialEq)]
struct Seen {
    /// `LayoutCallbackInfo::get_theme_name`.
    theme: String,
    /// `azul_core::app_theme::current_theme` - what a widget's `dom()` reads.
    widget_theme: String,
    reason: RelayoutReason,
}

#[derive(Clone)]
struct Model {
    layout_calls: Arc<AtomicU32>,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Model {
    fn new() -> Self {
        Self {
            layout_calls: Arc::new(AtomicU32::new(0)),
            seen: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn calls(&self) -> u32 {
        self.layout_calls.load(Ordering::SeqCst)
    }

    fn last(&self) -> Seen {
        self.seen
            .lock()
            .expect("seen")
            .last()
            .cloned()
            .expect("layout() ran at least once")
    }
}

/// A `layout()` whose STRUCTURE depends on the theme, the way a flora widget wraps its face:
/// under flora the box sits in one more div.
extern "C" fn theme_structured_layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let theme = info.get_theme_name();
    if let Some(model) = data.downcast_ref::<Model>() {
        model.layout_calls.fetch_add(1, Ordering::SeqCst);
        model.seen.lock().expect("seen").push(Seen {
            theme: theme.as_str().to_string(),
            widget_theme: azul_core::app_theme::current_theme().as_str().to_string(),
            reason: info.relayout_reason(),
        });
    }
    let face = Dom::create_div()
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(BOX_STYLE));
    let widget = if theme.as_str() == "flora" {
        Dom::create_div().with_child(face)
    } else {
        face
    };
    Dom::create_body().with_child(widget)
}

fn make_window(model: Model, layout: LayoutCallbackType) -> HeadlessWindow {
    let mut config = AppConfig::default();
    // Hermetic: a light desktop whatever the host is in.
    config.system_style = azul_css::system::defaults::macos_modern_light();

    let mut options = WindowCreateOptions::default();
    options.window_state.layout_callback = LayoutCallback {
        cb: layout,
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

fn set_theme(window: &mut HeadlessWindow, theme: &str) -> ProcessEventResult {
    window.apply_user_change(&CallbackChange::SetTheme {
        theme: AzString::from(theme.to_string()),
    })
}

/// `service_frame`'s routing for the regenerate tier.
fn honor(window: &mut HeadlessWindow, tier: ProcessEventResult) {
    if tier >= ProcessEventResult::ShouldRegenerateDomCurrentWindow {
        window
            .regenerate_layout()
            .expect("regenerate_layout after a rebuild request");
    }
}

fn window_theme(window: &HeadlessWindow) -> String {
    window
        .common
        .layout_window
        .as_ref()
        .expect("a layout window")
        .app_theme
        .as_str()
        .to_string()
}

fn node_count(window: &HeadlessWindow) -> usize {
    window
        .common
        .layout_window
        .as_ref()
        .expect("a layout window")
        .layout_results
        .get(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .styled_dom
        .node_data
        .as_container()
        .len()
}

fn box_fill(window: &HeadlessWindow) -> Option<ColorU> {
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
        .find_map(|item| match item {
            DisplayListItem::Rect { bounds, color, .. }
                if (bounds.0.size.width - 40.0).abs() < 0.01
                    && (bounds.0.size.height - 20.0).abs() < 0.01 =>
            {
                Some(*color)
            }
            _ => None,
        })
}

#[test]
fn the_first_layout_builds_in_the_default_theme_flat() {
    let _app = fresh_app();
    let model = Model::new();
    let mut window = make_window(model.clone(), theme_structured_layout);
    window.regenerate_layout().expect("first layout");

    let seen = model.last();
    assert_eq!(seen.theme, "flat");
    assert_eq!(seen.widget_theme, "flat", "a widget's dom() builds for the same theme");
    assert_eq!(window_theme(&window), "flat");
    assert_eq!(box_fill(&window), Some(FLAT));
}

#[test]
fn set_theme_recreates_the_dom_under_the_new_theme() {
    let _app = fresh_app();
    let model = Model::new();
    let mut window = make_window(model.clone(), theme_structured_layout);
    window.regenerate_layout().expect("first layout");
    let calls = model.calls();
    let flat_nodes = node_count(&window);

    let result = set_theme(&mut window, "flora");
    assert_eq!(
        result,
        ProcessEventResult::ShouldRegenerateDomCurrentWindow,
        "a theme may change the DOM's structure: a switch is a rebuild, never a restyle"
    );
    assert_eq!(
        window.pending_relayout_reason(),
        RelayoutReason::AppThemeChange,
        "and the rebuild says why"
    );
    honor(&mut window, result);

    assert!(model.calls() > calls, "layout() ran again");
    let seen = model.last();
    assert_eq!(
        seen,
        Seen {
            theme: "flora".to_string(),
            widget_theme: "flora".to_string(),
            reason: RelayoutReason::AppThemeChange,
        }
    );
    assert_eq!(window_theme(&window), "flora");
    assert_eq!(
        node_count(&window),
        flat_nodes + 1,
        "the flora structure (one wrapper more) is what the window now holds"
    );
    assert_eq!(box_fill(&window), Some(FLORA), "and it paints the flora block");

    // And back: flat's structure and flat's block again.
    let result = set_theme(&mut window, "flat");
    assert_eq!(result, ProcessEventResult::ShouldRegenerateDomCurrentWindow);
    honor(&mut window, result);
    assert_eq!(model.last().theme, "flat");
    assert_eq!(node_count(&window), flat_nodes);
    assert_eq!(box_fill(&window), Some(FLAT));
}

#[test]
fn setting_the_theme_the_window_already_shows_costs_nothing() {
    let _app = fresh_app();
    let model = Model::new();
    let mut window = make_window(model.clone(), theme_structured_layout);
    window.regenerate_layout().expect("first layout");
    let calls = model.calls();

    let result = set_theme(&mut window, "flat");
    assert_eq!(result, ProcessEventResult::DoNothing);
    assert_eq!(model.calls(), calls, "layout() did not run");
}

#[test]
fn every_window_rebuilds_under_the_apps_new_theme() {
    let _app = fresh_app();
    let first_model = Model::new();
    let other_model = Model::new();
    let mut first = make_window(first_model.clone(), theme_structured_layout);
    let mut other = make_window(other_model.clone(), theme_structured_layout);
    first.regenerate_layout().expect("first layout");
    other.regenerate_layout().expect("other layout");
    assert_eq!(other_model.last().theme, "flat", "premise");

    let result = set_theme(&mut first, "flora");
    honor(&mut first, result);
    assert_eq!(first_model.last().theme, "flora");

    // The other window's next pass (the fan-out asks every window for one) rebuilds under the
    // app's new theme, tagged as a theme switch - whatever tag its own request carried.
    other.regenerate_layout().expect("the other window's pass");
    assert_eq!(
        other_model.last(),
        Seen {
            theme: "flora".to_string(),
            widget_theme: "flora".to_string(),
            reason: RelayoutReason::AppThemeChange,
        }
    );
    assert_eq!(window_theme(&other), "flora");
    assert_eq!(box_fill(&other), Some(FLORA));

    // A window opened after the switch starts in it.
    let late_model = Model::new();
    let mut late = make_window(late_model.clone(), theme_structured_layout);
    assert_eq!(window_theme(&late), "flora", "a new window starts in the app's theme");
    late.regenerate_layout().expect("late layout");
    assert_eq!(late_model.last().theme, "flora");
    assert_ne!(
        late_model.last().reason,
        RelayoutReason::AppThemeChange,
        "its first layout is no switch"
    );
}
