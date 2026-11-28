//! AzMaps' window: the map fills it, and everything else floats on it.
//!
//! ```text
//!  a window from 560 px: the panel at the left       a phone upright: a search bar, chips, the sheet
//! +--------------------------------------------+    +-------------------------+
//! | +-panel-----------+                  [+][-] |    | ( search places   [x][mic] [gear] )
//! | | ( search  [<] ) |                  [ o ]  |    | [Food] [Coffee] [Fuel] [Parking]  |
//! | | [Food] [Fuel]   |                  [ = ]  |    |                         [ o ]     |
//! | | results, or the |                         |    |      the map            [ = ]     |
//! | | place, or the   |   the map               |    |  +-sheet---------------------+   |
//! | | way             |                         |    |  |  ---  name, [Route] [Save] ...  |
//! | +-----------------+                         |    |  +---------------------------+   |
//! +--------------------------------------------+    +-------------------------+
//! ```
//!
//! No ribbon, no tab row, no title bar of its own: the map is the window. What floats on it is
//! the same three views everywhere ([`crate::flow::Panel`]): the search (the field, the chips, the
//! results, then the recent and the saved places), one place (its name, Route, Save, Share) and the
//! way (both ends, the modes, the summary, the steps). A desktop holds them in the panel, which folds
//! away to its search key; a phone holds the search bar and the chips on top of the map and the views
//! in a sheet dragged between a peek, half the screen and the full height. The zoom, locate and map
//! style keys float at the right of the map in both.

use azul::{
    callbacks::{
        BottomSheetOnEventCallbackType, ButtonOnClickCallbackType, TextInputOnTextInputCallbackType,
        TextInputOnVirtualKeyDownCallbackType,
    },
    prelude::*,
    str::String as AzString,
    widgets::{
        BottomSheet, BottomSheetEvent, BottomSheetEventKind, Button, ButtonType, Dialog, Popover, SheetDetent,
        TextInput,
    },
};
use azul_appkit::{
    insets::Insets,
    l10n::label,
    ui as kit,
};

use crate::{
    flow,
    flow::{Panel, PlaceCard},
    ids, lists_ui, MapState,
};

/// The panel's width, px.
pub const PANEL_WIDTH: f32 = 340.0;
/// The panel's left offset, px.
const PANEL_LEFT: f32 = 10.0;
/// What the panel covers of the map's left edge, for the places that are framed beside it (its offset, its width, a gap).
pub const PANEL_COVER: f32 = PANEL_LEFT + PANEL_WIDTH + 12.0;
/// A phone's search bar: its height and its gaps, px; what the sheet leaves above itself at the full height.
const BAR_HEIGHT: f32 = 56.0;
const BAR_GAP: f32 = 8.0;
/// The sheet's peek, px: its grip, a place's name and its keys.
pub const PEEK_HEIGHT: f32 = 168.0;
/// The room above a phone's sheet at the full height, px: the search bar stays in sight.
const TOP_ROOM: f32 = BAR_HEIGHT + 2.0 * BAR_GAP;
/// A key's side on a phone, px (a finger's target).
const KEY: f32 = 48.0;
/// The gap between a floating key group and the sheet's top edge, px.
const ABOVE_SHEET: f32 = 16.0;

/// A floating surface: the window's ground, its ink, a soft shadow (flora's own corners and warm shadow under flora).
const FLOAT: &str = "background: system:window-background; color: system:text; -azul-app-region: no-drag; \
                     box-shadow: 0px 2px 10px rgba(0, 0, 0, 0.25); @theme(flora) { border-radius: 5px; \
                     box-shadow: 0px 2px 10px rgba(48, 45, 38, 0.22); @media (prefers-color-scheme: dark) { \
                     box-shadow: 0px 2px 10px rgba(0, 0, 0, 0.5); } }";
/// The panel at the left.
const PANEL: &str = "position: absolute; display: flex; flex-direction: column; gap: 10px; padding: 12px; \
                     box-sizing: border-box; border-radius: 12px;";
