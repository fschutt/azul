//! Test helpers for asserting what a themed widget looks like: resolve a
//! node's inline style the way the cascade does (last match wins) under a
//! given theme and pseudo-state, and walk a widget's tree for the invariants
//! every theme owes - dark twins after their light half, focus rings in both
//! modes, the same accessibility tree whichever theme drew it.

use alloc::{format, string::String, vec::Vec};

use azul_core::dom::{Dom, IdOrClass};
use azul_css::{
    dynamic_selector::{
        CssPropertyWithConditions, DynamicSelector, DynamicSelectorContext, DynamicSelectorVec,
        PseudoStateType, ThemeCondition,
    },
    props::{
        basic::color::ColorU,
        property::{CssProperty, CssPropertyType},
        style::StyleBackgroundContent,
    },
};

use super::UiTheme;

/// Every node of `dom`, depth first, with its path (`root/0/2`).
pub(crate) fn nodes(dom: &Dom) -> Vec<(String, &Dom)> {
    fn walk<'a>(node: &'a Dom, path: String, out: &mut Vec<(String, &'a Dom)>) {
        out.push((path.clone(), node));
        for (i, child) in node.children.as_ref().iter().enumerate() {
            walk(child, format!("{path}/{i}"), out);
        }
    }
    let mut out = Vec::new();
    walk(dom, String::from("root"), &mut out);
    out
}

/// Whether `node` carries the class `name`.
pub(crate) fn has_class(node: &Dom, name: &str) -> bool {
    node.root
        .get_ids_and_classes()
        .as_ref()
        .iter()
        .any(|c| matches!(c, IdOrClass::Class(s) if s.as_str() == name))
}

/// The first node (depth first) carrying the class `name`.
pub(crate) fn find<'a>(dom: &'a Dom, name: &str) -> Option<&'a Dom> {
    nodes(dom)
        .into_iter()
        .map(|(_, n)| n)
        .find(|n| has_class(n, name))
}

/// Every node carrying the class `name`.
pub(crate) fn find_all<'a>(dom: &'a Dom, name: &str) -> Vec<&'a Dom> {
    nodes(dom)
        .into_iter()
        .map(|(_, n)| n)
        .filter(|n| has_class(n, name))
        .collect()
}

/// The app theme every probe evaluates under: the one the test builds for,
/// entered explicitly (`theme_blocks::checks::under(theme, ..)`, a
/// `ThemeScope` - what a window does around its DOM build), and the default
/// app theme, flat, outside one. The widget built in the same scope is built
/// for the same theme ([`UiTheme::current`]), so a probe reads the look the
/// build shows.
///
/// ONE rule for every probe - [`resolve`] and everything on it, the
/// `widgets::theme_probe` readers and `theme_blocks::checks` - so a probe on
/// a widget that follows the app theme answers exactly what it answers on
/// the widget pinned to that theme.
#[must_use]
pub(crate) fn probe_theme() -> UiTheme {
    UiTheme::current()
}

/// `conditions` as the app theme `theme` sees them: `None` when they sit in
/// another app theme's block (`@theme(<name>)`, decided by the cascade's own
/// matcher), else the conditions left once the app-theme ones are dropped -
/// what the same declaration carries on a widget pinned to `theme`.
#[must_use]
pub(crate) fn live_conditions(
    conditions: &DynamicSelectorVec,
    theme: UiTheme,
) -> Option<DynamicSelectorVec> {
    let ctx = DynamicSelectorContext::default().with_app_theme(theme.name());
    let mut kept = Vec::new();
    for c in conditions.as_ref() {
        if matches!(c, DynamicSelector::Theme(ThemeCondition::Custom(_))) {
            if !c.matches(&ctx) {
                return None;
            }
        } else {
            kept.push(c.clone());
        }
    }
    Some(DynamicSelectorVec::from_vec(kept))
}

/// Whether a declaration's conditions all hold for a node in the `dark` (or
/// light) theme and in `state` (or at rest), under the app theme the probes
/// evaluate under ([`probe_theme`]) - a widget that follows the app theme
/// carries every theme's `@theme(<name>)` block, and only the live one
/// applies.
fn applies(conds: &DynamicSelectorVec, dark: bool, state: Option<PseudoStateType>) -> bool {
    let Some(live) = live_conditions(conds, probe_theme()) else {
        return false;
    };
    live.as_ref().iter().all(|c| match c {
        DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark) => dark,
        DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Light) => !dark,
        DynamicSelector::PseudoState(s) => Some(*s) == state,
        _ => false,
    })
}

