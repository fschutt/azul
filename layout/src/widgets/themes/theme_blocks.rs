//! Widgets that FOLLOW the app theme: one DOM carrying every widget theme's
//! declarations, each inside its `@theme(<name>)` block, so the cascade's
//! dynamic matching keeps the app theme's (`AppConfig::with_theme`,
//! `CallbackInfo::set_theme`) and no widget needs a `with_theme`
//! (scripts/T1_APP_THEME_2026_09_29.md section 4, the migration recipe).
//!
//! A widget with `theme: None` builds BOTH pinned looks - its
//! `themes::flat::x` and `themes::flora::x`, exactly today's generators - and
//! hands them to [`every_theme_dom`], which returns ONE tree:
//!
//! - the STRUCTURE (nodes, text, classes and theme marker, callbacks, a11y) is
//!   that of the theme the DOM is built for ([`UiTheme::current`]; a theme
//!   switch rebuilds the DOM, T1 section 2.5);
//! - each node's style is [`every_theme_css`] of its two looks: the leading
//!   rules both themes share, declared once and unconditionally (display,
//!   flex direction, padding...), then flat's other rules inside
//!   `@theme(flat)`, then flora's inside `@theme(flora)`. Under the app theme
//!   T the live rules are therefore exactly pinned T's, in pinned T's order -
//!   the sharing only ever takes a common PREFIX, so no rule moves past
//!   another;
//! - a subtree that is the same in both looks (the caller's content, a part
//!   the two themes draw alike) is kept once, as it is.
//!
//! Pinned widgets (`with_theme`) never come here: their look stays
//! unconditioned, so the pin holds under any app theme.

use alloc::vec::Vec;

use azul_core::dom::Dom;
use azul_css::{
    css::{Css, CssDeclaration, CssRuleBlock},
    dynamic_selector::{DynamicSelector, DynamicSelectorVec, ThemeCondition},
    props::property::CssPropertyType,
    AzString,
};

use super::UiTheme;

/// The condition of `theme`'s block: `@theme(<name>)`.
#[must_use]
pub(crate) fn theme_condition(theme: UiTheme) -> DynamicSelector {
    DynamicSelector::Theme(ThemeCondition::Custom(AzString::from_const_str(
        theme.name(),
    )))
}

/// `rule` inside `theme`'s block. The theme name goes FIRST, its own
/// conditions (dark, `:hover`, ...) after it: they conjoin, and the compact
/// cache reads a node's first non-pseudo condition (T1 section 4).
fn in_block(mut rule: CssRuleBlock, theme: UiTheme) -> CssRuleBlock {
    let own = core::mem::take(&mut rule.conditions).into_library_owned_vec();
    let mut conditions = Vec::with_capacity(own.len() + 1);
    conditions.push(theme_condition(theme));
    conditions.extend(own);
    rule.conditions = DynamicSelectorVec::from_vec(conditions);
    rule
}

/// A whole sheet inside `theme`'s block (its `@keyframes` stay as they are:
/// a track is named, not conditioned).
fn in_theme_css(css: Css, theme: UiTheme) -> Css {
    let Css { rules, keyframes } = css;
    Css {
        rules: rules
            .into_library_owned_vec()
            .into_iter()
            .map(|r| in_block(r, theme))
            .collect::<Vec<CssRuleBlock>>()
            .into(),
        keyframes,
    }
}

/// Every property type that `rules` declares a DARK twin of.
fn dark_twinned(rules: &[CssRuleBlock], out: &mut Vec<CssPropertyType>) {
    for rule in rules {
        let dark = rule
            .conditions
            .as_slice()
            .iter()
            .any(|c| matches!(c, DynamicSelector::Theme(ThemeCondition::Dark)));
        if dark {
            out.extend(rule.declarations.as_slice().iter().map(CssDeclaration::get_type));
        }
    }
}