/// The folded panel: its search key.
const PANEL_PILL: &str = "position: absolute; display: flex; flex-direction: row; align-items: center; \
                          border-radius: 8px;";
/// A phone's search bar and its chips: on top of the map, between its sides.
const TOP: &str = "position: absolute; left: 12px; right: 12px; display: flex; flex-direction: column; gap: 8px; \
                   -azul-app-region: no-drag;";
/// The search bar: the field between its icon and its keys.
const SEARCH_BAR: &str = "display: flex; flex-direction: row; align-items: center; gap: 4px; padding: 4px 6px 4px 12px; \
                          box-sizing: border-box; min-height: 56px; border-radius: 28px;";
/// The search bar inside the panel: no surface of its own.
const SEARCH_ROW: &str = "display: flex; flex-direction: row; align-items: center; gap: 4px; flex-shrink: 0;";
const FIELD: &str = "flex-grow: 1; min-width: 0px;";
/// The chips on a phone: a row that scrolls sideways when it does not fit (no scrollbar over them); a chip keeps its width.
const CHIPS: &str = "display: flex; flex-direction: row; gap: 8px; overflow-x: auto; scrollbar-width: none; flex-shrink: 0; padding: 2px 0px;";
/// The chips in the panel: they wrap onto a second line, a mouse has no sideways scroll to find the rest by.
const CHIPS_WRAPPED: &str = "display: flex; flex-direction: row; flex-wrap: wrap; gap: 8px; flex-shrink: 0; padding: 2px 0px;";
const CHIP: &str = "flex-shrink: 0; display: flex; flex-direction: row;";
/// What a view holds: it scrolls when it is longer than the panel or the sheet.
const BODY: &str = "display: flex; flex-direction: column; gap: 10px; flex-grow: 1; min-height: 0px; overflow-y: auto;";
const VIEW: &str = "display: flex; flex-direction: column; gap: 10px; padding: 0px 14px 14px 14px; box-sizing: border-box;";
const VIEW_IN_PANEL: &str = "display: flex; flex-direction: column; gap: 10px;";
const HEAD_ROW: &str = "display: flex; flex-direction: row; align-items: center; gap: 6px; flex-shrink: 0;";
const TITLE: &str = "font-size: 20px; font-weight: bold; white-space: normal;";
const SUBTITLE: &str = "font-size: 13px; color: system:secondary-text; white-space: normal;";
const ACTIONS: &str = "display: flex; flex-direction: row; flex-wrap: wrap; align-items: center; gap: 8px;";
const HINT: &str = "font-size: 13px; color: system:secondary-text; white-space: normal; padding: 4px 2px;";
/// The floating keys at the right of the map: the zoom pair, the locate key, the styles' key.
const CONTROLS: &str = "position: absolute; right: 12px; display: flex; flex-direction: column; gap: 10px;";
const GROUP: &str = "display: flex; flex-direction: column; border-radius: 8px;";
/// The styles' menu, the content of the popover the layers key opens.
const LAYERS_MENU: &str = "display: flex; flex-direction: column; gap: 4px; min-width: 180px;";
/// The little notice that says a link was copied.
const TOAST: &str = "position: absolute; left: 12px; right: 84px; display: flex; flex-direction: row; \
                     justify-content: center; text-align: center; font-size: 13px; color: #ffffff; \
                     background: rgba(32, 33, 36, 0.92); padding: 8px 12px; border-radius: 8px; \
                     @theme(flora) { color: #F4F2EA; background: rgba(48, 45, 38, 0.94); border-radius: 3px; }";

/// The window's insets as the layout reads them.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    /// The window's size, px.
    pub size: (f32, f32),
    /// What the system keeps of the root, which the engine has already inset by the bars and the cutout: the keyboard
    /// (`Insets::inside_root`).
    pub insets: Insets,
    /// The room the window's own controls keep at the top (macOS's lights at the left, Windows' caption buttons at the right).
    pub title: kit::TitleInsets,
}

impl Frame {
    /// Whether the panel stands at the left of the map.
    #[must_use]
    pub fn wide(&self) -> bool {
        flow::is_wide(self.size.0)
    }

