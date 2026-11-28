//! What a ribbon command shows of itself (`RibbonButtonFace`): the icon and the label (`Both`, the
//! Office look), the label alone (`Text`) or the icon alone (`Icon`: the label is the accessible
//! name and the tooltip) - never both but where the app asks for both.
//!
//! Not compiled by the author (house rule).

use azul_core::dom::{Dom, EventFilter, HoverEventFilter, NodeType};
use azul_css::AzString;

use super::*;
use crate::widgets::themes::{theme_checks as tc, UiTheme};

fn s(text: &str) -> AzString {
    AzString::from(text)
}

fn texts(node: &Dom) -> Vec<String> {
    let mut out = Vec::new();
    if let NodeType::Text(t) = node.root.get_node_type() {
        if !t.as_str().is_empty() {
            out.push(t.as_str().to_string());
        }
    }
    for child in node.children.as_ref() {
        out.extend(texts(child));
    }
    out
}

/// The words under `node`, as one line.
fn words(node: &Dom) -> String {
    texts(node)
        .iter()
        .flat_map(|t| t.split_whitespace().map(str::to_string).collect::<Vec<_>>())
        .collect::<Vec<_>>()
        .join(" ")
}

fn icons(node: &Dom) -> Vec<String> {
    let mut out = Vec::new();
    if let NodeType::Icon(icon) = node.root.get_node_type() {
        out.push(icon.as_ref().as_str().to_string());
    }
    for child in node.children.as_ref() {
        out.extend(icons(child));
    }
    out
}

fn name_of(node: &Dom) -> Option<String> {
    node.root
        .get_accessibility_info()
        .and_then(|info| info.accessibility_name.as_ref().map(|name| name.as_str().to_string()))
}

/// The tooltip callbacks a node has: (the pointer enters, a long press, the pointer leaves).
fn tip_events(node: &Dom) -> (usize, usize, usize) {
    let count = |wanted: HoverEventFilter| {
        node.root
            .get_callbacks()
            .as_ref()
            .iter()
            .filter(|cb| matches!(cb.event, EventFilter::Hover(e) if e == wanted))
            .count()
    };
    (
        count(HoverEventFilter::MouseEnter),
        count(HoverEventFilter::LongPress),
        count(HoverEventFilter::MouseLeave),
    )
}

/// One tab with one large and one small button (`Paste special` has two words, so a large button
/// would set it on two lines under its icon).
fn ribbon(face: RibbonButtonFace) -> Ribbon {
    let group = RibbonGroup::new(s("Clipboard"))
        .with_item(RibbonItem::LargeButton(RibbonButton::new(s("content_paste"), s("Paste special"))))
        .with_item(RibbonItem::SmallButton(RibbonButton::new(s("content_copy"), s("Copy"))));
    Ribbon::new(RibbonTabVec::from_vec(vec![
        RibbonTab::new(s("Home")).with_group(group),
    ]))
    .with_tab_style(crate::widgets::tabs::TabStyle::Rectangular)
    .with_theme(UiTheme::Flat)
    .with_button_face(face)
}

/// The two buttons of the tab, large first.
fn buttons_of(dom: &Dom) -> Vec<&Dom> {
    tc::find_all(dom, "__azul-native-button")
}

#[test]
fn a_ribbon_shows_icon_and_label_until_asked_for_one() {
    assert_eq!(Ribbon::new(RibbonTabVec::from_vec(Vec::new())).button_face, RibbonButtonFace::Both);
    assert_eq!(RibbonButtonFace::default(), RibbonButtonFace::Both);
    let dom = ribbon(RibbonButtonFace::Both).dom_desktop();
    let buttons = buttons_of(&dom);
    assert_eq!(buttons.len(), 2);
    // The large button: its icon over the label block; the small one: its icon beside its label.
    assert_eq!(icons(buttons[0]), vec!["content_paste"]);
    assert_eq!(words(buttons[0]), "Paste special");
    assert_eq!(icons(buttons[1]), vec!["content_copy"]);
    assert_eq!(texts(buttons[1]), vec!["Copy"]);
    // Both is today's look: no tooltip on a button that shows its label.
    for button in buttons {
        assert_eq!(tip_events(button), (0, 0, 0));
    }
}

