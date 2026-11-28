//! The window: azul's widget family (`AppFrame`: the sidebar, the tab ribbon, the body, the Preview,
//! the status line) around the interpreted views.
//!
//! - [`frame`]: the frame, the form factor (desktop or phone), the settings page under a title row;
//! - [`sidebar`], [`location`], [`ribbon`], [`preview`], [`sheet`]: the family's widgets, each built
//!   from the app's model (`crate::nav` is the plain data they are made of);
//! - the page (the routed view: a `table` view is azul's DataTable, a `detail` view a slim header
//!   with tabs and named panels, the reports / run / import pages their panels, a `form` view the
//!   body itself, a `form_modal` view a modal; [`states`] are the designed empty states);
//! - every change of the state queues its files ([`crate::app::State`]); [`pump`] hands the queue
//!   to azul-appkit's file thread one batch at a time, [`on_files`] hears the answers (and the
//!   load, and an import file);
//! - a CloseGuard holds a window close while a form has unsaved edits.
//!
//! On stdout, for scripts (`scripts/azerp_e2e.py`, the scenario files): `AZERP_READY <assets>`
//! when the records are in, `AZERP_PAGE <path>` after a navigation, `AZERP_FORM <view>` when a form
//! opens, `AZERP_SELECTED <id>` when the selection moves to a record, `AZERP_SAVED <key>` /
//! `AZERP_REMOVED <key>` per file written, `AZERP_REFUSED <field>: <why>` for a form that was not
//! saved, `AZERP_EXPORTED <key>`, `AZERP_SAMPLE <n>` when the sample register was written,
//! `AZERP_ASK_DELETE <id>` when Delete asks "Delete A-0001?" (only its Delete button deletes).

pub mod detail;
pub mod form;
pub mod frame;
pub mod location;
pub mod panels;
pub mod preview;
pub mod ribbon;
pub mod sheet;
pub mod sidebar;
pub mod states;
pub mod table;

use std::path::PathBuf;

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, ModalOnCloseCallbackType, StandardDialogOnEventCallbackType,
    },
    dom::VirtualKeyCode,
    prelude::*,
    str::String as AzString,
    widgets::{
        Button, ButtonType, CloseGuardDocumentState, CloseGuardEvent, CloseGuardEventKind,
        DataTableView, FormFactor, MessageBox, MessageBoxKind, Modal, ModalState, SidebarState,
        StandardDialogEvent, StandardDialogEventKind, StatusBar, StatusBarSegment,
    },
};
use azul_appkit::{
    files::{FileJob, FileOutcome},
    l10n::{self, t, t_args, Arg},
    ui::{self as kit, Kit},
};
use azul_pim::write_queue::Write;

use crate::{
    app::State,
    follow::Follow,
    ids, metrics, money, reports,
    store::{self, Kind},
    views::{
        fill_path,
        spec::{self, ActionKind, ActionSpec},
        Params, ViewKind,
    },
};

/// The write-back tags of the file jobs.
pub const TAG_LOAD: u64 = 1;
pub const TAG_WRITE: u64 = 2;
pub const TAG_IMPORT: u64 = 3;

/// A column that fills the rest of its parent.
pub const COLUMN: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;";

