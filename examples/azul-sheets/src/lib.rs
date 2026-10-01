//! AzSheets: a spreadsheet on the public azul API - Excel 2010's window on
//! the DocumentShell: the title row, the ribbon (FILE opens the backstage:
//! New / Open / Save / Save As / Export; HOME, INSERT, FORMULAS, DATA,
//! VIEW), the formula bar (the name box, fx and the cell's formula with
//! function autocomplete), the CellGrid widget, the sheet tabs and the
//! status bar (READY / CALCULATING, the selection's Average / Count / Sum,
//! the zoom).
//!
//! THE ENGINE is IronCalc behind the `SheetEngine` trait (`engine.rs`), on
//! its own thread with a 256 MB stack (`worker.rs`: IronCalc evaluates
//! recursively and a ~3000-cell dependency chain overflows a normal stack).
//! The UI talks to it by MESSAGES: a callback sends an `EngineMsg` (the
//! command plus the window it wants to see) and starts one short azul
//! `Thread` that waits for the `Reply` and writes it back; the UI thread
//! never blocks. Every reply carries a `Snapshot` of the window (the cells
//! in view with their text, input and style, widths, heights, frozen
//! panes, the selection's statistics), which the grid's data and style
//! callbacks read.
//!
//! FILES (the S3 split): a workbook is `sheets/<uuid>.xlsx` plus
//! `sheets/<uuid>.json` (title, zoom, sheet, cursor) in the data folder,
//! written and read through azul-storage's `Drive` (a `LocalDrive` rooted at
//! `AZSHEETS_DATA`, else `<user data dir>/Azlin`) from an azul `Thread`.
//! Exports go to `exports/` in the same folder.
//!
//! On stdout, for scripts: `AZSHEETS_READY`, `AZSHEETS_REPLY <seq> ok|err`,
//! `AZSHEETS_CELL <A1> <shown text>` (the active cell after a reply),
//! `AZSHEETS_STATS count=<n> sum=<s>`, `AZSHEETS_FROZEN <rows> <columns>`,
//! `AZSHEETS_SAVED <id>`, `AZSHEETS_OPENED <id>`, `AZSHEETS_EXPORTED <path>`,
//! `AZSHEETS_SHEETS <name>,..`.

pub mod args;
pub mod engine;
pub mod fake_engine;
pub mod functions;
pub mod ironcalc_engine;
pub mod model;
pub mod ops;
pub mod sample;
pub mod storage;
pub mod worker;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        mpsc::{Receiver, Sender},
        Arc,
    },
};

use azul::{
    callbacks::{
        BackstageOnNavSelectCallbackType, ButtonOnClickCallbackType,
        CellGridDataSourceCallbackType, CellGridOnEventCallbackType,
        CellGridStyleSourceCallbackType, RibbonOnTabClickCallbackType,
        TextInputOnTextInputCallbackType, TextInputOnVirtualKeyDownCallbackType,
    },
    css::DarkLightMode,
    dialog::{FileDialog, FileOpenResult},
    dom::{ButtonOnClickCallback, ClipboardContent},
    file::FilePath,
    option::{OptionColorU, OptionDarkLightMode, OptionFileTypeList, OptionRefAny, OptionString},
    pdf::Pdf,
    prelude::*,
    shells::{DocumentShell, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    vec::{BackstageNavItemVec, CellGridSizeVec, StyledTextRunVec},
    widgets::{
        Backstage, BackstageNavItem, Button, ButtonOnClick, CellGrid, CellGridCell,
        CellGridCellKind, CellGridCellRef, CellGridCellStyle, CellGridEditMode, CellGridEvent,
        CellGridEventKind, CellGridHorizontalAlign, CellGridRange, CellGridSize,
        CellGridVerticalAlign, CellGridView, OnTextInputReturn, Ribbon, RibbonAppButton,
        RibbonButton, RibbonColumn, RibbonGroup, RibbonItem, RibbonTab, StatusBar,
        StatusBarSegment, StatusBarZoom, TextInput, TextInputState, TextInputValid, Titlebar,
    },
    window::WindowDecorations,
};
use azul_storage::{local::LocalDrive, Drive};

pub use crate::args::Args;
use crate::{
    engine::{
        BorderPreset, CellAddr, CellArea, CellStyle, HAlign, SheetInfo, StylePatch, VAlign,
        LAST_COLUMN, LAST_ROW,
    },
    ironcalc_engine::IronCalcEngine,
    storage::Sidecar,
    worker::{Command, EngineMsg, Reply, Snapshot, ValueKind, ViewRequest},
};

/// The grid node's id (scripts focus it by it).
pub const GRID_ID: &str = "cell-grid";
/// The formula bar's field.
pub const FORMULA_ID: &str = "formula-bar";
/// The name box.
pub const NAME_BOX_ID: &str = "name-box";
/// The px the chrome around the grid takes (title row, ribbon, formula bar,
/// sheet tabs, status bar) - the rest of the window is the grid's viewport.
const CHROME_HEIGHT: f32 = 236.0;
/// The suggestions the formula bar offers at most.
const SUGGESTIONS: usize = 6;

// ==== The state ====

/// Which screen the window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Workbook,
    Backstage,
}

