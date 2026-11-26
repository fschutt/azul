//! The header of the whiteboard's panel: the board that is open (its name is a button), "new board", and the list of
//! boards as a popover (open one, rename, duplicate, delete, move up and down, export as a JSON file, import one); a
//! deleted board is told in a bar at the foot of the panel with Undo. Export asks first (a small sheet: what the file
//! holds, the author name it will carry, where the pictures go) and then hands the host the board to write
//! ([`Plan::export`]); import asks the host for a file ([`Plan::import`]) and takes what it read as a new board
//! ([`Panel::import_loaded`]). What the clicks ask of the boards is [`Panel::board_action`], which is done at once
//! or, when the open board is not kept yet, when it is ([`BoardOutcome::Wait`], [`Panel::after_wait`]). The window's part
//! is the [`Plan`] the host gets ([`crate::whiteboard_ui::Host::on_plan`]): the list to write, a copy to write, a board
//! to read, a board to export, a file to import.

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
    whiteboard_export::ExportPictures,
    whiteboard_store::Loaded,
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
    /// The sheet that asks how to export the open board.
    OpenExport,
    /// Where the pictures of the export go.
    Pictures(ExportPictures),
    CloseExport,
    /// Export the open board as the sheet says: the host asks where.
    Export,
    /// Open a file of a board as a new board: the host asks for the file.
    Import,
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
    /// The export sheet, open: where the pictures go.
    pub export: Option<ExportPictures>,
    /// A file that was read and waits for the open board to be kept before it becomes a board of its own.
    pub pending_import: Option<Box<Loaded>>,
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
            BoardAction::OpenExport => {
                self.switcher.export = Some(ExportPictures::default());
                Ok(Plan::default())
            }
            BoardAction::Pictures(p) => {
                if self.switcher.export.is_some() {
                    self.switcher.export = Some(*p);
                }
                Ok(Plan::default())
            }
            BoardAction::CloseExport => {
                self.switcher.export = None;
                Ok(Plan::default())
            }
            BoardAction::Export => Ok(match self.switcher.export.take() {
                Some(pictures) => Plan { export: Some(self.wb.export_request(pictures, None, now)), ..Plan::default() },
                None => Plan::default(),
            }),
            BoardAction::Import => Ok(Plan { import: true, ..Plan::default() }),
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
        if let Some(loaded) = self.switcher.pending_import.take() {
            return Some(self.import_loaded(*loaded));
        }
        let action = self.switcher.pending.take()?;
        Some(self.board_action(action))
    }

    /// A file of a board was read: it becomes a new board of the workspace, named for what the file calls it, and is
    /// open. The open board must be kept first: when it is not, the file waits for it ([`BoardOutcome::Wait`]).
    pub fn import_loaded(&mut self, loaded: Loaded) -> BoardOutcome {
        if self.wb.can_leave().is_err() {
            self.switcher.pending_import = Some(Box::new(loaded));
            return BoardOutcome::Wait;
        }
        let now = self.now_ms();
        let fallback = t("azreview-whiteboard-board-imported");
        let result = self.wb.import_board(loaded, &fallback, now);
        self.collect_notices();
        match result {
            Ok(plan) => {
                self.switcher.open = false;
                self.switcher.renaming = None;
                BoardOutcome::Done(plan)
            }
            Err(n) => {
                self.hint = Some(say(&n));
                BoardOutcome::Told
            }
        }
    }

    /// A file could not be exported or imported: what the person is told (the sentence is in their language).
    pub fn tell(&mut self, sentence: String) {
        self.hint = Some(sentence);
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
        "flex-grow: 1; min-width: 0px; text-align: left; font-size: 12px; font-weight: bold; overflow: hidden; white-space: nowrap;",
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
             padding: 0px 6px; background: system:window-background; border-bottom: 1px solid system:separator;"
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
                 flex-direction: column; padding: 4px; background: system:window-background; border: 1px solid system:separator; \
                 box-shadow: 0px 3px 10px rgba(0,0,0,0.25);",
                HEADER_PX + 2.0
            )
            .as_str(),
        );
    for (n, b) in shown.iter().enumerate() {
        let is_open = b.id == open_id;
        // A board is two lines: its name (a button that opens it) over what can be done to it, so that the bar's width never
        // squeezes a name out; the open one is marked with a tint that reads on a light and on a dark ground.
        let mut row = Dom::create_div().with_css(
            format!(
                "display: flex; flex-direction: column; padding: 2px 2px 3px 2px; margin-bottom: 2px; background: {};",
                if is_open { "rgba(32, 92, 214, 0.22)" } else { "transparent" }
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
                "min-width: 0px; text-align: left; font-size: 12px; overflow: hidden; white-space: nowrap;",
            ));
            let small = |key: &str, cb: ButtonOnClickCallbackType, suffix: &str| {
                button(&t(key), &t(key), tap(host, &b.id), cb, &format!("__azreview_whiteboard-board-{}-{suffix}", b.id), SMALL)
            };
            let mut actions = Dom::create_div().with_css("display: flex; flex-direction: row; justify-content: flex-end; margin-top: 2px;");
            actions.add_child(small("azreview-whiteboard-board-rename", on_rename, "rename"));
            if is_open {
                actions.add_child(small("azreview-whiteboard-board-duplicate", on_duplicate, "duplicate"));
            }
            actions.add_child(small("azreview-whiteboard-board-delete", on_delete, "delete"));
            if n > 0 {
                actions.add_child(small("azreview-whiteboard-board-up", on_up, "up"));
            }
            if n < last {
                actions.add_child(small("azreview-whiteboard-board-down", on_down, "down"));
            }
            row.add_child(actions);
        }
        list.add_child(row);
    }
    list.with_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; justify-content: flex-end; padding: 4px 2px 0px 2px; margin-top: 2px; border-top: 1px solid system:separator;")
            .with_child(button(
                &t("azreview-whiteboard-export"),
                &t("azreview-whiteboard-export-name"),
                tap(host, ""),
                on_export_open,
                "__azreview_whiteboard-board-export",
                SMALL,
            ))
            .with_child(button(
                &t("azreview-whiteboard-import"),
                &t("azreview-whiteboard-import-name"),
                tap(host, ""),
                on_import,
                "__azreview_whiteboard-board-import",
                SMALL,
            )),
    )
}

