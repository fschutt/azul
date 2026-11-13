//! Widgets that FOLLOW the app theme: a widget with no theme of its own
//! (`with_theme` never called) is built in the look of the theme the DOM is
//! built for ([`UiTheme::current`]: the window's app theme while the engine
//! builds its DOM), exactly as if it were pinned to that theme.
//!
//! The DOM carries that one look and no `@theme(<name>)` block of another:
//! a theme switch rebuilds every window's DOM (`CallbackInfo::set_theme`
//! re-runs each window's `layout()` with `RelayoutReason::ThemeChange`), so
//! the other theme's declarations could never apply - building them, and
//! merging them node by node into the live look, was half of every rebuild
//! of a window full of widgets for nothing.
//!
//! # Three shapes of widget
//!
//! - [`follow_app_theme`]: a widget whose flat and flora looks are two
//!   builders (`themes::flat::x`, `themes::flora::x`) - the `None` arm of its
//!   `dom()` - is built by the live theme's builder alone.
//! - [`live_theme`]: a widget built from a skin or a look struct picks the
//!   live theme's (the pinned one, or the one the DOM is built for) and is
//!   built once from it, as a pinned widget is.
//! - [`skins_of`] / [`part_of`]: the same for the skin-built widgets that
//!   keep their skins in a list (the chart, the gauge, the money input, the
//!   date range picker).
//!
//! Pinned widgets (`with_theme`) look the same under every app theme.

use alloc::vec::Vec;

use azul_core::dom::Dom;
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};

use super::{OptionUiTheme, UiTheme};

/// The theme a widget whose theme option is `theme` is built in: the pinned
/// one, or - unpinned - the theme the DOM is built for ([`UiTheme::current`]).
#[must_use]
pub(crate) fn live_theme(theme: OptionUiTheme) -> UiTheme {
    theme.into_option().unwrap_or_else(UiTheme::current)
}

/// Builds `widget` with the builder of the theme the DOM is being built for
/// ([`UiTheme::current`]) - the `None` arm of a two-builder widget's `dom()`.
/// The other theme's builder is never called.
#[must_use]
pub(crate) fn follow_app_theme<W>(widget: W, flat: fn(W) -> Dom, flora: fn(W) -> Dom) -> Dom {
    match UiTheme::current() {
        UiTheme::Flat => flat(widget),
        UiTheme::Flora => flora(widget),
    }
}

/// `extra` stacked onto `base` - one node's style from two parts, `extra`
/// overriding `base` where they collide (the later declaration wins). THE
/// way a widget puts a state part (active, gap, selected) after its base
/// part.
#[must_use]
pub(crate) fn stack_parts(
    base: &CssPropertyWithConditionsVec,
    extra: &CssPropertyWithConditionsVec,
) -> CssPropertyWithConditionsVec {
    if extra.as_ref().is_empty() {
        return base.clone();
    }
    let mut v: Vec<CssPropertyWithConditions> = base.as_ref().to_vec();
    v.extend_from_slice(extra.as_ref());
    CssPropertyWithConditionsVec::from_vec(v)
}

// ---------------------------------------------------------------------------
// Skin-built widgets: their skin, and one part of it
// ---------------------------------------------------------------------------
//
// A widget whose look is a SKIN struct (`themes::flat::x_skin()` /
// `themes::flora::x_skin()`) builds its DOM ONCE from the parts of the live
// theme's skin ([`live_theme`]): the chart, the gauge, the money input, the
// date range picker, the reference picker.

/// The skin a skin-built widget whose theme option is `theme` is drawn with,
/// as the one-skin list [`part_of`] and [`structure_skin`] read: the pinned
/// theme's, or the one of the theme the DOM is built for. Only that skin is
/// built.
#[must_use]
pub(crate) fn skins_of<S>(theme: OptionUiTheme, flat: fn() -> S, flora: fn() -> S) -> Vec<S> {
    alloc::vec![match live_theme(theme) {
        UiTheme::Flat => flat(),
        UiTheme::Flora => flora(),
    }]
}

