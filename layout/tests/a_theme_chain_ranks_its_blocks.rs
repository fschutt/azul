//! RANK: where two live `@theme(..)` blocks declare the same property, the one ranked higher in
//! the theme chain wins (scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md §7.1).
//!
//! The chain lists the active app themes most specific first: `AZ_THEME=xyz:pink` is
//! `[xyz:pink, xyz, <default>]`. A live block's rank is its theme's index in the chain;
//! declarations outside every theme block rank last. The cascade order is `(priority, rank,
//! selector specificity, source order)` - CSS `@layer` semantics, rank BEFORE specificity - so a
//! spin-off's `.btn { }` beats its base's `.btn.primary { }`, and "where `xyz:pink` has no rule,
//! `xyz` applies" needs no special case.
//!
//! Every check reads three paths that must agree: the node's own value on the compact tier
//! (`get_style_properties`), on the slow path (`get_text_color`), and the value the text inside
//! it inherits (the restyle's inheritance walk). And every check runs in BOTH source orders: a
//! block that wins by coming last would pass a one-order assertion.
//!
//! The contexts set the chain directly; building it (`AZ_THEME`, `:` expansion, `fallback:`
//! headers, the default last) is a separate step.

use azul_core::{
    app_theme::ThemeScope,
    dom::{Dom, NodeId},
    styled_dom::StyledDom,
};
use azul_css::{
    css::Css,
    dynamic_selector::DynamicSelectorContext,
    props::{
        basic::{ColorU, PhysicalSize},
        property::CssPropertyType,
    },
    AzString, StringVec,
};
use azul_layout::{solver3::getters, widgets::themes::UiTheme};

const PINK: ColorU = ColorU { r: 255, g: 0, b: 255, a: 255 };
const BLUE: ColorU = ColorU { r: 0, g: 0, b: 255, a: 255 };
const GREEN: ColorU = ColorU { r: 0, g: 255, b: 0, a: 255 };

/// A window context whose theme chain is `names`.
fn under(names: &[&str]) -> DynamicSelectorContext {
    let mut ctx = DynamicSelectorContext::default().with_viewport(800.0, 600.0);
    ctx.theme_chain = StringVec::from_vec(
        names
            .iter()
            .map(|n| AzString::from((*n).to_string()))
            .collect(),
    );
    ctx
}

/// What `body > div > "ink"` paints: the div's `color` on the compact tier and on the slow
/// path, and the colour the text inside it inherits.
#[derive(Debug, PartialEq)]
struct Ink {
    compact: ColorU,
    slow: ColorU,
    text: ColorU,
}

fn ink_of(sd: &StyledDom) -> Ink {
    let (div, text) = (NodeId::new(1), NodeId::new(2));
    let slow = |node: NodeId| {
        let nd = &sd.node_data.as_container()[node];
        let state = sd.styled_nodes.as_container()[node].styled_node_state;
        let color = sd
            .get_css_property_cache()
            .get_text_color(nd, &node, &state)
            .and_then(|v| v.get_property().copied())
            .map(|c| c.inner)
            .expect("a `color`");
        getters::system_colors_resolved(sd, color)
    };
    Ink {
        compact: getters::get_style_properties(sd, div, None, PhysicalSize::new(800.0, 600.0))
            .color,
        slow: slow(div),
        text: slow(text),
    }
}

fn all(c: ColorU) -> Ink {
    Ink {
        compact: c,
        slow: c,
        text: c,
    }
}

/// `body > div(inline style) > "ink"`, cascaded under `chain`.
fn inline_document(inline: &str, chain: &[&str]) -> Ink {
    let mut div = Dom::create_div()
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("ink"));
    div.root.set_css(inline);
    let sd = StyledDom::create_from_dom_with_context(
        Dom::create_body().with_child(div),
        Some(under(chain)),
    );
    ink_of(&sd)
}

/// `body > div.btn.primary > "ink"` with the author stylesheet `css`, cascaded under `chain`.
fn stylesheet_document(css: &str, chain: &[&str]) -> Ink {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_class(AzString::from_const_str("btn"))
            .with_class(AzString::from_const_str("primary"))
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("ink")),
    );
    let css = Css::from_string(AzString::from(css.to_string()));
    let sd = StyledDom::create_with_context(&mut dom, css, Some(under(chain)));
    ink_of(&sd)
}

const SPIN_OFF: &[&str] = &["xyz:pink", "xyz"];