/// How many LEADING rules the two looks share and can declare once,
/// unconditionally: equal, unconditional, and not of a property either look
/// gives a dark twin (that twin sits in its theme's block and must find its
/// light half there - `widgets::theme_pairs`). A prefix, so under either
/// theme no rule changes place relative to another.
fn shared_prefix(flat: &[CssRuleBlock], flora: &[CssRuleBlock]) -> usize {
    let mut twinned = Vec::new();
    dark_twinned(flat, &mut twinned);
    dark_twinned(flora, &mut twinned);
    flat.iter()
        .zip(flora)
        .take_while(|(a, b)| {
            a == b
                && a.conditions.as_slice().is_empty()
                && a
                    .declarations
                    .as_slice()
                    .iter()
                    .all(|d| !twinned.contains(&d.get_type()))
        })
        .count()
}

/// One node's style in every theme: the rules both looks share (a leading
/// run, see [`shared_prefix`]) once, then `flat`'s other rules inside
/// `@theme(flat)`, then `flora`'s inside `@theme(flora)`. Two equal looks are
/// the node's style in every theme and come back as they are. `@keyframes`
/// are named, not conditioned: `structure`'s come first, the other look's
/// only under a name `structure` does not use.
#[must_use]
pub(crate) fn every_theme_css(structure: UiTheme, flat: Css, flora: Css) -> Css {
    if flat == flora {
        return flat;
    }
    let Css {
        rules: flat_rules,
        keyframes: flat_keyframes,
    } = flat;
    let Css {
        rules: flora_rules,
        keyframes: flora_keyframes,
    } = flora;
    let flat_rules = flat_rules.into_library_owned_vec();
    let flora_rules = flora_rules.into_library_owned_vec();

    let shared = shared_prefix(&flat_rules, &flora_rules);
    let mut rules = Vec::with_capacity(flat_rules.len() + flora_rules.len() - shared);
    let mut flat_rules = flat_rules.into_iter();
    rules.extend(flat_rules.by_ref().take(shared));
    rules.extend(flat_rules.map(|r| in_block(r, UiTheme::Flat)));
    rules.extend(
        flora_rules
            .into_iter()
            .skip(shared)
            .map(|r| in_block(r, UiTheme::Flora)),
    );

    let (own_keyframes, others) = match structure {
        UiTheme::Flat => (flat_keyframes, flora_keyframes),
        UiTheme::Flora => (flora_keyframes, flat_keyframes),
    };
    let mut keyframes = own_keyframes.into_library_owned_vec();
    for track in others.into_library_owned_vec() {
        if !keyframes.iter().any(|k| k.name == track.name) {
            keyframes.push(track);
        }
    }

    Css {
        rules: rules.into(),
        keyframes: keyframes.into(),
    }
}

/// The widget's DOM in every theme, from its two pinned looks: the tree of
/// the theme the DOM is built for ([`UiTheme::current`]), each node's style
/// [`every_theme_css`] of the node's two looks. What a widget whose `theme`
/// is `None` returns.
#[must_use]
pub(crate) fn every_theme_dom(flat: Dom, flora: Dom) -> Dom {
    let mut dom = every_theme_dom_for(UiTheme::current(), flat, flora);
    let _ = dom.fixup_children_estimated();
    dom
}

