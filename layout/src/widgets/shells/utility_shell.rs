//! S9 - Utility window: the calculator, the clock, the colour chooser, the
//! emoji picker, 2FA, weather, QR, the USB writer, sticky notes
//! (04-app-shells.md, S9).
//!
//! ```text
//! ┌ title row (optional) ────────────┐
//! │ [World] [Alarm] [Timer] [Stopwatch] modes (Segmented / TabHeader)
//! ├──────────────────────────────────┤
//! │ content: a grid of controls      │
//! │                                  │
//! └──────────────────────────────────┘
//! ```
//!
//! One small window: an optional title row, an optional mode row and the
//! content (`shell-content`, the `<main>`). The shell asks the theme for
//! the ground and the mode row's paint, and keeps a compact minimum size
//! (calculator.md) through [`UtilityShell::min_width`] /
//! [`UtilityShell::min_height`]. No panes, no F6: a utility is one region.
//!
//! Key types: [`UtilityShell`].

use azul_core::{
    a11y::AccessibilityInfo,
    dom::{Dom, DomVec, NodeType, OptionDom},
};
use azul_css::{
    dynamic_selector::CssPropertyWithConditions,
    props::{
        basic::pixel::PixelValue,
        layout::{
            LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow, LayoutHeight, LayoutMinHeight,
            LayoutMinWidth, LayoutOverflow, LayoutWidth,
        },
        property::CssProperty,
    },
    AzString,
};

use super::{id_and_class, look_for, part, root_classes, ShellLook, CHROME_ROW_BASE, PANE_BASE};
use crate::widgets::themes::{OptionUiTheme, UiTheme};

/// The shell root's class.
pub const UTILITY_CLASS: &str = "__azul-native-utility-shell";
/// The mode row's class.
pub const MODES_CLASS: &str = "__azul-native-utility-shell-modes";
/// The mode row's DOM id.
pub const MODES_ID: &str = "shell-modes";
/// The content's DOM id.
pub const CONTENT_ID: &str = "shell-content";
/// The title row host's DOM id.
pub const TITLE_ID: &str = "shell-title";

/// S9: a title row, a mode row, the content.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct UtilityShell {
    /// The app-drawn title row.
    pub title_row: OptionDom,
    /// The mode row (a `Segmented` or a `TabHeader`).
    pub modes: OptionDom,
    /// The content.
    pub content: Dom,
    /// The content's accessible name.
    pub label: AzString,
    /// The window's compact minimum width in px (0: none).
    pub min_width: f32,
    /// The window's compact minimum height in px (0: none).
    pub min_height: f32,
    /// The widget theme this shell is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
}

impl UtilityShell {
    /// A shell around `content`, nothing else.
    #[must_use]
    pub fn create(content: Dom) -> Self {
        Self {
            title_row: OptionDom::None,
            modes: OptionDom::None,
            content,
            label: AzString::from_const_str("Content"),
            min_width: 0.0,
            min_height: 0.0,
            theme: OptionUiTheme::None,
        }
    }

    /// The app-drawn title row.
    pub fn set_title_row(&mut self, title_row: Dom) {
        self.title_row = OptionDom::Some(title_row);
    }

    /// [`Self::set_title_row`] for the builder chain.
    #[must_use]
    pub fn with_title_row(mut self, title_row: Dom) -> Self {
        self.set_title_row(title_row);
        self
    }

    /// The mode row.
    pub fn set_modes(&mut self, modes: Dom) {
        self.modes = OptionDom::Some(modes);
    }

    /// [`Self::set_modes`] for the builder chain.
    #[must_use]
    pub fn with_modes(mut self, modes: Dom) -> Self {
        self.set_modes(modes);
        self
    }

    /// The content's accessible name.
    pub fn set_label(&mut self, label: AzString) {
        self.label = label;
    }

    /// [`Self::set_label`] for the builder chain.
    #[must_use]
    pub fn with_label(mut self, label: AzString) -> Self {
        self.set_label(label);
        self
    }

    /// The compact minimum size.
    pub const fn set_min_size(&mut self, width: f32, height: f32) {
        self.min_width = width;
        self.min_height = height;
    }

    /// [`Self::set_min_size`] for the builder chain.
    #[must_use]
    pub const fn with_min_size(mut self, width: f32, height: f32) -> Self {
        self.set_min_size(width, height);
        self
    }

    /// Pin the widget theme; unset, the shell follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty shell and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(Dom::create_div());
        core::mem::swap(&mut s, self);
        s
    }

    /// The shell's DOM: root [title row?, modes?, main content].
    #[must_use]
    pub fn dom(self) -> Dom {
        let look = look_for(self.theme);
        build(self, &look)
    }
}

