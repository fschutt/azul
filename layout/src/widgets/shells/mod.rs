//! App shells: the window layouts every Azlin app shares
//! (`azul-apps/planning/foundation/04-app-shells.md`).
//!
//! About 85 apps reduce to ELEVEN window layouts. Each is built once here, as
//! a composition of the widgets that already exist (the ribbon, the backstage,
//! the split pane, the status bar, the tree view, the accordion groups), in
//! ONE Office-like design - Outlook 2010 and Windows Explorer: a title row,
//! a ribbon with a Backstage, a navigation pane, panes between splitters, a
//! status bar - drawn by both widget themes (flat, flora) in both modes
//! (light, dark).
//!
//! Every slot is a `Dom` the app hands in; a shell owns the LAYOUT, the
//! splitters, the keyboard (F6 cycles the panes, Shift+F6 backwards), the
//! accessibility landmarks and the theming, never the content.
//!
//! The shared pieces, each a widget of its own:
//!
//! - [`OfficeShell`]: the frame every desktop shell is - title row, ribbon or
//!   backstage, N resizable panes, an optional right bar, a bottom pane, a
//!   status bar.
//! - [`ShellNavigationPane`]: Outlook's navigation pane - collapsible groups of
//!   trees, a module switcher of big buttons, a "collapse to strip" state.
//! - [`ShellCommandPalette`]: Ctrl/Cmd+K over the app's command table.
//! - [`ShellSettingsLayout`]: categories on the left, form sections on the right,
//!   search on top.
//! - [`ShellEmptyState`]: an icon, one line, one action.
//! - [`ShellThemeScope`]: the app's root - the widget theme's ground and an accent
//!   family, once.
//!
//! The eleven shells, S1..S11, are compositions of those: [`DocumentShell`],
//! [`CanvasShell`], [`TimelineShell`], [`PimShell`], [`BrowserShell`],
//! [`RecordsShell`], [`MediaShell`], [`DeveloperShell`], [`UtilityShell`],
//! [`CallShell`], [`MobileShell`].
//!
//! # One look
//!
//! The shells are ONE design, so a theme decides about them once:
//! [`ShellLook`] is the skin of every shell part, built by
//! `themes::flat::shell_look` and `themes::flora::shell_look`. The STRUCTURE
//! of every part (what makes a pane a column that grows, a rail a fixed
//! strip) is the shells' own, declared once here in the `*_BASE` statics and
//! the same under every theme (R5). A shell that follows the app theme merges
//! the two skins part by part ([`follow_look`]) and builds ONCE, so the app's
//! slot content is never cloned for a second build.

pub mod browser_shell;
pub mod call_shell;
pub mod canvas_shell;
pub mod command_palette;
pub mod developer_shell;
pub mod document_shell;
pub mod empty_state;
pub mod media_shell;
pub mod mobile_shell;
pub mod navigation_pane;
pub mod office_shell;
pub mod pim_shell;
pub mod records_shell;
pub mod settings_layout;
pub mod settings_dialog;
pub mod theme_scope;
pub mod timeline_shell;
pub mod utility_shell;

pub use browser_shell::BrowserShell;
pub use call_shell::CallShell;
pub use canvas_shell::CanvasShell;
pub use command_palette::{ShellCommandPalette, ShellPaletteCommand};
pub use developer_shell::DeveloperShell;
pub use document_shell::DocumentShell;
pub use empty_state::ShellEmptyState;
pub use media_shell::MediaShell;
pub use mobile_shell::{ShellBottomTab, MobileShell};
pub use navigation_pane::{ShellNavigationGroup, ShellNavigationModule, ShellNavigationPane, ShellNavigationPaneEvent};
pub use office_shell::{OfficeShell, ShellPane, ShellPaneKind};
pub use pim_shell::PimShell;
pub use records_shell::RecordsShell;
pub use settings_layout::{ShellSettingsLayout, ShellSettingsSection};
pub use settings_dialog::{
    ShellSetting, ShellSettingChoice, ShellSettingNumber, ShellSettingShortcut, ShellSettingValue,
    ShellSettingsApplyMode, ShellSettingsDialog, ShellSettingsEvent, ShellSettingsEventKind,
};
pub use theme_scope::{ShellThemeAccent, ShellThemeScope};
pub use timeline_shell::TimelineShell;
pub use utility_shell::UtilityShell;

use alloc::vec::Vec;

use azul_core::dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::{length::FloatValue, pixel::PixelValue},
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutFlexShrink, LayoutFlexWrap, LayoutHeight, LayoutJustifyContent,
            LayoutMinHeight, LayoutMinWidth, LayoutOverflow, LayoutPosition, LayoutWidth,
        },
        property::CssProperty,
        style::{StyleCursor, StyleUserSelect},
    },
    AzString,
};

