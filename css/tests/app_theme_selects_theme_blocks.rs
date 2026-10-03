//! `@theme(<name>)` is a block of the APP THEME (`flat`, `flora`, later `native` and user themes),
//! live exactly while that theme is the app's; `@theme(light)` / `@theme(dark)` keep meaning the
//! COLOUR SCHEME.
//!
//! The mechanism the widget migration rests on: every widget carries ALL its theme blocks (a flat
//! one and a flora one, as const statics), and the dynamic-condition matching - the same machinery
//! as `@media`, `@os` and dark mode - keeps only the active theme's. So an app does not write
//! `with_theme` on every widget, and a widget cannot be forgotten in the other theme.
//!
//! Every check pins BOTH directions: a block that "works" by applying unconditionally would pass
//! a one-sided assertion (and that is exactly what `@theme(flora)` did before: the unknown name
//! parsed to NO condition, so its rules applied in every theme).

use azul_css::{
    dynamic_selector::{
        app_theme_chain, CssPropertyWithConditions, CssPropertyWithConditionsVec, DynamicSelector,
        DynamicSelectorContext, PseudoStateType, ThemeCondition, DEFAULT_APP_THEME,
    },
    parser2::new_from_str,
    props::{basic::color::ColorU, property::CssProperty, style::StyleTextColor},
    theme_chain::{resolve_theme_head, ThemeEnv},
    AzString, StringVec,
};

const FLORA_INK: ColorU = ColorU::rgb(0x10, 0x20, 0x30);
const FLORA_NIGHT_INK: ColorU = ColorU::rgb(0xe0, 0xd0, 0xc0);
const FLAT_INK: ColorU = ColorU::rgb(0x01, 0x02, 0x03);

const fn ink(c: ColorU) -> CssProperty {
    CssProperty::const_text_color(StyleTextColor { inner: c })
}

/// The shape a migrated widget ships: its theme blocks as const statics, each declaration
/// conditioned on its theme (and, for a dark twin, on the colour scheme as well).
static WIDGET_INK: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::with_single_condition(
        ink(FLAT_INK),
        azul_css::theme_conditions!("flat"),
    ),
    CssPropertyWithConditions::with_single_condition(
        ink(FLORA_INK),
        azul_css::theme_conditions!("flora"),
    ),
    CssPropertyWithConditions::with_single_condition(
        ink(FLORA_NIGHT_INK),
        azul_css::theme_conditions!("flora", DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark)),
    ),
];

fn named(name: &str) -> DynamicSelector {
    DynamicSelector::Theme(ThemeCondition::Custom(AzString::from(name.to_string())))
}

/// The conditions of every rule block of `css`, in source order.
fn conditions_of(css: &str) -> Vec<Vec<DynamicSelector>> {
    let (result, _warnings) = new_from_str(css);
    result
        .rules()
        .map(|r| r.conditions.iter().cloned().collect())
        .collect()
}

/// A rule block is live iff every one of its conditions holds (they conjoin).
fn live(conditions: &[DynamicSelector], ctx: &DynamicSelectorContext) -> bool {
    conditions.iter().all(|c| c.matches(ctx))
}

fn ctx(app_theme: &str, mode: azul_css::system::DarkLightMode) -> DynamicSelectorContext {
    DynamicSelectorContext {
        mode,
        ..Default::default()
    }
    .with_app_theme(app_theme)
}

/// The value the inline declarations resolve to under `ctx`: last match wins, as in the
/// cascade's inline resolution.
fn resolved_ink(props: &[CssPropertyWithConditions], ctx: &DynamicSelectorContext) -> Option<ColorU> {
    props
        .iter()
        .filter(|p| p.matches(ctx))
        .filter_map(|p| match &p.property {
            CssProperty::TextColor(v) => v.get_property().map(|c| c.inner),
            _ => None,
        })
        .last()
}

#[test]
fn a_named_theme_block_parses_as_that_app_theme() {
    for css in [
        "@theme(flora) { div { color: red; } }",
        "@theme flora { div { color: red; } }",
        "@theme(\"flora\") { div { color: red; } }",
    ] {
        assert_eq!(
            conditions_of(css),
            vec![vec![named("flora")]],
            "{css} must be ONE rule block conditioned on the app theme `flora`"
        );
    }
    // A spin-off name survives whole: the theme chain (scripts/ideas/RICING_LAYERS_... §7)
    // expands `xyz:pink` later; the parser must not cut it.
    assert_eq!(
        conditions_of("@theme(xyz:pink) { div { color: red; } }"),
        vec![vec![named("xyz:pink")]]
    );
}

/// The regression this whole step starts from: `@theme(flora)` parsed to NO condition, so a
/// widget's flora block painted in every app.
#[test]
fn a_flora_block_does_not_apply_in_an_app_that_did_not_choose_flora() {
    let conditions = conditions_of("@theme(flora) { div { color: red; } }");
    assert_eq!(conditions.len(), 1, "one rule block");
    assert!(
        !live(&conditions[0], &DynamicSelectorContext::default()),
        "under the default app theme a flora block must be inert, got conditions {:?}",
        conditions[0]
    );
}

