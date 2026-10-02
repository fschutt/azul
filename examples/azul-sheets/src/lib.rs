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
        ShellSettingsLayoutOnCategoryCallbackType, TextInputOnTextInputCallbackType,
        TextInputOnVirtualKeyDownCallbackType,
    },
    css::{DarkLightMode, HoverEventFilter},
    dialog::{FileDialog, FileOpenResult},
    dom::VirtualKeyCode,
    file::FilePath,
    option::{OptionColorU, OptionDarkLightMode, OptionFileTypeList, OptionString},
    pdf::Pdf,
    prelude::*,
    shells::{
        DocumentShell, ShellSettingsLayout, ShellSettingsSection, ShellThemeAccent,
        ShellThemeScope,
    },
    str::String as AzString,
    vec::{BackstageNavItemVec, CellGridRangeVec, CellGridSizeVec, StringVec},
    widgets::{
        Backstage, BackstageNavItem, Button, CellGrid, CellGridCell,
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
pub const BACKSTAGE_ITEMS: [&str; 9] = [
    "Info", "New", "Open", "Save", "Save As", "Export", "Close", "Options", "About",
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
    /// The command line, until the window's startup has acted on it.
    pub args: Option<Args>,
    /// The Options page's category.
    pub settings_category: usize,
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
            args: None,
            settings_category: 0,
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
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    pub fn window_cells(&self) -> (i32, i32) {
        let zoom = self.zoom.max(10) as f32 / 100.0;
        let rows = ((self.window.1 - CHROME_HEIGHT).max(100.0) / (20.0 * zoom)).ceil();
        let columns = (self.window.0.max(200.0) / (64.0 * zoom)).ceil();
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
    view.ranges = CellGridRangeVec::from_vec(vec![range]);
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
    apply_reply(&mut info, &handle, &mut *guard, reply, io);
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
    s.zoom = sidecar.zoom.clamp(ZOOM_MIN, ZOOM_MAX);
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

// ==== The grid's data and looks ====

/// What the grid's data and style callbacks read: the snapshot, shared.
struct GridSource {
    cache: Arc<ViewCache>,
}

/// The cell `cell` (0-based) of the snapshot.
fn source_cell(data: &mut RefAny, cell: CellGridCellRef) -> Option<(String, ValueKind, CellGridCellStyle)> {
    let src = data.downcast_ref::<GridSource>()?;
    let row = i32::try_from(cell.row).ok()? + 1;
    let column = i32::try_from(cell.column).ok()? + 1;
    let v = src.cache.cell(row, column)?;
    let style = src
        .cache
        .styles
        .get(v.style as usize)
        .cloned()
        .unwrap_or_else(CellGridCellStyle::create);
    Some((v.formatted.clone(), v.kind, style))
}

extern "C" fn cell_data(mut data: RefAny, cell: CellGridCellRef) -> CellGridCell {
    match source_cell(&mut data, cell) {
        Some((text, kind, _)) => CellGridCell {
            text: AzString::from(text),
            kind: grid_kind(kind),
        },
        None => CellGridCell {
            text: AzString::from(""),
            kind: CellGridCellKind::Empty,
        },
    }
}

extern "C" fn cell_look(mut data: RefAny, cell: CellGridCellRef) -> CellGridCellStyle {
    source_cell(&mut data, cell).map_or_else(CellGridCellStyle::create, |(_, _, style)| style)
}

/// The most common of `sizes` (the sheet's default), else `fallback`.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn common_size(sizes: &[(i32, f64)], fallback: f32) -> f32 {
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for (_, px) in sizes {
        let key = (*px * 10.0).round() as i64;
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(k, n)| (*n, *k))
        .map_or(fallback, |(k, _)| k as f32 / 10.0)
}

/// The sizes that differ from `default`, as the grid's overrides.
fn size_overrides(sizes: &[(i32, f64)], default: f32) -> CellGridSizeVec {
    #[allow(clippy::cast_possible_truncation)]
    let v: Vec<CellGridSize> = sizes
        .iter()
        .filter(|(_, px)| (*px as f32 - default).abs() > 0.05)
        .map(|(i, px)| CellGridSize {
            size: *px as f32,
            index: u32::try_from(*i - 1).unwrap_or(0),
        })
        .collect();
    CellGridSizeVec::from_vec(v)
}

fn grid(s: &AppState, app: &RefAny) -> Dom {
    let snap = &s.cache.snapshot;
    let column_default = common_size(&snap.column_widths, 100.0);
    let row_default = common_size(&snap.row_heights, 25.0);
    let name = snap
        .sheets
        .get(s.sheet as usize)
        .map_or_else(|| String::from("Sheet"), |x| x.name.clone());
    let source = RefAny::new(GridSource {
        cache: Arc::clone(&s.cache),
    });
    CellGrid::create(
        u32::try_from(LAST_ROW).unwrap_or(1_048_576),
        u32::try_from(LAST_COLUMN).unwrap_or(16_384),
    )
    .with_id(AzString::from(GRID_ID))
    .with_accessibility_name(AzString::from(name))
    .with_view(s.view.clone())
    .with_viewport(s.window.0, (s.window.1 - CHROME_HEIGHT).max(120.0))
    .with_zoom(s.zoom as f32 / 100.0)
    .with_default_sizes(column_default, row_default)
    .with_column_widths(size_overrides(&snap.column_widths, column_default))
    .with_row_heights(size_overrides(&snap.row_heights, row_default))
    .with_frozen(
        u32::try_from(snap.frozen.0).unwrap_or(0),
        u32::try_from(snap.frozen.1).unwrap_or(0),
    )
    .with_content_extent(
        u32::try_from(snap.extent.0).unwrap_or(0),
        u32::try_from(snap.extent.1).unwrap_or(0),
    )
    .with_show_grid_lines(snap.grid_lines)
    .with_show_headers(s.show_headers)
    .with_data_source(source.clone(), cell_data as CellGridDataSourceCallbackType)
    .with_style_source(source, cell_look as CellGridStyleSourceCallbackType)
    .with_on_event(app.clone(), on_grid_event as CellGridOnEventCallbackType)
    .dom()
}

// ==== The ribbon ====

/// A command of the ribbon (and of the keyboard shortcuts).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Undo,
    Redo,
    Paste,
    Cut,
    Copy,
    Bold,
    Italic,
    Underline,
    Strike,
    Grow,
    Shrink,
    InkRed,
    InkAuto,
    FillYellow,
    FillGreen,
    FillNone,
    AlignLeft,
    AlignCenter,
    AlignRight,
    AlignTop,
    AlignMiddle,
    AlignBottom,
    Wrap,
    FormatGeneral,
    FormatNumber,
    FormatCurrency,
    FormatPercent,
    FormatDate,
    DecimalMore,
    DecimalLess,
    StyleHeading,
    StyleTotal,
    StyleGood,
    StyleBad,
    BordersAll,
    BordersOutline,
    BordersNone,
    InsertRow,
    InsertColumn,
    DeleteRow,
    DeleteColumn,
    InsertSheet,
    DeleteSheet,
    RenameSheet,
    SheetLeft,
    SheetRight,
    TabColor,
    AutoSum,
    FillDown,
    FillRight,
    SortAsc,
    SortDesc,
    Filter,
    ClearFilter,
    RemoveDuplicates,
    Find,
    ClearContents,
    ClearFormats,
    Chart,
    InsertFunction,
    NameManager,
    DefineName,
    CalculateNow,
    FreezePanes,
    FreezeTopRow,
    FreezeFirstColumn,
    Unfreeze,
    Gridlines,
    Headings,
    ZoomIn,
    ZoomOut,
    Zoom100,
    ThemeFlat,
    ThemeFlora,
    ModeLight,
    ModeDark,
    Save,
    ExportCsv,
    ExportPdf,
}

/// A ribbon button's click data.
struct ActionRef {
    app: RefAny,
    action: Action,
}

fn action_button(app: &RefAny, icon: &str, label: &str, action: Action, toggled: bool) -> RibbonButton {
    RibbonButton::create(AzString::from(icon), AzString::from(label))
        .with_toggled(toggled)
        .with_on_click(
            RefAny::new(ActionRef {
                app: app.clone(),
                action,
            }),
            on_action as ButtonOnClickCallbackType,
        )
}

fn large(app: &RefAny, icon: &str, label: &str, action: Action) -> RibbonItem {
    RibbonItem::LargeButton(action_button(app, icon, label, action, false))
}

fn small(app: &RefAny, icon: &str, label: &str, action: Action) -> RibbonItem {
    RibbonItem::SmallButton(action_button(app, icon, label, action, false))
}

fn toggle(app: &RefAny, icon: &str, label: &str, action: Action, on: bool) -> RibbonItem {
    RibbonItem::SmallButton(action_button(app, icon, label, action, on))
}

fn column(items: Vec<RibbonItem>) -> RibbonItem {
    RibbonItem::Column(RibbonColumn::create().with_items(items.into()))
}

fn group(label: &str, items: Vec<RibbonItem>) -> RibbonGroup {
    RibbonGroup::create(AzString::from(label)).with_items(items.into())
}

fn tab(label: &str, groups: Vec<RibbonGroup>) -> RibbonTab {
    RibbonTab::create(AzString::from(label)).with_groups(groups.into())
}

fn ribbon(s: &AppState, app: &RefAny) -> Dom {
    let style = s
        .cache
        .cell(s.active().row, s.active().column)
        .and_then(|c| s.cache.snapshot.styles.get(c.style as usize))
        .cloned()
        .unwrap_or_default();
    let (fr, fc) = s.cache.snapshot.frozen;
    let home = tab(
        "HOME",
        vec![
            group(
                "Clipboard",
                vec![
                    large(app, "content_paste", "Paste", Action::Paste),
                    column(vec![
                        small(app, "content_cut", "Cut", Action::Cut),
                        small(app, "content_copy", "Copy", Action::Copy),
                        small(app, "undo", "Undo", Action::Undo),
                    ]),
                    column(vec![small(app, "redo", "Redo", Action::Redo)]),
                ],
            ),
            group(
                "Font",
                vec![
                    column(vec![
                        toggle(app, "format_bold", "Bold", Action::Bold, style.bold),
                        toggle(app, "format_italic", "Italic", Action::Italic, style.italic),
                        toggle(app, "format_underlined", "Underline", Action::Underline, style.underline),
                    ]),
                    column(vec![
                        toggle(app, "format_strikethrough", "Strikethrough", Action::Strike, style.strike),
                        small(app, "text_increase", "Grow font", Action::Grow),
                        small(app, "text_decrease", "Shrink font", Action::Shrink),
                    ]),
                    column(vec![
                        small(app, "format_color_text", "Red text", Action::InkRed),
                        small(app, "format_color_reset", "Automatic text", Action::InkAuto),
                        small(app, "format_color_fill", "Yellow fill", Action::FillYellow),
                    ]),
                    column(vec![
                        small(app, "format_color_fill", "Green fill", Action::FillGreen),
                        small(app, "format_color_reset", "No fill", Action::FillNone),
                    ]),
                ],
            ),
            group(
                "Alignment",
                vec![
                    column(vec![
                        toggle(app, "vertical_align_top", "Top", Action::AlignTop, style.v_align == VAlign::Top),
                        toggle(app, "vertical_align_center", "Middle", Action::AlignMiddle, style.v_align == VAlign::Center),
                        toggle(app, "vertical_align_bottom", "Bottom", Action::AlignBottom, style.v_align == VAlign::Bottom),
                    ]),
                    column(vec![
                        toggle(app, "format_align_left", "Left", Action::AlignLeft, style.h_align == HAlign::Left),
                        toggle(app, "format_align_center", "Center", Action::AlignCenter, style.h_align == HAlign::Center),
                        toggle(app, "format_align_right", "Right", Action::AlignRight, style.h_align == HAlign::Right),
                    ]),
                    column(vec![toggle(app, "wrap_text", "Wrap text", Action::Wrap, style.wrap)]),
                ],
            ),
            group(
                "Number",
                vec![
                    column(vec![
                        small(app, "notes", "General", Action::FormatGeneral),
                        small(app, "pin", "Number", Action::FormatNumber),
                        small(app, "payments", "Currency", Action::FormatCurrency),
                    ]),
                    column(vec![
                        small(app, "percent", "Percent", Action::FormatPercent),
                        small(app, "calendar_today", "Date", Action::FormatDate),
                    ]),
                    column(vec![
                        small(app, "add", "More decimals", Action::DecimalMore),
                        small(app, "remove", "Fewer decimals", Action::DecimalLess),
                    ]),
                ],
            ),
            group(
                "Styles",
                vec![
                    column(vec![
                        small(app, "title", "Heading", Action::StyleHeading),
                        small(app, "functions", "Total", Action::StyleTotal),
                    ]),
                    column(vec![
                        small(app, "thumb_up", "Good", Action::StyleGood),
                        small(app, "thumb_down", "Bad", Action::StyleBad),
                    ]),
                    column(vec![
                        small(app, "border_all", "All borders", Action::BordersAll),
                        small(app, "border_outer", "Outside borders", Action::BordersOutline),
                        small(app, "border_clear", "No borders", Action::BordersNone),
                    ]),
                ],
            ),
            group(
                "Cells",
                vec![
                    column(vec![
                        small(app, "table_rows", "Insert row", Action::InsertRow),
                        small(app, "view_column", "Insert column", Action::InsertColumn),
                        small(app, "add_box", "Insert sheet", Action::InsertSheet),
                    ]),
                    column(vec![
                        small(app, "delete_sweep", "Delete row", Action::DeleteRow),
                        small(app, "delete", "Delete column", Action::DeleteColumn),
                        small(app, "delete_forever", "Delete sheet", Action::DeleteSheet),
                    ]),
                    column(vec![
                        small(app, "drive_file_rename_outline", "Rename sheet", Action::RenameSheet),
                        small(app, "arrow_back", "Move sheet left", Action::SheetLeft),
                        small(app, "arrow_forward", "Move sheet right", Action::SheetRight),
                    ]),
                    column(vec![small(app, "palette", "Tab color", Action::TabColor)]),
                ],
            ),
            group(
                "Editing",
                vec![
                    large(app, "functions", "AutoSum", Action::AutoSum),
                    column(vec![
                        small(app, "arrow_downward", "Fill down", Action::FillDown),
                        small(app, "arrow_forward", "Fill right", Action::FillRight),
                        small(app, "backspace", "Clear contents", Action::ClearContents),
                    ]),
                    column(vec![
                        small(app, "sort_by_alpha", "Sort A to Z", Action::SortAsc),
                        small(app, "filter_alt", "Filter", Action::Filter),
                        small(app, "search", "Find", Action::Find),
                    ]),
                ],
            ),
        ],
    );
    let insert = tab(
        "INSERT",
        vec![
            group("Charts", vec![large(app, "insert_chart", "Chart", Action::Chart)]),
            group("Functions", vec![large(app, "function", "Function", Action::InsertFunction)]),
        ],
    );
    let formulas = tab(
        "FORMULAS",
        vec![
            group(
                "Function Library",
                vec![
                    large(app, "function", "Insert Function", Action::InsertFunction),
                    large(app, "functions", "AutoSum", Action::AutoSum),
                ],
            ),
            group(
                "Defined Names",
                vec![
                    large(app, "badge", "Name Manager", Action::NameManager),
                    column(vec![small(app, "label", "Define name", Action::DefineName)]),
                ],
            ),
            group(
                "Calculation",
                vec![large(app, "calculate", "Calculate Now", Action::CalculateNow)],
            ),
        ],
    );
    let data = tab(
        "DATA",
        vec![
            group(
                "Sort & Filter",
                vec![
                    column(vec![
                        small(app, "arrow_upward", "Sort A to Z", Action::SortAsc),
                        small(app, "arrow_downward", "Sort Z to A", Action::SortDesc),
                    ]),
                    large(app, "filter_alt", "Filter", Action::Filter),
                    column(vec![small(app, "filter_alt_off", "Clear filter", Action::ClearFilter)]),
                ],
            ),
            group(
                "Data Tools",
                vec![large(app, "cleaning_services", "Remove Duplicates", Action::RemoveDuplicates)],
            ),
            group(
                "Export",
                vec![
                    large(app, "description", "CSV", Action::ExportCsv),
                    large(app, "picture_as_pdf", "PDF", Action::ExportPdf),
                ],
            ),
        ],
    );
    let view = tab(
        "VIEW",
        vec![
            group(
                "Show",
                vec![column(vec![
                    toggle(app, "grid_on", "Gridlines", Action::Gridlines, s.cache.snapshot.grid_lines),
                    toggle(app, "view_headline", "Headings", Action::Headings, s.show_headers),
                ])],
            ),
            group(
                "Window",
                vec![
                    large(app, "view_compact", "Freeze Panes", Action::FreezePanes),
                    column(vec![
                        toggle(app, "vertical_align_top", "Freeze top row", Action::FreezeTopRow, fr == 1 && fc == 0),
                        toggle(app, "border_left", "Freeze first column", Action::FreezeFirstColumn, fr == 0 && fc == 1),
                        small(app, "grid_off", "Unfreeze panes", Action::Unfreeze),
                    ]),
                ],
            ),
            group(
                "Zoom",
                vec![column(vec![
                    small(app, "zoom_in", "Zoom in", Action::ZoomIn),
                    small(app, "zoom_out", "Zoom out", Action::ZoomOut),
                    small(app, "fit_screen", "100%", Action::Zoom100),
                ])],
            ),
            group(
                "Look",
                vec![
                    column(vec![
                        small(app, "crop_square", "Flat", Action::ThemeFlat),
                        small(app, "spa", "Flora", Action::ThemeFlora),
                    ]),
                    column(vec![
                        small(app, "light_mode", "Light", Action::ModeLight),
                        small(app, "dark_mode", "Dark", Action::ModeDark),
                    ]),
                ],
            ),
        ],
    );
    Ribbon::create(vec![home, insert, formulas, data, view])
        .with_app_button(
            RibbonAppButton::create(AzString::from("FILE"))
                .with_on_click(app.clone(), on_file as ButtonOnClickCallbackType),
        )
        .with_active_tab(s.ribbon_tab)
        .with_on_tab_click(app.clone(), on_ribbon_tab as RibbonOnTabClickCallbackType)
        .dom_desktop()
}

// ==== The formula bar ====

/// Text inputs reporting to the app with their purpose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    NameBox,
    Formula,
    Find,
    Rename,
}

