//! The quick-action SHEET: a phone's overlay list of icon + label rows under a grab handle, the
//! bottom sheet of picture #8 (a file's actions: Preview, Download, Rename, Move, See versions,
//! "Move to trash" in red).
//!
//! ```text
//!        ▬▬▬                      the grab handle: a tap closes the sheet
//!  report_2026.pdf                the subject, bold, and a line under it
//!  last modified 18 Jun
//!  ─────────────────────────────
//!  ◉  Preview
//!  ⬇  Download
//!  ✎  Rename
//!  ⇄  Move
//!  ⏲  See versions
//!  🗑  Move to trash       (red)
//! ```
//!
//! It is the phone's own edge: a long press on a row opens it (the sidebar reports
//! `RowLongPressed`, a list reports the press), and it is NOT the Inspector's pane (which is a
//! split the user resizes). It lies over the frame it is a child of - put it as the last child of
//! the app frame, which is `position: relative` - over a light scrim that closes it when tapped,
//! and Escape closes it too. On a desktop it draws nothing (a desktop has the context menu), unless
//! the form factor is named ([`ActionSheet::with_form`]).
//!
//! The sheet owns nothing: the app keeps `open`, hears the chosen row or the dismissal through ONE
//! callback ([`ActionSheetOnEvent`]) and rebuilds.
//!
//! Key types: [`ActionSheet`], [`ActionSheetItem`], [`ActionSheetEvent`], [`ActionSheetOnEvent`].

use alloc::{format, vec::Vec};

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    callbacks::{CoreCallback, CoreCallbackData, FocusTarget, Update},
    dom::{ComponentEventFilter, Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClassVec, TabIndex},
    events::FocusEventFilter,
    refany::{OptionRefAny, RefAny},
    window::VirtualKeyCode,
};
use azul_css::{
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut, impl_vec_partialeq, AzString,
};

use super::{
    sidebar::SidebarSurface,
    sidebar_layout::FormFactor,
    themes::{OptionUiTheme, UiTheme},
    widget_p_with_text,
};
use crate::callbacks::CallbackInfo;

/// The sheet's root: the scrim and the sheet.
pub const SHEET_CLASS: &str = "__azul-native-action-sheet";
/// The scrim under the sheet.
pub const SHEET_SCRIM_CLASS: &str = "__azul-native-action-sheet-scrim";
/// The sheet itself.
pub const SHEET_PANEL_CLASS: &str = "__azul-native-action-sheet-panel";
/// The grab handle.
pub const SHEET_HANDLE_CLASS: &str = "__azul-native-action-sheet-handle";
/// The subject and its line.
pub const SHEET_HEADER_CLASS: &str = "__azul-native-action-sheet-header";
/// One row.
pub const SHEET_ITEM_CLASS: &str = "__azul-native-action-sheet-item";
/// Added to a row that cannot be undone ("Move to trash").
pub const SHEET_DANGER_CLASS: &str = "__azul-native-action-sheet-danger";

/// The most rows a sheet shows; the rest is dropped with a diagnostic (a sheet is quick actions).
pub const MAX_ITEMS: usize = 24;
/// A row's height, px: a finger's target.
pub const ITEM_HEIGHT: f32 = 48.0;

/// One quick action.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ActionSheetItem {
    /// The label ("Rename").
    pub label: AzString,
    /// The icon, a Material name; empty for none.
    pub icon: AzString,
    /// What the app calls this action: handed back when it is chosen.
    pub tag: usize,
    /// A destructive action: its label and icon are red (`system:danger`).
    pub danger: bool,
    /// Why the action cannot run now; empty: it can. A disabled row is dimmed and runs nothing.
    pub disabled_reason: AzString,
}

impl_option!(ActionSheetItem, OptionActionSheetItem, copy = false, [Debug, Clone, PartialEq]);
impl_vec!(
    ActionSheetItem,
    ActionSheetItemVec,
    ActionSheetItemVecDestructor,
    ActionSheetItemVecDestructorType,
    ActionSheetItemVecSlice,
    OptionActionSheetItem
);
impl_vec_clone!(ActionSheetItem, ActionSheetItemVec, ActionSheetItemVecDestructor);
impl_vec_debug!(ActionSheetItem, ActionSheetItemVec);
impl_vec_partialeq!(ActionSheetItem, ActionSheetItemVec);
impl_vec_mut!(ActionSheetItem, ActionSheetItemVec);