    /// The top of what floats: a gap under the root's top edge (the root starts under the status bar and the cutout) on a phone,
    /// under the window's own controls on a desktop.
    #[must_use]
    pub fn top(&self) -> f32 {
        if self.wide() {
            self.title.height.max(40.0) + 4.0
        } else {
            self.insets.top.max(8.0)
        }
    }
}

/// The sheet's height at `detent` for this window, px: where the keys above it stand.
#[must_use]
pub fn sheet_height(detent: SheetDetent, frame: &Frame) -> f32 {
    sheet(&Dom::create_div(), detent, frame).height_of(detent)
}

/// A sheet of `content` with this window's numbers; one place for them, so the keys above the sheet and the sheet agree.
fn sheet(content: &Dom, detent: SheetDetent, frame: &Frame) -> BottomSheet {
    BottomSheet::create(content.clone())
        .with_label(label("azmaps-sheet"))
        .with_grip_label(label("azmaps-sheet-grip"))
        .with_detent(detent)
        .with_lowest(SheetDetent::Peek)
        .with_highest(SheetDetent::Full)
        .with_container_height(frame.size.1)
        .with_peek_height(PEEK_HEIGHT)
        .with_top_margin(frame.top() + TOP_ROOM)
        .with_bottom_inset(frame.insets.lift())
        .with_safe_bottom(frame.insets.bottom_clear())
}

// ==== The window ====

/// What floats on the map: the panel or the search bar with the sheet, the keys, the notices.
pub fn floating(s: &MapState, app: &RefAny, frame: &Frame) -> Vec<Dom> {
    let mut out = Vec::new();
    // While a navigation runs its banner and its bar are what floats (drive_ui).
    if s.drive.nav.is_none() {
        if frame.wide() {
            out.push(panel(s, app, frame));
        } else {
            out.push(top_bar(s, app, frame));
            out.push(bottom_sheet(s, app, frame));
        }
    }
    // A sheet drawn up to the full height, or the keyboard over the lower half, leaves the map no room for the keys: they wait until the
    // sheet comes down / the keyboard goes (they stood over the chips with the keyboard up).
    let covered = !frame.wide() && s.drive.nav.is_none() && (s.sheet == SheetDetent::Full || frame.insets.lift() > 0.0);
    if !covered {
        out.push(controls(s, app, frame));
    }
    if !s.toast.0.is_empty() {
        let above = if frame.wide() { 0.0 } else { sheet_height(s.sheet, frame) + frame.insets.lift() };
        out.push(
            Dom::create_div()
                .with_css(format!("{TOAST} bottom: {:.0}px;", above + 70.0).as_str())
                .with_id(ids::LINK_COPIED)
                .with_child(Dom::create_span_with_text(s.toast.0.as_str())),
        );
    }
    out
}

// ==== The search bar and the chips ====

/// What the search field's callbacks act on: the field's end (the destination's text is the search's) and the app.
fn field_ref(app: &RefAny) -> RefAny {
    RefAny::new(crate::FieldRef { app: app.clone(), end: crate::model::End::To })
}

/// The search field with its keys: a search icon, the field, a key that clears it, the microphone.
fn search_field(s: &MapState, app: &RefAny) -> Dom {
    let mut row = Dom::create_div().with_css(SEARCH_ROW).with_id(ids::SEARCH_ROW).with_child(
        Dom::create_icon("search").with_css("font-size: 22px; flex-shrink: 0; opacity: 0.7;"),
    );
    row.add_child(
        Dom::create_div().with_css(FIELD).with_child(
            TextInput::create()
                .with_text(s.travel.to.as_str())
                .with_placeholder(label("azmaps-search-places"))
                .with_accessibility_name(label("azmaps-search-places"))
                .with_on_text_input(field_ref(app), crate::on_travel_text as TextInputOnTextInputCallbackType)
                .with_on_virtual_key_down(field_ref(app), crate::on_travel_key as TextInputOnVirtualKeyDownCallbackType)
                .dom()
                .with_id(ids::SEARCH),
        ),
    );
    if !s.travel.to.is_empty() {
        row.add_child(crate::tool(app, "close", "azmaps-clear-search", ids::SEARCH_CLEAR, on_search_clear));
    }
    row.add_child(crate::tool(app, "mic", "azmaps-speak-destination", ids::TRAVEL_SPEAK, crate::on_speak_destination));
    row
}

