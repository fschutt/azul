//! S10 - Call / capture: Meet, the camera, the screen recorder's capture
//! (04-app-shells.md, S10).
//!
//! ```text
//! ┌ header (call clock, REC, signal) ───────────────────┬────────────────┐
//! │ ┌────────────┐ ┌────────────┐ ┌────────────┐        │ PARTICIPANTS   │
//! │ │ You        │ │ Anna       │ │ Bob        │  tiles │ 👤 Anna   🎤   │
//! │ └────────────┘ └────────────┘ └────────────┘        │ 👤 Bob    🔇   │
//! │ ┌───────────────────────────────────────────┐       ├────────────────┤
//! │ │ shared screen                             │       │ devices        │
//! │ └───────────────────────────────────────────┘       │ 🎤 ▾  📷 ▾     │
//! ├─────────────────────────────────────────────────────┴────────────────┤
//! │   [🎤 Mute] [📷 Stop video] [🖥 Share] [✋] [💬] [👥]      [ Leave ] │
//! └──────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! An [`OfficeShell`] whose main pane (`shell-tiles`) lays the tiles out
//! as a wrapping grid - one column for one tile, two for up to four, three
//! for up to nine, four beyond (VideoTileGrid's gallery layout; the tile's
//! width is what the app sizes its stream by) - beside an `<aside>` column
//! of the side panel (participants or chat, `shell-side-panel`) over the
//! devices panel (`shell-devices`), with the CONTROLS bar as the footer
//! (`shell-controls`, a `Toolbar`).
//!
//! Key types: [`CallShell`].

use alloc::vec::Vec;

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    dom::{Dom, DomVec, OptionDom},
    refany::RefAny,
};
use azul_css::{
    dynamic_selector::CssPropertyWithConditions,
    props::{
        basic::pixel::PixelValue,
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutFlexShrink, LayoutMinWidth, LayoutOverflow, LayoutWidth},
        property::CssProperty,
        basic::length::FloatValue,
    },
    AzString,
};

use super::{
    id_and_class, look_for,
    office_shell::{
        self, OfficeShell, OptionShellOnPaneFocus, OptionShellOnPaneResize, ShellOnPaneFocus,
        ShellOnPaneFocusCallback, ShellOnPaneResize, ShellOnPaneResizeCallback, ShellPane,
        ShellPaneKind,
    },
    part, ShellLook, CHROME_ROW_BASE, GROW_COLUMN_BASE, WRAP_ROW_BASE,
};
use crate::widgets::themes::{OptionUiTheme, UiTheme};

/// The tile grid pane's DOM id.
pub const TILES_ID: &str = "shell-tiles";
/// The side column's DOM id (the side panel over the devices).
pub const SIDE_PANEL_ID: &str = "shell-side-panel";
/// The devices panel's DOM id.
pub const DEVICES_ID: &str = "shell-devices";
/// The controls bar's DOM id.
pub const CONTROLS_ID: &str = "shell-controls";
/// The tile grid's class.
pub const GRID_CLASS: &str = "__azul-native-call-shell-grid";
/// One tile cell's class.
pub const TILE_CLASS: &str = "__azul-native-call-shell-tile";
/// The side column's class.
pub const SIDE_CLASS: &str = "__azul-native-call-shell-side";
/// The side panel host's class.
pub const PANEL_CLASS: &str = "__azul-native-call-shell-panel";
/// The devices panel host's class.
pub const DEVICES_CLASS: &str = "__azul-native-call-shell-devices";
/// The controls bar's class.
pub const CONTROLS_CLASS: &str = "__azul-native-call-shell-controls";

/// The number of columns the gallery lays `count` tiles out in.
#[must_use]
pub const fn gallery_columns(count: usize) -> usize {
    match count {
        0 | 1 => 1,
        2..=4 => 2,
        5..=9 => 3,
        _ => 4,
    }
}

/// S10: a tile grid beside the side panel and the devices, the controls
/// bar as the footer.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct CallShell {
    /// The header (the call clock, REC, the signal), or nothing.
    pub header: OptionDom,
    /// The tiles (the camera, the remote videos, a shared screen).
    pub tiles: DomVec,
    /// The side panel (participants, chat).
    pub side_panel: OptionDom,
    /// The devices panel under the side panel.
    pub devices: OptionDom,
    /// The controls bar.
    pub controls: Dom,
    /// F6 moved the focus to a pane.
    pub on_pane_focus: OptionShellOnPaneFocus,
    /// A splitter moved.
    pub on_pane_resize: OptionShellOnPaneResize,
    /// The tiles' share of the width beside the side column (default 0.75).
    pub tiles_ratio: f32,
    /// The widget theme this shell is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
}

