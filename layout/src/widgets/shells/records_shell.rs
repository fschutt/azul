//! S6 - Records & dashboards: the system monitor, ERP modules, the
//! network analyzer, the log viewer, personal finance (04-app-shells.md,
//! S6).
//!
//! ```text
//! ┌ title row ───────────────────────────────────────────────────────┐
//! │ [Processes] [Performance] [Services]           [🔍 filter ] tabs │
//! ├────────────────────────────────────────────────────┬─────────────┤
//! │ CPU 23% ▁▂▃ │ Memory 61% ▅▅ │ Disk │ Net     cards │ record form │
//! ├────────────────────────────────────────────────────┤ (optional)  │
//! │ Name ▲ │ PID │ CPU │ Memory │ ...          table   │             │
//! ├────────────────────────────────────────────────────┴─────────────┤
//! │ status bar                                                       │
//! └──────────────────────────────────────────────────────────────────┘
//! ```
//!
//! An [`OfficeShell`] whose ribbon row is the tab row (the `TabHeader` and
//! the filter), whose main pane (`shell-table`) is the cards strip over the
//! table, and whose optional side pane (`shell-form`) is the record form.
//!
//! Key types: [`RecordsShell`].

use azul_core::{
    dom::{Dom, DomVec, OptionDom},
    refany::RefAny,
};
use azul_css::AzString;

use super::{
    id_and_class,
    office_shell::{
        OfficeShell, OptionShellOnPaneFocus, OptionShellOnPaneResize, ShellOnPaneFocus,
        ShellOnPaneFocusCallback, ShellOnPaneResize, ShellOnPaneResizeCallback, ShellPane,
        ShellPaneKind,
    },
    part, CHROME_ROW_BASE, GROW_COLUMN_BASE,
};
use crate::widgets::themes::{OptionUiTheme, UiTheme};

/// The table pane's DOM id.
pub const TABLE_ID: &str = "shell-table";
/// The record form pane's DOM id.
pub const FORM_ID: &str = "shell-form";
/// The cards strip's DOM id.
pub const CARDS_ID: &str = "shell-cards";
/// The cards strip's class.
pub const CARDS_CLASS: &str = "__azul-native-records-shell-cards";
/// The class of the column that stacks the cards over the table.
pub const MAIN_CLASS: &str = "__azul-native-records-shell-main";

/// S6: tabs, a cards strip, the record table, an optional record form.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct RecordsShell {
    /// The app-drawn title row.
    pub title_row: OptionDom,
    /// The tab row (a `TabHeader` and a filter field).
    pub tabs: Dom,
    /// The cards strip over the table (charts, sparklines, gauges).
    pub cards: OptionDom,
    /// The record table.
    pub table: Dom,
    /// The record form beside the table.
    pub form: OptionDom,
    /// The status bar.
    pub status_bar: OptionDom,
    /// F6 moved the focus to a pane.
    pub on_pane_focus: OptionShellOnPaneFocus,
    /// A splitter moved.
    pub on_pane_resize: OptionShellOnPaneResize,
    /// The table's share beside the form (default 0.7).
    pub table_ratio: f32,
    /// The widget theme this shell is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
}