/// The skin whose STRUCTURE the DOM carries (its theme marker class, its
/// metrics a builder reads as numbers): the one skin of `skins`.
#[must_use]
pub(crate) const fn structure_skin<S>(skins: &[S], _theme: OptionUiTheme) -> Option<&S> {
    skins.first()
}

/// One part, built from the skin by `f`.
#[must_use]
pub(crate) fn part_of<S>(
    skins: &[S],
    f: impl Fn(&S) -> Vec<CssPropertyWithConditions>,
) -> CssPropertyWithConditionsVec {
    skins.first().map_or_else(
        || CssPropertyWithConditionsVec::from_const_slice(&[]),
        |skin| CssPropertyWithConditionsVec::from_vec(f(skin)),
    )
}

// ---------------------------------------------------------------------------
// Test helpers: the contract, asked the same way for every widget
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod checks {
    use alloc::{format, string::String, vec::Vec};

    use azul_core::{app_theme::ThemeScope, dom::Dom};
    use azul_css::{
        css::{Css, CssDeclaration, CssRuleBlock},
        dynamic_selector::{DynamicSelector, DynamicSelectorVec, ThemeCondition},
        props::property::CssProperty,
        AzString,
    };

    use crate::widgets::themes::{theme_checks, UiTheme};

    /// Both widget themes, in block order.
    pub(crate) const BOTH: [UiTheme; 2] = [UiTheme::Flat, UiTheme::Flora];

    /// Run `f` while the DOM being built is for the app theme `theme` - what
    /// the engine does around every DOM build of a window
    /// (`azul_core::app_theme::ThemeScope`). Never `set_app_theme`: the
    /// test binary runs in parallel.
    pub(crate) fn under<T>(theme: UiTheme, f: impl FnOnce() -> T) -> T {
        let _scope = ThemeScope::enter(AzString::from_const_str(theme.name()));
        f()
    }

    /// `css`'s rules as the app theme `theme` sees them
    /// (`theme_checks::live_conditions`): a rule inside another theme's block
    /// is dropped and the live theme's name is stripped from the rest, so what
    /// remains compares equal to the rules of a widget pinned to `theme`.
    pub(crate) fn live_rules(css: &Css, theme: UiTheme) -> Vec<CssRuleBlock> {
        css.rules
            .as_slice()
            .iter()
            .filter_map(|rule| {
                let conditions = theme_checks::live_conditions(&rule.conditions, theme)?;
                let mut live = rule.clone();
                live.conditions = conditions;
                Some(live)
            })
            .collect()
    }

    /// `node`'s inline declarations as the app theme the test builds for sees
    /// them (`theme_checks::probe_theme`: flat, unless [`under`] says otherwise):
    /// the other theme's block dropped, the live theme's name stripped. A
    /// widget's older tests read its nodes through this, so they ask a widget
    /// that follows the app theme exactly what they asked a pinned one.
    pub(crate) fn live_inline(node: &Dom) -> Vec<(CssProperty, DynamicSelectorVec)> {
        live_style(&node.root.style)
    }

    /// [`live_inline`] for a node's style on its own.
    pub(crate) fn live_style(style: &Css) -> Vec<(CssProperty, DynamicSelectorVec)> {
        let mut out = Vec::new();
        for rule in live_rules(style, theme_checks::probe_theme()) {
            for d in rule.declarations.as_slice() {
                if let CssDeclaration::Static(p) = d {
                    out.push((p.clone(), rule.conditions.clone()));
                }
            }
        }
        out
    }

    /// [`live_inline`] without the conditions: the properties, in order.
    pub(crate) fn live_properties(node: &Dom) -> Vec<CssProperty> {
        live_inline(node).into_iter().map(|(p, _)| p).collect()
    }

    /// The app-theme names the rules of `dom` are conditioned on (inline
    /// styles and component sheets), each once.
    pub(crate) fn theme_names(dom: &Dom) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (_, node) in theme_checks::nodes(dom) {
            let sheets = core::iter::once(&node.root.style).chain(node.css.as_slice().iter());
            for css in sheets {
                for rule in css.rules.as_slice() {
                    for c in rule.conditions.as_slice() {
                        if let DynamicSelector::Theme(ThemeCondition::Custom(name)) = c {
                            if !out.iter().any(|n| n.as_str() == name.as_str()) {
                                out.push(String::from(name.as_str()));
                            }
                        }
                    }
                }
            }
        }
        out
    }

    /// Each node's style as `theme` sees it: its inline rules, then each of
    /// its component sheets that has a live rule (a sheet of the other
    /// theme's look is wholly inside that theme's block).
    fn styles(dom: &Dom, theme: UiTheme) -> Vec<Vec<Vec<CssRuleBlock>>> {
        theme_checks::nodes(dom)
            .into_iter()
            .map(|(_, node)| {
                let mut style = alloc::vec![live_rules(&node.root.style, theme)];
                style.extend(
                    node.css
                        .as_slice()
                        .iter()
                        .map(|sheet| live_rules(sheet, theme))
                        .filter(|rules| !rules.is_empty()),
                );
                style
            })
            .collect()
    }

    /// One line per node - its path, type and classes - and its style as
    /// `theme` sees it.
    fn look(dom: &Dom, theme: UiTheme) -> Vec<(String, Vec<Vec<CssRuleBlock>>)> {
        theme_checks::nodes(dom)
            .into_iter()
            .zip(styles(dom, theme))
            .map(|((path, node), style)| {
                (
                    format!(
                        "{path} {:?} {:?}",
                        node.root.get_node_type(),
                        node.root.get_ids_and_classes()
                    ),
                    style,
                )
            })
            .collect()
    }

    /// The migration's contract for one widget (T1 report, section 4 item 6).
    /// `follow` builds the widget with NO theme; `pinned(t)` builds it with
    /// `with_theme(t)`.
    ///
    /// 1. Under the app theme T the followed widget IS the pinned T one: the
    ///    same tree, types and classes (the marker included), and per node
    ///    exactly pinned T's rules, in order.
    /// 2. Its accessibility tree is pinned T's, and the same under both
    ///    themes.
    /// 3. Every dark twin pairs with a light half inside its own theme block.
    /// 4. A pinned widget ignores the app theme.
    /// 5. It carries no app-theme block: one look, built once (a theme
    ///    switch rebuilds every window).
    pub(crate) fn assert_follows_the_app_theme(
        what: &str,
        follow: impl Fn() -> Dom,
        pinned: impl Fn(UiTheme) -> Dom,
    ) {
        for theme in BOTH {
            let name = theme.name();
            let (followed, pin) = under(theme, || (follow(), pinned(theme)));
            let got = look(&followed, theme);
            let want = look(&pin, theme);
            assert_eq!(
                got.len(),
                want.len(),
                "{what} under the app theme {name}: {} nodes, the pinned {name} one has {}",
                got.len(),
                want.len()
            );
            for (g, w) in got.iter().zip(&want) {
                assert_eq!(
                    g, w,
                    "{what} under the app theme {name}: a node does not render as the pinned \
                     {name} widget's"
                );
            }
            assert_eq!(
                theme_checks::a11y_outline(&followed),
                theme_checks::a11y_outline(&pin),
                "{what} under the app theme {name}: the accessibility tree is not the pinned one"
            );
            let halves = under(theme, || theme_checks::half_pairs(&followed));
            assert!(
                halves.is_empty(),
                "{what} under the app theme {name}: half pairs:\n  {}",
                halves.join("\n  ")
            );
        }

        for pin in BOTH {
            let in_flat = under(UiTheme::Flat, || pinned(pin));
            let in_flora = under(UiTheme::Flora, || pinned(pin));
            assert_eq!(
                look(&in_flat, pin),
                look(&in_flora, pin),
                "{what} pinned to {} changes with the app theme",
                pin.name()
            );
            assert!(
                theme_names(&in_flat).is_empty(),
                "{what} pinned to {} carries app-theme blocks: {:?}",
                pin.name(),
                theme_names(&in_flat)
            );
        }

        let a11y: Vec<Vec<String>> = BOTH
            .iter()
            .map(|t| under(*t, || theme_checks::a11y_outline(&follow())))
            .collect();
        assert_eq!(
            a11y[0], a11y[1],
            "{what}: the accessibility tree changes with the app theme"
        );

        for theme in BOTH {
            let names = under(theme, || theme_names(&follow()));
            assert!(
                names.is_empty(),
                "{what} built for {}: carries app-theme blocks {names:?} - a widget that \
                 follows the app theme is built in the live theme's look only",
                theme.name()
            );
        }
    }
}