use crate::widgets::themes::{OptionUiTheme, UiTheme};

/// What a theme decides about the shells: the SKIN of every part, laid over
/// the part's base (its structure, the shells' own) by each shell's `build`;
/// built by `themes::flat::shell_look` and `themes::flora::shell_look`.
///
/// Every field is one part's paint and metrics (colours, borders, padding,
/// widths, fonts, the hover / focus / active states), never its structure.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ShellLook {
    // ---- OfficeShell ----
    /// The shell's root: the window's ground, the chrome font and ink.
    pub shell_root: Vec<CssPropertyWithConditions>,
    /// The row that holds the app-drawn title row.
    pub shell_title: Vec<CssPropertyWithConditions>,
    /// The row that holds the ribbon (or a toolbar).
    pub shell_ribbon: Vec<CssPropertyWithConditions>,
    /// The body between the ribbon and the status bar.
    pub shell_body: Vec<CssPropertyWithConditions>,
    /// A pane: a surface, its hairline, the focus ring F6 lands on.
    pub shell_pane: Vec<CssPropertyWithConditions>,
    /// A fixed-width pane (a rail): the strip's surface and its hairline.
    pub shell_rail: Vec<CssPropertyWithConditions>,
    /// The optional right bar (Outlook's To-Do bar).
    pub shell_right_bar: Vec<CssPropertyWithConditions>,
    /// The row that holds the status bar.
    pub shell_status: Vec<CssPropertyWithConditions>,
    /// The full-window backstage host.
    pub shell_backstage: Vec<CssPropertyWithConditions>,

    // ---- ShellNavigationPane ----
    /// The pane's box.
    pub nav_root: Vec<CssPropertyWithConditions>,
    /// The header slot ("+ New").
    pub nav_header: Vec<CssPropertyWithConditions>,
    /// The scrolling column of groups.
    pub nav_groups: Vec<CssPropertyWithConditions>,
    /// The module switcher's box.
    pub nav_modules: Vec<CssPropertyWithConditions>,
    /// One module button at rest, with its hover and focus states.
    pub nav_module: Vec<CssPropertyWithConditions>,
    /// Added to the active module button.
    pub nav_module_active: Vec<CssPropertyWithConditions>,
    /// The module button's icon.
    pub nav_module_icon: Vec<CssPropertyWithConditions>,
    /// The module button's label.
    pub nav_module_label: Vec<CssPropertyWithConditions>,
    /// The footer row that holds the collapse chevron.
    pub nav_footer: Vec<CssPropertyWithConditions>,
    /// The collapsed pane: a narrow strip.
    pub nav_strip: Vec<CssPropertyWithConditions>,
    /// One icon-only module item of the strip.
    pub nav_strip_item: Vec<CssPropertyWithConditions>,

    // ---- ShellCommandPalette ----
    /// The backdrop over the window.
    pub palette_backdrop: Vec<CssPropertyWithConditions>,
    /// The panel.
    pub palette_panel: Vec<CssPropertyWithConditions>,
    /// The row that holds the search field.
    pub palette_input: Vec<CssPropertyWithConditions>,
    /// The result list.
    pub palette_list: Vec<CssPropertyWithConditions>,
    /// One result row at rest, with its hover and focus states.
    pub palette_row: Vec<CssPropertyWithConditions>,
    /// Added to the selected row.
    pub palette_row_selected: Vec<CssPropertyWithConditions>,
    /// The row's icon.
    pub palette_row_icon: Vec<CssPropertyWithConditions>,
    /// The row's label.
    pub palette_row_label: Vec<CssPropertyWithConditions>,
    /// The row's shortcut, set right.
    pub palette_row_shortcut: Vec<CssPropertyWithConditions>,
    /// The "no matching commands" line.
    pub palette_empty: Vec<CssPropertyWithConditions>,

    // ---- ShellSettingsLayout ----
    /// The layout's box.
    pub settings_root: Vec<CssPropertyWithConditions>,
    /// The search row on top.
    pub settings_search: Vec<CssPropertyWithConditions>,
    /// The category list on the left.
    pub settings_categories: Vec<CssPropertyWithConditions>,
    /// One category at rest, with its hover and focus states.
    pub settings_category: Vec<CssPropertyWithConditions>,
    /// Added to the active category.
    pub settings_category_active: Vec<CssPropertyWithConditions>,
    /// The scrolling column of sections.
    pub settings_sections: Vec<CssPropertyWithConditions>,
    /// One section.
    pub settings_section: Vec<CssPropertyWithConditions>,
    /// A section's title.
    pub settings_section_title: Vec<CssPropertyWithConditions>,

    // ---- ShellEmptyState ----
    /// The block.
    pub empty_root: Vec<CssPropertyWithConditions>,
    /// The icon.
    pub empty_icon: Vec<CssPropertyWithConditions>,
    /// The one line.
    pub empty_title: Vec<CssPropertyWithConditions>,
    /// The detail line under it.
    pub empty_detail: Vec<CssPropertyWithConditions>,
    /// The box around the action button.
    pub empty_action: Vec<CssPropertyWithConditions>,

    // ---- ShellThemeScope ----
    /// The app's root: the theme's ground, ink and font.
    pub scope_root: Vec<CssPropertyWithConditions>,

    // ---- the bars the S-shells add (S2 tool options, S8 rail, S10 controls, S11) ----
    /// A horizontal command bar (S2's tool options, S10's controls).
    pub toolbar_row: Vec<CssPropertyWithConditions>,
    /// A bottom drawer / panel host (S2's node graph, S8's terminal).
    pub drawer: Vec<CssPropertyWithConditions>,
    /// S10's tile grid.
    pub tiles_grid: Vec<CssPropertyWithConditions>,
    /// One cell of S10's tile grid.
    pub tile_cell: Vec<CssPropertyWithConditions>,
    /// S11's app bar.
    pub app_bar: Vec<CssPropertyWithConditions>,
    /// S11's app bar title.
    pub app_bar_title: Vec<CssPropertyWithConditions>,
    /// S11's page host.
    pub page: Vec<CssPropertyWithConditions>,
    /// S11's floating action button host.
    pub fab: Vec<CssPropertyWithConditions>,
    /// S11's bottom tab bar.
    pub bottom_tabs: Vec<CssPropertyWithConditions>,
    /// One bottom tab at rest, with its hover and focus states.
    pub bottom_tab: Vec<CssPropertyWithConditions>,
    /// Added to the active bottom tab.
    pub bottom_tab_active: Vec<CssPropertyWithConditions>,
    /// A bottom tab's icon.
    pub bottom_tab_icon: Vec<CssPropertyWithConditions>,
    /// A bottom tab's label.
    pub bottom_tab_label: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on every shell root, if it has one.
    pub marker: Option<&'static str>,
}

