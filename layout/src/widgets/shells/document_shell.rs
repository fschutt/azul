//! S1 - Document + ribbon: Word, Excel, PowerPoint, the PDF editor
//! (04-app-shells.md, S1).
//!
//! ```text
//! ┌ title row ───────────────────────────────────────────────────────┐
//! │ FILE │ HOME │ INSERT │ ...                                 ribbon │
//! ├────────────┬──────────────────────────────────────┬──────────────┤
//! │ navigation │ document surface                     │ side pane    │
//! │ (optional) │ (PageView | CellGrid | slide canvas) │ (optional)   │
//! ├────────────┴──────────────────────────────────────┴──────────────┤
//! │ status bar                                                       │
//! └──────────────────────────────────────────────────────────────────┘
//! FILE -> the full-window Backstage in place of the ribbon and the panes.
//! ```
//!
//! An [`OfficeShell`] with three panes: the navigation pane
//! (`shell-navigation`, a `<nav>`), the document (`shell-document`, the
//! `<main>`) and the side pane (`shell-side`, an `<aside>`). Rulers, the
//! find bar and the document surface are the app's, inside the document
//! slot.
//!
//! Key types: [`DocumentShell`].

use azul_core::dom::{Dom, OptionDom};
use azul_css::AzString;

use super::office_shell::{OfficeShell, ShellPane, ShellPaneKind};

/// The navigation pane's DOM id.
pub const NAVIGATION_ID: &str = "shell-navigation";
/// The document pane's DOM id.
pub const DOCUMENT_ID: &str = "shell-document";
/// The side pane's DOM id.
pub const SIDE_ID: &str = "shell-side";

/// S1: the document window - title row, ribbon (or backstage), an optional
/// navigation pane, the document, an optional side pane, a status bar.
///
/// The preset holds the panes; the chrome (title row, ribbon, backstage,
/// status bar, the F6 / splitter hooks, the theme) is the [`OfficeShell`]'s:
/// `DocumentShell::create(doc).office_shell().with_ribbon(ribbon)`.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct DocumentShell {
    /// The navigation pane (headings, pages, thumbnails).
    pub navigation: OptionDom,
    /// The document surface.
    pub document: Dom,
    /// The side pane (comments, styles, format).
    pub side_pane: OptionDom,
    /// The navigation pane's share of the width (default 0.2).
    pub navigation_ratio: f32,
    /// The document's share of what is left beside the side pane
    /// (default 0.75).
    pub document_ratio: f32,
}

impl DocumentShell {
    /// A shell around `document`, nothing else.
    #[must_use]
    pub fn create(document: Dom) -> Self {
        Self {
            navigation: OptionDom::None,
            document,
            side_pane: OptionDom::None,
            navigation_ratio: 0.2,
            document_ratio: 0.75,
        }
    }

    /// The navigation pane.
    pub fn set_navigation(&mut self, navigation: Dom) {
        self.navigation = OptionDom::Some(navigation);
    }

    /// [`Self::set_navigation`] for the builder chain.
    #[must_use]
    pub fn with_navigation(mut self, navigation: Dom) -> Self {
        self.set_navigation(navigation);
        self
    }

    /// The side pane.
    pub fn set_side_pane(&mut self, side_pane: Dom) {
        self.side_pane = OptionDom::Some(side_pane);
    }

    /// [`Self::set_side_pane`] for the builder chain.
    #[must_use]
    pub fn with_side_pane(mut self, side_pane: Dom) -> Self {
        self.set_side_pane(side_pane);
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

    /// The document's share beside the side pane.
    pub const fn set_document_ratio(&mut self, ratio: f32) {
        self.document_ratio = ratio;
    }

    /// [`Self::set_document_ratio`] for the builder chain.
    #[must_use]
    pub const fn with_document_ratio(mut self, ratio: f32) -> Self {
        self.set_document_ratio(ratio);
        self
    }

    /// Replaces `self` with an empty shell and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(Dom::create_div());
        core::mem::swap(&mut s, self);
        s
    }

