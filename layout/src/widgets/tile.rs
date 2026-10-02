//! Tile widget - one item of a file manager's tile view: an icon beside a
//! column of the title, an optional CAPACITY BAR and a detail line. Windows
//! Explorer's "Computer" view is a grid of these: a drive icon, "Local Disk
//! (C:)", a blue bar that turns red when the disk is nearly full, and
//! "324 GB free of 456 GB".
//!
//! The bar is the [`ProgressBar`] widget (the tile picks its fill from the
//! theme: the capacity blue, or the alarm red past [`TileCapacity::is_nearly_full`]),
//! the text is [`TileCapacity::label`]. A tile without a capacity shows its
//! [`Tile::detail`] instead ("S3 bucket", "Folder").
//!
//! A tile is a keyboard stop and a list item for assistive technology; it
//! reports a click and a double-click (open) to the app, which owns the
//! selection ([`Tile::is_selected`]) and rebuilds.
//!
//! Key types: [`Tile`], [`TileCapacity`], [`TileOnClick`].

use alloc::{string::String, vec::Vec};

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec,
        TabIndex,
    },
    refany::RefAny,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option,
    props::{
        basic::{length::FloatValue, pixel::PixelValue},
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutFlexShrink, LayoutMinWidth, LayoutOverflow,
        },
        property::{CssProperty, StyleWhiteSpaceValue},
        style::{StyleBackgroundContentVec, StyleCursor, StyleUserSelect, StyleWhiteSpace},
    },
    AzString,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::progressbar::ProgressBar,
};

static TILE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-tile"))];
static TILE_ICON_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-tile-icon"))];
static TILE_COLUMN_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-tile-column"))];
static TILE_TITLE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-tile-title"))];
static TILE_BAR_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-tile-bar"))];
static TILE_DETAIL_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-tile-detail"))];

/// Callback invoked when a tile is clicked (a selection) or double-clicked
/// (an open).
pub type TileOnClickCallbackType = extern "C" fn(RefAny, CallbackInfo) -> Update;
impl_widget_callback!(
    TileOnClick,
    OptionTileOnClick,
    TileOnClickCallback,
    TileOnClickCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        TileOnClickCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: TILE_ON_CLICK_INVOKER,
    invoker_ty:     AzTileOnClickCallbackInvoker,
    thunk_fn:       az_tile_on_click_callback_thunk,
    setter_fn:      AzApp_setTileOnClickCallbackInvoker,
    from_handle_fn: AzTileOnClickCallback_createFromHostHandle,
    from_handle_byref_fn: AzTileOnClickCallback_createFromHostHandleByref,
}

/// How full a volume is: what the capacity bar and its "x free of y" text
/// show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct TileCapacity {
    /// The volume's size, in bytes.
    pub total: u64,
    /// The bytes still free.
    pub free: u64,
}

impl_option!(
    TileCapacity,
    OptionTileCapacity,
    [Debug, Copy, Clone, PartialEq, Eq]
);

impl TileCapacity {
    /// A capacity from the volume's size and its free bytes.
    #[must_use]
    pub const fn create(total: u64, free: u64) -> Self {
        Self { total, free }
    }

    /// The bytes in use.
    #[must_use]
    pub const fn used(&self) -> u64 {
        self.total.saturating_sub(self.free)
    }

    /// The bar's fill: the used share of the volume, 0 to 100.
    #[must_use]
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    pub fn used_percent(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        ((self.used() as f64 / self.total as f64) * 100.0).clamp(0.0, 100.0) as f32
    }

    /// Whether less than a tenth of the volume is free - when Explorer
    /// paints the bar red.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn is_nearly_full(&self) -> bool {
        self.total > 0 && (self.free as f64) < (self.total as f64) * 0.1
    }

    /// "324 GB free of 456 GB".
    #[must_use]
    pub fn label(&self) -> AzString {
        AzString::from(alloc::format!(
            "{} free of {}",
            format_bytes(self.free),
            format_bytes(self.total)
        ))
    }
}

/// `bytes` as a file manager writes it: "500 B", "1.5 KB", "324 GB" - 1024
/// per step, one decimal below ten, none above.
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    if bytes < 1024 {
        return alloc::format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if value < 10.0 {
        alloc::format!("{value:.1} {}", UNITS[unit])
    } else {
        alloc::format!("{} {}", value.round() as u64, UNITS[unit])
    }
}