/// The chips: one search each; `wrapped` in the panel, a scrolling row under the phone's search bar.
fn chips(app: &RefAny, wrapped: bool) -> Dom {
    let mut row = Dom::create_div()
        .with_css(if wrapped { CHIPS_WRAPPED } else { CHIPS })
        .with_id(ids::CHIPS)
        .with_accessibility_name(label("azmaps-chips"));
    for (index, chip) in flow::CHIPS.iter().enumerate() {
        row.add_child(
            Dom::create_div().with_css(CHIP).with_child(
                Button::create(label(chip.name))
                    .with_icon(chip.icon)
                    .with_on_click(
                        RefAny::new(ChipRef { app: app.clone(), index }),
                        on_chip as ButtonOnClickCallbackType,
                    )
                    .dom()
                    .with_id(ids::indexed("chip", index)),
            ),
        );
    }
    row
}

/// A chip's index and the app.
struct ChipRef {
    app: RefAny,
    index: usize,
}

/// A chip: its words are searched, the results come in the panel or the sheet.
extern "C" fn on_chip(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<ChipRef>().map(|c| (c.app.clone(), c.index)) else {
        return Update::DoNothing;
    };
    let Some(chip) = flow::CHIPS.get(index) else {
        return Update::DoNothing;
    };
    let query = label(chip.query).as_str().to_string();
    let update = crate::with_map(&mut app, |s| {
        s.flow.clear();
        s.selected = None;
        s.travel.to = query.clone();
        s.travel.to_pick = None;
        s.suggestions = None;
        // The search starts at the next tick: the text counts as typed a while ago.
        s.typed = Some((crate::model::End::To, crate::rested()));
        s.sheet = SheetDetent::Half;
        println!("AZMAPS_CHIP {index} {query}");
    });
    crate::adopt_field_texts(&mut info);
    update
}

/// The search field's clear key: the text, the results and the place go.
extern "C" fn on_search_clear(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let update = crate::with_map(&mut data, |s| {
        s.travel.to.clear();
        s.travel.to_pick = None;
        s.suggestions = None;
        s.typed = None;
        s.selected = None;
        s.flow.clear();
        s.announce_travel();
    });
    crate::adopt_field_texts(&mut info);
    update
}

// ==== The phone: the bar on top, the sheet at the bottom ====

/// The search bar and the chips on top of the map; in the way's view the sheet holds the ends, so the bar goes.
fn top_bar(s: &MapState, app: &RefAny, frame: &Frame) -> Dom {
    let mut top = Dom::create_div().with_css(format!("{TOP} top: {:.0}px;", frame.top()).as_str());
    if s.flow.panel == Panel::Directions {
        return top;
    }
    top.add_child(
        Dom::create_div()
            .with_css(format!("{SEARCH_BAR} {FLOAT}").as_str())
            .with_id(ids::SEARCH_BAR)
            .with_child(Dom::create_div().with_css(FIELD).with_child(search_field(s, app)))
            .with_child(crate::tool(app, "settings", "azmaps-settings", ids::SETTINGS, crate::on_settings_open)),
    );
    if s.flow.panel == Panel::Browse {
        top.add_child(chips(app, false));
    }
    top
}

/// The sheet: the view of the moment under its grip, at the detent the app keeps.
fn bottom_sheet(s: &MapState, app: &RefAny, frame: &Frame) -> Dom {
    let content = Dom::create_div().with_css(VIEW).with_child(view(s, app));
    let sheet = sheet(&content, s.sheet, frame)
        .with_grab_height(0.0)
        .with_on_event(app.clone(), on_sheet_event as BottomSheetOnEventCallbackType);
    sheet.dom().with_id(ids::SHEET)
}