/// The app: the kit, the state, what the window keeps between frames.
pub struct Erp {
    /// azul-appkit's kit (settings, data root, the settings page).
    pub kit: RefAny,
    pub state: State,
    /// The page table's view (sort, filter, scroll, selection); reset with the page.
    pub table: DataTableView,
    /// A detail tab's table (the schedule, an embedded table).
    pub inner_table: DataTableView,
    /// The page the tables' views belong to.
    pub table_page: String,
    /// The record kind the page's table lists.
    pub table_kind: Option<Kind>,
    /// The record id of each row of the page table (the app's row order).
    pub row_ids: Vec<String>,
    /// The path each row of the page table opens.
    pub row_paths: Vec<String>,
    /// The Preview and the selection follow the table's top edge: what the scroll has done.
    pub follow: Follow,
    pub window: (f32, f32),
    /// The batch of writes the file thread has.
    pub in_flight: Vec<Write>,
    /// `--sample`: fill an empty register.
    pub sample: bool,
    /// A CSV file named on the command line, imported once the records are in.
    pub pending_import: Option<(String, String)>,
    /// The close guard asks "save the form?".
    pub asking: bool,
    /// What is typed in a record field of the open form (its ReferencePicker
    /// filters by it): `(field, text)`.
    pub reference_query: Option<(String, String)>,
    /// Desktop or phone: `AZERP_FORM`, else the window decides.
    pub form: FormFactor,
    /// The sidebar: the rail, expanded, or the 16 px handle.
    pub sidebar: SidebarState,
    /// Whether the module's section of the sidebar is open.
    pub module_open: bool,
    /// Whether the Preview shows, and the size the drag left it at (0: the default).
    pub preview_open: bool,
    pub preview_size: f32,
    /// The ribbon's tab (0 is File, the location).
    pub ribbon_tab: usize,
    /// The phone's action sheet is open for the selected record.
    pub sheet_open: bool,
}

impl Erp {
    /// The app on `state`.
    #[must_use]
    pub fn new(kit: RefAny, state: State, sample: bool, window: (f32, f32)) -> Erp {
        let page = state.page.clone();
        let form = frame::form_of(std::env::var(frame::FORM_VAR).ok().as_deref());
        Erp {
            kit,
            state,
            table: DataTableView::create(),
            inner_table: DataTableView::create(),
            table_page: page,
            table_kind: None,
            row_ids: Vec::new(),
            row_paths: Vec::new(),
            follow: Follow::new(),
            window,
            in_flight: Vec::new(),
            sample,
            pending_import: None,
            asking: false,
            reference_query: None,
            form,
            sidebar: frame::initial_sidebar(form, frame::touch_platform()),
            module_open: true,
            preview_open: true,
            preview_size: 0.0,
            ribbon_tab: 0,
            sheet_open: false,
        }
    }

    /// The data root (the bucket's root, later).
    #[must_use]
    pub fn data_root(&self) -> PathBuf {
        let mut kit = self.kit.clone();
        kit.downcast_ref::<Kit>()
            .map(|k| k.data_root.clone())
            .unwrap_or_default()
    }
}

/// The room the page's table has now (`metrics`): the root, the sidebar's column, the Preview's size,
/// the ribbon and the status line.
#[must_use]
pub(crate) fn metrics_room(
    s: &Erp,
    root: (f32, f32),
    phone: bool,
    sidebar: f32,
    preview: bool,
) -> metrics::Room {
    let dragged = preview::clamped_size(s.preview_size);
    let preview_px = if !preview {
        0.0
    } else if dragged > 0.0 {
        dragged
    } else if phone {
        metrics::PREVIEW_PEEK
    } else {
        metrics::PREVIEW_WIDTH
    };
    metrics::Room {
        root,
        phone,
        sidebar,
        preview: preview_px,
        footer: true,
    }
}

/// A new record id: a version 4 UUID from azul-storage's one seed source.
#[must_use]
pub fn mint() -> String {
    azul_storage::ids::new_uuid()
}

/// Runs `f` on the app, then hands the queued writes to the file thread.
pub fn with_erp(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    f: impl FnOnce(&mut Erp, &mut CallbackInfo),
) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<Erp>() else {
        return Update::DoNothing;
    };
    f(&mut *guard, info);
    pump(&mut *guard, info, &app);
    Update::RefreshDom
}

/// Starts the next batch of writes when none is in flight.
pub fn pump(s: &mut Erp, info: &mut CallbackInfo, app: &RefAny) {
    let Some(batch) = s.state.queue.take() else {
        return;
    };
    let jobs: Vec<FileJob> = batch
        .iter()
        .map(|w| match w {
            Write::Put { key, bytes } => FileJob::Put {
                key: key.clone(),
                bytes: bytes.clone(),
            },
            Write::Delete { key } => FileJob::Delete { key: key.clone() },
        })
        .collect();
    s.in_flight = batch;
    let root = s.data_root();
    kit::spawn_file_jobs(info, &root, jobs, app.clone(), TAG_WRITE, on_files);
}

