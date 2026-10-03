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
//! * ... also when the theme chain holds the OTHER compiled-in theme behind T (`[flora, flat]`:
//!   the app default is the chain's implicit last entry). Each compiled-in theme is a complete
//!   look, so only the first one in the chain is live: flat must not fill the gaps of flora's
//!   look (a property flat declares and flora does not stays flora's - undeclared);
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

/// `node`'s inline style under the theme chain `chain` (most specific first),
/// the colour scheme and `state` (`None`: at rest): the declarations in the
/// cascade's order, last match wins per property.
fn resolve(
    node: &Dom,
    chain: &[&str],
    dark: bool,
    state: Option<PseudoStateType>,
) -> Vec<(CssPropertyType, CssProperty)> {
    let mut ctx = DynamicSelectorContext {
        mode: if dark {
            azul_css::system::DarkLightMode::Dark
        } else {
            azul_css::system::DarkLightMode::Light
        },
        ..Default::default()
    };
    ctx.theme_chain = azul_css::StringVec::from_vec(
        chain
            .iter()
            .map(|n| AzString::from((*n).to_string()))
            .collect(),
    );
    let mut out: Vec<(CssPropertyType, CssProperty)> = Vec::new();
    // The cascade's own order: theme rank, then source order.
    let mut in_order = Vec::new();
    node.root
        .style
        .inline_properties_in_cascade_order(|c| ctx.cascade_rank(c), &mut in_order);
    for (p, conds) in in_order {
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

/// The accessibility tree: every accessibility declaration, in tree order
/// (a theme may draw a part with more or fewer presentational nodes - a
/// spinner's spokes - without changing what a screen reader hears).
fn a11y(dom: &Dom) -> Vec<Box<azul_core::a11y::AccessibilityInfo>> {
    nodes(dom)
        .into_iter()
        .filter_map(|(_, n)| n.root.accessibility.clone())
        .collect()
}

/// `theme` first, then every other compiled-in widget theme: a chain holding
/// more than one complete look (`[flora, flat]`).
fn with_the_others_behind(theme: UiTheme) -> Vec<&'static str> {
    let mut chain = vec![theme.name()];
    chain.extend(THEMES.iter().filter(|t| **t != theme).map(|t| t.name()));
    chain
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
                    let want = resolve(pnode, &[theme.name()], dark, state);
                    for chain in [vec![theme.name()], with_the_others_behind(theme)] {
                        let got = resolve(fnode, &chain, dark, state);
                        if got != want {
                            bad.push(format!(
                                "{widget} under the chain {chain:?}, dark {dark}, {state:?}: \
                                 node {path} resolves\n      {got:?}\n    \
                                 with_theme({theme:?}) resolves\n      {want:?}"
                            ));
                        }
                    }
                }
            }
        }
    }

    let looks_differ = styles(&built_for(UiTheme::Flat, &|| make(Some(UiTheme::Flat))))
        != styles(&built_for(UiTheme::Flat, &|| make(Some(UiTheme::Flora))));
    // Where the looks differ the DOM carries a theme block - but not
    // necessarily one per theme: a look whose declarations are all generic
    // (the same in the other theme) has nothing of its own, only the base
    // outside every block (user ruling: generic rules stay outside `@theme`).
    if looks_differ {
        for theme in THEMES {
            let names = theme_names(&built_for(theme, &|| make(None)));
            if names.is_empty() {
                bad.push(format!(
                    "{widget} built for {theme:?} carries no theme block although its looks differ"
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

#[test]
fn text_areas_follow_the_app_theme() {
    use azul_layout::widgets::{text_area::TextArea, themes::OptionUiTheme};
    assert_eq!(
        TextArea::create().theme,
        OptionUiTheme::None,
        "a fresh text area has no theme opinion: it follows the app"
    );
    for text in ["", "line one\nline two"] {
        assert_follows_the_app_theme(&format!("text area {text:?}"), |t| {
            pinned(
                TextArea::create()
                    .with_text(AzString::from_const_str(text))
                    .with_accessibility_name("Notes"),
                t,
                TextArea::with_theme,
            )
            .dom()
        });
    }
}

#[test]
fn sliders_follow_the_app_theme() {
    use azul_layout::widgets::{slider::Slider, themes::OptionUiTheme};
    assert_eq!(
        Slider::create(0.0, 0.0, 100.0).theme,
        OptionUiTheme::None,
        "a fresh slider has no theme opinion: it follows the app"
    );
    for value in [0.0f32, 40.0, 100.0] {
        assert_follows_the_app_theme(&format!("slider {value}"), |t| {
            pinned(
                Slider::create(value, 0.0, 100.0).with_accessibility_name("Volume"),
                t,
                Slider::with_theme,
            )
            .dom()
        });
    }
}

/// Flat's and flora's switch are one look today (`themes::flat::switch` ==
/// `themes::flora::switch`), so this is a guard, green before and after the
/// migration: the moment flora's switch gets a look of its own, an unpinned
/// switch must carry it.
#[test]
fn switches_follow_the_app_theme() {
    use azul_layout::widgets::switch::Switch;
    for checked in [false, true] {
        assert_follows_the_app_theme(&format!("switch checked={checked}"), |t| {
            pinned(
                Switch::create(checked).with_accessibility_name("Wi-Fi"),
                t,
                Switch::with_theme,
            )
            .dom()
        });
    }
}

#[test]
fn spinners_follow_the_app_theme() {
    use azul_css::props::basic::color::ColorU;
    use azul_layout::widgets::spinner::{Spinner, SpinnerStyle};
    for style in [SpinnerStyle::Auto, SpinnerStyle::Spokes, SpinnerStyle::Ring] {
        for track in [false, true] {
            assert_follows_the_app_theme(&format!("spinner {style:?} track={track}"), |t| {
                let s = Spinner::create().with_indicator(style);
                let s = if track {
                    s.with_track_color(ColorU::rgb(200, 200, 200))
                } else {
                    s
                };
                pinned(s, t, Spinner::with_theme).dom()
            });
        }
    }
}

/// A progress bar's look is rendered by its `VirtualView`; `render_bar` is
/// the same tree without the fast path, and follows the app theme the same
/// way (the `VirtualView` path is pinned by the widget's own tests).
#[test]
fn progress_bars_follow_the_app_theme() {
    use azul_layout::widgets::{progressbar::ProgressBar, themes::OptionUiTheme};
    assert_eq!(
        ProgressBar::create(0.0).theme,
        OptionUiTheme::None,
        "a fresh bar has no theme opinion: it follows the app"
    );
    for percent in [0.0f32, 40.0, 100.0] {
        assert_follows_the_app_theme(&format!("progress bar {percent}"), |t| {
            pinned(
                ProgressBar::create(percent).with_accessibility_name("Upload"),
                t,
                ProgressBar::with_theme,
            )
            .render_bar()
        });
    }
}

/// A video's own chrome is its "no signal" poster, drawn by its
/// `VirtualView` (pinned by the widget's own tests); the outer DOM carries
/// the theme marker the structure theme puts on it.
#[test]
fn videos_follow_the_app_theme() {
    use azul_core::video::VideoConfig;
    use azul_layout::widgets::video::VideoWidget;
    assert_follows_the_app_theme("video", |t| {
        pinned(
            VideoWidget::create(VideoConfig::default()),
            t,
            VideoWidget::with_theme,
        )
        .dom()
    });
}

#[test]
fn comboboxes_follow_the_app_theme() {
    use azul_css::StringVec;
    use azul_layout::widgets::combobox::ComboBox;
    for text in ["", "Two"] {
        assert_follows_the_app_theme(&format!("combobox {text:?}"), |t| {
            pinned(
                ComboBox::new(StringVec::from_vec(vec![
                    AzString::from("One"),
                    AzString::from("Two"),
                ]))
                .with_text(AzString::from_const_str(text))
                .with_accessibility_name("Pick"),
                t,
                ComboBox::with_theme,
            )
            .dom()
        });
    }
}

#[test]
fn file_inputs_follow_the_app_theme() {
    use azul_css::OptionString;
    use azul_layout::widgets::file_input::FileInput;
    for path in [None, Some("/tmp/report.pdf")] {
        assert_follows_the_app_theme(&format!("file input {path:?}"), |t| {
            let path = match path {
                Some(p) => OptionString::Some(AzString::from(p.to_string())),
                None => OptionString::None,
            };
            pinned(FileInput::create(path), t, FileInput::with_theme).dom()
        });
    }
}


// ---------------------------------------------------------------------------
// The Office chrome (W5a): ribbon, quick-access title band, status bar
// ---------------------------------------------------------------------------

extern "C" fn chrome_noop(
    _: azul_core::refany::RefAny,
    _: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    azul_core::callbacks::Update::DoNothing
}

/// A ribbon with every part a look paints - the application button, a
/// selected and an unselected tab, a large, a small and a toggled button, a
/// separator, a dialog launcher, a gallery with a selected cell - and an
/// embedded combobox the caller left unpinned.
fn chrome_ribbon() -> azul_layout::widgets::ribbon::Ribbon {
    use azul_css::StringVec;
    use azul_layout::widgets::{
        combobox::ComboBox,
        ribbon::{
            Ribbon, RibbonAppButton, RibbonArrow, RibbonButton, RibbonColumn, RibbonGallery,
            RibbonGalleryCell, RibbonGroup, RibbonItem, RibbonTab, RibbonTabVec,
        },
    };
    let cells: Vec<RibbonGalleryCell> = (0..3)
        .map(|i| RibbonGalleryCell::new(Dom::create_div(), AzString::from(format!("Style {i}"))))
        .collect();
    let clipboard = RibbonGroup::new(AzString::from("Clipboard"))
        .with_item(RibbonItem::LargeButton(
            RibbonButton::new(AzString::from("content_paste"), AzString::from("Paste"))
                .with_arrow(RibbonArrow::Split),
        ))
        .with_item(RibbonItem::Column(
            RibbonColumn::new()
                .with_item(RibbonItem::SmallButton(RibbonButton::new(
                    AzString::from("content_cut"),
                    AzString::from("Cut"),
                )))
                .with_item(RibbonItem::SmallButton(
                    RibbonButton::new(AzString::from("format_bold"), AzString::from(""))
                        .with_toggled(true),
                )),
        ))
        .with_item(RibbonItem::Separator)
        .with_launcher(
            azul_core::refany::RefAny::new(0u8),
            chrome_noop as azul_layout::widgets::button::ButtonOnClickCallbackType,
        );
    let font = RibbonGroup::new(AzString::from("Font")).with_item(RibbonItem::Combo(
        ComboBox::new(StringVec::from_vec(vec![AzString::from("Calibri")]))
            .with_accessibility_name("Font"),
    ));
    let styles = RibbonGroup::new(AzString::from("Styles")).with_item(RibbonItem::Gallery(
        RibbonGallery::new(cells.into()).with_selected(1),
    ));
    Ribbon::new(RibbonTabVec::from_vec(vec![
        RibbonTab::new(AzString::from("HOME"))
            .with_group(clipboard)
            .with_group(font)
            .with_group(styles),
        RibbonTab::new(AzString::from("INSERT")),
    ]))
    .with_app_button(RibbonAppButton::new(AzString::from("FILE")))
}

/// A status bar with an inert, an icon and a live (marked) segment, the view
/// switcher and the zoom cluster.
fn chrome_status_bar() -> azul_layout::widgets::statusbar::StatusBar {
    use azul_layout::widgets::statusbar::{
        StatusBar, StatusBarSegment, StatusBarSegmentVec, StatusBarViewSwitcher, StatusBarZoom,
    };
    StatusBar::new(StatusBarSegmentVec::from_vec(vec![
        StatusBarSegment::new(AzString::from("PAGE 1 OF 1")),
        StatusBarSegment::new(AzString::from("0 WORDS")).with_marker(AzString::from("words")),
        StatusBarSegment::new(AzString::from("ENGLISH")).with_icon(AzString::from("spellcheck")),
    ]))
    .with_views(StatusBarViewSwitcher::office_2013())
    .with_zoom(StatusBarZoom::office_2013())
}

/// The Office title band with a leading slot.
fn chrome_quick_access() -> azul_layout::widgets::quick_access::QuickAccessBar {
    azul_layout::widgets::quick_access::QuickAccessBar::office_2013(AzString::from(
        "Document1 - AzWriter",
    ))
    .with_leading(Dom::create_div())
}

#[test]
fn ribbons_follow_the_app_theme_in_every_chrome() {
    use azul_layout::widgets::ribbon::Ribbon;
    assert_follows_the_app_theme("ribbon", |t| {
        pinned(chrome_ribbon(), t, Ribbon::with_theme).dom()
    });
    assert_follows_the_app_theme("ribbon (desktop chrome)", |t| {
        pinned(chrome_ribbon(), t, Ribbon::with_theme).dom_desktop()
    });
    assert_follows_the_app_theme("ribbon (touch chrome)", |t| {
        pinned(chrome_ribbon(), t, Ribbon::with_theme).dom_mobile()
    });
}

#[test]
fn status_bars_follow_the_app_theme() {
    use azul_layout::widgets::statusbar::StatusBar;
    assert_follows_the_app_theme("status bar", |t| {
        pinned(chrome_status_bar(), t, StatusBar::with_theme).dom()
    });
}

#[test]
fn quick_access_bars_follow_the_app_theme() {
    use azul_layout::widgets::quick_access::QuickAccessBar;
    assert_follows_the_app_theme("quick access bar", |t| {
        pinned(chrome_quick_access(), t, QuickAccessBar::with_theme).dom()
    });
}

/// The follow checks above hold trivially for a widget whose two looks are
/// one (they then need no theme blocks). The chrome widgets must not be that
/// widget: each has a flora look of its own.
#[test]
fn the_ribbon_quick_access_bar_and_status_bar_each_have_a_flora_look_of_their_own() {
    let looks: Vec<(&str, Box<dyn Fn(UiTheme) -> Dom>)> = vec![
        (
            "ribbon",
            Box::new(|t: UiTheme| chrome_ribbon().with_theme(t).dom()),
        ),
        (
            "status bar",
            Box::new(|t: UiTheme| chrome_status_bar().with_theme(t).dom()),
        ),
        (
            "quick access bar",
            Box::new(|t: UiTheme| chrome_quick_access().with_theme(t).dom()),
        ),
    ];
    for (name, look) in &looks {
        let flat = built_for(UiTheme::Flat, &|| look(UiTheme::Flat));
        let flora = built_for(UiTheme::Flat, &|| look(UiTheme::Flora));
        assert!(styles(&flat) != styles(&flora), "the {name} has no flora look of its own");
    }
}

/// A tree view follows the app theme (W5b): every row, icon, label and
/// children container of an unpinned tree resolves like the tree pinned to
/// the app theme - selected rows, open and closed parents, leaves, with and
/// without a click handler (which is what makes the rows Tab stops).
#[test]
fn tree_views_follow_the_app_theme() {
    use azul_core::{callbacks::Update, refany::RefAny};
    use azul_layout::{
        callbacks::CallbackInfo,
        widgets::tree_view::{TreeView, TreeViewNode, TreeViewOnNodeClickCallbackType},
    };
    extern "C" fn pick(_: RefAny, _: CallbackInfo, _: usize) -> Update {
        Update::DoNothing
    }
    let tree = || {
        TreeViewNode::new("Root")
            .with_expanded(true)
            .with_child(
                TreeViewNode::new("Picked")
                    .with_selected(true)
                    .with_child(TreeViewNode::new("Inner")),
            )
            .with_child(
                TreeViewNode::new("Open")
                    .with_expanded(true)
                    .with_child(TreeViewNode::new("Leaf")),
            )
            .with_child(TreeViewNode::new("Plain"))
    };
    for clickable in [false, true] {
        assert_follows_the_app_theme(&format!("tree view clickable={clickable}"), |t| {
            let tv = TreeView::new(tree());
            let tv = if clickable {
                tv.with_on_node_click(RefAny::new(()), pick as TreeViewOnNodeClickCallbackType)
            } else {
                tv
            };
            pinned(tv, t, TreeView::with_theme).dom()
        });
    }
}

/// A tab bar and its panel follow the app theme (W5b): every spacer and tab
/// of an unpinned bar - the active one, its two neighbours, the rest, an
/// out-of-range index, with and without a click handler (which is what makes
/// the tabs Tab stops) - and the panel with and without padding resolve like
/// the widget pinned to the app theme.
#[test]
fn tab_bars_and_their_panels_follow_the_app_theme() {
    use azul_core::{callbacks::Update, refany::RefAny};
    use azul_css::StringVec;
    use azul_layout::{
        callbacks::CallbackInfo,
        widgets::tabs::{TabContent, TabHeader, TabHeaderState, TabOnClickCallbackType},
    };
    extern "C" fn pick(_: RefAny, _: CallbackInfo, _: TabHeaderState) -> Update {
        Update::DoNothing
    }
    let labels = || {
        StringVec::from_vec(vec![
            AzString::from("One"),
            AzString::from("Two"),
            AzString::from("Three"),
            AzString::from("Four"),
        ])
    };
    for active in [0usize, 1, 3, 4] {
        for clickable in [false, true] {
            assert_follows_the_app_theme(
                &format!("tab bar, tab {active} active, clickable={clickable}"),
                |t| {
                    let bar = TabHeader::create(labels()).with_active_tab(active);
                    let bar = if clickable {
                        bar.with_on_click(RefAny::new(()), pick as TabOnClickCallbackType)
                    } else {
                        bar
                    };
                    pinned(bar, t, TabHeader::with_theme).dom()
                },
            );
        }
    }
    for padding in [true, false] {
        assert_follows_the_app_theme(&format!("tab panel padding={padding}"), |t| {
            pinned(
                TabContent::new(Dom::create_p_with_text("Body")).with_padding(padding),
                t,
                TabContent::with_theme,
            )
            .dom()
        });
    }
}

/// A titlebar follows the app theme (W5b) in every shape the shell builds it
/// in - the title-only bar, the CSD row with its controls on either side, the
/// controls alone - whether the desktop coloured it (`from_system_style`,
/// `from_system_style_csd`) or not (`create`). The shell injects these bars
/// unpinned, so under a flora app theme the window's own chrome is flora's.
#[test]
fn titlebars_follow_the_app_theme_in_every_shape() {
    use azul_css::system::{defaults, TitlebarButtonSide, TitlebarButtons};
    use azul_layout::widgets::titlebar::Titlebar;
    let all = TitlebarButtons {
        has_close: true,
        has_minimize: true,
        has_maximize: true,
        has_fullscreen: false,
    };
    fn create() -> Titlebar {
        Titlebar::create(AzString::from("Window"))
    }
    fn macos() -> Titlebar {
        Titlebar::from_system_style(AzString::from("Window"), &defaults::macos_modern_light())
    }
    fn gnome_csd() -> Titlebar {
        Titlebar::from_system_style_csd(
            AzString::from("Window"),
            &defaults::gnome_adwaita_light(),
        )
    }
    let bars: [(&str, fn() -> Titlebar); 3] =
        [("create", create), ("macos", macos), ("gnome csd", gnome_csd)];
    for (name, bar) in bars {
        assert_follows_the_app_theme(&format!("{name} titlebar"), |t| {
            pinned(bar(), t, Titlebar::with_theme).dom()
        });
        for side in [TitlebarButtonSide::Left, TitlebarButtonSide::Right] {
            assert_follows_the_app_theme(&format!("{name} csd titlebar {side:?}"), |t| {
                pinned(bar(), t, Titlebar::with_theme).dom_with_buttons(&all, side)
            });
            assert_follows_the_app_theme(&format!("{name} window controls {side:?}"), |t| {
                pinned(bar(), t, Titlebar::with_theme).dom_controls_only(&all, side)
            });
        }
    }
}

// ---------------------------------------------------------------------------
// The backstage (W5c)
// ---------------------------------------------------------------------------

extern "C" fn backstage_nav_noop(
    _: azul_core::refany::RefAny,
    _: azul_layout::callbacks::CallbackInfo,
    _: usize,
) -> azul_core::callbacks::Update {
    azul_core::callbacks::Update::DoNothing
}

/// The Office backstage with every part a look paints - the back button,
/// the selected item, a plain one and the one after the gap - wired the way
/// an app wires it (back and nav handlers), with a title strip and a pane of
/// the caller's.
fn chrome_backstage(active: usize) -> azul_layout::widgets::backstage::Backstage {
    use azul_layout::widgets::{
        backstage::{Backstage, BackstageOnNavSelectCallbackType},
        button::ButtonOnClickCallbackType,
    };
    Backstage::office_2013()
        .with_active_item(active)
        .with_on_back(
            azul_core::refany::RefAny::new(0u8),
            chrome_noop as ButtonOnClickCallbackType,
        )
        .with_on_nav_select(
            azul_core::refany::RefAny::new(0u8),
            backstage_nav_noop as BackstageOnNavSelectCallbackType,
        )
        .with_title_strip(Dom::create_div())
        .with_content(Dom::create_p_with_text("Recent documents"))
}

/// An unpinned backstage follows the app theme (W5c): its column, its back
/// button, every nav item - the selected one plain ("Open") and after the
/// gap ("Account") - and the host of the caller's pane resolve like the
/// backstage pinned to the app theme.
#[test]
fn backstages_follow_the_app_theme() {
    use azul_layout::widgets::backstage::Backstage;
    for active in [2usize, 9] {
        assert_follows_the_app_theme(&format!("backstage, item {active} active"), |t| {
            pinned(chrome_backstage(active), t, Backstage::with_theme).dom()
        });
    }
}

/// The follow check above holds trivially for a widget whose two looks are
/// one: the backstage must not be that widget.
#[test]
fn the_backstage_has_a_flora_look_of_its_own() {
    let flat = built_for(UiTheme::Flat, &|| {
        chrome_backstage(2).with_theme(UiTheme::Flat).dom()
    });
    let flora = built_for(UiTheme::Flat, &|| {
        chrome_backstage(2).with_theme(UiTheme::Flora).dom()
    });
    assert!(
        styles(&flat) != styles(&flora),
        "the backstage has no flora look of its own"
    );
}
