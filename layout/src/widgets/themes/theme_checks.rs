//! Test helpers for asserting what a themed widget looks like: resolve a
//! node's inline style the way the cascade does (last match wins) under a
//! given theme and pseudo-state, and walk a widget's tree for the invariants
//! every theme owes - dark twins after their light half, focus rings in both
//! modes, the same accessibility tree whichever theme drew it.

use alloc::{format, string::String, vec::Vec};

use azul_core::dom::{Dom, IdOrClass};
use azul_css::{
    dynamic_selector::{
        CssPropertyWithConditions, DynamicSelector, DynamicSelectorVec, PseudoStateType,
        ThemeCondition,
    },
    props::{
        basic::color::ColorU,
        property::{CssProperty, CssPropertyType},
        style::StyleBackgroundContent,
    },
};

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

/// Whether a declaration's conditions all hold for a node in the `dark` (or
/// light) theme and in `state` (or at rest).
fn applies(conds: &DynamicSelectorVec, dark: bool, state: Option<PseudoStateType>) -> bool {
    conds.as_ref().iter().all(|c| match c {
        DynamicSelector::Theme(ThemeCondition::Dark) => dark,
        DynamicSelector::Theme(ThemeCondition::Light) => !dark,
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

/// Whether `node` shows a focus ring in the light or dark theme: all four
/// border edges have a width, and focusing changes each edge's colour to an
/// opaque one.
pub(crate) fn has_focus_ring(node: &Dom, dark: bool) -> bool {
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
        let opaque = match &focus {
            Some(CssProperty::BorderTopColor(v)) => v.get_property().is_some_and(|c| c.inner.a > 0),
            Some(CssProperty::BorderRightColor(v)) => {
                v.get_property().is_some_and(|c| c.inner.a > 0)
            }
            Some(CssProperty::BorderBottomColor(v)) => {
                v.get_property().is_some_and(|c| c.inner.a > 0)
            }
            Some(CssProperty::BorderLeftColor(v)) => {
                v.get_property().is_some_and(|c| c.inner.a > 0)
            }
            _ => false,
        };
        has_width && opaque && focus != rest
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
/// dark twin has a light declaration of the same property and pseudo-states
/// BEFORE it. Returns one message per violation.
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
            let at = props.iter().position(|p| {
                p.property.get_type() == ty
                    && p.is_light_half()
                    && p.pseudo_state_conditions() == states
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

/// A resting dark twin pushed AFTER a state rule of the same property
/// shadows it in the dark theme (a resting twin matches in every state).
pub(crate) fn shadowed_states(dom: &Dom) -> Vec<String> {
    let mut out = Vec::new();
    for (path, node) in nodes(dom) {
        let props: Vec<(CssProperty, DynamicSelectorVec)> = node
            .root
            .style
            .iter_inline_properties()
            .map(|(p, c)| (p.clone(), c.clone()))
            .collect();
        for (i, (p, c)) in props.iter().enumerate() {
            let is_state = c
                .as_ref()
                .iter()
                .any(|s| matches!(s, DynamicSelector::PseudoState(_)));
            if !is_state {
                continue;
            }
            let later_resting = props[i + 1..].iter().any(|(q, d)| {
                q.get_type() == p.get_type()
                    && !d
                        .as_ref()
                        .iter()
                        .any(|s| matches!(s, DynamicSelector::PseudoState(_)))
            });
            if later_resting {
                out.push(format!(
                    "{path}: a resting {:?} comes after a state rule and shadows it",
                    p.get_type()
                ));
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
