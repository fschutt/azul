//! The header of the whiteboard's panel: the board that is open (its name is a button), "new board", and the list of
//! boards as a popover (open one, rename, duplicate, delete, move up and down); a deleted board is told in a bar at the
//! foot of the panel with Undo. What the clicks ask of the boards is [`Panel::board_action`], which is done at once
//! or, when the open board is not kept yet, when it is ([`BoardOutcome::Wait`], [`Panel::after_wait`]). The window's part
//! is the [`Plan`] the host gets ([`crate::whiteboard_ui::Host::on_plan`]): the list to write, a copy to write, a board
//! to read.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, TextInputOnFocusLostCallbackType, TextInputOnTextInputCallbackType,
        TextInputOnVirtualKeyDownCallbackType,
    },
    dom::{AttributeType, VirtualKeyCode},
    prelude::*,
    str::String as AzString,
    widgets::{Button, OnTextInputReturn, TextInput, TextInputState, TextInputValid},
};
use azul_appkit::l10n::{t, t_args, Arg};

use crate::{
    whiteboard_app::{Plan, HEADER_PX},
    whiteboard_ui::{say, Host, Panel},
};

/// How long the bar that tells a board was deleted stays, ms.
pub const TOAST_MS: u64 = 8_000;

/// A change of the boards that a click asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum BoardAction {
    /// Open this board.
    Switch(String),
    New,
    /// A copy of the open board.
    Duplicate,
    Delete(String),
    Restore(String),
    Rename(String, String),
    /// Move this board `n` places in the list (negative: up).
    Move(String, isize),
}

/// What a bar at the foot of the panel says, and what its button undoes.
#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub text: String,
    /// The board that Undo brings back.
    pub undo: Option<String>,
    pub until_ms: u64,
}

/// The state of the header: whether the list is open, which board is being renamed, what is told, what waits.
#[derive(Debug, Clone, Default)]
pub struct Switcher {
    pub open: bool,
    /// The board whose name is being typed.
    pub renaming: Option<String>,
    pub toast: Option<Toast>,
    /// A change that waits for the open board to be kept: it is done when the save is ([`Panel::after_wait`]).
    pub pending: Option<BoardAction>,
}

/// What a [`BoardAction`] came to.
#[derive(Debug, Clone, PartialEq)]
pub enum BoardOutcome {
    /// Done: the host does the plan.
    Done(Plan),
    /// The open board is not kept yet (or one is being read): the host starts a save and asks again ([`Panel::after_wait`]).
    Wait,
    /// Refused: what was said is in [`Panel::hint`].
    Told,
}

/// The name a board gets when the person gave none: "Board 3".
#[must_use]
pub fn default_name(n: u64) -> String {
    t_args("azreview-whiteboard-board-default", &[("n", Arg::from(usize::try_from(n).unwrap_or(usize::MAX)))])
}

impl Panel {
    /// The name of the first board of a workspace.
    #[must_use]
    pub fn first_board_name() -> String {
        default_name(1)
    }