/// The backstage's panes, in the nav's order.
pub const BACKSTAGE_ITEMS: [&str; 8] = [
    "Info", "New", "Open", "Save", "Save As", "Export", "Close", "About",
];

/// A side panel over the grid's right edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    None,
    /// Insert Function: the catalogue by category.
    Functions,
    /// The Name Manager.
    Names,
    /// Find.
    Find,
    /// The chart placeholder (the engine has no charts).
    Chart,
}

/// What a message to the engine was for.
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    /// Nothing beyond the snapshot.
    Other,
    /// `Reply::found`: move the cursor there.
    Find,
    /// `Reply::area`: propose `=SUM(area)` in this cell's editor.
    SumRange(CellAddr),
    /// `Reply::count`: duplicates removed.
    RemoveDuplicates,
    /// `Reply::count`: rows hidden by the filter.
    Filter,
    /// The workbook `id` was loaded.
    Opened(String),
    /// The workbook was written (`AZSHEETS_SAVED`).
    Saved,
    /// The CSV was written.
    Exported,
}

/// The workbook on screen.
#[derive(Clone, Debug, PartialEq)]
pub struct DocInfo {
    /// The file id (`sheets/<id>.xlsx`).
    pub id: String,
    pub title: String,
    /// Changed since the last save.
    pub dirty: bool,
}

/// The snapshot the grid draws from, indexed: (row, column), 1-based, to
/// its cell; the styles already in the grid's terms.
#[derive(Debug, Default)]
pub struct ViewCache {
    pub snapshot: Snapshot,
    pub cells: HashMap<(i32, i32), usize>,
    pub styles: Vec<CellGridCellStyle>,
}

impl ViewCache {
    /// The cache of `snapshot`.
    #[must_use]
    pub fn of(snapshot: Snapshot) -> Self {
        let cells = snapshot
            .cells
            .iter()
            .enumerate()
            .map(|(i, c)| ((c.row, c.column), i))
            .collect();
        let styles = snapshot.styles.iter().map(grid_style).collect();
        Self {
            snapshot,
            cells,
            styles,
        }
    }

    /// The cell at `row`, `column` (1-based), if the snapshot holds it.
    #[must_use]
    pub fn cell(&self, row: i32, column: i32) -> Option<&worker::CellView> {
        self.cells
            .get(&(row, column))
            .and_then(|i| self.snapshot.cells.get(*i))
    }

    /// The input (formula or typed text) of a cell; "" when empty or not
    /// fetched.
    #[must_use]
    pub fn input(&self, row: i32, column: i32) -> String {
        self.cell(row, column)
            .map(|c| c.input.clone())
            .unwrap_or_default()
    }

    /// The text a cell shows.
    #[must_use]
    pub fn shown(&self, row: i32, column: i32) -> String {
        self.cell(row, column)
            .map(|c| c.formatted.clone())
            .unwrap_or_default()
    }
}

