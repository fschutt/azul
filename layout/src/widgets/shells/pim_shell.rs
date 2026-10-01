//! S4 - Three-pane PIM: Mail, Notes, News, ToDo, Chat, Passwords,
//! Contacts, the clipboard history (04-app-shells.md, S4) - Outlook 2010.
//!
//! ```text
//! ┌ title row ───────────────────────────────────────────────────────┐
//! │ ribbon                                                           │
//! ├────────────┬───────────────┬──────────────────────┬──────────────┤
//! │ navigation │ list          │ reading pane         │ To-Do bar    │
//! │ pane       │ (messages)    │                      │ (optional)   │
//! ├────────────┴───────────────┴──────────────────────┴──────────────┤
//! │ status bar                                                       │
//! └──────────────────────────────────────────────────────────────────┘
//! ```
//!
//! An [`OfficeShell`] with three panes - navigation (`shell-navigation`, a
//! `<nav>`), the list (`shell-list`, a `<section>`) and the reading pane
//! (`shell-reading`, the `<main>`) - and Outlook's To-Do bar as the right
//! bar. The navigation slot is where a [`super::ShellNavigationPane`] goes; the
//! list and the reading pane are AzMail's own widgets (MAILWIDGETS).
//!
//! Key types: [`PimShell`].

use azul_core::{dom::{Dom, OptionDom}, refany::RefAny};
use azul_css::AzString;

use super::office_shell::{
    OfficeShell, OptionShellOnPaneFocus, OptionShellOnPaneResize, ShellOnPaneFocus,
    ShellOnPaneFocusCallback, ShellOnPaneResize, ShellOnPaneResizeCallback, ShellPane,
    ShellPaneKind,
};
use crate::widgets::themes::{OptionUiTheme, UiTheme};

/// The navigation pane's DOM id.
pub const NAVIGATION_ID: &str = "shell-navigation";
/// The list pane's DOM id.
pub const LIST_ID: &str = "shell-list";
/// The reading pane's DOM id.
pub const READING_ID: &str = "shell-reading";

/// S4: navigation | list | reading, with an optional To-Do bar.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct PimShell {
    /// The app-drawn title row.
    pub title_row: OptionDom,
    /// The ribbon.
    pub ribbon: OptionDom,
    /// The backstage, shown in place of the ribbon and the panes.
    pub backstage: OptionDom,
    /// The navigation pane.
    pub navigation: Dom,
    /// The list (messages, notes, contacts).
    pub list: Dom,
    /// The reading pane.
    pub reading: Dom,
    /// The To-Do bar.
    pub todo_bar: OptionDom,
    /// The status bar.
    pub status_bar: OptionDom,
    /// F6 moved the focus to a pane.
    pub on_pane_focus: OptionShellOnPaneFocus,
    /// A splitter moved.
    pub on_pane_resize: OptionShellOnPaneResize,
    /// The list's accessible name ("Message list").
    pub list_label: AzString,
    /// The navigation pane's share of the width (default 0.2).
    pub navigation_ratio: f32,
    /// The list's share of what is left beside the reading pane (default
    /// 0.4).
    pub list_ratio: f32,
    /// The widget theme this shell is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
}

