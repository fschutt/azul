//! The window: azul's S6 `RecordsShell` around the interpreted views.
//!
//! - the title row (`NoTitle` + appkit's `Titlebar`), the section tabs (the
//!   views' `menu`), the page (the routed view: a `table` view is azul's
//!   DataTable, a `detail` view a header with tabs and named panels, the
//!   reports / run / import pages their panels), the record form in the
//!   shell's form pane (`form` views) or a modal (`form_modal` views), the
//!   status bar;
//! - every change of the state queues its files ([`crate::app::State`]);
//!   [`pump`] hands the queue to azul-appkit's file thread one batch at a
//!   time, [`on_files`] hears the answers (and the load, and an import file);
//! - a CloseGuard holds a window close while a form has unsaved edits.
//!
//! On stdout, for scripts (`scripts/azerp_e2e.py`): `AZERP_READY <assets>`
//! when the records are in, `AZERP_PAGE <path>` after a navigation,
//! `AZERP_FORM <view>` when a form opens, `AZERP_SAVED <key>` /
//! `AZERP_REMOVED <key>` per file written, `AZERP_REFUSED <field>: <why>`
//! for a form that was not saved, `AZERP_EXPORTED <key>`.

pub mod detail;
pub mod form;
pub mod panels;
pub mod table;

use std::path::PathBuf;

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CloseGuardDirtyCheckCallbackType, CloseGuardOnEventCallbackType,
        TabOnClickCallbackType,
    },
    dom::VirtualKeyCode,
    prelude::*,
    shells::{RecordsShell, ShellEmptyState, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    widgets::{
        Button, ButtonType, CloseGuard, CloseGuardDocumentState, CloseGuardEvent, CloseGuardEventKind,
        DataTableView, StatusBar, StatusBarSegment, TabHeader, TabHeaderState,
    },
};
use azul_appkit::{
    files::{FileJob, FileOutcome},
    ui::{self as kit, Kit},
};
use azul_pim::write_queue::Write;

use crate::{
    app::State,
    ids, money, reports, store,
    views::{
        fill_path,
        spec::{self, ActionKind},
        Params, View, ViewKind,
    },
};

/// The write-back tags of the file jobs.
pub const TAG_LOAD: u64 = 1;
pub const TAG_WRITE: u64 = 2;
pub const TAG_IMPORT: u64 = 3;

/// The px the chrome takes over and under a page's table (title row, tab
/// row, the page's title and tools, the status bar).
pub const CHROME_HEIGHT: f32 = 168.0;

/// A column that fills the rest of its parent.
pub const COLUMN: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;";

/// The app: the kit, the state, what the window keeps between frames.
pub struct Erp {
    /// azul-appkit's kit (settings, data root, the settings page).
    pub kit: RefAny,
    pub state: State,
    /// The page table's view (sort, filter, scroll); reset with the page.
    pub table: DataTableView,
    /// A detail tab's table (the schedule, an embedded table).
    pub inner_table: DataTableView,
    /// The page the tables' views belong to.
    pub table_page: String,
    /// The path each row of the page table opens.
    pub row_paths: Vec<String>,
    pub window: (f32, f32),
    /// The batch of writes the file thread has.
    pub in_flight: Vec<Write>,
    /// `--sample`: fill an empty register.
    pub sample: bool,
    /// A CSV file named on the command line, imported once the records are in.
    pub pending_import: Option<(String, String)>,
    /// The close guard asks "save the form?".
    pub asking: bool,
}

impl Erp {
    /// The app on `state`.
    #[must_use]
    pub fn new(kit: RefAny, state: State, sample: bool, window: (f32, f32)) -> Erp {
        let page = state.page.clone();
        Erp {
            kit,
            state,
            table: DataTableView::create(),
            inner_table: DataTableView::create(),
            table_page: page,
            row_paths: Vec::new(),
            window,
            in_flight: Vec::new(),
            sample,
            pending_import: None,
            asking: false,
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
        let mut new_id = mint;
        let n = s.state.seed_sample(&mut new_id);
        if n > 0 {
            println!("AZERP_SAMPLE {n}");
        }
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
        s.state.notice = format!("{} could not be saved: {e}", w.key());
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
                Ok(None) => s.state.notice = format!("{key} does not exist."),
                Err(e) => s.state.notice = e.clone(),
            }
        }
    }
}

// ==== The window ====

/// The window: the RecordsShell in the theme scope, the form, the guard.
pub extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
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
    if s.table_page != s.state.page {
        s.table = DataTableView::create();
        s.inner_table = DataTableView::create();
        s.table_page = s.state.page.clone();
    }
    let shell = if kit::settings_open(&s.kit) {
        RecordsShell::create(Dom::create_div(), kit::settings_page(&s.kit, Vec::new()))
    } else {
        let tabs = section_tabs(s, &app);
        let page = page(s, &app);
        let mut shell = RecordsShell::create(tabs, page);
        if let Some(pane) = form::side_pane(s, &app) {
            shell = shell.with_form(pane);
        }
        shell
    };
    let mut column = Dom::create_div().with_css(COLUMN).with_child(
        shell
            .office_shell()
            .with_title_row(kit::title_row(crate::SPEC.name))
            .with_status_bar(status_bar(s))
            .dom(),
    );
    if let Some(modal) = form::modal(s, &app) {
        column.add_child(modal);
    }
    // A form with edits is not lost to the close button: the guard asks.
    let content = if s.state.form.is_some() {
        CloseGuard::create(column, "the open form")
            .with_dirty_check(app.clone(), dirty_check as CloseGuardDirtyCheckCallbackType)
            .with_asking(s.asking)
            .with_on_event(app.clone(), on_guard as CloseGuardOnEventCallbackType)
            .dom()
    } else {
        column
    };
    ShellThemeScope::create(content)
        .with_accent(ShellThemeAccent::Slate)
        .body()
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app.clone(),
            on_key,
        )
}