#[test]
fn text_draws_the_label_alone_on_a_large_and_a_small_button() {
    let dom = ribbon(RibbonButtonFace::Text).dom_desktop();
    let buttons = buttons_of(&dom);
    assert_eq!(buttons.len(), 2);
    for (i, button) in buttons.iter().enumerate() {
        assert!(icons(button).is_empty(), "button {i}: no icon");
    }
    // One line of text in the large button, not the two-line label block under an icon.
    assert_eq!(words(buttons[0]), "Paste special");
    assert!(
        tc::find(buttons[0], "__azul-native-ribbon-large-label").is_none(),
        "no stacked label under an icon that is not there"
    );
    assert_eq!(texts(buttons[1]), vec!["Copy"]);
    // The name a screen reader says is the label, as it was.
    assert_eq!(name_of(buttons[1]).as_deref().unwrap_or("Copy"), "Copy");
    // A label that shows is no tooltip.
    for button in buttons {
        assert_eq!(tip_events(button), (0, 0, 0));
    }
}

#[test]
fn a_large_text_button_is_as_tall_as_ever_and_its_line_stands_in_the_middle() {
    let both = ribbon(RibbonButtonFace::Both).dom_desktop();
    let text = ribbon(RibbonButtonFace::Text).dom_desktop();
    let height = |dom: &Dom| {
        crate::widgets::theme_probe::unconditional(buttons_of(dom)[0])
            .iter()
            .rev()
            .find_map(|p| match p {
                P::Height(v) => v.get_property().and_then(|h| match h {
                    LayoutHeight::Px(px) => Some(px.number.get() as isize),
                    _ => None,
                }),
                _ => None,
            })
    };
    assert_eq!(height(&both), height(&text), "the same height");
    assert!(height(&text).is_some());
    let centred = crate::widgets::theme_probe::unconditional(buttons_of(&text)[0])
        .iter()
        .any(|p| matches!(p, P::JustifyContent(v) if v.get_property() == Some(&LayoutJustifyContent::Center)));
    assert!(centred, "a one-line label is centred in the button's height");
}

#[test]
fn icon_draws_the_icon_alone_named_and_with_a_tooltip_by_the_label() {
    let dom = ribbon(RibbonButtonFace::Icon).dom_desktop();
    let buttons = buttons_of(&dom);
    assert_eq!(buttons.len(), 2);
    assert_eq!(icons(buttons[0]), vec!["content_paste"]);
    assert_eq!(icons(buttons[1]), vec!["content_copy"]);
    for (i, button) in buttons.iter().enumerate() {
        assert!(texts(button).is_empty(), "button {i}: its icon alone");
        // The label is the name a screen reader says ...
        assert_eq!(
            name_of(button).as_deref(),
            Some(["Paste special", "Copy"][i]),
            "button {i}"
        );
    }
    // ... and the tooltip: the pointer entering, a long press, the pointer leaving. The callbacks
    // sit on the button's own node (the Button's root), the node a pointer is over.
    for (i, button) in buttons.iter().enumerate() {
        assert_eq!(tip_events(button), (1, 1, 1), "button {i}");
    }
}

#[test]
fn the_tooltip_says_the_label() {
    // The tooltip's text is the one a disabled button's reason is: the Button's own callbacks.
    let dom = ribbon(RibbonButtonFace::Icon).dom_desktop();
    let mut said: Vec<String> = Vec::new();
    for button in buttons_of(&dom) {
        for cb in button.root.get_callbacks().as_ref() {
            let mut data = cb.refany.clone();
            let reason = data
                .downcast_ref::<crate::widgets::button::DisabledReason>()
                .map(|reason| reason.0.as_str().to_string());
            if let Some(reason) = reason {
                said.push(reason);
            }
        }
    }
    said.sort();
    said.dedup();
    assert_eq!(said, vec!["Copy".to_string(), "Paste special".to_string()]);
}

