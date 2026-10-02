//! Widgets that FOLLOW the app theme: one DOM carrying every widget theme's
//! declarations, each inside its `@theme(<name>)` block, so the cascade's
//! dynamic matching keeps the app theme's (`AppConfig::with_theme`,
//! `CallbackInfo::set_theme`) and no widget needs a `with_theme`
//! (scripts/T1_APP_THEME_2026_09_29.md section 4, the migration recipe).
//!
//! This is the ONE merge every such widget goes through (the two halves of
//! the migration each wrote one; they are this one now).
//!
//! # Three entry points
//!
//! - [`follow_app_theme`]: a widget whose flat and flora looks are two
//!   builders (`themes::flat::x`, `themes::flora::x`) builds itself both ways
//!   and merges the two DOMs - the `None` arm of its `dom()`.
//! - [`follow_dom`]: the same for two DOMs the caller built (both looks built
//!   around a placeholder, a render callback's tree).
//! - [`follow_props`]: one PART's declarations, for a widget built from a
//!   skin or a look struct: its DOM is built ONCE from the merged parts, and a
//!   container's caller content is never cloned or walked.
//!
//! # The tree
//!
//! The STRUCTURE (nodes, text, classes and theme marker, callbacks, datasets,
//! a11y) is that of the theme the DOM is built for ([`UiTheme::current`]; a
//! theme switch rebuilds the DOM, T1 section 2.5). Nodes pair by position.
//! Where the two trees part (a child count differs), the rest of the subtree
//! is the structure theme's own, as it is: a part only one theme draws
//! (flora's spinner spokes, flat's ring) never has to answer for the other,
//! and the other theme's block could never match while the tree is this
//! one's. Each paired node's inline style and component sheets go through
//! [`follow_css`]; a node both looks style alike keeps its style as it is.
//!
//! # The merge
//!
//! Per PROPERTY. A property both looks declare ALIKE - the same
//! declarations, conditions included, in the same order - is declared once,
//! unconditionally (so it also holds under an app theme no widget knows).
//! Every other declaration goes into its theme's block, `@theme(flat)` or
//! `@theme(flora)`, the theme name FIRST in its conditions (they conjoin,
//! and the compact cache reads a node's first non-pseudo condition).
//!
//! The ORDER is each theme's own. The shared declarations are anchors that
//! appear in both looks in the same order; between two anchors come flat's
//! other declarations (in flat's order), then flora's (in flora's). So under
//! the app theme T the live declarations ARE pinned T's, in pinned T's order -
//! equal, not merely resolving alike, and that is what the tests compare. A
//! dark twin stays with its light half: both are shared, or both sit in the
//! same theme's block (`widgets::theme_pairs` pairs per theme name).
//!
//! Two cases write a property per theme although it is alike:
//!
//! - where the two orders CROSS (flat declares A before B, flora B before A),
//!   sharing both would give one theme the other's order: the property whose
//!   twin is further away gives way;
//! - a rule is never split: a rule block that declares an alike property and
//!   a differing one goes whole into each theme's block, and takes the alike
//!   property's other declarations with it (all of a property's declarations
//!   are shared, or none).
//!
//! Two equal looks - a part, a sheet, a node's style - come back as they are.
//! `@keyframes` are named, not conditioned: the structure theme's come
//! first, the other look's only under a name the structure theme does not
//! use. A component sheet only one look attaches (`with_css`: the flat
//! menubar) goes whole into that theme's block; sheets both attach are
//! merged pairwise.
//!
//! Pinned widgets (`with_theme`) never come here: their look stays
//! unconditioned, so the pin holds under any app theme.

use alloc::vec::Vec;