struct FieldRef {
    app: RefAny,
    field: Field,
}

fn field(app: &RefAny, which: Field, text: &str, placeholder: &str) -> TextInput {
    let data = || {
        RefAny::new(FieldRef {
            app: app.clone(),
            field: which,
        })
    };
    TextInput::create()
        .with_text(AzString::from(text))
        .with_placeholder(AzString::from(placeholder))
        .with_on_text_input(data(), on_field_text as TextInputOnTextInputCallbackType)
        .with_on_virtual_key_down(data(), on_field_key as TextInputOnVirtualKeyDownCallbackType)
}

/// A function the autocomplete offers, clicked.
struct SuggestionRef {
    app: RefAny,
    name: &'static str,
}

fn formula_bar(s: &AppState, app: &RefAny) -> Dom {
    let at = s.active();
    let name = s.name_box.clone().unwrap_or_else(|| {
        let r = s.current_area();
        if r.width > 1 || r.height > 1 {
            a1_area(r)
        } else {
            a1(at)
        }
    });
    let formula = if s.view.edit_mode == CellGridEditMode::None {
        s.cache.input(at.row, at.column)
    } else {
        s.view.edit_text.as_str().to_string()
    };
    let name_box = field(app, Field::NameBox, &name, "Name box")
        .with_accessibility_name(AzString::from("Name box"))
        .dom()
        .with_id(AzString::from(NAME_BOX_ID))
        .with_css("width: 110px; flex-grow: 0; margin-right: 6px;");
    let fx = Button::create(AzString::from("fx"))
        .with_on_click(
            RefAny::new(ActionRef {
                app: app.clone(),
                action: Action::InsertFunction,
            }),
            on_action as ButtonOnClickCallbackType,
        )
        .dom()
        .with_css("flex-grow: 0; margin-right: 6px;");
    let input = field(app, Field::Formula, &formula, "")
        .with_accessibility_name(AzString::from("Formula bar"))
        .dom()
        .with_id(AzString::from(FORMULA_ID))
        .with_css("flex-grow: 1; min-width: 0px;");
    let bar = Dom::create_div()
        .with_id(AzString::from("formula-row"))
        .with_css(
            "display: flex; flex-direction: row; align-items: center; flex-grow: 0; padding: 4px \
             8px; border-bottom: 1px solid rgba(128, 128, 128, 0.35);",
        )
        .with_child(name_box)
        .with_child(fx)
        .with_child(input);
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 0;")
        .with_child(bar);
    // Autocomplete: the functions whose name starts with what is typed.
    if s.view.edit_mode != CellGridEditMode::None {
        let text = s.view.edit_text.as_str();
        if let Some((_, prefix)) = functions::typed_name(text, s.view.edit_cursor as usize) {
            let found = functions::by_prefix(&prefix, SUGGESTIONS);
            if !found.is_empty() {
                let mut row = Dom::create_div()
                    .with_id(AzString::from("formula-suggestions"))
                    .with_css(
                        "display: flex; flex-direction: row; align-items: center; flex-grow: 0; \
                         padding: 2px 8px; font-size: 12px;",
                    );
                for f in found {
                    row.add_child(
                        Button::with_type(AzString::from(f.signature()), azul::widgets::ButtonType::Link)
                            .with_on_click(
                                RefAny::new(SuggestionRef {
                                    app: app.clone(),
                                    name: f.name,
                                }),
                                on_suggestion as ButtonOnClickCallbackType,
                            )
                            .dom()
                            .with_css("margin-right: 8px;"),
                    );
                }
                column.add_child(row);
            }
        }
    }
    column
}