impl ActionSheetItem {
    /// An action `label` with `icon`, called `tag` by the app.
    #[must_use]
    pub const fn create(label: AzString, icon: AzString, tag: usize) -> Self {
        Self {
            label,
            icon,
            tag,
            danger: false,
            disabled_reason: AzString::from_const_str(""),
        }
    }

    /// Marks the action destructive.
    pub const fn set_danger(&mut self, danger: bool) {
        self.danger = danger;
    }

    /// [`Self::set_danger`] for the builder chain.
    #[must_use]
    pub const fn with_danger(mut self, danger: bool) -> Self {
        self.set_danger(danger);
        self
    }

    /// Why the action cannot run now.
    pub fn set_disabled_reason(&mut self, reason: AzString) {
        self.disabled_reason = reason;
    }

    /// [`Self::set_disabled_reason`] for the builder chain.
    #[must_use]
    pub fn with_disabled_reason(mut self, reason: AzString) -> Self {
        self.set_disabled_reason(reason);
        self
    }
}

/// What happened on the sheet.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum ActionSheetEventKind {
    /// A row was chosen: `index` and `tag` name it. The app closes the sheet and runs it.
    ItemChosen,
    /// The scrim, the grab handle or Escape: the app closes the sheet.
    Dismissed,
}

/// One report of the sheet.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct ActionSheetEvent {
    /// What happened.
    pub kind: ActionSheetEventKind,
    /// The row's place (`ItemChosen`); 0 otherwise.
    pub index: usize,
    /// The row's tag; 0 otherwise.
    pub tag: usize,
}

/// Callback invoked for the chosen row and for the dismissal.
pub type ActionSheetOnEventCallbackType = extern "C" fn(RefAny, CallbackInfo, ActionSheetEvent) -> Update;
impl_widget_callback!(
    ActionSheetOnEvent,
    OptionActionSheetOnEvent,
    ActionSheetOnEventCallback,
    ActionSheetOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ActionSheetOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: ACTION_SHEET_ON_EVENT_INVOKER,
    invoker_ty:     AzActionSheetOnEventCallbackInvoker,
    thunk_fn:       az_action_sheet_on_event_callback_thunk,
    setter_fn:      AzApp_setActionSheetOnEventCallbackInvoker,
    from_handle_fn: AzActionSheetOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzActionSheetOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: ActionSheetEvent ],
}

/// A phone's bottom sheet of quick actions.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ActionSheet {
    /// The subject ("`report_2026.pdf`"); empty for a sheet of rows only.
    pub title: AzString,
    /// The line under the subject ("last modified 18 Jun"); empty for none.
    pub subtitle: AzString,
    /// The rows, top to bottom.
    pub items: ActionSheetItemVec,
    /// Whether the sheet shows.
    pub open: bool,
    /// Desktop or phone: `Auto` asks the window. A desktop draws nothing.
    pub form: FormFactor,
    /// What the sheet is painted with.
    pub surface: SidebarSurface,
    /// The chosen row and the dismissal.
    pub on_event: OptionActionSheetOnEvent,
    /// The widget theme this sheet is PINNED to, or `None` to follow the app theme.
    pub theme: OptionUiTheme,
}

impl ActionSheet {
    /// A closed sheet with no rows.
    #[must_use]
    pub const fn create() -> Self {
        Self {
            title: AzString::from_const_str(""),
            subtitle: AzString::from_const_str(""),
            items: ActionSheetItemVec::from_const_slice(&[]),
            open: false,
            form: FormFactor::Auto,
            surface: SidebarSurface::Theme,
            on_event: OptionActionSheetOnEvent::None,
            theme: OptionUiTheme::None,
        }
    }