use azul_core::dom::Dom;
use azul_css::{
    css::{Css, CssDeclaration, CssRuleBlock},
    dynamic_selector::{
        CssPropertyWithConditions, CssPropertyWithConditionsVec, DynamicSelector,
        DynamicSelectorVec, ThemeCondition,
    },
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

/// A rule's `conditions` inside `theme`'s block: the theme name first, the
/// rule's own conditions (dark, `:hover`, ...) after it - what
/// `CssPropertyWithConditions::in_theme` does for a part's declaration.
fn theme_first(conditions: &DynamicSelectorVec, theme: UiTheme) -> DynamicSelectorVec {
    let own = conditions.as_slice();
    let mut out = Vec::with_capacity(own.len() + 1);
    out.push(theme_condition(theme));
    out.extend(own.iter().cloned());
    DynamicSelectorVec::from_vec(out)
}

/// What the merge moves as a whole: one part's declaration, or one rule
/// block (never split).
trait Unit: Clone + PartialEq {
    /// Every property type it declares, pushed onto `out`.
    fn types_into(&self, out: &mut Vec<CssPropertyType>);
    /// Whether it declares a property of type `ty`.
    fn declares(&self, ty: CssPropertyType) -> bool;
    /// It inside `theme`'s block.
    #[must_use]
    fn into_block(self, theme: UiTheme) -> Self;
    /// The conditions it applies under (`@theme(<name>)` among them).
    fn conditions(&self) -> &DynamicSelectorVec;
}

impl Unit for CssPropertyWithConditions {
    fn types_into(&self, out: &mut Vec<CssPropertyType>) {
        out.push(self.property.get_type());
    }

    fn declares(&self, ty: CssPropertyType) -> bool {
        self.property.get_type() == ty
    }

    fn into_block(self, theme: UiTheme) -> Self {
        // The css crate's own helper: the theme name first, then the rest.
        self.in_theme(theme.name())
    }

    fn conditions(&self) -> &DynamicSelectorVec {
        &self.apply_if
    }
}

impl Unit for CssRuleBlock {
    fn types_into(&self, out: &mut Vec<CssPropertyType>) {
        out.extend(
            self.declarations
                .as_slice()
                .iter()
                // A custom-property definition sets no property (R1).
                .filter_map(CssDeclaration::get_type),
        );
    }

    fn declares(&self, ty: CssPropertyType) -> bool {
        self.declarations
            .as_slice()
            .iter()
            .any(|d| d.get_type() == Some(ty))
    }

    fn into_block(mut self, theme: UiTheme) -> Self {
        self.conditions = theme_first(&self.conditions, theme);
        self
    }

    fn conditions(&self) -> &DynamicSelectorVec {
        &self.conditions
    }
}

/// The property types `unit` declares.
fn types_of<T: Unit>(unit: &T) -> Vec<CssPropertyType> {
    let mut out = Vec::new();
    unit.types_into(&mut out);
    out
}

/// Whether `unit` is declared once for both themes: it declares something,
/// and only `shared` properties.
fn is_shared<T: Unit>(unit: &T, shared: &[CssPropertyType]) -> bool {
    let types = types_of(unit);
    !types.is_empty() && types.iter().all(|t| shared.contains(t))
}

/// Where the merged list takes its next declaration from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// The next of both looks (they are equal), once, unconditionally.
    Shared,
    /// The next of flat's, inside `@theme(flat)`.
    Flat,
    /// The next of flora's, inside `@theme(flora)`.
    Flora,
}

/// How `flat` and `flora` merge (module docs, "The merge").
fn plan<T: Unit>(flat: &[T], flora: &[T]) -> Vec<Step> {
    // Every property either look declares, once each.
    let mut types: Vec<CssPropertyType> = Vec::new();
    for unit in flat.iter().chain(flora) {
        for t in types_of(unit) {
            if !types.contains(&t) {
                types.push(t);
            }
        }
    }
    // The properties both looks declare alike.
    let mut shared: Vec<CssPropertyType> = types
        .into_iter()
        .filter(|t| {
            flat.iter()
                .filter(|u| u.declares(*t))
                .eq(flora.iter().filter(|u| u.declares(*t)))
        })
        .collect();
    loop {
        // A rule declaring an alike property and a differing one goes into
        // the blocks whole, and takes the alike property with it.
        let mut mixed = false;
        for unit in flat.iter().chain(flora) {
            let types = types_of(unit);
            if types.iter().any(|t| shared.contains(t))
                && types.iter().any(|t| !shared.contains(t))
            {
                shared.retain(|s| !types.contains(s));
                mixed = true;
            }
        }
        if mixed {
            continue;
        }
        match interleave(flat, flora, &shared) {
            Ok(steps) => return steps,
            // Each round writes at least one more property per theme, so
            // this ends - at the latest with nothing shared.
            Err(gives_way) => shared.retain(|s| !gives_way.contains(s)),
        }
    }
}

/// The steps that keep both looks' orders with the `shared` properties
/// declared once, or - where the two orders cross - the properties that
/// have to give way.
fn interleave<T: Unit>(
    flat: &[T],
    flora: &[T],
    shared: &[CssPropertyType],
) -> Result<Vec<Step>, Vec<CssPropertyType>> {
    let (mut i, mut j) = (0, 0);
    let mut steps = Vec::with_capacity(flat.len() + flora.len());
    loop {
        // Each look's own declarations, up to its next shared one.
        while i < flat.len() && !is_shared(&flat[i], shared) {
            steps.push(Step::Flat);
            i += 1;
        }
        while j < flora.len() && !is_shared(&flora[j], shared) {
            steps.push(Step::Flora);
            j += 1;
        }
        match (flat.get(i), flora.get(j)) {
            (None, None) => return Ok(steps),
            (Some(a), Some(b)) if a == b => {
                steps.push(Step::Shared);
                i += 1;
                j += 1;
            }
            // Both at a shared declaration, but not the same one: the two
            // orders cross here. The one whose twin is further ahead is the
            // one out of place.
            (Some(a), Some(b)) => {
                let a_twin = flora[j..].iter().position(|u| u == a);
                let b_twin = flat[i..].iter().position(|u| u == b);
                let b_gives_way = match (a_twin, b_twin) {
                    (_, None) => true,
                    (None, Some(_)) => false,
                    (Some(x), Some(y)) => y > x,
                };
                return Err(types_of(if b_gives_way { b } else { a }));
            }
            (Some(a), None) => return Err(types_of(a)),
            (None, Some(b)) => return Err(types_of(b)),
        }
    }
}