// ==== The sheet tabs ====

struct TabRef {
    app: RefAny,
    sheet: u32,
}

fn sheet_tabs(s: &AppState, app: &RefAny) -> Dom {
    let mut row = Dom::create_div().with_id(AzString::from("sheet-tabs")).with_css(
        "display: flex; flex-direction: row; align-items: center; flex-grow: 0; padding: 2px 8px; \
         border-top: 1px solid rgba(128, 128, 128, 0.35);",
    );
    row.add_child(
        Button::create(AzString::from(""))
            .with_icon(AzString::from("add"))
            .with_on_click(
                RefAny::new(ActionRef {
                    app: app.clone(),
                    action: Action::InsertSheet,
                }),
                on_action as ButtonOnClickCallbackType,
            )
            .dom()
            .with_css("flex-grow: 0; margin-right: 6px;"),
    );
    for (i, info) in s.cache.snapshot.sheets.iter().enumerate() {
        let sheet = u32::try_from(i).unwrap_or(0);
        if info.hidden {
            continue;
        }
        let underline = info
            .color
            .as_deref()
            .and_then(model::parse_hex)
            .map_or_else(String::new, |(r, g, b)| {
                format!("border-bottom: 3px solid rgb({r}, {g}, {b});")
            });
        if let Some((renaming, text)) = &s.renaming {
            if *renaming == sheet {
                row.add_child(
                    field(app, Field::Rename, text, "Sheet name")
                        .with_accessibility_name(AzString::from("Sheet name"))
                        .dom()
                        .with_id(AzString::from("sheet-rename"))
                        .with_css("width: 140px; flex-grow: 0; margin-right: 4px;"),
                );
                continue;
            }
        }
        let kind = if sheet == s.sheet {
            azul::widgets::ButtonType::Primary
        } else {
            azul::widgets::ButtonType::Default
        };
        let data = RefAny::new(TabRef {
            app: app.clone(),
            sheet,
        });
        row.add_child(
            Button::with_type(AzString::from(info.name.as_str()), kind)
                .with_on_click(data.clone(), on_tab_click as ButtonOnClickCallbackType)
                .dom()
                .with_css(format!("flex-grow: 0; margin-right: 4px; {underline}"))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::DoubleClick),
                    data,
                    on_tab_double_click,
                ),
        );
    }
    row
}

// ==== The status bar ====

/// The zoom range in percent: what the zoom buttons reach and what the
/// status bar's slider spans.
const ZOOM_MIN: u32 = 10;
/// See [`ZOOM_MIN`].
const ZOOM_MAX: u32 = 400;

fn zoom_action(app: &RefAny, action: Action) -> RefAny {
    RefAny::new(ActionRef {
        app: app.clone(),
        action,
    })
}

fn status_bar(s: &AppState, app: &RefAny) -> Dom {
    let state = if s.in_flight > 0 { "CALCULATING..." } else { "READY" };
    let mut segments = vec![StatusBarSegment::create(AzString::from(state))];
    if !s.message.is_empty() {
        segments.push(StatusBarSegment::create(AzString::from(s.message.as_str())));
    }
    for text in model::stats_segments(&s.cache.snapshot.stats) {
        segments.push(StatusBarSegment::create(AzString::from(text)));
    }
    // The slider spans the buttons' whole range: a fixed 10..190 window
    // pinned a 400 % zoom's thumb to its end (DEDUP_OFFICE D28).
    let zoom = StatusBarZoom::create(s.zoom as f32, ZOOM_MIN as f32, ZOOM_MAX as f32)
        .with_on_zoom_out(zoom_action(app, Action::ZoomOut), on_action as ButtonOnClickCallbackType)
        .with_on_zoom_in(zoom_action(app, Action::ZoomIn), on_action as ButtonOnClickCallbackType);
    StatusBar::create(segments).with_zoom(zoom).dom()
}

// ==== The side panel ====

/// A function picked in the Insert Function panel.
struct FunctionRef {
    app: RefAny,
    name: &'static str,
}

/// A defined name to delete.
struct NameRef {
    app: RefAny,
    index: usize,
}

fn panel(s: &AppState, app: &RefAny) -> Option<Dom> {
    let frame = |title: &str| {
        Dom::create_div()
            .with_id(AzString::from("side-panel"))
            .with_css(
                "display: flex; flex-direction: column; flex-grow: 0; width: 280px; padding: 8px; \
                 border-left: 1px solid rgba(128, 128, 128, 0.35); overflow-y: auto;",
            )
            .with_child(Dom::create_p_with_text(AzString::from(title)).with_css("font-weight: 600; margin: 4px 0px 8px 0px;"))
    };
    let close = || {
        Button::create(AzString::from("Close"))
            .with_on_click(app.clone(), on_panel_close as ButtonOnClickCallbackType)
            .dom()
            .with_css("flex-grow: 0; margin-top: 8px;")
    };
    let line = |text: &str| Dom::create_p_with_text(AzString::from(text)).with_css("font-size: 12px; margin: 2px 0px;");
    match s.panel {
        Panel::None => None,
        Panel::Functions => {
            let mut p = frame("Insert Function");
            for category in functions::Category::ALL {
                p.add_child(line(category.label()).with_css("font-weight: 600; margin-top: 6px;"));
                for f in functions::in_category(category) {
                    p.add_child(
                        Button::with_type(AzString::from(f.signature()), azul::widgets::ButtonType::Link)
                            .with_on_click(
                                RefAny::new(FunctionRef {
                                    app: app.clone(),
                                    name: f.name,
                                }),
                                on_insert_function as ButtonOnClickCallbackType,
                            )
                            .dom(),
                    );
                }
            }
            Some(p.with_child(close()))
        }
        Panel::Names => {
            let mut p = frame("Name Manager");
            if s.cache.snapshot.names.is_empty() {
                p.add_child(line("No names yet. Select a range, type a name in the name box, then Define name."));
            }
            for (i, n) in s.cache.snapshot.names.iter().enumerate() {
                p.add_child(line(&format!("{}  {}", n.name, n.formula)));
                p.add_child(
                    Button::with_type(AzString::from("Delete"), azul::widgets::ButtonType::Link)
                        .with_on_click(
                            RefAny::new(NameRef {
                                app: app.clone(),
                                index: i,
                            }),
                            on_delete_name as ButtonOnClickCallbackType,
                        )
                        .dom(),
                );
            }
            Some(p.with_child(close()))
        }
        Panel::Find => {
            let mut p = frame("Find");
            p.add_child(
                field(app, Field::Find, &s.find, "Find what")
                    .with_accessibility_name(AzString::from("Find what"))
                    .dom()
                    .with_id(AzString::from("find-field")),
            );
            p.add_child(line("Enter finds the next cell."));
            Some(p.with_child(close()))
        }
        Panel::Chart => {
            let mut p = frame("Charts");
            p.add_child(line(
                "Charts are not in the spreadsheet engine yet (IronCalc has none); they will be drawn \
                 by the app from a range.",
            ));
            Some(p.with_child(close()))
        }
    }
}

// ==== The backstage ====

/// A workbook of the Open list, clicked.
struct OpenRef {
    app: RefAny,
    index: usize,
}