    /// The subject and the line under it.
    pub fn set_title(&mut self, title: AzString, subtitle: AzString) {
        self.title = title;
        self.subtitle = subtitle;
    }

    /// [`Self::set_title`] for the builder chain.
    #[must_use]
    pub fn with_title(mut self, title: AzString, subtitle: AzString) -> Self {
        self.set_title(title, subtitle);
        self
    }

    /// Appends a row.
    pub fn add_item(&mut self, item: ActionSheetItem) {
        let mut v = self.items.clone().into_library_owned_vec();
        v.push(item);
        self.items = ActionSheetItemVec::from_vec(v);
    }

    /// [`Self::add_item`] for the builder chain.
    #[must_use]
    pub fn with_item(mut self, item: ActionSheetItem) -> Self {
        self.add_item(item);
        self
    }

    /// Whether the sheet shows.
    pub const fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    /// [`Self::set_open`] for the builder chain.
    #[must_use]
    pub const fn with_open(mut self, open: bool) -> Self {
        self.set_open(open);
        self
    }

    /// Desktop or phone: `Auto` (the default) asks the window.
    pub const fn set_form(&mut self, form: FormFactor) {
        self.form = form;
    }

    /// [`Self::set_form`] for the builder chain.
    #[must_use]
    pub const fn with_form(mut self, form: FormFactor) -> Self {
        self.set_form(form);
        self
    }

    /// What the sheet is painted with.
    pub const fn set_surface(&mut self, surface: SidebarSurface) {
        self.surface = surface;
    }

    /// [`Self::set_surface`] for the builder chain.
    #[must_use]
    pub const fn with_surface(mut self, surface: SidebarSurface) -> Self {
        self.set_surface(surface);
        self
    }

    /// Pin the widget theme; unset, the sheet follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// The chosen row and the dismissal.
    pub fn set_on_event<C: Into<ActionSheetOnEventCallback>>(&mut self, data: RefAny, on_event: C) {
        self.on_event = OptionActionSheetOnEvent::Some(ActionSheetOnEvent {
            refany: data,
            callback: on_event.into(),
        });
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<ActionSheetOnEventCallback>>(mut self, data: RefAny, on_event: C) -> Self {
        self.set_on_event(data, on_event);
        self
    }

    /// Whether the sheet draws anything: open, on a phone.
    #[must_use]
    pub fn shows(&self) -> bool {
        self.open && self.form.is_phone()
    }

    /// The sheet's DOM: a node that fills the nearest positioned ancestor (the frame) with the
    /// scrim and the sheet at its bottom, or a node with no size when it shows nothing.
    #[must_use]
    pub fn dom(self) -> Dom {
        build(self)
    }
}

impl Default for ActionSheet {
    fn default() -> Self {
        Self::create()
    }
}

impl From<ActionSheet> for Dom {
    fn from(s: ActionSheet) -> Self {
        s.dom()
    }
}

// ---------------------------------------------------------------------------
// The report
// ---------------------------------------------------------------------------

struct PartRef {
    on_event: OptionActionSheetOnEvent,
    event: ActionSheetEvent,
}

extern "C" fn on_part_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(part) = data.downcast_ref::<PartRef>() else {
        return Update::DoNothing;
    };
    match part.on_event.as_ref() {
        Some(ActionSheetOnEvent { callback, refany }) => callback.invoke(refany.clone(), info, part.event),
        None => Update::DoNothing,
    }
}

/// Escape closes the sheet.
extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let key = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    if key != Some(VirtualKeyCode::Escape) {
        return Update::DoNothing;
    }
    info.prevent_default();
    on_part_click(data, info)
}

/// The sheet appeared: its first row takes the keyboard (the wrapper hands the focus to its first
/// focusable node), so Escape and the arrow keys are heard.
extern "C" fn on_mounted(_data: RefAny, mut info: CallbackInfo) -> Update {
    let node = info.get_hit_node();
    info.set_focus(FocusTarget::Id(node));
    Update::DoNothing
}

// ---------------------------------------------------------------------------
// The build
// ---------------------------------------------------------------------------