/// Reads every record file under `erp/` on the file thread.
pub fn spawn_load(info: &mut CallbackInfo, app: &RefAny, root: &std::path::Path) {
    kit::spawn_file_jobs(
        info,
        root,
        vec![FileJob::GetAll {
            prefix: store::prefix(),
            suffix: ".json".to_string(),
        }],
        app.clone(),
        TAG_LOAD,
        on_files,
    );
}

/// The file thread's answers: the load, a batch of writes, an import file.
pub extern "C" fn on_files(mut data: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    with_erp(&mut data, &mut info, |s, _info| match reply.tag {
        TAG_LOAD => loaded(s, &reply.outcomes),
        TAG_WRITE => written(s, &reply.outcomes),
        TAG_IMPORT => import_read(s, &reply.outcomes),
        _ => {}
    })
}

/// The records are in: the book, the sample (`--sample`), a pending import.
fn loaded(s: &mut Erp, outcomes: &[FileOutcome]) {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for outcome in outcomes {
        if let FileOutcome::GotAll {
            files: got, errors, ..
        } = outcome
        {
            files.extend(got.iter().cloned());
            for e in errors {
                eprintln!("[AzERP] {e}");
            }
        }
    }
    s.state.load(&files);
    for skipped in &s.state.skipped {
        eprintln!("[AzERP] {} left alone: {}", skipped.key, skipped.reason);
    }
    if s.sample {
        states::load_sample(s);
    }
    if let Some((name, text)) = s.pending_import.take() {
        s.state.start_import(&name, &text);
    }
    println!("AZERP_READY {}", s.state.book.assets.len());
}

/// A batch of writes landed (or some did not: kept for the next batch).
fn written(s: &mut Erp, outcomes: &[FileOutcome]) {
    let batch = std::mem::take(&mut s.in_flight);
    let mut failed = Vec::new();
    for outcome in outcomes {
        let (key, result, verb) = match outcome {
            FileOutcome::Put { key, result } => (key, result, "SAVED"),
            FileOutcome::Deleted { key, result } => (key, result, "REMOVED"),
            _ => continue,
        };
        match result {
            Ok(()) => println!("AZERP_{verb} {key}"),
            Err(e) => {
                println!("AZERP_SAVE_FAILED {key}");
                if let Some(w) = batch.iter().find(|w| w.key() == key.as_str()) {
                    failed.push((w.clone(), e.clone()));
                }
            }
        }
    }
    if let Some((w, e)) = failed.first() {
        s.state.notice = t_args(
            "azerp-not-saved",
            &[("key", Arg::from(w.key())), ("why", Arg::from(e))],
        );
    }
    s.state.queue.finish(failed);
}

/// The CSV file to import was read.
fn import_read(s: &mut Erp, outcomes: &[FileOutcome]) {
    for outcome in outcomes {
        if let FileOutcome::Got { key, result } = outcome {
            match result {
                Ok(Some(bytes)) => {
                    let text = String::from_utf8_lossy(bytes);
                    s.state.start_import(key, &text);
                    println!("AZERP_IMPORT_PREVIEW {key}");
                }
                Ok(None) => {
                    s.state.notice = t_args("azerp-file-missing", &[("key", Arg::from(key))])
                }
                Err(e) => s.state.notice = e.clone(),
            }
        }
    }
}

// ==== The window ====

/// The window: the frame in the theme scope, the form, the guard.
pub extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // The window's language for every word this pass says.
    l10n::begin_layout(&info);
    // Reading the mode makes a light / dark switch rebuild the window.
    let _mode = info.get_mode();
    let window = (info.get_window_width(), info.get_window_height());
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<Erp>() else {
        return Dom::create_body();
    };
    let s = &mut *guard;
    if window.0 > 0.0 && window.1 > 0.0 {
        s.window = window;
    }
    if s.state.form.is_none() {
        s.reference_query = None;
    }
    if s.table_page != s.state.page {
        s.table = DataTableView::create();
        s.inner_table = DataTableView::create();
        s.table_page = s.state.page.clone();
        s.follow.reset();
        s.sheet_open = false;
    }
    frame::window(s, &app, &info)
}