    /// The [`OfficeShell`] this shell is: navigation | document | side pane.
    #[must_use]
    pub fn office_shell(self) -> OfficeShell {
        let Self {
            navigation,
            document,
            side_pane,
            navigation_ratio,
            document_ratio,
        } = self;
        let mut shell = OfficeShell::create();
        if let Some(nav) = navigation.into_option() {
            shell.add_pane(
                ShellPane::create(AzString::from_const_str(NAVIGATION_ID), nav)
                    .with_kind(ShellPaneKind::Navigation)
                    .with_label(AzString::from_const_str("Navigation"))
                    .with_ratio(navigation_ratio),
            );
        }
        shell.add_pane(
            ShellPane::create(AzString::from_const_str(DOCUMENT_ID), document)
                .with_kind(ShellPaneKind::Main)
                .with_label(AzString::from_const_str("Document"))
                .with_ratio(document_ratio),
        );
        if let Some(side) = side_pane.into_option() {
            shell.add_pane(
                ShellPane::create(AzString::from_const_str(SIDE_ID), side)
                    .with_kind(ShellPaneKind::Side)
                    .with_label(AzString::from_const_str("Side pane")),
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

impl From<DocumentShell> for Dom {
    fn from(s: DocumentShell) -> Self {
        s.dom()
    }
}

#[cfg(test)]
mod document_shell_tests {
    use azul_core::dom::{IdOrClass, NodeType};

    use super::*;
    use crate::widgets::{
        shells::fixtures::slot,
        themes::{theme_blocks::checks, theme_checks as tc, UiTheme},
    };

    fn ids(dom: &Dom) -> Vec<(String, NodeType)> {
        tc::nodes(dom)
            .into_iter()
            .filter_map(|(_, n)| {
                n.root.get_ids_and_classes().as_ref().iter().find_map(|c| match c {
                    IdOrClass::Id(s) => Some((s.as_str().to_string(), n.root.get_node_type().clone())),
                    IdOrClass::Class(_) => None,
                })
            })
            .collect()
    }

    /// S1 with every slot filled, the chrome set on the converted shell.
    fn full() -> OfficeShell {
        DocumentShell::create(slot())
            .with_navigation(slot())
            .with_side_pane(slot())
            .office_shell()
            .with_title_row(slot())
            .with_ribbon(slot())
            .with_status_bar(slot())
    }

    #[test]
    fn s1_is_navigation_document_side_between_the_ribbon_and_the_status_bar() {
        let dom = full().with_theme(UiTheme::Flat).dom();
        let found = ids(&dom);
        let order: Vec<&str> = found.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(
            order,
            vec!["shell-title", "shell-ribbon", NAVIGATION_ID, DOCUMENT_ID, SIDE_ID, "shell-status"]
        );
        let kind = |id: &str| found.iter().find(|(x, _)| x == id).map(|(_, t)| t.clone()).expect(id);
        assert!(matches!(kind(NAVIGATION_ID), NodeType::Nav));
        assert!(matches!(kind(DOCUMENT_ID), NodeType::Main));
        assert!(matches!(kind(SIDE_ID), NodeType::Aside));
        assert_eq!(full().cycle_ids().len(), 3);
    }

    #[test]
    fn s1_without_its_optional_panes_is_the_document_alone() {
        let dom = DocumentShell::create(slot())
            .office_shell()
            .with_theme(UiTheme::Flat)
            .dom();
        let order: Vec<String> = ids(&dom).into_iter().map(|(id, _)| id).collect();
        assert_eq!(order, vec![DOCUMENT_ID]);
        assert!(tc::find(&dom, "__azul-native-split-pane").is_none());
    }

    #[test]
    fn s1_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "document_shell",
            || full().dom(),
            |t: UiTheme| full().with_theme(t).dom(),
        );
    }
}