/// The app's state.
pub struct AppState {
    /// Where messages to the engine thread go (`None` when it could not
    /// start; the UI then says so).
    pub engine: Option<Sender<EngineMsg>>,
    /// The last message's number.
    pub seq: u64,
    /// The newest reply applied (older ones arriving late are dropped).
    pub applied: u64,
    /// Messages sent, not yet answered (CALCULATING while any).
    pub in_flight: usize,
    /// What the messages in flight were for, by number (a reply's extras
    /// mean something only for the command that asked).
    pub pending: HashMap<u64, Pending>,
    pub cache: Arc<ViewCache>,
    /// The sheet shown.
    pub sheet: u32,
    /// The grid's view: selection, scroll, edit, drag (0-based).
    pub view: CellGridView,
    /// Percent.
    pub zoom: u32,
    pub show_headers: bool,
    pub doc: DocInfo,
    pub screen: Screen,
    pub backstage_pane: usize,
    pub ribbon_tab: usize,
    /// The name box's text while the user types in it.
    pub name_box: Option<String>,
    /// A sheet tab being renamed: (sheet, text).
    pub renaming: Option<(u32, String)>,
    pub panel: Panel,
    /// The find field's text.
    pub find: String,
    /// A line for the user (an error, a result).
    pub message: String,
    /// The internal clipboard of the ribbon's Copy / Cut: the range, cut?,
    /// its inputs.
    pub clipboard: Option<(CellArea, bool, Vec<Vec<String>>)>,
    /// The data folder.
    pub data_root: PathBuf,
    /// The workbooks in the data folder (the backstage's Open list).
    pub workbooks: Vec<(String, Sidecar)>,
    /// The window size the last layout saw.
    pub window: (f32, f32),
}

impl AppState {
    /// A fresh state for the data folder `data_root`, nothing open yet.
    #[must_use]
    pub fn new(data_root: PathBuf) -> Self {
        Self {
            engine: None,
            seq: 0,
            applied: 0,
            in_flight: 0,
            pending: HashMap::new(),
            cache: Arc::new(ViewCache::default()),
            sheet: 0,
            view: CellGridView::create(),
            zoom: 100,
            show_headers: true,
            doc: DocInfo {
                id: storage::new_id(),
                title: String::from("Book1"),
                dirty: false,
            },
            screen: Screen::Workbook,
            backstage_pane: 0,
            ribbon_tab: 0,
            name_box: None,
            renaming: None,
            panel: Panel::None,
            find: String::new(),
            message: String::new(),
            clipboard: None,
            data_root,
            workbooks: Vec::new(),
            window: (1280.0, 800.0),
        }
    }

    /// The active cell, 1-based, on the sheet shown.
    #[must_use]
    pub fn active(&self) -> CellAddr {
        to_addr(self.sheet, self.view.active)
    }

    /// The current range, 1-based.
    #[must_use]
    pub fn current_area(&self) -> CellArea {
        to_area(self.sheet, self.view.current_range())
    }

    /// Every selected range, 1-based.
    #[must_use]
    pub fn areas(&self) -> Vec<CellArea> {
        self.view
            .ranges
            .as_ref()
            .iter()
            .map(|r| to_area(self.sheet, *r))
            .collect()
    }

    /// Rows and columns the grid shows at once (an estimate from the
    /// window and the default sizes; the overscan covers the rest).
    #[must_use]
    pub fn window_cells(&self) -> (i32, i32) {
        let zoom = self.zoom.max(10) as f32 / 100.0;
        let rows = ((self.window.1 - CHROME_HEIGHT).max(100.0) / (20.0 * zoom)).ceil();
        let columns = (self.window.0.max(200.0) / (64.0 * zoom)).ceil();
        #[allow(clippy::cast_possible_truncation)]
        (rows as i32, columns as i32)
    }

