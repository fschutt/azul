//! The touch strip through the real dispatcher: a row longer than the window scrolls sideways and
//! shows no scrollbar, at rest and while it is scrolled; and `scrollbar-width: none` written as CSS
//! text hides the bar as the typed property does (it came back as the CSS-wide keyword, which reads
//! as "no value", and every bar that was asked away in CSS text was drawn all the same).

use azul_core::{
    dom::{Dom, IdOrClass},
    styled_dom::StyledDom,
};
use azul_css::{AzString, StringVec};

use super::{runner::run_e2e_test_with_dom, E2eTest, E2eTestResult};
use crate::widgets::tabs::{TabHeader, TabStyle};

const TABS: usize = 12;

fn run(name: &str, dom: StyledDom, steps: serde_json::Value) -> E2eTestResult {
    run_at(name, dom, steps, 400, 96)
}

/// The same on a window `width` px wide at `dpi` (96 is a scale of 1, 252 is a Pixel 8's 2.625).
fn run_at(name: &str, dom: StyledDom, steps: serde_json::Value, width: u32, dpi: u32) -> E2eTestResult {
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": name,
        "config": { "continue_on_failure": true },
        "setup": { "window_width": width, "window_height": 400, "dpi": dpi },
        "steps": steps,
    }))
    .expect("the scenario literal is a valid E2eTest");
    run_e2e_test_with_dom(&test, Some(dom))
}

