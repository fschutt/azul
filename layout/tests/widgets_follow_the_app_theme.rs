//! A widget nobody pinned follows the APP theme (`AppConfig::with_theme`,
//! `CallbackInfo::set_theme`) - the second half of the widgets (T3).
//!
//! An unpinned widget (`theme: None`, no `with_theme`) is built in the
//! STRUCTURE of the theme its DOM is built for and carries the declarations
//! of EVERY theme, each block conditioned `@theme(<name>)`; the cascade keeps
//! the live theme's. So an app sets its theme once and no widget can be
//! forgotten and look out of place. A window builds its DOM inside a
//! `ThemeScope` of its app theme; these tests do the same and never publish
//! the app-global choice (the layout tests share one process).
//!
//! What every migrated widget owes, checked through the public API only
//! ([`assert_follows_the_app_theme`]):
//!
//! * unpinned, built for app theme T, every node resolves exactly like the widget pinned to T
//!   (`with_theme(T)`) - in light and dark, at rest and hovered / pressed / focused - with the same
//!   classes (the theme marker included) and accessibility;
//! * its DOM carries BOTH themes' blocks (unless the two looks are one);
//! * a pinned widget ignores the app theme: no `@theme` condition anywhere, the same styles
//!   whatever the app theme;
//! * its accessibility tree is the same under both app themes.

use azul_core::{app_theme::ThemeScope, dom::Dom};
use azul_css::{
    dynamic_selector::{DynamicSelector, DynamicSelectorContext, PseudoStateType, ThemeCondition},
    props::property::{CssProperty, CssPropertyType},
    AzString,
};
use azul_layout::widgets::themes::UiTheme;

const THEMES: [UiTheme; 2] = [UiTheme::Flat, UiTheme::Flora];

const STATES: [Option<PseudoStateType>; 4] = [
    None,
    Some(PseudoStateType::Hover),
    Some(PseudoStateType::Active),
    Some(PseudoStateType::Focus),
];

/// `make()` built for the app theme `theme`, the way a window's layout pass
/// builds its DOM.
fn built_for(theme: UiTheme, make: &dyn Fn() -> Dom) -> Dom {
    let _scope = ThemeScope::enter(AzString::from_const_str(theme.name()));
    make()
}

/// `node`'s inline style under the app theme `app_theme`, the colour scheme
/// and `state` (`None`: at rest): last match wins per property.
fn resolve(
    node: &Dom,
    app_theme: &str,
    dark: bool,
    state: Option<PseudoStateType>,
) -> Vec<(CssPropertyType, CssProperty)> {
    let ctx = DynamicSelectorContext {
        theme: if dark {
            ThemeCondition::Dark
        } else {
            ThemeCondition::Light
        },
        ..Default::default()
    }
    .with_app_theme(app_theme);
    let mut out: Vec<(CssPropertyType, CssProperty)> = Vec::new();
    for (p, conds) in node.root.style.iter_inline_properties() {
        let applies = conds.as_ref().iter().all(|c| match c {
            DynamicSelector::PseudoState(s) => Some(*s) == state,
            other => other.matches(&ctx),
        });
        if !applies {
            continue;
        }
        let ty = p.get_type();
        match out.iter_mut().find(|(t, _)| *t == ty) {
            Some(slot) => slot.1 = p.clone(),
            None => out.push((ty, p.clone())),
        }
    }
    out.sort_by_key(|(t, _)| *t);
    out
}

/// Every node of `dom`, depth first, with its path (`root/0/2`).
fn nodes(dom: &Dom) -> Vec<(String, &Dom)> {
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

/// Every app-theme name any declaration in `dom` is conditioned on.
fn theme_names(dom: &Dom) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (_, node) in nodes(dom) {
        for (_, conds) in node.root.style.iter_inline_properties() {
            for c in conds.as_ref() {
                if let DynamicSelector::Theme(ThemeCondition::Custom(name)) = c {
                    if !out.iter().any(|n| n == name.as_str()) {
                        out.push(name.as_str().to_string());
                    }
                }
            }
        }
    }
    out.sort();
    out
}

/// Every node's inline style, in tree order.
fn styles(dom: &Dom) -> Vec<azul_css::css::Css> {
    nodes(dom)
        .into_iter()
        .map(|(_, n)| n.root.style.clone())
        .collect()
}

