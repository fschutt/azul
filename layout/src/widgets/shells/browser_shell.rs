//! S5 - Browser / manager: Files, the photo, font and archive managers,
//! the disk analyzer (04-app-shells.md, S5) - Windows Explorer.
//!
//! ```text
//! ┌ title row ─────────────────────────────────────────────────────────┐
//! │ ribbon                                                             │
//! │ [<][>][^]  [ This PC > Home > Documents ]           [🔍 ] address bar│
//! ├──────────────┬─────────────────────────────────────┬───────────────┤
//! │ tree         │ content (tiles | details list)      │ preview       │
//! │ (places,     │                                     │ (optional)    │
//! │  devices)    │                                     │               │
//! ├──────────────┴─────────────────────────────────────┴───────────────┤
//! │ details pane (optional, Explorer's bottom pane)                    │
//! ├────────────────────────────────────────────────────────────────────┤
//! │ status bar                                                         │
//! └────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! An [`OfficeShell`] whose ribbon row holds the ribbon AND the address
//! bar (`shell-address-bar`), with the tree (`shell-tree`, a `<nav>`), the
//! content (`shell-content`, the `<main>`), an optional preview pane
//! (`shell-preview`, an `<aside>`) and Explorer's details pane under the
//! row (`shell-details`).
//!
//! Key types: [`BrowserShell`].

use azul_core::dom::{Dom, DomVec, OptionDom};
use azul_css::AzString;

use super::{
    id_and_class,
    office_shell::{OfficeShell, ShellPane, ShellPaneKind},
    part, CHROME_ROW_BASE, COLUMN_BASE,
};

/// The tree pane's DOM id.
pub const TREE_ID: &str = "shell-tree";
/// The content pane's DOM id.
pub const CONTENT_ID: &str = "shell-content";
/// The preview pane's DOM id.
pub const PREVIEW_ID: &str = "shell-preview";
/// The details pane's DOM id.
pub const DETAILS_ID: &str = "shell-details";
/// The address bar host's DOM id.
pub const ADDRESS_BAR_ID: &str = "shell-address-bar";
/// The address bar host's class.
pub const ADDRESS_BAR_CLASS: &str = "__azul-native-browser-shell-address-bar";
/// The class of the column that stacks the ribbon over the address bar.
pub const RIBBON_ROW_CLASS: &str = "__azul-native-browser-shell-ribbon-row";

/// S5: address bar + tree + content, an optional preview pane and details
/// pane.
///
/// The preset holds the panes and the ribbon (which it stacks over the
/// address bar); the rest of the chrome (title row, backstage, status bar,
/// the F6 / splitter hooks, the theme) is the [`OfficeShell`]'s:
/// `BrowserShell::create(bar, tree, content).office_shell().with_status_bar(s)`.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct BrowserShell {
    /// The ribbon (over the address bar).
    pub ribbon: OptionDom,
    /// The address bar (an `AddressBar`).
    pub address_bar: Dom,
    /// The navigation tree.
    pub tree: Dom,
    /// The content view.
    pub content: Dom,
    /// The preview pane at the right.
    pub preview: OptionDom,
    /// The details pane under the row.
    pub details: OptionDom,
    /// The tree's share of the width (default 0.22).
    pub tree_ratio: f32,
    /// The content's share beside the preview pane (default 0.7).
    pub content_ratio: f32,
    /// The details pane's share of the height (default 0.22).
    pub details_ratio: f32,
    /// Whether the navigation tree is shown (default); hidden, the content
    /// takes its place and F6 skips it (Explorer's View > Navigation pane).
    pub tree_visible: bool,
}

