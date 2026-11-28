//! The location block at the head of the ribbon's File tab (azul's `LocationBar`): the parents of
//! the open place as links - This PC, the drive, every folder down to it - over the open folder's
//! name, big and bold. It replaces the address row's breadcrumb box and its path box.
//!
//! - A click on a parent goes there (a click on the open folder's own name reads it again).
//! - The pen turns the name into a live field: every key narrows the folder view to the names
//!   that hold the text (`DriveState::location_text`, read by `visible_entries`), Enter goes to
//!   what was typed - a path as the old path box took it (`Home/Documents`, `This PC`), else the
//!   name of an item of the folder (a folder opens, a file is selected), else the one item the
//!   text leaves - and Escape gives the edit up: the name and the whole folder are back.
//! - A click elsewhere ends the edit too (the field lost the keyboard). When that click is on a
//!   row the narrowed list shows, the list is not drawn again before the click has run: it would
//!   otherwise slide the row from under the pointer ([`cancel_is_blur`]).
//!
//! While a search of the folder is open the name says so ("Search: report"): the text is in the
//! command line at the window's foot.

use azul::{
    callbacks::LocationOnEventCallbackType,
    prelude::*,
    str::String as AzString,
    widgets::{LocationBar, LocationCrumb, LocationEvent, LocationEventKind},
};
use azul_appkit::l10n::{label, t_args, Arg, Phrase};

use crate::{
    actions,
    browse::{self, Place},
    go, ui_view, with_state, DriveState,
};

/// The trail of `place` in the window's language, the open place last: This PC (for a folder),
/// the drive, the folders down to it. A crumb's tag in the widget is its place in this list.
#[must_use]
pub(crate) fn trail_of(place: &Place, drive_name: &str) -> Vec<(String, Place)> {
    let mut trail = Vec::new();
    if matches!(place, Place::Folder { .. }) {
        trail.push((browse::this_pc(), Place::ThisPc));
    }
    trail.extend(browse::crumbs_of(place, drive_name));
    trail
}

/// The block for the open place: its parents as links, its name over them, the pen.
pub(crate) fn location(s: &DriveState, app: &RefAny) -> Dom {
    let trail = trail_of(&s.place, &s.drive_name(&s.place));
    let (open, parents) = match trail.split_last() {
        Some((open, parents)) => (open.0.clone(), parents),
        None => (s.place_name(), &[][..]),
    };
    let name = if s.find.is_some() {
        t_args(
            "azdrive-loctab-search",
            &[("text", Arg::from(s.search.trim().to_string()))],
        )
    } else {
        open
    };
    let mut bar = LocationBar::create(AzString::from(name))
        .with_editing(s.editing_path, AzString::from(s.location_text.as_str()))
        .with_placeholder(label("azdrive-location-placeholder"))
        .with_form(s.form)
        .with_on_event(app.clone(), on_location as LocationOnEventCallbackType);
    for (tag, (text, _)) in parents.iter().enumerate() {
        bar = bar.with_parent(LocationCrumb::create(AzString::from(text.as_str()), tag));
    }
    bar.dom()
}

/// What Enter in the location's field goes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Goal {
    /// Nothing was typed: the field just closes.
    Stay,
    /// A path that names a place (`Home/Documents`, `This PC`).
    Place(Place),
    /// The folder `key` of the open folder.
    Folder(String),
    /// The file `key` of the open folder: it is selected and brought into view.
    File(String),
    /// Neither a path nor a name of the folder.
    Nowhere,
}

/// Where the typed `text` goes: a path first (as the old path box read it), then the name of an
/// item of the open folder - the exact name without regard to case, else the one item whose name
/// holds the text. `items` are the folder's `(name, key, is_folder)`.
#[must_use]
pub(crate) fn goal_of(text: &str, drives: &[(String, String)], items: &[(&str, &str, bool)]) -> Goal {
    let text = text.trim();
    if text.is_empty() {
        return Goal::Stay;
    }
    if let Some(place) = browse::parse_path(text, drives) {
        return Goal::Place(place);
    }
    let needle = text.to_lowercase();
    let by_name = |(name, _, _): &&(&str, &str, bool)| name.to_lowercase() == needle;
    let holds = |(name, _, _): &&(&str, &str, bool)| name.to_lowercase().contains(&needle);
    let found = items.iter().find(by_name).or_else(|| {
        let mut holding = items.iter().filter(holds);
        match (holding.next(), holding.next()) {
            (Some(only), None) => Some(only),
            _ => None,
        }
    });
    match found {
        Some((_, key, true)) => Goal::Folder((*key).to_string()),
        Some((_, key, false)) => Goal::File((*key).to_string()),
        None => Goal::Nowhere,
    }
}

