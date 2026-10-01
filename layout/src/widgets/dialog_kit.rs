//! Dialog kit - the parts every dialog-shaped widget shares: the install
//! wizard's pages ([`crate::widgets::wizard_pages`]), the path field
//! ([`crate::widgets::path_input`]), the shortcut recorder
//! ([`crate::widgets::shortcut_recorder`]), the settings rows
//! ([`crate::widgets::shells::settings_dialog`]) and the standard dialogs
//! ([`crate::widgets::standard_dialogs`]).
//!
//! They are ONE design - an Office dialog: a page of text and fields, a
//! bordered box that scrolls (a license, a log, a component list), rows a
//! hairline apart, a hint ink for sizes and help, a button row on a strip -
//! so a theme decides about them once: [`DialogKitLook`] is the skin of
//! every part, built by `themes::flat::dialog_kit_look` and
//! `themes::flora::dialog_kit_look`. The STRUCTURE of every part (a row, a
//! column that scrolls, a label that grows) is the kit's own, the `*_BASE`
//! statics here and the shells' bases, the same under every theme (R5).
//! A widget that follows the app theme merges the two skins part by part
//! ([`follow_look`], the shells' shape) and builds once.
//!
//! Key types: [`DialogKitLook`].

use alloc::vec::Vec;

use azul_core::dom::{Dom, DomVec, IdOrClass, IdOrClass::Class, IdOrClassVec, NodeType};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::length::FloatValue,
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow, LayoutFlexShrink,
            LayoutMinHeight, LayoutMinWidth, LayoutOverflow,
        },
        property::CssProperty,
        style::{StyleCursor, StyleUserSelect},
    },
    AzString,
};

use crate::widgets::themes::{OptionUiTheme, UiTheme};

/// What a theme decides about the dialog widgets: the SKIN of every part,
/// laid over the part's base (its structure, the kit's own) by each
/// widget's build; built by `themes::flat::dialog_kit_look` and
/// `themes::flora::dialog_kit_look`.
///
/// Every field is one part's paint and metrics (colours, borders, padding,
/// widths, fonts, the hover / focus states), never its structure.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct DialogKitLook {
    // ---- text ----
    /// A page: the column a wizard page or a dialog body is (its padding).
    pub page: Vec<CssPropertyWithConditions>,
    /// A heading on a page ("Welcome to the AzOffice Setup Wizard").
    pub heading: Vec<CssPropertyWithConditions>,
    /// Body text.
    pub text: Vec<CssPropertyWithConditions>,
    /// Secondary text: a size, a help line, a description.
    pub hint: Vec<CssPropertyWithConditions>,
    /// The big glyph of a welcome page or an About box.
    pub logo: Vec<CssPropertyWithConditions>,
    /// A block's spacing on a page (the gap under it).
    pub block: Vec<CssPropertyWithConditions>,
    /// A field's label over or beside the field.
    pub label: Vec<CssPropertyWithConditions>,

    // ---- boxes and lists ----
    /// A bordered box on the field colour that scrolls: a license, a log, a
    /// component list. A keyboard stop (its focus ring) when it scrolls.
    pub scroll_box: Vec<CssPropertyWithConditions>,
    /// A row of a list box: a hairline under it.
    pub list_row: Vec<CssPropertyWithConditions>,
    /// A size set right in a row ("120 MB").
    pub size: Vec<CssPropertyWithConditions>,
    /// The total under a list ("Space required: 1.2 GB").
    pub total: Vec<CssPropertyWithConditions>,
    /// A row of a checkbox (or a radio) and its label.
    pub check_row: Vec<CssPropertyWithConditions>,
    /// The label beside a checkbox (a click on it toggles the box).
    pub check_label: Vec<CssPropertyWithConditions>,
    /// The description under an option, indented to its label.
    pub description: Vec<CssPropertyWithConditions>,
    /// The key column of a summary row ("Destination folder").
    pub summary_key: Vec<CssPropertyWithConditions>,
    /// A summary row.
    pub summary_row: Vec<CssPropertyWithConditions>,

    // ---- settings rows ----
    /// A setting row: the label column beside the control, a hairline under.
    pub field_row: Vec<CssPropertyWithConditions>,
    /// The label column of a setting row.
    pub field_label: Vec<CssPropertyWithConditions>,
    /// The help line under a setting's label.
    pub help: Vec<CssPropertyWithConditions>,
    /// A search match inside a label or a help line (`<mark>`).
    pub mark: Vec<CssPropertyWithConditions>,
    /// The dot that marks a setting changed but not applied yet.
    pub modified: Vec<CssPropertyWithConditions>,
    /// A unit or a value read-out beside a control ("ms", "75 %").
    pub unit: Vec<CssPropertyWithConditions>,

    // ---- the shortcut recorder ----
    /// The recorder's field at rest, its hover and focus states.
    pub recorder: Vec<CssPropertyWithConditions>,
    /// Added while the recorder listens for a chord.
    pub recorder_recording: Vec<CssPropertyWithConditions>,

    // ---- dialogs ----
    /// A dialog body (its padding, its width).
    pub dialog: Vec<CssPropertyWithConditions>,
    /// The glyph of an information message.
    pub icon_info: Vec<CssPropertyWithConditions>,
    /// The glyph of a warning.
    pub icon_warning: Vec<CssPropertyWithConditions>,
    /// The glyph of an error.
    pub icon_error: Vec<CssPropertyWithConditions>,
    /// The glyph of a question.
    pub icon_question: Vec<CssPropertyWithConditions>,
    /// A dialog's button row: a strip under a hairline.
    pub buttons: Vec<CssPropertyWithConditions>,
    /// The box around one button of a row (its spacing).
    pub button: Vec<CssPropertyWithConditions>,
    /// Added to the box of an inert button (dimmed).
    pub held: Vec<CssPropertyWithConditions>,
    /// A notice in a button row ("Restart to apply some changes.").
    pub notice: Vec<CssPropertyWithConditions>,
    /// The glyph before a settings category's name.
    pub category_icon: Vec<CssPropertyWithConditions>,

    /// The theme's marker class on every root, if it has one.
    pub marker: Option<&'static str>,
}