/// The value of `ty` on `node` in the `dark` (or light) theme and in `state`
/// (`None`: at rest), resolved last-match-wins over the inline declarations.
pub(crate) fn resolve(
    node: &Dom,
    ty: CssPropertyType,
    dark: bool,
    state: Option<PseudoStateType>,
) -> Option<CssProperty> {
    node.root
        .style
        .iter_inline_properties()
        .filter(|(p, c)| p.get_type() == ty && applies(c, dark, state))
        .map(|(p, _)| p.clone())
        .last()
}

/// The first layer's colour of a `background`, or `None` for a gradient or
/// a system colour.
pub(crate) fn bg_color(p: &CssProperty) -> Option<ColorU> {
    match p {
        CssProperty::BackgroundContent(v) => match v.get_property()?.as_ref().first()? {
            StyleBackgroundContent::Color(c) => Some(*c),
            _ => None,
        },
        _ => None,
    }
}

/// The layers of a `background`.
pub(crate) fn bg_layers(p: &CssProperty) -> Vec<StyleBackgroundContent> {
    match p {
        CssProperty::BackgroundContent(v) => v
            .get_property()
            .map(|b| b.as_ref().to_vec())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The resting background of `node` in the light or dark theme.
pub(crate) fn background(node: &Dom, dark: bool) -> Option<CssProperty> {
    resolve(node, CssPropertyType::BackgroundContent, dark, None)
}

/// The resting text colour of `node` in the light or dark theme.
pub(crate) fn text_color(node: &Dom, dark: bool) -> Option<ColorU> {
    match resolve(node, CssPropertyType::TextColor, dark, None)? {
        CssProperty::TextColor(v) => v.get_property().map(|c| c.inner),
        _ => None,
    }
}

/// The top border colour of `node` in the light or dark theme and `state`.
pub(crate) fn border_top_color(
    node: &Dom,
    dark: bool,
    state: Option<PseudoStateType>,
) -> Option<ColorU> {
    match resolve(node, CssPropertyType::BorderTopColor, dark, state)? {
        CssProperty::BorderTopColor(v) => v.get_property().map(|c| c.inner),
        _ => None,
    }
}

/// Whether `node` shows a focus ring in the light or dark theme: either all
/// four border edges have a width and focusing changes each edge's colour to
/// an opaque one ([`has_border_ring`]), or focusing lays an opaque inset /
/// outset shadow ring over the box ([`has_shadow_ring`] - the ring a joined
/// button bar uses, whose inner items share their side borders).
pub(crate) fn has_focus_ring(node: &Dom, dark: bool) -> bool {
    has_border_ring(node, dark) || has_shadow_ring(node, dark)
}

/// The border form of [`has_focus_ring`].
pub(crate) fn has_border_ring(node: &Dom, dark: bool) -> bool {
    use CssPropertyType as T;
    let edges = [
        (T::BorderTopWidth, T::BorderTopColor),
        (T::BorderRightWidth, T::BorderRightColor),
        (T::BorderBottomWidth, T::BorderBottomColor),
        (T::BorderLeftWidth, T::BorderLeftColor),
    ];
    edges.iter().all(|(w, c)| {
        let has_width = resolve(node, *w, dark, Some(PseudoStateType::Focus)).is_some();
        let rest = resolve(node, *c, dark, None);
        let focus = resolve(node, *c, dark, Some(PseudoStateType::Focus));
        let opaque = focus.as_ref().and_then(border_color).is_some_and(|c| c.a > 0);
        has_width && opaque && focus != rest
    })
}

/// The colour of a border-colour declaration.
pub(crate) fn border_color(p: &CssProperty) -> Option<ColorU> {
    match p {
        CssProperty::BorderTopColor(v) => v.get_property().map(|c| c.inner),
        CssProperty::BorderRightColor(v) => v.get_property().map(|c| c.inner),
        CssProperty::BorderBottomColor(v) => v.get_property().map(|c| c.inner),
        CssProperty::BorderLeftColor(v) => v.get_property().map(|c| c.inner),
        _ => None,
    }
}

/// The shadow form of [`has_focus_ring`]: a box-shadow declared for `:focus`
/// in that mode, opaque, with a spread or blur, that the resting box does
/// not have.
pub(crate) fn has_shadow_ring(node: &Dom, dark: bool) -> bool {
    use CssPropertyType as T;
    [T::BoxShadowTop, T::BoxShadowRight, T::BoxShadowBottom, T::BoxShadowLeft]
        .iter()
        .any(|ty| {
            let focus = resolve(node, *ty, dark, Some(PseudoStateType::Focus));
            let rest = resolve(node, *ty, dark, None);
            let ringed = focus.as_ref().and_then(shadow_color_and_reach).is_some_and(
                |(c, reach)| c.a > 0 && reach,
            );
            ringed && focus != rest
        })
}

/// The colour of a box-shadow declaration, and whether it reaches past a
/// hairline (a spread or a blur).
pub(crate) fn shadow_color_and_reach(p: &CssProperty) -> Option<(ColorU, bool)> {
    let v = match p {
        CssProperty::BoxShadowTop(v)
        | CssProperty::BoxShadowRight(v)
        | CssProperty::BoxShadowBottom(v)
        | CssProperty::BoxShadowLeft(v) => v,
        _ => return None,
    };
    let s = v.get_property()?.as_ref();
    let reach = s.spread_radius.inner.number.get() > 0.0 || s.blur_radius.inner.number.get() > 0.0;
    Some((s.color, reach))
}

/// The focus-ring colour of `node` in the light or dark theme: the top
/// border's colour under `:focus`, or the focus shadow's.
pub(crate) fn focus_ring_color(node: &Dom, dark: bool) -> Option<ColorU> {
    use CssPropertyType as T;
    let focus = Some(PseudoStateType::Focus);
    if has_border_ring(node, dark) {
        return resolve(node, T::BorderTopColor, dark, focus).as_ref().and_then(border_color);
    }
    [T::BoxShadowTop, T::BoxShadowRight, T::BoxShadowBottom, T::BoxShadowLeft]
        .iter()
        .find_map(|ty| {
            let f = resolve(node, *ty, dark, focus)?;
            (Some(&f) != resolve(node, *ty, dark, None).as_ref())
                .then(|| shadow_color_and_reach(&f).map(|(c, _)| c))
                .flatten()
        })
}

/// Every node of `dom` a user can Tab to. A roving group's other items
/// (`NoKeyboardFocus`) are reached by arrow keys, so a widget's own tests
/// check those by class; a `NoKeyboardFocus` window root is never ringed.
pub(crate) fn focusable(dom: &Dom) -> Vec<(String, &Dom)> {
    use azul_core::dom::TabIndex;
    nodes(dom)
        .into_iter()
        .filter(|(_, n)| {
            matches!(
                n.root.get_tab_index(),
                Some(TabIndex::Auto | TabIndex::OverrideInParent(_))
            )
        })
        .collect()
}

/// The accessibility tree as a flat list: every node that declares an
/// accessibility info or a tab index, in tree order. Two themes of one
/// widget must produce the same list.
pub(crate) fn a11y_outline(dom: &Dom) -> Vec<String> {
    nodes(dom)
        .into_iter()
        .filter_map(|(_, n)| {
            let a11y = n.root.get_accessibility_info();
            let tab = n.root.get_tab_index();
            if a11y.is_none() && tab.is_none() {
                None
            } else {
                Some(format!("{a11y:?} / {tab:?}"))
            }
        })
        .collect()
}

/// The theme-pair invariant (`widgets::theme_pairs`) over one tree: every
/// dark twin has a light declaration of the same property, pseudo-states and
/// app theme (`@theme(<name>)` block) BEFORE it. Returns one message per
/// violation.
pub(crate) fn half_pairs(dom: &Dom) -> Vec<String> {
    let mut out = Vec::new();
    for (path, node) in nodes(dom) {
        let props: Vec<CssPropertyWithConditions> = node
            .root
            .style
            .iter_inline_properties()
            .map(|(p, c)| CssPropertyWithConditions {
                property: p.clone(),
                apply_if: c.clone(),
            })
            .collect();
        for (i, twin) in props.iter().enumerate() {
            if !twin.is_dark_twin() {
                continue;
            }
            let ty = twin.property.get_type();
            let states = twin.pseudo_state_conditions();
            let themes = twin.theme_names();
            let at = props.iter().position(|p| {
                p.property.get_type() == ty
                    && p.is_light_half()
                    && p.pseudo_state_conditions() == states
                    && p.theme_names() == themes
            });
            match at {
                None => out.push(format!("{path}: dark {ty:?} {states:?} has no light half")),
                Some(j) if j > i => {
                    out.push(format!("{path}: dark {ty:?} {states:?} comes before its light half"))
                }
                Some(_) => {}
            }
        }
    }
    out
}

/// An interactive state (`:hover`, `:active`, `:focus`) the node declares
/// for a property but that a later RESTING declaration of the same property
/// beats in that state - typically a resting dark twin pushed after the state
/// rules (it matches in every state, so it wins). One message per property,
/// mode and state where that happens.
pub(crate) fn shadowed_states(dom: &Dom) -> Vec<String> {
    use PseudoStateType::{Active, Focus, Hover};
    fn has_state(c: &DynamicSelectorVec, state: PseudoStateType) -> bool {
        c.as_ref()
            .iter()
            .any(|s| matches!(s, DynamicSelector::PseudoState(x) if *x == state))
    }
    let mut out = Vec::new();
    for (path, node) in nodes(dom) {
        let props: Vec<(CssProperty, DynamicSelectorVec)> = node
            .root
            .style
            .iter_inline_properties()
            .map(|(p, c)| (p.clone(), c.clone()))
            .collect();
        let mut types: Vec<CssPropertyType> = props.iter().map(|(p, _)| p.get_type()).collect();
        types.sort_unstable();
        types.dedup();
        for ty in types {
            for dark in [false, true] {
                for state in [Hover, Active, Focus] {
                    // Walk the declarations that apply in this state and mode
                    // in order: the last one wins.
                    let mut declared = false;
                    let mut winner_is_state = false;
                    for (p, c) in &props {
                        if p.get_type() != ty || !applies(c, dark, Some(state)) {
                            continue;
                        }
                        let is_state = has_state(c, state);
                        declared |= is_state;
                        winner_is_state = is_state;
                    }
                    if declared && !winner_is_state {
                        out.push(format!(
                            "{path}: {ty:?} {state:?} (dark: {dark}) is shadowed by a later \
                             resting declaration"
                        ));
                    }
                }
            }
        }
    }
    out
}

/// Asserts the invariants every themed tree owes, with `what` in the
/// message: no half pairs, no shadowed states, and every focusable node
/// ringed in both modes.
pub(crate) fn assert_theme_invariants(what: &str, dom: &Dom) {
    let halves = half_pairs(dom);
    assert!(halves.is_empty(), "{what}: half pairs:\n  {}", halves.join("\n  "));
    let shadowed = shadowed_states(dom);
    assert!(shadowed.is_empty(), "{what}: shadowed states:\n  {}", shadowed.join("\n  "));
    for (path, node) in focusable(dom) {
        assert!(has_focus_ring(node, false), "{what}: {path} has no light focus ring");
        assert!(has_focus_ring(node, true), "{what}: {path} has no dark focus ring");
    }
}

// ==== R5: a widget's structure is its BASE, never a theme's ====

/// The properties that lay a widget out. They are the same in every theme,
/// so they belong to the widget's BASE (unconditioned, live under flat,
/// flora and any future theme), never inside a `@theme(<name>)` block (user
/// ruling 2026-09-29: the rules that apply to all themes go outside the
/// `@theme` blocks). What a theme owns is paint and metrics: colours,
/// backgrounds, borders, radii, shadows, fonts, padding, gaps and sizes.
pub(crate) const STRUCTURE_PROPERTIES: &[CssPropertyType] = &[
    CssPropertyType::Display,
    CssPropertyType::Position,
    CssPropertyType::Float,
    CssPropertyType::BoxSizing,
    CssPropertyType::FlexDirection,
    CssPropertyType::FlexWrap,
    CssPropertyType::FlexGrow,
    CssPropertyType::FlexShrink,
    CssPropertyType::JustifyContent,
    CssPropertyType::AlignItems,
    CssPropertyType::AlignContent,
    CssPropertyType::AlignSelf,
    CssPropertyType::OverflowX,
    CssPropertyType::OverflowY,
    CssPropertyType::Cursor,
    CssPropertyType::UserSelect,
    CssPropertyType::WhiteSpace,
    CssPropertyType::TextOverflow,
];

/// A structure declaration a widget's themes REALLY draw differently, which
/// may stay inside its theme's block: the class of the node (or the
/// selector of the component-sheet rule), the property, and why.
pub(crate) type ThemedStructure = (&'static str, CssPropertyType, &'static str);

/// Every structure declaration of `dom` (its nodes' inline styles and
/// component sheets) that sits inside an app theme's `@theme(<name>)` block
/// and is not `allowed`, one line each: `<path> .<class> <property>
/// @theme(<name>)`, or `<path> sheet <selector> ...` for a sheet's rule.
pub(crate) fn themed_structure(dom: &Dom, allowed: &[ThemedStructure]) -> Vec<String> {
    let mut out = Vec::new();
    for (path, node) in nodes(dom) {
        let classes: Vec<String> = node
            .root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                IdOrClass::Class(s) => Some(String::from(s.as_str())),
                IdOrClass::Id(_) => None,
            })
            .collect();
        let sheets = core::iter::once((true, &node.root.style))
            .chain(node.css.as_slice().iter().map(|css| (false, css)));
        for (inline, css) in sheets {
            for rule in css.rules.as_slice() {
                let Some(theme) = rule.conditions.as_slice().iter().find_map(|c| match c {
                    DynamicSelector::Theme(ThemeCondition::Custom(name)) => Some(name.as_str()),
                    _ => None,
                }) else {
                    continue;
                };
                let selector = format!("{}", rule.path);
                for d in rule.declarations.as_slice() {
                    let Some(ty) = d.get_type() else { continue };
                    if !STRUCTURE_PROPERTIES.contains(&ty) {
                        continue;
                    }
                    let is_allowed = allowed.iter().any(|(who, t, _why)| {
                        *t == ty
                            && if inline {
                                classes.iter().any(|c| c.as_str() == *who)
                            } else {
                                selector.contains(*who)
                            }
                    });
                    if is_allowed {
                        continue;
                    }
                    let whom = if inline {
                        format!(".{}", classes.first().map_or("<no class>", String::as_str))
                    } else {
                        format!("sheet {selector}")
                    };
                    out.push(format!("{path} {whom} {ty:?} @theme({theme})"));
                }
            }
        }
    }
    out
}