impl From<UtilityShell> for Dom {
    fn from(s: UtilityShell) -> Self {
        s.dom()
    }
}

/// The shell's DOM in `look`.
pub(crate) fn build(shell: UtilityShell, look: &ShellLook) -> Dom {
    let UtilityShell {
        title_row,
        modes,
        content,
        label,
        min_width,
        min_height,
        theme: _,
    } = shell;
    let mut children: alloc::vec::Vec<Dom> = alloc::vec::Vec::with_capacity(3);
    if let Some(t) = title_row.into_option() {
        children.push(
            Dom::create_node(NodeType::Header)
                .with_ids_and_classes(id_and_class(
                    &AzString::from_const_str(TITLE_ID),
                    super::office_shell::TITLE_CLASS,
                ))
                .with_css_props(part(CHROME_ROW_BASE, &look.shell_title))
                .with_child(t),
        );
    }
    if let Some(m) = modes.into_option() {
        children.push(
            Dom::create_div()
                .with_ids_and_classes(id_and_class(&AzString::from_const_str(MODES_ID), MODES_CLASS))
                .with_css_props(part(CHROME_ROW_BASE, &look.toolbar_row))
                .with_child(m),
        );
    }
    children.push(
        Dom::create_node(NodeType::Main)
            .with_ids_and_classes(id_and_class(
                &AzString::from_const_str(CONTENT_ID),
                super::office_shell::PANE_CLASS,
            ))
            .with_css_props(part(PANE_BASE, &look.shell_body))
            .with_accessibility_info(AccessibilityInfo::named(
                label,
                azul_core::a11y::AccessibilityRole::Pane,
            ))
            .with_child(content),
    );
    // The root's structure, then the compact minimum size the app asked
    // for (the base declares no minimum of its own, so neither is written
    // twice), then the theme's skin.
    let mut base: alloc::vec::Vec<CssPropertyWithConditions> = UTILITY_BASE.to_vec();
    #[allow(clippy::cast_possible_truncation)]
    if min_width > 0.0 {
        base.push(CssPropertyWithConditions::simple(CssProperty::const_min_width(
            LayoutMinWidth::const_px(min_width as isize),
        )));
    }
    #[allow(clippy::cast_possible_truncation)]
    if min_height > 0.0 {
        base.push(CssPropertyWithConditions::simple(CssProperty::const_min_height(
            LayoutMinHeight::const_px(min_height as isize),
        )));
    }
    Dom::create_div()
        .with_ids_and_classes(root_classes(UTILITY_CLASS, look))
        .with_css_props(part(&base, &look.shell_root))
        .with_children(DomVec::from_vec(children))
}

/// The utility root: a column that fills its window and clips, with no
/// minimum of its own (the shell adds the app's).
static UTILITY_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::Px(
        PixelValue::const_percent(100),
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::Px(
        PixelValue::const_percent(100),
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
];

#[cfg(test)]
mod utility_shell_tests {
    use azul_core::dom::IdOrClass;

    use super::*;
    use crate::widgets::{
        shells::fixtures::slot,
        themes::{theme_blocks::checks, theme_checks as tc, UiTheme},
    };

    fn full() -> UtilityShell {
        UtilityShell::create(slot())
            .with_title_row(slot())
            .with_modes(slot())
            .with_min_size(320.0, 480.0)
    }

    #[test]
    fn s9_is_a_title_row_a_mode_row_and_the_content_with_a_compact_minimum() {
        let dom = full().with_theme(UiTheme::Flat).dom();
        let ids: Vec<String> = tc::nodes(&dom)
            .into_iter()
            .filter_map(|(_, n)| {
                n.root.get_ids_and_classes().as_ref().iter().find_map(|c| match c {
                    IdOrClass::Id(s) => Some(s.as_str().to_string()),
                    IdOrClass::Class(_) => None,
                })
            })
            .collect();
        assert_eq!(ids, vec![TITLE_ID, MODES_ID, CONTENT_ID]);
        assert!(matches!(dom.children.as_ref()[2].root.get_node_type(), NodeType::Main));
        let min_w = tc::resolve(&dom, azul_css::props::property::CssPropertyType::MinWidth, false, None);
        assert!(matches!(min_w, Some(CssProperty::MinWidth(_))), "{min_w:?}");
        assert!(tc::find(&dom, "__azul-native-split-pane").is_none(), "one region, no splits");
    }

    #[test]
    fn s9_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "utility_shell",
            || full().dom(),
            |t: UiTheme| full().with_theme(t).dom(),
        );
    }
}