/// Whether the field's end was a click elsewhere rather than Escape: the field reports Escape with
/// the text it held, and a lost keyboard with none, so a report of no text while text was typed
/// is the click.
#[must_use]
pub(crate) fn cancel_is_blur(reported: &str, typed: &str) -> bool {
    reported.is_empty() && !typed.trim().is_empty()
}

/// The pen's reports: the parents, the pen, every key of the field, Enter, Escape.
extern "C" fn on_location(mut data: RefAny, mut info: CallbackInfo, event: LocationEvent) -> Update {
    let text = event.text.as_str().to_string();
    if event.kind == LocationEventKind::EditCancelled {
        // A click elsewhere: the state is back to the name, but the rows stay as they are until
        // the click that ended the edit has run (it draws them again).
        let blur = data
            .downcast_ref::<DriveState>()
            .is_some_and(|s| s.editing_path && cancel_is_blur(&text, &s.location_text));
        if blur {
            if let Some(mut s) = data.downcast_mut::<DriveState>() {
                s.editing_path = false;
                s.location_text.clear();
            }
            return Update::DoNothing;
        }
    }
    with_state(&mut data, &mut info, |info, app, s| match event.kind {
        LocationEventKind::CrumbClicked => crumb_clicked(info, app, s, event.tag),
        LocationEventKind::EditStarted => {
            s.editing_path = true;
            s.location_text.clear();
        }
        LocationEventKind::EditChanged => narrow(info, s, text),
        LocationEventKind::EditCommitted => commit(info, app, s, &text),
        LocationEventKind::EditCancelled => end_edit(info, s),
    })
}

/// A parent was clicked: the window goes there; the open folder's own name reads it again.
fn crumb_clicked(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, tag: usize) {
    let trail = trail_of(&s.place, &s.drive_name(&s.place));
    let Some((_, place)) = trail.get(tag).cloned() else {
        return;
    };
    if place == s.place {
        crate::refresh(info, app, s);
    } else {
        go(info, app, s, place, true);
    }
}

/// A key of the field: the folder's rows narrow to the names that hold `text`; the view starts
/// at its top (the narrowed list is shorter than where it was scrolled to).
fn narrow(info: &mut CallbackInfo, s: &mut DriveState, text: String) {
    s.editing_path = true;
    s.location_text = text;
    s.view_scroll.0 = 0.0;
    ui_view::scroll_view_to_top(info);
    let keys = s.visible_keys();
    let order: Vec<&str> = keys.iter().map(String::as_str).collect();
    s.selection.retain(&order);
}

/// The edit is given up: the name and the whole folder are back.
fn end_edit(info: &mut CallbackInfo, s: &mut DriveState) {
    if !s.editing_path && s.location_text.is_empty() {
        return;
    }
    s.editing_path = false;
    s.location_text.clear();
    ui_view::scroll_view_to_top(info);
}

/// Enter: the field closes and the window goes where the text says.
fn commit(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, text: &str) {
    let items: Vec<(String, String, bool)> = s
        .visible_entries()
        .iter()
        .map(|e| (e.name.clone(), e.key.clone(), e.is_folder))
        .collect();
    let borrowed: Vec<(&str, &str, bool)> = items
        .iter()
        .map(|(name, key, folder)| (name.as_str(), key.as_str(), *folder))
        .collect();
    let goal = goal_of(text, &s.drive_names(), &borrowed);
    end_edit(info, s);
    match goal {
        Goal::Stay => {}
        Goal::Place(place) => go(info, app, s, place, true),
        Goal::Folder(key) => match s.current_drive_id() {
            Some(drive) => go(info, app, s, Place::folder(&drive, &key), true),
            None => s.error(Phrase::new("azdrive-no-drive-for").arg("path", text.trim())),
        },
        Goal::File(key) => {
            actions::click_item(info, app, s, &key, false, false);
            ui_view::reveal_item(info, s, &key);
        }
        Goal::Nowhere => s.error(Phrase::new("azdrive-no-drive-for").arg("path", text.trim())),
    }
}

#[cfg(test)]
#[path = "ui_location_test.rs"]
mod ui_location_test;