/// The sheet was dragged or let go: the detent is kept, the keys above it follow once it rests.
extern "C" fn on_sheet_event(mut data: RefAny, _info: CallbackInfo, event: BottomSheetEvent) -> Update {
    if event.kind != BottomSheetEventKind::Settled {
        return Update::DoNothing;
    }
    crate::with_map(&mut data, |s| s.sheet = event.detent)
}

// ==== The panel at the left ====

/// The panel at the left of the map, or - folded - its search key.
fn panel(s: &MapState, app: &RefAny, frame: &Frame) -> Dom {
    let left = PANEL_LEFT;
    if s.flow.collapsed {
        return Dom::create_div()
            .with_css(format!("{PANEL_PILL} {FLOAT} left: {left:.0}px; top: {:.0}px;", frame.top()).as_str())
            .with_child(crate::tool(app, "search", "azmaps-show-search", ids::PANEL_SHOW, on_panel_toggle))
            .with_child(crate::tool(app, "chevron_right", "azmaps-expand-panel", ids::PANEL_TOGGLE, on_panel_toggle));
    }
    let mut head = Dom::create_div().with_css(HEAD_ROW);
    if s.flow.panel == Panel::Directions {
        head.add_child(back_key(app));
        head.add_child(
            Dom::create_div()
                .with_css(format!("{FIELD} font-size: 16px; font-weight: bold;").as_str())
                .with_child(Dom::create_span_with_text(label("azmaps-directions"))),
        );
    } else {
        head.add_child(Dom::create_div().with_css(FIELD).with_child(search_field(s, app)));
    }
    head.add_child(crate::tool(app, "settings", "azmaps-settings", ids::SETTINGS, crate::on_settings_open));
    head.add_child(crate::tool(app, "chevron_left", "azmaps-collapse-panel", ids::PANEL_TOGGLE, on_panel_toggle));
    let mut panel = Dom::create_div()
        .with_css(
            format!(
                "{PANEL} {FLOAT} left: {left:.0}px; top: {:.0}px; bottom: 26px; width: {PANEL_WIDTH:.0}px;",
                frame.top()
            )
            .as_str(),
        )
        .with_id(ids::PANEL)
        .with_accessibility_name(label("azmaps-panel"))
        .with_child(head);
    if s.flow.panel == Panel::Browse {
        panel.add_child(chips(app, true));
    }
    panel.add_child(Dom::create_div().with_css(BODY).with_child(view(s, app)));
    panel
}

/// The panel folds away, or comes back; kept in the settings (the key of the old sidebar: open, closed).
extern "C" fn on_panel_toggle(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let mut kept = None;
    let update = crate::with_map(&mut data, |s| {
        s.flow.toggle_collapsed();
        let value = if s.flow.collapsed { "closed" } else { "open" };
        println!("AZMAPS_PANEL {value}");
        kept = Some((s.kit.clone(), value));
    });
    if let Some((kit_ref, value)) = kept {
        kit::set_value(&kit_ref, &mut info, crate::model::SIDEBAR_KEY, value);
    }
    update
}

// ==== The three views ====

/// The way back: from the way to its place, from the place to the search.
fn back_key(app: &RefAny) -> Dom {
    crate::tool(app, "arrow_back", "azmaps-back", ids::BACK, on_back)
}

extern "C" fn on_back(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let update = crate::with_map(&mut data, |s| {
        s.flow.back();
        s.selected = s.flow.place.as_ref().and_then(|p| p.pin);
        if s.flow.panel == Panel::Browse {
            s.travel.to.clear();
            s.travel.to_pick = None;
        }
        s.announce_travel();
    });
    crate::adopt_field_texts(&mut info);
    crate::ask_route(&data, &mut info);
    update
}

/// What the panel or the sheet holds now.
fn view(s: &MapState, app: &RefAny) -> Dom {
    match (s.flow.panel, s.flow.place.as_ref()) {
        (Panel::Place, Some(card)) => place_view(s, app, card),
        (Panel::Directions, _) => directions_view(s, app),
        _ => browse_view(s, app),
    }
}