    /// The view request for the window in view and the selection.
    #[must_use]
    pub fn view_request(&self) -> ViewRequest {
        let (rows, columns) = self.window_cells();
        let snap = &self.cache.snapshot;
        let (row_spans, column_spans) = model::spans_to_fetch(
            i32::try_from(self.view.top_row).unwrap_or(LAST_ROW - 1) + 1,
            i32::try_from(self.view.left_column).unwrap_or(LAST_COLUMN - 1) + 1,
            rows,
            columns,
            snap.frozen,
            snap.extent,
        );
        ViewRequest {
            sheet: self.sheet,
            rows: row_spans,
            columns: column_spans,
            selection: self.areas(),
        }
    }

    /// Whether the snapshot already holds the window in view.
    #[must_use]
    pub fn window_fetched(&self) -> bool {
        let want = self.view_request();
        let snap = &self.cache.snapshot;
        snap.sheet == self.sheet
            && model::covers(&snap.rows, &want.rows)
            && model::covers(&snap.columns, &want.columns)
    }
}

/// A grid cell (0-based) as an engine address (1-based).
#[must_use]
pub fn to_addr(sheet: u32, cell: CellGridCellRef) -> CellAddr {
    CellAddr::new(
        sheet,
        i32::try_from(cell.row).unwrap_or(LAST_ROW - 1) + 1,
        i32::try_from(cell.column).unwrap_or(LAST_COLUMN - 1) + 1,
    )
}

/// A grid range (0-based) as an engine area (1-based).
#[must_use]
pub fn to_area(sheet: u32, range: CellGridRange) -> CellArea {
    let a = to_addr(sheet, range.first);
    let b = to_addr(sheet, range.last);
    CellArea::spanning(sheet, a.row, a.column, b.row, b.column)
}

/// An engine address as a grid cell.
#[must_use]
pub fn to_cell(at: CellAddr) -> CellGridCellRef {
    CellGridCellRef {
        row: u32::try_from(at.row - 1).unwrap_or(0),
        column: u32::try_from(at.column - 1).unwrap_or(0),
    }
}

/// "B7" for an engine address.
#[must_use]
pub fn a1(at: CellAddr) -> String {
    CellGrid::cell_label(to_cell(at)).as_str().to_string()
}

/// "B3:B6" for an engine area ("B3" for one cell).
#[must_use]
pub fn a1_area(area: CellArea) -> String {
    let first = a1(CellAddr::new(area.sheet, area.row, area.column));
    if area.width <= 1 && area.height <= 1 {
        return first;
    }
    let last = a1(CellAddr::new(area.sheet, area.last_row(), area.last_column()));
    format!("{first}:{last}")
}

/// What the name box names: "B7", "B3:D6", "Sheet2!A1" (`sheets` resolves
/// the sheet name) - the sheet and the 0-based range.
#[must_use]
pub fn parse_reference(text: &str, sheets: &[SheetInfo], current: u32) -> Option<(u32, CellGridRange)> {
    let text = text.trim();
    let (sheet, range) = match text.rsplit_once('!') {
        Some((name, range)) => {
            let name = name.trim_matches('\'');
            let index = sheets
                .iter()
                .position(|s| s.name.eq_ignore_ascii_case(name))?;
            (u32::try_from(index).ok()?, range)
        }
        None => (current, text),
    };
    let cell = |t: &str| CellGrid::parse_cell_label(AzString::from(t)).into_option();
    let range = match range.split_once(':') {
        Some((a, b)) => {
            let (a, b) = (cell(a)?, cell(b)?);
            CellGridRange {
                first: CellGridCellRef {
                    row: a.row.min(b.row),
                    column: a.column.min(b.column),
                },
                last: CellGridCellRef {
                    row: a.row.max(b.row),
                    column: a.column.max(b.column),
                },
            }
        }
        None => {
            let c = cell(range)?;
            CellGridRange { first: c, last: c }
        }
    };
    Some((sheet, range))
}

/// An engine colour ("#RRGGBB") as the grid's.
fn color_of(hex: &Option<String>) -> OptionColorU {
    match hex.as_deref().and_then(model::parse_hex) {
        Some((r, g, b)) => OptionColorU::Some(ColorU { r, g, b, a: 255 }),
        None => OptionColorU::None,
    }
}