/// Asserts that `dom` - a widget that FOLLOWS the app theme (no
/// `with_theme` pin), so it carries every theme's block - declares its
/// structure in its base: no [`STRUCTURE_PROPERTIES`] inside a `@theme`
/// block except the `allowed` ones.
pub(crate) fn assert_structure_is_shared(what: &str, dom: &Dom, allowed: &[ThemedStructure]) {
    let themed = themed_structure(dom, allowed);
    assert!(
        themed.is_empty(),
        "{what}: structure inside a theme block (make it the widget's base, or allow it with \
         the reason the themes differ):\n  {}",
        themed.join("\n  ")
    );
}

#[cfg(test)]
mod structure_tests {
    use azul_core::dom::Dom;
    use azul_css::{
        dynamic_selector::CssPropertyWithConditions,
        props::{
            layout::{LayoutDisplay, LayoutFlexDirection},
            property::{CssProperty, CssPropertyType},
        },
    };

    use super::{themed_structure, UiTheme};
    use crate::widgets::themes::theme_blocks::follow_dom;

    fn look(direction: LayoutFlexDirection) -> Dom {
        Dom::create_div()
            .with_class("probe".into())
            .with_css_props(
                vec![
                    CssPropertyWithConditions::simple(CssProperty::const_display(
                        LayoutDisplay::Flex,
                    )),
                    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
                        direction,
                    )),
                ]
                .into(),
            )
    }

    #[test]
    fn a_structure_both_themes_declare_alike_is_the_base() {
        let dom = follow_dom(
            UiTheme::Flat,
            look(LayoutFlexDirection::Row),
            look(LayoutFlexDirection::Row),
        );
        assert!(themed_structure(&dom, &[]).is_empty());
    }

    #[test]
    fn a_structure_the_themes_declare_apart_is_reported_unless_allowed() {
        let dom = follow_dom(
            UiTheme::Flat,
            look(LayoutFlexDirection::Row),
            look(LayoutFlexDirection::Column),
        );
        let themed = themed_structure(&dom, &[]);
        assert_eq!(themed.len(), 2, "{themed:?}");
        assert!(themed.iter().all(|l| l.contains(".probe flex-direction")), "{themed:?}");
        let allowed = [("probe", CssPropertyType::FlexDirection, "the probe differs")];
        assert!(themed_structure(&dom, &allowed).is_empty());
    }
}