/// Both themes' kit looks in one, part by part
/// (`themes::theme_blocks::follow_props`), with the `structure` theme's
/// marker: the look an unpinned dialog widget is built with.
#[must_use]
pub(crate) fn follow_look(structure: UiTheme) -> DialogKitLook {
    use crate::widgets::themes::{flat, flora, theme_blocks::follow_props};
    let (a, b) = (flat::dialog_kit_look(), flora::dialog_kit_look());
    let both = |x: &[CssPropertyWithConditions], y: &[CssPropertyWithConditions]| {
        follow_props(x, y).into_library_owned_vec()
    };
    macro_rules! merged {
        ($($field:ident),* $(,)?) => {
            DialogKitLook {
                $($field: both(a.$field.as_slice(), b.$field.as_slice()),)*
                marker: match structure {
                    UiTheme::Flat => a.marker,
                    UiTheme::Flora => b.marker,
                },
            }
        };
    }
    merged!(
        page,
        heading,
        text,
        hint,
        logo,
        block,
        label,
        scroll_box,
        list_row,
        size,
        total,
        check_row,
        check_label,
        description,
        summary_key,
        summary_row,
        field_row,
        field_label,
        help,
        mark,
        modified,
        unit,
        recorder,
        recorder_recording,
        dialog,
        icon_info,
        icon_warning,
        icon_error,
        icon_question,
        buttons,
        button,
        held,
        notice,
        category_icon,
    )
}

/// The look a dialog widget with the theme option `theme` is built with:
/// the pinned theme's own look, or both looks merged ([`follow_look`]) in
/// the structure of the theme the DOM is being built for.
#[must_use]
pub(crate) fn look_for(theme: OptionUiTheme) -> DialogKitLook {
    use crate::widgets::themes::{flat, flora};
    match theme.into_option() {
        Some(UiTheme::Flat) => flat::dialog_kit_look(),
        Some(UiTheme::Flora) => flora::dialog_kit_look(),
        None => follow_look(UiTheme::current()),
    }
}

/// A part's declarations: its base (the structure), then the look's skin.
#[must_use]
pub(crate) fn part(
    base: &[CssPropertyWithConditions],
    skin: &[CssPropertyWithConditions],
) -> CssPropertyWithConditionsVec {
    crate::widgets::shells::part(base, skin)
}

/// A state part (recording, selected) over `base` - the shells' rule.
#[must_use]
pub(crate) fn stack_state(
    base: &CssPropertyWithConditionsVec,
    extra: &[CssPropertyWithConditions],
) -> CssPropertyWithConditionsVec {
    crate::widgets::shells::stack_state(base, extra)
}

/// A root's classes: the widget class, then the theme marker if the look
/// has one.
#[must_use]
pub(crate) fn root_classes(class: &'static str, look: &DialogKitLook) -> IdOrClassVec {
    let mut classes: Vec<IdOrClass> = alloc::vec![Class(AzString::from_const_str(class))];
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    IdOrClassVec::from_vec(classes)
}

/// One class on a node.
#[must_use]
pub(crate) fn class(name: &'static str) -> IdOrClassVec {
    IdOrClassVec::from_vec(alloc::vec![Class(AzString::from_const_str(name))])
}

