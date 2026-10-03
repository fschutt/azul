//! The Find / Replace pane (Mod+F, Mod+H, HOME > Editing): the standard
//! `FindReplaceDialog` in the side pane, over [`crate::find`]. Find next
//! goes to the slide and selects the element (or says "in the notes");
//! Replace rewrites the place found and goes on; Replace all is one undo
//! step.

use azul::{
    callbacks::{CallbackInfo, RefAny, StandardDialogOnEventCallbackType, Update},
    dom::Dom,
    str::String as AzString,
    widgets::{FindReplaceDialog, StandardDialogEvent, StandardDialogEventKind},
};

use crate::{
    app::AppState,
    find::{self, FindState},
};

/// The pane for the open find.
#[must_use]
pub fn pane(app: &RefAny, f: &FindState) -> Dom {
    let mut dialog = FindReplaceDialog::create(AzString::from(f.text.as_str()))
        .with_options(f.how.match_case, f.how.whole_word)
        .with_status(AzString::from(f.status.as_str()))
        .with_on_event(app.clone(), on_find_event as StandardDialogOnEventCallbackType);
    if f.show_replace {
        dialog = dialog.with_replace(AzString::from(f.replace.as_str()));
    }
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; padding: 8px 12px; overflow-y: auto;")
        .with_child(dialog.dom())
}

/// Goes to the next (or previous) match: its slide, its element selected.
fn go(s: &mut AppState, backwards: bool) {
    let (Some(f), Some(ed)) = (s.find.as_mut(), s.editor.as_mut()) else {
        return;
    };
    match find::find_next(&ed.deck, f.last, &f.text, f.how, backwards) {
        Some(place) => {
            ed.go_to(place.slide);
            f.status = match place.element {
                Some(id) => {
                    if let Some(i) = ed.slide().elements.iter().position(|e| e.id == id) {
                        ed.select_element(i, false, false);
                    }
                    format!("Slide {}", place.slide + 1)
                }
                None => format!("Slide {}, in the notes", place.slide + 1),
            };
            f.last = Some(place);
        }
        None => {
            f.status = format!("\"{}\" was not found.", f.text);
            f.last = None;
        }
    }
}

extern "C" fn on_find_event(mut data: RefAny, _info: CallbackInfo, event: StandardDialogEvent) -> Update {
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let kind = event.kind;
    if kind == StandardDialogEventKind::FieldChanged {
        // Kept, no rebuild: the field shows its own text.
        if let Some(f) = s.find.as_mut() {
            let text = event.text.as_str().to_string();
            if event.index == 0 {
                f.text = text;
                f.last = None;
            } else {
                f.replace = text;
            }
        }
        return Update::DoNothing;
    }
    if s.find.as_ref().is_some_and(|f| f.text.is_empty()) && kind != StandardDialogEventKind::Cancel {
        return Update::DoNothing;
    }
    match kind {
        StandardDialogEventKind::OptionToggled => {
            if let Some(f) = s.find.as_mut() {
                if event.index == 0 {
                    f.how.match_case = event.checked;
                } else {
                    f.how.whole_word = event.checked;
                }
            }
        }
        StandardDialogEventKind::FindNext => go(s, false),
        StandardDialogEventKind::FindPrevious => go(s, true),
        StandardDialogEventKind::Replace => {
            if let (Some(f), Some(ed)) = (s.find.as_ref(), s.editor.as_mut()) {
                if let Some(place) = f.last {
                    let mut copy = ed.deck.clone();
                    if find::replace_in(&mut copy, place, &f.text, &f.replace, f.how) {
                        ed.checkpoint();
                        ed.deck = copy;
                    }
                }
            }
            go(s, false);
        }
        StandardDialogEventKind::ReplaceAll => {
            if let (Some(f), Some(ed)) = (s.find.as_mut(), s.editor.as_mut()) {
                let mut copy = ed.deck.clone();
                let n = find::replace_all(&mut copy, &f.text, &f.replace, f.how);
                if n > 0 {
                    // One undo step for all of it.
                    ed.checkpoint();
                    ed.deck = copy;
                }
                f.status = format!("Replaced in {n} place(s).");
                f.last = None;
            }
        }
        StandardDialogEventKind::Cancel => s.find = None,
        _ => return Update::DoNothing,
    }
    Update::RefreshDom
}