/// An engine style in the grid's terms.
#[must_use]
pub fn grid_style(s: &CellStyle) -> CellGridCellStyle {
    #[allow(clippy::cast_precision_loss)]
    let font_size = if s.font_size > 0 && s.font_size != engine::DEFAULT_FONT_SIZE {
        // IronCalc's sizes are points; the grid's px at 96 dpi.
        s.font_size as f32 * 4.0 / 3.0
    } else {
        0.0
    };
    CellGridCellStyle {
        font_size,
        align: match s.h_align {
            HAlign::General => CellGridHorizontalAlign::General,
            HAlign::Left => CellGridHorizontalAlign::Left,
            HAlign::Center => CellGridHorizontalAlign::Center,
            HAlign::Right => CellGridHorizontalAlign::Right,
        },
        vertical_align: match s.v_align {
            VAlign::Bottom => CellGridVerticalAlign::Bottom,
            VAlign::Center => CellGridVerticalAlign::Center,
            VAlign::Top => CellGridVerticalAlign::Top,
        },
        fill: color_of(&s.fill),
        ink: color_of(&s.font_color),
        border_top: color_of(&s.borders.top),
        border_right: color_of(&s.borders.right),
        border_bottom: color_of(&s.borders.bottom),
        border_left: color_of(&s.borders.left),
        bold: s.bold,
        italic: s.italic,
        underline: s.underline,
        strike: s.strike,
        wrap: s.wrap,
    }
}

/// An engine value kind as the grid's cell kind.
#[must_use]
pub const fn grid_kind(kind: ValueKind) -> CellGridCellKind {
    match kind {
        ValueKind::Empty => CellGridCellKind::Empty,
        ValueKind::Number => CellGridCellKind::Number,
        ValueKind::Text => CellGridCellKind::Text,
        ValueKind::Boolean => CellGridCellKind::Boolean,
        ValueKind::Error => CellGridCellKind::Error,
    }
}

// ==== The engine: messages out, replies back ====

/// What the thread waiting for a reply does with it before handing it to
/// the UI: the file I/O a save or an export needs (through the `Drive`,
/// never on the UI thread).
enum Post {
    None,
    /// `sheets/<id>.xlsx` and `.json` from `Reply::saved`.
    Save {
        root: PathBuf,
        id: String,
        sidecar: Sidecar,
    },
    /// `exports/<name>.csv` from `Reply::csv`.
    Csv { root: PathBuf, name: String },
}

/// A waiting thread's start data, taken out once.
struct WaitInit {
    rx: Option<Receiver<Reply>>,
    post: Option<Post>,
}

/// A reply on its way to the UI, taken out once; `io` is what the post
/// step did (the id saved, the file written, or the error).
struct ReplyMsg {
    reply: Option<Reply>,
    io: Option<Result<String, String>>,
}

/// A name usable as a file name: letters, digits, '-' and '_'; the rest
/// becomes '-'.
#[must_use]
pub fn safe_name(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        String::from("workbook")
    } else {
        s
    }
}

/// The post step of a reply.
fn run_post(post: Post, reply: &Reply) -> Option<Result<String, String>> {
    match post {
        Post::None => None,
        Post::Save { root, id, sidecar } => {
            let bytes = reply.saved.as_ref()?;
            let drive = LocalDrive::new(root);
            Some(
                storage::save(&drive, &id, bytes, &sidecar)
                    .map(|()| id)
                    .map_err(|e| format!("Could not save the workbook: {e}")),
            )
        }
        Post::Csv { root, name } => {
            let csv = reply.csv.as_ref()?;
            let key = format!("exports/{name}.csv");
            let drive = LocalDrive::new(root.clone());
            Some(
                drive
                    .put(&key, csv.as_bytes())
                    .map(|()| root.join(&key).display().to_string())
                    .map_err(|e| format!("Could not export: {e}")),
            )
        }
    }
}