#[test]
fn the_default_app_theme_is_flat() {
    assert_eq!(DEFAULT_APP_THEME, "flat");
    assert_eq!(DynamicSelectorContext::default().app_theme(), DEFAULT_APP_THEME);
    let flat = conditions_of("@theme(flat) { div { color: red; } }");
    assert!(
        live(&flat[0], &DynamicSelectorContext::default()),
        "a flat block is live in an app that chose nothing"
    );
}

#[test]
fn a_flora_block_applies_only_under_flora_and_a_flat_block_only_under_flat() {
    let blocks = conditions_of(
        "@theme(flat) { div { color: blue; } }
         @theme(flora) { div { color: red; } }",
    );
    assert_eq!(blocks.len(), 2, "two rule blocks");
    let (flat, flora) = (&blocks[0], &blocks[1]);
    for scheme in [azul_css::system::DarkLightMode::Light, azul_css::system::DarkLightMode::Dark] {
        let under_flat = ctx("flat", scheme.clone());
        let under_flora = ctx("flora", scheme.clone());
        assert!(live(flat, &under_flat), "flat block, flat app, {scheme:?}");
        assert!(!live(flora, &under_flat), "flora block, flat app, {scheme:?}");
        assert!(live(flora, &under_flora), "flora block, flora app, {scheme:?}");
        assert!(!live(flat, &under_flora), "flat block, flora app, {scheme:?}");
    }
    // An app theme nobody wrote a block for is the default theme's look: every chain ends in the
    // default theme, the floor (design §7.1, `[abc, <default>]`).
    let under_other = ctx("monokai", azul_css::system::DarkLightMode::Light);
    assert!(live(flat, &under_other), "an unknown theme falls back to the default theme's block");
    assert!(!live(flora, &under_other), "and to no other theme's");
}

#[test]
fn light_and_dark_still_match_the_colour_scheme_whatever_the_app_theme() {
    for css in [
        "@theme(dark) { div { color: red; } }",
        "@media (prefers-color-scheme: dark) { div { color: red; } }",
    ] {
        let blocks = conditions_of(css);
        assert_eq!(
            blocks,
            vec![vec![DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark)]],
            "{css} stays the colour scheme"
        );
        for app in ["flat", "flora"] {
            assert!(live(&blocks[0], &ctx(app, azul_css::system::DarkLightMode::Dark)), "{css}, {app}, dark");
            assert!(!live(&blocks[0], &ctx(app, azul_css::system::DarkLightMode::Light)), "{css}, {app}, light");
        }
    }
    let light = conditions_of("@theme(light) { div { color: red; } }");
    assert_eq!(light, vec![vec![DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Light)]]);
    assert!(live(&light[0], &ctx("flora", azul_css::system::DarkLightMode::Light)));
    assert!(!live(&light[0], &ctx("flora", azul_css::system::DarkLightMode::Dark)));
}

/// A theme's dark sub-mode is a colour-scheme block NESTED in the theme block: it needs both.
#[test]
fn a_dark_block_nested_in_a_theme_block_needs_the_theme_and_the_dark_scheme() {
    let blocks = conditions_of("@theme(flora) { @theme(dark) { div { color: red; } } }");
    assert_eq!(
        blocks,
        vec![vec![named("flora"), DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark)]]
    );
    let nested = &blocks[0];
    assert!(live(nested, &ctx("flora", azul_css::system::DarkLightMode::Dark)));
    assert!(!live(nested, &ctx("flora", azul_css::system::DarkLightMode::Light)));
    assert!(!live(nested, &ctx("flat", azul_css::system::DarkLightMode::Dark)));
    assert!(!live(nested, &ctx("flat", azul_css::system::DarkLightMode::Light)));
}

/// The names of a context's theme chain, most specific first.
fn names(chain: &StringVec) -> Vec<&str> {
    chain.as_ref().iter().map(AzString::as_str).collect()
}

/// The context holds the active theme as a CHAIN, most specific first, built by the one chain
/// builder (`theme_chain::expand_chain`): the app theme, its `:` prefixes, and the default theme
/// as the floor every chain ends in (design §7.1). Which of two compiled-in themes' blocks
/// apply when both are in the chain is the matcher's decision, not the chain's.
#[test]
fn the_theme_chain_is_the_app_theme_over_the_default_theme() {
    assert_eq!(names(&app_theme_chain("flora")), ["flora", "flat"]);
    assert_eq!(names(&app_theme_chain(DEFAULT_APP_THEME)), [DEFAULT_APP_THEME]);
    let ctx = DynamicSelectorContext::default().with_app_theme("flora");
    assert_eq!(ctx.app_theme(), "flora");
    assert_eq!(ctx.theme_chain, app_theme_chain("flora"));
    assert_ne!(
        ctx,
        DynamicSelectorContext::default(),
        "a context with another app theme is another context (the cascade epoch and the \
         restyle decision key on context equality)"
    );
}

