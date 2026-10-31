//! `CallbackInfo::set_node_style` with a style that differs from the node's
//! current one only in its `var()` references or its custom-property
//! definitions (`--x`): the node, and every reader below it, resolves the new
//! variables.
//!
//! The content chokepoint decided "changed" from the node's STATIC
//! declarations alone (`Css::iter_inline_properties` skips `var()` / `env()`
//! references and `--x` definitions), so such a style was stored but
//! `Unchanged` came back, with no recascade: the old colour stayed painted.
//! And the UA / inheritance / compact tail the chokepoint re-ran does not
//! resolve variables - the author cascade's variable pass does.
//!
//! Driven through the chokepoint the dll host and the e2e runner both
//! delegate `CallbackChange::SetNodeStyle` to.

use azul_core::dom::Dom;
use azul_css::{css::Css, props::basic::color::ColorU};
use azul_layout::overlay::ContentDirtyTier;

use super::a_replaced_inline_style_follows_the_mode::{box_fill, replace, window_with, RED};

const GREEN: ColorU = ColorU::rgb(0, 160, 0);

/// Every `var()` here falls back to blue, which no test expects.
const FALLBACK: &str = "rgb(0, 0, 200)";

/// `body > box`: the body defines `--a` red and `--b` green; the box (node 1)
/// is `style`.
fn page(style: &str) -> azul_layout::window::LayoutWindow {
    window_with(
        Dom::create_body()
            .with_style(Css::parse_inline(
                "margin: 0; --a: rgb(200, 0, 0); --b: rgb(0, 160, 0);",
            ))
            .with_child(Dom::create_div().with_style(Css::parse_inline(style))),
    )
}

/// A 40 x 20 box painted `var(--name)`.
fn reading(name: &str) -> String {
    format!("width: 40px; height: 20px; background: var(--{name}, {FALLBACK});")
}

/// A 40 x 20 box that defines `--x` as `colour` and paints itself with it.
fn defining(colour: &str) -> String {
    format!("--x: {colour}; width: 40px; height: 20px; background: var(--x, {FALLBACK});")
}

#[test]
fn a_node_restyled_to_read_another_variable_paints_that_variables_value() {
    let mut lw = page(&reading("a"));
    assert_eq!(box_fill(&lw, 40.0), Some(RED), "premise: the box reads --a");

    let tier = replace(&mut lw, Css::parse_inline(&reading("b")));
    assert_eq!(box_fill(&lw, 40.0), Some(GREEN), "the box reads --b now");
    assert_eq!(
        tier,
        ContentDirtyTier::RebuildDisplayList,
        "only a colour changed: a repaint, not a relayout"
    );
}

#[test]
fn a_node_restyled_with_a_changed_definition_paints_the_new_value() {
    let mut lw = page(&defining("rgb(200, 0, 0)"));
    assert_eq!(box_fill(&lw, 40.0), Some(RED), "premise: --x is red");

    let tier = replace(&mut lw, Css::parse_inline(&defining("rgb(0, 160, 0)")));
    assert_eq!(box_fill(&lw, 40.0), Some(GREEN), "--x is green now");
    assert_eq!(
        tier,
        ContentDirtyTier::Relayout,
        "a definition can feed any property below it, layout ones included"
    );
}

/// A definition is inherited: a changed one reaches the readers below the
/// node, whose own styles did not change.
#[test]
fn a_changed_definition_reaches_a_child_that_reads_it() {
    let definition = |colour: &str| format!("--x: {colour}; width: 40px; height: 20px;");
    let mut lw = window_with(
        Dom::create_body()
            .with_style(Css::parse_inline("margin: 0;"))
            .with_child(
                Dom::create_div()
                    .with_style(Css::parse_inline(&definition("rgb(200, 0, 0)")))
                    .with_child(Dom::create_div().with_style(Css::parse_inline(&format!(
                        "width: 30px; height: 20px; background: var(--x, {FALLBACK});"
                    )))),
            ),
    );
    assert_eq!(box_fill(&lw, 30.0), Some(RED), "premise: the child reads the box's --x");

    replace(&mut lw, Css::parse_inline(&definition("rgb(0, 160, 0)")));
    assert_eq!(box_fill(&lw, 30.0), Some(GREEN), "the child reads the new --x");
}

/// PIN: the same variables again change nothing.
#[test]
fn restyling_a_node_with_the_same_variables_is_unchanged() {
    let mut lw = page(&defining("rgb(200, 0, 0)"));
    assert_eq!(
        replace(&mut lw, Css::parse_inline(&defining("rgb(200, 0, 0)"))),
        ContentDirtyTier::Unchanged
    );
}