/// `steps` run over the two looks.
fn apply<T: Unit>(
    steps: &[Step],
    flat: impl IntoIterator<Item = T>,
    flora: impl IntoIterator<Item = T>,
) -> Vec<T> {
    let (mut flat, mut flora) = (flat.into_iter(), flora.into_iter());
    let mut out = Vec::with_capacity(steps.len());
    for step in steps {
        match step {
            Step::Shared => {
                out.extend(flat.next());
                let _twin = flora.next();
            }
            Step::Flat => out.extend(flat.next().map(|u| u.into_block(UiTheme::Flat))),
            Step::Flora => out.extend(flora.next().map(|u| u.into_block(UiTheme::Flora))),
        }
    }
    out
}

/// One part's declarations in BOTH themes, as the list a widget that
/// follows the app theme puts on the part (module docs, "The merge"). What a
/// skin- or look-built widget (a dialog, a tooltip, a frame) builds its
/// follow skin with, part by part. Two equal parts come back as they are.
#[must_use]
pub(crate) fn follow_props(
    flat: &[CssPropertyWithConditions],
    flora: &[CssPropertyWithConditions],
) -> CssPropertyWithConditionsVec {
    if flat == flora {
        return CssPropertyWithConditionsVec::from_vec(flat.to_vec());
    }
    let steps = plan(flat, flora);
    CssPropertyWithConditionsVec::from_vec(apply(
        &steps,
        flat.iter().cloned(),
        flora.iter().cloned(),
    ))
}