/// One tile of a tile view: an icon, a title, a capacity bar or a detail
/// line, a click and a double-click.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct Tile {
    /// The icon beside the column (a `Dom::create_icon` name: "storage",
    /// "folder", "cloud"), or empty for an empty slot of the same width.
    pub icon: AzString,
    /// The first line: the drive's or file's name.
    pub title: AzString,
    /// The line under the title when the tile has no capacity ("S3 bucket").
    pub detail: AzString,
    /// The capacity bar and its "x free of y" text; `None` shows `detail`.
    pub capacity: OptionTileCapacity,
    /// Fires on a click: the app selects the tile and rebuilds.
    pub on_click: OptionTileOnClick,
    /// Fires on a double-click: the app opens what the tile stands for.
    pub on_double_click: OptionTileOnClick,
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: crate::widgets::themes::OptionUiTheme,
    /// Drawn as the selected item, and announced as such.
    pub is_selected: bool,
}

/// What a theme decides about a tile: the SKIN of each part, laid over the
/// part's base (the tile's structure, the same in every theme: `TILE_*_BASE`)
/// by [`build`]; built by `themes::flat::tile_look` and
/// `themes::flora::tile_look`.
pub(crate) struct TileLook {
    /// The tile's box at rest, its hover and focus states included.
    pub tile: Vec<CssPropertyWithConditions>,
    /// Added to the box when the tile is selected.
    pub tile_selected: Vec<CssPropertyWithConditions>,
    /// The icon (or the empty slot in its place).
    pub icon: Vec<CssPropertyWithConditions>,
    /// The title line.
    pub title: Vec<CssPropertyWithConditions>,
    /// The detail / capacity line.
    pub detail: Vec<CssPropertyWithConditions>,
    /// The box around the capacity bar.
    pub bar: Vec<CssPropertyWithConditions>,
    /// The bar's height, in px.
    pub bar_height: isize,
    /// The bar's track.
    pub bar_track: StyleBackgroundContentVec,
    /// The bar's fill.
    pub bar_fill: StyleBackgroundContentVec,
    /// The bar's fill when the volume is nearly full.
    pub bar_fill_alarm: StyleBackgroundContentVec,
    /// The theme's marker class on the tile, if it has one.
    pub marker: Option<&'static str>,
}

// ---- the base: a tile's structure, in every theme ----

/// The tile: a row of the icon and the column, a click target whose text a
/// drag never selects.
pub(crate) static TILE_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// The icon keeps its size beside the column.
pub(crate) static TILE_ICON_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// The column takes the rest of the tile and may shrink below its text.
pub(crate) static TILE_COLUMN_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

/// A text line of the column: one line, clipped at the column's edge.
pub(crate) static TILE_LINE_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::WhiteSpace(StyleWhiteSpaceValue::Exact(
        StyleWhiteSpace::Nowrap,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
];

/// The bar's box: a block across the column.
pub(crate) static TILE_BAR_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

impl Tile {
    /// A tile with `title`, no icon, no detail, no capacity.
    #[must_use]
    pub fn create(title: AzString) -> Self {
        Self {
            icon: AzString::from_const_str(""),
            title,
            detail: AzString::from_const_str(""),
            capacity: OptionTileCapacity::None,
            on_click: None.into(),
            on_double_click: None.into(),
            theme: crate::widgets::themes::OptionUiTheme::None,
            is_selected: false,
        }
    }

    /// The icon beside the column (a `Dom::create_icon` name).
    pub fn set_icon(&mut self, icon: AzString) {
        self.icon = icon;
    }

    /// [`Self::set_icon`] for the builder chain.
    #[must_use]
    pub fn with_icon(mut self, icon: AzString) -> Self {
        self.set_icon(icon);
        self
    }

    /// The line under the title when the tile has no capacity.
    pub fn set_detail(&mut self, detail: AzString) {
        self.detail = detail;
    }

    /// [`Self::set_detail`] for the builder chain.
    #[must_use]
    pub fn with_detail(mut self, detail: AzString) -> Self {
        self.set_detail(detail);
        self
    }

    /// The capacity bar and its text.
    pub const fn set_capacity(&mut self, capacity: TileCapacity) {
        self.capacity = OptionTileCapacity::Some(capacity);
    }

    /// [`Self::set_capacity`] for the builder chain.
    #[must_use]
    pub const fn with_capacity(mut self, capacity: TileCapacity) -> Self {
        self.set_capacity(capacity);
        self
    }

    /// Draw and announce the tile as selected.
    pub const fn set_selected(&mut self, selected: bool) {
        self.is_selected = selected;
    }

    /// [`Self::set_selected`] for the builder chain.
    #[must_use]
    pub const fn with_selected(mut self, selected: bool) -> Self {
        self.set_selected(selected);
        self
    }

    /// Pin the widget theme; unset, the tile follows the app theme.
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// The click: the app selects the tile.
    pub fn set_on_click<C: Into<TileOnClickCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_click = Some(TileOnClick {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_click`] for the builder chain.
    #[must_use]
    pub fn with_on_click<C: Into<TileOnClickCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_on_click(data, callback);
        self
    }

    /// The double-click: the app opens the tile.
    pub fn set_on_double_click<C: Into<TileOnClickCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.on_double_click = Some(TileOnClick {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_double_click`] for the builder chain.
    #[must_use]
    pub fn with_on_double_click<C: Into<TileOnClickCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_double_click(data, callback);
        self
    }

    /// Replaces `self` with an empty tile and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }

    /// The tile's DOM. The look comes from the theme module
    /// (`themes::flat::tile` / `themes::flora::tile`); `None` carries both
    /// looks, each in its `@theme(<name>)` block, and the app theme picks.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::UiTheme;
        match self.theme.into_option() {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::tile(self),
            Some(UiTheme::Flat) => crate::widgets::themes::flat::tile(self),
            // No theme: follow the app theme - both looks in one DOM, each
            // inside its `@theme(<name>)` block, and the app theme picks.
            None => crate::widgets::themes::theme_blocks::follow_app_theme(
                self,
                crate::widgets::themes::flat::tile,
                crate::widgets::themes::flora::tile,
            ),
        }
    }
}

