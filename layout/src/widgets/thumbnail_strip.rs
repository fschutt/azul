//! Thumbnail strip widget - a list of previews the user picks from and
//! reorders: PowerPoint's slide rail (a column: the number beside each
//! slide) and its slide sorter (a wrapping grid: the number under each),
//! with section headers that fold their run of items away.
//!
//! THE APP OWNS THE ITEMS: each [`ThumbnailItem`] carries the preview `Dom`
//! the app drew (the strip gives it a `thumb_width` x `thumb_height` px box
//! that clips it), its number, its accessible name, a badge glyph (a build,
//! a transition), and whether it is selected, hidden (dimmed) or the first
//! of a section ([`ThumbnailItem::section`]: a header titled so goes before
//! it; [`ThumbnailItem::section_collapsed`] folds the section's items). One
//! callback reports what happened ([`ThumbnailStripEvent`]):
//!
//! - `Select` (a click, or an arrow key: the item `index`; `shift` extends
//!   from the anchor, `ctrl` toggles - the app's selection rule);
//! - `Activate` (a double-click or Enter: open it in the editor);
//! - `Move` (a drag dropped on another item, or Ctrl+Up / Ctrl+Down: move
//!   `index` - and the app's other selected items - so they stand before the
//!   item that is now at `target`; `target == count` is the end);
//! - `Delete` (Delete / Backspace on the focused item);
//! - `SectionToggled` (a click on a header: `index` is the section's first
//!   item).
//!
//! DRAG REORDER goes through the engine's drag and drop: every item is
//! `draggable`; `DragStart` sets the item's index as drag data (MIME
//! [`DRAG_MIME`]), `DragOver` accepts a move, `Drop` reports the `Move`.
//! A drop on an item after the dragged one puts the dragged one after it,
//! a drop on one before puts it before.
//!
//! KEYBOARD (WAI-ARIA APG listbox): the items are ONE Tab stop - the active
//! item. Up / Down (and Left / Right in the grid) select the neighbour and
//! take the focus there, Shift extends; Home / End go to the ends; Ctrl /
//! Cmd + Up / Down move the item; Enter activates; Delete / Backspace delete.
//!
//! Key types: [`ThumbnailStrip`], [`ThumbnailItem`], [`ThumbnailStripEvent`].

use alloc::vec::Vec;

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{AttributeType, Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec},
    events::FocusEventFilter,
    refany::{OptionRefAny, RefAny},
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option, impl_option_inner, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    impl_vec_partialeq,
    props::{
        basic::length::FloatValue,
        layout::{
            LayoutAlignContent, LayoutAlignItems, LayoutBoxSizing, LayoutDisplay,
            LayoutFlexDirection, LayoutFlexShrink, LayoutFlexWrap, LayoutHeight, LayoutMinHeight,
            LayoutOverflow, LayoutPosition, LayoutWidth,
        },
        property::CssProperty,
        style::StyleCursor,
    },
    AzString,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::{
        roving::{self, Step},
        themes::{OptionUiTheme, UiTheme},
    },
};

/// The MIME type of a thumbnail drag: the dragged item's index as decimal text.
pub const DRAG_MIME: &str = "application/x-azul-thumbnail";

/// The strip's class.
pub(crate) const STRIP_CLASS: &str = "__azul-native-thumbnail-strip";
/// The strip's class in the grid layout (added to [`STRIP_CLASS`]).
pub(crate) const GRID_CLASS: &str = "__azul-native-thumbnail-strip-grid";
/// A section header.
pub(crate) const SECTION_CLASS: &str = "__azul-native-thumbnail-strip-section";
/// An item.
pub(crate) const ITEM_CLASS: &str = "__azul-native-thumbnail-strip-item";
/// A selected item (added to [`ITEM_CLASS`]).
pub(crate) const ITEM_SELECTED_CLASS: &str = "__azul-native-thumbnail-strip-item-selected";
/// A hidden item (added to [`ITEM_CLASS`]).
pub(crate) const ITEM_HIDDEN_CLASS: &str = "__azul-native-thumbnail-strip-item-hidden";
/// An item's number and badge.
pub(crate) const NUMBER_CLASS: &str = "__azul-native-thumbnail-strip-number";
/// An item's badge glyph.
pub(crate) const BADGE_CLASS: &str = "__azul-native-thumbnail-strip-badge";
/// The box that holds an item's preview.
pub(crate) const THUMB_CLASS: &str = "__azul-native-thumbnail-strip-thumb";