#[test]
fn a_spin_off_beats_its_base_in_a_nodes_own_declarations_regardless_of_source_order() {
    for inline in [
        "@theme(xyz:pink) { color: #ff00ff; } @theme(xyz) { color: #0000ff; }",
        "@theme(xyz) { color: #0000ff; } @theme(xyz:pink) { color: #ff00ff; }",
    ] {
        assert_eq!(inline_document(inline, SPIN_OFF), all(PINK), "{inline}");
        assert_eq!(
            inline_document(inline, &["xyz"]),
            all(BLUE),
            "{inline}: without the spin-off in the chain its block is dead"
        );
    }
}

#[test]
fn a_spin_off_beats_its_base_in_a_stylesheet_regardless_of_source_order() {
    for css in [
        "@theme(xyz:pink) { .btn { color: #ff00ff; } } @theme(xyz) { .btn { color: #0000ff; } }",
        "@theme(xyz) { .btn { color: #0000ff; } } @theme(xyz:pink) { .btn { color: #ff00ff; } }",
    ] {
        assert_eq!(stylesheet_document(css, SPIN_OFF), all(PINK), "{css}");
        assert_eq!(stylesheet_document(css, &["xyz"]), all(BLUE), "{css}");
    }
}

/// Rank sorts BEFORE selector specificity (`@layer` semantics): the spin-off's plain `.btn`
/// beats the base's `.btn.primary`.
#[test]
fn a_spin_offs_plain_rule_beats_a_more_specific_rule_of_its_base() {
    for css in [
        "@theme(xyz:pink) { .btn { color: #ff00ff; } } \
         @theme(xyz) { .btn.primary { color: #0000ff; } }",
        "@theme(xyz) { .btn.primary { color: #0000ff; } } \
         @theme(xyz:pink) { .btn { color: #ff00ff; } }",
    ] {
        assert_eq!(stylesheet_document(css, SPIN_OFF), all(PINK), "{css}");
    }
}

/// A theme block's universal rule is stored apart from the per-node rules (the global `*`
/// bucket); rank must order it there too, on the compact tier and on the slow path. (The text
/// inside is not checked: the slow path's inheritance walk does not read the `*` bucket at all,
/// with or without themes - a separate gap.)
#[test]
fn a_spin_offs_universal_rule_beats_its_base_on_every_path() {
    for css in [
        "@theme(xyz:pink) { * { color: #ff00ff; } } @theme(xyz) { * { color: #0000ff; } }",
        "@theme(xyz) { * { color: #0000ff; } } @theme(xyz:pink) { * { color: #ff00ff; } }",
    ] {
        let ink = stylesheet_document(css, SPIN_OFF);
        assert_eq!((ink.compact, ink.slow), (PINK, PINK), "{css}");
    }
}

/// Declarations outside every theme block rank LAST: a theme's block overrides them wherever
/// both apply, whatever the order.
#[test]
fn a_theme_block_beats_a_declaration_outside_every_block() {
    for inline in [
        "color: #0000ff; @theme(xyz) { color: #ff00ff; }",
        "@theme(xyz) { color: #ff00ff; } color: #0000ff;",
    ] {
        assert_eq!(inline_document(inline, &["xyz"]), all(PINK), "{inline}");
        assert_eq!(inline_document(inline, &["abc"]), all(BLUE), "{inline}");
    }
}

/// `[abc, flat]`: a theme no widget knows shows the flat floor - and whatever abc's own block
/// adds ranks above it.
#[test]
fn an_unknown_theme_shows_the_flat_floor_and_its_own_block_on_top() {
    let widget = "@theme(flat) { color: #0000ff; } @theme(flora) { color: #ff00ff; }";
    assert_eq!(inline_document(widget, &["abc", "flat"]), all(BLUE));
    for inline in [
        "@theme(abc) { color: #00ff00; } @theme(flat) { color: #0000ff; }",
        "@theme(flat) { color: #0000ff; } @theme(abc) { color: #00ff00; }",
    ] {
        assert_eq!(inline_document(inline, &["abc", "flat"]), all(GREEN), "{inline}");
    }
}

/// `[flora, flat]` - flora, with the app default as the implicit last entry. Flat is a complete
/// look of its own and must not fill the gaps of flora's: a property only flat declares stays
/// undeclared.
#[test]
fn a_second_compiled_in_theme_in_the_chain_fills_no_gap() {
    for inline in [
        "@theme(flat) { color: #0000ff; }",
        "@theme(flora) { background-color: #ff00ff; } @theme(flat) { color: #0000ff; }",
    ] {
        let ink = inline_document(inline, &["flora", "flat"]);
        assert_ne!(ink.compact, BLUE, "{inline}: flat's colour leaked into flora");
        assert_ne!(ink.slow, BLUE, "{inline}");
        assert_ne!(ink.text, BLUE, "{inline}");
        assert_eq!(inline_document(inline, &["flat", "flora"]), all(BLUE), "{inline}");
    }
}

