//! S8 - Developer surface: the code editor, the terminal, the hex editor,
//! man pages (04-app-shells.md, S8).
//!
//! ```text
//! ┌──┬──────────────────┬─ main.rs × ─ lib.rs ● ───────────────────────┐
//! │📄│ ▾ AZUL-APPS      │  1  use azul::prelude::*;              editor │
//! │🔍│   ▾ apps         │  2                                            │
//! │⎇ │     ▸ writer     ├───────────────────────────────────────────────┤
//! │▶ │   ▸ planning     │ TERMINAL  PROBLEMS  OUTPUT               panel │
//! │  │       side bar   │ $ cargo run -p AzWriter                       │
//! ├──┴──────────────────┴───────────────────────────────────────────────┤
//! │ ⎇ main  ⚠ 1  ✕ 0                        Ln 4, Col 25   status bar  │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! An [`OfficeShell`] with an ACTIVITY BAR rail (`shell-activity-bar`, a
//! fixed-width `<nav>`), the side bar (`shell-side-bar`, a `<nav>`), the
//! editor (`shell-editor`, the `<main>`, its document tabs inside) and the
//! bottom panel (`shell-panel`: terminal, problems, output) under the row.
//!
//! Key types: [`DeveloperShell`].

use azul_core::dom::{Dom, OptionDom};
use azul_css::AzString;

use super::office_shell::{OfficeShell, ShellPane, ShellPaneKind};

/// The activity bar's DOM id.
pub const ACTIVITY_BAR_ID: &str = "shell-activity-bar";
/// The side bar's DOM id.
pub const SIDE_BAR_ID: &str = "shell-side-bar";
/// The editor pane's DOM id.
pub const EDITOR_ID: &str = "shell-editor";
/// The bottom panel's DOM id.
pub const PANEL_ID: &str = "shell-panel";
/// The activity bar's width in px.
pub const ACTIVITY_BAR_WIDTH: f32 = 48.0;

/// S8: activity bar | side bar | editor, the panel under them, a status
/// bar.
///
/// The preset holds its panes and slots; the chrome (title row, status bar, the F6 / splitter hooks, the
/// theme) is the [`OfficeShell`]'s:
/// `DeveloperShell::create(activity, side, editor).office_shell().with_status_bar(s)`.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct DeveloperShell {
    /// The activity bar (the icon column).
    pub activity_bar: Dom,
    /// The side bar (the explorer tree, search, source control).
    pub side_bar: Dom,
    /// The editor (document tabs over the code view).
    pub editor: Dom,
    /// The bottom panel (terminal, problems, output).
    pub panel: OptionDom,
    /// The side bar's share of the width beside the editor (default 0.25).
    pub side_bar_ratio: f32,
    /// The panel's share of the height (default 0.3).
    pub panel_ratio: f32,
}

impl DeveloperShell {
    /// A shell of the activity bar, the side bar and the editor.
    #[must_use]
    pub fn create(activity_bar: Dom, side_bar: Dom, editor: Dom) -> Self {
        Self {
            activity_bar,
            side_bar,
            editor,
            panel: OptionDom::None,
            side_bar_ratio: 0.25,
            panel_ratio: 0.3,
        }
    }

    /// The bottom panel.
    pub fn set_panel(&mut self, panel: Dom) {
        self.panel = OptionDom::Some(panel);
    }

    /// [`Self::set_panel`] for the builder chain.
    #[must_use]
    pub fn with_panel(mut self, panel: Dom) -> Self {
        self.set_panel(panel);
        self
    }

    /// The side bar's share of the width beside the editor.
    pub const fn set_side_bar_ratio(&mut self, ratio: f32) {
        self.side_bar_ratio = ratio;
    }

    /// [`Self::set_side_bar_ratio`] for the builder chain.
    #[must_use]
    pub const fn with_side_bar_ratio(mut self, ratio: f32) -> Self {
        self.set_side_bar_ratio(ratio);
        self
    }

    /// The panel's share of the height.
    pub const fn set_panel_ratio(&mut self, ratio: f32) {
        self.panel_ratio = ratio;
    }

    /// [`Self::set_panel_ratio`] for the builder chain.
    #[must_use]
    pub const fn with_panel_ratio(mut self, ratio: f32) -> Self {
        self.set_panel_ratio(ratio);
        self
    }

    /// Replaces `self` with an empty shell and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(Dom::create_div(), Dom::create_div(), Dom::create_div());
        core::mem::swap(&mut s, self);
        s
    }

    /// The [`OfficeShell`] this shell is: the activity bar rail, side bar |
    /// editor, the panel under the row.
    #[must_use]
    pub fn office_shell(self) -> OfficeShell {
        let Self {
            activity_bar,
            side_bar,
            editor,
            panel,
            side_bar_ratio,
            panel_ratio,
        } = self;
        let mut shell = OfficeShell::create().with_pane(
            ShellPane::create(AzString::from_const_str(ACTIVITY_BAR_ID), activity_bar)
                .with_kind(ShellPaneKind::Navigation)
                .with_label(AzString::from_const_str("Activity bar"))
                .with_width(ACTIVITY_BAR_WIDTH),
        )
        .with_pane(
            ShellPane::create(AzString::from_const_str(SIDE_BAR_ID), side_bar)
                .with_kind(ShellPaneKind::Navigation)
                .with_label(AzString::from_const_str("Side bar"))
                .with_ratio(side_bar_ratio),
        )
        .with_pane(
            ShellPane::create(AzString::from_const_str(EDITOR_ID), editor)
                .with_kind(ShellPaneKind::Main)
                .with_label(AzString::from_const_str("Editor")),
        );
        if let Some(p) = panel.into_option() {
            shell.set_bottom(
                ShellPane::create(AzString::from_const_str(PANEL_ID), p)
                    .with_label(AzString::from_const_str("Panel"))
                    .with_ratio(panel_ratio),
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

impl From<DeveloperShell> for Dom {
    fn from(s: DeveloperShell) -> Self {
        s.dom()
    }
}

#[cfg(test)]
mod developer_shell_tests {
    use azul_core::dom::IdOrClass;

    use super::*;
    use crate::widgets::{
        shells::fixtures::slot,
        themes::{theme_blocks::checks, theme_checks as tc, UiTheme},
    };

    /// S8 with every slot filled, the chrome set on the converted shell.
    fn full() -> OfficeShell {
        DeveloperShell::create(slot(), slot(), slot())
            .with_panel(slot())
            .office_shell()
            .with_status_bar(slot())
    }

    #[test]
    fn s8_is_a_rail_a_side_bar_the_editor_and_the_panel_under_them() {
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
        assert_eq!(ids, vec![ACTIVITY_BAR_ID, SIDE_BAR_ID, EDITOR_ID, PANEL_ID, "shell-status"]);
        let rail = tc::find(&dom, "__azul-native-office-shell-rail").expect("the activity bar rail");
        assert!(rail
            .root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, IdOrClass::Id(s) if s.as_str() == ACTIVITY_BAR_ID)));
        // One horizontal split (side bar | editor) and one vertical (row / panel).
        assert_eq!(tc::find_all(&dom, "__azul-native-split-pane").len(), 2);
        assert_eq!(
            full().cycle_ids().iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            vec![ACTIVITY_BAR_ID, SIDE_BAR_ID, EDITOR_ID, PANEL_ID]
        );
    }

    #[test]
    fn s8_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "developer_shell",
            || full().dom(),
            |t: UiTheme| full().with_theme(t).dom(),
        );
    }
}
