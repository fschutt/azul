//! Cascade-level custom properties: `--name` definitions are inheritable,
//! conditional declarations stored on nodes, and `var(--name, fallback)` is
//! resolved by the cascade under the window's live context.
//!
//! Design: `scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md`
//! §7.3 ("the one hard precondition") and §7.4 step 1. Before this, `var()`
//! was substituted at PARSE time, per stylesheet string and blind to
//! conditions: a `:root { --x }` in one stylesheet never reached a `var(--x)`
//! in another stylesheet or in a node's inline declarations, and
//! `@theme(dark) { :root { --bg } }` next to a light definition collapsed to
//! whichever came last.
//!
//! The painted values are read from the compact cache (the layout / paint
//! fast path) and, for pseudo-states, through `CssPropertyCache::get_property`
//! (the slow path the painter uses for a hovered node) - both cascades must
//! agree.

use azul_core::{
    diff::compute_node_changes,
    dom::{Dom, NodeData, NodeId},
    styled_dom::{StyledDom, StyledNodeState},
};
use azul_css::{
    css::Css,
    dynamic_selector::{DynamicSelectorContext, ThemeCondition},
    props::property::{parse_css_property, CssProperty, CssPropertyType},
};

// ------------------------------------------------------------------ helpers

fn ctx(theme: ThemeCondition) -> DynamicSelectorContext {
    DynamicSelectorContext {
        theme,
        ..Default::default()
    }
}

/// Several stylesheet strings as the ONE `Css` the cascade receives, in
/// order - an app sheet plus a theme sheet plus a rice, each parsed on its
/// own (so nothing can be substituted across them at parse time).
fn sheets(sources: &[&str]) -> Css {
    let mut rules = Vec::new();
    for s in sources {
        rules.extend(Css::from_string((*s).into()).rules.into_library_owned_vec());
    }
    Css::new(rules)
}

/// A node whose own (inline) style is `style`, the way a widget writes it.
fn inline(style: &str) -> Dom {
    Dom::create_div().with_style(Css::parse_inline(style))
}

fn styled(mut dom: Dom, css: Css, theme: ThemeCondition) -> StyledDom {
    StyledDom::create_with_context(&mut dom, css, Some(ctx(theme)))
}

/// `0xRRGGBBAA`, the compact cache's colour encoding.
const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    (r << 24) | (g << 16) | (b << 8) | 0xFF
}

/// The node's resolved text colour on the layout / paint fast path.
fn color(sd: &StyledDom, node: usize) -> u32 {
    sd.get_css_property_cache()
        .compact_cache
        .as_ref()
        .expect("the compact cache is built at creation")
        .tier2b_text[node]
        .text_color
}

/// The node's resolved width on the layout fast path (raw encoding: compare
/// against a reference DOM, never against a hand-decoded number).
fn width(sd: &StyledDom, node: usize) -> u32 {
    sd.get_css_property_cache()
        .compact_cache
        .as_ref()
        .expect("the compact cache is built at creation")
        .tier2_dims[node]
        .width
}

/// The node's value of `ty` in `state`, through the slow path the painter
/// uses for a node in a pseudo-state.
fn slow(sd: &StyledDom, node: usize, state: StyledNodeState, ty: CssPropertyType) -> CssProperty {
    let node_id = NodeId::new(node);
    let node_data = &sd.node_data.as_container()[node_id];
    sd.get_css_property_cache()
        .get_property(node_data, &node_id, &state, &ty)
        .cloned()
        .unwrap_or_else(|| panic!("node {node} resolves no {ty:?}"))
}

fn parsed(ty: CssPropertyType, value: &str) -> CssProperty {
    parse_css_property(ty, value).unwrap_or_else(|e| panic!("{value:?}: {e:?}"))
}

// ------------------------------------------------------------ §7.4 step 1

