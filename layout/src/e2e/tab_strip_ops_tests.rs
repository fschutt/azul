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
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": name,
        "config": { "continue_on_failure": true },
        "setup": { "window_width": 400, "window_height": 400, "dpi": 96 },
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