/// [`every_theme_dom`] with the structural theme given.
///
/// Nodes pair by position. Where the two looks' trees part (a child count
/// differs), the rest of the subtree is `structure`'s own, as it is: that
/// part exists in one theme only, and a theme switch rebuilds the DOM. A
/// mis-paired node can only ever carry DEAD declarations - the other
/// theme's block never matches while the tree is `structure`'s.
#[must_use]
pub(crate) fn every_theme_dom_for(structure: UiTheme, flat: Dom, flora: Dom) -> Dom {
    if flat == flora {
        return flat;
    }
    let (mut own, mut other) = match structure {
        UiTheme::Flat => (flat, flora),
        UiTheme::Flora => (flora, flat),
    };
    // `structure`'s value and the other theme's, back in (flat, flora) order.
    fn pair<T>(structure: UiTheme, mine: T, theirs: T) -> (T, T) {
        match structure {
            UiTheme::Flat => (mine, theirs),
            UiTheme::Flora => (theirs, mine),
        }
    }

    let (flat_style, flora_style) = pair(
        structure,
        core::mem::take(&mut own.root.style),
        core::mem::take(&mut other.root.style),
    );
    own.root.style = every_theme_css(structure, flat_style, flora_style);

    // The node's own component sheets (`with_css`, `widget_p`'s reset):
    // pairwise when both looks attach as many, else each look's sheets whole,
    // every rule inside its theme's block.
    if own.css != other.css {
        let (flat_sheets, flora_sheets) = pair(
            structure,
            core::mem::take(&mut own.css).into_library_owned_vec(),
            core::mem::take(&mut other.css).into_library_owned_vec(),
        );
        own.css = if flat_sheets.len() == flora_sheets.len() {
            flat_sheets
                .into_iter()
                .zip(flora_sheets)
                .map(|(f, fl)| every_theme_css(structure, f, fl))
                .collect::<Vec<Css>>()
                .into()
        } else {
            flat_sheets
                .into_iter()
                .map(|s| in_theme_css(s, UiTheme::Flat))
                .chain(flora_sheets.into_iter().map(|s| in_theme_css(s, UiTheme::Flora)))
                .collect::<Vec<Css>>()
                .into()
        };
    }

    let mine = core::mem::take(&mut own.children).into_library_owned_vec();
    let theirs = core::mem::take(&mut other.children).into_library_owned_vec();
    own.children = if mine.len() == theirs.len() {
        mine.into_iter()
            .zip(theirs)
            .map(|(m, t)| {
                let (f, fl) = pair(structure, m, t);
                every_theme_dom_for(structure, f, fl)
            })
            .collect::<Vec<Dom>>()
            .into()
    } else {
        mine.into()
    };
    own
}