fn backstage(s: &AppState, app: &RefAny) -> Dom {
    let items: Vec<BackstageNavItem> = BACKSTAGE_ITEMS
        .iter()
        .map(|label| {
            let item = BackstageNavItem::create(AzString::from(*label));
            if *label == "Options" {
                item.with_gap_before()
            } else {
                item
            }
        })
        .collect();
    let line = |text: &str| Dom::create_p_with_text(AzString::from(text)).with_css("font-size: 13px; margin: 4px 0px;");
    let button = |label: &str, action: Action| {
        Button::create(AzString::from(label))
            .with_on_click(
                RefAny::new(ActionRef {
                    app: app.clone(),
                    action,
                }),
                on_action as ButtonOnClickCallbackType,
            )
            .dom()
            .with_css("flex-grow: 0; margin: 4px 0px; width: 220px;")
    };
    let mut pane = Dom::create_div()
        .with_id(AzString::from("backstage-pane"))
        .with_css("display: flex; flex-direction: column; flex-grow: 1; padding: 24px 40px;")
        .with_child(
            Dom::create_p_with_text(AzString::from(BACKSTAGE_ITEMS[s.backstage_pane.min(BACKSTAGE_ITEMS.len() - 1)]))
                .with_css("font-size: 28px; margin: 0px 0px 16px 0px;"),
        );
    match BACKSTAGE_ITEMS.get(s.backstage_pane).copied().unwrap_or("Info") {
        "Info" => {
            pane.add_child(line(&format!("Workbook: {}", s.doc.title)));
            pane.add_child(line(&format!("File: sheets/{}.xlsx", s.doc.id)));
            pane.add_child(line(&format!("Folder: {}", s.data_root.display())));
            pane.add_child(line(&format!("Sheets: {}", s.cache.snapshot.sheets.len())));
            pane.add_child(line(if s.doc.dirty { "Changed since the last save." } else { "Saved." }));
        }
        "New" => {
            pane.add_child(
                Button::create(AzString::from("Blank workbook"))
                    .with_on_click(app.clone(), on_new_blank as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(AzString::from("new-blank"))
                    .with_css("flex-grow: 0; margin: 4px 0px; width: 220px;"),
            );
            pane.add_child(
                Button::create(AzString::from("Budget 2027 (sample)"))
                    .with_on_click(app.clone(), on_new_sample as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(AzString::from("new-sample"))
                    .with_css("flex-grow: 0; margin: 4px 0px; width: 220px;"),
            );
        }
        "Open" => {
            pane.add_child(
                Button::create(AzString::from("Browse for an .xlsx file..."))
                    .with_on_click(app.clone(), on_browse as ButtonOnClickCallbackType)
                    .dom()
                    .with_css("flex-grow: 0; margin: 4px 0px 12px 0px; width: 260px;"),
            );
            if s.workbooks.is_empty() {
                pane.add_child(line("No workbooks in the data folder yet."));
            }
            for (i, (id, sidecar)) in s.workbooks.iter().enumerate() {
                let title = if sidecar.title.is_empty() { id.as_str() } else { sidecar.title.as_str() };
                pane.add_child(
                    Button::with_type(AzString::from(title), azul::widgets::ButtonType::Link)
                        .with_on_click(
                            RefAny::new(OpenRef {
                                app: app.clone(),
                                index: i,
                            }),
                            on_open_entry as ButtonOnClickCallbackType,
                        )
                        .dom()
                        .with_id(AzString::from(format!("open-{i}"))),
                );
            }
        }
        "Save" => {
            pane.add_child(line(&format!("Saves \"{}\" as sheets/{}.xlsx.", s.doc.title, s.doc.id)));
            pane.add_child(button("Save", Action::Save));
        }
        "Save As" => {
            pane.add_child(line("Saves a copy under a new file id; the copy stays open."));
            pane.add_child(
                Button::create(AzString::from("Save a copy"))
                    .with_on_click(app.clone(), on_save_as as ButtonOnClickCallbackType)
                    .dom()
                    .with_css("flex-grow: 0; margin: 4px 0px; width: 220px;"),
            );
        }
        "Export" => {
            pane.add_child(line("Writes the sheet shown into exports/ in the data folder."));
            pane.add_child(button("Export CSV", Action::ExportCsv));
            pane.add_child(button("Export PDF", Action::ExportPdf));
        }
        "Close" => {
            pane.add_child(line("Closes the workbook and opens a blank one."));
            pane.add_child(
                Button::create(AzString::from("Close workbook"))
                    .with_on_click(app.clone(), on_new_blank as ButtonOnClickCallbackType)
                    .dom()
                    .with_css("flex-grow: 0; margin: 4px 0px; width: 220px;"),
            );
        }
        "Options" => {
            let mut appearance = Dom::create_div().with_css("display: flex; flex-direction: column;");
            appearance.add_child(line("The look follows the app theme and the system's mode; pick them here."));
            appearance.add_child(button("Flat", Action::ThemeFlat));
            appearance.add_child(button("Flora", Action::ThemeFlora));
            appearance.add_child(button("Light", Action::ModeLight));
            appearance.add_child(button("Dark", Action::ModeDark));
            let files = Dom::create_div()
                .with_css("display: flex; flex-direction: column;")
                .with_child(line(&format!("Data folder: {}", s.data_root.display())))
                .with_child(line("Workbooks: sheets/<id>.xlsx with a sheets/<id>.json sidecar; exports: exports/."))
                .with_child(line("Set AZSHEETS_DATA to use another folder."));
            pane.add_child(
                ShellSettingsLayout::create(StringVec::from_vec(vec![
                    AzString::from("Appearance"),
                    AzString::from("Files"),
                ]))
                .with_active_category(s.settings_category)
                .with_on_category(app.clone(), on_settings_category as ShellSettingsLayoutOnCategoryCallbackType)
                .with_section(ShellSettingsSection::create(AzString::from("Appearance"), appearance))
                .with_section(ShellSettingsSection::create(AzString::from("Files"), files))
                .dom(),
            );
        }
        _ => {
            pane.add_child(line("AzSheets - a spreadsheet on azul."));
            pane.add_child(line("Engine: IronCalc 0.8.3 (MIT OR Apache-2.0), on its own thread."));
            pane.add_child(line("Shortcuts: Ctrl+S save, Ctrl+Z / Ctrl+Y undo / redo, Ctrl+B / I / U, Ctrl+F find, F9 calculate."));
        }
    }
    Backstage::create(BackstageNavItemVec::from_vec(items))
        .with_active_item(s.backstage_pane)
        .with_on_nav_select(app.clone(), on_backstage_nav as BackstageOnNavSelectCallbackType)
        .with_on_back(app.clone(), on_backstage_back as ButtonOnClickCallbackType)
        .with_title_strip(title_row(s))
        .with_content(pane)
        .dom()
}

// ==== The window ====

fn title_row(s: &AppState) -> Dom {
    let mark = if s.doc.dirty { " *" } else { "" };
    Titlebar::create(AzString::from(format!("{}{mark} - AzSheets", s.doc.title)))
        .without_border_bottom()
        .dom()
}

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let _mode = info.get_mode();
    let window = (info.get_window_width(), info.get_window_height());
    let app = data.clone();
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        if window.0 > 0.0 && window.1 > 0.0 {
            s.window = window;
        }
    }
    let Some(guard) = data.downcast_ref::<AppState>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let shell = if s.screen == Screen::Backstage {
        DocumentShell::create(Dom::create_div())
            .with_title_row(title_row(s))
            .with_backstage(backstage(s, &app))
    } else {
        let mut middle = Dom::create_div()
            .with_css("display: flex; flex-direction: row; flex-grow: 1; min-height: 0px;")
            .with_child(grid(s, &app));
        if let Some(p) = panel(s, &app) {
            middle.add_child(p);
        }
        let document = Dom::create_div()
            .with_id(AzString::from("workbook"))
            .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
            .with_child(formula_bar(s, &app))
            .with_child(middle)
            .with_child(sheet_tabs(s, &app));
        DocumentShell::create(document)
            .with_title_row(title_row(s))
            .with_ribbon(ribbon(s, &app))
            .with_status_bar(status_bar(s, &app))
    };
    Dom::create_body()
        .with_css("display: flex; flex-direction: column;")
        .with_child(
            ShellThemeScope::create(shell.dom())
                .with_accent(ShellThemeAccent::Leaf)
                .dom(),
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app,
            on_window_key,
        )
}

// ==== Callbacks ====

/// Runs `f` on the app state and rebuilds.
fn with_app(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    f: impl FnOnce(&mut CallbackInfo, &RefAny, &mut AppState),
) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    f(info, &app, &mut *guard);
    Update::RefreshDom
}

/// The inputs of `area` from the snapshot (what the internal clipboard
/// keeps).
fn inputs_of(s: &AppState, area: CellArea) -> Vec<Vec<String>> {
    (area.row..=area.last_row())
        .map(|r| (area.column..=area.last_column()).map(|c| s.cache.input(r, c)).collect())
        .collect()
}

/// The block of filled cells around `at` (Excel's "current region"): grown
/// while a filled cell touches its edge, within the snapshot.
#[must_use]
pub fn current_region(cache: &ViewCache, at: CellAddr) -> CellArea {
    let filled = |r: i32, c: i32| cache.cell(r, c).is_some_and(|v| !v.input.is_empty() || !v.formatted.is_empty());
    let (mut r0, mut c0, mut r1, mut c1) = (at.row, at.column, at.row, at.column);
    loop {
        let mut grew = false;
        if r0 > 1 && (c0.max(1) - 1..=c1 + 1).any(|c| filled(r0 - 1, c)) {
            r0 -= 1;
            grew = true;
        }
        if r1 < LAST_ROW && (c0.max(1) - 1..=c1 + 1).any(|c| filled(r1 + 1, c)) {
            r1 += 1;
            grew = true;
        }
        if c0 > 1 && (r0.max(1) - 1..=r1 + 1).any(|r| filled(r, c0 - 1)) {
            c0 -= 1;
            grew = true;
        }
        if c1 < LAST_COLUMN && (r0.max(1) - 1..=r1 + 1).any(|r| filled(r, c1 + 1)) {
            c1 += 1;
            grew = true;
        }
        if !grew {
            break;
        }
    }
    CellArea::spanning(at.sheet, r0, c0, r1, c1)
}

/// The range a sort / filter / dedupe works on: the selection when it is
/// more than one cell (no header), else the current region (a header when
/// its first row is all text).
fn data_area(s: &AppState) -> (CellArea, bool) {
    let area = s.current_area();
    if area.width > 1 || area.height > 1 {
        return (area, false);
    }
    let region = current_region(&s.cache, s.active());
    let header = region.height > 1
        && (region.column..=region.last_column()).all(|c| {
            s.cache
                .cell(region.row, c)
                .map_or(true, |v| v.kind == ValueKind::Text || v.kind == ValueKind::Empty)
        });
    (region, header)
}

/// "$B$3:$D$6" (absolute) for a defined name.
fn absolute(area: CellArea) -> String {
    let dollar = |at: CellAddr| {
        let cell = to_cell(at);
        format!(
            "${}${}",
            CellGrid::column_label(cell.column).as_str(),
            at.row
        )
    };
    let first = dollar(CellAddr::new(area.sheet, area.row, area.column));
    if area.width <= 1 && area.height <= 1 {
        return first;
    }
    format!("{first}:{}", dollar(CellAddr::new(area.sheet, area.last_row(), area.last_column())))
}