/// The search: the results of what was typed, else what AzMaps remembers - the saved lists, the recent places - or a
/// line that says what to do.
fn browse_view(s: &MapState, app: &RefAny) -> Dom {
    if let Some(list) = &s.suggestions {
        return Dom::create_div()
            .with_css(VIEW_IN_PANEL)
            .with_id(ids::RESULTS)
            .with_child(crate::suggestion_list(list, app));
    }
    let mut view = Dom::create_div().with_css(VIEW_IN_PANEL).with_id(ids::HOME);
    let saved = lists_ui::saved_section(s, app);
    let recents = crate::recents(s, app);
    if saved.is_none() && recents.is_none() {
        view.add_child(
            Dom::create_div().with_css(HINT).with_child(Dom::create_span_with_text(label("azmaps-home-hint"))),
        );
    }
    if let Some(saved) = saved {
        view.add_child(saved);
    }
    if let Some(recents) = recents {
        view.add_child(recents);
    }
    view
}

/// One place: its name, what it is, the keys, and - the further the sheet is drawn up - the facts.
fn place_view(s: &MapState, app: &RefAny, card: &PlaceCard) -> Dom {
    let (lat, lon) = card.at;
    let title = if card.name.is_empty() {
        match lists_ui::address_of(s, card.at) {
            Some(address) => address.to_string(),
            None => label("azmaps-dropped-pin").as_str().to_string(),
        }
    } else {
        card.name.clone()
    };
    let kind = if card.kind.is_empty() { String::new() } else { label(card.kind).as_str().to_string() };
    let subtitle = if kind.is_empty() { crate::coords(lat, lon) } else { format!("{kind} \u{b7} {}", crate::coords(lat, lon)) };
    let saved = lists_ui::is_favourite(s, card.at);
    let mut keys = Dom::create_div().with_css(ACTIONS).with_child(
        Button::with_type(label("azmaps-route"), ButtonType::Primary)
            .with_icon("directions")
            .with_on_click(app.clone(), on_place_route as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::PLACE_ROUTE),
    );
    keys.add_child(
        Button::create(label("azmaps-save"))
            .with_icon(if saved { "star" } else { "star_border" })
            .with_toggled(saved)
            .with_on_click(app.clone(), on_place_save as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::PLACE_SAVE),
    );
    keys.add_child(
        Button::create(label("azmaps-share"))
            .with_icon("share")
            .with_on_click(app.clone(), on_place_share as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::PLACE_SHARE)
            .with_accessibility_name(label("azmaps-share-name")),
    );
    if card.pin.is_some() {
        keys.add_child(
            Button::create("")
                .with_icon("delete")
                .with_on_click(app.clone(), on_place_remove_card as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::PLACE_REMOVE)
                .with_accessibility_name(label("azmaps-remove")),
        );
    }
    let mut head = Dom::create_div().with_css(HEAD_ROW).with_child(back_key(app));
    head.add_child(
        Dom::create_div().with_css(FIELD).with_child(
            Dom::create_div().with_css(TITLE).with_child(Dom::create_span_with_text(title)),
        ),
    );
    Dom::create_div()
        .with_css(VIEW_IN_PANEL)
        .with_id(ids::PLACE)
        .with_accessibility_name(label("azmaps-place"))
        .with_child(head)
        .with_child(Dom::create_div().with_css(SUBTITLE).with_child(Dom::create_span_with_text(subtitle)))
        .with_child(keys)
}

/// The way: the back key and the title, then the travel panel (ends, modes, summary, start, steps).
fn directions_view(s: &MapState, app: &RefAny) -> Dom {
    let mut view = Dom::create_div().with_css(VIEW_IN_PANEL);
    // On a phone the sheet holds the ends, so the way back is the head of the view; in the panel it is the panel's head.
    if !flow::is_wide(s.size.get().0) {
        view.add_child(
            Dom::create_div().with_css(HEAD_ROW).with_child(back_key(app)).with_child(
                Dom::create_div()
                    .with_css(format!("{FIELD} font-size: 16px; font-weight: bold;").as_str())
                    .with_child(Dom::create_span_with_text(label("azmaps-directions"))),
            ),
        );
    }
    view.add_child(crate::travel_panel(s, app));
    view
}

