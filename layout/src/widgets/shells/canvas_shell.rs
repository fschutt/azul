//! S2 - Canvas editor: Photoshop, Illustrator, the infinite canvas, the
//! blackboard, font editors (04-app-shells.md, S2).
//!
//! ```text
//! ┌ File Edit Image Layer Select Filter View Window Help ── menu bar ──┐
//! │ [tool options: size ●── hardness ●── opacity ●── mode ▾]   toolbar │
//! ├──┬─ photo × ─ poster × ──────────────────────┬─────────────────────┤
//! │▣ │ document tabs                             │ ▾ LAYERS     panels │
//! │✎ │ ┌───────────────────────────────────────┐ │ ▾ PROPERTIES        │
//! │⬚ │ │            canvas                     │ │ ▾ COLOR             │
//! │T │ └───────────────────────────────────────┘ │                     │
//! │  ├───────────────────────────────────────────┤                     │
//! │  │ node graph drawer                         │                     │
//! ├──┴───────────────────────────────────────────┴─────────────────────┤
//! │ 100% │ 1920x1080 px │ sRGB                                status bar│
//! └────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! An [`OfficeShell`] whose title row is the menu bar, whose ribbon row is
//! the TOOL OPTIONS toolbar (`shell-toolbar`, a `Toolbar` for assistive
//! technology), with the tool palette as a rail (`shell-tools`), the
//! document tabs over the canvas as the main pane (`shell-canvas`), the
//! panels as the side pane (`shell-panels`) and the drawer under the row
//! (`shell-drawer`).
//!
//! Key types: [`CanvasShell`].

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    dom::{Dom, DomVec, OptionDom},
};
use azul_css::AzString;

use super::{
    id_and_class, look_for,
    office_shell::{self, OfficeShell, ShellPane, ShellPaneKind},
    part, ShellLook, CHROME_ROW_BASE, GROW_COLUMN_BASE,
};
use crate::widgets::themes::{OptionUiTheme, UiTheme};

/// The tool palette rail's DOM id.
pub const TOOLS_ID: &str = "shell-tools";
/// The canvas pane's DOM id.
pub const CANVAS_ID: &str = "shell-canvas";
/// The panels pane's DOM id.
pub const PANELS_ID: &str = "shell-panels";
/// The drawer's DOM id.
pub const DRAWER_ID: &str = "shell-drawer";
/// The tool options toolbar's DOM id.
pub const TOOLBAR_ID: &str = "shell-toolbar";
/// The document tabs host's DOM id.
pub const TABS_ID: &str = "shell-tabs";
/// The tool options toolbar's class.
pub const TOOLBAR_CLASS: &str = "__azul-native-canvas-shell-toolbar";
/// The document tabs host's class.
pub const TABS_CLASS: &str = "__azul-native-canvas-shell-tabs";
/// The class of the column that stacks the tabs over the canvas.
pub const MAIN_CLASS: &str = "__azul-native-canvas-shell-main";
/// The tool palette's width in px.
pub const TOOLS_WIDTH: f32 = 44.0;

/// S2: menu bar, tool options, tool palette | tabs over the canvas |
/// panels, the drawer under the row, a status bar.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct CanvasShell {
    /// The in-window menu bar (or nothing under a native one).
    pub menu_bar: OptionDom,
    /// The tool options bar for the active tool.
    pub tool_options: OptionDom,
    /// The tool palette (the icon column).
    pub tool_palette: OptionDom,
    /// The document tabs over the canvas.
    pub document_tabs: OptionDom,
    /// The canvas viewport.
    pub canvas: Dom,
    /// The panels at the right (layers, properties, colour).
    pub panels: OptionDom,
    /// The drawer under the row (the node graph).
    pub drawer: OptionDom,
    /// The canvas's share beside the panels (default 0.75).
    pub canvas_ratio: f32,
    /// The drawer's share of the height (default 0.3).
    pub drawer_ratio: f32,
    /// The widget theme this shell is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
}