/// Whether `text` can be a defined name (a letter or '_' first, then
/// letters, digits, '_' and '.').
fn is_name(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '.')
}

/// A grid event: store the view the grid computed, then do what the event
/// asks of the engine.
fn grid_event(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState, event: CellGridEvent) {
    let before = s.view.clone();
    s.view = event.view.clone();
    s.name_box = None;
    let sheet = s.sheet;
    match event.kind {
        CellGridEventKind::Select | CellGridEventKind::Drag | CellGridEventKind::Scroll => {
            if before.ranges.as_ref() != s.view.ranges.as_ref() {
                // The status bar's statistics are the engine's.
                send(info, app, s, Command::Fetch, Pending::Other, Post::None);
            } else {
                fetch_if_needed(info, app, s);
            }
        }
        CellGridEventKind::EditStart => {
            if s.view.edit_mode == CellGridEditMode::Edit && s.view.edit_text.as_str().is_empty() {
                // F2 / double-click: the edit starts from the cell's input.
                let at = s.active();
                let input = s.cache.input(at.row, at.column);
                s.view.edit_cursor = u32::try_from(input.chars().count()).unwrap_or(0);
                s.view.edit_text = AzString::from(input);
            }
        }
        CellGridEventKind::EditCommit => {
            let at = to_addr(sheet, event.range.first);
            let input = event.text.as_str().to_string();
            run(info, app, s, Command::SetInput { at, input });
        }
        CellGridEventKind::Fill => {
            let source = to_area(sheet, before.current_range());
            let reach = to_area(sheet, event.range);
            if let Some(to) = model::fill_to(source, reach) {
                run(info, app, s, Command::Fill { source, to });
            }
        }
        CellGridEventKind::ResizeColumn | CellGridEventKind::AutoFitColumn => {
            let column = i32::try_from(event.index).unwrap_or(0) + 1;
            let px = if event.kind == CellGridEventKind::AutoFitColumn {
                let texts: Vec<String> = s
                    .cache
                    .snapshot
                    .cells
                    .iter()
                    .filter(|c| c.column == column)
                    .map(|c| c.formatted.clone())
                    .collect();
                model::autofit_px(texts.iter().map(String::as_str), 13.0)
            } else {
                f64::from(event.size)
            };
            run(
                info,
                app,
                s,
                Command::ColumnWidth {
                    sheet,
                    first: column,
                    last: column,
                    px,
                },
            );
        }
        CellGridEventKind::ResizeRow => {
            let row = i32::try_from(event.index).unwrap_or(0) + 1;
            run(
                info,
                app,
                s,
                Command::RowHeight {
                    sheet,
                    first: row,
                    last: row,
                    px: f64::from(event.size),
                },
            );
        }
        CellGridEventKind::Copy | CellGridEventKind::Cut => {
            let area = to_area(sheet, event.range);
            let cut = event.kind == CellGridEventKind::Cut;
            s.clipboard = Some((area, cut, inputs_of(s, area)));
        }
        CellGridEventKind::Paste => {
            let at = to_addr(sheet, event.range.first);
            let tsv = event.text.as_str().to_string();
            run(info, app, s, Command::Paste { at, tsv });
            if let Some((source, true, _)) = s.clipboard.clone() {
                // A cut moves: the source is cleared once it is pasted.
                if source.row != at.row || source.column != at.column {
                    run(info, app, s, Command::Clear { areas: vec![source] });
                }
                s.clipboard = None;
            }
        }
        CellGridEventKind::Delete => {
            let areas = s.areas();
            run(info, app, s, Command::Clear { areas });
        }
        _ => {}
    }
}

extern "C" fn on_grid_event(mut data: RefAny, mut info: CallbackInfo, event: CellGridEvent) -> Update {
    with_app(&mut data, &mut info, |info, app, s| grid_event(info, app, s, event))
}

/// Several style changes to the selection.
fn restyle(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState, patches: Vec<StylePatch>) {
    let areas = s.areas();
    for patch in patches {
        run(
            info,
            app,
            s,
            Command::Style {
                areas: areas.clone(),
                patch,
            },
        );
    }
}