/// Both themes' shell looks in one, part by part
/// (`themes::theme_blocks::follow_props`), with the `structure` theme's
/// marker: the look an unpinned shell is built with. Built once per `dom()`.
#[must_use]
pub(crate) fn follow_look(structure: UiTheme) -> ShellLook {
    use crate::widgets::themes::{flat, flora, theme_blocks::follow_props};
    let (a, b) = (flat::shell_look(), flora::shell_look());
    let both = |x: &[CssPropertyWithConditions], y: &[CssPropertyWithConditions]| {
        follow_props(x, y).into_library_owned_vec()
    };
    macro_rules! merged {
        ($($field:ident),* $(,)?) => {
            ShellLook {
                $($field: both(a.$field.as_slice(), b.$field.as_slice()),)*
                marker: match structure {
                    UiTheme::Flat => a.marker,
                    UiTheme::Flora => b.marker,
                },
            }
        };
    }
    merged!(
        shell_root,
        shell_title,
        shell_ribbon,
        shell_body,
        shell_pane,
        shell_rail,
        shell_right_bar,
        shell_status,
        shell_backstage,
        nav_root,
        nav_header,
        nav_groups,
        nav_modules,
        nav_module,
        nav_module_active,
        nav_module_icon,
        nav_module_label,
        nav_footer,
        nav_strip,
        nav_strip_item,
        palette_backdrop,
        palette_panel,
        palette_input,
        palette_list,
        palette_row,
        palette_row_selected,
        palette_row_icon,
        palette_row_label,
        palette_row_shortcut,
        palette_empty,
        settings_root,
        settings_search,
        settings_categories,
        settings_category,
        settings_category_active,
        settings_sections,
        settings_section,
        settings_section_title,
        empty_root,
        empty_icon,
        empty_title,
        empty_detail,
        empty_action,
        scope_root,
        toolbar_row,
        drawer,
        tiles_grid,
        tile_cell,
        app_bar,
        app_bar_title,
        page,
        fab,
        bottom_tabs,
        bottom_tab,
        bottom_tab_active,
        bottom_tab_icon,
        bottom_tab_label,
    )
}

/// The look a shell with the theme option `theme` is built with: the pinned
/// theme's own look, or both looks merged ([`follow_look`]) in the structure
/// of the theme the DOM is being built for.
#[must_use]
pub(crate) fn look_for(theme: OptionUiTheme) -> ShellLook {
    use crate::widgets::themes::{flat, flora};
    match theme.into_option() {
        Some(UiTheme::Flat) => flat::shell_look(),
        Some(UiTheme::Flora) => flora::shell_look(),
        None => follow_look(UiTheme::current()),
    }
}