/// The sheet that asks how a board is exported: what the file holds, the name it carries as its author (the setting says
/// which, and the sheet says it, so that nobody publishes a name without having been told), where the pictures go.
pub(crate) fn export_dialog_dom(panel: &Panel, host: &Host, pictures: ExportPictures) -> Dom {
    let name = panel.wb.active_board().1.to_string();
    let author = match panel.wb.author_name() {
        Some(a) => t_args("azreview-whiteboard-export-author", &[("author", Arg::from(a))]),
        None => t("azreview-whiteboard-export-author-none"),
    };
    let paragraph = |text: String| Dom::create_p_with_text(text.as_str()).with_css("margin: 0px 0px 6px 0px; font-size: 11px; color: system:text;");
    let choice = |key: &str, chosen: bool, cb: ButtonOnClickCallbackType, id: &str| {
        button(
            &format!("{} {}", if chosen { "\u{25c9}" } else { "\u{25cb}" }, t(key)),
            &t(key),
            tap(host, ""),
            cb,
            id,
            "font-size: 11px; padding: 2px 6px; margin-right: 6px; text-align: left;",
        )
    };
    Dom::create_div()
        .with_id("__azreview_whiteboard-export")
        .with_css(
            format!(
                "position: absolute; left: 6px; right: 6px; top: {}px; display: flex; flex-direction: column; padding: 8px; \
                 background: system:window-background; border: 1px solid system:separator; box-shadow: 0px 3px 10px rgba(0,0,0,0.25);",
                HEADER_PX + 2.0
            )
            .as_str(),
        )
        .with_child(
            Dom::create_p_with_text(t_args("azreview-whiteboard-export-title", &[("name", Arg::from(name))]).as_str())
                .with_css("margin: 0px 0px 6px 0px; font-size: 12px; font-weight: bold; color: system:text;"),
        )
        .with_child(paragraph(t("azreview-whiteboard-export-what")))
        .with_child(paragraph(author))
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; margin-bottom: 6px;")
                .with_child(choice("azreview-whiteboard-export-pictures-inside", pictures == ExportPictures::Embedded, on_pictures_inside, "__azreview_whiteboard-export-inside"))
                .with_child(choice("azreview-whiteboard-export-pictures-beside", pictures == ExportPictures::Beside, on_pictures_beside, "__azreview_whiteboard-export-beside")),
        )
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; justify-content: flex-end;")
                .with_child(button(
                    &t("azreview-whiteboard-export-go"),
                    &t("azreview-whiteboard-export-go"),
                    tap(host, ""),
                    on_export_go,
                    "__azreview_whiteboard-export-go",
                    "font-size: 11px; padding: 2px 10px; margin-right: 6px;",
                ))
                .with_child(button(
                    &t("azreview-whiteboard-export-cancel"),
                    &t("azreview-whiteboard-export-cancel"),
                    tap(host, ""),
                    on_export_cancel,
                    "__azreview_whiteboard-export-cancel",
                    "font-size: 11px; padding: 2px 10px;",
                )),
        )
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

extern "C" fn on_export_open(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, BoardAction::OpenExport, &mut info)
}

extern "C" fn on_export_cancel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, BoardAction::CloseExport, &mut info)
}

extern "C" fn on_export_go(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, BoardAction::Export, &mut info)
}

extern "C" fn on_pictures_inside(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, BoardAction::Pictures(ExportPictures::Embedded), &mut info)
}

extern "C" fn on_pictures_beside(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, BoardAction::Pictures(ExportPictures::Beside), &mut info)
}

extern "C" fn on_import(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, BoardAction::Import, &mut info)
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