impl CanvasShell {
    /// A shell around `canvas`, nothing else.
    #[must_use]
    pub const fn create(canvas: Dom) -> Self {
        Self {
            menu_bar: OptionDom::None,
            tool_options: OptionDom::None,
            tool_palette: OptionDom::None,
            document_tabs: OptionDom::None,
            canvas,
            panels: OptionDom::None,
            drawer: OptionDom::None,
            canvas_ratio: 0.75,
            drawer_ratio: 0.3,
            theme: OptionUiTheme::None,
        }
    }

    /// The in-window menu bar.
    pub fn set_menu_bar(&mut self, menu_bar: Dom) {
        self.menu_bar = OptionDom::Some(menu_bar);
    }

    /// [`Self::set_menu_bar`] for the builder chain.
    #[must_use]
    pub fn with_menu_bar(mut self, menu_bar: Dom) -> Self {
        self.set_menu_bar(menu_bar);
        self
    }

    /// The tool options bar.
    pub fn set_tool_options(&mut self, tool_options: Dom) {
        self.tool_options = OptionDom::Some(tool_options);
    }

    /// [`Self::set_tool_options`] for the builder chain.
    #[must_use]
    pub fn with_tool_options(mut self, tool_options: Dom) -> Self {
        self.set_tool_options(tool_options);
        self
    }

    /// The tool palette.
    pub fn set_tool_palette(&mut self, tool_palette: Dom) {
        self.tool_palette = OptionDom::Some(tool_palette);
    }

    /// [`Self::set_tool_palette`] for the builder chain.
    #[must_use]
    pub fn with_tool_palette(mut self, tool_palette: Dom) -> Self {
        self.set_tool_palette(tool_palette);
        self
    }

    /// The document tabs.
    pub fn set_document_tabs(&mut self, document_tabs: Dom) {
        self.document_tabs = OptionDom::Some(document_tabs);
    }

    /// [`Self::set_document_tabs`] for the builder chain.
    #[must_use]
    pub fn with_document_tabs(mut self, document_tabs: Dom) -> Self {
        self.set_document_tabs(document_tabs);
        self
    }

    /// The panels.
    pub fn set_panels(&mut self, panels: Dom) {
        self.panels = OptionDom::Some(panels);
    }

    /// [`Self::set_panels`] for the builder chain.
    #[must_use]
    pub fn with_panels(mut self, panels: Dom) -> Self {
        self.set_panels(panels);
        self
    }

    /// The drawer.
    pub fn set_drawer(&mut self, drawer: Dom) {
        self.drawer = OptionDom::Some(drawer);
    }

    /// [`Self::set_drawer`] for the builder chain.
    #[must_use]
    pub fn with_drawer(mut self, drawer: Dom) -> Self {
        self.set_drawer(drawer);
        self
    }

    /// The canvas's share beside the panels.
    pub const fn set_canvas_ratio(&mut self, ratio: f32) {
        self.canvas_ratio = ratio;
    }

    /// [`Self::set_canvas_ratio`] for the builder chain.
    #[must_use]
    pub const fn with_canvas_ratio(mut self, ratio: f32) -> Self {
        self.set_canvas_ratio(ratio);
        self
    }

    /// The drawer's share of the height.
    pub const fn set_drawer_ratio(&mut self, ratio: f32) {
        self.drawer_ratio = ratio;
    }