/// A part's declarations: its base (the structure), then the look's skin.
#[must_use]
pub(crate) fn part(
    base: &[CssPropertyWithConditions],
    skin: &[CssPropertyWithConditions],
) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(crate::widgets::themes::decl::on_base(base, skin))
}

/// A root's classes: the widget class, then the theme marker if the look has
/// one.
#[must_use]
pub(crate) fn root_classes(class: &'static str, look: &ShellLook) -> IdOrClassVec {
    let mut classes: Vec<IdOrClass> = alloc::vec![Class(AzString::from_const_str(class))];
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    IdOrClassVec::from_vec(classes)
}

/// A node's classes: `class`, then `extra` when `flag` holds (a state class:
/// active, selected, collapsed).
#[must_use]
pub(crate) fn state_classes(class: &'static str, flag: bool, extra: &'static str) -> IdOrClassVec {
    let mut classes: Vec<IdOrClass> = alloc::vec![Class(AzString::from_const_str(class))];
    if flag {
        classes.push(Class(AzString::from_const_str(extra)));
    }
    IdOrClassVec::from_vec(classes)
}

/// `id` and `class` on one node.
#[must_use]
pub(crate) fn id_and_class(id: &AzString, class: &'static str) -> IdOrClassVec {
    let mut classes: Vec<IdOrClass> = Vec::with_capacity(2);
    if !id.as_str().is_empty() {
        classes.push(IdOrClass::Id(id.clone()));
    }
    classes.push(Class(AzString::from_const_str(class)));
    IdOrClassVec::from_vec(classes)
}

/// A STATE part (active, selected) over `base` (the item at rest, with its
/// hover and focus states), as ONE declaration per property and
/// conditions on the node: a declaration of `base` that `extra` restates is
/// dropped, and so is a `:hover` rule of `base` for a property `extra` sets
/// at rest (a selected row keeps its colour under the pointer, as
/// Explorer's does). Then `theme_blocks::stack_parts` ranks the two under
/// every app theme.
#[must_use]
pub(crate) fn stack_state(
    base: &CssPropertyWithConditionsVec,
    extra: &[CssPropertyWithConditions],
) -> CssPropertyWithConditionsVec {
    use azul_css::dynamic_selector::PseudoStateType;
    let restated = |p: &CssPropertyWithConditions| {
        extra
            .iter()
            .any(|e| e.property.get_type() == p.property.get_type() && e.apply_if == p.apply_if)
    };
    let hover_overridden = |p: &CssPropertyWithConditions| {
        p.pseudo_state_conditions().contains(&PseudoStateType::Hover)
            && extra.iter().any(|e| {
                e.property.get_type() == p.property.get_type()
                    && e.pseudo_state_conditions().is_empty()
            })
    };
    let kept: Vec<CssPropertyWithConditions> = base
        .as_ref()
        .iter()
        .filter(|p| !restated(p) && !hover_overridden(p))
        .cloned()
        .collect();
    crate::widgets::themes::theme_blocks::stack_parts(
        &CssPropertyWithConditionsVec::from_vec(kept),
        &CssPropertyWithConditionsVec::from_vec(extra.to_vec()),
    )
}

/// A theme setter for the widgets a shell builds for itself (its Buttons,
/// Badges, TreeViews): `Some(theme)` when the shell is pinned, so the whole
/// shell renders the same under every app theme; `None` follows.
#[must_use]
pub(crate) const fn inner_theme(theme: OptionUiTheme) -> Option<UiTheme> {
    match theme {
        OptionUiTheme::Some(t) => Some(t),
        OptionUiTheme::None => None,
    }
}

// ---------------------------------------------------------------------------
// The bases: the shells' structure, the same in every theme (R5)
// ---------------------------------------------------------------------------

const fn simple(p: CssProperty) -> CssPropertyWithConditions {
    CssPropertyWithConditions::simple(p)
}

/// A flex column.
pub(crate) static COLUMN_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
];

/// A flex row.
pub(crate) static ROW_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
];

/// A flex column that takes the rest of its parent and may shrink below
/// its content (the rule that keeps a scrolling pane inside the window).
pub(crate) static GROW_COLUMN_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
];

/// A flex row that takes the rest of its parent and may shrink.
pub(crate) static GROW_ROW_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
];