/// The page: the routed view.
fn page(s: &mut Erp, app: &RefAny, room: &metrics::Room, phone: bool, keyboard: f32) -> Dom {
    if !s.state.loaded {
        return states::loading();
    }
    // an open `form` view is the body itself
    if let Some(form) = form::page(s, app, keyboard) {
        return form;
    }
    let Some((view, params)) = s.state.route().map(|r| (r.view.clone(), r.params.clone())) else {
        return states::quiet("construction", &t("azerp-nothing-here"), &s.state.page);
    };
    match view.kind {
        ViewKind::Table | ViewKind::TableEmbedded => table::page(s, app, &view, room, phone),
        ViewKind::Detail => detail::page(s, app, &view, &params, room, phone),
        ViewKind::Report => panels::reports(s, room),
        ViewKind::Wizard => panels::run(s, app, &view),
        ViewKind::Custom => panels::import(s, app),
        _ => states::no_screen(&view),
    }
}

/// The commands of the page the window shows: its view's actions (a form's Save and Cancel when a
/// form is open over it are the form's own: this is the page beneath).
#[must_use]
pub(crate) fn page_actions(s: &Erp) -> Vec<ActionSpec> {
    s.state
        .route()
        .map(|r| spec::actions(&r.view.actions, &s.state.labels))
        .unwrap_or_default()
}

/// An empty state: an icon, one line and a line of detail.
#[must_use]
pub fn empty(title: &str, detail: &str) -> Dom {
    states::quiet("inventory_2", title, detail)
}

/// "Delete A-0001?": azul's `MessageBox` (a question: Delete, Cancel - the default) in a
/// `Modal`, while the state waits for the answer. Only Delete deletes.
fn delete_question(s: &Erp, app: &RefAny) -> Option<Dom> {
    let (number, name) = s.state.delete_question()?;
    let message = MessageBox::create(
        MessageBoxKind::Question,
        t_args("azerp-delete-ask", &[("number", Arg::from(&number))]),
        t_args(
            "azerp-delete-what",
            &[("number", Arg::from(&number)), ("name", Arg::from(&name))],
        ),
    )
    .with_detail(t("azerp-delete-detail"))
    .with_buttons(
        vec![
            AzString::from(t("azerp-delete")),
            AzString::from(t("kit-button-cancel")),
        ],
        1,
    )
    .with_on_event(
        app.clone(),
        on_delete_answer as StandardDialogOnEventCallbackType,
    );
    Some(
        Modal::create(message.dom())
            .with_title(crate::SPEC.name)
            .with_open(true)
            .with_on_close(app.clone(), on_delete_dismissed as ModalOnCloseCallbackType)
            .dom()
            .with_id(ids::CONFIRM_DELETE),
    )
}

/// The answer to "Delete ...?": the first button deletes, any other keeps the asset.
extern "C" fn on_delete_answer(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: StandardDialogEvent,
) -> Update {
    let yes = matches!(event.kind, StandardDialogEventKind::Button) && event.index == 0;
    with_erp(&mut data, &mut info, |s, _info| {
        s.state.answer_delete(yes);
        if yes {
            println!("AZERP_PAGE {}", s.state.page);
        }
    })
}

/// The question was closed (Escape, the close button): nothing is deleted.
extern "C" fn on_delete_dismissed(
    mut data: RefAny,
    mut info: CallbackInfo,
    _state: ModalState,
) -> Update {
    with_erp(&mut data, &mut info, |s, _info| {
        s.state.answer_delete(false)
    })
}

/// A column of `children`.
#[must_use]
pub fn column(children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css(COLUMN)
        .with_children(DomVec::from_vec(children))
}