impl CallShell {
    /// A shell of the tiles and the controls bar.
    #[must_use]
    pub fn create(tiles: DomVec, controls: Dom) -> Self {
        Self {
            header: OptionDom::None,
            tiles,
            side_panel: OptionDom::None,
            devices: OptionDom::None,
            controls,
            on_pane_focus: None.into(),
            on_pane_resize: None.into(),
            tiles_ratio: 0.75,
            theme: OptionUiTheme::None,
        }
    }

    /// The header.
    pub fn set_header(&mut self, header: Dom) {
        self.header = OptionDom::Some(header);
    }

    /// [`Self::set_header`] for the builder chain.
    #[must_use]
    pub fn with_header(mut self, header: Dom) -> Self {
        self.set_header(header);
        self
    }

    /// Appends a tile.
    pub fn add_tile(&mut self, tile: Dom) {
        let mut v = self.tiles.clone().into_library_owned_vec();
        v.push(tile);
        self.tiles = DomVec::from_vec(v);
    }

    /// [`Self::add_tile`] for the builder chain.
    #[must_use]
    pub fn with_tile(mut self, tile: Dom) -> Self {
        self.add_tile(tile);
        self
    }

    /// The side panel.
    pub fn set_side_panel(&mut self, side_panel: Dom) {
        self.side_panel = OptionDom::Some(side_panel);
    }

    /// [`Self::set_side_panel`] for the builder chain.
    #[must_use]
    pub fn with_side_panel(mut self, side_panel: Dom) -> Self {
        self.set_side_panel(side_panel);
        self
    }

    /// The devices panel.
    pub fn set_devices(&mut self, devices: Dom) {
        self.devices = OptionDom::Some(devices);
    }

    /// [`Self::set_devices`] for the builder chain.
    #[must_use]
    pub fn with_devices(mut self, devices: Dom) -> Self {
        self.set_devices(devices);
        self
    }

    /// The tiles' share of the width beside the side column.
    pub const fn set_tiles_ratio(&mut self, ratio: f32) {
        self.tiles_ratio = ratio;
    }

    /// [`Self::set_tiles_ratio`] for the builder chain.
    #[must_use]
    pub const fn with_tiles_ratio(mut self, ratio: f32) -> Self {
        self.set_tiles_ratio(ratio);
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
        let mut s = Self::create(DomVec::from_const_slice(&[]), Dom::create_div());
        core::mem::swap(&mut s, self);
        s
    }

    /// The shell's DOM.
    #[must_use]
    pub fn dom(self) -> Dom {
        let look = look_for(self.theme);
        build(self, &look)
    }
}

impl From<CallShell> for Dom {
    fn from(s: CallShell) -> Self {
        s.dom()
    }
}

/// A tile cell: a column that keeps its share of the row's width and clips
/// its stream.
static CELL_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
];