/// `AZ_THEME=xyz:pink` outranks the app's own choice and gives the context the spin-off's prefix
/// chain over the default theme. Through the variable's VALUE (`ThemeEnv::from_values`), never
/// the process environment, so no other test sees it.
#[test]
fn az_theme_xyz_pink_gives_the_context_the_chain_xyz_pink_xyz_flat() {
    let env = ThemeEnv::from_values(Some("xyz:pink"), None);
    let head = resolve_theme_head(env.theme.as_deref(), Some("flora"));
    assert_eq!(head, "xyz:pink", "the environment outranks the app's choice");
    let ctx = DynamicSelectorContext::default().with_app_theme(head);
    assert_eq!(names(&ctx.theme_chain), ["xyz:pink", "xyz", "flat"]);
    assert_eq!(ctx.app_theme(), "xyz:pink");
}

/// A mode word is never an app theme: `AppConfig::with_theme("dark")` is an error in the chain,
/// which falls back to the default theme alone.
#[test]
fn a_mode_word_as_the_app_theme_leaves_the_default_chain() {
    assert_eq!(names(&app_theme_chain("dark")), [DEFAULT_APP_THEME]);
    assert_eq!(
        DynamicSelectorContext::default()
            .with_app_theme("dark")
            .app_theme(),
        DEFAULT_APP_THEME
    );
}

/// What a widget ships: a const static block per theme, resolved by the matcher.
#[test]
fn a_widgets_theme_blocks_resolve_to_the_active_themes_values() {
    assert_eq!(
        resolved_ink(WIDGET_INK, &ctx("flat", azul_css::system::DarkLightMode::Light)),
        Some(FLAT_INK)
    );
    assert_eq!(
        resolved_ink(WIDGET_INK, &ctx("flat", azul_css::system::DarkLightMode::Dark)),
        Some(FLAT_INK),
        "flora's dark twin must not leak into the flat theme"
    );
    assert_eq!(
        resolved_ink(WIDGET_INK, &ctx("flora", azul_css::system::DarkLightMode::Light)),
        Some(FLORA_INK)
    );
    assert_eq!(
        resolved_ink(WIDGET_INK, &ctx("flora", azul_css::system::DarkLightMode::Dark)),
        Some(FLORA_NIGHT_INK)
    );
}

/// The runtime helpers for declarations a widget builds at DOM-build time, and the pairing
/// vocabulary the dark-twin lint (`widgets::theme_pairs`) needs once blocks carry theme names.
#[test]
fn declaration_helpers_add_read_and_strip_the_theme_name() {
    let light = CssPropertyWithConditions::simple(ink(FLORA_INK)).in_theme("flora");
    assert_eq!(light.apply_if.as_slice(), &[named("flora")][..]);
    assert_eq!(light.theme_names(), vec!["flora"]);
    assert!(light.is_light_half(), "a theme's resting light value is a light half");
    assert!(!light.is_dark_twin());

    let dark = CssPropertyWithConditions::dark_mode(ink(FLORA_NIGHT_INK)).in_theme("flora");
    assert_eq!(
        dark.apply_if.as_slice(),
        &[named("flora"), DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark)][..],
        "the theme name goes FIRST, the colour scheme and states stay"
    );
    assert!(dark.is_dark_twin());
    assert!(!dark.is_light_half());

    let hover = CssPropertyWithConditions::on_hover(ink(FLORA_INK)).in_theme("flora");
    assert!(hover.is_light_half());
    assert_eq!(hover.pseudo_state_conditions(), vec![PseudoStateType::Hover]);

    // A widget pinned to one theme (`with_theme`) uses that theme's block WITHOUT the name, so
    // it applies whatever the app theme is.
    let pinned = dark.without_theme_names();
    assert_eq!(
        pinned.apply_if.as_slice(),
        &[DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark)][..]
    );
    assert!(pinned.theme_names().is_empty());
}

/// The declaration-string parser (`@theme dark { ... }` inside a node's inline style) knows the
/// app theme too, in both spellings.
#[test]
fn the_inline_declaration_parser_reads_a_theme_name() {
    for css in ["@theme flora { color: red; }", "@theme(flora) { color: red; }"] {
        let parsed = CssPropertyWithConditionsVec::parse(css);
        let props = parsed.as_slice();
        assert_eq!(props.len(), 1, "{css}: one declaration");
        assert_eq!(props[0].apply_if.as_slice(), &[named("flora")][..], "{css}");
    }
    let dark = CssPropertyWithConditionsVec::parse("@theme(dark) { color: red; }");
    assert_eq!(
        dark.as_slice()[0].apply_if.as_slice(),
        &[DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark)][..]
    );
}