// ==== Types ====

/// A column (the slide rail) or a wrapping grid (the slide sorter).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ThumbnailStripLayout {
    #[default]
    Column,
    Grid,
}

/// One preview of the strip.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ThumbnailItem {
    /// The preview the app drew; the strip clips it to its box.
    pub content: Dom,
    /// The number shown beside (column) or under (grid) the preview.
    pub label: AzString,
    /// The accessible name ("Slide 3: Why local-first").
    pub name: AzString,
    /// A glyph under the number (a `Dom::create_icon` name), or empty.
    pub badge: AzString,
    /// Non-empty: a section titled so starts at this item.
    pub section: AzString,
    /// Selected.
    pub selected: bool,
    /// Hidden from the show: dimmed.
    pub hidden: bool,
    /// The section that starts here is folded: its items are not shown.
    pub section_collapsed: bool,
}

impl ThumbnailItem {
    /// An item showing `content`, numbered `label`, named `name`.
    #[must_use]
    pub fn create(content: Dom, label: AzString, name: AzString) -> Self {
        Self {
            content,
            label,
            name,
            badge: AzString::from_const_str(""),
            section: AzString::from_const_str(""),
            selected: false,
            hidden: false,
            section_collapsed: false,
        }
    }

    /// The badge glyph.
    #[must_use]
    pub fn with_badge(mut self, badge: AzString) -> Self {
        self.badge = badge;
        self
    }

    /// A section titled `title` starts at this item.
    #[must_use]
    pub fn with_section(mut self, title: AzString) -> Self {
        self.section = title;
        self
    }

    /// Selected or not.
    #[must_use]
    pub const fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Hidden (dimmed) or not.
    #[must_use]
    pub const fn with_hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    /// The section that starts here folded or not.
    #[must_use]
    pub const fn with_section_collapsed(mut self, collapsed: bool) -> Self {
        self.section_collapsed = collapsed;
        self
    }
}

impl_option!(
    ThumbnailItem,
    OptionThumbnailItem,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    ThumbnailItem,
    ThumbnailItemVec,
    ThumbnailItemVecDestructor,
    ThumbnailItemVecDestructorType,
    ThumbnailItemVecSlice,
    OptionThumbnailItem
);
impl_vec_clone!(ThumbnailItem, ThumbnailItemVec, ThumbnailItemVecDestructor);
impl_vec_debug!(ThumbnailItem, ThumbnailItemVec);
impl_vec_partialeq!(ThumbnailItem, ThumbnailItemVec);
impl_vec_mut!(ThumbnailItem, ThumbnailItemVec);

/// What happened in the strip.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ThumbnailStripEventKind {
    /// Item `index` was clicked or reached with an arrow key.
    Select,
    /// Item `index` was double-clicked or Enter was pressed on it.
    Activate,
    /// Move item `index` (and the other selected items) before the item at
    /// `target` (`target` = the count: to the end).
    Move,
    /// Delete / Backspace on item `index`.
    Delete,
    /// The section whose first item is `index` was folded or unfolded.
    SectionToggled,
}

/// One action in the strip.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThumbnailStripEvent {
    /// The item acted on.
    pub index: usize,
    /// `Move`: where it goes (before the item now at `target`).
    pub target: usize,
    /// What happened.
    pub kind: ThumbnailStripEventKind,
    /// `Select`: Shift was held (extend from the anchor).
    pub shift: bool,
    /// `Select`: Ctrl / Cmd was held (toggle).
    pub ctrl: bool,
}