/// A run of text.
#[must_use]
pub fn text(content: &str) -> Dom {
    Dom::create_span_with_text(content)
}

/// A problem's ink: flat's red; under flora the clay stone, by day and by night (flat's red
/// reads at 2.8:1 on flora's night leaf).
pub const PROBLEM_INK: &str = "color: system:danger;";
/// Quiet text (a field's label, a figure's caption, a subtitle): flat fades the ink; under
/// flora it is the label ink, unfaded.
pub const QUIET_FLORA: &str = "@theme(flora) { opacity: 1; color: system:secondary-text; }";
/// A heading inside a panel under flora: flora's label (capitals in the label ink).
pub const LABEL_FLORA: &str = "@theme(flora) { font-size: 0.6875rem; font-weight: bold; \
                               text-transform: uppercase; letter-spacing: 0.12em; color: \
                               system:secondary-text; }";
/// A status pill: a quiet badge; under flora flora.css's `.pill` at the house radius, its edge the
/// theme's, its label in the label ink's capitals.
pub const PILL_CSS: &str = "padding: 2px 8px; border-radius: 10px; font-size: 0.75rem; border: 1px solid #8888; \
                            @theme(flora) { border-radius: 3px; border: 1px solid system:separator; \
                            color: system:secondary-text; font-size: 0.6875rem; font-weight: bold; \
                            text-transform: uppercase; letter-spacing: 0.07em; }";

/// What a button made from a view action carries.
pub struct ActionRef {
    pub app: RefAny,
    pub kind: ActionKind,
    /// The page's parameters (`:id` of the record the page shows).
    pub params: Params,
}

/// A button for a view action.
#[must_use]
pub fn action_button(app: &RefAny, action: &spec::ActionSpec, params: &Params) -> Dom {
    let name = match &action.kind {
        ActionKind::Named(name) => name.clone(),
        _ => action.label.clone(),
    };
    let mut button = Button::create(action.label.as_str());
    if !action.icon.is_empty() {
        button = button.with_icon(action.icon.as_str());
    }
    if action.primary {
        button = button.with_button_type(ButtonType::Primary);
    }
    button
        .with_on_click(
            RefAny::new(ActionRef {
                app: app.clone(),
                kind: action.kind.clone(),
                params: params.clone(),
            }),
            on_action as ButtonOnClickCallbackType,
        )
        .dom()
        .with_id(ids::action(&name))
        .with_css("margin-left: 6px;")
}

/// Runs a view action.
pub(crate) fn run_action(s: &mut Erp, kind: &ActionKind, params: &Params) {
    match kind {
        ActionKind::Link(pattern) => {
            let path = fill_path(pattern, params);
            s.state.open(&path);
            match &s.state.form {
                Some(draft) => println!("AZERP_FORM {}", draft.view),
                None => println!("AZERP_PAGE {}", s.state.page),
            }
        }
        ActionKind::Named(name) => {
            let id = params.get("id").cloned().unwrap_or_default();
            match name.as_str() {
                "export_csv" => println!("AZERP_EXPORTED {}", s.state.export_register()),
                "export_schedule" => {
                    if let Some(key) = s.state.export_schedule(&id) {
                        println!("AZERP_EXPORTED {key}");
                    }
                }
                "check_in" => s.state.check_in(&id),
                "delete" => {
                    s.state.ask_delete(&id);
                    println!("AZERP_ASK_DELETE {id}");
                }
                _ => s.state.notice = t_args("azerp-not-built", &[("name", Arg::from(name))]),
            }
        }
        ActionKind::Submit => {
            let mut new_id = mint;
            if s.state.save_form(&mut new_id) {
                println!("AZERP_SAVED_FORM {}", s.state.page);
            } else if let Some(draft) = &s.state.form {
                for (field, why) in &draft.problems {
                    println!("AZERP_REFUSED {field}: {why}");
                }
            }
        }
        ActionKind::Cancel => s.state.cancel_form(),
    }
}

