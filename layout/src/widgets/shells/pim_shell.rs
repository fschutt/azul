//! S4 - Three-pane PIM: Mail, Notes, News, `ToDo`, Chat, Passwords,
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
//! list and the reading pane are `AzMail`'s own widgets (MAILWIDGETS).
//!
//! Key types: [`PimShell`].

use azul_core::dom::{Dom, OptionDom};
use azul_css::AzString;

use super::office_shell::{OfficeShell, ShellPane, ShellPaneKind};

/// The navigation pane's DOM id.
pub const NAVIGATION_ID: &str = "shell-navigation";
/// The list pane's DOM id.
pub const LIST_ID: &str = "shell-list";
/// The reading pane's DOM id.
pub const READING_ID: &str = "shell-reading";

/// S4: navigation | list | reading, with an optional To-Do bar.
///
/// The preset holds the panes; the chrome (title row, ribbon, backstage,
/// status bar, the F6 / splitter hooks, the theme) is the [`OfficeShell`]'s:
/// `PimShell::create(nav, list, reading).office_shell().with_ribbon(ribbon)`.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct PimShell {
    /// The navigation pane.
    pub navigation: Dom,
    /// The list (messages, notes, contacts).
    pub list: Dom,
    /// The reading pane.
    pub reading: Dom,
    /// The To-Do bar.
    pub todo_bar: OptionDom,
    /// The list's accessible name ("Message list").
    pub list_label: AzString,
    /// The navigation pane's share of the width (default 0.2).
    pub navigation_ratio: f32,
    /// The list's share of what is left beside the reading pane (default
    /// 0.4).
    pub list_ratio: f32,
}

impl PimShell {
    /// A shell of the three panes, nothing else.
    #[must_use]
    pub fn create(navigation: Dom, list: Dom, reading: Dom) -> Self {
        Self {
            navigation,
            list,
            reading,
            todo_bar: OptionDom::None,
            list_label: AzString::from_const_str("List"),
            navigation_ratio: 0.2,
            list_ratio: 0.4,
        }
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
            navigation,
            list,
            reading,
            todo_bar,
            list_label,
            navigation_ratio,
            list_ratio,
        } = self;
        OfficeShell {
            right_bar: todo_bar,
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

    /// S4 with every slot filled, the chrome set on the converted shell.
    fn full() -> OfficeShell {
        PimShell::create(slot(), slot(), slot())
            .with_todo_bar(slot())
            .with_list_label(AzString::from("Message list"))
            .office_shell()
            .with_title_row(slot())
            .with_ribbon(slot())
            .with_status_bar(slot())
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
        let cycle = full().cycle_ids();
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