impl ThumbnailStripEvent {
    /// A `kind` event on item `index`, no target, no modifiers.
    #[must_use]
    pub const fn create(kind: ThumbnailStripEventKind, index: usize) -> Self {
        Self {
            index,
            target: 0,
            kind,
            shift: false,
            ctrl: false,
        }
    }
}

/// Callback invoked for an action in the strip.
pub type ThumbnailStripOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, ThumbnailStripEvent) -> Update;
impl_widget_callback!(
    ThumbnailStripOnEvent,
    OptionThumbnailStripOnEvent,
    ThumbnailStripOnEventCallback,
    ThumbnailStripOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ThumbnailStripOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: THUMBNAIL_STRIP_ON_EVENT_INVOKER,
    invoker_ty:     AzThumbnailStripOnEventCallbackInvoker,
    thunk_fn:       az_thumbnail_strip_on_event_callback_thunk,
    setter_fn:      AzApp_setThumbnailStripOnEventCallbackInvoker,
    from_handle_fn: AzThumbnailStripOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzThumbnailStripOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: ThumbnailStripEvent ],
}

/// The strip of previews (module docs).
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ThumbnailStrip {
    /// The items, in order.
    pub items: ThumbnailItemVec,
    /// Every action in the strip.
    pub on_event: OptionThumbnailStripOnEvent,
    /// The strip's accessible name ("Slides").
    pub accessibility_name: AzString,
    /// The item holding the Tab stop (the current slide).
    pub active: usize,
    /// A preview box's width, px.
    pub thumb_width: f32,
    /// A preview box's height, px.
    pub thumb_height: f32,
    /// A column or a grid.
    pub layout: ThumbnailStripLayout,
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
}

impl ThumbnailStrip {
    /// A column of `items` with 160 x 90 px previews, the first active.
    #[must_use]
    pub fn create(items: ThumbnailItemVec) -> Self {
        Self {
            items,
            on_event: None.into(),
            accessibility_name: AzString::from_const_str("Thumbnails"),
            active: 0,
            thumb_width: 160.0,
            thumb_height: 90.0,
            layout: ThumbnailStripLayout::Column,
            theme: OptionUiTheme::None,
        }
    }

    /// Adds an item at the end.
    pub fn add_item(&mut self, item: ThumbnailItem) {
        let mut items = core::mem::replace(&mut self.items, ThumbnailItemVec::from_const_slice(&[]))
            .into_library_owned_vec();
        items.push(item);
        self.items = ThumbnailItemVec::from_vec(items);
    }

    /// [`Self::add_item`] for the builder chain.
    #[must_use]
    pub fn with_item(mut self, item: ThumbnailItem) -> Self {
        self.add_item(item);
        self
    }

    /// The item holding the Tab stop.
    pub const fn set_active(&mut self, active: usize) {
        self.active = active;
    }

    /// [`Self::set_active`] for the builder chain.
    #[must_use]
    pub const fn with_active(mut self, active: usize) -> Self {
        self.set_active(active);
        self
    }

    /// The preview box's size, px.
    pub fn set_thumb_size(&mut self, width: f32, height: f32) {
        self.thumb_width = width.max(1.0);
        self.thumb_height = height.max(1.0);
    }

    /// [`Self::set_thumb_size`] for the builder chain.
    #[must_use]
    pub fn with_thumb_size(mut self, width: f32, height: f32) -> Self {
        self.set_thumb_size(width, height);
        self
    }

    /// A column or a grid.
    pub const fn set_layout(&mut self, layout: ThumbnailStripLayout) {
        self.layout = layout;
    }

    /// [`Self::set_layout`] for the builder chain.
    #[must_use]
    pub const fn with_layout(mut self, layout: ThumbnailStripLayout) -> Self {
        self.set_layout(layout);
        self
    }