/// A row of the chrome that keeps its height: the title row, the ribbon
/// row, the status row, a toolbar.
pub(crate) static CHROME_ROW_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// A pane: a column that fills the split pane's container (a plain block,
/// so the pane says `100%` twice), clips its content and is a keyboard stop.
pub(crate) static PANE_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_width(LayoutWidth::Px(PixelValue::const_percent(100)))),
    simple(CssProperty::const_height(LayoutHeight::Px(PixelValue::const_percent(100)))),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
    simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
];

/// A shell's root: a column that fills the window (or whatever hosts it).
pub(crate) static FILL_COLUMN_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_width(LayoutWidth::Px(PixelValue::const_percent(100)))),
    simple(CssProperty::const_height(LayoutHeight::Px(PixelValue::const_percent(100)))),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
    simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
];

/// A rail or a bar of fixed width: a column that never grows or shrinks.
pub(crate) static RAIL_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
    simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
];

/// A column that scrolls vertically.
pub(crate) static SCROLL_COLUMN_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
    simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    simple(CssProperty::const_overflow_y(LayoutOverflow::Auto)),
];

/// A clickable row of the chrome (a module button, a palette row, a
/// category, a bottom tab): a row of icon and label whose text a drag never
/// selects.
pub(crate) static ITEM_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// A bottom tab: a column of icon over label, centred, one of N equal cells.
pub(crate) static TAB_CELL_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    simple(CssProperty::const_justify_content(LayoutJustifyContent::Center)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// A label that hugs its text and never shrinks it.
pub(crate) static LABEL_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// A label that takes the rest of its row.
pub(crate) static GROW_LABEL_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

/// A centred column (the empty state).
pub(crate) static CENTRED_COLUMN_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    simple(CssProperty::const_justify_content(LayoutJustifyContent::Center)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
];

/// An overlay over its positioned parent.
pub(crate) static OVERLAY_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    simple(CssProperty::const_position(LayoutPosition::Absolute)),
];

/// A wrapping row of cells (S10's tile grid).
pub(crate) static WRAP_ROW_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_flex_wrap(LayoutFlexWrap::Wrap)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
    simple(CssProperty::const_overflow_y(LayoutOverflow::Auto)),
    simple(CssProperty::const_align_items(LayoutAlignItems::Start)),
];

/// A positioned host (S11's page, so its FAB can sit in a corner).
pub(crate) static RELATIVE_COLUMN_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
    simple(CssProperty::const_position(LayoutPosition::Relative)),
    simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
];

/// A `<p>` that carries a shell's own text.
#[must_use]
pub(crate) fn text(s: AzString) -> Dom {
    crate::widgets::widget_p_with_text(s)
}