impl RecordsShell {
    /// A shell of the tab row and the table, nothing else.
    #[must_use]
    pub fn create(tabs: Dom, table: Dom) -> Self {
        Self {
            title_row: OptionDom::None,
            tabs,
            cards: OptionDom::None,
            table,
            form: OptionDom::None,
            status_bar: OptionDom::None,
            on_pane_focus: None.into(),
            on_pane_resize: None.into(),
            table_ratio: 0.7,
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

    /// The cards strip.
    pub fn set_cards(&mut self, cards: Dom) {
        self.cards = OptionDom::Some(cards);
    }

    /// [`Self::set_cards`] for the builder chain.
    #[must_use]
    pub fn with_cards(mut self, cards: Dom) -> Self {
        self.set_cards(cards);
        self
    }

    /// The record form.
    pub fn set_form(&mut self, form: Dom) {
        self.form = OptionDom::Some(form);
    }

    /// [`Self::set_form`] for the builder chain.
    #[must_use]
    pub fn with_form(mut self, form: Dom) -> Self {
        self.set_form(form);
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

    /// The table's share beside the form.
    pub const fn set_table_ratio(&mut self, ratio: f32) {
        self.table_ratio = ratio;
    }

    /// [`Self::set_table_ratio`] for the builder chain.
    #[must_use]
    pub const fn with_table_ratio(mut self, ratio: f32) -> Self {
        self.set_table_ratio(ratio);
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
        let mut s = Self::create(Dom::create_div(), Dom::create_div());
        core::mem::swap(&mut s, self);
        s
    }

    /// The [`OfficeShell`] this shell is: the tab row as the ribbon row,
    /// cards over the table as the main pane, the form beside it.
    #[must_use]
    pub fn office_shell(self) -> OfficeShell {
        let Self {
            title_row,
            tabs,
            cards,
            table,
            form,
            status_bar,
            on_pane_focus,
            on_pane_resize,
            table_ratio,
            theme,
        } = self;
        let mut column: alloc::vec::Vec<Dom> = alloc::vec::Vec::with_capacity(2);
        if let Some(c) = cards.into_option() {
            column.push(
                Dom::create_div()
                    .with_ids_and_classes(id_and_class(&AzString::from_const_str(CARDS_ID), CARDS_CLASS))
                    .with_css_props(part(CHROME_ROW_BASE, &[]))
                    .with_child(c),
            );
        }
        column.push(table);
        let main = Dom::create_div()
            .with_class(AzString::from_const_str(MAIN_CLASS))
            .with_css_props(part(GROW_COLUMN_BASE, &[]))
            .with_children(DomVec::from_vec(column));
        let mut shell = OfficeShell {
            title_row,
            ribbon: OptionDom::Some(tabs),
            status_bar,
            on_pane_focus,
            on_pane_resize,
            theme,
            ..OfficeShell::create()
        }
        .with_pane(
            ShellPane::create(AzString::from_const_str(TABLE_ID), main)
                .with_kind(ShellPaneKind::Main)
                .with_label(AzString::from_const_str("Records"))
                .with_ratio(table_ratio),
        );
        if let Some(f) = form.into_option() {
            shell.add_pane(
                ShellPane::create(AzString::from_const_str(FORM_ID), f)
                    .with_kind(ShellPaneKind::Side)
                    .with_label(AzString::from_const_str("Record")),
            );
        }
        shell
    }

    /// The shell's DOM.
    #[must_use]
    pub fn dom(self) -> Dom {
        self.office_shell().dom()
    }
}

impl From<RecordsShell> for Dom {
    fn from(s: RecordsShell) -> Self {
        s.dom()
    }
}

#[cfg(test)]
mod records_shell_tests {
    use azul_core::dom::IdOrClass;

    use super::*;
    use crate::widgets::{
        shells::fixtures::slot,
        themes::{theme_blocks::checks, theme_checks as tc, UiTheme},
    };

    fn full() -> RecordsShell {
        RecordsShell::create(slot(), slot())
            .with_title_row(slot())
            .with_cards(slot())
            .with_form(slot())
            .with_status_bar(slot())
    }

    fn ids(dom: &Dom) -> Vec<String> {
        tc::nodes(dom)
            .into_iter()
            .filter_map(|(_, n)| {
                n.root.get_ids_and_classes().as_ref().iter().find_map(|c| match c {
                    IdOrClass::Id(s) => Some(s.as_str().to_string()),
                    IdOrClass::Class(_) => None,
                })
            })
            .collect()
    }

    #[test]
    fn s6_is_the_tab_row_then_cards_over_the_table_beside_the_form() {
        let dom = full().with_theme(UiTheme::Flat).dom();
        assert_eq!(
            ids(&dom),
            vec!["shell-title", "shell-ribbon", TABLE_ID, CARDS_ID, FORM_ID, "shell-status"]
        );
        assert_eq!(full().office_shell().cycle_ids().len(), 2);
        let bare = RecordsShell::create(slot(), slot()).with_theme(UiTheme::Flat).dom();
        assert_eq!(ids(&bare), vec!["shell-ribbon", TABLE_ID]);
    }

    #[test]
    fn s6_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "records_shell",
            || full().dom(),
            |t: UiTheme| full().with_theme(t).dom(),
        );
    }
}