#[cfg(test)]
mod tests {
    use core::cell::Cell;

    use azul_core::dom::{Dom, IdOrClass, IdOrClassVec};
    use azul_css::AzString;

    use super::{checks, follow_app_theme, live_theme, skins_of};
    use crate::widgets::themes::{OptionUiTheme, UiTheme};

    std::thread_local! {
        static BUILT: Cell<(u32, u32)> = const { Cell::new((0, 0)) };
    }

    fn flat_look(name: &'static str) -> Dom {
        BUILT.with(|b| b.set((b.get().0 + 1, b.get().1)));
        Dom::create_div().with_ids_and_classes(IdOrClassVec::from_vec(alloc::vec![
            IdOrClass::Class(AzString::from_const_str(name)),
        ]))
    }

    fn flora_look(name: &'static str) -> Dom {
        BUILT.with(|b| b.set((b.get().0, b.get().1 + 1)));
        Dom::create_div().with_ids_and_classes(IdOrClassVec::from_vec(alloc::vec![
            IdOrClass::Class(AzString::from_const_str(name)),
        ]))
    }

    /// An unpinned widget costs ONE build, in the live theme's builder: the
    /// other theme's look is never built (a theme switch rebuilds the DOM).
    #[test]
    fn an_unpinned_widget_is_built_once_in_the_live_theme() {
        for theme in checks::BOTH {
            BUILT.with(|b| b.set((0, 0)));
            let dom = checks::under(theme, || follow_app_theme("x", flat_look, flora_look));
            let want = match theme {
                UiTheme::Flat => (1, 0),
                UiTheme::Flora => (0, 1),
            };
            assert_eq!(BUILT.with(Cell::get), want, "built under {theme:?}");
            assert!(checks::theme_names(&dom).is_empty(), "{theme:?}");
        }
    }