// ---------------------------------------------------------------------------
// Lints over every shell (the manifest the tests below share)
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod fixtures {
    //! Every shell built with placeholder content, for the lints that must
    //! hold across the whole set: one entry per shell and shared piece, in
    //! S-order.

    use alloc::{string::String, vec::Vec};

    use azul_core::dom::{Dom, DomVec};
    use azul_css::{AzString, StringVec};

    use super::*;

    /// A slot's placeholder: property-free, as an app's content is.
    pub(crate) fn slot() -> Dom {
        Dom::create_div()
    }

    fn labels(items: &[&str]) -> StringVec {
        StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect::<Vec<_>>())
    }

    fn tree(label: &str) -> crate::widgets::tree_view::TreeViewNode {
        crate::widgets::tree_view::TreeViewNode::new(label)
            .with_expanded(true)
            .with_child(crate::widgets::tree_view::TreeViewNode::new("Inbox"))
    }

    pub(crate) fn navigation_pane() -> ShellNavigationPane {
        ShellNavigationPane::create()
            .with_group(ShellNavigationGroup::create(AzString::from("Favorites"), tree("Favorites")))
            .with_group(
                ShellNavigationGroup::create(AzString::from("me@example.org"), tree("me@example.org"))
                    .with_count(3),
            )
            .with_module(ShellNavigationModule::create(AzString::from("Mail"), AzString::from("mail")))
            .with_module(
                ShellNavigationModule::create(AzString::from("Calendar"), AzString::from("event"))
                    .with_badge(AzString::from("2")),
            )
            .with_module(ShellNavigationModule::create(
                AzString::from("Contacts"),
                AzString::from("person"),
            ))
            .with_active_module(0)
    }

    pub(crate) fn command_palette() -> ShellCommandPalette {
        ShellCommandPalette::create()
            .with_command(
                ShellPaletteCommand::create(AzString::from("New message"))
                    .with_shortcut(AzString::from("Ctrl+N"))
                    .with_icon(AzString::from("mail")),
            )
            .with_command(ShellPaletteCommand::create(AzString::from("Archive")))
            .with_command(ShellPaletteCommand::create(AzString::from("Settings")))
            .with_open(true)
    }

    pub(crate) fn settings_layout() -> ShellSettingsLayout {
        ShellSettingsLayout::create(labels(&["General", "Accounts", "Appearance"]))
            .with_section(ShellSettingsSection::create(AzString::from("Startup"), slot()))
            .with_section(ShellSettingsSection::create(AzString::from("Language"), slot()))
            .with_active_category(1)
    }

    pub(crate) fn empty_state() -> ShellEmptyState {
        ShellEmptyState::create(AzString::from("No message selected"))
            .with_icon(AzString::from("mail"))
            .with_detail(AzString::from("Pick a message to read it here."))
            .with_action_label(AzString::from("New message"))
    }

    pub(crate) fn office_shell() -> OfficeShell {
        OfficeShell::create()
            .with_title_row(slot())
            .with_ribbon(slot())
            .with_pane(
                ShellPane::create(AzString::from("shell-navigation"), slot())
                    .with_kind(ShellPaneKind::Navigation)
                    .with_label(AzString::from("Navigation"))
                    .with_ratio(0.2),
            )
            .with_pane(
                ShellPane::create(AzString::from("shell-list"), slot())
                    .with_label(AzString::from("List"))
                    .with_ratio(0.4),
            )
            .with_pane(
                ShellPane::create(AzString::from("shell-reading"), slot())
                    .with_kind(ShellPaneKind::Main)
                    .with_label(AzString::from("Reading")),
            )
            .with_right_bar(slot())
            .with_status_bar(slot())
    }

    pub(crate) fn mobile_shell() -> MobileShell {
        MobileShell::create(AzString::from("Inbox"))
            .with_page(slot())
            .with_page(slot())
            .with_fab(slot())
            .with_tab(ShellBottomTab::create(AzString::from("Mail"), AzString::from("mail")))
            .with_tab(ShellBottomTab::create(AzString::from("Calendar"), AzString::from("event")))
            .with_tab(ShellBottomTab::create(AzString::from("Settings"), AzString::from("settings")))
            .with_active_tab(0)
    }

    /// Every shell and shared piece, built with placeholder content, with
    /// no theme pin (the follow path).
    pub(crate) fn every_shell() -> Vec<(&'static str, Dom)> {
        alloc::vec![
            ("office_shell", office_shell().dom()),
            ("navigation_pane", navigation_pane().dom()),
            (
                "navigation_pane (collapsed)",
                navigation_pane().with_collapsed(true).dom()
            ),
            ("command_palette", command_palette().dom()),
            ("settings_layout", settings_layout().dom()),
            ("empty_state", empty_state().dom()),
            ("theme_scope", ShellThemeScope::create(slot()).dom()),
            (
                "document_shell",
                DocumentShell::create(slot())
                    .with_title_row(slot())
                    .with_ribbon(slot())
                    .with_navigation(slot())
                    .with_side_pane(slot())
                    .with_status_bar(slot())
                    .dom()
            ),
            (
                "canvas_shell",
                CanvasShell::create(slot())
                    .with_menu_bar(slot())
                    .with_tool_options(slot())
                    .with_tool_palette(slot())
                    .with_document_tabs(slot())
                    .with_panels(slot())
                    .with_drawer(slot())
                    .with_status_bar(slot())
                    .dom()
            ),
            (
                "timeline_shell",
                TimelineShell::create(slot(), slot(), slot(), slot(), slot())
                    .with_menu_bar(slot())
                    .with_meters(slot())
                    .dom()
            ),
            (
                "pim_shell",
                PimShell::create(slot(), slot(), slot())
                    .with_title_row(slot())
                    .with_ribbon(slot())
                    .with_todo_bar(slot())
                    .with_status_bar(slot())
                    .dom()
            ),
            (
                "browser_shell",
                BrowserShell::create(slot(), slot(), slot())
                    .with_title_row(slot())
                    .with_ribbon(slot())
                    .with_details(slot())
                    .with_preview(slot())
                    .with_status_bar(slot())
                    .dom()
            ),
            (
                "records_shell",
                RecordsShell::create(slot(), slot())
                    .with_title_row(slot())
                    .with_cards(slot())
                    .with_form(slot())
                    .with_status_bar(slot())
                    .dom()
            ),
            (
                "media_shell",
                MediaShell::create(slot(), slot(), slot())
                    .with_title_row(slot())
                    .dom()
            ),
            (
                "developer_shell",
                DeveloperShell::create(slot(), slot(), slot())
                    .with_panel(slot())
                    .with_status_bar(slot())
                    .dom()
            ),
            (
                "utility_shell",
                UtilityShell::create(slot())
                    .with_title_row(slot())
                    .with_modes(slot())
                    .dom()
            ),
            (
                "call_shell",
                CallShell::create(DomVec::from_vec(alloc::vec![slot(), slot(), slot()]), slot())
                    .with_header(slot())
                    .with_side_panel(slot())
                    .with_devices(slot())
                    .dom()
            ),
            ("mobile_shell", mobile_shell().dom()),
        ]
    }

    /// The same set, pinned to `theme`.
    pub(crate) fn every_shell_pinned(theme: UiTheme) -> Vec<(String, Dom)> {
        alloc::vec![
            (
                alloc::format!("office_shell ({})", theme.name()),
                office_shell().with_theme(theme).dom()
            ),
            (
                alloc::format!("navigation_pane ({})", theme.name()),
                navigation_pane().with_theme(theme).dom()
            ),
            (
                alloc::format!("command_palette ({})", theme.name()),
                command_palette().with_theme(theme).dom()
            ),
            (
                alloc::format!("settings_layout ({})", theme.name()),
                settings_layout().with_theme(theme).dom()
            ),
            (
                alloc::format!("empty_state ({})", theme.name()),
                empty_state().with_theme(theme).dom()
            ),
            (
                alloc::format!("mobile_shell ({})", theme.name()),
                mobile_shell().with_theme(theme).dom()
            ),
        ]
    }
}