    /// Does what a click on the header asks for, or says why not.
    pub fn board_action(&mut self, action: BoardAction) -> BoardOutcome {
        let now = self.now_ms();
        let fresh = default_name(self.wb.boards().next_number());
        let result = match &action {
            BoardAction::Switch(id) => self.wb.switch_board(id, now),
            BoardAction::New => self.wb.new_board(&fresh, now),
            BoardAction::Duplicate => {
                let name = self.wb.active_board().1.to_string();
                self.wb.duplicate_board(&t_args("azreview-whiteboard-board-copy", &[("name", Arg::from(name))]), now)
            }
            BoardAction::Delete(id) => {
                let name = self.wb.boards().get(id).map(|b| b.name.clone()).unwrap_or_default();
                let plan = self.wb.delete_board(id, now, &fresh);
                if plan.is_ok() {
                    self.switcher.toast = Some(Toast {
                        text: t_args("azreview-whiteboard-board-deleted", &[("name", Arg::from(name))]),
                        undo: Some(id.clone()),
                        until_ms: now + TOAST_MS,
                    });
                }
                plan
            }
            BoardAction::Restore(id) => {
                let plan = self.wb.restore_board(id, now);
                if plan.is_ok() {
                    self.switcher.toast = None;
                }
                plan
            }
            BoardAction::Rename(id, name) => self.wb.rename_board(id, name, now),
            BoardAction::Move(id, by) => {
                let at = self.wb.boards().boards().position(|b| &b.id == id).unwrap_or(0);
                let to = usize::try_from((at as isize + by).max(0)).unwrap_or(0);
                self.wb.move_board(id, to, now)
            }
        };
        self.collect_notices();
        match result {
            Ok(plan) => {
                self.switcher.pending = None;
                if !matches!(action, BoardAction::Rename(..) | BoardAction::Move(..) | BoardAction::Restore(_)) {
                    self.switcher.open = false;
                }
                self.switcher.renaming = None;
                BoardOutcome::Done(plan)
            }
            Err(crate::whiteboard::Notice::BoardBusy) => {
                self.switcher.pending = Some(action);
                BoardOutcome::Wait
            }
            Err(n) => {
                self.hint = Some(say(&n));
                BoardOutcome::Told
            }
        }
    }

    /// The open board was kept (or read): what waited for it is asked again.
    pub fn after_wait(&mut self) -> Option<BoardOutcome> {
        let action = self.switcher.pending.take()?;
        Some(self.board_action(action))
    }

    /// The bar that tells a board was deleted goes when its time is up. Whether it went (the window shows it).
    pub fn expire_toast(&mut self, now_ms: u64) -> bool {
        if self.switcher.toast.as_ref().is_some_and(|t| now_ms >= t.until_ms) {
            self.switcher.toast = None;
            return true;
        }
        false
    }
}

// ---- the DOM

/// What a button of the header carries: the panel it is on and the board it is about.
struct Tap {
    host: Host,
    id: String,
}

fn tap(host: &Host, id: &str) -> RefAny {
    RefAny::new(Tap { host: host.clone(), id: id.to_string() })
}

fn button(text: &str, name: &str, tag: RefAny, on_click: ButtonOnClickCallbackType, id: &str, css: &str) -> Dom {
    Button::create(text)
        .with_on_click(tag, on_click)
        .dom()
        .with_id(id)
        .with_accessibility_name(AzString::from(name))
        .with_css(css)
}

const SMALL: &str = "font-size: 10px; padding: 1px 5px; margin-left: 3px; flex-shrink: 0;";

/// The header: the open board's name (it opens the list) and "new board".
pub(crate) fn header_dom(panel: &Panel, host: &Host) -> Dom {
    let name = panel.wb.active_board().1.to_string();
    let switcher = button(
        &format!("{name} \u{25be}"),
        &t_args("azreview-whiteboard-boards-switcher", &[("name", Arg::from(name))]),
        tap(host, ""),
        on_toggle,
        "__azreview_whiteboard-boards",
        "flex-grow: 1; min-width: 0px; text-align: left; font-size: 12px; font-weight: bold; overflow: hidden;",
    );
    let new = button(
        "+",
        &t("azreview-whiteboard-board-new"),
        tap(host, ""),
        on_new,
        "__azreview_whiteboard-board-new",
        "font-size: 12px; padding: 1px 8px; margin-left: 4px; flex-shrink: 0;",
    );
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; height: {HEADER_PX}px; \
             padding: 0px 6px; background: #ebe7dd; border-bottom: 1px solid #d6d1c6;"
        ).as_str())
        .with_child(switcher)
        .with_child(new)
}