    /// [`Self::set_drawer_ratio`] for the builder chain.
    #[must_use]
    pub const fn with_drawer_ratio(mut self, ratio: f32) -> Self {
        self.set_drawer_ratio(ratio);
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

    /// The [`OfficeShell`] this shell is, its own rows (the tool options)
    /// built in the shell's theme. The chrome the presets share (the status
    /// bar, the F6 / splitter hooks) is set on it.
    #[must_use]
    pub fn office_shell(self) -> OfficeShell {
        let look = look_for(self.theme);
        office_shell_in(self, &look)
    }

    /// The shell's DOM.
    #[must_use]
    pub fn dom(self) -> Dom {
        let look = look_for(self.theme);
        office_shell::build(office_shell_in(self, &look), &look)
    }
}

impl From<CanvasShell> for Dom {
    fn from(s: CanvasShell) -> Self {
        s.dom()
    }
}

/// The [`OfficeShell`] this shell is, in `look`.
fn office_shell_in(shell: CanvasShell, look: &ShellLook) -> OfficeShell {
    let CanvasShell {
        menu_bar,
        tool_options,
        tool_palette,
        document_tabs,
        canvas,
        panels,
        drawer,
        canvas_ratio,
        drawer_ratio,
        theme,
    } = shell;
    // The tool options bar is the ribbon row.
    let ribbon = tool_options.into_option().map(|t| {
        Dom::create_div()
            .with_ids_and_classes(id_and_class(&AzString::from_const_str(TOOLBAR_ID), TOOLBAR_CLASS))
            .with_css_props(part(CHROME_ROW_BASE, &look.toolbar_row))
            .with_accessibility_info(AccessibilityInfo::named("Tool options", AccessibilityRole::Toolbar))
            .with_child(t)
    });
    // The main pane: the document tabs over the canvas.
    let mut column: Vec<Dom> = Vec::with_capacity(2);
    if let Some(tabs) = document_tabs.into_option() {
        column.push(
            Dom::create_div()
                .with_ids_and_classes(id_and_class(&AzString::from_const_str(TABS_ID), TABS_CLASS))
                .with_css_props(part(CHROME_ROW_BASE, &[]))
                .with_child(tabs),
        );
    }
    column.push(canvas);
    let main = Dom::create_div()
        .with_class(AzString::from_const_str(MAIN_CLASS))
        .with_css_props(part(GROW_COLUMN_BASE, &[]))
        .with_children(DomVec::from_vec(column));

    let mut office = OfficeShell {
        title_row: menu_bar,
        ribbon: ribbon.into(),
        theme,
        ..OfficeShell::create()
    };
    if let Some(tools) = tool_palette.into_option() {
        office.add_pane(
            ShellPane::create(AzString::from_const_str(TOOLS_ID), tools)
                .with_label(AzString::from_const_str("Tools"))
                .with_width(TOOLS_WIDTH),
        );
    }
    office.add_pane(
        ShellPane::create(AzString::from_const_str(CANVAS_ID), main)
            .with_kind(ShellPaneKind::Main)
            .with_label(AzString::from_const_str("Canvas"))
            .with_ratio(canvas_ratio),
    );
    if let Some(p) = panels.into_option() {
        office.add_pane(
            ShellPane::create(AzString::from_const_str(PANELS_ID), p)
                .with_kind(ShellPaneKind::Side)
                .with_label(AzString::from_const_str("Panels")),
        );
    }
    if let Some(d) = drawer.into_option() {
        office.set_bottom(
            ShellPane::create(AzString::from_const_str(DRAWER_ID), d)
                .with_label(AzString::from_const_str("Drawer"))
                .with_ratio(drawer_ratio),
        );
    }
    office
}

#[cfg(test)]
mod canvas_shell_tests {
    use azul_core::dom::IdOrClass;

    use super::*;
    use crate::widgets::{
        shells::fixtures::slot,
        themes::{theme_blocks::checks, theme_checks as tc, UiTheme},
    };

    fn full() -> CanvasShell {
        CanvasShell::create(slot())
            .with_menu_bar(slot())
            .with_tool_options(slot())
            .with_tool_palette(slot())
            .with_document_tabs(slot())
            .with_panels(slot())
            .with_drawer(slot())
    }

    /// `shell` converted, with the status bar the OfficeShell holds.
    fn chrome(shell: CanvasShell) -> OfficeShell {
        shell.office_shell().with_status_bar(slot())
    }

    #[test]
    fn s2_is_tool_options_a_tool_rail_tabs_over_the_canvas_panels_and_a_drawer() {
        let dom = chrome(full().with_theme(UiTheme::Flat)).dom();
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
                TOOLBAR_ID,
                TOOLS_ID,
                CANVAS_ID,
                TABS_ID,
                PANELS_ID,
                DRAWER_ID,
                "shell-status"
            ]
        );
        let toolbar = tc::find(&dom, TOOLBAR_CLASS).expect("toolbar");
        assert_eq!(
            toolbar.root.get_accessibility_info().map(|i| i.role),
            Some(AccessibilityRole::Toolbar)
        );
        assert!(tc::find(&dom, "__azul-native-office-shell-rail").is_some());
    }

    #[test]
    fn s2_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "canvas_shell",
            || chrome(full()).dom(),
            |t: UiTheme| chrome(full().with_theme(t)).dom(),
        );
    }
}
