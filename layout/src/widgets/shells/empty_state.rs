//! `ShellEmptyState` - an icon, one line, one action: the "nothing here yet",
//! "nothing selected" and error block every app shows in a pane that has
//! no content (05-widget-backlog.md, `ShellEmptyState`).
//!
//! ```text
//!            ✉
//!    No message selected
//!  Pick a message to read it here.
//!        [ New message ]
//! ```
//!
//! The block centres itself in whatever pane holds it (it grows), and is a
//! group named by its line for assistive technology. The action is a
//! [`Button`] the app's click callback answers.
//!
//! Key types: [`ShellEmptyState`].

use alloc::vec::Vec;

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    dom::{Dom, DomVec},
    refany::RefAny,
};
use azul_css::AzString;

use super::{look_for, part, root_classes, text, ShellLook, CENTRED_COLUMN_BASE, LABEL_BASE};
use crate::widgets::{
    button::{Button, ButtonOnClick, ButtonOnClickCallback, OptionButtonOnClick},
    themes::{OptionUiTheme, UiTheme},
};

/// The block's class.
pub const EMPTY_STATE_CLASS: &str = "__azul-native-empty-state";
/// The icon's class.
pub const ICON_CLASS: &str = "__azul-native-empty-state-icon";
/// The line's class.
pub const TITLE_CLASS: &str = "__azul-native-empty-state-title";
/// The detail line's class.
pub const DETAIL_CLASS: &str = "__azul-native-empty-state-detail";
/// The class of the box around the action.
pub const ACTION_CLASS: &str = "__azul-native-empty-state-action";

/// An icon, one line, an optional detail line and an optional action.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ShellEmptyState {
    /// The icon over the line (a `Dom::create_icon` name), or empty.
    pub icon: AzString,
    /// The one line ("No message selected").
    pub title: AzString,
    /// The line under it, or empty.
    pub detail: AzString,
    /// The action button's label, or empty for no button.
    pub action_label: AzString,
    /// The action button's click.
    pub on_action: OptionButtonOnClick,
    /// The widget theme this block is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
}

impl ShellEmptyState {
    /// A block with the one line `title`, no icon, no detail, no action.
    #[must_use]
    pub fn create(title: AzString) -> Self {
        Self {
            icon: AzString::from_const_str(""),
            title,
            detail: AzString::from_const_str(""),
            action_label: AzString::from_const_str(""),
            on_action: None.into(),
            theme: OptionUiTheme::None,
        }
    }

    /// The icon over the line.
    pub fn set_icon(&mut self, icon: AzString) {
        self.icon = icon;
    }

    /// [`Self::set_icon`] for the builder chain.
    #[must_use]
    pub fn with_icon(mut self, icon: AzString) -> Self {
        self.set_icon(icon);
        self
    }

    /// The line under the title.
    pub fn set_detail(&mut self, detail: AzString) {
        self.detail = detail;
    }

    /// [`Self::set_detail`] for the builder chain.
    #[must_use]
    pub fn with_detail(mut self, detail: AzString) -> Self {
        self.set_detail(detail);
        self
    }

    /// The action button's label (empty: no button).
    pub fn set_action_label(&mut self, label: AzString) {
        self.action_label = label;
    }

    /// [`Self::set_action_label`] for the builder chain.
    #[must_use]
    pub fn with_action_label(mut self, label: AzString) -> Self {
        self.set_action_label(label);
        self
    }