// ==== A place's keys ====

/// Route: the way to the place; both ends are shown, the start empty is where you are.
extern "C" fn on_place_route(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let update = crate::with_map(&mut data, |s| {
        let Some(card) = s.flow.place.clone() else {
            return;
        };
        // A place the search named keeps its name; a pin or a saved place without one is its coordinates.
        if s.travel.to_pick.as_ref().map(|p| p.at) != Some(card.at) {
            s.travel.to = if card.name.is_empty() { crate::model::place_text(card.at.0, card.at.1) } else { card.name.clone() };
            s.travel.to_pick = if card.name.is_empty() { None } else { Some(crate::Picked { label: card.name.clone(), at: card.at }) };
        }
        s.suggestions = None;
        s.selected = None;
        s.flow.directions();
        s.sheet = SheetDetent::Half;
        s.announce_travel();
    });
    crate::adopt_field_texts(&mut info);
    crate::ask_route(&data, &mut info);
    update
}

/// Save: the place goes into the favourites, or out of them.
extern "C" fn on_place_save(mut data: RefAny, _info: CallbackInfo) -> Update {
    crate::with_map(&mut data, |s| {
        let Some(card) = s.flow.place.clone() else {
            return;
        };
        let name = if card.name.is_empty() { crate::coords(card.at.0, card.at.1) } else { card.name.clone() };
        lists_ui::toggle_favourite(s, &name, card.at);
        println!("AZMAPS_FAVOURITE {} {}", card.pin.map_or_else(|| "-".to_string(), |p| p.to_string()), if lists_ui::is_favourite(s, card.at) { "on" } else { "off" });
    })
}

/// Share: a link to the place on the clipboard and a notice that says so (what a desktop does; a phone's system sheet is WP-F's).
// TODO(WP-F): on a phone `info.share(ShareRequest::url(link).with_title(name))`, `can_share()` choosing, the button's rect as its anchor, once ShareRequest is in api.json.
extern "C" fn on_place_share(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(link) = data
        .downcast_ref::<MapState>()
        .and_then(|s| s.flow.place.as_ref().map(|p| flow::share_link(p.at.0, p.at.1)))
    else {
        return Update::DoNothing;
    };
    info.set_clipboard_content(azul::dom::ClipboardContent {
        plain_text: AzString::from(link.as_str()),
        styled_runs: azul::vec::StyledTextRunVec::create(),
        html: azul::option::OptionString::None,
        files: azul::vec::StringVec::create(),
        image: azul::option::OptionU8Vec::None,
        link: azul::option::OptionString::Some(AzString::from(link.as_str())),
        concealed: false,
    });
    println!("AZMAPS_SHARE {link}");
    crate::with_map(&mut data, |s| s.toast = (label("azmaps-link-copied").as_str().to_string(), crate::TOAST_TICKS))
}

/// The delete key of a dropped pin's card: the pin goes, and its card with it.
extern "C" fn on_place_remove_card(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let mut removed = false;
    let update = crate::with_map(&mut data, |s| {
        let Some(index) = s.flow.place.as_ref().and_then(|p| p.pin) else {
            return;
        };
        if index < s.pins.len() {
            s.pins.remove(index);
            removed = true;
        }
        s.flow.pin_removed(Some(index));
        s.selected = None;
    });
    if removed {
        crate::save_pins(&data, &mut info);
    }
    update
}

// ==== The floating keys ====