/// A widget-owned line of text (`widget_p_with_text`): `base`, then `skin`.
#[must_use]
pub(crate) fn line(
    text: AzString,
    base: &[CssPropertyWithConditions],
    skin: &[CssPropertyWithConditions],
) -> Dom {
    crate::widgets::widget_p_with_text(text).with_css_props(part(base, skin))
}

/// The theme setter for the widgets a dialog widget builds for itself (its
/// Buttons, CheckBoxes, TextInputs): `Some(theme)` when it is pinned;
/// `None` follows.
#[must_use]
pub(crate) const fn inner_theme(theme: OptionUiTheme) -> Option<UiTheme> {
    crate::widgets::shells::inner_theme(theme)
}

/// A button of a dialog's button row, in its box: with a click (`data`
/// and `on_click`), or - with none - INERT: no click, no Tab stop,
/// announced unavailable (described by `reason` when there is one), its box
/// dimmed by the `held` skin. The one shape every dialog widget's buttons
/// take (the wizard's Back / Next, a dialog's OK / Apply).
#[allow(clippy::too_many_arguments)]
#[must_use]
pub(crate) fn row_button(
    label: AzString,
    kind: crate::widgets::button::ButtonType,
    click: Option<(azul_core::refany::RefAny, crate::widgets::button::ButtonOnClickCallbackType)>,
    reason: Option<AzString>,
    theme: Option<UiTheme>,
    classes: (&'static str, &'static str),
    base: &[CssPropertyWithConditions],
    skins: (&[CssPropertyWithConditions], &[CssPropertyWithConditions]),
) -> Dom {
    use azul_core::{
        a11y::{AccessibilityInfo, AccessibilityState, AccessibilityStateVec},
        dom::TabIndex,
    };
    let inert = click.is_none();
    let mut b = crate::widgets::button::Button::with_type(label, kind);
    if let Some((data, on_click)) = click {
        b = b.with_on_click(data, on_click);
    }
    if let Some(theme) = theme {
        b = b.with_theme(theme);
    }
    let mut dom = b.dom();
    let mut ids: Vec<IdOrClass> = alloc::vec![Class(AzString::from_const_str(classes.0))];
    let mut css = part(base, skins.0);
    if inert {
        dom.set_tab_index(TabIndex::NoKeyboardFocus);
        dom = dom.with_accessibility_assign(AccessibilityInfo {
            states: AccessibilityStateVec::from_vec(alloc::vec![AccessibilityState::Unavailable]),
            description: reason.filter(|r| !r.as_str().is_empty()).into(),
            ..Default::default()
        });
        ids.push(Class(AzString::from_const_str(classes.1)));
        css = crate::widgets::themes::theme_blocks::stack_parts(
            &css,
            &CssPropertyWithConditionsVec::from_vec(skins.1.to_vec()),
        );
    }
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(ids))
        .with_css_props(css)
        .with_child(dom)
}

/// The class an inert button's box takes in the kit's rows.
pub const HELD_CLASS: &str = "__azul-native-dialog-kit-held";
/// The class of a button's box in the kit's rows.
pub const BUTTON_BOX_CLASS: &str = "__azul-native-dialog-kit-button";

// ---------------------------------------------------------------------------
// Search matches
// ---------------------------------------------------------------------------

/// The byte range of the first case-insensitive occurrence of `needle` in
/// `hay`, or `None` (an empty needle matches nothing). Compared char by
/// char, each lowercased, so the range always falls on char boundaries of
/// `hay`.
#[must_use]
pub(crate) fn find_ignore_case(hay: &str, needle: &str) -> Option<(usize, usize)> {
    let needle: Vec<char> = needle.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return None;
    }
    for (start, _) in hay.char_indices() {
        let mut matched = 0;
        let mut end = start;
        for (offset, c) in hay[start..].char_indices() {
            let lower: Vec<char> = c.to_lowercase().collect();
            if needle.len() < matched + lower.len()
                || needle[matched..matched + lower.len()] != lower[..]
            {
                break;
            }
            matched += lower.len();
            end = start + offset + c.len_utf8();
            if matched == needle.len() {
                return Some((start, end));
            }
        }
    }
    None
}

/// Whether `hay` contains `needle`, ignoring case (an empty needle: yes).
#[must_use]
pub(crate) fn contains_ignore_case(hay: &str, needle: &str) -> bool {
    needle.trim().is_empty() || find_ignore_case(hay, needle.trim()).is_some()
}