impl Default for Tile {
    fn default() -> Self {
        Self::create(AzString::from_const_str(""))
    }
}

impl From<Tile> for Dom {
    fn from(t: Tile) -> Self {
        t.dom()
    }
}

/// A click hook as the engine wires it on the tile's root.
fn hook(event: EventFilter, on: OptionTileOnClick) -> Option<CoreCallbackData> {
    on.into_option().map(|TileOnClick { refany, callback }| {
        CoreCallbackData::create(
            event,
            refany,
            CoreCallback {
                cb: callback.cb as *const () as usize,
                ctx: callback.ctx,
            },
        )
    })
}

/// The tile's DOM in `look`: root [icon, column [title, (bar, capacity text)
/// | detail]]. Every part is its base (the structure), then the look's skin.
pub(crate) fn build(tile: Tile, look: &TileLook) -> Dom {
    let part = |base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]| {
        CssPropertyWithConditionsVec::from_vec(crate::widgets::themes::decl::on_base(base, skin))
    };
    let Tile {
        icon,
        title,
        detail,
        capacity,
        on_click,
        on_double_click,
        theme,
        is_selected,
    } = tile;

    // The icon, or an empty slot of the icon's size so the columns line up.
    let icon_node = if icon.as_str().is_empty() {
        Dom::create_div()
    } else {
        Dom::create_icon(icon)
    }
    .with_ids_and_classes(IdOrClassVec::from_const_slice(TILE_ICON_CLASS))
    .with_css_props(part(TILE_ICON_BASE, &look.icon));

    let line = |text: AzString, class: &'static [IdOrClass], skin: &[CssPropertyWithConditions]| {
        crate::widgets::widget_p_with_text(text)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(class))
            .with_css_props(part(TILE_LINE_BASE, skin))
    };

    let mut column = Vec::with_capacity(3);
    column.push(line(title.clone(), TILE_TITLE_CLASS, &look.title));
    match capacity.into_option() {
        Some(capacity) => {
            let fill = if capacity.is_nearly_full() {
                look.bar_fill_alarm.clone()
            } else {
                look.bar_fill.clone()
            };
            let mut bar = ProgressBar::create(capacity.used_percent())
                .with_height(PixelValue::const_px(look.bar_height))
                .with_bar_background(fill)
                .with_container_background(look.bar_track.clone())
                .with_accessibility_name(AzString::from(alloc::format!(
                    "{}: {}",
                    title.as_str(),
                    capacity.label().as_str()
                )));
            if let Some(theme) = theme.into_option() {
                bar = bar.with_theme(theme);
            }
            column.push(
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(TILE_BAR_CLASS))
                    .with_css_props(part(TILE_BAR_BASE, &look.bar))
                    .with_child(bar.dom()),
            );
            column.push(line(capacity.label(), TILE_DETAIL_CLASS, &look.detail));
        }
        None => {
            if !detail.as_str().is_empty() {
                column.push(line(detail, TILE_DETAIL_CLASS, &look.detail));
            }
        }
    }

    let column = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(TILE_COLUMN_CLASS))
        .with_css_props(part(TILE_COLUMN_BASE, &[]))
        .with_children(DomVec::from_vec(column));

    let mut skin = look.tile.clone();
    if is_selected {
        skin.extend(look.tile_selected.iter().cloned());
    }
    let mut classes: Vec<IdOrClass> = TILE_CLASS.to_vec();
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    let callbacks: Vec<CoreCallbackData> = [
        hook(EventFilter::Hover(HoverEventFilter::Click), on_click),
        hook(
            EventFilter::Hover(HoverEventFilter::DoubleClick),
            on_double_click,
        ),
    ]
    .into_iter()
    .flatten()
    .collect();

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(part(TILE_BASE, &skin))
        .with_tab_index(TabIndex::Auto)
        // An ITEM of the tile view, selected or not; its NAME is its title.
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::ListItem,
            states: if is_selected {
                azul_core::a11y::AccessibilityStateVec::from_vec(alloc::vec![
                    azul_core::a11y::AccessibilityState::Selected,
                ])
            } else {
                azul_core::a11y::AccessibilityStateVec::from_const_slice(&[])
            },
            ..Default::default()
        })
        .with_callbacks(callbacks.into())
        .with_children(DomVec::from_vec(alloc::vec![icon_node, column]))
}