impl BrowserShell {
    /// A shell of the address bar, the tree and the content, nothing else.
    #[must_use]
    pub const fn create(address_bar: Dom, tree: Dom, content: Dom) -> Self {
        Self {
            ribbon: OptionDom::None,
            address_bar,
            tree,
            content,
            preview: OptionDom::None,
            details: OptionDom::None,
            tree_ratio: 0.22,
            content_ratio: 0.7,
            details_ratio: 0.22,
            tree_visible: true,
        }
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

    /// The preview pane.
    pub fn set_preview(&mut self, preview: Dom) {
        self.preview = OptionDom::Some(preview);
    }

    /// [`Self::set_preview`] for the builder chain.
    #[must_use]
    pub fn with_preview(mut self, preview: Dom) -> Self {
        self.set_preview(preview);
        self
    }

    /// The details pane.
    pub fn set_details(&mut self, details: Dom) {
        self.details = OptionDom::Some(details);
    }

    /// [`Self::set_details`] for the builder chain.
    #[must_use]
    pub fn with_details(mut self, details: Dom) -> Self {
        self.set_details(details);
        self
    }

    /// Show or hide the navigation tree.
    pub const fn set_tree_visible(&mut self, visible: bool) {
        self.tree_visible = visible;
    }

    /// [`Self::set_tree_visible`] for the builder chain.
    #[must_use]
    pub const fn with_tree_visible(mut self, visible: bool) -> Self {
        self.set_tree_visible(visible);
        self
    }

    /// The tree's share of the width.
    pub const fn set_tree_ratio(&mut self, ratio: f32) {
        self.tree_ratio = ratio;
    }

    /// [`Self::set_tree_ratio`] for the builder chain.
    #[must_use]
    pub const fn with_tree_ratio(mut self, ratio: f32) -> Self {
        self.set_tree_ratio(ratio);
        self
    }

    /// The content's share beside the preview pane.
    pub const fn set_content_ratio(&mut self, ratio: f32) {
        self.content_ratio = ratio;
    }

    /// [`Self::set_content_ratio`] for the builder chain.
    #[must_use]
    pub const fn with_content_ratio(mut self, ratio: f32) -> Self {
        self.set_content_ratio(ratio);
        self
    }

    /// The details pane's share of the height.
    pub const fn set_details_ratio(&mut self, ratio: f32) {
        self.details_ratio = ratio;
    }

    /// [`Self::set_details_ratio`] for the builder chain.
    #[must_use]
    pub const fn with_details_ratio(mut self, ratio: f32) -> Self {
        self.set_details_ratio(ratio);
        self
    }

    /// Replaces `self` with an empty shell and returns the original.
    #[must_use]
    pub const fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(Dom::create_div(), Dom::create_div(), Dom::create_div());
        core::mem::swap(&mut s, self);
        s
    }

