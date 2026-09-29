//! The THEME CHAIN (scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md §7.1): the
//! active app themes, most specific first - `AZ_THEME=xyz:pink` is `[xyz:pink, xyz, <default>]`.
//! Which `@theme(<name>)` blocks it makes live:
//!
//! * PREFIX, by `:` segment (the `LanguageCondition::Prefix` precedent): `@theme(xyz)` is live
//!   under a chain holding `xyz` or a spin-off of it (`xyz:pink`), never under `xyzzy`.
//! * The compiled-in themes (`flat`, `flora`, later `native`) are EXCLUSIVE FLOORS: every widget
//!   carries a complete block for each, so only the FIRST of them in the chain is live. Under
//!   `[flora, flat]` (flora, with the app default as the implicit last entry) flat's blocks must
//!   not fill the gaps of flora's look - flora draws no `border`, and would inherit flat's.
//! * A theme no widget knows (`[abc, flat]`) shows the flat floor, not nothing.
//!
//! The contexts here set the chain directly: building it (`AZ_THEME`, `:` expansion, `fallback:`
//! headers) is a separate step; the matcher only consumes it.

use azul_css::{
    dynamic_selector::{
        CssPropertyWithConditions, DynamicSelector, DynamicSelectorContext, ThemeCondition,
    },
    props::{basic::color::ColorU, property::CssProperty, style::StyleTextColor},
    AzString, StringVec,
};

fn named(name: &str) -> DynamicSelector {
    DynamicSelector::Theme(ThemeCondition::Custom(AzString::from(name.to_string())))
}

fn chain(names: &[&str]) -> DynamicSelectorContext {
    let mut ctx = DynamicSelectorContext::default();
    ctx.theme_chain = StringVec::from_vec(
        names
            .iter()
            .map(|n| AzString::from((*n).to_string()))
            .collect(),
    );
    ctx
}

fn live(name: &str, names: &[&str]) -> bool {
    named(name).matches(&chain(names))
}

#[test]
fn a_theme_block_is_live_under_its_spin_offs() {
    assert!(live("xyz", &["xyz:pink"]), "xyz:pink is a spin-off of xyz");
    assert!(live("xyz", &["xyz:pink:night"]), "any depth of spin-off");
    assert!(live("xyz:pink", &["xyz:pink:night"]));
    assert!(live("xyz:pink", &["xyz:pink"]));
    assert!(live("xyz", &["abc", "xyz:pink"]), "anywhere in the chain");
}

#[test]
fn a_theme_name_matches_by_segment_not_by_letters() {
    assert!(!live("xyz", &["xyzzy"]), "xyz is not a prefix SEGMENT of xyzzy");
    assert!(!live("xyz", &["xyzzy:pink"]));
    assert!(!live("xyz:pink", &["xyz:pinkish"]));
    assert!(!live("xyz:pink", &["xyz"]), "a spin-off is not live under its base");
    assert!(!live("pink", &["xyz:pink"]), "a later segment is not a prefix");
    assert!(!live("xy", &["xyz"]));
}

#[test]
fn only_the_first_compiled_in_theme_of_a_chain_is_live() {
    assert!(live("flora", &["flora", "flat"]));
    assert!(
        !live("flat", &["flora", "flat"]),
        "flat is a complete look of its own: under flora it must not fill flora's gaps"
    );
    assert!(live("flat", &["flat", "flora"]));
    assert!(!live("flora", &["flat", "flora"]));
    assert!(!live("native", &["flora", "native", "flat"]));
    // A spin-off of a compiled-in theme names it as its structural base.
    assert!(live("flora", &["flora:abc", "flat"]));
    assert!(!live("flat", &["flora:abc", "flat"]));
}

#[test]
fn user_themes_layer_on_top_of_the_compiled_in_floor() {
    for name in ["abc", "flat"] {
        assert!(live(name, &["abc", "flat"]), "{name} under [abc, flat]");
    }
    assert!(!live("flora", &["abc", "flat"]));
    for name in ["xyz:pink", "xyz", "flora"] {
        assert!(live(name, &["xyz:pink", "xyz", "flora", "flat"]), "{name}");
    }
    assert!(!live("flat", &["xyz:pink", "xyz", "flora", "flat"]));
}

/// A widget's const statics under a chain holding a theme no widget knows: the flat floor's
/// values, so an unknown theme looks like the default, not like nothing.
#[test]
fn an_unknown_theme_shows_the_flat_floor() {
    const FLAT_INK: ColorU = ColorU::rgb(0x01, 0x02, 0x03);
    const FLORA_INK: ColorU = ColorU::rgb(0x10, 0x20, 0x30);
    const fn ink(c: ColorU) -> CssProperty {
        CssProperty::const_text_color(StyleTextColor { inner: c })
    }
    static WIDGET_INK: &[CssPropertyWithConditions] = &[
        CssPropertyWithConditions::with_single_condition(
            ink(FLAT_INK),
            azul_css::theme_conditions!("flat"),
        ),
        CssPropertyWithConditions::with_single_condition(
            ink(FLORA_INK),
            azul_css::theme_conditions!("flora"),
        ),
    ];
    let ctx = chain(&["abc", "flat"]);
    let live: Vec<ColorU> = WIDGET_INK
        .iter()
        .filter(|p| p.matches(&ctx))
        .filter_map(|p| match &p.property {
            CssProperty::TextColor(v) => v.get_property().map(|c| c.inner),
            _ => None,
        })
        .collect();
    assert_eq!(live, vec![FLAT_INK]);
}

/// The context's own answer agrees with the matcher.
#[test]
fn has_app_theme_is_the_matchers_answer() {
    let ctx = chain(&["xyz:pink", "flora", "flat"]);
    for (name, want) in [
        ("xyz:pink", true),
        ("xyz", true),
        ("flora", true),
        ("flat", false),
        ("xyzzy", false),
    ] {
        assert_eq!(ctx.has_app_theme(name), want, "{name}");
        assert_eq!(named(name).matches(&ctx), want, "{name}");
    }
}