/// The waiting thread: blocks on the reply (this thread, never the UI),
/// does the post step, writes the reply back.
extern "C" fn wait_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let taken = init
        .downcast_mut::<WaitInit>()
        .and_then(|mut w| Some((w.rx.take()?, w.post.take().unwrap_or(Post::None))));
    let Some((rx, post)) = taken else {
        return;
    };
    let Ok(reply) = rx.recv() else {
        return; // the engine thread is gone; the UI notices on its next send
    };
    let io = run_post(post, &reply);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_reply,
        RefAny::new(ReplyMsg {
            reply: Some(reply),
            io,
        }),
    )));
}

/// Sends `command` to the engine with the window in view, and starts the
/// thread that waits for its reply.
fn send(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut AppState,
    command: Command,
    pending: Pending,
    post: Post,
) {
    let Some(engine) = s.engine.clone() else {
        s.message = String::from("The spreadsheet engine is not running.");
        return;
    };
    s.seq += 1;
    let (tx, rx) = std::sync::mpsc::channel();
    let msg = EngineMsg {
        seq: s.seq,
        command,
        view: s.view_request(),
        reply: tx,
    };
    if engine.send(msg).is_err() {
        s.engine = None;
        s.message = String::from("The spreadsheet engine stopped.");
        return;
    }
    s.in_flight += 1;
    s.pending.insert(s.seq, pending);
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(WaitInit {
                rx: Some(rx),
                post: Some(post),
            }),
            app.clone(),
            wait_thread,
        ),
    );
}

/// `send` for a command with no extras and no post step.
fn run(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState, command: Command) {
    let changes = !matches!(command, Command::Fetch | Command::Save | Command::ExportCsv { .. });
    if changes {
        s.doc.dirty = true;
    }
    send(info, app, s, command, Pending::Other, Post::None);
}

/// Fetches the window again if the snapshot does not hold it (a scroll, a
/// sheet switch) and nothing is on its way that will.
fn fetch_if_needed(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState) {
    if s.in_flight == 0 && !s.window_fetched() {
        send(info, app, s, Command::Fetch, Pending::Other, Post::None);
    }
}

/// The lines scripts read after a reply.
fn announce(s: &AppState) {
    let at = s.active();
    println!(
        "AZSHEETS_CELL {} {}",
        a1(at),
        s.cache.shown(at.row, at.column)
    );
    let stats = &s.cache.snapshot.stats;
    println!(
        "AZSHEETS_STATS count={} sum={}",
        stats.count,
        model::format_number(stats.sum)
    );
    let (fr, fc) = s.cache.snapshot.frozen;
    println!("AZSHEETS_FROZEN {fr} {fc}");
    let names: Vec<&str> = s.cache.snapshot.sheets.iter().map(|x| x.name.as_str()).collect();
    println!("AZSHEETS_SHEETS {}", names.join(","));
}

/// The cursor on `at`, the window scrolled to show it (a few rows of
/// context above).
fn go_to(s: &mut AppState, sheet: u32, range: CellGridRange) {
    s.sheet = sheet;
    let mut view = CellGridView::create();
    view.active = range.first;
    view.anchor = range.first;
    view.ranges = vec![range].into();
    let (rows, columns) = s.window_cells();
    let (rows, columns) = (u32::try_from(rows).unwrap_or(30), u32::try_from(columns).unwrap_or(12));
    if range.first.row >= view.top_row + rows.saturating_sub(2) {
        view.top_row = range.first.row.saturating_sub(3);
    }
    if range.first.column >= view.left_column + columns.saturating_sub(1) {
        view.left_column = range.first.column.saturating_sub(1);
    }
    s.view = view;
}