/// `xyz` is a prefix SEGMENT of `xyz:pink`, not a string prefix of `xyzzy`.
#[test]
fn a_theme_block_is_live_under_its_spin_off_and_not_under_a_longer_name() {
    let inline = "color: #00ff00; @theme(xyz) { color: #0000ff; }";
    assert_eq!(inline_document(inline, &["xyz:pink"]), all(BLUE));
    assert_eq!(inline_document(inline, &["xyzzy"]), all(GREEN));
}

/// Every property either DOM declares on a node, resolved on the slow path, and the node's text
/// style on the compact tier - for every node of two same-shaped styled DOMs.
fn assert_styles_alike(what: &str, got: &StyledDom, want: &StyledDom) {
    let (g, w) = (got.node_data.as_container(), want.node_data.as_container());
    assert_eq!(g.len(), w.len(), "{what}: node count");
    let mut bad = Vec::new();
    for i in 0..g.len() {
        let node = NodeId::new(i);
        let mut types: Vec<CssPropertyType> = Vec::new();
        for nd in [&g[node], &w[node]] {
            for (p, _) in nd.style.iter_inline_properties() {
                if !types.contains(&p.get_type()) {
                    types.push(p.get_type());
                }
            }
        }
        for ty in types {
            let read = |sd: &StyledDom| {
                let nd = &sd.node_data.as_container()[node];
                let state = sd.styled_nodes.as_container()[node].styled_node_state;
                sd.get_css_property_cache()
                    .get_property(nd, &node, &state, &ty)
                    .cloned()
            };
            let (a, b) = (read(got), read(want));
            if a != b {
                bad.push(format!("node {i} {ty:?}: {a:?}, pinned {b:?}"));
            }
        }
        let size = PhysicalSize::new(800.0, 600.0);
        if getters::get_style_properties(got, node, None, size)
            != getters::get_style_properties(want, node, None, size)
        {
            bad.push(format!("node {i}: the compact text style differs"));
        }
    }
    assert!(bad.is_empty(), "{what}:\n  {}", bad.join("\n  "));
}

/// A `StyledDom` no window adopted (headless styling, PDF export, tests) evaluates every
/// condition but the app theme as false; the app theme is the one the DOM is being built for.
/// So an unpinned widget styled headless looks like the widget pinned to that theme - not like
/// its bare structure.
#[test]
fn a_headless_styled_dom_of_an_unpinned_widget_styles_like_the_current_app_theme() {
    use azul_layout::widgets::{button::Button, check_box::CheckBox};
    for theme in [UiTheme::Flat, UiTheme::Flora] {
        let _scope = ThemeScope::enter(AzString::from_const_str(theme.name()));
        let followed = StyledDom::create_from_dom(Button::create("Save".into()).dom());
        let pinned =
            StyledDom::create_from_dom(Button::create("Save".into()).with_theme(theme).dom());
        assert_styles_alike(&format!("button under {theme:?}"), &followed, &pinned);

        let followed = StyledDom::create_from_dom(CheckBox::create(true).dom());
        let pinned = StyledDom::create_from_dom(CheckBox::create(true).with_theme(theme).dom());
        assert_styles_alike(&format!("check box under {theme:?}"), &followed, &pinned);
    }
}

/// A spin-off of a compiled-in theme (`flora:abc`) names flora as its structural base: the
/// widget is built in flora's structure and wears flora's look, headless and in a window.
#[test]
fn a_spin_off_of_flora_builds_and_styles_like_flora() {
    use azul_layout::widgets::button::Button;
    let _scope = ThemeScope::enter(AzString::from_const_str("flora:abc"));
    assert_eq!(UiTheme::current(), UiTheme::Flora, "the structural base of flora:abc");
    let pinned = || Button::create("Save".into()).with_theme(UiTheme::Flora).dom();
    let followed = || Button::create("Save".into()).dom();
    assert_styles_alike(
        "headless button under flora:abc",
        &StyledDom::create_from_dom(followed()),
        &StyledDom::create_from_dom(pinned()),
    );
    let ctx = under(&["flora:abc", "flat"]);
    assert_styles_alike(
        "button in a window under [flora:abc, flat]",
        &StyledDom::create_from_dom_with_context(followed(), Some(ctx.clone())),
        &StyledDom::create_from_dom_with_context(pinned(), Some(ctx)),
    );
}