#[cfg(test)]
mod shell_lints {
    //! The invariants every shell owes, asked once over the whole set - on
    //! the shells' OWN nodes. The widgets a shell builds for itself (its
    //! Buttons, TreeViews, SplitPanes, TextInputs) answer for themselves in
    //! their own suites, some with allow-lists of their own.

    use alloc::{format, string::String, vec::Vec};
    use std::collections::BTreeSet;

    use azul_core::dom::{Dom, IdOrClass};
    use azul_css::{
        dynamic_selector::{CssPropertyWithConditions, DynamicSelectorVec},
        props::property::CssPropertyType,
    };

    use super::fixtures::{every_shell, every_shell_pinned};
    use crate::widgets::themes::{
        theme_blocks::checks::{under, BOTH},
        theme_checks as tc,
    };

    /// The class prefixes of the nodes the shells build themselves.
    const OWN_PREFIXES: &[&str] = &[
        "__azul-native-office-shell",
        "__azul-native-navigation-pane",
        "__azul-native-command-palette",
        "__azul-native-settings-layout",
        "__azul-native-empty-state",
        "__azul-native-theme-scope",
        "__azul-native-browser-shell",
        "__azul-native-records-shell",
        "__azul-native-utility-shell",
        "__azul-native-canvas-shell",
        "__azul-native-timeline-shell",
        "__azul-native-call-shell",
        "__azul-native-mobile-shell",
    ];

    /// Whether `node` is one the shells built (by its classes).
    fn own(node: &Dom) -> bool {
        node.root.get_ids_and_classes().as_ref().iter().any(|c| match c {
            IdOrClass::Class(s) => OWN_PREFIXES.iter().any(|p| s.as_str().starts_with(p)),
            IdOrClass::Id(_) => false,
        })
    }

    /// The paths (`root/0/2`) of the shell's own nodes.
    fn own_paths(dom: &Dom) -> BTreeSet<String> {
        tc::nodes(dom)
            .into_iter()
            .filter(|(_, n)| own(n))
            .map(|(p, _)| p)
            .collect()
    }

    /// `messages` (each starting with the node's path) about own nodes only.
    fn about_own(dom: &Dom, messages: Vec<String>) -> Vec<String> {
        let paths = own_paths(dom);
        messages
            .into_iter()
            .filter(|m| {
                let path = m.split([':', ' ']).next().unwrap_or("");
                paths.contains(path)
            })
            .collect()
    }

    /// Every declaration on one own node that repeats an EARLIER declaration
    /// of the same property under the same conditions: the later one
    /// silently wins, so the first was a mistake (a base and a skin both
    /// writing `display`, a look listing `padding` twice). One message per
    /// repeat.
    fn duplicate_declarations(dom: &Dom) -> Vec<String> {
        let mut out = Vec::new();
        for (path, node) in tc::nodes(dom) {
            if !own(node) {
                continue;
            }
            let props: Vec<CssPropertyWithConditions> = node
                .root
                .style
                .iter_inline_properties()
                .map(|(p, c)| CssPropertyWithConditions {
                    property: p.clone(),
                    apply_if: c.clone(),
                })
                .collect();
            let mut seen: Vec<(CssPropertyType, DynamicSelectorVec)> = Vec::new();
            for p in &props {
                let key = (p.property.get_type(), p.apply_if.clone());
                if seen.contains(&key) {
                    out.push(format!(
                        "{path}: {:?} declared twice under {:?}",
                        key.0, key.1
                    ));
                } else {
                    seen.push(key);
                }
            }
        }
        out
    }