/// The design's step-1 test: ONE DOM, the consumer in a node's INLINE
/// declarations, the definitions in two stylesheets under `@theme(dark)` and
/// `@theme(light)`. The value follows the context through the restyle path
/// (`set_dynamic_selector_context`), with no DOM rebuild.
#[test]
fn an_inline_var_follows_the_mode_across_two_stylesheets_without_a_dom_rebuild() {
    let dom = Dom::create_body().with_child(inline("color: var(--fg, #ff0000);"));
    let css = sheets(&[
        "@theme(dark) { :root { --fg: #ffffff; } }",
        "@theme(light) { :root { --fg: #000000; } }",
    ]);
    let mut sd = styled(dom, css, ThemeCondition::Light);
    assert_eq!(color(&sd, 1), rgb(0, 0, 0), "light: the light definition");

    sd.set_dynamic_selector_context(ctx(ThemeCondition::Dark));
    assert_eq!(
        color(&sd, 1),
        rgb(255, 255, 255),
        "dark: the dark definition"
    );

    sd.set_dynamic_selector_context(ctx(ThemeCondition::Light));
    assert_eq!(color(&sd, 1), rgb(0, 0, 0), "and back, on the same DOM");
}

/// The same, with the consumer in a STYLESHEET rule instead: both channels
/// (author rules and a node's own declarations) read one environment.
#[test]
fn a_stylesheet_var_follows_the_mode_too() {
    let dom = Dom::create_body().with_child(Dom::create_div().with_class("label".into()));
    let css = sheets(&[
        ".label { color: var(--fg, #ff0000); }",
        "@theme(dark) { :root { --fg: #ffffff; } } @theme(light) { :root { --fg: #000000; } }",
    ]);
    let mut sd = styled(dom, css, ThemeCondition::Dark);
    assert_eq!(color(&sd, 1), rgb(255, 255, 255));
    sd.set_dynamic_selector_context(ctx(ThemeCondition::Light));
    assert_eq!(color(&sd, 1), rgb(0, 0, 0));
}

// ------------------------------------------------------------ across sheets

#[test]
fn a_root_definition_in_one_stylesheet_reaches_a_consumer_in_another() {
    let dom = Dom::create_body().with_child(Dom::create_div().with_class("v".into()));
    let with_var = styled(
        dom.clone(),
        sheets(&[
            ":root { --boxw: 150px; }",
            ".v { width: var(--boxw, 10px); }",
        ]),
        ThemeCondition::Light,
    );
    let direct = styled(
        dom,
        sheets(&[".v { width: 150px; }"]),
        ThemeCondition::Light,
    );
    assert_eq!(width(&with_var, 1), width(&direct, 1));
}

/// What the parse-time substitution did (one string holding both the
/// definition and the consumer) keeps working, now through the cascade.
#[test]
fn a_definition_in_the_same_string_still_reaches_its_consumer() {
    let dom = Dom::create_body().with_child(Dom::create_div().with_class("v".into()));
    let with_var = styled(
        dom.clone(),
        Css::from_string(":root{--boxw:150px} .v{width:var(--boxw)}".into()),
        ThemeCondition::Light,
    );
    let direct = styled(
        dom,
        Css::from_string(".v{width:150px}".into()),
        ThemeCondition::Light,
    );
    assert_eq!(width(&with_var, 1), width(&direct, 1));
}

#[test]
fn an_inline_definition_reaches_a_descendant_consumer_in_a_stylesheet() {
    let dom = Dom::create_body().with_child(
        inline("--fg: #0000ff;").with_child(Dom::create_div().with_class("label".into())),
    );
    let sd = styled(
        dom,
        sheets(&[".label { color: var(--fg, #ff0000); }"]),
        ThemeCondition::Light,
    );
    assert_eq!(color(&sd, 2), rgb(0, 0, 255));
}

// -------------------------------------------------------------- inheritance

/// §9.1 pitfall 1: the NEAREST definition wins down the tree, and a sibling
/// subtree is untouched by a definition that is not its ancestor's.
///
/// body (:root, --fg red)
/// |- .panel (--fg blue)
/// |  `- .leaf   -> blue
/// `- .other
///    `- .leaf   -> red
#[test]
fn a_panel_definition_beats_root_and_a_sibling_subtree_still_sees_root() {
    let dom = Dom::create_body()
        .with_child(
            Dom::create_div()
                .with_class("panel".into())
                .with_child(Dom::create_div().with_class("leaf".into())),
        )
        .with_child(
            Dom::create_div()
                .with_class("other".into())
                .with_child(Dom::create_div().with_class("leaf".into())),
        );
    let css = sheets(&[
        ":root { --fg: #ff0000; }",
        ".panel { --fg: #0000ff; }",
        ".leaf { color: var(--fg, #00ff00); }",
    ]);
    let sd = styled(dom, css, ThemeCondition::Light);
    assert_eq!(
        color(&sd, 2),
        rgb(0, 0, 255),
        "inside the panel: the panel's"
    );
    assert_eq!(
        color(&sd, 4),
        rgb(255, 0, 0),
        "the sibling subtree: :root's"
    );
}