/// The list of boards under the header: one row for each, the open one marked.
pub(crate) fn popover_dom(panel: &Panel, host: &Host) -> Dom {
    let open_id = panel.wb.active_board().0.to_string();
    let shown: Vec<_> = panel.wb.boards().boards().cloned().collect();
    let last = shown.len().saturating_sub(1);
    let mut list = Dom::create_div()
        .with_id("__azreview_whiteboard-board-list")
        .with_css(
            format!(
                "position: absolute; left: 6px; right: 6px; top: {}px; max-height: 320px; overflow-y: auto; display: flex; \
                 flex-direction: column; padding: 4px; background: #fbfaf6; border: 1px solid #c9c3b6; \
                 box-shadow: 0px 3px 10px rgba(0,0,0,0.25);",
                HEADER_PX + 2.0
            )
            .as_str(),
        );
    for (n, b) in shown.iter().enumerate() {
        let is_open = b.id == open_id;
        let mut row = Dom::create_div().with_css(
            format!(
                "display: flex; flex-direction: row; align-items: center; padding: 2px 2px; background: {};",
                if is_open { "#e3edfb" } else { "transparent" }
            )
            .as_str(),
        );
        if panel.switcher.renaming.as_deref() == Some(b.id.as_str()) {
            row.add_child(rename_field(host, &b.id, &b.name));
        } else {
            row.add_child(button(
                &b.name,
                &b.name,
                tap(host, &b.id),
                on_pick,
                &format!("__azreview_whiteboard-board-{}", b.id),
                "flex-grow: 1; min-width: 0px; text-align: left; font-size: 12px; overflow: hidden;",
            ));
            let small = |key: &str, cb: ButtonOnClickCallbackType, suffix: &str| {
                button(&t(key), &t(key), tap(host, &b.id), cb, &format!("__azreview_whiteboard-board-{}-{suffix}", b.id), SMALL)
            };
            row.add_child(small("azreview-whiteboard-board-rename", on_rename, "rename"));
            if is_open {
                row.add_child(small("azreview-whiteboard-board-duplicate", on_duplicate, "duplicate"));
            }
            row.add_child(small("azreview-whiteboard-board-delete", on_delete, "delete"));
            if n > 0 {
                row.add_child(small("azreview-whiteboard-board-up", on_up, "up"));
            }
            if n < last {
                row.add_child(small("azreview-whiteboard-board-down", on_down, "down"));
            }
        }
        list.add_child(row);
    }
    list
}

/// The field a board's name is typed in (Enter or leaving it keeps the name, Escape drops it).
fn rename_field(host: &Host, id: &str, name: &str) -> Dom {
    let tag = tap(host, id);
    TextInput::create()
        .with_text(name)
        .with_accessibility_name(t_args("azreview-whiteboard-board-rename-name", &[("name", Arg::from(name.to_string()))]))
        .with_on_text_input(tag.clone(), on_rename_text as TextInputOnTextInputCallbackType)
        .with_on_virtual_key_down(tag.clone(), on_rename_key as TextInputOnVirtualKeyDownCallbackType)
        .with_on_focus_lost(tag, on_rename_blur as TextInputOnFocusLostCallbackType)
        .dom()
        .with_id(format!("__azreview_whiteboard-board-{id}-field").as_str())
        .with_attribute(AttributeType::Autofocus)
        .with_css("flex-grow: 1; min-width: 0px;")
}

/// The bar at the foot of the panel: what was done, and Undo.
pub(crate) fn toast_dom(toast: &Toast, host: &Host) -> Dom {
    let mut bar = Dom::create_div()
        .with_id("__azreview_whiteboard-toast")
        .with_css(
            "position: absolute; left: 8px; right: 8px; bottom: 8px; display: flex; flex-direction: row; align-items: center; \
             padding: 4px 8px; background: #2d3340; border-radius: 4px; box-shadow: 0px 2px 8px rgba(0,0,0,0.3);",
        )
        .with_child(
            Dom::create_p_with_text(toast.text.as_str())
                .with_css("flex-grow: 1; margin: 0px; font-size: 11px; color: #ffffff; overflow: hidden;"),
        );
    if let Some(id) = &toast.undo {
        bar.add_child(button(
            &t("azreview-whiteboard-board-undo"),
            &t("azreview-whiteboard-board-undo"),
            tap(host, id),
            on_undo,
            "__azreview_whiteboard-toast-undo",
            "font-size: 11px; padding: 1px 8px; margin-left: 6px; flex-shrink: 0;",
        ));
    }
    bar
}