/// One sheet - a node's inline style, or one of its component sheets - in
/// BOTH themes (module docs). Two equal sheets come back as they are.
#[must_use]
pub(crate) fn follow_css(structure: UiTheme, flat: Css, flora: Css) -> Css {
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
    let steps = plan(&flat_rules, &flora_rules);
    let rules = apply(&steps, flat_rules, flora_rules);

    let (own_keyframes, others) = mine_first(structure, flat_keyframes, flora_keyframes);
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

/// A whole sheet inside `theme`'s block (its `@keyframes` stay as they are:
/// a track is named, not conditioned).
fn in_theme_css(css: Css, theme: UiTheme) -> Css {
    let Css { rules, keyframes } = css;
    Css {
        rules: rules
            .into_library_owned_vec()
            .into_iter()
            .map(|r| r.into_block(theme))
            .collect::<Vec<CssRuleBlock>>()
            .into(),
        keyframes,
    }
}

/// `flat` and `flora` as (the `structure` theme's, the other theme's) - and,
/// the same swap, a (`structure`'s, other's) pair back as (flat, flora).
fn mine_first<T>(structure: UiTheme, flat: T, flora: T) -> (T, T) {
    match structure {
        UiTheme::Flat => (flat, flora),
        UiTheme::Flora => (flora, flat),
    }
}

/// Gives `built` (a node of the `structure` theme's DOM) the styles of both
/// themes, `other` being the same node as the other theme built it, and
/// recurses into the children the two trees pair.
fn follow_node(built: &mut Dom, mut other: Dom, structure: UiTheme) {
    let (flat, flora) = mine_first(
        structure,
        core::mem::take(&mut built.root.style),
        core::mem::take(&mut other.root.style),
    );
    built.root.style = follow_css(structure, flat, flora);

    // The node's own component sheets (`with_css`, `widget_p`'s reset):
    // pairwise when both looks attach as many, else each look's sheets
    // whole, every rule inside its theme's block.
    if built.css != other.css {
        let (flat, flora) = mine_first(
            structure,
            core::mem::take(&mut built.css).into_library_owned_vec(),
            core::mem::take(&mut other.css).into_library_owned_vec(),
        );
        built.css = if flat.len() == flora.len() {
            flat.into_iter()
                .zip(flora)
                .map(|(f, fl)| follow_css(structure, f, fl))
                .collect::<Vec<Css>>()
                .into()
        } else {
            flat.into_iter()
                .map(|s| in_theme_css(s, UiTheme::Flat))
                .chain(flora.into_iter().map(|s| in_theme_css(s, UiTheme::Flora)))
                .collect::<Vec<Css>>()
                .into()
        };
    }

    let theirs = core::mem::take(&mut other.children).into_library_owned_vec();
    let mine: &mut [Dom] = built.children.as_mut();
    if mine.len() == theirs.len() {
        for (child, twin) in mine.iter_mut().zip(theirs) {
            follow_node(child, twin, structure);
        }
    }
}

/// A widget built by BOTH themes, as the DOM a widget that follows the app
/// theme returns: `structure`'s tree - its nodes, classes (the theme
/// marker), callbacks, datasets and accessibility - with every node carrying
/// both themes' styles (module docs). The other theme's DOM is read for its
/// styles only.
#[must_use]
pub(crate) fn follow_dom(structure: UiTheme, flat: Dom, flora: Dom) -> Dom {
    let (mut built, other) = mine_first(structure, flat, flora);
    follow_node(&mut built, other, structure);
    built
}

/// Builds `widget` with `flat` and with `flora` and merges the two
/// ([`follow_dom`]) in the structure of the theme the DOM is being built for
/// ([`UiTheme::current`]) - the `None` arm of a two-builder widget's `dom()`.
/// The structure theme's build is the one the app gets; the other one is a
/// style-only twin (`widgets::style_only_build`: read for its styles, its
/// warnings silent, so an unnamed slider warns once, not twice).
#[must_use]
pub(crate) fn follow_app_theme<W: Clone>(
    widget: W,
    flat: fn(W) -> Dom,
    flora: fn(W) -> Dom,
) -> Dom {
    use crate::widgets::style_only_build;
    let structure = UiTheme::current();
    let (flat_dom, flora_dom) = match structure {
        UiTheme::Flat => {
            let own = flat(widget.clone());
            (own, style_only_build(|| flora(widget)))
        }
        UiTheme::Flora => {
            let own = flora(widget.clone());
            (style_only_build(|| flat(widget)), own)
        }
    };
    follow_dom(structure, flat_dom, flora_dom)
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

    use super::{checks, follow_css, follow_dom, follow_props, theme_condition};
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
            follow_css(UiTheme::Flat, look.clone(), look.clone()),
            look
        );
    }

    #[test]
    fn the_shared_lead_is_declared_once_and_the_rest_goes_in_blocks() {
        let flat = css(alloc::vec![display(), height(4)]);
        let flora = css(alloc::vec![display(), height(8)]);
        let merged = follow_css(UiTheme::Flat, flat.clone(), flora.clone());
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
        let merged = follow_css(UiTheme::Flat, flat, flora);
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
            P::dark_mode(ink(ColorU::rgb(1, 1, 1)))
        ]);
        let flora = css(alloc::vec![
            P::simple(ink(white)),
            P::dark_mode(ink(ColorU::rgb(2, 2, 2)))
        ]);
        let merged = follow_css(UiTheme::Flat, flat, flora);
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
            alloc::vec![P::dark_mode(CssProperty::const_display(LayoutDisplay::Block))],
        ));
        let look = |px: isize, class: &'static str| {
            classed(class)
                .with_css_props(CssPropertyWithConditionsVec::from_vec(alloc::vec![height(px)]))
                .with_child(content.clone())
        };
        for structure in [UiTheme::Flat, UiTheme::Flora] {
            let dom = follow_dom(structure, look(4, "flat"), look(8, "flora"));
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
            follow_dom(UiTheme::Flora, flat.clone(), flora.clone())
                .children
                .as_ref()
                .len(),
            2
        );
        assert_eq!(
            follow_dom(UiTheme::Flat, flat, flora)
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
        let dom = follow_dom(UiTheme::Flat, flat, flora);
        assert_eq!(dom.css.as_ref().len(), 1);
        for rule in dom.css.as_ref()[0].rules.as_slice() {
            assert_eq!(
                rule.conditions.as_slice().first(),
                Some(&theme_condition(UiTheme::Flat))
            );
        }
        assert!(checks::live_rules(&dom.css.as_ref()[0], UiTheme::Flora).is_empty());
    }

    // ---- U1: one merge for every widget that follows the app theme ----
    //
    // A property both looks declare ALIKE (the same declarations, conditions
    // included, in the same order) is declared once, unconditionally,
    // wherever it sits; every other declaration goes into its theme's block;
    // and under either app theme the live declarations are EXACTLY that
    // theme's own, in that theme's order.

    use azul_css::{
        css::{rule_priority, CssDeclaration, CssPath, CssRuleBlock},
        props::{layout::LayoutPaddingTop, property::CssPropertyType},
    };

    fn pad(px: isize) -> P {
        P::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(px)))
    }

    fn inked(v: u8) -> P {
        P::simple(ink(ColorU::rgb(v, v, v)))
    }

    /// `part` as the rules of a node that carries it.
    fn rules_of(part: &[P]) -> Vec<CssRuleBlock> {
        css(part.to_vec()).rules.as_slice().to_vec()
    }

    /// The declarations of `merged` the app theme `theme` sees, as rules.
    fn live_part(merged: &[P], theme: UiTheme) -> Vec<CssRuleBlock> {
        checks::live_rules(&css(merged.to_vec()), theme)
    }

    /// How many of `merged`'s declarations of `ty` sit in no theme's block.
    fn unconditional(merged: &[P], ty: CssPropertyType) -> usize {
        merged
            .iter()
            .filter(|p| p.property.get_type() == ty && p.theme_names().is_empty())
            .count()
    }

    fn assert_each_theme_sees_its_own(merged: &[P], flat: &[P], flora: &[P]) {
        assert_eq!(
            live_part(merged, UiTheme::Flat),
            rules_of(flat),
            "under flat: {merged:?}"
        );
        assert_eq!(
            live_part(merged, UiTheme::Flora),
            rules_of(flora),
            "under flora: {merged:?}"
        );
    }

    fn rule(declarations: Vec<CssProperty>) -> CssRuleBlock {
        CssRuleBlock {
            path: CssPath {
                selectors: Vec::new().into(),
            },
            declarations: declarations
                .into_iter()
                .map(CssDeclaration::Static)
                .collect::<Vec<CssDeclaration>>()
                .into(),
            conditions: Vec::new().into(),
            priority: rule_priority::INLINE,
        }
    }

    #[test]
    fn a_followed_part_under_either_theme_is_exactly_that_themes_part_in_order() {
        // The ink is alike in both parts, but flat declares it after the
        // height and flora before it.
        let flat = alloc::vec![display(), height(4), inked(9)];
        let flora = alloc::vec![display(), inked(9), height(8)];
        let merged = follow_props(&flat, &flora);
        let merged = merged.as_slice();
        assert_each_theme_sees_its_own(merged, &flat, &flora);
        assert_eq!(unconditional(merged, CssPropertyType::Display), 1, "{merged:?}");
        assert_eq!(unconditional(merged, CssPropertyType::TextColor), 1, "{merged:?}");
        assert_eq!(
            merged.len(),
            4,
            "display and the ink once, each height in its block: {merged:?}"
        );
    }

    #[test]
    fn a_property_both_looks_declare_alike_is_declared_once_even_after_a_difference() {
        let flat = alloc::vec![height(4), display()];
        let flora = alloc::vec![height(8), display()];
        let merged = follow_css(UiTheme::Flat, css(flat.clone()), css(flora.clone()));
        let rules = merged.rules.as_slice();
        assert_eq!(
            rules.len(),
            3,
            "each height in its block, then display once: {rules:?}"
        );
        assert!(rules[2].conditions.as_slice().is_empty(), "{rules:?}");
        assert_eq!(checks::live_rules(&merged, UiTheme::Flat), rules_of(&flat));
        assert_eq!(checks::live_rules(&merged, UiTheme::Flora), rules_of(&flora));
    }

    #[test]
    fn where_the_two_orders_cross_the_property_out_of_place_gives_way() {
        // Alike in every property, but display leads flat and trails flora:
        // sharing all four would give one theme the other's order.
        let flat = alloc::vec![display(), height(4), pad(2), inked(9)];
        let flora = alloc::vec![height(4), pad(2), inked(9), display()];
        let merged = follow_props(&flat, &flora);
        let merged = merged.as_slice();
        assert_each_theme_sees_its_own(merged, &flat, &flora);
        for ty in [
            CssPropertyType::Height,
            CssPropertyType::PaddingTop,
            CssPropertyType::TextColor,
        ] {
            assert_eq!(unconditional(merged, ty), 1, "{ty:?}: {merged:?}");
        }
        assert_eq!(
            unconditional(merged, CssPropertyType::Display),
            0,
            "display gives way, written in each block: {merged:?}"
        );
        assert_eq!(merged.len(), 5, "{merged:?}");
    }

    #[test]
    fn a_rule_is_never_split_and_one_differing_declaration_takes_it_into_each_block() {
        let flex = CssProperty::const_display(LayoutDisplay::Flex);
        let tall = |px: isize| CssProperty::const_height(LayoutHeight::const_px(px));
        let dark_ink = ink(ColorU::rgb(9, 9, 9));
        let flat = Css::from(alloc::vec![
            rule(alloc::vec![flex.clone(), tall(4)]),
            rule(alloc::vec![dark_ink.clone()]),
        ]);
        let flora = Css::from(alloc::vec![
            rule(alloc::vec![flex, tall(8)]),
            rule(alloc::vec![dark_ink]),
        ]);
        let merged = follow_css(UiTheme::Flat, flat.clone(), flora.clone());
        let rules = merged.rules.as_slice();
        assert_eq!(
            rules
                .iter()
                .map(|r| r.declarations.as_slice().len())
                .collect::<Vec<usize>>(),
            alloc::vec![2, 2, 1],
            "each two-declaration rule whole in its block, then the ink once: {rules:?}"
        );
        assert!(rules[2].conditions.as_slice().is_empty(), "{rules:?}");
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
    fn a_property_whose_dark_twin_is_alike_too_is_shared_light_half_and_twin_together() {
        let white = ColorU::rgb(255, 255, 255);
        let flat = alloc::vec![
            P::simple(ink(white)),
            P::dark_mode(ink(ColorU::rgb(1, 1, 1))),
            height(4)
        ];
        let flora = alloc::vec![
            P::simple(ink(white)),
            P::dark_mode(ink(ColorU::rgb(1, 1, 1))),
            height(8)
        ];
        let merged = follow_css(UiTheme::Flat, css(flat.clone()), css(flora.clone()));
        let inks: Vec<&CssRuleBlock> = merged
            .rules
            .as_slice()
            .iter()
            .filter(|r| {
                r.declarations
                    .as_slice()
                    .iter()
                    .any(|d| d.get_type() == Some(CssPropertyType::TextColor))
            })
            .collect();
        assert_eq!(inks.len(), 2, "the light half and its twin, once: {inks:?}");
        assert!(
            inks.iter().all(|r| !r
                .conditions
                .as_slice()
                .iter()
                .any(|c| matches!(c, DynamicSelector::Theme(ThemeCondition::Custom(_))))),
            "neither sits in a theme's block: {inks:?}"
        );
        assert_eq!(checks::live_rules(&merged, UiTheme::Flat), rules_of(&flat));
        assert_eq!(checks::live_rules(&merged, UiTheme::Flora), rules_of(&flora));
    }
}