/// Every node's accessibility declaration, in tree order.
fn a11y(dom: &Dom) -> Vec<Option<Box<azul_core::a11y::AccessibilityInfo>>> {
    nodes(dom)
        .into_iter()
        .map(|(_, n)| n.root.accessibility.clone())
        .collect()
}

/// The checks every widget that follows the app theme owes (module docs).
/// `make(None)` builds it unpinned, `make(Some(t))` pinned to `t`.
fn assert_follows_the_app_theme(widget: &str, make: impl Fn(Option<UiTheme>) -> Dom) {
    let mut bad: Vec<String> = Vec::new();

    for theme in THEMES {
        let followed = built_for(theme, &|| make(None));
        let pinned = built_for(theme, &|| make(Some(theme)));
        let (f, p) = (nodes(&followed), nodes(&pinned));
        if f.len() != p.len() {
            bad.push(format!(
                "{widget} under {theme:?}: {} nodes unpinned, {} with_theme({theme:?})",
                f.len(),
                p.len()
            ));
            continue;
        }
        for ((path, fnode), (_, pnode)) in f.iter().zip(p.iter()) {
            if fnode.root.get_ids_and_classes() != pnode.root.get_ids_and_classes() {
                bad.push(format!(
                    "{widget} under {theme:?}: node {path} has classes {:?}, with_theme gives {:?}",
                    fnode.root.get_ids_and_classes(),
                    pnode.root.get_ids_and_classes()
                ));
            }
            if fnode.root.accessibility != pnode.root.accessibility {
                bad.push(format!(
                    "{widget} under {theme:?}: node {path} is announced differently from with_theme"
                ));
            }
            for dark in [false, true] {
                for state in STATES {
                    let got = resolve(fnode, theme.name(), dark, state);
                    let want = resolve(pnode, theme.name(), dark, state);
                    if got != want {
                        bad.push(format!(
                            "{widget} under {theme:?}, dark {dark}, {state:?}: node {path} \
                             resolves\n      {got:?}\n    with_theme({theme:?}) resolves\n      \
                             {want:?}"
                        ));
                    }
                }
            }
        }
    }

    let looks_differ = styles(&built_for(UiTheme::Flat, &|| make(Some(UiTheme::Flat))))
        != styles(&built_for(UiTheme::Flat, &|| make(Some(UiTheme::Flora))));
    if looks_differ {
        for theme in THEMES {
            let names = theme_names(&built_for(theme, &|| make(None)));
            if names != ["flat", "flora"] {
                bad.push(format!(
                    "{widget} built for {theme:?} carries the theme blocks {names:?}, not both"
                ));
            }
        }
    }

    for pin in THEMES {
        let under: Vec<Dom> = THEMES
            .iter()
            .map(|app| built_for(*app, &|| make(Some(pin))))
            .collect();
        for (app, dom) in THEMES.iter().zip(under.iter()) {
            let names = theme_names(dom);
            if !names.is_empty() {
                bad.push(format!(
                    "{widget} pinned to {pin:?}, built for {app:?}, carries theme blocks {names:?}"
                ));
            }
        }
        if styles(&under[0]) != styles(&under[1]) {
            bad.push(format!("{widget} pinned to {pin:?} changes with the app theme"));
        }
    }

    let (flat, flora) = (
        built_for(UiTheme::Flat, &|| make(None)),
        built_for(UiTheme::Flora, &|| make(None)),
    );
    if a11y(&flat) != a11y(&flora) {
        bad.push(format!(
            "{widget}: the accessibility tree changes with the app theme"
        ));
    }

    assert!(
        bad.is_empty(),
        "{} finding(s):\n  {}",
        bad.len(),
        bad.join("\n  ")
    );
}

/// `widget`, pinned to `theme` when there is one.
fn pinned<W>(widget: W, theme: Option<UiTheme>, with_theme: fn(W, UiTheme) -> W) -> W {
    match theme {
        Some(t) => with_theme(widget, t),
        None => widget,
    }
}

