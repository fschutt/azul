//! `:backdrop` (GTK: the window is not the active one) paints on every path.
//!
//! The pseudo-class names a WINDOW state, so it cannot come from the node the
//! way `:hover` does. Three stages each dropped it:
//! - the stylesheet parser did not know the name, so `.bar:backdrop { .. }`
//!   was a parse error and the rule vanished;
//! - the cascade collected stylesheet rules for every pseudo-state but this
//!   one;
//! - the paint-time resolver never asked for the backdrop state, so even the
//!   titlebar's INLINE `:backdrop` declaration (`TitleBar::background_inactive`)
//!   resolved to the resting colour - only the dynamic-selector matcher knew
//!   about the window flag.
//!
//! Each test cascades one DOM under an active and an inactive window context
//! and reads the colour back through the getter the display list paints with,
//! against the node's own state, as the display list does.

use azul_core::{
    dom::{Dom, IdOrClass},
    id::NodeId,
    styled_dom::StyledDom,
};
use azul_css::{
    dynamic_selector::{
        CssPropertyWithConditions, DynamicSelector, DynamicSelectorContext, PseudoStateType,
    },
    props::{
        basic::{color::ColorU, PhysicalSize},
        property::CssProperty,
        style::{StyleBackgroundContent, StyleBackgroundContentVec},
    },
};
use azul_layout::solver3::getters;

const GREEN: ColorU = ColorU {
    r: 0,
    g: 0xff,
    b: 0,
    a: 0xff,
};
const GREY: ColorU = ColorU {
    r: 0x80,
    g: 0x80,
    b: 0x80,
    a: 0xff,
};

/// Every DOM here is `body(0) > div.bar(1) > text(2)`.
const DIV: NodeId = NodeId::new(1);
const TEXT: NodeId = NodeId::new(2);

const BACKGROUND_SHEET: &str =
    ".bar { background: #00ff00; } .bar:backdrop { background: #808080; }";

const BACKDROP: &[DynamicSelector] = &[DynamicSelector::PseudoState(PseudoStateType::Backdrop)];

/// The context a window cascades under, active or not.
fn context(window_active: bool) -> DynamicSelectorContext {
    let mut ctx = DynamicSelectorContext::default().with_viewport(800.0, 600.0);
    ctx.window_focused = window_active;
    ctx
}

fn cascade(dom: Dom, window_active: bool) -> StyledDom {
    StyledDom::create_from_dom_with_context(dom, Some(context(window_active)))
}

fn text() -> Dom {
    Dom::create_text_do_not_use_without_block_level_wrapper("ink")
}

/// `div.bar` under a body carrying `sheet`.
fn bar_with_sheet(sheet: &str) -> Dom {
    Dom::create_body().with_css(sheet).with_child(
        Dom::create_div()
            .with_ids_and_classes(vec![IdOrClass::Class("bar".into())].into())
            .with_child(text()),
    )
}

fn solid(c: ColorU) -> CssProperty {
    CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(vec![
        StyleBackgroundContent::Color(c),
    ]))
}

/// The div's painted background, read against its own state.
fn background(sd: &StyledDom) -> ColorU {
    let state = sd.styled_nodes.as_container()[DIV].styled_node_state;
    getters::get_background_color(sd, DIV, &state)
}

/// The colour the text inside the div is painted in.
fn text_colour(sd: &StyledDom) -> ColorU {
    getters::get_style_properties(sd, TEXT, None, PhysicalSize::new(800.0, 600.0)).color
}

#[test]
fn a_stylesheet_backdrop_rule_paints_while_the_window_is_inactive() {
    assert_eq!(
        background(&cascade(bar_with_sheet(BACKGROUND_SHEET), true)),
        GREEN,
        "an active window paints the resting rule"
    );
    assert_eq!(
        background(&cascade(bar_with_sheet(BACKGROUND_SHEET), false)),
        GREY,
        "an inactive window paints the :backdrop rule"
    );
}

/// The titlebar's `background_inactive` is exactly this: an inline
/// declaration conditioned on `:backdrop`, after the resting one.
#[test]
fn an_inline_backdrop_declaration_paints_while_the_window_is_inactive() {
    let bar = || {
        Dom::create_body().with_child(
            Dom::create_div()
                .with_css_props(
                    vec![
                        CssPropertyWithConditions::simple(solid(GREEN)),
                        CssPropertyWithConditions::with_single_condition(solid(GREY), BACKDROP),
                    ]
                    .into(),
                )
                .with_child(text()),
        )
    };
    assert_eq!(background(&cascade(bar(), true)), GREEN);
    assert_eq!(
        background(&cascade(bar(), false)),
        GREY,
        "an inactive window paints the inline :backdrop declaration"
    );
}

/// A window is activated and deactivated long after its DOM was cascaded:
/// the context the window offers on each change is what flips it
/// (`LayoutWindow::apply_window_activation` offers exactly this).
#[test]
fn the_backdrop_colour_follows_the_window_back_and_forth() {
    let mut sd = cascade(bar_with_sheet(BACKGROUND_SHEET), true);
    assert_eq!(background(&sd), GREEN, "premise: active");
    sd.set_dynamic_selector_context(context(false));
    assert_eq!(background(&sd), GREY, "deactivated: the :backdrop rule");
    sd.set_dynamic_selector_context(context(true));
    assert_eq!(background(&sd), GREEN, "active again: the resting rule");
}

/// `color` inherits: a `:backdrop` text colour on a container reaches the
/// text inside it, as its resting colour does.
#[test]
fn a_backdrop_text_colour_reaches_the_text_inside() {
    let sheet = ".bar { color: #00ff00; } .bar:backdrop { color: #808080; }";
    assert_eq!(text_colour(&cascade(bar_with_sheet(sheet), true)), GREEN);
    assert_eq!(
        text_colour(&cascade(bar_with_sheet(sheet), false)),
        GREY,
        "the text of an inactive window inherits the :backdrop colour"
    );
}