#[cfg(test)]
mod follow_tests {
    //! The merge a widget that follows the APP theme builds its styles with:
    //! every theme's declarations on one node, each theme's block
    //! conditioned `@theme(<name>)`, resolving under either app theme
    //! exactly as that theme's own build does.
    use azul_core::dom::{Dom, IdOrClassVec};
    use azul_css::{
        dynamic_selector::{
            CssPropertyWithConditions as P, DynamicSelector, DynamicSelectorContext,
            PseudoStateType, ThemeCondition,
        },
        props::{
            basic::color::ColorU,
            layout::{LayoutDisplay, LayoutPaddingTop},
            property::{CssProperty, CssPropertyType},
        },
    };

    use super::{follow_dom, follow_props};
    use crate::widgets::themes::{decl, style_kit, UiTheme};

    const STATES: [Option<PseudoStateType>; 4] = [
        None,
        Some(PseudoStateType::Hover),
        Some(PseudoStateType::Active),
        Some(PseudoStateType::Focus),
    ];

    /// Last match wins per property, the way inline declarations resolve.
    fn resolve(
        props: &[P],
        app_theme: &str,
        dark: bool,
        state: Option<PseudoStateType>,
    ) -> Vec<(CssPropertyType, CssProperty)> {
        let ctx = DynamicSelectorContext {
            mode: if dark {
                azul_css::system::DarkLightMode::Dark
            } else {
                azul_css::system::DarkLightMode::Light
            },
            ..Default::default()
        }
        .with_app_theme(app_theme);
        let mut out: Vec<(CssPropertyType, CssProperty)> = Vec::new();
        for p in props {
            let applies = p.apply_if.as_slice().iter().all(|c| match c {
                DynamicSelector::PseudoState(s) => Some(*s) == state,
                other => other.matches(&ctx),
            });
            if !applies {
                continue;
            }
            let ty = p.property.get_type();
            match out.iter_mut().find(|(t, _)| *t == ty) {
                Some(slot) => slot.1 = p.property.clone(),
                None => out.push((ty, p.property.clone())),
            }
        }
        out.sort_by_key(|(t, _)| *t);
        out
    }