/// The [`OfficeShell`] this shell is, in `look`, built once.
pub(crate) fn build(shell: CallShell, look: &ShellLook) -> Dom {
    let CallShell {
        header,
        tiles,
        side_panel,
        devices,
        controls,
        on_pane_focus,
        on_pane_resize,
        tiles_ratio,
        theme,
    } = shell;
    let tiles = tiles.into_library_owned_vec();
    let columns = gallery_columns(tiles.len());
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    let share = (100.0 / columns as f32).floor() as isize;
    let cells: Vec<Dom> = tiles
        .into_iter()
        .map(|t| {
            let mut base: Vec<CssPropertyWithConditions> = CELL_BASE.to_vec();
            base.push(CssPropertyWithConditions::simple(CssProperty::const_width(
                LayoutWidth::Px(PixelValue::const_percent(share)),
            )));
            Dom::create_div()
                .with_class(AzString::from_const_str(TILE_CLASS))
                .with_css_props(part(&base, &look.tile_cell))
                .with_child(t)
        })
        .collect();
    let grid = Dom::create_div()
        .with_class(AzString::from_const_str(GRID_CLASS))
        .with_css_props(part(WRAP_ROW_BASE, &look.tiles_grid))
        .with_children(DomVec::from_vec(cells));

    let mut office = OfficeShell {
        title_row: header,
        status_bar: OptionDom::Some(
            Dom::create_div()
                .with_ids_and_classes(id_and_class(&AzString::from_const_str(CONTROLS_ID), CONTROLS_CLASS))
                .with_css_props(part(CHROME_ROW_BASE, &look.toolbar_row))
                .with_accessibility_info(AccessibilityInfo::named("Call controls", AccessibilityRole::Toolbar))
                .with_child(controls),
        ),
        on_pane_focus,
        on_pane_resize,
        theme,
        ..OfficeShell::create()
    }
    .with_pane(
        ShellPane::create(AzString::from_const_str(TILES_ID), grid)
            .with_kind(ShellPaneKind::Main)
            .with_label(AzString::from_const_str("Participants"))
            .with_ratio(tiles_ratio),
    );

    let mut side: Vec<Dom> = Vec::with_capacity(2);
    if let Some(p) = side_panel.into_option() {
        side.push(
            Dom::create_div()
                .with_class(AzString::from_const_str(PANEL_CLASS))
                .with_css_props(part(GROW_COLUMN_BASE, &[]))
                .with_child(p),
        );
    }
    if let Some(d) = devices.into_option() {
        side.push(
            Dom::create_div()
                .with_ids_and_classes(id_and_class(&AzString::from_const_str(DEVICES_ID), DEVICES_CLASS))
                .with_css_props(part(CHROME_ROW_BASE, &look.drawer))
                .with_child(d),
        );
    }
    if !side.is_empty() {
        office.add_pane(
            ShellPane::create(
                AzString::from_const_str(SIDE_PANEL_ID),
                Dom::create_div()
                    .with_class(AzString::from_const_str(SIDE_CLASS))
                    .with_css_props(part(GROW_COLUMN_BASE, &[]))
                    .with_children(DomVec::from_vec(side)),
            )
            .with_kind(ShellPaneKind::Side)
            .with_label(AzString::from_const_str("Panel")),
        );
    }
    office_shell::build(office, look)
}

#[cfg(test)]
mod call_shell_tests {
    use azul_core::dom::IdOrClass;

    use super::*;
    use crate::widgets::{
        shells::fixtures::slot,
        themes::{theme_blocks::checks, theme_checks as tc, UiTheme},
    };

    fn full(tiles: usize) -> CallShell {
        let mut s = CallShell::create(DomVec::from_const_slice(&[]), slot())
            .with_header(slot())
            .with_side_panel(slot())
            .with_devices(slot());
        for _ in 0..tiles {
            s.add_tile(slot());
        }
        s
    }

    #[test]
    fn the_gallery_grows_its_columns_with_the_tiles() {
        assert_eq!(gallery_columns(0), 1);
        assert_eq!(gallery_columns(1), 1);
        assert_eq!(gallery_columns(2), 2);
        assert_eq!(gallery_columns(4), 2);
        assert_eq!(gallery_columns(5), 3);
        assert_eq!(gallery_columns(9), 3);
        assert_eq!(gallery_columns(10), 4);
        assert_eq!(gallery_columns(30), 4);
    }

    #[test]
    fn s10_is_a_tile_grid_beside_the_side_column_with_the_controls_as_the_footer() {
        let dom = full(3).with_theme(UiTheme::Flat).dom();
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
            vec!["shell-title", TILES_ID, SIDE_PANEL_ID, DEVICES_ID, "shell-status", CONTROLS_ID]
        );
        assert_eq!(tc::find_all(&dom, TILE_CLASS).len(), 3);
        let controls = tc::find(&dom, CONTROLS_CLASS).expect("controls");
        assert_eq!(
            controls.root.get_accessibility_info().map(|i| i.role),
            Some(AccessibilityRole::Toolbar)
        );
        // Three tiles: two columns, so every cell takes half the row.
        let cell = tc::find(&dom, TILE_CLASS).expect("a cell");
        let width = tc::resolve(cell, azul_css::props::property::CssPropertyType::Width, false, None);
        match width {
            Some(CssProperty::Width(w)) => {
                let pv = match w.get_property() {
                    Some(LayoutWidth::Px(pv)) => *pv,
                    other => panic!("{other:?}"),
                };
                assert!((pv.number.get() - 50.0).abs() < 0.01, "{pv:?}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn s10_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "call_shell",
            || full(2).dom(),
            |t: UiTheme| full(2).with_theme(t).dom(),
        );
    }
}