    /// No shell declares a property twice on one node under one set of
    /// conditions - not in a pinned look, not in the merged one.
    #[test]
    fn no_shell_declares_a_property_twice_on_one_node() {
        let mut bad = Vec::new();
        for theme in BOTH {
            for (name, dom) in every_shell_pinned(theme) {
                for line in duplicate_declarations(&dom) {
                    bad.push(format!("{name}: {line}"));
                }
            }
            for (name, dom) in under(theme, every_shell) {
                for line in duplicate_declarations(&dom) {
                    bad.push(format!("{name} (follow, built for {}): {line}", theme.name()));
                }
            }
        }
        assert!(
            bad.is_empty(),
            "duplicated declarations ({}):\n  {}",
            bad.len(),
            bad.join("\n  ")
        );
    }

    /// The structure of every shell (flex, overflow, cursor, ...) is declared
    /// once, outside the theme blocks: a theme paints a shell, it never
    /// re-lays it out.
    #[test]
    fn every_shell_declares_its_structure_once_for_every_theme() {
        for theme in BOTH {
            for (name, dom) in under(theme, every_shell) {
                let themed = about_own(&dom, tc::themed_structure(&dom, &[]));
                assert!(
                    themed.is_empty(),
                    "{name} built for {}: structure inside a theme block:\n  {}",
                    theme.name(),
                    themed.join("\n  ")
                );
            }
        }
    }

    /// Every pinned shell keeps the theme invariants on its own nodes: dark
    /// twins after their light half, no shadowed state, a focus ring on
    /// every keyboard stop in both modes.
    #[test]
    fn every_pinned_shell_keeps_the_theme_invariants() {
        for theme in BOTH {
            for (name, dom) in every_shell_pinned(theme) {
                let halves = about_own(&dom, tc::half_pairs(&dom));
                assert!(halves.is_empty(), "{name}: half pairs:\n  {}", halves.join("\n  "));
                let shadowed = about_own(&dom, tc::shadowed_states(&dom));
                assert!(
                    shadowed.is_empty(),
                    "{name}: shadowed states:\n  {}",
                    shadowed.join("\n  ")
                );
                for (path, node) in tc::focusable(&dom) {
                    if !own(node) {
                        continue;
                    }
                    assert!(tc::has_focus_ring(node, false), "{name}: {path} has no light focus ring");
                    assert!(tc::has_focus_ring(node, true), "{name}: {path} has no dark focus ring");
                }
            }
        }
    }

    /// A shell built with no theme pin follows the app theme: under each
    /// app theme it IS the pinned one.
    #[test]
    fn every_shell_without_a_theme_follows_the_app_theme() {
        use super::fixtures::{
            command_palette, empty_state, mobile_shell, navigation_pane, office_shell,
            settings_layout,
        };
        use crate::widgets::themes::{theme_blocks::checks, UiTheme};
        checks::assert_follows_the_app_theme(
            "office_shell",
            || office_shell().dom(),
            |t: UiTheme| office_shell().with_theme(t).dom(),
        );
        checks::assert_follows_the_app_theme(
            "navigation_pane",
            || navigation_pane().dom(),
            |t: UiTheme| navigation_pane().with_theme(t).dom(),
        );
        checks::assert_follows_the_app_theme(
            "command_palette",
            || command_palette().dom(),
            |t: UiTheme| command_palette().with_theme(t).dom(),
        );
        checks::assert_follows_the_app_theme(
            "settings_layout",
            || settings_layout().dom(),
            |t: UiTheme| settings_layout().with_theme(t).dom(),
        );
        checks::assert_follows_the_app_theme(
            "empty_state",
            || empty_state().dom(),
            |t: UiTheme| empty_state().with_theme(t).dom(),
        );
        checks::assert_follows_the_app_theme(
            "mobile_shell",
            || mobile_shell().dom(),
            |t: UiTheme| mobile_shell().with_theme(t).dom(),
        );
    }

    /// The whole set builds, and every shell root carries its widget class.
    #[test]
    fn every_shell_builds_and_names_its_root() {
        for (name, dom) in every_shell() {
            let classes = dom.root.get_ids_and_classes();
            assert!(
                classes.as_ref().iter().any(|c| matches!(
                    c,
                    azul_core::dom::IdOrClass::Class(s) if s.as_str().starts_with("__azul-native-")
                )),
                "{name}: the root has no widget class: {classes:?}"
            );
        }
    }
}