/// A ribbon command (or its keyboard shortcut).
#[allow(clippy::too_many_lines)]
fn act(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState, action: Action) {
    let at = s.active();
    let area = s.current_area();
    let sheet = s.sheet;
    let style = s
        .cache
        .cell(at.row, at.column)
        .and_then(|c| s.cache.snapshot.styles.get(c.style as usize))
        .cloned()
        .unwrap_or_default();
    let color = |hex: &str| Some(String::from(hex));
    match action {
        Action::Undo => run(info, app, s, Command::Undo),
        Action::Redo => run(info, app, s, Command::Redo),
        Action::Copy | Action::Cut => {
            s.clipboard = Some((area, action == Action::Cut, inputs_of(s, area)));
            s.message = format!("{} {}.", if action == Action::Cut { "Cut" } else { "Copied" }, a1_area(area));
        }
        Action::Paste => match s.clipboard.clone() {
            Some((source, cut, rows)) => {
                run(info, app, s, Command::Paste { at, tsv: model::tsv_of(&rows) });
                if cut {
                    run(info, app, s, Command::Clear { areas: vec![source] });
                    s.clipboard = None;
                }
            }
            None => s.message = String::from("Nothing copied in AzSheets yet; Ctrl+V pastes from other apps."),
        },
        Action::Bold => restyle(info, app, s, vec![StylePatch::Bold(!style.bold)]),
        Action::Italic => restyle(info, app, s, vec![StylePatch::Italic(!style.italic)]),
        Action::Underline => restyle(info, app, s, vec![StylePatch::Underline(!style.underline)]),
        Action::Strike => restyle(info, app, s, vec![StylePatch::Strike(!style.strike)]),
        Action::Grow => restyle(info, app, s, vec![StylePatch::FontSizeDelta(1)]),
        Action::Shrink => restyle(info, app, s, vec![StylePatch::FontSizeDelta(-1)]),
        Action::InkRed => restyle(info, app, s, vec![StylePatch::FontColor(color("#C00000"))]),
        Action::InkAuto => restyle(info, app, s, vec![StylePatch::FontColor(None)]),
        Action::FillYellow => restyle(info, app, s, vec![StylePatch::Fill(color("#FFF2CC"))]),
        Action::FillGreen => restyle(info, app, s, vec![StylePatch::Fill(color("#E2EFDA"))]),
        Action::FillNone => restyle(info, app, s, vec![StylePatch::Fill(None)]),
        Action::AlignLeft => restyle(info, app, s, vec![StylePatch::HAlign(HAlign::Left)]),
        Action::AlignCenter => restyle(info, app, s, vec![StylePatch::HAlign(HAlign::Center)]),
        Action::AlignRight => restyle(info, app, s, vec![StylePatch::HAlign(HAlign::Right)]),
        Action::AlignTop => restyle(info, app, s, vec![StylePatch::VAlign(VAlign::Top)]),
        Action::AlignMiddle => restyle(info, app, s, vec![StylePatch::VAlign(VAlign::Center)]),
        Action::AlignBottom => restyle(info, app, s, vec![StylePatch::VAlign(VAlign::Bottom)]),
        Action::Wrap => restyle(info, app, s, vec![StylePatch::Wrap(!style.wrap)]),
        Action::FormatGeneral => restyle(info, app, s, vec![StylePatch::NumberFormat(String::from("general"))]),
        Action::FormatNumber => restyle(info, app, s, vec![StylePatch::NumberFormat(String::from("#,##0.00"))]),
        Action::FormatCurrency => {
            restyle(info, app, s, vec![StylePatch::NumberFormat(String::from("\"$\"#,##0.00"))]);
        }
        Action::FormatPercent => restyle(info, app, s, vec![StylePatch::NumberFormat(String::from("0.00%"))]),
        Action::FormatDate => restyle(info, app, s, vec![StylePatch::NumberFormat(String::from("yyyy-mm-dd"))]),
        Action::DecimalMore => {
            restyle(info, app, s, vec![StylePatch::NumberFormat(model::step_decimals(&style.num_fmt, true))]);
        }
        Action::DecimalLess => {
            restyle(info, app, s, vec![StylePatch::NumberFormat(model::step_decimals(&style.num_fmt, false))]);
        }
        Action::StyleHeading => restyle(
            info,
            app,
            s,
            vec![
                StylePatch::Bold(true),
                StylePatch::FontSize(15),
                StylePatch::Borders {
                    preset: BorderPreset::Bottom,
                    color: String::from("#4472C4"),
                },
            ],
        ),
        Action::StyleTotal => restyle(
            info,
            app,
            s,
            vec![
                StylePatch::Bold(true),
                StylePatch::Borders {
                    preset: BorderPreset::Top,
                    color: String::from("#000000"),
                },
            ],
        ),
        Action::StyleGood => restyle(
            info,
            app,
            s,
            vec![StylePatch::Fill(color("#C6EFCE")), StylePatch::FontColor(color("#006100"))],
        ),
        Action::StyleBad => restyle(
            info,
            app,
            s,
            vec![StylePatch::Fill(color("#FFC7CE")), StylePatch::FontColor(color("#9C0006"))],
        ),
        Action::BordersAll | Action::BordersOutline | Action::BordersNone => {
            let preset = match action {
                Action::BordersAll => BorderPreset::All,
                Action::BordersOutline => BorderPreset::Outer,
                _ => BorderPreset::None,
            };
            restyle(
                info,
                app,
                s,
                vec![StylePatch::Borders {
                    preset,
                    color: String::from("#000000"),
                }],
            );
        }
        Action::InsertRow => run(info, app, s, Command::InsertRows { sheet, row: area.row, count: area.height }),
        Action::InsertColumn => run(
            info,
            app,
            s,
            Command::InsertColumns {
                sheet,
                column: area.column,
                count: area.width,
            },
        ),
        Action::DeleteRow => run(info, app, s, Command::DeleteRows { sheet, row: area.row, count: area.height }),
        Action::DeleteColumn => run(
            info,
            app,
            s,
            Command::DeleteColumns {
                sheet,
                column: area.column,
                count: area.width,
            },
        ),
        Action::InsertSheet => {
            let next = u32::try_from(s.cache.snapshot.sheets.len()).unwrap_or(0);
            run(info, app, s, Command::AddSheet);
            s.sheet = next;
            s.view = CellGridView::create();
        }
        Action::DeleteSheet => {
            if s.cache.snapshot.sheets.len() > 1 {
                run(info, app, s, Command::DeleteSheet { sheet });
                s.sheet = sheet.saturating_sub(1);
                s.view = CellGridView::create();
            } else {
                s.message = String::from("A workbook keeps at least one sheet.");
            }
        }
        Action::RenameSheet => {
            let name = s
                .cache
                .snapshot
                .sheets
                .get(sheet as usize)
                .map(|x| x.name.clone())
                .unwrap_or_default();
            s.renaming = Some((sheet, name));
        }
        Action::SheetLeft => {
            if sheet > 0 {
                run(info, app, s, Command::MoveSheet { sheet, to: sheet - 1 });
                s.sheet = sheet - 1;
            }
        }
        Action::SheetRight => {
            if (sheet as usize) + 1 < s.cache.snapshot.sheets.len() {
                run(info, app, s, Command::MoveSheet { sheet, to: sheet + 1 });
                s.sheet = sheet + 1;
            }
        }
        Action::TabColor => {
            const COLORS: [Option<&str>; 5] = [Some("#C00000"), Some("#70AD47"), Some("#4472C4"), Some("#ED7D31"), None];
            let current = s.cache.snapshot.sheets.get(sheet as usize).and_then(|x| x.color.clone());
            let index = COLORS
                .iter()
                .position(|c| c.map(str::to_ascii_uppercase) == current.as_deref().map(str::to_ascii_uppercase))
                .map_or(0, |i| (i + 1) % COLORS.len());
            let color = COLORS[index].map(String::from);
            run(info, app, s, Command::SheetColor { sheet, color });
        }
        Action::AutoSum => send(info, app, s, Command::SumRange { at }, Pending::SumRange(at), Post::None),
        Action::FillDown => {
            if area.height > 1 {
                let source = CellArea::spanning(sheet, area.row, area.column, area.row, area.last_column());
                run(
                    info,
                    app,
                    s,
                    Command::Fill {
                        source,
                        to: engine::FillTo::Row(area.last_row()),
                    },
                );
            }
        }
        Action::FillRight => {
            if area.width > 1 {
                let source = CellArea::spanning(sheet, area.row, area.column, area.last_row(), area.column);
                run(
                    info,
                    app,
                    s,
                    Command::Fill {
                        source,
                        to: engine::FillTo::Column(area.last_column()),
                    },
                );
            }
        }
        Action::SortAsc | Action::SortDesc => {
            let (data, has_header) = data_area(s);
            run(
                info,
                app,
                s,
                Command::Sort {
                    area: data,
                    key_column: at.column.clamp(data.column, data.last_column()),
                    descending: action == Action::SortDesc,
                    has_header,
                },
            );
        }
        Action::Filter | Action::ClearFilter => {
            let (data, _) = data_area(s);
            let keep = (action == Action::Filter).then(|| s.cache.shown(at.row, at.column));
            s.doc.dirty = true;
            send(
                info,
                app,
                s,
                Command::Filter {
                    area: data,
                    column: at.column,
                    keep,
                },
                Pending::Filter,
                Post::None,
            );
        }
        Action::RemoveDuplicates => {
            let (data, has_header) = data_area(s);
            s.doc.dirty = true;
            send(
                info,
                app,
                s,
                Command::RemoveDuplicates { area: data, has_header },
                Pending::RemoveDuplicates,
                Post::None,
            );
        }
        Action::Find => s.panel = Panel::Find,
        Action::ClearContents => {
            let areas = s.areas();
            run(info, app, s, Command::Clear { areas });
        }
        Action::ClearFormats => {
            let areas = s.areas();
            run(info, app, s, Command::ClearFormats { areas });
        }
        Action::Chart => s.panel = Panel::Chart,
        Action::InsertFunction => s.panel = Panel::Functions,
        Action::NameManager => s.panel = Panel::Names,
        Action::DefineName => {
            s.message = String::from("Select the range, type the name into the name box and press Enter.");
        }
        Action::CalculateNow => run(info, app, s, Command::Evaluate),
        Action::FreezePanes => {
            let cell = s.view.active;
            run(
                info,
                app,
                s,
                Command::Freeze {
                    sheet,
                    rows: i32::try_from(cell.row).unwrap_or(0),
                    columns: i32::try_from(cell.column).unwrap_or(0),
                },
            );
        }
        Action::FreezeTopRow => run(info, app, s, Command::Freeze { sheet, rows: 1, columns: 0 }),
        Action::FreezeFirstColumn => run(info, app, s, Command::Freeze { sheet, rows: 0, columns: 1 }),
        Action::Unfreeze => run(info, app, s, Command::Freeze { sheet, rows: 0, columns: 0 }),
        Action::Gridlines => {
            let show = !s.cache.snapshot.grid_lines;
            run(info, app, s, Command::GridLines { sheet, show });
        }
        Action::Headings => s.show_headers = !s.show_headers,
        Action::ZoomIn | Action::ZoomOut | Action::Zoom100 => {
            s.zoom = match action {
                Action::ZoomIn => (s.zoom + 10).min(ZOOM_MAX),
                Action::ZoomOut => s.zoom.saturating_sub(10).max(ZOOM_MIN),
                _ => 100,
            };
            fetch_if_needed(info, app, s);
        }
        Action::ThemeFlat => info.set_theme(AzString::from("flat")),
        Action::ThemeFlora => info.set_theme(AzString::from("flora")),
        Action::ModeLight => info.set_mode(OptionDarkLightMode::Some(DarkLightMode::Light)),
        Action::ModeDark => info.set_mode(OptionDarkLightMode::Some(DarkLightMode::Dark)),
        Action::Save => save(info, app, s),
        Action::ExportCsv => {
            let post = Post::Csv {
                root: s.data_root.clone(),
                name: safe_name(&s.doc.title),
            };
            send(info, app, s, Command::ExportCsv { sheet }, Pending::Exported, post);
        }
        Action::ExportPdf => export_pdf(info, app, s),
    }
}

extern "C" fn on_action(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, action)) = data
        .downcast_ref::<ActionRef>()
        .map(|r| (r.app.clone(), r.action))
    else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |info, app, s| act(info, app, s, action))
}

/// A copy of the callback info for the PDF renderer, which takes it by
/// value (the pattern AzWriter's PDF export uses).
fn reborrow_info(info: &CallbackInfo) -> CallbackInfo {
    CallbackInfo {
        ref_data: info.ref_data,
        hit_dom_node: info.hit_dom_node,
        cursor_relative_to_item: info.cursor_relative_to_item,
        cursor_in_viewport: info.cursor_in_viewport,
        changes: info.changes,
    }
}

/// The sheet's data as a plain table, rendered to PDF, written to
/// `exports/<title>.pdf` from a worker thread.
fn export_pdf(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState) {
    let (max_row, max_column) = s.cache.snapshot.extent;
    if max_row == 0 || max_column == 0 {
        s.message = String::from("The sheet is empty: nothing to export.");
        return;
    }
    let mut table = Dom::create_div().with_css("display: flex; flex-direction: column; font-size: 10px;");
    for r in 1..=max_row.min(500) {
        let mut line = Dom::create_div().with_css("display: flex; flex-direction: row;");
        for c in 1..=max_column.min(26) {
            let text = s.cache.shown(r, c);
            let numeric = s.cache.cell(r, c).is_some_and(|v| v.kind == ValueKind::Number);
            line.add_child(
                Dom::create_div()
                    .with_css(format!(
                        "width: 80px; min-width: 80px; padding: 1px 3px; border: 1px solid #cccccc; \
                         overflow: hidden; text-align: {};",
                        if numeric { "right" } else { "left" }
                    ))
                    .with_child(Dom::create_p_with_text(AzString::from(text))),
            );
        }
        table.add_child(line);
    }
    let doc = Dom::create_body()
        .with_css("margin: 0px; padding: 48px; background: white; color: black; font-family: sans-serif;")
        .with_child(
            Dom::create_p_with_text(AzString::from(s.doc.title.as_str()))
                .with_css("font-size: 16px; font-weight: bold; margin: 0px 0px 12px 0px;"),
        )
        .with_child(table);
    let bytes = Pdf::create()
        .from_dom_in_callback(reborrow_info(info), doc, 794.0, 1123.0)
        .as_ref()
        .to_vec();
    if bytes.is_empty() {
        s.message = String::from("The PDF export produced nothing.");
        return;
    }
    spawn_job(
        info,
        app,
        Job::Write {
            root: s.data_root.clone(),
            key: format!("exports/{}.pdf", safe_name(&s.doc.title)),
            bytes,
        },
    );
}

extern "C" fn on_file(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |_, _, s| {
        s.screen = Screen::Backstage;
        s.backstage_pane = 0;
    })
}

extern "C" fn on_ribbon_tab(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_app(&mut data, &mut info, |_, _, s| s.ribbon_tab = index)
}

/// The text of a field's state.
fn state_text(state: &TextInputState) -> String {
    state.get_text().as_str().to_string()
}