    /// The live theme of an unpinned widget is the one the DOM is built for;
    /// a pin wins; a theme no widget theme knows builds the default look.
    #[test]
    fn the_live_theme_is_the_pin_else_the_theme_the_dom_is_built_for() {
        for theme in checks::BOTH {
            assert_eq!(checks::under(theme, || live_theme(OptionUiTheme::None)), theme);
            for pin in checks::BOTH {
                assert_eq!(checks::under(theme, || live_theme(OptionUiTheme::Some(pin))), pin);
            }
        }
        let _scope = azul_core::app_theme::ThemeScope::enter(AzString::from_const_str("monokai"));
        assert_eq!(live_theme(OptionUiTheme::None), UiTheme::default());
    }

    /// A skin-built widget builds only the live theme's skin.
    #[test]
    fn a_skin_built_widget_builds_one_skin() {
        fn flat() -> &'static str {
            "flat"
        }
        fn flora() -> &'static str {
            "flora"
        }
        assert_eq!(checks::under(UiTheme::Flora, || skins_of(OptionUiTheme::None, flat, flora)), ["flora"]);
        assert_eq!(checks::under(UiTheme::Flat, || skins_of(OptionUiTheme::None, flat, flora)), ["flat"]);
        assert_eq!(
            checks::under(UiTheme::Flat, || skins_of(OptionUiTheme::Some(UiTheme::Flora), flat, flora)),
            ["flora"]
        );
    }
}