fn failures(result: &E2eTestResult) -> String {
    result
        .steps
        .iter()
        .filter(|s| s.status != "pass")
        .map(|s| {
            format!(
                "  step {} `{}`: {}",
                s.step_index,
                s.op,
                s.error.clone().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_passes(result: &E2eTestResult) {
    assert!(
        result.status == "pass" && result.steps_failed == 0,
        "scenario '{}' failed:\n{}",
        result.name,
        failures(result)
    );
}

/// `get_scrollbar_info` for the horizontal bar of `selector`: `found` is whether the node has one.
fn bar_of(selector: &str) -> serde_json::Value {
    serde_json::json!({
        "op": "get_scrollbar_info", "selector": selector, "orientation": "horizontal"
    })
}

fn has_a_bar() -> serde_json::Value {
    serde_json::json!({ "op": "assert_response", "contains": "\"found\":true" })
}

fn has_no_bar() -> serde_json::Value {
    serde_json::json!({ "op": "assert_response", "contains": "\"found\":false" })
}

#[test]
fn the_touch_strip_scrolls_sideways_and_shows_no_scrollbar() {
    let labels = StringVec::from_vec(
        (0..TABS)
            .map(|i| AzString::from(format!("Tab number {i}")))
            .collect(),
    );
    let mut dom = Dom::create_body().with_child(
        TabHeader::create(labels)
            .with_active_tab(0)
            .with_style(TabStyle::TouchPills)
            .dom(),
    );
    let (css, _) = azul_css::parser2::new_from_str("* { margin: 0; padding: 0; }");
    let strip = ".__azul-native-tabs-touch-strip";
    assert_passes(&run(
        "touch_strip_has_no_bar",
        StyledDom::create(&mut dom, css),
        serde_json::json!([
            { "op": "wait_frame" },
            // the row is longer than the window: the engine says it scrolls sideways
            { "op": "get_scrollable_nodes" },
            { "op": "assert_response", "contains": "\"can_scroll_x\":true" },
            bar_of(strip),
            has_no_bar(),
            // scrolled by hand (the finger, the wheel, the keys): still no bar
            { "op": "scroll_node_by", "selector": strip, "delta_x": 150.0, "delta_y": 0.0 },
            { "op": "wait_frame" },
            { "op": "tick_ms", "ms": 400 },
            bar_of(strip),
            has_no_bar(),
        ]),
    ));
}

/// The tab a scenario clicks by its text, recorded by the strip's own callback.
struct Clicked(std::sync::Arc<std::sync::atomic::AtomicUsize>);

extern "C" fn record_tab(
    data: azul_core::refany::RefAny,
    _info: crate::callbacks::CallbackInfo,
    state: crate::widgets::tabs::TabHeaderState,
) -> azul_core::callbacks::Update {
    let mut data = data;
    if let Some(clicked) = data.downcast_ref::<Clicked>() {
        clicked
            .0
            .store(state.active_tab + 1, std::sync::atomic::Ordering::SeqCst);
    }
    azul_core::callbacks::Update::DoNothing
}

/// A scenario that clicks a pill by its text must reach one that is out of the window: the op scrolls
/// the strip to it, then presses on it. On a phone (Android emulator, 2026-11-28) a click on a tab of the
/// ribbon in the band found no panel afterwards.
#[test]
fn a_click_by_text_reaches_a_pill_of_the_touch_strip_that_is_out_of_the_window() {
    // a desktop window at a scale of 1, and a phone's: 411 px wide at 2.625; and a window that sits on a
    // Pixel 8, whose status bar and camera push the content down (a click is aimed at the pill's place
    // in the WINDOW, not in the layout)
    for (width, dpi, preset) in [(400, 96, "off"), (411, 252, "off"), (411, 252, "pixel8")] {
        let seen = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        // the `fn` item as the `fn` pointer the `Into` bound needs
        let on_click: crate::widgets::tabs::TabOnClickCallbackType = record_tab;
        let labels = StringVec::from_vec(
            (0..TABS)
                .map(|i| AzString::from(format!("Tab number {i}")))
                .collect(),
        );
        let mut dom = Dom::create_body().with_child(
            TabHeader::create(labels)
                .with_active_tab(0)
                .with_style(TabStyle::TouchPills)
                .with_on_click(
                    azul_core::refany::RefAny::new(Clicked(seen.clone())),
                    on_click,
                )
                .dom(),
        );
        let (css, _) = azul_css::parser2::new_from_str("* { margin: 0; padding: 0; }");
        assert_passes(&run_at(
            "click_by_text_far_pill",
            StyledDom::create(&mut dom, css),
            serde_json::json!([
                { "op": "wait_frame" },
                { "op": "set_display_preset", "preset": preset },
                { "op": "wait_frame" },
                { "op": "wait_frame" },
                { "op": "click", "text": "Tab number 11" },
                { "op": "wait_frame" },
                { "op": "wait_frame" },
                { "op": "wait_frame" },
            ]),
            width,
            dpi,
        ));
        assert_eq!(
            seen.load(std::sync::atomic::Ordering::SeqCst),
            TABS,
            "{width} px at {dpi} dpi, preset {preset}: the last pill (index 11, recorded as 12) was clicked"
        );
    }
}

#[test]
fn scrollbar_width_none_in_css_text_hides_the_bar_as_the_typed_property_does() {
    #[allow(clippy::wildcard_imports)] // the property types the typed form is made of
    use azul_css::props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    };

    let wide = || Dom::create_div().with_css("width: 1000px; height: 10px; flex-shrink: 0;");
    let class =
        |name: &str| -> azul_core::dom::IdOrClassVec { vec![IdOrClass::Class(name.into())].into() };
    let scroller = |name: &str, css: &str| {
        Dom::create_div()
            .with_ids_and_classes(class(name))
            .with_css(&format!(
                "display: flex; width: 100px; height: 48px; overflow-x: auto; overflow-y: hidden; {css}"
            ))
            .with_child(wide())
    };
    // The same ask four ways: nothing (the control: it has a bar), the inline CSS text, the
    // typed property, and a rule of the stylesheet.
    let typed = scroller("typed", "").with_css_props(
        vec![
            azul_css::dynamic_selector::CssPropertyWithConditions::simple(
                CssProperty::ScrollbarWidth(LayoutScrollbarWidthValue::Exact(
                    LayoutScrollbarWidth::None,
                )),
            ),
        ]
        .into(),
    );
    let mut dom = Dom::create_body()
        .with_child(scroller("plain", ""))
        .with_child(scroller("inline", "scrollbar-width: none;"))
        .with_child(typed)
        .with_child(scroller("sheet", ""));
    let (css, _) = azul_css::parser2::new_from_str(
        "* { margin: 0; padding: 0; } .sheet { scrollbar-width: none; }",
    );
    assert_passes(&run(
        "scrollbar_width_none",
        StyledDom::create(&mut dom, css),
        serde_json::json!([
            { "op": "wait_frame" },
            bar_of(".plain"),
            has_a_bar(),
            bar_of(".inline"),
            has_no_bar(),
            bar_of(".typed"),
            has_no_bar(),
            bar_of(".sheet"),
            has_no_bar(),
        ]),
    ));
}

// ---- the desktop's row of 22 px pills: a fixed box with a row inside that scrolls ----

fn desktop_row(tabs: &[&str]) -> StyledDom {
    let labels = StringVec::from_vec(tabs.iter().map(|s| AzString::from(*s)).collect());
    let mut dom = Dom::create_body().with_child(
        TabHeader::create(labels)
            .with_active_tab(0)
            .with_theme(crate::widgets::themes::UiTheme::Flora)
            .with_style(TabStyle::Pills)
            .dom(),
    );
    let (css, _) = azul_css::parser2::new_from_str("* { margin: 0; padding: 0; }");
    StyledDom::create(&mut dom, css)
}

const SHORT: [&str; 4] = ["Home", "Insert", "Draw", "View"];
const LONG: [&str; 12] = [
    "Overview", "Mailings", "References", "Developer", "Navigation", "Properties", "Statistics", "Templates",
    "Annotations", "Calendars", "Documents", "Reviewing",
];

/// 1280, 700 and 480 px (the last two narrower than the long row): a row that fits does not scroll, one
/// that does not fit scrolls sideways and shows no scrollbar, at rest and while it is scrolled.
#[test]
fn a_desktop_pill_row_scrolls_when_it_does_not_fit_and_shows_no_scrollbar() {
    let strip = ".__azul-native-tabs-pill-strip";
    for width in [1280, 700, 480] {
        // four tabs fit in all three
        assert_passes(&run_at(
            "desktop_row_fits",
            desktop_row(&SHORT),
            serde_json::json!([
                { "op": "wait_frame" },
                { "op": "get_scrollable_nodes" },
                { "op": "assert_response", "not_contains": "\"can_scroll_x\":true" },
                bar_of(strip),
                has_no_bar(),
            ]),
            width,
            96,
        ));
    }
    for width in [700, 480] {
        // twelve long ones do not fit in 700 or 480
        assert_passes(&run_at(
            "desktop_row_overflows",
            desktop_row(&LONG),
            serde_json::json!([
                { "op": "wait_frame" },
                { "op": "get_scrollable_nodes" },
                { "op": "assert_response", "contains": "\"can_scroll_x\":true" },
                bar_of(strip),
                has_no_bar(),
                // scrolled by hand: the row moves, still no bar
                { "op": "scroll_node_by", "selector": strip, "delta_x": 150.0, "delta_y": 0.0 },
                { "op": "wait_frame" },
                { "op": "tick_ms", "ms": 400 },
                { "op": "assert_scroll", "selector": strip, "x": 150.0, "tolerance": 3.0 },
                bar_of(strip),
                has_no_bar(),
            ]),
            width,
            96,
        ));
    }
}

/// The wheel: a vertical notch over a row that only scrolls sideways moves it sideways (every browser does,
/// and the engine maps it): a notch of 200 px (the engine's sign: negative is "down") ends 200 px along.
#[test]
fn the_wheel_over_a_desktop_pill_row_moves_it_sideways() {
    let strip = ".__azul-native-tabs-pill-strip";
    assert_passes(&run_at(
        "desktop_row_wheel",
        desktop_row(&LONG),
        serde_json::json!([
            { "op": "wait_frame" },
            { "op": "wheel", "x": 200.0, "y": 14.0, "delta_x": 0.0, "delta_y": -200.0 },
            { "op": "wait_frame" },
            { "op": "tick_ms", "ms": 800 },
            { "op": "wait_frame" },
            { "op": "assert_scroll", "selector": strip, "x": 200.0, "tolerance": 30.0 },
        ]),
        480,
        96,
    ));
}