const fn text_return(update: Update) -> OnTextInputReturn {
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_field_text(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let Some((mut app, which)) = data.downcast_ref::<FieldRef>().map(|r| (r.app.clone(), r.field)) else {
        return text_return(Update::DoNothing);
    };
    let text = state_text(&state);
    let mut rebuild = false;
    with_app(&mut app, &mut info, |_, _, s| match which {
        Field::NameBox => s.name_box = Some(text),
        Field::Find => s.find = text,
        Field::Rename => {
            if let Some((sheet, _)) = s.renaming.clone() {
                s.renaming = Some((sheet, text));
            }
        }
        Field::Formula => {
            // The formula bar edits the active cell: the grid shows the
            // same edit in the cell.
            if s.view.edit_mode == CellGridEditMode::None {
                s.view.edit_mode = CellGridEditMode::Edit;
            }
            s.view.edit_cursor = u32::try_from(text.chars().count()).unwrap_or(0);
            s.view.edit_text = AzString::from(text);
            rebuild = true;
        }
    });
    text_return(if rebuild { Update::RefreshDom } else { Update::DoNothing })
}

/// Commits the edit in progress at the active cell and moves on (Enter in
/// the formula bar).
fn commit_formula(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState, down: bool) {
    let at = s.active();
    let input = s.view.edit_text.as_str().to_string();
    s.view.edit_mode = CellGridEditMode::None;
    s.view.edit_text = AzString::from("");
    s.view.edit_cursor = 0;
    run(info, app, s, Command::SetInput { at, input });
    let next = if down {
        CellGridCellRef {
            row: s.view.active.row.saturating_add(1),
            column: s.view.active.column,
        }
    } else {
        CellGridCellRef {
            row: s.view.active.row,
            column: s.view.active.column.saturating_add(1),
        }
    };
    s.view.active = next;
    s.view.anchor = next;
    s.view.ranges = CellGridRangeVec::from_vec(vec![CellGridRange { first: next, last: next }]);
}

extern "C" fn on_field_key(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let Some((mut app, which)) = data.downcast_ref::<FieldRef>().map(|r| (r.app.clone(), r.field)) else {
        return text_return(Update::DoNothing);
    };
    let key = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    let text = state_text(&state);
    let handled = matches!(key, Some(VirtualKeyCode::Return | VirtualKeyCode::Escape | VirtualKeyCode::Tab));
    if !handled {
        return text_return(Update::DoNothing);
    }
    let update = with_app(&mut app, &mut info, |info, app, s| match (which, key) {
        (Field::NameBox, Some(VirtualKeyCode::Return)) => {
            let sheets = s.cache.snapshot.sheets.clone();
            match parse_reference(&text, &sheets, s.sheet) {
                Some((sheet, range)) => {
                    let switched = sheet != s.sheet;
                    go_to(s, sheet, range);
                    if switched {
                        send(info, app, s, Command::Fetch, Pending::Other, Post::None);
                    } else {
                        fetch_if_needed(info, app, s);
                    }
                }
                None if is_name(text.trim()) => {
                    let area = s.current_area();
                    let sheet_name = sheets.get(s.sheet as usize).map(|x| x.name.clone()).unwrap_or_default();
                    let formula = format!("='{sheet_name}'!{}", absolute(area));
                    run(
                        info,
                        app,
                        s,
                        Command::DefineName {
                            name: text.trim().to_string(),
                            scope: None,
                            formula,
                        },
                    );
                    s.message = format!("Defined the name {} for {}.", text.trim(), a1_area(area));
                }
                None => s.message = format!("\"{text}\" is not a cell, a range or a name."),
            }
            s.name_box = None;
        }
        (Field::NameBox, _) => s.name_box = None,
        (Field::Formula, Some(VirtualKeyCode::Escape)) => {
            s.view.edit_mode = CellGridEditMode::None;
            s.view.edit_text = AzString::from("");
            s.view.edit_cursor = 0;
        }
        (Field::Formula, Some(VirtualKeyCode::Tab)) => commit_formula(info, app, s, false),
        (Field::Formula, _) => commit_formula(info, app, s, true),
        (Field::Find, Some(VirtualKeyCode::Return)) => {
            let needle = s.find.clone();
            if !needle.is_empty() {
                let from = s.active();
                send(info, app, s, Command::Find { from, needle }, Pending::Find, Post::None);
            }
        }
        (Field::Find, _) => s.panel = Panel::None,
        (Field::Rename, Some(VirtualKeyCode::Return)) => {
            if let Some((sheet, name)) = s.renaming.take() {
                let name = name.trim().to_string();
                if !name.is_empty() {
                    run(info, app, s, Command::RenameSheet { sheet, name });
                }
            }
        }
        (Field::Rename, _) => s.renaming = None,
    });
    text_return(update)
}

extern "C" fn on_suggestion(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, name)) = data.downcast_ref::<SuggestionRef>().map(|r| (r.app.clone(), r.name)) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |_, _, s| {
        let text = s.view.edit_text.as_str().to_string();
        let (completed, cursor) = functions::complete(&text, s.view.edit_cursor as usize, name);
        s.view.edit_text = AzString::from(completed);
        s.view.edit_cursor = u32::try_from(cursor).unwrap_or(0);
        if s.view.edit_mode == CellGridEditMode::None {
            s.view.edit_mode = CellGridEditMode::Edit;
        }
    })
}

extern "C" fn on_insert_function(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, name)) = data.downcast_ref::<FunctionRef>().map(|r| (r.app.clone(), r.name)) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |_, _, s| {
        let mut text = s.view.edit_text.as_str().to_string();
        if s.view.edit_mode == CellGridEditMode::None || text.is_empty() {
            text = String::from("=");
        }
        text.push_str(name);
        text.push('(');
        s.view.edit_mode = CellGridEditMode::Edit;
        s.view.edit_cursor = u32::try_from(text.chars().count()).unwrap_or(0);
        s.view.edit_text = AzString::from(text);
        s.panel = Panel::None;
    })
}

extern "C" fn on_delete_name(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<NameRef>().map(|r| (r.app.clone(), r.index)) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |info, app, s| {
        if let Some(n) = s.cache.snapshot.names.get(index).cloned() {
            run(
                info,
                app,
                s,
                Command::DeleteName {
                    name: n.name,
                    scope: n.scope,
                },
            );
        }
    })
}

extern "C" fn on_panel_close(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |_, _, s| s.panel = Panel::None)
}

extern "C" fn on_tab_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, sheet)) = data.downcast_ref::<TabRef>().map(|r| (r.app.clone(), r.sheet)) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |info, app, s| {
        if s.sheet != sheet {
            s.sheet = sheet;
            s.view = CellGridView::create();
            send(info, app, s, Command::Fetch, Pending::Other, Post::None);
        }
    })
}

extern "C" fn on_tab_double_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, sheet)) = data.downcast_ref::<TabRef>().map(|r| (r.app.clone(), r.sheet)) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |_, _, s| {
        let name = s
            .cache
            .snapshot
            .sheets
            .get(sheet as usize)
            .map(|x| x.name.clone())
            .unwrap_or_default();
        s.renaming = Some((sheet, name));
    })
}

/// Enters the backstage's `pane`; the Open pane lists the data folder.
fn backstage_pane(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState, pane: usize) {
    s.screen = Screen::Backstage;
    s.backstage_pane = pane.min(BACKSTAGE_ITEMS.len() - 1);
    if BACKSTAGE_ITEMS[s.backstage_pane] == "Open" {
        spawn_job(
            info,
            app,
            Job::List {
                root: s.data_root.clone(),
            },
        );
    }
}

extern "C" fn on_backstage_nav(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_app(&mut data, &mut info, |info, app, s| {
        match BACKSTAGE_ITEMS.get(index).copied() {
            // Save acts at once, like Excel's: the pane only confirms.
            Some("Save") => {
                save(info, app, s);
                s.backstage_pane = index;
            }
            _ => backstage_pane(info, app, s, index),
        }
    })
}

extern "C" fn on_settings_category(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_app(&mut data, &mut info, |_, _, s| s.settings_category = index)
}

extern "C" fn on_backstage_back(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |_, _, s| s.screen = Screen::Workbook)
}

/// A new workbook (blank or the sample) under a fresh id.
fn new_workbook(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState, sample: bool) {
    let taken: Vec<String> = s.workbooks.iter().map(|(_, c)| c.title.clone()).collect();
    let title = if sample {
        String::from("Budget 2027")
    } else {
        model::next_book_title(&taken)
    };
    s.doc = DocInfo {
        id: storage::new_id(),
        title: title.clone(),
        dirty: sample,
    };
    s.sheet = 0;
    s.view = CellGridView::create();
    s.screen = Screen::Workbook;
    s.panel = Panel::None;
    let command = if sample {
        Command::Sample
    } else {
        Command::New { name: title }
    };
    send(info, app, s, command, Pending::Other, Post::None);
}

extern "C" fn on_new_blank(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |info, app, s| new_workbook(info, app, s, false))
}

extern "C" fn on_new_sample(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |info, app, s| new_workbook(info, app, s, true))
}

extern "C" fn on_save_as(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |info, app, s| {
        s.doc.id = storage::new_id();
        s.doc.title = format!("{} (copy)", s.doc.title);
        save(info, app, s);
        s.screen = Screen::Workbook;
    })
}

extern "C" fn on_open_entry(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<OpenRef>().map(|r| (r.app.clone(), r.index)) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |info, app, s| {
        if let Some((id, _)) = s.workbooks.get(index).cloned() {
            s.message = String::from("Opening...");
            spawn_job(
                info,
                app,
                Job::Load {
                    root: s.data_root.clone(),
                    id,
                },
            );
        }
    })
}

extern "C" fn on_browse(data: RefAny, _info: CallbackInfo) -> Update {
    let _request = FileDialog::open_file(
        AzString::from("Open a workbook"),
        OptionString::None,
        OptionFileTypeList::None,
        data,
        on_picked,
    );
    Update::DoNothing
}

extern "C" fn on_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing; // cancelled
    };
    let path = PathBuf::from(path.as_string().as_str());
    with_app(&mut data, &mut info, |info, app, s| {
        s.message = format!("Opening {}...", path.display());
        spawn_job(info, app, Job::Import { path });
    })
}

/// The shortcuts no widget takes (the grid has its own keys).
extern "C" fn on_window_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let key = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    let modifiers = info.get_key_modifiers();
    let command = modifiers.primary_down();
    let action = match key {
        Some(VirtualKeyCode::S) if command => Some(Action::Save),
        Some(VirtualKeyCode::Z) if command && modifiers.shift => Some(Action::Redo),
        Some(VirtualKeyCode::Z) if command => Some(Action::Undo),
        Some(VirtualKeyCode::Y) if command => Some(Action::Redo),
        Some(VirtualKeyCode::B) if command => Some(Action::Bold),
        Some(VirtualKeyCode::I) if command => Some(Action::Italic),
        Some(VirtualKeyCode::U) if command => Some(Action::Underline),
        Some(VirtualKeyCode::F) if command => Some(Action::Find),
        Some(VirtualKeyCode::F9) => Some(Action::CalculateNow),
        _ => None,
    };
    if let Some(action) = action {
        return with_app(&mut data, &mut info, |info, app, s| act(info, app, s, action));
    }
    match key {
        Some(VirtualKeyCode::O) if command => {
            with_app(&mut data, &mut info, |info, app, s| backstage_pane(info, app, s, 2))
        }
        Some(VirtualKeyCode::N) if command => {
            with_app(&mut data, &mut info, |info, app, s| new_workbook(info, app, s, false))
        }
        Some(VirtualKeyCode::Escape) => with_app(&mut data, &mut info, |_, _, s| {
            if s.screen == Screen::Backstage {
                s.screen = Screen::Workbook;
            }
        }),
        _ => Update::DoNothing,
    }
}