// ---------------------------------------------------------------- fallbacks

#[test]
fn an_undefined_variable_takes_the_declared_fallback() {
    let dom = Dom::create_body().with_child(inline("color: var(--nope, #00ff00);"));
    let sd = styled(dom, Css::empty(), ThemeCondition::Light);
    assert_eq!(color(&sd, 1), rgb(0, 255, 0));
}

#[test]
fn a_fallback_may_itself_read_a_variable() {
    let dom = Dom::create_body().with_child(inline("color: var(--nope, var(--accent, #ff0000));"));
    let sd = styled(
        dom,
        sheets(&[":root { --accent: #0000ff; }"]),
        ThemeCondition::Light,
    );
    assert_eq!(
        color(&sd, 1),
        rgb(0, 0, 255),
        "the fallback's variable is defined"
    );

    let dom = Dom::create_body().with_child(inline("color: var(--nope, var(--gone, #ff0000));"));
    let sd = styled(dom, Css::empty(), ThemeCondition::Light);
    assert_eq!(
        color(&sd, 1),
        rgb(255, 0, 0),
        "neither is: the innermost literal"
    );
}

/// A definition may read another variable; the reference resolves where the
/// definition is (CSS computed-value semantics), not where it is read.
#[test]
fn a_definition_may_read_another_variable() {
    let dom = Dom::create_body().with_child(inline("color: var(--fg, #ff0000);"));
    let sd = styled(
        dom,
        sheets(&[":root { --base: #0000ff; --fg: var(--base); }"]),
        ThemeCondition::Light,
    );
    assert_eq!(color(&sd, 1), rgb(0, 0, 255));
}

#[test]
fn a_cycle_resolves_to_the_fallback_and_never_loops() {
    let dom = Dom::create_body().with_child(inline("color: var(--a, #00ff00);"));
    let sd = styled(
        dom,
        sheets(&[":root { --a: var(--b); --b: var(--a); }"]),
        ThemeCondition::Light,
    );
    assert_eq!(color(&sd, 1), rgb(0, 255, 0));
}

/// No fallback at all: the property's initial value (and a warning once),
/// exactly what `color: initial` gives the same node.
#[test]
fn a_cycle_without_a_fallback_resolves_to_the_initial_value() {
    let with_var = styled(
        Dom::create_body().with_child(inline("color: var(--a);")),
        sheets(&[":root { --a: var(--b); --b: var(--a); }"]),
        ThemeCondition::Light,
    );
    let initial = styled(
        Dom::create_body().with_child(inline("color: initial;")),
        Css::empty(),
        ThemeCondition::Light,
    );
    assert_eq!(color(&with_var, 1), color(&initial, 1));
}

// --------------------------------------------------------------- conditions

/// A definition carries a pseudo-state condition like any declaration, and
/// a consumer declared for the resting state reads it while the node is in
/// that state (`.btn:hover { --bg: .. }` + `.btn { background: var(--bg) }`,
/// the most common variable pattern on the web).
#[test]
fn a_hover_definition_recolours_a_resting_consumer_while_hovered() {
    let dom = Dom::create_body().with_child(Dom::create_div().with_class("b".into()));
    let css = sheets(&[
        ".b { color: var(--fg, #ff0000); }",
        ".b:hover { --fg: #00ff00; }",
    ]);
    let sd = styled(dom, css, ThemeCondition::Light);
    let tc = CssPropertyType::TextColor;
    assert_eq!(color(&sd, 1), rgb(255, 0, 0), "resting: the fallback");
    let hovered = StyledNodeState {
        hover: true,
        ..StyledNodeState::default()
    };
    assert_eq!(slow(&sd, 1, hovered, tc), parsed(tc, "#00ff00"), "hovered");
    assert_eq!(
        slow(&sd, 1, StyledNodeState::default(), tc),
        parsed(tc, "#ff0000"),
        "the slow path agrees with the compact cache at rest"
    );
}