    /// The [`OfficeShell`] this shell is: the ribbon over the address bar,
    /// tree | content | preview, the details pane under the row.
    #[must_use]
    pub fn office_shell(self) -> OfficeShell {
        let Self {
            ribbon,
            address_bar,
            tree,
            content,
            preview,
            details,
            tree_ratio,
            content_ratio,
            details_ratio,
            tree_visible,
        } = self;
        // The ribbon row: the ribbon (if any) over the address bar.
        let address_row = Dom::create_div()
            .with_ids_and_classes(id_and_class(
                &AzString::from_const_str(ADDRESS_BAR_ID),
                ADDRESS_BAR_CLASS,
            ))
            .with_css_props(part(CHROME_ROW_BASE, &[]))
            .with_child(address_bar);
        let mut ribbon_row: Vec<Dom> = Vec::with_capacity(2);
        if let Some(r) = ribbon.into_option() {
            ribbon_row.push(r);
        }
        ribbon_row.push(address_row);
        let ribbon_slot = Dom::create_div()
            .with_class(AzString::from_const_str(RIBBON_ROW_CLASS))
            .with_css_props(part(COLUMN_BASE, &[]))
            .with_children(DomVec::from_vec(ribbon_row));

        let mut shell = OfficeShell {
            ribbon: OptionDom::Some(ribbon_slot),
            ..OfficeShell::create()
        }
        .with_pane(
            ShellPane::create(AzString::from_const_str(TREE_ID), tree)
                .with_kind(ShellPaneKind::Navigation)
                .with_label(AzString::from_const_str("Navigation"))
                .with_ratio(tree_ratio)
                .with_visible(tree_visible),
        )
        .with_pane(
            ShellPane::create(AzString::from_const_str(CONTENT_ID), content)
                .with_kind(ShellPaneKind::Main)
                .with_label(AzString::from_const_str("Content"))
                .with_ratio(content_ratio),
        );
        if let Some(p) = preview.into_option() {
            shell.add_pane(
                ShellPane::create(AzString::from_const_str(PREVIEW_ID), p)
                    .with_kind(ShellPaneKind::Side)
                    .with_label(AzString::from_const_str("Preview")),
            );
        }
        if let Some(d) = details.into_option() {
            shell.set_bottom(
                ShellPane::create(AzString::from_const_str(DETAILS_ID), d)
                    .with_label(AzString::from_const_str("Details"))
                    .with_ratio(details_ratio),
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

impl From<BrowserShell> for Dom {
    fn from(s: BrowserShell) -> Self {
        s.dom()
    }
}

#[cfg(test)]
mod browser_shell_tests {
    use azul_core::dom::IdOrClass;

    use super::*;
    use crate::widgets::{
        shells::fixtures::slot,
        themes::{theme_blocks::checks, theme_checks as tc, UiTheme},
    };

    fn full() -> BrowserShell {
        BrowserShell::create(slot(), slot(), slot())
            .with_ribbon(slot())
            .with_preview(slot())
            .with_details(slot())
    }

    /// `shell` converted, with the chrome the OfficeShell holds.
    fn chrome(shell: BrowserShell) -> OfficeShell {
        shell
            .office_shell()
            .with_title_row(slot())
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
    fn s5_puts_the_address_bar_under_the_ribbon_and_the_details_under_the_row() {
        let dom = chrome(full()).with_theme(UiTheme::Flat).dom();
        assert_eq!(
            ids(&dom),
            vec![
                "shell-title",
                "shell-ribbon",
                ADDRESS_BAR_ID,
                TREE_ID,
                CONTENT_ID,
                PREVIEW_ID,
                DETAILS_ID,
                "shell-status"
            ]
        );
        assert_eq!(
            full().office_shell().cycle_ids().iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            vec![TREE_ID, CONTENT_ID, PREVIEW_ID, DETAILS_ID]
        );
        // The address bar host sits inside the ribbon row, after the ribbon.
        let ribbon_row = tc::find(&dom, "__azul-native-office-shell-ribbon").expect("ribbon row");
        let column = &ribbon_row.children.as_ref()[0];
        assert_eq!(column.children.as_ref().len(), 2);
        assert!(tc::has_class(&column.children.as_ref()[1], ADDRESS_BAR_CLASS));
    }

    /// Explorer's View > Navigation pane: the tree can be hidden. The shell
    /// then lays out the content (and the preview) alone, and F6 skips the
    /// hidden pane; shown is the default.
    #[test]
    fn s5_hides_the_navigation_pane_when_asked_and_f6_skips_it() {
        assert!(BrowserShell::create(slot(), slot(), slot()).tree_visible);
        let hidden = full().with_tree_visible(false);
        assert!(!hidden.tree_visible);
        let dom = chrome(hidden.clone()).with_theme(UiTheme::Flat).dom();
        assert_eq!(
            ids(&dom),
            vec![
                "shell-title",
                "shell-ribbon",
                ADDRESS_BAR_ID,
                CONTENT_ID,
                PREVIEW_ID,
                DETAILS_ID,
                "shell-status"
            ]
        );
        assert_eq!(
            hidden
                .office_shell()
                .cycle_ids()
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
            vec![CONTENT_ID, PREVIEW_ID, DETAILS_ID]
        );
    }

    #[test]
    fn s5_without_a_ribbon_still_has_its_address_bar() {
        let dom = BrowserShell::create(slot(), slot(), slot())
            .office_shell()
            .with_theme(UiTheme::Flat)
            .dom();
        assert_eq!(ids(&dom), vec!["shell-ribbon", ADDRESS_BAR_ID, TREE_ID, CONTENT_ID]);
    }

    #[test]
    fn s5_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "browser_shell",
            || chrome(full()).dom(),
            |t: UiTheme| chrome(full()).with_theme(t).dom(),
        );
    }
}