/// The words over the board while its file is read.
pub(crate) fn loading_dom() -> Dom {
    Dom::create_p_with_text(t("azreview-whiteboard-board-loading").as_str())
        .with_css("position: absolute; left: 16px; right: 16px; top: 24px; margin: 0px; color: #6b665e; font-size: 12px;")
}

// ---- the clicks

fn of(data: &mut RefAny) -> Option<(Host, String)> {
    data.downcast_ref::<Tap>().map(|t| (t.host.clone(), t.id.clone()))
}

/// Asks for `action`; the plan that comes of it goes to the host, a wait starts a save.
fn act(host: &Host, action: BoardAction, info: &mut CallbackInfo) -> Update {
    match host.run(|p| p.board_action(action.clone())) {
        Some(BoardOutcome::Done(plan)) => (host.on_plan)(&mut host.app.clone(), plan, info),
        Some(BoardOutcome::Wait) => (host.on_changed)(&mut host.app.clone(), info),
        Some(BoardOutcome::Told) | None => {}
    }
    Update::RefreshDom
}

extern "C" fn on_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((host, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    host.run(|p| {
        p.switcher.open = !p.switcher.open;
        p.switcher.renaming = None;
    });
    Update::RefreshDom
}

extern "C" fn on_new(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, BoardAction::New, &mut info)
}

extern "C" fn on_pick(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, id)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, BoardAction::Switch(id), &mut info)
}

extern "C" fn on_rename(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((host, id)) = of(&mut data) else {
        return Update::DoNothing;
    };
    host.run(|p| p.switcher.renaming = Some(id.clone()));
    Update::RefreshDom
}

extern "C" fn on_duplicate(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, BoardAction::Duplicate, &mut info)
}

extern "C" fn on_delete(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, id)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, BoardAction::Delete(id), &mut info)
}

extern "C" fn on_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, id)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, BoardAction::Move(id, -1), &mut info)
}

extern "C" fn on_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, id)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, BoardAction::Move(id, 1), &mut info)
}

extern "C" fn on_undo(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, id)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, BoardAction::Restore(id), &mut info)
}

extern "C" fn on_rename_text(_data: RefAny, _info: CallbackInfo, _state: TextInputState) -> OnTextInputReturn {
    OnTextInputReturn { update: Update::DoNothing, valid: TextInputValid::Yes }
}

/// Keeps the name that was typed (when this board is still the one being renamed).
fn commit(host: &Host, id: &str, text: &str, info: &mut CallbackInfo) -> Update {
    let renaming = host.run(|p| p.switcher.renaming.as_deref() == Some(id)).unwrap_or(false);
    if !renaming {
        return Update::DoNothing;
    }
    act(host, BoardAction::Rename(id.to_string(), text.to_string()), info)
}

extern "C" fn on_rename_key(mut data: RefAny, mut info: CallbackInfo, field: TextInputState) -> OnTextInputReturn {
    let Some((host, id)) = of(&mut data) else {
        return OnTextInputReturn { update: Update::DoNothing, valid: TextInputValid::Yes };
    };
    let update = match info.get_current_keyboard_state().current_virtual_keycode.into_option() {
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) => commit(&host, &id, field.get_text().as_str(), &mut info),
        Some(VirtualKeyCode::Escape) => {
            host.run(|p| p.switcher.renaming = None);
            Update::RefreshDom
        }
        _ => Update::DoNothing,
    };
    OnTextInputReturn { update, valid: TextInputValid::Yes }
}

extern "C" fn on_rename_blur(mut data: RefAny, mut info: CallbackInfo, field: TextInputState) -> Update {
    let Some((host, id)) = of(&mut data) else {
        return Update::DoNothing;
    };
    commit(&host, &id, field.get_text().as_str(), &mut info)
}

#[cfg(test)]
#[path = "whiteboard_boards_ui_test.rs"]
mod whiteboard_boards_ui_test;