impl PimShell {
    /// A shell of the three panes, nothing else.
    #[must_use]
    pub fn create(navigation: Dom, list: Dom, reading: Dom) -> Self {
        Self {
            title_row: OptionDom::None,
            ribbon: OptionDom::None,
            backstage: OptionDom::None,
            navigation,
            list,
            reading,
            todo_bar: OptionDom::None,
            status_bar: OptionDom::None,
            on_pane_focus: None.into(),
            on_pane_resize: None.into(),
            list_label: AzString::from_const_str("List"),
            navigation_ratio: 0.2,
            list_ratio: 0.4,
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

    /// The ribbon.
    pub fn set_ribbon(&mut self, ribbon: Dom) {
        self.ribbon = OptionDom::Some(ribbon);
    }

    /// [`Self::set_ribbon`] for the builder chain.
    #[must_use]
    pub fn with_ribbon(mut self, ribbon: Dom) -> Self {
        self.set_ribbon(ribbon);
        self
    }

    /// The backstage.
    pub fn set_backstage(&mut self, backstage: Dom) {
        self.backstage = OptionDom::Some(backstage);
    }

    /// [`Self::set_backstage`] for the builder chain.
    #[must_use]
    pub fn with_backstage(mut self, backstage: Dom) -> Self {
        self.set_backstage(backstage);
        self
    }

    /// The To-Do bar.
    pub fn set_todo_bar(&mut self, todo_bar: Dom) {
        self.todo_bar = OptionDom::Some(todo_bar);
    }

    /// [`Self::set_todo_bar`] for the builder chain.
    #[must_use]
    pub fn with_todo_bar(mut self, todo_bar: Dom) -> Self {
        self.set_todo_bar(todo_bar);
        self
    }

    /// The status bar.
    pub fn set_status_bar(&mut self, status_bar: Dom) {
        self.status_bar = OptionDom::Some(status_bar);
    }

    /// [`Self::set_status_bar`] for the builder chain.
    #[must_use]
    pub fn with_status_bar(mut self, status_bar: Dom) -> Self {
        self.set_status_bar(status_bar);
        self
    }

    /// The list's accessible name.
    pub fn set_list_label(&mut self, label: AzString) {
        self.list_label = label;
    }

    /// [`Self::set_list_label`] for the builder chain.
    #[must_use]
    pub fn with_list_label(mut self, label: AzString) -> Self {
        self.set_list_label(label);
        self
    }

    /// The navigation pane's share of the width.
    pub const fn set_navigation_ratio(&mut self, ratio: f32) {
        self.navigation_ratio = ratio;
    }

    /// [`Self::set_navigation_ratio`] for the builder chain.
    #[must_use]
    pub const fn with_navigation_ratio(mut self, ratio: f32) -> Self {
        self.set_navigation_ratio(ratio);
        self
    }

    /// The list's share beside the reading pane.
    pub const fn set_list_ratio(&mut self, ratio: f32) {
        self.list_ratio = ratio;
    }

    /// [`Self::set_list_ratio`] for the builder chain.
    #[must_use]
    pub const fn with_list_ratio(mut self, ratio: f32) -> Self {
        self.set_list_ratio(ratio);
        self
    }

    /// F6 moved the focus to a pane.
    pub fn set_on_pane_focus<C: Into<ShellOnPaneFocusCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_pane_focus = Some(ShellOnPaneFocus {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_pane_focus`] for the builder chain.
    #[must_use]
    pub fn with_on_pane_focus<C: Into<ShellOnPaneFocusCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_on_pane_focus(data, callback);
        self
    }

    /// A splitter moved.
    pub fn set_on_pane_resize<C: Into<ShellOnPaneResizeCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_pane_resize = Some(ShellOnPaneResize {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_pane_resize`] for the builder chain.
    #[must_use]
    pub fn with_on_pane_resize<C: Into<ShellOnPaneResizeCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_on_pane_resize(data, callback);
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
        let mut s = Self::create(Dom::create_div(), Dom::create_div(), Dom::create_div());
        core::mem::swap(&mut s, self);
        s
    }

    /// The [`OfficeShell`] this shell is: navigation | list | reading, the
    /// To-Do bar at the right.
    #[must_use]
    pub fn office_shell(self) -> OfficeShell {
        let Self {
            title_row,
            ribbon,
            backstage,
            navigation,
            list,
            reading,
            todo_bar,
            status_bar,
            on_pane_focus,
            on_pane_resize,
            list_label,
            navigation_ratio,
            list_ratio,
            theme,
        } = self;
        OfficeShell {
            title_row,
            ribbon,
            backstage,
            right_bar: todo_bar,
            status_bar,
            on_pane_focus,
            on_pane_resize,
            theme,
            ..OfficeShell::create()
        }
        .with_pane(
            ShellPane::create(AzString::from_const_str(NAVIGATION_ID), navigation)
                .with_kind(ShellPaneKind::Navigation)
                .with_label(AzString::from_const_str("Navigation"))
                .with_ratio(navigation_ratio),
        )
        .with_pane(
            ShellPane::create(AzString::from_const_str(LIST_ID), list)
                .with_label(list_label)
                .with_ratio(list_ratio),
        )
        .with_pane(
            ShellPane::create(AzString::from_const_str(READING_ID), reading)
                .with_kind(ShellPaneKind::Main)
                .with_label(AzString::from_const_str("Reading pane")),
        )
    }

    /// The shell's DOM.
    #[must_use]
    pub fn dom(self) -> Dom {
        self.office_shell().dom()
    }
}

impl From<PimShell> for Dom {
    fn from(s: PimShell) -> Self {
        s.dom()
    }
}

#[cfg(test)]
mod pim_shell_tests {
    use azul_core::dom::{IdOrClass, NodeType};

    use super::*;
    use crate::widgets::{
        shells::fixtures::slot,
        themes::{theme_blocks::checks, theme_checks as tc, UiTheme},
    };

    fn full() -> PimShell {
        PimShell::create(slot(), slot(), slot())
            .with_title_row(slot())
            .with_ribbon(slot())
            .with_todo_bar(slot())
            .with_status_bar(slot())
            .with_list_label(AzString::from("Message list"))
    }

    #[test]
    fn s4_is_navigation_list_reading_and_the_todo_bar() {
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
        assert_eq!(
            ids,
            vec![
                "shell-title",
                "shell-ribbon",
                NAVIGATION_ID,
                LIST_ID,
                READING_ID,
                "shell-right-bar",
                "shell-status"
            ]
        );
        let list = tc::nodes(&dom)
            .into_iter()
            .map(|(_, n)| n)
            .find(|n| {
                n.root
                    .get_ids_and_classes()
                    .as_ref()
                    .iter()
                    .any(|c| matches!(c, IdOrClass::Id(s) if s.as_str() == LIST_ID))
            })
            .expect("the list pane");
        assert!(matches!(list.root.get_node_type(), NodeType::Section));
        assert_eq!(
            list.root
                .get_accessibility_info()
                .and_then(|i| i.accessibility_name.as_ref().map(|s| s.as_str().to_string())),
            Some("Message list".to_string())
        );
        let cycle = full().office_shell().cycle_ids();
        assert_eq!(cycle.len(), 4, "three panes and the To-Do bar");
    }

    #[test]
    fn s4_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "pim_shell",
            || full().dom(),
            |t: UiTheme| full().with_theme(t).dom(),
        );
    }
}