    /// The strip's accessible name.
    pub fn set_accessibility_name(&mut self, name: AzString) {
        self.accessibility_name = name;
    }

    /// [`Self::set_accessibility_name`] for the builder chain.
    #[must_use]
    pub fn with_accessibility_name(mut self, name: AzString) -> Self {
        self.set_accessibility_name(name);
        self
    }

    /// Every action in the strip.
    pub fn set_on_event<C: Into<ThumbnailStripOnEventCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_event = Some(ThumbnailStripOnEvent {
            refany: data,
            callback: cb.into(),
        })
        .into();
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<ThumbnailStripOnEventCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_on_event(data, cb);
        self
    }

    /// Pin the widget theme; unset, the strip follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty strip and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    /// The strip's DOM, in the pinned theme's look or both looks merged
    /// (the previews are built once).
    #[must_use]
    pub fn dom(self) -> Dom {
        let look = look_for(self.theme);
        build(self, &look)
    }
}

impl Default for ThumbnailStrip {
    fn default() -> Self {
        Self::create(ThumbnailItemVec::from_const_slice(&[]))
    }
}

impl From<ThumbnailStrip> for Dom {
    fn from(s: ThumbnailStrip) -> Self {
        s.dom()
    }
}

/// Where a dragged item goes when it is dropped on item `on`: after it when
/// it came from before it, before it otherwise (`target` of a `Move`).
#[must_use]
pub const fn drop_target(from: usize, on: usize) -> usize {
    if from < on {
        on + 1
    } else {
        on
    }
}

// ==== The look and the DOM ====

/// What a theme decides about a strip: the SKIN of each part, laid over the
/// part's base (`THUMBNAIL_*_BASE`) by [`build`].
#[derive(Debug, Clone, Default)]
pub(crate) struct ThumbnailStripLook {
    /// The strip (its ground, its padding).
    pub strip: Vec<CssPropertyWithConditions>,
    /// A section header.
    pub section: Vec<CssPropertyWithConditions>,
    /// A section header's chevron.
    pub section_icon: Vec<CssPropertyWithConditions>,
    /// An item (its spacing, its focus ring).
    pub item: Vec<CssPropertyWithConditions>,
    /// Added to a selected item.
    pub item_selected: Vec<CssPropertyWithConditions>,
    /// An item's number column.
    pub number: Vec<CssPropertyWithConditions>,
    /// An item's badge glyph.
    pub badge: Vec<CssPropertyWithConditions>,
    /// The preview box.
    pub thumb: Vec<CssPropertyWithConditions>,
    /// Added to a selected item's preview box.
    pub thumb_selected: Vec<CssPropertyWithConditions>,
    /// Added to a hidden item's preview box.
    pub thumb_hidden: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the strip, if it has one.
    pub marker: Option<&'static str>,
}

/// The look a strip with the theme option `theme` is built with.
pub(crate) fn look_for(theme: OptionUiTheme) -> ThumbnailStripLook {
    let _ = theme;
    ThumbnailStripLook::default()
}

/// The strip's DOM in `look`.
pub(crate) fn build(strip: ThumbnailStrip, look: &ThumbnailStripLook) -> Dom {
    let _ = (strip, look);
    Dom::create_div()
}