/// The section tabs: the views' menu; the active one is the section the
/// page is in.
fn section_tabs(s: &Erp, app: &RefAny) -> Dom {
    let menu = &s.state.views.menu;
    let labels: Vec<AzString> = menu
        .iter()
        .map(|m| AzString::from(s.state.labels.get(&m.key)))
        .collect();
    let active = menu
        .iter()
        .enumerate()
        .filter(|(_, m)| s.state.page.starts_with(m.path.as_str()))
        .max_by_key(|(_, m)| m.path.len())
        .map_or(0, |(i, _)| i);
    Dom::create_div()
        .with_id(ids::TABS)
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 2px 8px 0px 8px;")
        .with_child(
            TabHeader::create(labels)
                .with_active_tab(active)
                .with_on_click(app.clone(), on_section as TabOnClickCallbackType)
                .dom(),
        )
}

/// The page: the routed view.
fn page(s: &mut Erp, app: &RefAny) -> Dom {
    if !s.state.loaded {
        return empty("Reading the asset register...", "The records are files in your data folder.");
    }
    let Some((view, params)) = s.state.route().map(|r| (r.view.clone(), r.params.clone())) else {
        return empty("There is nothing here.", &s.state.page);
    };
    match view.kind {
        ViewKind::Table | ViewKind::TableEmbedded => table::page(s, app, &view),
        ViewKind::Detail => detail::page(s, app, &view, &params),
        ViewKind::Report => panels::reports(s, app, &view),
        ViewKind::Wizard => panels::run(s, app, &view),
        ViewKind::Custom => panels::import(s, app, &view),
        _ => empty("This page has no screen yet.", &view.id),
    }
}

/// An empty state.
#[must_use]
pub fn empty(title: &str, detail: &str) -> Dom {
    ShellEmptyState::create(title)
        .with_icon("inventory_2")
        .with_detail(detail)
        .dom()
        .with_id(ids::EMPTY)
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

/// A page's title and its tool buttons (the view's `actions`).
#[must_use]
pub fn page_header(s: &Erp, app: &RefAny, view: &View, title: &str, params: &Params) -> Dom {
    let mut row = Dom::create_div()
        .with_id(ids::TOOLS)
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 8px 12px;")
        .with_child(
            text(title)
                .with_id(ids::PAGE_TITLE)
                .with_css("font-size: 16px; font-weight: 600; flex-grow: 1;"),
        );
    for action in spec::actions(&view.actions, &s.state.labels) {
        row.add_child(action_button(app, &action, params));
    }
    row
}

/// Runs a view action.
fn run_action(s: &mut Erp, kind: &ActionKind, params: &Params) {
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
                    s.state.delete_asset(&id);
                    println!("AZERP_PAGE {}", s.state.page);
                }
                _ => s.state.notice = format!("\"{name}\" is not built yet."),
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
    with_erp(&mut app, &mut info, |s, _info| run_action(s, &kind, &params))
}

extern "C" fn on_section(mut data: RefAny, mut info: CallbackInfo, state: TabHeaderState) -> Update {
    with_erp(&mut data, &mut info, |s, _info| {
        let path = s.state.views.menu.get(state.active_tab).map(|m| m.path.clone());
        if let Some(path) = path {
            s.state.open(&path);
            println!("AZERP_PAGE {}", s.state.page);
        }
    })
}

/// The status bar: the register's count and value, the writes, the notice.
fn status_bar(s: &Erp) -> Dom {
    let t = reports::totals(&s.state.book, s.state.today);
    let mut segments = vec![
        StatusBarSegment::create(AzString::from(format!("{} assets", t.count))).with_marker(ids::STATUS_COUNT),
        StatusBarSegment::create(AzString::from(format!(
            "Book value {}",
            money::format_amount(t.book_value)
        ))),
    ];
    let writing = s.state.queue.pending() + s.in_flight.len();
    if writing > 0 {
        segments.push(
            StatusBarSegment::create(AzString::from(format!("Saving {writing} files...")))
                .with_marker(ids::STATUS_SYNC),
        );
    }
    if !s.state.notice.is_empty() {
        segments.push(
            StatusBarSegment::create(AzString::from(s.state.notice.as_str())).with_marker(ids::STATUS_NOTICE),
        );
    }
    StatusBar::create(segments).dom()
}

/// The kit's keys (Mod+, settings, F1 shortcuts, Escape); Escape closes an
/// open form; Back (Alt+Left) goes back.
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
        Some(VirtualKeyCode::Escape) => with_erp(&mut data, &mut info, |s, _| s.state.cancel_form()),
        _ => Update::DoNothing,
    }
}

/// The close guard asks this when a close request arrives: does the form
/// hold edits now?
extern "C" fn dirty_check(mut data: RefAny, _info: CallbackInfo) -> CloseGuardDocumentState {
    match data.downcast_ref::<Erp>() {
        Some(s) if s.state.form.as_ref().is_some_and(|f| f.dirty) => CloseGuardDocumentState::Unsaved,
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
