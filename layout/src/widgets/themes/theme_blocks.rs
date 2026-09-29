//! Widgets that FOLLOW the app theme: one DOM carrying every widget theme's
//! declarations, each inside its `@theme(<name>)` block, so the cascade's
//! dynamic matching keeps the app theme's (`AppConfig::with_theme`,
//! `CallbackInfo::set_theme`) and no widget needs a `with_theme`
//! (scripts/T1_APP_THEME_2026_09_29.md section 4, the migration recipe).

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
        let mut out = Vec::new();
        for rule in live_rules(&node.root.style, UiTheme::current()) {
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
    /// its component sheets.
    fn styles(dom: &Dom, theme: UiTheme) -> Vec<Vec<Vec<CssRuleBlock>>> {
        theme_checks::nodes(dom)
            .into_iter()
            .map(|(_, node)| {
                core::iter::once(&node.root.style)
                    .chain(node.css.as_slice().iter())
                    .map(|css| live_rules(css, theme))
                    .collect()
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