/// A reply arrives: the snapshot (if it is the newest), the command's
/// extras, the post step's outcome.
fn apply_reply(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState, reply: Reply, io: Option<Result<String, String>>) {
    s.in_flight = s.in_flight.saturating_sub(1);
    let pending = s.pending.remove(&reply.seq).unwrap_or(Pending::Other);
    println!(
        "AZSHEETS_REPLY {} {}",
        reply.seq,
        if reply.result.is_ok() { "ok" } else { "err" }
    );
    match &reply.result {
        Ok(()) => {}
        Err(e) => s.message = e.clone(),
    }
    match io {
        Some(Ok(text)) => match pending {
            Pending::Saved => {
                s.doc.dirty = false;
                s.message = format!("Saved \"{}\".", s.doc.title);
                println!("AZSHEETS_SAVED {text}");
            }
            Pending::Exported => {
                s.message = format!("Exported to {text}");
                println!("AZSHEETS_EXPORTED {text}");
            }
            _ => {}
        },
        Some(Err(e)) => s.message = e,
        None => {}
    }
    match &pending {
        Pending::Find => match reply.found {
            Some(at) => {
                let cell = to_cell(at);
                go_to(s, at.sheet, CellGridRange { first: cell, last: cell });
            }
            None => s.message = format!("\"{}\" was not found.", s.find),
        },
        Pending::SumRange(at) => {
            let formula = match reply.area {
                Some(area) => format!("=SUM({})", a1_area(area)),
                None => String::from("=SUM()"),
            };
            let cursor = if reply.area.is_some() {
                formula.chars().count()
            } else {
                5
            };
            let cell = to_cell(*at);
            if s.view.active == cell {
                s.view.edit_mode = CellGridEditMode::Edit;
                s.view.edit_text = AzString::from(formula);
                s.view.edit_cursor = u32::try_from(cursor).unwrap_or(0);
            }
        }
        Pending::RemoveDuplicates => {
            if let Some(n) = reply.count {
                s.message = format!("{n} duplicate row(s) removed.");
            }
        }
        Pending::Filter => {
            if let Some(n) = reply.count {
                s.message = format!("{n} row(s) hidden by the filter.");
            }
        }
        Pending::Opened(id) => {
            if reply.result.is_ok() {
                println!("AZSHEETS_OPENED {id}");
            }
        }
        Pending::Other | Pending::Saved | Pending::Exported => {}
    }
    if reply.seq >= s.applied {
        s.applied = reply.seq;
        let snapshot = reply.snapshot;
        if snapshot.sheet != s.sheet {
            // The sheet shown is gone (deleted): the engine clamped it.
            s.sheet = snapshot.sheet;
            s.view = CellGridView::create();
        }
        s.cache = Arc::new(ViewCache::of(snapshot));
        announce(s);
    }
    fetch_if_needed(info, app, s);
}

/// The writeback of a reply.
extern "C" fn on_reply(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let taken = msg
        .downcast_mut::<ReplyMsg>()
        .and_then(|mut m| Some((m.reply.take()?, m.io.take())));
    let Some((reply, io)) = taken else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    apply_reply(&mut info, &handle, &mut guard, reply, io);
    Update::RefreshDom
}

// ==== Storage jobs: listing, loading, importing, writing a file ====

/// A blocking storage call for a worker thread.
enum Job {
    /// The workbooks in the data folder.
    List { root: PathBuf },
    /// `sheets/<id>.xlsx` and its sidecar.
    Load { root: PathBuf, id: String },
    /// An `.xlsx` from anywhere on disk.
    Import { path: PathBuf },
    /// Bytes to `<root>/<key>` (the PDF export).
    Write { root: PathBuf, key: String, bytes: Vec<u8> },
}

/// What a job answers.
enum JobDone {
    Listed(Result<Vec<(String, Sidecar)>, String>),
    /// (id, bytes, sidecar, imported?)
    Loaded(Result<(String, Vec<u8>, Sidecar, bool), String>),
    Wrote(Result<String, String>),
}

struct JobInit {
    job: Option<Job>,
}

struct JobMsg {
    done: Option<JobDone>,
}