/// The keys at the right of the map: zoom in and out, where I am, the map styles. On a phone they stand above the sheet.
fn controls(s: &MapState, app: &RefAny, frame: &Frame) -> Dom {
    let (locate_icon, locate_name) = if s.locate_failed {
        ("location_disabled", label("azmaps-location-unavailable"))
    } else {
        ("my_location", label("azmaps-show-location"))
    };
    let bottom = controls_bottom(s, frame);
    let mut keys = Dom::create_div()
        .with_css(format!("{CONTROLS} bottom: {bottom:.0}px;").as_str())
        .with_id(ids::CONTROLS)
        .with_child(
            Dom::create_div()
                .with_css(format!("{GROUP} {FLOAT}").as_str())
                .with_child(crate::tool(app, "add", "azmaps-zoom-in", ids::ZOOM_IN, crate::on_zoom_in))
                .with_child(crate::tool(app, "remove", "azmaps-zoom-out", ids::ZOOM_OUT, crate::on_zoom_out)),
        )
        .with_child(
            Dom::create_div().with_css(format!("{GROUP} {FLOAT}").as_str()).with_child(
                Button::create("")
                    .with_icon(locate_icon)
                    .with_toggled(s.locating)
                    .with_on_click(app.clone(), crate::on_locate as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::LOCATE)
                    .with_accessibility_name(locate_name),
            ),
        );
    keys.add_child(Dom::create_div().with_css(format!("{GROUP} {FLOAT}").as_str()).with_child(layers_key(s, app)));
    keys
}

/// How far above the window's bottom the keys stand: a desktop's corner, a phone's sheet top.
fn controls_bottom(s: &MapState, frame: &Frame) -> f32 {
    if frame.wide() || s.drive.nav.is_some() {
        return 30.0;
    }
    // The sheet's lower edge is lifted by the keyboard; the keys stand above its top.
    (sheet_height(s.sheet, frame) + frame.insets.lift() + ABOVE_SHEET).min(frame.size.1 - 3.0 * KEY - frame.top())
}

/// A style's index and the app.
struct StyleRef {
    app: RefAny,
    index: usize,
}

/// The map styles' key: a popover (the engine's own: it opens below the key on a click and closes on a press outside, on Escape
/// or when a style is picked) with one row per style, the chosen one lit.
fn layers_key(s: &MapState, app: &RefAny) -> Dom {
    let anchor = Button::create("")
        .with_icon("layers")
        .dom()
        .with_id(ids::LAYERS)
        .with_accessibility_name(label("azmaps-layers"));
    let mut menu = Dom::create_div()
        .with_css(LAYERS_MENU)
        .with_id(ids::LAYERS_MENU)
        .with_accessibility_name(label("azmaps-layers-menu"));
    for (index, style) in flow::STYLES.iter().enumerate() {
        menu.add_child(
            Button::create(label(style.name))
                .with_toggled(s.style == index)
                .with_on_click(
                    RefAny::new(StyleRef { app: app.clone(), index }),
                    on_style as ButtonOnClickCallbackType,
                )
                .dom()
                .with_id(ids::indexed("style", index)),
        );
    }
    Popover::create(anchor, menu).dom()
}

/// A style was picked: the map is drawn in it, the choice is kept.
extern "C" fn on_style(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<StyleRef>().map(|r| (r.app.clone(), r.index)) else {
        return Update::DoNothing;
    };
    let mut kept = None;
    let update = crate::with_map(&mut app, |s| {
        let Some(style) = flow::STYLES.get(index) else {
            return;
        };
        s.style = index;
        s.layer.theme = crate::theme_of(index);
        println!("AZMAPS_STYLE {}", style.key);
        kept = Some((s.kit.clone(), style.key));
    });
    if let Some((kit_ref, value)) = kept {
        kit::set_value(&kit_ref, &mut info, flow::STYLE_KEY, value);
    }
    // The popover holds the row that was pressed: it closes.
    let hit = info.get_hit_node();
    let _ = Dialog::close_from(info, hit, "");
    update
}

/// The desktop's strip over the top of the map, which the window is moved by. It lies under everything else.
#[must_use]
pub fn drag_strip(frame: &Frame) -> Option<Dom> {
    if !frame.wide() {
        return None;
    }
    Some(
        Dom::create_div()
            .with_css(
                format!(
                    "position: absolute; left: 0px; top: 0px; right: 0px; height: {:.0}px; -azul-app-region: drag;",
                    frame.title.height.max(28.0)
                )
                .as_str(),
            )
            .with_id(ids::TITLE),
    )
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