/// A guard on the guard: a widget that ignores the app theme, and one whose
/// unpinned build is the flat look alone, both fail.
#[test]
fn the_check_fails_a_widget_that_does_not_follow() {
    use azul_css::{dynamic_selector::CssPropertyWithConditions as P, props::basic::color::ColorU};
    fn face(theme: UiTheme) -> Dom {
        let v = match theme {
            UiTheme::Flat => 200,
            UiTheme::Flora => 100,
        };
        Dom::create_div().with_css_props(
            vec![P::simple(CssProperty::const_text_color(
                azul_css::props::style::StyleTextColor {
                    inner: ColorU::rgb(v, v, v),
                },
            ))]
            .into(),
        )
    }
    let flat_always = std::panic::catch_unwind(|| {
        assert_follows_the_app_theme("fixture", |t| face(t.unwrap_or(UiTheme::Flat)));
    });
    assert!(flat_always.is_err(), "an unpinned build that is always flat must fail");
}

#[test]
fn dialogs_modals_and_popovers_follow_the_app_theme() {
    use azul_layout::widgets::{dialog::Dialog, modal::Modal, popover::Popover};
    let body = || Dom::create_div().with_child(Dom::create_p_with_text("Body"));
    assert_follows_the_app_theme("dialog", |t| {
        pinned(
            Dialog::create(body())
                .with_title(AzString::from_const_str("Settings"))
                .show_modal(),
            t,
            Dialog::with_theme,
        )
        .dom()
    });
    assert_follows_the_app_theme("non-modal dialog", |t| {
        pinned(
            Dialog::create(body()).with_invoker(Dom::create_p_with_text("Open")),
            t,
            Dialog::with_theme,
        )
        .dom()
    });
    assert_follows_the_app_theme("modal", |t| {
        pinned(
            Modal::create(body())
                .with_title(AzString::from_const_str("Settings"))
                .with_open(true),
            t,
            Modal::with_theme,
        )
        .dom()
    });
    assert_follows_the_app_theme("popover", |t| {
        pinned(
            Popover::new(Dom::create_p_with_text("Anchor"), body()).with_open(true),
            t,
            Popover::with_theme,
        )
        .dom()
    });
}

#[test]
fn tooltips_follow_the_app_theme() {
    use azul_layout::widgets::tooltip::Tooltip;
    assert_follows_the_app_theme("tooltip", |t| {
        pinned(
            Tooltip::new(
                Dom::create_p_with_text("Anchor"),
                AzString::from_const_str("Explains it"),
            ),
            t,
            Tooltip::with_theme,
        )
        .dom()
    });
}

#[test]
fn split_panes_follow_the_app_theme() {
    use azul_layout::widgets::split_pane::{SplitDirection, SplitPane};
    for dir in [SplitDirection::Horizontal, SplitDirection::Vertical] {
        assert_follows_the_app_theme(&format!("split pane {dir:?}"), |t| {
            pinned(
                SplitPane::create(
                    dir,
                    Dom::create_p_with_text("First"),
                    Dom::create_p_with_text("Second"),
                ),
                t,
                SplitPane::with_theme,
            )
            .dom()
        });
    }
}

#[test]
fn radio_groups_follow_the_app_theme() {
    use azul_css::StringVec;
    use azul_layout::widgets::radio_group::RadioGroup;
    for horizontal in [false, true] {
        assert_follows_the_app_theme(&format!("radio group horizontal={horizontal}"), |t| {
            pinned(
                RadioGroup::create(StringVec::from_vec(vec![
                    AzString::from("First"),
                    AzString::from("Second"),
                    AzString::from("Third"),
                ]))
                .with_selected_index(1)
                .with_horizontal(horizontal)
                .with_accessibility_name("Choice"),
                t,
                RadioGroup::with_theme,
            )
            .dom()
        });
    }
}

#[test]
fn time_pickers_follow_the_app_theme() {
    use azul_layout::widgets::time_picker::TimePicker;
    for is_24h in [true, false] {
        assert_follows_the_app_theme(&format!("time picker 24h={is_24h}"), |t| {
            pinned(
                TimePicker::create(9, 30)
                    .with_24h(is_24h)
                    .with_accessibility_name("Alarm"),
                t,
                TimePicker::with_theme,
            )
            .dom()
        });
    }
}