fn run_job(job: Job) -> JobDone {
    match job {
        Job::List { root } => JobDone::Listed(
            storage::list(&LocalDrive::new(root)).map_err(|e| format!("Could not list the workbooks: {e}")),
        ),
        Job::Load { root, id } => JobDone::Loaded(
            storage::load(&LocalDrive::new(root), &id)
                .map(|(bytes, sidecar)| (id.clone(), bytes, sidecar, false))
                .map_err(|e| format!("Could not open the workbook: {e}")),
        ),
        Job::Import { path } => JobDone::Loaded(
            std::fs::read(&path)
                .map(|bytes| {
                    let title = path
                        .file_stem()
                        .map_or_else(|| String::from("Imported"), |s| s.to_string_lossy().into_owned());
                    (storage::new_id(), bytes, Sidecar::titled(&title), true)
                })
                .map_err(|e| format!("Could not read {}: {e}", path.display())),
        ),
        Job::Write { root, key, bytes } => {
            let drive = LocalDrive::new(root.clone());
            JobDone::Wrote(
                drive
                    .put(&key, &bytes)
                    .map(|()| root.join(&key).display().to_string())
                    .map_err(|e| format!("Could not write {key}: {e}")),
            )
        }
    }
}

extern "C" fn job_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some(job) = init.downcast_mut::<JobInit>().and_then(|mut i| i.job.take()) else {
        return;
    };
    let done = run_job(job);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_job_done,
        RefAny::new(JobMsg { done: Some(done) }),
    )));
}

fn spawn_job(info: &mut CallbackInfo, app: &RefAny, job: Job) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(RefAny::new(JobInit { job: Some(job) }), app.clone(), job_thread),
    );
}

/// A job's answer.
extern "C" fn on_job_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(done) = msg.downcast_mut::<JobMsg>().and_then(|mut m| m.done.take()) else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    match done {
        JobDone::Listed(Ok(list)) => s.workbooks = list,
        JobDone::Listed(Err(e)) | JobDone::Wrote(Err(e)) | JobDone::Loaded(Err(e)) => s.message = e,
        JobDone::Wrote(Ok(path)) => {
            s.message = format!("Exported to {path}");
            println!("AZSHEETS_EXPORTED {path}");
        }
        JobDone::Loaded(Ok((id, bytes, sidecar, imported))) => {
            open_bytes(&mut info, &handle, s, id, bytes, sidecar, imported);
        }
    }
    Update::RefreshDom
}

/// Shows a loaded workbook: the engine loads the bytes, the view comes
/// from the sidecar.
fn open_bytes(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut AppState,
    id: String,
    bytes: Vec<u8>,
    sidecar: Sidecar,
    imported: bool,
) {
    s.doc = DocInfo {
        id: id.clone(),
        title: if sidecar.title.is_empty() {
            id.clone()
        } else {
            sidecar.title.clone()
        },
        dirty: imported,
    };
    s.zoom = sidecar.zoom.clamp(10, 400);
    s.screen = Screen::Workbook;
    let active = to_cell(CellAddr::new(sidecar.sheet, sidecar.active.0.max(1), sidecar.active.1.max(1)));
    go_to(s, sidecar.sheet, CellGridRange { first: active, last: active });
    s.view.top_row = u32::try_from(sidecar.top_left.0 - 1).unwrap_or(0);
    s.view.left_column = u32::try_from(sidecar.top_left.1 - 1).unwrap_or(0);
    let name = s.doc.title.clone();
    send(
        info,
        app,
        s,
        Command::Load { bytes, name },
        Pending::Opened(id),
        Post::None,
    );
}

/// The sidecar of the workbook on screen.
fn sidecar_of(s: &AppState) -> Sidecar {
    let at = s.active();
    Sidecar {
        title: s.doc.title.clone(),
        zoom: s.zoom,
        sheet: s.sheet,
        active: (at.row, at.column),
        top_left: (
            i32::try_from(s.view.top_row).unwrap_or(0) + 1,
            i32::try_from(s.view.left_column).unwrap_or(0) + 1,
        ),
        modified: storage::now_secs(),
    }
}

/// Saves the workbook on screen as `sheets/<id>.xlsx` (+ sidecar).
fn save(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState) {
    let post = Post::Save {
        root: s.data_root.clone(),
        id: s.doc.id.clone(),
        sidecar: sidecar_of(s),
    };
    send(info, app, s, Command::Save, Pending::Saved, post);
}