#[test]
fn a_button_with_nothing_else_to_show_keeps_what_it_has() {
    let group = RibbonGroup::new(s("Mixed"))
        .with_item(RibbonItem::SmallButton(RibbonButton::new(s(""), s("No icon"))))
        .with_item(RibbonItem::SmallButton(RibbonButton::new(s("star"), s(""))));
    let build = |face| {
        Ribbon::new(RibbonTabVec::from_vec(vec![
            RibbonTab::new(s("Home")).with_group(group.clone()),
        ]))
        .with_tab_style(crate::widgets::tabs::TabStyle::Rectangular)
        .with_theme(UiTheme::Flat)
        .with_button_face(face)
        .dom_desktop()
    };
    for face in [RibbonButtonFace::Text, RibbonButtonFace::Icon] {
        let dom = build(face);
        let buttons = buttons_of(&dom);
        assert_eq!(buttons.len(), 2, "{face:?}");
        // No icon: the label stays (Icon has nothing to draw instead).
        assert_eq!(texts(buttons[0]), vec!["No icon"], "{face:?}");
        assert!(icons(buttons[0]).is_empty());
        // No label: the icon stays (Text has nothing to draw instead).
        assert_eq!(icons(buttons[1]), vec!["star"], "{face:?}");
        assert!(texts(buttons[1]).is_empty());
    }
}

#[test]
fn buttons_in_columns_and_rows_take_the_face_too() {
    let group = RibbonGroup::new(s("Stacked"))
        .with_item(RibbonItem::Column(RibbonColumn {
            items: RibbonItemVec::from_vec(vec![
                RibbonItem::SmallButton(RibbonButton::new(s("cut"), s("Cut"))),
                RibbonItem::Row(RibbonRow {
                    items: RibbonItemVec::from_vec(vec![RibbonItem::SmallButton(RibbonButton::new(
                        s("undo"),
                        s("Undo"),
                    ))]),
                }),
            ]),
        }));
    let dom = Ribbon::new(RibbonTabVec::from_vec(vec![RibbonTab::new(s("Home")).with_group(group)]))
        .with_tab_style(crate::widgets::tabs::TabStyle::Rectangular)
        .with_theme(UiTheme::Flat)
        .with_button_face(RibbonButtonFace::Icon)
        .dom_desktop();
    let buttons = buttons_of(&dom);
    assert_eq!(buttons.len(), 2);
    for button in buttons {
        assert!(texts(button).is_empty());
        assert_eq!(icons(button).len(), 1);
    }
}

#[test]
fn a_face_only_changes_the_shown_tab_and_never_the_band_height() {
    for face in [RibbonButtonFace::Both, RibbonButtonFace::Text, RibbonButtonFace::Icon] {
        let dom = ribbon(face).dom_desktop();
        let band = tc::find(&dom, "__azul-native-ribbon-content").expect("the band");
        let height = crate::widgets::theme_probe::unconditional(band)
            .iter()
            .rev()
            .find_map(|p| match p {
                P::Height(v) => v.get_property().and_then(|h| match h {
                    LayoutHeight::Px(px) => Some(px.number.get() as isize),
                    _ => None,
                }),
                _ => None,
            });
        assert_eq!(height, Some(crate::widgets::tabs::TAB_BAND_PX), "{face:?}");
    }
}

#[test]
fn a_text_button_is_measured_as_one_line_so_the_scaling_counts_its_real_width() {
    let two_words = RibbonButton::new(s(""), s("Paste special"));
    let one_line = large_button_px(&two_words);
    let stacked = large_button_px(&RibbonButton::new(s("content_paste"), s("Paste special")));
    // With an icon the two words are stacked (the longer of the two lines counts); without one
    // they are one line, and wider.
    assert!(one_line > stacked, "one line {one_line}, stacked {stacked}");
    let single = large_button_px(&RibbonButton::new(s(""), s("Paste")));
    assert!(one_line > single, "two words are wider than one");
}