#[test]
fn toasts_follow_the_app_theme() {
    use azul_layout::widgets::toast::{Toast, ToastKind};
    for kind in [
        ToastKind::Info,
        ToastKind::Success,
        ToastKind::Warning,
        ToastKind::Danger,
    ] {
        for dismissible in [true, false] {
            assert_follows_the_app_theme(
                &format!("toast {kind:?} dismissible={dismissible}"),
                |t| {
                    pinned(
                        Toast::with_kind(AzString::from_const_str("Saved"), kind)
                            .with_dismissible(dismissible),
                        t,
                        Toast::with_theme,
                    )
                    .dom()
                },
            );
        }
    }
}

#[test]
fn paginations_follow_the_app_theme() {
    use azul_layout::widgets::pagination::Pagination;
    for (current, total) in [(1, 1), (1, 4), (2, 4), (4, 4)] {
        assert_follows_the_app_theme(&format!("pagination {current}/{total}"), |t| {
            pinned(Pagination::create(current, total), t, Pagination::with_theme).dom()
        });
    }
}

#[test]
fn segmented_controls_follow_the_app_theme() {
    use azul_css::StringVec;
    use azul_layout::widgets::segmented::Segmented;
    let labels = |n: usize| {
        StringVec::from_vec(
            ["Day", "Week", "Month", "Year"][..n]
                .iter()
                .map(|s| AzString::from(*s))
                .collect(),
        )
    };
    for n in [1usize, 2, 4] {
        for selected in [0usize, n - 1] {
            assert_follows_the_app_theme(&format!("segmented {selected} of {n}"), |t| {
                pinned(
                    Segmented::create(labels(n)).with_selected_index(selected),
                    t,
                    Segmented::with_theme,
                )
                .dom()
            });
        }
    }
}

#[test]
fn steppers_follow_the_app_theme() {
    use azul_css::StringVec;
    use azul_layout::widgets::stepper::Stepper;
    let labels = |n: usize| {
        StringVec::from_vec(
            ["Start", "Details", "Review", "Done"][..n]
                .iter()
                .map(|s| AzString::from(*s))
                .collect(),
        )
    };
    for n in [1usize, 2, 4] {
        for current in [0usize, n - 1] {
            assert_follows_the_app_theme(&format!("stepper {current} of {n}"), |t| {
                pinned(
                    Stepper::create(labels(n)).with_current_step(current),
                    t,
                    Stepper::with_theme,
                )
                .dom()
            });
        }
    }
}

#[test]
fn number_inputs_follow_the_app_theme() {
    use azul_layout::widgets::number_input::NumberInput;
    for value in [0.0f32, 42.5, -3.0] {
        assert_follows_the_app_theme(&format!("number input {value}"), |t| {
            pinned(
                NumberInput::create(value).with_accessibility_name("Amount"),
                t,
                NumberInput::with_theme,
            )
            .dom()
        });
    }
}

#[test]
fn text_inputs_of_every_kind_follow_the_app_theme() {
    use azul_layout::widgets::text_input::{TextInput, TextInputKind};
    for kind in [
        TextInputKind::Text,
        TextInputKind::Password,
        TextInputKind::Search,
        TextInputKind::Email,
        TextInputKind::Tel,
        TextInputKind::Url,
    ] {
        for text in ["", "abc"] {
            assert_follows_the_app_theme(&format!("text input {kind:?} {text:?}"), |t| {
                pinned(
                    TextInput::create_with_kind(kind)
                        .with_text(AzString::from_const_str(text))
                        .with_accessibility_name("Field"),
                    t,
                    TextInput::with_theme,
                )
                .dom()
            });
        }
    }
}

/// A constrained field built unpinned for flora is marked flora's, so its
/// handlers paint the invalid ring in flora's colours.
#[test]
fn an_unpinned_constrained_field_built_for_flora_is_marked_flora() {
    use azul_core::dom::IdOrClass;
    use azul_layout::widgets::text_input::{TextInput, THEME_FLAT_CLASS, THEME_FLORA_CLASS};
    let marked = |dom: &Dom, class: &str| {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, IdOrClass::Class(s) if s.as_str() == class))
    };
    let flora = built_for(UiTheme::Flora, &|| TextInput::create_email().dom());
    assert!(marked(&flora, THEME_FLORA_CLASS) && !marked(&flora, THEME_FLAT_CLASS));
    let flat = built_for(UiTheme::Flat, &|| TextInput::create_email().dom());
    assert!(marked(&flat, THEME_FLAT_CLASS) && !marked(&flat, THEME_FLORA_CLASS));
}