    fn inline(dom: &Dom) -> Vec<P> {
        dom.root
            .style
            .iter_inline_properties()
            .map(|(p, c)| P {
                property: p.clone(),
                apply_if: c.clone(),
            })
            .collect()
    }

    fn pad(px: isize) -> CssProperty {
        CssProperty::const_padding_top(LayoutPaddingTop::const_px(px))
    }

    fn flex() -> CssProperty {
        CssProperty::const_display(LayoutDisplay::Flex)
    }

    fn c(v: u8) -> ColorU {
        ColorU::rgb(v, v, v)
    }

    /// A flat part and a flora part that agree on some properties and not
    /// on others - in their resting value, their dark twin, a state, or
    /// only in how many declarations they make.
    fn parts() -> (Vec<P>, Vec<P>) {
        let flat = vec![
            P::simple(flex()),
            P::simple(pad(8)),
            P::on_hover(pad(4)),
            P::simple(decl::fill(c(250))),
            P::dark_mode(decl::fill(c(30))),
            P::on_hover(decl::fill(c(240))),
            P::dark_on_hover(decl::fill(c(40))),
        ];
        let flora = vec![
            P::simple(flex()),
            P::simple(pad(8)),
            P::simple(decl::fill(c(200))),
            P::dark_mode(decl::fill(c(60))),
            P::on_focus(decl::fill(c(210))),
        ];
        (flat, flora)
    }