#[cfg(test)]
mod thumbnail_strip_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        a11y::{AccessibilityRole, AccessibilityState},
        dom::{DomId, DomNodeId, NodeId, NodeType, TabIndex},
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::VirtualKeyCode as K,
    };

    use super::*;
    use crate::{
        callbacks::CallbackChange,
        widgets::{
            roving::test_support as rv,
            themes::{theme_blocks::checks, theme_checks},
        },
    };

    type Log = Arc<Mutex<Vec<String>>>;

    extern "C" fn record(mut data: RefAny, _: CallbackInfo, e: ThumbnailStripEvent) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(format!(
                "{:?} {} {}{}{}",
                e.kind,
                e.index,
                e.target,
                if e.shift { " shift" } else { "" },
                if e.ctrl { " ctrl" } else { "" }
            ));
        }
        Update::RefreshDom
    }

    fn log() -> Log {
        Arc::new(Mutex::new(Vec::new()))
    }

    fn preview(n: usize) -> Dom {
        Dom::create_div().with_class(AzString::from(format!("preview-{n}")))
    }

    fn item(n: usize) -> ThumbnailItem {
        ThumbnailItem::create(
            preview(n),
            AzString::from(format!("{}", n + 1)),
            AzString::from(format!("Slide {}", n + 1)),
        )
    }

    /// Four slides: a section "Intro" at the first, slide 2 selected, slide 3
    /// hidden with a badge, slide 2 active.
    fn strip(log: &Log) -> ThumbnailStrip {
        ThumbnailStrip::create(ThumbnailItemVec::from_vec(vec![
            item(0).with_section(AzString::from("Intro")),
            item(1).with_selected(true),
            item(2).with_hidden(true).with_badge(AzString::from("star")),
            item(3),
        ]))
        .with_active(1)
        .with_thumb_size(160.0, 90.0)
        .with_accessibility_name(AzString::from("Slides"))
        .with_on_event(RefAny::new(log.clone()), record as ThumbnailStripOnEventCallbackType)
    }

    fn texts(node: &Dom, out: &mut Vec<String>) {
        if let NodeType::Text(s) = node.root.get_node_type() {
            if !s.as_ref().as_str().is_empty() {
                out.push(s.as_ref().as_str().to_string());
            }
        }
        for c in node.children.as_ref() {
            texts(c, out);
        }
    }

    fn id(n: NodeId) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(n)),
        }
    }

    /// The strip's direct children's node ids, in order.
    fn children(styled: &StyledDom) -> Vec<NodeId> {
        let hierarchy = styled.node_hierarchy.as_ref();
        let mut out = Vec::new();
        let mut next = hierarchy[0].first_child_id(NodeId::new(0));
        while let Some(n) = next {
            out.push(n);
            next = hierarchy[n.index()].next_sibling_id();
        }
        out
    }

    fn sized(w: f32, h: f32) -> [CssProperty; 2] {
        [
            CssProperty::const_width(LayoutWidth::px(w)),
            CssProperty::const_height(LayoutHeight::px(h)),
        ]
    }

    #[test]
    fn a_column_shows_each_preview_in_its_box_beside_its_number_after_its_section_header() {
        let log = log();
        for theme in checks::BOTH {
            let dom = strip(&log).with_theme(theme).dom();
            let kids = dom.children.as_ref();
            assert_eq!(kids.len(), 5, "{}: the header and four items", theme.name());
            assert!(theme_checks::has_class(&kids[0], SECTION_CLASS));
            let mut header = Vec::new();
            texts(&kids[0], &mut header);
            assert_eq!(header, vec![String::from("Intro")]);
            for (n, kid) in kids[1..].iter().enumerate() {
                assert!(theme_checks::has_class(kid, ITEM_CLASS), "{}", theme.name());
                let number = theme_checks::find(kid, NUMBER_CLASS).expect("the number");
                let mut label = Vec::new();
                texts(number, &mut label);
                assert_eq!(label, vec![format!("{}", n + 1)]);
                let thumb = theme_checks::find(kid, THUMB_CLASS).expect("the preview box");
                let props = checks::live_properties(thumb);
                for want in sized(160.0, 90.0) {
                    assert!(props.contains(&want), "{}: {want:?} in {props:?}", theme.name());
                }
                assert!(
                    theme_checks::find(thumb, &format!("preview-{n}")).is_some(),
                    "the app's preview sits in the box"
                );
            }
            assert!(theme_checks::find(&kids[3], BADGE_CLASS).is_some(), "slide 3 shows its badge");
            assert!(!theme_checks::has_class(&dom, GRID_CLASS));
        }
    }

    #[test]
    fn a_grid_wraps_its_items() {
        let log = log();
        let dom = strip(&log)
            .with_layout(ThumbnailStripLayout::Grid)
            .with_theme(UiTheme::Flat)
            .dom();
        assert!(theme_checks::has_class(&dom, GRID_CLASS));
        assert!(checks::live_properties(&dom)
            .contains(&CssProperty::const_flex_wrap(LayoutFlexWrap::Wrap)));
    }

    #[test]
    fn a_folded_section_hides_its_items_until_the_next_section() {
        let log = log();
        let dom = ThumbnailStrip::create(ThumbnailItemVec::from_vec(vec![
            item(0).with_section(AzString::from("A")).with_section_collapsed(true),
            item(1),
            item(2).with_section(AzString::from("B")),
            item(3),
        ]))
        .with_on_event(RefAny::new(log.clone()), record as ThumbnailStripOnEventCallbackType)
        .with_theme(UiTheme::Flat)
        .dom();
        let kids = dom.children.as_ref();
        assert_eq!(kids.len(), 4, "header A, header B, slides 3 and 4");
        assert!(theme_checks::has_class(&kids[0], SECTION_CLASS));
        assert!(theme_checks::has_class(&kids[1], SECTION_CLASS));
        assert!(theme_checks::find(&kids[2], "preview-2").is_some());
        let state = kids[0]
            .root
            .get_accessibility_info()
            .map(|i| i.states.as_ref().to_vec())
            .unwrap_or_default();
        assert!(state.contains(&AccessibilityState::Collapsed), "{state:?}");
    }

    #[test]
    fn selected_and_hidden_items_are_marked_and_announced() {
        let log = log();
        let dom = strip(&log).with_theme(UiTheme::Flat).dom();
        let info = dom.root.get_accessibility_info().expect("a11y");
        assert_eq!(info.role, AccessibilityRole::List);
        assert_eq!(info.accessibility_name.as_ref().map(|n| n.as_str().to_string()), Some(String::from("Slides")));
        let kids = dom.children.as_ref();
        assert!(theme_checks::has_class(&kids[2], ITEM_SELECTED_CLASS));
        assert!(theme_checks::has_class(&kids[3], ITEM_HIDDEN_CLASS));
        let item = kids[2].root.get_accessibility_info().expect("a11y");
        assert_eq!(item.role, AccessibilityRole::ListItem);
        assert_eq!(item.accessibility_name.as_ref().map(|n| n.as_str().to_string()), Some(String::from("Slide 2")));
        assert!(item.states.as_ref().contains(&AccessibilityState::Selected));
        assert_ne!(kids[1].root.get_style(), kids[2].root.get_style(), "the selection shows");
    }

    #[test]
    fn the_active_item_is_the_one_tab_stop_and_the_keys_select_move_activate_and_delete() {
        let log = log();
        let styled = StyledDom::create_from_dom(strip(&log).with_theme(UiTheme::Flat).dom());
        let kids = children(&styled);
        let items = &kids[1..];
        let stops: Vec<bool> = items
            .iter()
            .map(|n| styled.node_data.as_ref()[n.index()].get_tab_index() == Some(TabIndex::Auto))
            .collect();
        assert_eq!(stops, vec![false, true, false, false]);

        let (_, changes) = rv::press(&styled, id(items[1]), K::Down, &[]).expect("a key handler");
        assert_eq!(rv::focus_request(&changes), Some(id(items[2])));
        assert!(rv::prevented(&changes));
        rv::press(&styled, id(items[1]), K::Down, &[K::LShift]);
        rv::press(&styled, id(items[1]), K::Down, &[K::LControl]);
        rv::press(&styled, id(items[1]), K::Up, &[K::LControl]);
        rv::press(&styled, id(items[1]), K::Return, &[]);
        rv::press(&styled, id(items[1]), K::Delete, &[]);
        rv::press(&styled, id(items[1]), K::Home, &[]);
        let (_, changes) = rv::press(&styled, id(items[0]), K::Up, &[]).expect("a key handler");
        assert_eq!(rv::focus_request(&changes), None, "the first item holds at the top");
        assert_eq!(
            *log.lock().expect("log"),
            vec![
                "Select 2 0",
                "Select 2 0 shift",
                "Move 1 3",
                "Move 1 0",
                "Activate 1 0",
                "Delete 1 0",
                "Select 0 0",
            ]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_click_selects_a_double_click_activates_and_a_header_click_folds_its_section() {
        let log = log();
        let styled = StyledDom::create_from_dom(strip(&log).with_theme(UiTheme::Flat).dom());
        let kids = children(&styled);
        rv::fire(&styled, id(kids[3]), EventFilter::Hover(HoverEventFilter::Click)).expect("a click target");
        rv::fire(&styled, id(kids[3]), EventFilter::Hover(HoverEventFilter::DoubleClick)).expect("a double-click target");
        rv::fire(&styled, id(kids[0]), EventFilter::Hover(HoverEventFilter::Click)).expect("the header");
        assert_eq!(
            *log.lock().expect("log"),
            vec!["Select 2 0", "Activate 2 0", "SectionToggled 0 0"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_drag_onto_another_item_moves_it_there() {
        let log = log();
        let styled = StyledDom::create_from_dom(strip(&log).with_theme(UiTheme::Flat).dom());
        let kids = children(&styled);
        let items = &kids[1..];
        assert!(
            styled.node_data.as_ref()[items[0].index()]
                .attributes()
                .as_ref()
                .contains(&AttributeType::Draggable(true)),
            "an item is draggable"
        );
        let (_, changes) = rv::fire(&styled, id(items[0]), EventFilter::Hover(HoverEventFilter::DragStart))
            .expect("a drag source");
        assert!(changes.iter().any(|c| matches!(
            c,
            CallbackChange::SetDragData { mime_type, data }
                if mime_type.as_str() == DRAG_MIME && data.as_slice() == b"0"
        )));
        let (_, changes) = rv::fire(&styled, id(items[2]), EventFilter::Hover(HoverEventFilter::DragOver))
            .expect("a drop target");
        assert!(changes.iter().any(|c| matches!(c, CallbackChange::AcceptDrop)));
        rv::fire(&styled, id(items[2]), EventFilter::Hover(HoverEventFilter::Drop)).expect("a drop target");
        rv::fire(&styled, id(items[3]), EventFilter::Hover(HoverEventFilter::DragStart));
        rv::fire(&styled, id(items[1]), EventFilter::Hover(HoverEventFilter::Drop));
        assert_eq!(
            *log.lock().expect("log"),
            vec!["Move 0 3", "Move 3 1"].into_iter().map(String::from).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_drop_after_the_dragged_item_lands_after_and_before_it_lands_before() {
        assert_eq!(drop_target(0, 2), 3);
        assert_eq!(drop_target(3, 1), 1);
        assert_eq!(drop_target(2, 2), 2);
    }

    #[test]
    fn a_strip_without_a_theme_follows_the_app_theme_and_declares_its_structure_once() {
        let log = log();
        for layout in [ThumbnailStripLayout::Column, ThumbnailStripLayout::Grid] {
            checks::assert_follows_the_app_theme(
                "thumbnail_strip",
                || strip(&log).with_layout(layout).dom(),
                |t: UiTheme| strip(&log).with_layout(layout).with_theme(t).dom(),
            );
            for theme in checks::BOTH {
                let dom = checks::under(theme, || strip(&log).with_layout(layout).dom());
                theme_checks::assert_structure_is_shared(
                    &format!("thumbnail_strip built for {}", theme.name()),
                    &dom,
                    &[],
                );
                theme_checks::assert_theme_invariants(&format!("thumbnail_strip ({})", theme.name()), &dom);
            }
        }
    }
}