/// The window is up: act on the command line.
extern "C" fn startup(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |info, app, s| {
        let args = s.args.take().unwrap_or_default();
        if let Some(path) = args.open.clone() {
            spawn_job(info, app, Job::Import { path });
        } else if args.sample {
            new_workbook(info, app, s, true);
        } else {
            send(info, app, s, Command::Fetch, Pending::Other, Post::None);
        }
        match args.screen {
            args::Screen::Workbook => {}
            args::Screen::BackstageInfo => backstage_pane(info, app, s, 0),
            args::Screen::BackstageNew => backstage_pane(info, app, s, 1),
            args::Screen::BackstageOpen => backstage_pane(info, app, s, 2),
        }
        println!("AZSHEETS_READY {}", s.data_root.display());
    })
}

// ==== Entry ====

fn user_data_dir() -> Option<PathBuf> {
    FilePath::get_data_dir()
        .into_option()
        .map(|dir| PathBuf::from(dir.inner.as_str()))
}

/// The engine the app runs: IronCalc, built on the engine thread.
fn make_engine() -> Box<dyn engine::SheetEngine> {
    Box::new(IronCalcEngine::new_empty())
}

pub fn start(args: Args) {
    let data_root = storage::data_root(
        std::env::var(storage::DATA_VAR).ok().as_deref(),
        user_data_dir(),
    );
    let mut state = AppState::new(data_root);
    match worker::spawn_engine(make_engine) {
        Ok(tx) => state.engine = Some(tx),
        Err(e) => state.message = format!("The spreadsheet engine could not start: {e}"),
    }
    let (width, height) = args.size.unwrap_or((1280.0, 800.0));
    state.window = (width, height);
    let mut config = AppConfig::create();
    if let Some(theme) = args.theme {
        config = config.with_theme(AzString::from(match theme {
            args::Theme::Flat => "flat",
            args::Theme::Flora => "flora",
        }));
    }
    if let Some(mode) = args.mode {
        config = config.with_mode(OptionDarkLightMode::Some(match mode {
            args::Mode::Light => DarkLightMode::Light,
            args::Mode::Dark => DarkLightMode::Dark,
        }));
    }
    eprintln!("[azsheets] data folder {}", state.data_root.display());
    state.args = Some(args);
    let app = App::create(RefAny::new(state), config);
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.size.dimensions = LogicalSize::create(width, height);
    window.window_state.title = AzString::from("AzSheets");
    window.window_state.flags.decorations = WindowDecorations::NoTitle;
    window.create_callback = Some(Callback::create(startup)).into();
    app.run(window);
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;

    use super::*;
    use crate::{
        engine::{SheetEngine, StylePatch},
        fake_engine::FakeEngine,
        worker::{handle, CellView},
    };

    fn cell(row: u32, column: u32) -> CellGridCellRef {
        CellGridCellRef { row, column }
    }

    fn sheets(names: &[&str]) -> Vec<SheetInfo> {
        names
            .iter()
            .map(|n| SheetInfo {
                name: (*n).to_string(),
                color: None,
                hidden: false,
            })
            .collect()
    }

    #[test]
    fn grid_cells_and_engine_addresses_convert_both_ways() {
        let at = to_addr(2, cell(6, 1));
        assert_eq!(at, CellAddr::new(2, 7, 2), "0-based grid, 1-based engine");
        assert_eq!(to_cell(at), cell(6, 1));
        let area = to_area(0, CellGridRange { first: cell(2, 1), last: cell(5, 3) });
        assert_eq!(area, CellArea::spanning(0, 3, 2, 6, 4));
    }

    #[test]
    fn a1_names_come_from_the_grid_widget() {
        assert_eq!(a1(CellAddr::new(0, 7, 2)), "B7");
        assert_eq!(a1_area(CellArea::spanning(0, 3, 2, 6, 2)), "B3:B6");
        assert_eq!(a1_area(CellArea::cell(CellAddr::new(0, 1, 1))), "A1");
        assert_eq!(absolute(CellArea::spanning(0, 3, 2, 6, 4)), "$B$3:$D$6");
    }

    #[test]
    fn the_name_box_reads_cells_ranges_and_sheet_references() {
        let book = sheets(&["Summary", "Data"]);
        assert_eq!(
            parse_reference("B7", &book, 0),
            Some((0, CellGridRange { first: cell(6, 1), last: cell(6, 1) }))
        );
        assert_eq!(
            parse_reference("data!C3:a1", &book, 0),
            Some((1, CellGridRange { first: cell(0, 0), last: cell(2, 2) })),
            "the sheet by name in any case, the corners in any order"
        );
        assert_eq!(parse_reference("Other!A1", &book, 0), None);
        assert_eq!(parse_reference("A1:", &book, 0), None);
        assert_eq!(parse_reference("Total", &book, 0), None, "a name, not a reference");
        assert!(is_name("Total") && is_name("_x.y") && !is_name("1st") && !is_name("a b"));
    }

    fn snapshot_with(cells: Vec<CellView>, styles: Vec<CellStyle>) -> Snapshot {
        Snapshot {
            cells,
            styles,
            ..Snapshot::default()
        }
    }

    fn view(row: i32, column: i32, shown: &str, input: &str, kind: ValueKind, style: u32) -> CellView {
        CellView {
            row,
            column,
            formatted: shown.to_string(),
            input: input.to_string(),
            kind,
            style,
        }
    }

    #[test]
    fn the_view_cache_indexes_the_snapshot_and_maps_its_styles_to_the_grid() {
        let bold_yellow = CellStyle {
            bold: true,
            fill: Some(String::from("#FFFF00")),
            h_align: HAlign::Right,
            ..CellStyle::default()
        };
        let cache = ViewCache::of(snapshot_with(
            vec![
                view(2, 3, "1,250.00", "1250", ValueKind::Number, 1),
                view(4, 1, "9", "=SUM(A1:A3)", ValueKind::Number, 0),
            ],
            vec![CellStyle::default(), bold_yellow],
        ));
        assert_eq!(cache.shown(2, 3), "1,250.00");
        assert_eq!(cache.input(4, 1), "=SUM(A1:A3)");
        assert_eq!(cache.input(9, 9), "", "not fetched: empty");
        let style = &cache.styles[1];
        assert!(style.bold);
        assert_eq!(style.align, CellGridHorizontalAlign::Right);
        match &style.fill {
            OptionColorU::Some(c) => assert_eq!((c.r, c.g, c.b), (255, 255, 0)),
            OptionColorU::None => panic!("the fill is mapped"),
        }
        assert!(matches!(cache.styles[0].fill, OptionColorU::None));
    }

    #[test]
    fn the_current_region_grows_over_touching_filled_cells() {
        let cache = ViewCache::of(snapshot_with(
            vec![
                view(2, 1, "Category", "Category", ValueKind::Text, 0),
                view(2, 2, "Jan", "Jan", ValueKind::Text, 0),
                view(3, 1, "Rent", "Rent", ValueKind::Text, 0),
                view(3, 2, "1250", "1250", ValueKind::Number, 0),
                view(4, 2, "400", "400", ValueKind::Number, 0),
                view(9, 9, "far", "far", ValueKind::Text, 0),
            ],
            vec![CellStyle::default()],
        ));
        assert_eq!(
            current_region(&cache, CellAddr::new(0, 3, 2)),
            CellArea::spanning(0, 2, 1, 4, 2),
            "the block, not the far cell"
        );
    }

    #[test]
    fn a_new_state_asks_for_its_window_and_selection() {
        let s = AppState::new(PathBuf::from("/tmp/azsheets-test"));
        let request = s.view_request();
        assert_eq!(request.sheet, 0);
        assert_eq!(request.rows.first().map(|r| r.0), Some(1));
        assert_eq!(request.selection, vec![CellArea::cell(CellAddr::new(0, 1, 1))]);
        assert!(!s.window_fetched(), "nothing fetched yet");
        assert_eq!(safe_name("Budget 2027 / Q1"), "Budget-2027---Q1");
        assert_eq!(safe_name("///"), "workbook");
    }

    #[test]
    fn the_grid_shows_what_the_engine_answered_for_the_window() {
        let mut engine = FakeEngine::new();
        engine.set_cell_input(CellAddr::new(0, 1, 1), "4").unwrap();
        engine.set_cell_input(CellAddr::new(0, 2, 1), "5").unwrap();
        engine.set_cell_input(CellAddr::new(0, 3, 1), "=SUM(A1:A2)").unwrap();
        engine
            .update_style(CellArea::cell(CellAddr::new(0, 3, 1)), &StylePatch::Bold(true))
            .unwrap();
        let s = AppState::new(PathBuf::from("/tmp/azsheets-test"));
        let (tx, _rx) = channel();
        let reply = handle(
            &mut engine,
            &EngineMsg {
                seq: 1,
                command: Command::Fetch,
                view: s.view_request(),
                reply: tx,
            },
        );
        let cache = Arc::new(ViewCache::of(reply.snapshot));
        assert_eq!(cache.shown(3, 1), "9");
        let source = RefAny::new(GridSource {
            cache: Arc::clone(&cache),
        });
        let shown = cell_data(source.clone(), cell(2, 0));
        assert_eq!(shown.text.as_str(), "9");
        assert_eq!(shown.kind, CellGridCellKind::Number);
        assert!(cell_look(source.clone(), cell(2, 0)).bold, "the total is bold");
        let empty = cell_data(source, cell(40, 40));
        assert_eq!(empty.kind, CellGridCellKind::Empty);
    }
}