    /// The action button's click.
    pub fn set_on_action<C: Into<ButtonOnClickCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_action = Some(ButtonOnClick {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_action`] for the builder chain.
    #[must_use]
    pub fn with_on_action<C: Into<ButtonOnClickCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_action(data, callback);
        self
    }

    /// Pin the widget theme; unset, the block follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty block and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }

    /// The block's DOM: root [icon?, title, detail?, action?].
    #[must_use]
    pub fn dom(self) -> Dom {
        let look = look_for(self.theme);
        build(self, &look)
    }
}

impl Default for ShellEmptyState {
    fn default() -> Self {
        Self::create(AzString::from_const_str(""))
    }
}

impl From<ShellEmptyState> for Dom {
    fn from(e: ShellEmptyState) -> Self {
        e.dom()
    }
}

/// The block's DOM in `look`.
pub(crate) fn build(state: ShellEmptyState, look: &ShellLook) -> Dom {
    let ShellEmptyState {
        icon,
        title,
        detail,
        action_label,
        on_action,
        theme,
    } = state;
    let mut children: Vec<Dom> = Vec::with_capacity(4);
    if !icon.as_str().is_empty() {
        children.push(
            Dom::create_icon(icon)
                .with_class(AzString::from_const_str(ICON_CLASS))
                .with_css_props(part(LABEL_BASE, &look.empty_icon)),
        );
    }
    children.push(
        text(title.clone())
            .with_class(AzString::from_const_str(TITLE_CLASS))
            .with_css_props(part(LABEL_BASE, &look.empty_title)),
    );
    if !detail.as_str().is_empty() {
        children.push(
            text(detail)
                .with_class(AzString::from_const_str(DETAIL_CLASS))
                .with_css_props(part(LABEL_BASE, &look.empty_detail)),
        );
    }
    if !action_label.as_str().is_empty() {
        let mut button = Button::create(action_label);
        if let Some(ButtonOnClick { refany, callback }) = on_action.into_option() {
            button = button.with_on_click(refany, callback);
        }
        if let Some(t) = super::inner_theme(theme) {
            button = button.with_theme(t);
        }
        children.push(
            Dom::create_div()
                .with_class(AzString::from_const_str(ACTION_CLASS))
                .with_css_props(part(LABEL_BASE, &look.empty_action))
                .with_child(button.dom()),
        );
    }
    Dom::create_div()
        .with_ids_and_classes(root_classes(EMPTY_STATE_CLASS, look))
        .with_css_props(part(CENTRED_COLUMN_BASE, &look.empty_root))
        .with_accessibility_info(AccessibilityInfo::named(title, AccessibilityRole::Grouping))
        .with_children(DomVec::from_vec(children))
}

#[cfg(test)]
mod empty_state_tests {
    use azul_core::{
        a11y::AccessibilityRole,
        callbacks::Update,
        dom::{EventFilter, HoverEventFilter, NodeType},
    };

    use super::*;
    use crate::{
        callbacks::CallbackInfo,
        widgets::{
            button::ButtonOnClickCallbackType,
            shells::fixtures::empty_state,
            themes::{theme_blocks::checks, theme_checks as tc, UiTheme},
        },
    };

    fn text_of(node: &Dom) -> Option<&str> {
        match node.children.as_ref() {
            [only] => match only.root.get_node_type() {
                NodeType::Text(s) => Some(s.as_ref().as_str()),
                _ => None,
            },
            _ => None,
        }
    }

    #[test]
    fn the_flat_icon_reads_at_3_to_1_on_the_surface_it_sits_on() {
        // WCAG 1.4.11 (graphics: 3:1). AzCalendar's empty agenda: the flat
        // icon was #adb5bd on the #f8f9fa surface, 1.97:1 - the 48 px glyph
        // barely showed. Light ink on both light grounds, dark ink on both
        // dark ones.
        use azul_css::props::property::CssProperty;

        use crate::widgets::themes::flat;
        let look = crate::widgets::themes::flat::shell_look();
        let inks: Vec<(bool, azul_css::props::basic::color::ColorU)> = look
            .empty_icon
            .iter()
            .filter_map(|p| match &p.property {
                CssProperty::TextColor(v) => {
                    v.get_property().map(|c| (p.apply_if.as_ref().is_empty(), c.inner))
                }
                _ => None,
            })
            .collect();
        assert_eq!(inks.len(), 2, "a light and a dark ink: {inks:?}");
        for (light, ink) in inks {
            let grounds = if light {
                [flat::LIGHT_PG, flat::LIGHT_SUR]
            } else {
                [flat::DARK_PG, flat::DARK_SUR]
            };
            for ground in grounds {
                let ratio = ink.contrast_ratio(&ground);
                assert!(ratio >= 3.0, "{ink:?} on {ground:?} reads {ratio:.2}:1");
            }
        }
    }

    #[test]
    fn an_empty_state_is_an_icon_over_a_line_a_detail_and_an_action() {
        let dom = empty_state().with_theme(UiTheme::Flat).dom();
        let kids = dom.children.as_ref();
        assert_eq!(kids.len(), 4);
        assert!(matches!(kids[0].root.get_node_type(), NodeType::Icon(_)));
        assert!(tc::has_class(&kids[0], ICON_CLASS));
        assert_eq!(text_of(&kids[1]), Some("No message selected"));
        assert_eq!(text_of(&kids[2]), Some("Pick a message to read it here."));
        assert!(tc::has_class(&kids[3], ACTION_CLASS));
        assert!(matches!(
            kids[3].children.as_ref()[0].root.get_node_type(),
            NodeType::Button
        ));
    }

    #[test]
    fn a_bare_line_is_just_the_line_named_for_assistive_technology() {
        let dom = ShellEmptyState::create(AzString::from("Nothing here"))
            .with_theme(UiTheme::Flat)
            .dom();
        assert_eq!(dom.children.as_ref().len(), 1);
        let info = dom.root.get_accessibility_info().expect("a11y");
        assert_eq!(info.role, AccessibilityRole::Grouping);
        assert_eq!(
            info.accessibility_name.as_ref().map(|s| s.as_str()),
            Some("Nothing here")
        );
    }

    extern "C" fn noop(_: RefAny, _: CallbackInfo) -> Update {
        Update::DoNothing
    }

    #[test]
    fn the_action_click_lands_on_the_button() {
        let dom = empty_state()
            .with_on_action(RefAny::new(()), noop as ButtonOnClickCallbackType)
            .with_theme(UiTheme::Flat)
            .dom();
        let button = &dom.children.as_ref()[3].children.as_ref()[0];
        assert!(button
            .root
            .get_callbacks()
            .as_ref()
            .iter()
            .any(|cb| cb.event == EventFilter::Hover(HoverEventFilter::Click)));
    }

    #[test]
    fn an_empty_state_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "empty_state",
            || empty_state().dom(),
            |t: UiTheme| empty_state().with_theme(t).dom(),
        );
    }
}