/// A widget-owned line of `text` whose first match of `query` is a
/// `<mark>` in the look's mark part: `[before, mark[match], after]`. No
/// match (or no query) is the plain line.
#[must_use]
pub(crate) fn highlighted_line(
    text: &AzString,
    query: &str,
    base: &[CssPropertyWithConditions],
    skin: &[CssPropertyWithConditions],
    mark: &[CssPropertyWithConditions],
) -> Dom {
    let s = text.as_str();
    let Some((start, end)) = find_ignore_case(s, query.trim()) else {
        return line(text.clone(), base, skin);
    };
    let leaf = |t: &str| Dom::create_text_do_not_use_without_block_level_wrapper(AzString::from(t));
    let mut parts: Vec<Dom> = Vec::with_capacity(3);
    if start > 0 {
        parts.push(leaf(&s[..start]));
    }
    parts.push(
        Dom::create_node(NodeType::Mark)
            .with_ids_and_classes(class(MARK_CLASS))
            .with_css_props(part(&[], mark))
            .with_child(leaf(&s[start..end])),
    );
    if end < s.len() {
        parts.push(leaf(&s[end..]));
    }
    crate::widgets::widget_p_chrome()
        .with_css_props(part(base, skin))
        .with_children(DomVec::from_vec(parts))
}

/// The class of a search match.
pub const MARK_CLASS: &str = "__azul-native-dialog-kit-mark";

// ---------------------------------------------------------------------------
// The bases: the kit's structure, the same in every theme (R5)
// ---------------------------------------------------------------------------

const fn simple(p: CssProperty) -> CssPropertyWithConditions {
    CssPropertyWithConditions::simple(p)
}

/// A row whose parts sit on one midline.
pub(crate) static ROW_MIDDLE_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
];

/// A row whose parts hang from its top (an icon beside a column of text).
pub(crate) static ROW_TOP_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_align_items(LayoutAlignItems::Start)),
];

/// A part that keeps its size in its row (a glyph, a size, a button box).
pub(crate) static FIXED_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// A box that scrolls its column of content, taking the rest of its page.
pub(crate) static SCROLL_BOX_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
    simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    simple(CssProperty::const_overflow_y(LayoutOverflow::Auto)),
];

/// A label a click acts on (a checkbox's label): the pointer, never a text
/// selection.
pub(crate) static CLICK_LABEL_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// A field a click and the keyboard act on (the shortcut recorder): a row
/// on its midline, the pointer, never a text selection.
pub(crate) static FIELD_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// The button row of a dialog: one row on its midline that keeps its
/// height, whose text a drag never selects.
pub(crate) static BUTTON_ROW_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// A button's box keeps its size in its row.
pub(crate) static BUTTON_BOX_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// A spacer that pushes the rest of its row to the right.
pub(crate) static SPACER_BASE: &[CssPropertyWithConditions] = &[simple(
    CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1)),
)];

#[cfg(test)]
mod dialog_kit_tests {
    use super::*;

    #[test]
    fn a_match_is_found_ignoring_case_on_char_boundaries() {
        assert_eq!(
            find_ignore_case("Open the last folder", "LAST"),
            Some((9, 13))
        );
        // "Schrift" is 7 bytes, "gr" 2, the umlaut 2.
        assert_eq!(
            find_ignore_case("Schriftgr\u{f6}\u{df}e", "GR\u{d6}"),
            Some((7, 11))
        );
        assert_eq!(find_ignore_case("Startup", "x"), None);
        assert_eq!(find_ignore_case("Startup", ""), None);
        assert!(contains_ignore_case("Startup", ""));
        assert!(contains_ignore_case("Startup", "  start "));
    }

    #[test]
    fn a_highlighted_line_marks_the_first_match_only() {
        let look = DialogKitLook::default();
        let dom = highlighted_line(
            &AzString::from("Show hidden files"),
            "hid",
            &[],
            &look.text,
            &look.mark,
        );
        let kids = dom.children.as_ref();
        assert_eq!(kids.len(), 3, "before, the mark, after");
        assert!(matches!(kids[1].root.get_node_type(), NodeType::Mark));
        let plain = highlighted_line(&AzString::from("Theme"), "zzz", &[], &look.text, &look.mark);
        assert_eq!(plain.children.as_ref().len(), 1, "no match: one text leaf");
    }

    #[test]
    fn an_unpinned_kit_look_is_the_pinned_one_under_each_app_theme() {
        use crate::widgets::themes::theme_blocks::checks::{under, BOTH};
        for theme in BOTH {
            let merged = under(theme, || look_for(OptionUiTheme::None));
            assert_eq!(
                merged.marker,
                look_for(OptionUiTheme::Some(theme)).marker,
                "{}: the marker is the structure theme's",
                theme.name()
            );
        }
    }
}