fn classes(list: &[&'static str]) -> IdOrClassVec {
    IdOrClassVec::from_vec(
        list.iter()
            .map(|c| IdOrClass::Class(AzString::from_const_str(c)))
            .collect::<Vec<_>>(),
    )
}

fn styled(mut dom: Dom, css: &str) -> Dom {
    dom.root.set_css(css);
    dom
}

fn cb(filter: EventFilter, data: RefAny, f: extern "C" fn(RefAny, CallbackInfo) -> Update) -> CoreCallbackData {
    CoreCallbackData {
        event: filter,
        callback: CoreCallback {
            cb: f as usize,
            ctx: OptionRefAny::None,
        },
        refany: data,
    }
}

fn build(sheet: ActionSheet) -> Dom {
    if !sheet.shows() {
        return styled(Dom::create_div(), "position: absolute; left: 0px; top: 0px; width: 0px; height: 0px; overflow: hidden;");
    }
    let ActionSheet {
        title,
        subtitle,
        items,
        surface,
        on_event,
        ..
    } = sheet;
    let mut items = items.into_library_owned_vec();
    if items.len() > MAX_ITEMS {
        let extra = items.len() - MAX_ITEMS;
        items.truncate(MAX_ITEMS);
        azul_core::diagnostics::emit_once(0x5ee7_0001, || {
            format!("ActionSheet: {extra} row(s) past the limit of {MAX_ITEMS} were dropped")
        });
    }
    let dismiss = |on_event: &OptionActionSheetOnEvent| {
        RefAny::new(PartRef {
            on_event: on_event.clone(),
            event: ActionSheetEvent {
                kind: ActionSheetEventKind::Dismissed,
                index: 0,
                tag: 0,
            },
        })
    };

    let scrim = styled(
        Dom::create_div(),
        "position: absolute; left: 0px; top: 0px; right: 0px; bottom: 0px; background: rgba(0, 0, 0, 0.28);",
    )
    .with_ids_and_classes(classes(&[SHEET_SCRIM_CLASS]))
    .with_callbacks(
        alloc::vec![cb(EventFilter::Hover(HoverEventFilter::Click), dismiss(&on_event), on_part_click)].into(),
    );

    let handle = styled(
        Dom::create_div().with_child(styled(
            Dom::create_div(),
            "width: 38px; height: 4px; border-radius: 2px; background: currentColor; opacity: 0.4;",
        )),
        "display: flex; align-items: center; justify-content: center; flex-shrink: 0; height: 24px; width: 100%; cursor: pointer;",
    )
    .with_ids_and_classes(classes(&[SHEET_HANDLE_CLASS]))
    .with_accessibility_info(AccessibilityInfo::named("Close", AccessibilityRole::PushButton))
    .with_callbacks(
        alloc::vec![cb(EventFilter::Hover(HoverEventFilter::Click), dismiss(&on_event), on_part_click)].into(),
    );

    let mut kids: Vec<Dom> = alloc::vec![handle];
    if !title.as_str().is_empty() {
        let mut lines: Vec<Dom> = alloc::vec![styled(
            widget_p_with_text(title.clone()),
            "margin: 0px; font-size: 15px; font-weight: 700; overflow: hidden; white-space: nowrap; text-overflow: ellipsis;"
        )];
        if !subtitle.as_str().is_empty() {
            lines.push(styled(
                widget_p_with_text(subtitle),
                "margin: 0px; font-size: 12px; opacity: 0.7; overflow: hidden; white-space: nowrap; text-overflow: ellipsis;",
            ));
        }
        kids.push(
            styled(
                Dom::create_div().with_children(DomVec::from_vec(lines)),
                "display: flex; flex-direction: column; gap: 2px; flex-shrink: 0; padding: 4px 16px 10px 16px; \
                 border-bottom: 1px solid rgba(127, 127, 127, 0.28);",
            )
            .with_ids_and_classes(classes(&[SHEET_HEADER_CLASS])),
        );
    }
    for (index, item) in items.iter().enumerate() {
        let disabled = !item.disabled_reason.as_str().is_empty();
        let color = if item.danger { "color: system:danger;" } else { "" };
        let mut row_kids: Vec<Dom> = Vec::with_capacity(2);
        if !item.icon.as_str().is_empty() {
            row_kids.push(styled(
                Dom::create_icon(item.icon.clone()),
                "font-size: 22px; width: 24px; flex-shrink: 0; text-align: center;",
            ));
        }
        row_kids.push(styled(
            widget_p_with_text(item.label.clone()),
            "flex-grow: 1; min-width: 0px; margin: 0px; font-size: 15px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis;",
        ));
        let mut row = styled(
            Dom::create_div().with_children(DomVec::from_vec(row_kids)),
            &format!(
                "display: flex; flex-direction: row; align-items: center; gap: 16px; flex-shrink: 0; \
                 box-sizing: border-box; height: {ITEM_HEIGHT}px; padding: 0px 16px; cursor: pointer; {color} {} \
                 :active {{ background: rgba(127, 127, 127, 0.20); }} \
                 :focus-visible {{ box-shadow: inset 0px 0px 0px 2px #4a8be8; }}",
                if disabled { "opacity: 0.45;" } else { "" }
            ),
        )
        .with_ids_and_classes(IdOrClassVec::from_vec(if item.danger {
            alloc::vec![
                IdOrClass::Class(AzString::from_const_str(SHEET_ITEM_CLASS)),
                IdOrClass::Class(AzString::from_const_str(SHEET_DANGER_CLASS)),
            ]
        } else {
            alloc::vec![IdOrClass::Class(AzString::from_const_str(SHEET_ITEM_CLASS))]
        }))
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::PushButton,
            accessibility_name: azul_css::OptionString::Some(item.label.clone()),
            description: if disabled {
                azul_css::OptionString::Some(item.disabled_reason.clone())
            } else {
                azul_css::OptionString::None
            },
            ..Default::default()
        })
        .with_tab_index(TabIndex::Auto);
        if !disabled {
            row = row.with_callbacks(
                alloc::vec![cb(
                    EventFilter::Hover(HoverEventFilter::Click),
                    RefAny::new(PartRef {
                        on_event: on_event.clone(),
                        event: ActionSheetEvent {
                            kind: ActionSheetEventKind::ItemChosen,
                            index,
                            tag: item.tag,
                        },
                    }),
                    on_part_click,
                )]
                .into(),
            );
        }
        kids.push(row);
    }

    let panel = styled(
        Dom::create_div().with_children(DomVec::from_vec(kids)),
        &format!(
            "display: flex; flex-direction: column; position: relative; box-sizing: border-box; width: 100%; \
             max-height: 80%; overflow-y: auto; padding: 0px 0px 12px 0px; border-radius: 14px 14px 0px 0px; \
             box-shadow: 0px -6px 18px rgba(0, 0, 0, 0.18); {}",
            match surface {
                SidebarSurface::Theme => "background: system:window-background; color: system:text;",
                other => other.root_css(),
            }
        ),
    )
    .with_ids_and_classes(classes(&[SHEET_PANEL_CLASS]))
    .with_accessibility_info(AccessibilityInfo::named(
        if title.as_str().is_empty() { "Actions" } else { title.as_str() },
        AccessibilityRole::Dialog,
    ));

    styled(
        Dom::create_div().with_children(DomVec::from_vec(alloc::vec![scrim, panel])),
        "position: absolute; left: 0px; top: 0px; right: 0px; bottom: 0px; z-index: 50; display: flex; \
         flex-direction: column; justify-content: flex-end; overflow: hidden;",
    )
    .with_ids_and_classes(classes(&[SHEET_CLASS]))
    .with_callbacks(
        alloc::vec![
            cb(EventFilter::Focus(FocusEventFilter::VirtualKeyDown), dismiss(&on_event), on_key),
            cb(EventFilter::Component(ComponentEventFilter::AfterMount), RefAny::new(()), on_mounted),
        ]
        .into(),
    )
    .with_tab_index(TabIndex::Auto)
}

#[cfg(test)]
#[path = "action_sheet_test.rs"]
mod action_sheet_test;