// ---------------------------------------------------------------------------
// Test helpers: the migration's contract, asked the same way for every widget
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod checks {
    use alloc::{format, string::String, vec::Vec};

    use azul_core::{app_theme::ThemeScope, dom::Dom};
    use azul_css::{
        css::{Css, CssDeclaration, CssRuleBlock},
        dynamic_selector::{
            DynamicSelector, DynamicSelectorContext, DynamicSelectorVec, ThemeCondition,
        },
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

    /// `css`'s rules as the app theme `theme` sees them: a rule inside
    /// another theme's block is dropped - the cascade's own matcher decides,
    /// under a context whose app theme is `theme` - and the live theme's name
    /// is stripped from the rest, so what remains compares equal to the rules
    /// of a widget pinned to `theme`.
    pub(crate) fn live_rules(css: &Css, theme: UiTheme) -> Vec<CssRuleBlock> {
        let ctx = DynamicSelectorContext::default().with_app_theme(theme.name());
        css.rules
            .as_slice()
            .iter()
            .filter_map(|rule| {
                let mut kept = Vec::new();
                for c in rule.conditions.as_slice() {
                    if matches!(c, DynamicSelector::Theme(ThemeCondition::Custom(_))) {
                        if !c.matches(&ctx) {
                            return None;
                        }
                    } else {
                        kept.push(c.clone());
                    }
                }
                let mut live = rule.clone();
                live.conditions = DynamicSelectorVec::from_vec(kept);
                Some(live)
            })
            .collect()
    }

    /// `node`'s inline declarations as the app theme the test builds for sees
    /// them ([`UiTheme::current`]: flat, unless a `ThemeScope` is entered):
    /// the other theme's block dropped, the live theme's name stripped. A
    /// widget's older tests read its nodes through this, so they ask a widget
    /// that follows the app theme exactly what they asked a pinned one.
    pub(crate) fn live_inline(node: &Dom) -> Vec<(CssProperty, DynamicSelectorVec)> {
        live_style(&node.root.style)
    }

    /// [`live_inline`] for a node's style on its own.
    pub(crate) fn live_style(style: &Css) -> Vec<(CssProperty, DynamicSelectorVec)> {
        let mut out = Vec::new();
        for rule in live_rules(style, UiTheme::current()) {
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
    ///    exactly pinned T's rules, in order, once the blocks the cascade's
    ///    matcher throws out are gone.
    /// 2. Its accessibility tree is pinned T's, and the same under both
    ///    themes.
    /// 3. Every dark twin pairs with a light half inside its own theme block.
    /// 4. A pinned widget ignores the app theme.
    /// 5. Where the themes look different, the DOM carries both blocks,
    ///    whichever theme it was built for.
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

        let flat_style = under(UiTheme::Flat, || styles(&pinned(UiTheme::Flat), UiTheme::Flat));
        let flora_style = under(UiTheme::Flora, || {
            styles(&pinned(UiTheme::Flora), UiTheme::Flora)
        });
        if flat_style != flora_style {
            for theme in BOTH {
                let names = under(theme, || theme_names(&follow()));
                assert!(
                    names.iter().any(|n| n == "flat") && names.iter().any(|n| n == "flora"),
                    "{what} built for {}: carries the blocks {names:?}, not both themes'",
                    theme.name()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use azul_core::dom::{Dom, IdOrClass, IdOrClassVec};
    use azul_css::{
        css::Css,
        dynamic_selector::{
            CssPropertyWithConditions as P, CssPropertyWithConditionsVec, DynamicSelector,
            ThemeCondition,
        },
        props::{
            basic::ColorU,
            layout::{LayoutDisplay, LayoutHeight},
            property::CssProperty,
            style::StyleTextColor,
        },
        AzString,
    };

    use super::{checks, every_theme_css, every_theme_dom_for, theme_condition};
    use crate::widgets::themes::UiTheme;

    fn css(props: Vec<P>) -> Css {
        CssPropertyWithConditionsVec::from_vec(props).into()
    }

    fn display() -> P {
        P::simple(CssProperty::const_display(LayoutDisplay::Flex))
    }

    fn height(px: isize) -> P {
        P::simple(CssProperty::const_height(LayoutHeight::const_px(px)))
    }

    fn ink(c: ColorU) -> CssProperty {
        CssProperty::const_text_color(StyleTextColor { inner: c })
    }

    fn classed(name: &'static str) -> Dom {
        Dom::create_div().with_ids_and_classes(IdOrClassVec::from_vec(alloc::vec![
            IdOrClass::Class(AzString::from_const_str(name))
        ]))
    }

    #[test]
    fn two_equal_looks_are_one_unconditioned_style() {
        let look = css(alloc::vec![display(), height(4)]);
        assert_eq!(
            every_theme_css(UiTheme::Flat, look.clone(), look.clone()),
            look
        );
    }

    #[test]
    fn the_shared_lead_is_declared_once_and_the_rest_goes_in_blocks() {
        let flat = css(alloc::vec![display(), height(4)]);
        let flora = css(alloc::vec![display(), height(8)]);
        let merged = every_theme_css(UiTheme::Flat, flat.clone(), flora.clone());
        let rules = merged.rules.as_slice();
        assert_eq!(rules.len(), 3, "display once, then each theme's height");
        assert!(rules[0].conditions.as_slice().is_empty());
        assert_eq!(
            rules[1].conditions.as_slice(),
            &[theme_condition(UiTheme::Flat)]
        );
        assert_eq!(
            rules[2].conditions.as_slice(),
            &[theme_condition(UiTheme::Flora)]
        );
        assert_eq!(
            checks::live_rules(&merged, UiTheme::Flat),
            flat.rules.as_slice().to_vec()
        );
        assert_eq!(
            checks::live_rules(&merged, UiTheme::Flora),
            flora.rules.as_slice().to_vec()
        );
    }

    #[test]
    fn a_blocks_own_conditions_follow_the_theme_name() {
        let flat = css(alloc::vec![P::on_hover(ink(ColorU::rgb(1, 2, 3)))]);
        let flora = css(alloc::vec![P::on_hover(ink(ColorU::rgb(4, 5, 6)))]);
        let merged = every_theme_css(UiTheme::Flat, flat, flora);
        for (rule, theme) in merged.rules.as_slice().iter().zip([UiTheme::Flat, UiTheme::Flora]) {
            let c = rule.conditions.as_slice();
            assert_eq!(c.len(), 2);
            assert_eq!(c[0], theme_condition(theme), "the theme name goes first");
            assert!(matches!(c[1], DynamicSelector::PseudoState(_)));
        }
    }

    #[test]
    fn a_property_with_a_dark_twin_stays_in_its_themes_block() {
        // Equal light halves, different dark twins: the light half must stay
        // in each block, or the twin finds no light half of its theme.
        let white = ColorU::rgb(255, 255, 255);
        let flat = css(alloc::vec![
            P::simple(ink(white)),
            P::dark_theme(ink(ColorU::rgb(1, 1, 1)))
        ]);
        let flora = css(alloc::vec![
            P::simple(ink(white)),
            P::dark_theme(ink(ColorU::rgb(2, 2, 2)))
        ]);
        let merged = every_theme_css(UiTheme::Flat, flat, flora);
        assert!(
            merged.rules.as_slice().iter().all(|r| matches!(
                r.conditions.as_slice().first(),
                Some(DynamicSelector::Theme(ThemeCondition::Custom(_)))
            )),
            "{merged:?}"
        );
    }

    #[test]
    fn the_tree_is_the_structural_themes_and_an_equal_subtree_stays_as_it_is() {
        let content = Dom::create_div().with_css_props(CssPropertyWithConditionsVec::from_vec(
            alloc::vec![P::dark_theme(CssProperty::const_display(LayoutDisplay::Block))],
        ));
        let look = |px: isize, class: &'static str| {
            classed(class)
                .with_css_props(CssPropertyWithConditionsVec::from_vec(alloc::vec![height(px)]))
                .with_child(content.clone())
        };
        for structure in [UiTheme::Flat, UiTheme::Flora] {
            let dom = every_theme_dom_for(structure, look(4, "flat"), look(8, "flora"));
            let want = match structure {
                UiTheme::Flat => "flat",
                UiTheme::Flora => "flora",
            };
            assert!(
                dom.root
                    .get_ids_and_classes()
                    .as_ref()
                    .iter()
                    .any(|c| matches!(c, IdOrClass::Class(s) if s.as_str() == want)),
                "{structure:?}: the root is not the structural theme's"
            );
            assert_eq!(dom.root.style.rules.as_slice().len(), 2, "both heights");
            assert_eq!(dom.children.as_ref()[0], content, "the content is untouched");
        }
    }

    #[test]
    fn where_the_trees_part_the_structural_themes_subtree_is_kept() {
        let flat = classed("flat").with_child(Dom::create_div());
        let flora = classed("flora")
            .with_child(Dom::create_div())
            .with_child(Dom::create_div());
        assert_eq!(
            every_theme_dom_for(UiTheme::Flora, flat.clone(), flora.clone())
                .children
                .as_ref()
                .len(),
            2
        );
        assert_eq!(
            every_theme_dom_for(UiTheme::Flat, flat, flora)
                .children
                .as_ref()
                .len(),
            1
        );
    }

    #[test]
    fn a_sheet_only_one_look_attaches_goes_in_that_themes_block() {
        let flat = Dom::create_div().with_css("color: red;");
        let flora = Dom::create_div()
            .with_css_props(CssPropertyWithConditionsVec::from_vec(alloc::vec![height(3)]));
        let dom = every_theme_dom_for(UiTheme::Flat, flat, flora);
        assert_eq!(dom.css.as_ref().len(), 1);
        for rule in dom.css.as_ref()[0].rules.as_slice() {
            assert_eq!(
                rule.conditions.as_slice().first(),
                Some(&theme_condition(UiTheme::Flat))
            );
        }
        assert!(checks::live_rules(&dom.css.as_ref()[0], UiTheme::Flora).is_empty());
    }
}