/// The design's own example: a `background` (a shorthand key that expands to
/// the ONE longhand `background-content`) reads a variable with a `system:`
/// fallback.
#[test]
fn a_background_var_with_a_system_fallback_follows_the_mode() {
    let dom = Dom::create_body().with_child(inline(
        "background: var(--azul-button-face, system:button-face);",
    ));
    let css = sheets(&[
        "@theme(dark) { :root { --azul-button-face: #272822; } }",
        "@theme(light) { :root { --azul-button-face: #fafafa; } }",
    ]);
    let bg = CssPropertyType::BackgroundContent;
    let mut sd = styled(dom, css, ThemeCondition::Dark);
    assert_eq!(
        slow(&sd, 1, StyledNodeState::default(), bg),
        parsed(bg, "#272822")
    );
    sd.set_dynamic_selector_context(ctx(ThemeCondition::Light));
    assert_eq!(
        slow(&sd, 1, StyledNodeState::default(), bg),
        parsed(bg, "#fafafa")
    );

    // Undefined: the `system:` fallback, still a reference for the getters.
    let sd = styled(
        Dom::create_body().with_child(inline(
            "background: var(--azul-button-face, system:button-face);",
        )),
        Css::empty(),
        ThemeCondition::Dark,
    );
    assert_eq!(
        slow(&sd, 1, StyledNodeState::default(), bg),
        parsed(bg, "system:button-face")
    );
}

/// Definitions in a node's OWN style carry their conditions too, and the
/// window's context change re-resolves them even when there is no author
/// stylesheet at all (the restyle path must not be skipped for an empty
/// sheet when variables depend on the context).
#[test]
fn an_inline_conditional_definition_follows_the_mode_with_an_empty_stylesheet() {
    let dom = inline("--fg: #000000; @theme(dark) { --fg: #ffffff; }")
        .with_child(inline("color: var(--fg, #ff0000);"));
    let mut sd = styled(
        Dom::create_body().with_child(dom),
        Css::empty(),
        ThemeCondition::Light,
    );
    assert_eq!(color(&sd, 2), rgb(0, 0, 0));
    sd.set_dynamic_selector_context(ctx(ThemeCondition::Dark));
    assert_eq!(color(&sd, 2), rgb(255, 255, 255));
}

/// A universal rule reading a variable resolves PER NODE: `*` rules are
/// normally applied once for every node, which cannot hold a per-node value.
#[test]
fn a_var_consumer_in_a_star_rule_resolves_per_node() {
    let dom = Dom::create_body()
        .with_child(
            Dom::create_div()
                .with_class("panel".into())
                .with_child(Dom::create_div()),
        )
        .with_child(Dom::create_div());
    let css = sheets(&[
        "* { color: var(--fg, #ff0000); }",
        ".panel { --fg: #0000ff; }",
    ]);
    let sd = styled(dom, css, ThemeCondition::Light);
    assert_eq!(color(&sd, 2), rgb(0, 0, 255), "inside the panel");
    assert_eq!(color(&sd, 3), rgb(255, 0, 0), "outside it");
}

// ----------------------------------------------------- invalidation (pit. 11)

/// §9.1 pitfall 11, the reconcile half: a node whose only change is a
/// custom-property DEFINITION is dirty - its descendants read it, so a DL
/// patch that thought nothing changed would splice their stale items ("a
/// rice edit repaints half the window").
#[test]
fn a_changed_definition_marks_its_node_dirty() {
    let old = NodeData::create_div().with_css("--fg: #ff0000");
    let new = NodeData::create_div().with_css("--fg: #0000ff");
    let changes = compute_node_changes(&old, &new, None, None);
    assert!(
        changes.needs_layout() && !changes.is_visually_unchanged(),
        "a definition change can move any reader below it: {:#b}",
        changes.bits
    );

    let same = NodeData::create_div().with_css("--fg: #ff0000");
    assert!(compute_node_changes(&old, &same, None, None).is_empty());
}

/// A changed `var()` CONSUMER (here: its fallback) is a change of the node's
/// own property.
#[test]
fn a_changed_consumer_marks_its_node_dirty() {
    let old = NodeData::create_div().with_css("color: var(--fg, #ff0000)");
    let new = NodeData::create_div().with_css("color: var(--fg, #0000ff)");
    let changes = compute_node_changes(&old, &new, None, None);
    assert!(changes.needs_paint(), "{:#b}", changes.bits);
}