extern "C" fn on_action(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, kind, params)) = data
        .downcast_ref::<ActionRef>()
        .map(|a| (a.app.clone(), a.kind.clone(), a.params.clone()))
    else {
        return Update::DoNothing;
    };
    with_erp(&mut app, &mut info, |s, _info| {
        run_action(s, &kind, &params)
    })
}

/// The status line: the register's count and value, the writes, the notice.
fn status_bar(s: &Erp) -> Dom {
    let totals = reports::totals(&s.state.book, s.state.today);
    let mut segments = vec![
        StatusBarSegment::create(AzString::from(t_args(
            "azerp-status-assets",
            &[("count", Arg::from(totals.count))],
        )))
        .with_marker(ids::STATUS_COUNT),
        StatusBarSegment::create(AzString::from(t_args(
            "azerp-status-book-value",
            &[("value", Arg::from(money::format_amount(totals.book_value)))],
        ))),
    ];
    let writing = s.state.queue.pending() + s.in_flight.len();
    if writing > 0 {
        segments.push(
            StatusBarSegment::create(AzString::from(t_args(
                "azerp-status-saving",
                &[("count", Arg::from(writing))],
            )))
            .with_marker(ids::STATUS_SYNC),
        );
    }
    if !s.state.notice.is_empty() {
        segments.push(
            StatusBarSegment::create(AzString::from(s.state.notice.as_str()))
                .with_marker(ids::STATUS_NOTICE),
        );
    }
    StatusBar::create(segments).dom().with_id(ids::STATUS)
}

/// One step back: what Escape undoes - and a phone's Back, which the shell sends as an Escape to a
/// window that claims it: the open sheet, then the open form, then the page before. `true` = it did
/// something, so the key is the app's.
fn back_step(s: &mut Erp) -> bool {
    if s.sheet_open {
        s.sheet_open = false;
        return true;
    }
    if s.state.form.is_some() {
        s.state.cancel_form();
        return true;
    }
    if s.state.confirm_delete.is_some() {
        return false;
    }
    if !s.state.back.is_empty() {
        s.state.go_back();
        println!("AZERP_PAGE {}", s.state.page);
        return true;
    }
    false
}

/// The kit's keys (Mod+, settings, F1 shortcuts, Escape); Escape takes one step back (the
/// sheet, the form, the page before).
extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(kit_ref) = data.downcast_ref::<Erp>().map(|s| s.kit.clone()) else {
        return Update::DoNothing;
    };
    if let Some(update) = kit::handle_key(&kit_ref, &mut info) {
        return update;
    }
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    match key {
        Some(VirtualKeyCode::Escape) => {
            let stepped = data
                .downcast_mut::<Erp>()
                .is_some_and(|mut s| back_step(&mut *s));
            if stepped {
                info.prevent_default();
                Update::RefreshDom
            } else {
                Update::DoNothing
            }
        }
        _ => Update::DoNothing,
    }
}

/// The close guard asks this when a close request arrives: does the form
/// hold edits now?
extern "C" fn dirty_check(mut data: RefAny, _info: CallbackInfo) -> CloseGuardDocumentState {
    match data.downcast_ref::<Erp>() {
        Some(s) if s.state.form.as_ref().is_some_and(|f| f.dirty) => {
            CloseGuardDocumentState::Unsaved
        }
        _ => CloseGuardDocumentState::Saved,
    }
}

/// The answer to "save the form?".
extern "C" fn on_guard(mut data: RefAny, mut info: CallbackInfo, event: CloseGuardEvent) -> Update {
    with_erp(&mut data, &mut info, |s, _| match event.kind {
        CloseGuardEventKind::Ask => s.asking = true,
        CloseGuardEventKind::Save => {
            s.asking = false;
            let mut new_id = mint;
            s.state.save_form(&mut new_id);
        }
        CloseGuardEventKind::Discard => {
            s.asking = false;
            s.state.cancel_form();
        }
        CloseGuardEventKind::Cancel => s.asking = false,
    })
}