    #[test]
    fn a_followed_part_resolves_like_each_themes_own_part_in_every_mode_and_state() {
        let (flat, flora) = parts();
        let merged = follow_props(&flat, &flora);
        for (name, own) in [("flat", &flat), ("flora", &flora)] {
            for dark in [false, true] {
                for state in STATES {
                    assert_eq!(
                        resolve(merged.as_slice(), name, dark, state),
                        resolve(own, name, dark, state),
                        "{name}, dark {dark}, {state:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_property_both_themes_declare_alike_stays_unconditional_once() {
        let (flat, flora) = parts();
        let merged = follow_props(&flat, &flora);
        let displays: Vec<&P> = merged
            .as_slice()
            .iter()
            .filter(|p| p.property.get_type() == CssPropertyType::Display)
            .collect();
        assert_eq!(displays.len(), 1, "{displays:?}");
        assert!(displays[0].apply_if.as_slice().is_empty(), "{displays:?}");
    }

    #[test]
    fn a_property_the_themes_declare_differently_is_written_per_theme_flat_first() {
        let (flat, flora) = parts();
        let merged = follow_props(&flat, &flora);
        // padding: equal at rest, but flat also pads on hover - so it is NOT
        // shared, or flora would inherit flat's hover padding.
        let pads: Vec<Vec<&str>> = merged
            .as_slice()
            .iter()
            .filter(|p| p.property.get_type() == CssPropertyType::PaddingTop)
            .map(P::theme_names)
            .collect();
        assert_eq!(pads, vec![vec!["flat"], vec!["flat"], vec!["flora"]]);
        // every themed declaration carries its theme name FIRST
        for p in merged.as_slice() {
            if p.theme_names().is_empty() {
                continue;
            }
            assert!(
                matches!(
                    p.apply_if.as_slice().first(),
                    Some(DynamicSelector::Theme(ThemeCondition::Custom(_)))
                ),
                "{p:?}"
            );
        }
        // the flat block comes before the flora block
        let names: Vec<&str> = merged
            .as_slice()
            .iter()
            .flat_map(P::theme_names)
            .collect();
        let last_flat = names.iter().rposition(|n| *n == "flat").expect("a flat block");
        let first_flora = names.iter().position(|n| *n == "flora").expect("a flora block");
        assert!(last_flat < first_flora, "{names:?}");
    }

    #[test]
    fn under_an_unknown_theme_a_followed_part_resolves_like_the_default_theme() {
        // An unknown theme's chain is `[monokai, flat]` (the app default is
        // always the last entry, §7.1), so the default theme's blocks are the
        // floor: an unknown theme looks like the default, never like no theme.
        let (flat, flora) = parts();
        let merged = follow_props(&flat, &flora);
        for dark in [false, true] {
            assert_eq!(
                resolve(merged.as_slice(), "monokai", dark, None),
                resolve(merged.as_slice(), azul_css::dynamic_selector::DEFAULT_APP_THEME, dark, None),
                "dark: {dark}"
            );
        }
    }

    fn node(theme: UiTheme, props: Vec<P>, children: Vec<Dom>) -> Dom {
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(vec![style_kit::marker(theme)]))
            .with_css_props(props.into())
            .with_children(children.into())
    }

    #[test]
    fn a_followed_dom_is_the_structure_themes_tree_with_every_themes_styles() {
        let (flat_props, flora_props) = parts();
        let flat = node(
            UiTheme::Flat,
            flat_props.clone(),
            vec![Dom::create_div().with_css_props(vec![P::simple(pad(1))].into())],
        );
        let flora = node(
            UiTheme::Flora,
            flora_props.clone(),
            vec![Dom::create_div().with_css_props(vec![P::simple(pad(2))].into())],
        );
        for structure in [UiTheme::Flat, UiTheme::Flora] {
            let dom = follow_dom(structure, flat.clone(), flora.clone());
            assert_eq!(
                dom.root.get_ids_and_classes(),
                IdOrClassVec::from_vec(vec![style_kit::marker(structure)]),
                "{structure:?}: the root is the structure theme's node"
            );
            for (name, own_root, own_child) in [
                ("flat", &flat_props, pad(1)),
                ("flora", &flora_props, pad(2)),
            ] {
                for dark in [false, true] {
                    for state in STATES {
                        assert_eq!(
                            resolve(&inline(&dom), name, dark, state),
                            resolve(own_root, name, dark, state),
                            "{structure:?} root under {name}, dark {dark}, {state:?}"
                        );
                    }
                }
                let child = &dom.children.as_ref()[0];
                assert_eq!(
                    resolve(&inline(child), name, false, None),
                    vec![(CssPropertyType::PaddingTop, own_child)],
                    "{structure:?} child under {name}"
                );
            }
        }
    }

    #[test]
    fn a_subtree_only_the_structure_theme_builds_is_kept_as_it_is() {
        let only_flora = Dom::create_div().with_css_props(vec![P::simple(pad(3))].into());
        let flat = node(UiTheme::Flat, vec![P::simple(pad(1))], vec![]);
        let flora = node(UiTheme::Flora, vec![P::simple(pad(2))], vec![only_flora.clone()]);
        let dom = follow_dom(UiTheme::Flora, flat, flora);
        assert!(dom.children.as_ref() == &[only_flora][..]);
    }

    #[test]
    fn a_dom_both_themes_build_alike_comes_back_unchanged() {
        let (flat_props, _) = parts();
        let same = node(
            UiTheme::Flat,
            flat_props,
            vec![Dom::create_div().with_css_props(vec![P::simple(pad(1))].into())],
        );
        for structure in [UiTheme::Flat, UiTheme::Flora] {
            assert!(follow_dom(structure, same.clone(), same.clone()) == same, "{structure:?}");
        }
    }
}

// ==== one node's parts: a later shared declaration against an earlier themed one ====

/// The theme names `conditions` puts a declaration in (`@theme(<name>)`).
fn themes_of(conditions: &DynamicSelectorVec) -> Vec<&str> {
    conditions
        .as_slice()
        .iter()
        .filter_map(|c| match c {
            DynamicSelector::Theme(ThemeCondition::Custom(name)) => Some(name.as_str()),
            _ => None,
        })
        .collect()
}

/// `units` - one node's declarations, in source order - with every one
/// outside the theme blocks that comes AFTER a themed one of the same
/// property written into EVERY widget theme's block instead, at its own
/// place: on one node a property is shared or themed, never both.
///
/// The cascade ranks a declaration in the live theme's block above one
/// outside every block, whatever their order (`@layer` semantics,
/// `azul_css::css::winning_inline_in`). [`follow_props`] merges ONE part, so
/// a widget that stacks parts on a node (the backstage's nav item, then its
/// gap) can get a property themed by the first part (the two looks' margins
/// differ) and shared by a later one (both gaps are 22px). Outside the
/// blocks, the later declaration lost to the earlier themed one; in the
/// theme's block it wins by source order, as it does on the pinned widget,
/// and the live declarations under each theme ARE the pinned widget's.
/// Declarations without theme blocks come back as they are.
fn settle<T: Unit>(units: Vec<T>) -> Vec<T> {
    if units.iter().all(|u| themes_of(u.conditions()).is_empty()) {
        return units;
    }
    let mut out: Vec<T> = Vec::with_capacity(units.len());
    for (i, unit) in units.iter().enumerate() {
        if themes_of(unit.conditions()).is_empty() {
            let types = types_of(unit);
            let mut earlier: Vec<&str> = Vec::new();
            for before in &units[..i] {
                if !types.iter().any(|t| before.declares(*t)) {
                    continue;
                }
                for name in themes_of(before.conditions()) {
                    if !earlier.contains(&name) {
                        earlier.push(name);
                    }
                }
            }
            if earlier.iter().any(|n| UiTheme::from_name(n).is_some()) {
                for theme in UiTheme::ALL {
                    out.push(unit.clone().into_block(theme));
                }
                continue;
            }
        }
        out.push(unit.clone());
    }
    out
}

/// `extra` stacked onto `base` - one node's style from two merged parts
/// ([`follow_props`]), `extra` overriding `base` where they collide, under
/// every app theme ([`settle`]). THE way a widget that follows the app
/// theme puts a state part (active, gap, selected) after its base part.
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
    CssPropertyWithConditionsVec::from_vec(settle(v))
}

#[cfg(test)]
mod settle_tests {
    use azul_css::{
        dynamic_selector::{
            CssPropertyWithConditions, CssPropertyWithConditionsVec, DynamicSelectorContext,
        },
        props::{
            layout::LayoutMarginTop,
            property::{CssProperty, CssPropertyType},
        },
    };

    use super::{follow_props, stack_parts, UiTheme};

    fn margin_top(px: isize) -> CssPropertyWithConditions {
        CssPropertyWithConditions::simple(CssProperty::const_margin_top(
            LayoutMarginTop::const_px(px),
        ))
    }

    /// `style`'s `margin-top` under the app theme `theme`, as the cascade
    /// ranks it.
    fn resolved_margin_top(
        style: &CssPropertyWithConditionsVec,
        theme: UiTheme,
    ) -> Option<CssProperty> {
        let ctx = DynamicSelectorContext::default().with_app_theme(theme.name());
        azul_css::css::winning_inline_in(
            style.as_slice().iter().map(|d| (&d.property, &d.apply_if)),
            CssPropertyType::MarginTop,
            |c| c.as_slice().iter().all(|s| s.matches(&ctx)),
            |c| ctx.cascade_rank(c),
        )
        .cloned()
    }

    #[test]
    fn a_later_part_shared_by_both_looks_beats_an_earlier_themed_one() {
        // The item part: the looks' margins differ, so they are themed.
        let item = follow_props(&[margin_top(1)], &[margin_top(4)]);
        // The gap part: alike in both looks, so it is shared.
        let gap = follow_props(&[margin_top(22)], &[margin_top(22)]);
        let style = stack_parts(&item, &gap);
        for theme in [UiTheme::Flat, UiTheme::Flora] {
            assert_eq!(
                resolved_margin_top(&style, theme),
                Some(margin_top(22).property),
                "{theme:?}"
            );
        }
    }

    #[test]
    fn parts_without_theme_blocks_stack_as_they_are() {
        let base = CssPropertyWithConditionsVec::from_vec(vec![margin_top(1)]);
        let extra = CssPropertyWithConditionsVec::from_vec(vec![margin_top(22)]);
        assert_eq!(
            stack_parts(&base, &extra).as_slice(),
            &[margin_top(1), margin_top(22)]
        );
    }
}