#[cfg(test)]
mod tile_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::dom::NodeType;

    use super::*;
    use crate::widgets::themes::{theme_blocks::checks, UiTheme};

    const GB: u64 = 1024 * 1024 * 1024;

    fn has_class(node: &Dom, name: &str) -> bool {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, Class(s) if s.as_str() == name))
    }

    /// The text of a `p > text` label.
    fn text_of(node: &Dom) -> Option<&str> {
        match node.children.as_ref() {
            [only] => match only.root.get_node_type() {
                NodeType::Text(s) => Some(s.as_ref().as_str()),
                _ => None,
            },
            _ => None,
        }
    }

    fn home() -> Tile {
        Tile::create(AzString::from("Home"))
            .with_icon(AzString::from("home"))
            .with_capacity(TileCapacity::create(456 * GB, 324 * GB))
    }

    fn bucket() -> Tile {
        Tile::create(AzString::from("S3 Drive"))
            .with_icon(AzString::from("cloud"))
            .with_detail(AzString::from("S3 bucket"))
    }

    #[test]
    fn bytes_read_like_a_file_manager_writes_them() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(324 * GB), "324 GB");
        assert_eq!(format_bytes(456 * GB), "456 GB");
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024 * 3 / 2), "1.5 TB");
    }

    #[test]
    fn a_capacity_says_how_much_is_free_of_the_total_and_when_it_is_nearly_full() {
        let c = TileCapacity::create(456 * GB, 324 * GB);
        assert_eq!(c.label().as_str(), "324 GB free of 456 GB");
        assert_eq!(c.used(), 132 * GB);
        assert!((c.used_percent() - 28.947).abs() < 0.01, "{}", c.used_percent());
        assert!(!c.is_nearly_full());
        let full = TileCapacity::create(100, 5);
        assert!(full.is_nearly_full(), "5% free is nearly full");
        assert!((full.used_percent() - 95.0).abs() < 0.01);
        assert!(!TileCapacity::create(100, 10).is_nearly_full(), "a tenth free is not");
        assert!(!TileCapacity::create(0, 0).is_nearly_full(), "no volume, no alarm");
        assert_eq!(TileCapacity::create(0, 0).used_percent(), 0.0);
    }

    #[test]
    fn a_capacity_tile_is_an_icon_beside_a_title_a_bar_and_the_free_text() {
        for theme in checks::BOTH {
            let dom = home().with_theme(theme).dom();
            let parts = dom.children.as_ref();
            assert_eq!(parts.len(), 2, "{}: icon, column", theme.name());
            assert!(has_class(&parts[0], "__azul-native-tile-icon"), "{}", theme.name());
            assert!(
                matches!(parts[0].root.get_node_type(), NodeType::Icon(_)),
                "{}: the icon is an icon node",
                theme.name()
            );
            let column = parts[1].children.as_ref();
            assert_eq!(column.len(), 3, "{}: title, bar, text", theme.name());
            assert_eq!(text_of(&column[0]), Some("Home"));
            assert!(
                has_class(&column[1], "__azul-native-tile-bar"),
                "{}: the bar sits between the title and the text",
                theme.name()
            );
            assert!(
                column[1].children.as_ref()[0].root.is_virtual_view_node(),
                "{}: the bar is the progress bar widget",
                theme.name()
            );
            assert_eq!(text_of(&column[2]), Some("324 GB free of 456 GB"));
        }
    }

    #[test]
    fn a_tile_without_a_capacity_shows_its_detail_and_no_bar() {
        let dom = bucket().with_theme(UiTheme::Flat).dom();
        let column = dom.children.as_ref()[1].children.as_ref();
        assert_eq!(column.len(), 2, "title, detail");
        assert_eq!(text_of(&column[0]), Some("S3 Drive"));
        assert_eq!(text_of(&column[1]), Some("S3 bucket"));
        let bare = Tile::create(AzString::from("Empty"))
            .with_theme(UiTheme::Flat)
            .dom();
        assert_eq!(bare.children.as_ref()[1].children.as_ref().len(), 1);
        assert!(
            matches!(bare.children.as_ref()[0].root.get_node_type(), NodeType::Div),
            "no icon: an empty slot keeps the column in place"
        );
    }

    #[test]
    fn a_selected_tile_says_so_and_every_tile_is_a_keyboard_stop() {
        use azul_core::a11y::{AccessibilityRole, AccessibilityState};
        let plain = home().with_theme(UiTheme::Flat).dom();
        let picked = home().with_selected(true).with_theme(UiTheme::Flat).dom();
        for dom in [&plain, &picked] {
            assert!(dom.root.get_tab_index().is_some());
            assert_eq!(
                dom.root.get_accessibility_info().map(|i| i.role),
                Some(AccessibilityRole::ListItem)
            );
        }
        let states = |dom: &Dom| {
            dom.root
                .get_accessibility_info()
                .map(|i| i.states.as_ref().to_vec())
                .unwrap_or_default()
        };
        assert!(states(&plain).is_empty());
        assert_eq!(states(&picked), vec![AccessibilityState::Selected]);
        assert_ne!(
            plain.root.get_style(),
            picked.root.get_style(),
            "the selected tile is painted differently"
        );
    }

    type Log = Arc<Mutex<Vec<&'static str>>>;

    extern "C" fn record_click(mut data: RefAny, _info: CallbackInfo) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push("click");
        }
        Update::DoNothing
    }

    extern "C" fn record_open(mut data: RefAny, _info: CallbackInfo) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push("open");
        }
        Update::RefreshDom
    }

    #[test]
    fn a_tile_carries_its_click_and_double_click_on_its_root() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = home()
            .with_on_click(RefAny::new(log.clone()), record_click as TileOnClickCallbackType)
            .with_on_double_click(RefAny::new(log.clone()), record_open as TileOnClickCallbackType)
            .with_theme(UiTheme::Flat)
            .dom();
        let events: Vec<EventFilter> = dom
            .root
            .get_callbacks()
            .as_ref()
            .iter()
            .map(|cb| cb.event)
            .collect();
        assert_eq!(
            events,
            vec![
                EventFilter::Hover(HoverEventFilter::Click),
                EventFilter::Hover(HoverEventFilter::DoubleClick),
            ]
        );
        let none = home().with_theme(UiTheme::Flat).dom();
        assert!(none.root.get_callbacks().as_ref().is_empty());
    }

    #[test]
    fn a_tile_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "tile",
            || home().dom(),
            |t: UiTheme| home().with_theme(t).dom(),
        );
        checks::assert_follows_the_app_theme(
            "tile (detail)",
            || bucket().with_selected(true).dom(),
            |t: UiTheme| bucket().with_selected(true).with_theme(t).dom(),
        );
    }

    #[test]
    fn a_flora_tile_carries_the_flora_marker() {
        let dom = home().with_theme(UiTheme::Flora).dom();
        assert!(has_class(&dom, "__azul-theme-flora"));
        assert!(!has_class(&home().with_theme(UiTheme::Flat).dom(), "__azul-theme-flora"));
    }
}
