//! The engine thread.
//!
//! IronCalc recalculates the whole workbook on every edit and evaluates
//! RECURSIVELY: a dependency chain of ~3,000 cells evaluated from its far end
//! overflows an 8 MB stack and aborts the process (ironcalc-performance.md).
//! So the engine lives on its own thread with a 256 MB stack
//! ([`ENGINE_STACK_BYTES`]) and the UI never calls it: it sends an
//! [`EngineMsg`] (a [`Command`] plus the [`ViewRequest`] it shows) and gets a
//! [`Reply`] with a fresh [`Snapshot`] of that view. Messages run in the order
//! they were sent.

use std::{
    collections::{BTreeSet, HashMap},
    sync::mpsc::{Receiver, Sender},
    time::Instant,
};

use crate::{
    engine::{
        CellAddr, CellArea, CellStyle, CellValue, DefinedName, EngineError, FillTo, SheetEngine,
        SheetInfo, StylePatch, LAST_COLUMN, LAST_ROW,
    },
    ops::{self, SelectionStats},
};

/// The engine thread's stack: IronCalc needs ~3 KB per dependency level, and
/// 256 MB runs the 50,000-cell chain of the engine study.
pub const ENGINE_STACK_BYTES: usize = 256 * 1024 * 1024;

/// What the UI asks the engine to do.
#[derive(Clone, Debug)]
pub enum Command {
    /// Nothing: just the snapshot (the view moved).
    Fetch,
    New {
        name: String,
    },
    Load {
        bytes: Vec<u8>,
        name: String,
    },
    /// A new workbook with the "Budget 2027" sample (`crate::sample`).
    Sample,
    /// `Reply::saved` = the workbook as `.xlsx`.
    Save,
    /// `Reply::csv` = the sheet's used range as CSV.
    ExportCsv {
        sheet: u32,
    },
    SetInput {
        at: CellAddr,
        input: String,
    },
    Paste {
        at: CellAddr,
        tsv: String,
    },
    Clear {
        areas: Vec<CellArea>,
    },
    ClearFormats {
        areas: Vec<CellArea>,
    },
    Style {
        areas: Vec<CellArea>,
        patch: StylePatch,
    },
    Fill {
        source: CellArea,
        to: FillTo,
    },
    InsertRows {
        sheet: u32,
        row: i32,
        count: i32,
    },
    DeleteRows {
        sheet: u32,
        row: i32,
        count: i32,
    },
    InsertColumns {
        sheet: u32,
        column: i32,
        count: i32,
    },
    DeleteColumns {
        sheet: u32,
        column: i32,
        count: i32,
    },
    ColumnWidth {
        sheet: u32,
        first: i32,
        last: i32,
        px: f64,
    },
    RowHeight {
        sheet: u32,
        first: i32,
        last: i32,
        px: f64,
    },
    Freeze {
        sheet: u32,
        rows: i32,
        columns: i32,
    },
    GridLines {
        sheet: u32,
        show: bool,
    },
    Sort {
        area: CellArea,
        key_column: i32,
        descending: bool,
        has_header: bool,
    },
    /// `Reply::count` = the rows removed.
    RemoveDuplicates {
        area: CellArea,
        has_header: bool,
    },
    /// `Reply::count` = the rows hidden; `keep: None` clears the filter.
    Filter {
        area: CellArea,
        column: i32,
        keep: Option<String>,
    },
    /// `Reply::found` = the next match after `from`.
    Find {
        from: CellAddr,
        needle: String,
    },
    AddSheet,
    RenameSheet {
        sheet: u32,
        name: String,
    },
    DeleteSheet {
        sheet: u32,
    },
    MoveSheet {
        sheet: u32,
        to: u32,
    },
    SheetColor {
        sheet: u32,
        color: Option<String>,
    },
    DefineName {
        name: String,
        scope: Option<u32>,
        formula: String,
    },
    DeleteName {
        name: String,
        scope: Option<u32>,
    },
    Undo,
    Redo,
    Evaluate,
}

/// What the UI shows: the sheet and the cell window it wants back with every
/// reply.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ViewRequest {
    pub sheet: u32,
    /// Inclusive row spans: the frozen rows and the window.
    pub rows: Vec<(i32, i32)>,
    /// Inclusive column spans.
    pub columns: Vec<(i32, i32)>,
    /// The selection, for the status bar's statistics.
    pub selection: Vec<CellArea>,
}

/// The kind of a cell's value (the grid aligns numbers right).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ValueKind {
    #[default]
    Empty,
    Number,
    Text,
    Boolean,
    Error,
}

impl ValueKind {
    /// The kind of `value`.
    #[must_use]
    pub fn of(value: &CellValue) -> Self {
        match value {
            CellValue::Empty => Self::Empty,
            CellValue::Number(_) => Self::Number,
            CellValue::Text(_) => Self::Text,
            CellValue::Boolean(_) => Self::Boolean,
            CellValue::Error(_) => Self::Error,
        }
    }
}

/// One cell of the view.
#[derive(Clone, Debug, PartialEq)]
pub struct CellView {
    pub row: i32,
    pub column: i32,
    /// The displayed text.
    pub formatted: String,
    /// The formula or the typed text (the formula bar shows it).
    pub input: String,
    pub kind: ValueKind,
    /// Index into [`Snapshot::styles`].
    pub style: u32,
}

/// Everything the UI draws from, for one [`ViewRequest`].
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Snapshot {
    /// The sheet shown (the request's, clamped to the sheets that exist).
    pub sheet: u32,
    pub sheets: Vec<SheetInfo>,
    /// The spans fetched (the request's, clamped to the sheet).
    pub rows: Vec<(i32, i32)>,
    pub columns: Vec<(i32, i32)>,
    /// The cells with an input, a value or a non-default style.
    pub cells: Vec<CellView>,
    /// The styles the cells use; `styles[0]` is `CellStyle::default()`.
    pub styles: Vec<CellStyle>,
    /// Every fetched column's width, px.
    pub column_widths: Vec<(i32, f64)>,
    /// Every fetched row's height, px (0 = hidden).
    pub row_heights: Vec<(i32, f64)>,
    /// The frozen panes: (rows, columns).
    pub frozen: (i32, i32),
    pub grid_lines: bool,
    /// (max_row, max_column) of the sheet's data.
    pub extent: (i32, i32),
    pub stats: SelectionStats,
    pub names: Vec<DefinedName>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub workbook_name: String,
}

/// The engine's answer to one message.
#[derive(Debug)]
pub struct Reply {
    pub seq: u64,
    /// The command's outcome; an `Err` is a sentence for the user.
    pub result: Result<(), EngineError>,
    /// The view after the command.
    pub snapshot: Snapshot,
    /// `Command::Save`.
    pub saved: Option<Vec<u8>>,
    /// `Command::ExportCsv`.
    pub csv: Option<String>,
    /// `Command::Find` (`None`: no match).
    pub found: Option<CellAddr>,
    /// `Command::RemoveDuplicates` / `Command::Filter`.
    pub count: Option<usize>,
    /// The command and the snapshot on the engine thread, ms.
    pub elapsed_ms: f64,
}

/// One message to the engine thread.
pub struct EngineMsg {
    pub seq: u64,
    pub command: Command,
    pub view: ViewRequest,
    /// Where the [`Reply`] goes.
    pub reply: Sender<Reply>,
}

/// The extra answers a command can have.
#[derive(Default)]
struct Extras {
    saved: Option<Vec<u8>>,
    csv: Option<String>,
    found: Option<CellAddr>,
    count: Option<usize>,
}

/// Inclusive spans clamped to `1..=limit`, empty ones dropped.
fn clamp_spans(spans: &[(i32, i32)], limit: i32) -> Vec<(i32, i32)> {
    spans
        .iter()
        .filter_map(|&(a, b)| {
            let (lo, hi) = (a.min(b).max(1), a.max(b).min(limit));
            (lo <= hi).then_some((lo, hi))
        })
        .collect()
}

/// Every line of `spans`, once, in order.
fn lines(spans: &[(i32, i32)]) -> BTreeSet<i32> {
    spans.iter().flat_map(|&(lo, hi)| lo..=hi).collect()
}

/// The view `view` of `engine`'s workbook.
#[must_use]
pub fn snapshot(engine: &dyn SheetEngine, view: &ViewRequest) -> Snapshot {
    let sheets = engine.sheets();
    let sheet = view.sheet.min(sheets.len().saturating_sub(1) as u32);
    let rows = clamp_spans(&view.rows, LAST_ROW);
    let columns = clamp_spans(&view.columns, LAST_COLUMN);
    let row_lines = lines(&rows);
    let column_lines = lines(&columns);

    let default_style = CellStyle::default();
    let mut styles = vec![default_style.clone()];
    let mut style_index: HashMap<CellStyle, u32> = HashMap::new();
    style_index.insert(default_style.clone(), 0);
    let mut cells = Vec::new();
    for &row in &row_lines {
        for &column in &column_lines {
            let at = CellAddr::new(sheet, row, column);
            let input = engine.cell_input(at);
            let value = engine.cell_value(at);
            let style = engine.cell_style(at);
            if input.is_empty() && value == CellValue::Empty && style == default_style {
                continue;
            }
            let index = match style_index.get(&style) {
                Some(i) => *i,
                None => {
                    let i = styles.len() as u32;
                    style_index.insert(style.clone(), i);
                    styles.push(style);
                    i
                }
            };
            cells.push(CellView {
                row,
                column,
                formatted: engine.cell_formatted(at),
                input,
                kind: ValueKind::of(&value),
                style: index,
            });
        }
    }

    Snapshot {
        sheet,
        column_widths: column_lines
            .iter()
            .map(|&c| (c, engine.column_width(sheet, c)))
            .collect(),
        row_heights: row_lines
            .iter()
            .map(|&r| (r, engine.row_height(sheet, r)))
            .collect(),
        sheets,
        rows,
        columns,
        cells,
        styles,
        frozen: engine.frozen(sheet),
        grid_lines: engine.show_grid_lines(sheet),
        extent: engine.extent(sheet),
        stats: ops::selection_stats(engine, &view.selection),
        names: engine.defined_names(),
        can_undo: engine.can_undo(),
        can_redo: engine.can_redo(),
        workbook_name: engine.workbook_name(),
    }
}

/// Runs `command`, filling `extras` with what it answers.
fn run(
    engine: &mut dyn SheetEngine,
    command: &Command,
    extras: &mut Extras,
) -> Result<(), EngineError> {
    match command {
        Command::Fetch => Ok(()),
        Command::New { name } => engine.new_workbook(name),
        Command::Load { bytes, name } => engine.load_xlsx(bytes, name),
        Command::Sample => {
            engine.new_workbook("Budget 2027")?;
            crate::sample::budget(engine)
        }
        Command::Save => {
            extras.saved = Some(engine.save_xlsx()?);
            Ok(())
        }
        Command::ExportCsv { sheet } => {
            extras.csv = Some(ops::export_csv(engine, *sheet));
            Ok(())
        }
        Command::SetInput { at, input } => engine.set_cell_input(*at, input),
        Command::Paste { at, tsv } => engine.paste_tsv(*at, tsv),
        Command::Clear { areas } => areas.iter().try_for_each(|a| engine.clear_contents(*a)),
        Command::ClearFormats { areas } => areas.iter().try_for_each(|a| engine.clear_formats(*a)),
        Command::Style { areas, patch } => areas
            .iter()
            .try_for_each(|a| engine.update_style(*a, patch)),
        Command::Fill { source, to } => engine.auto_fill(*source, *to),
        Command::InsertRows { sheet, row, count } => engine.insert_rows(*sheet, *row, *count),
        Command::DeleteRows { sheet, row, count } => engine.delete_rows(*sheet, *row, *count),
        Command::InsertColumns {
            sheet,
            column,
            count,
        } => engine.insert_columns(*sheet, *column, *count),
        Command::DeleteColumns {
            sheet,
            column,
            count,
        } => engine.delete_columns(*sheet, *column, *count),
        Command::ColumnWidth {
            sheet,
            first,
            last,
            px,
        } => engine.set_column_width(*sheet, *first, *last, *px),
        Command::RowHeight {
            sheet,
            first,
            last,
            px,
        } => engine.set_row_height(*sheet, *first, *last, *px),
        Command::Freeze {
            sheet,
            rows,
            columns,
        } => engine.set_frozen(*sheet, *rows, *columns),
        Command::GridLines { sheet, show } => engine.set_show_grid_lines(*sheet, *show),
        Command::Sort {
            area,
            key_column,
            descending,
            has_header,
        } => ops::sort_area(engine, *area, *key_column, *descending, *has_header),
        Command::RemoveDuplicates { area, has_header } => {
            extras.count = Some(ops::remove_duplicates(engine, *area, *has_header)?);
            Ok(())
        }
        Command::Filter { area, column, keep } => {
            extras.count = Some(ops::filter_rows(engine, *area, *column, keep.as_deref())?);
            Ok(())
        }
        Command::Find { from, needle } => {
            extras.found = ops::find_next(engine, *from, needle);
            Ok(())
        }
        Command::AddSheet => engine.add_sheet(),
        Command::RenameSheet { sheet, name } => engine.rename_sheet(*sheet, name),
        Command::DeleteSheet { sheet } => engine.delete_sheet(*sheet),
        Command::MoveSheet { sheet, to } => engine.move_sheet(*sheet, *to),
        Command::SheetColor { sheet, color } => engine.set_sheet_color(*sheet, color.as_deref()),
        Command::DefineName {
            name,
            scope,
            formula,
        } => engine.add_defined_name(name, *scope, formula),
        Command::DeleteName { name, scope } => engine.delete_defined_name(name, *scope),
        Command::Undo => engine.undo(),
        Command::Redo => engine.redo(),
        Command::Evaluate => {
            engine.evaluate();
            Ok(())
        }
    }
}

/// The text of a panic's payload.
fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        String::from("unknown panic")
    }
}

/// Runs one message: the command, then the snapshot. A failing command (or
/// one the engine panics on) answers an `Err` in `result` and the snapshot
/// of the workbook as it stands.
pub fn handle(engine: &mut dyn SheetEngine, msg: &EngineMsg) -> Reply {
    let started = Instant::now();
    let mut extras = Extras::default();
    let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run(engine, &msg.command, &mut extras)
    })) {
        Ok(result) => result,
        Err(payload) => Err(format!(
            "The engine failed: {}",
            panic_text(payload.as_ref())
        )),
    };
    let snapshot = snapshot(engine, &msg.view);
    Reply {
        seq: msg.seq,
        result,
        snapshot,
        saved: extras.saved,
        csv: extras.csv,
        found: extras.found,
        count: extras.count,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
    }
}

/// The engine thread's body: every message in order, until every sender is
/// gone.
pub fn engine_loop(mut engine: Box<dyn SheetEngine>, rx: Receiver<EngineMsg>) {
    while let Ok(msg) = rx.recv() {
        let reply = handle(engine.as_mut(), &msg);
        // The UI may have stopped listening (a window closed): not an error.
        let _ = msg.reply.send(reply);
    }
}

/// Starts the engine thread ("azsheets-engine", [`ENGINE_STACK_BYTES`] of
/// stack) and returns where to send it messages. `make` builds the engine ON
/// that thread. The thread ends when the last sender is dropped.
pub fn spawn_engine<F>(make: F) -> std::io::Result<Sender<EngineMsg>>
where
    F: FnOnce() -> Box<dyn SheetEngine> + Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name(String::from("azsheets-engine"))
        .stack_size(ENGINE_STACK_BYTES)
        .spawn(move || engine_loop(make(), rx))?;
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;

    use super::*;
    use crate::fake_engine::FakeEngine;

    fn at(row: i32, column: i32) -> CellAddr {
        CellAddr::new(0, row, column)
    }

    fn view(rows: (i32, i32), columns: (i32, i32)) -> ViewRequest {
        ViewRequest {
            sheet: 0,
            rows: vec![rows],
            columns: vec![columns],
            selection: Vec::new(),
        }
    }

    fn msg(seq: u64, command: Command, view: ViewRequest, reply: Sender<Reply>) -> EngineMsg {
        EngineMsg {
            seq,
            command,
            view,
            reply,
        }
    }

    #[test]
    fn a_snapshot_lists_only_the_cells_with_content_or_style_and_shares_their_styles() {
        let mut e = FakeEngine::new();
        e.set_inputs(
            at(1, 1),
            &[vec![
                String::from("x"),
                String::from("y"),
                String::from("z"),
            ]],
        )
        .unwrap();
        e.update_style(CellArea::spanning(0, 1, 1, 1, 2), &StylePatch::Bold(true))
            .unwrap();
        e.update_style(
            CellArea::cell(at(1, 4)),
            &StylePatch::Fill(Some("#FF0000".into())),
        )
        .unwrap();
        let s = snapshot(&e, &view((1, 3), (1, 6)));
        let placed: Vec<(i32, i32, u32)> =
            s.cells.iter().map(|c| (c.row, c.column, c.style)).collect();
        assert_eq!(placed, vec![(1, 1, 1), (1, 2, 1), (1, 3, 0), (1, 4, 2)]);
        assert_eq!(s.styles.len(), 3);
        assert_eq!(s.styles[0], CellStyle::default());
        assert!(s.styles[1].bold);
        assert_eq!(s.styles[2].fill.as_deref(), Some("#FF0000"));
        assert_eq!(s.cells[0].kind, ValueKind::Text);
        assert_eq!(s.column_widths.len(), 6);
        assert_eq!(s.row_heights.len(), 3);
    }

    #[test]
    fn a_snapshot_gives_a_hidden_row_no_height_and_clamps_the_spans() {
        let mut e = FakeEngine::new();
        e.set_rows_hidden(0, 2, 2, true).unwrap();
        let s = snapshot(&e, &view((0, 3), (16_380, 20_000)));
        assert_eq!(s.rows, vec![(1, 3)]);
        assert_eq!(s.columns, vec![(16_380, LAST_COLUMN)]);
        assert_eq!(s.row_heights[1], (2, 0.0));
        assert!(s.row_heights[0].1 > 0.0);
    }

    #[test]
    fn a_snapshot_carries_the_statistics_of_the_selection() {
        let mut e = FakeEngine::new();
        e.set_inputs(
            at(1, 1),
            &[vec![String::from("2")], vec![String::from("4")]],
        )
        .unwrap();
        let mut v = view((1, 2), (1, 1));
        v.selection = vec![CellArea::spanning(0, 1, 1, 2, 1)];
        let s = snapshot(&e, &v);
        assert_eq!(s.stats.sum, 6.0);
        assert_eq!(s.stats.average(), Some(3.0));
    }

    #[test]
    fn handling_a_message_runs_the_command_then_snapshots_the_view() {
        let mut e = FakeEngine::new();
        e.set_inputs(at(1, 2), &[vec![String::from("1"), String::from("2")]])
            .unwrap();
        let (tx, _rx) = channel();
        let reply = handle(
            &mut e,
            &msg(
                7,
                Command::SetInput {
                    at: at(1, 1),
                    input: "=SUM(B1:C1)".into(),
                },
                view((1, 1), (1, 3)),
                tx,
            ),
        );
        assert_eq!(reply.seq, 7);
        assert_eq!(reply.result, Ok(()));
        assert_eq!(reply.snapshot.cells[0].formatted, "3");
        assert_eq!(reply.snapshot.cells[0].input, "=SUM(B1:C1)");
        assert!(reply.snapshot.can_undo);
    }

    #[test]
    fn a_failing_command_answers_an_error_and_the_workbook_as_it_stands() {
        let mut e = FakeEngine::new();
        let (tx, _rx) = channel();
        let reply = handle(
            &mut e,
            &msg(
                1,
                Command::DeleteSheet { sheet: 0 },
                view((1, 1), (1, 1)),
                tx,
            ),
        );
        assert!(reply.result.is_err(), "the last sheet stays");
        assert_eq!(reply.snapshot.sheets.len(), 1);
    }

    #[test]
    fn save_and_find_answer_through_the_reply() {
        let mut e = FakeEngine::new();
        e.set_cell_input(at(3, 2), "needle").unwrap();
        let (tx, _rx) = channel();
        let saved = handle(
            &mut e,
            &msg(1, Command::Save, ViewRequest::default(), tx.clone()),
        );
        assert!(saved.saved.is_some_and(|b| !b.is_empty()));
        let found = handle(
            &mut e,
            &msg(
                2,
                Command::Find {
                    from: at(1, 1),
                    needle: "NEED".into(),
                },
                ViewRequest::default(),
                tx,
            ),
        );
        assert_eq!(found.found, Some(at(3, 2)));
    }

    #[test]
    fn the_engine_thread_answers_messages_in_the_order_they_were_sent() {
        let engine = spawn_engine(|| Box::new(FakeEngine::new())).expect("thread");
        let (tx, rx) = channel();
        engine
            .send(msg(
                1,
                Command::SetInput {
                    at: at(1, 1),
                    input: "5".into(),
                },
                view((1, 1), (1, 2)),
                tx.clone(),
            ))
            .unwrap();
        engine
            .send(msg(
                2,
                Command::SetInput {
                    at: at(1, 2),
                    input: "=SUM(A1:A1)".into(),
                },
                view((1, 1), (1, 2)),
                tx,
            ))
            .unwrap();
        let first = rx.recv().unwrap();
        let second = rx.recv().unwrap();
        assert_eq!((first.seq, second.seq), (1, 2));
        assert_eq!(
            second.snapshot.cells[1].formatted, "5",
            "the second saw the first"
        );
    }

    #[test]
    fn the_engine_loop_ends_when_the_last_sender_is_dropped() {
        let (tx, rx) = channel::<EngineMsg>();
        let thread = std::thread::spawn(move || engine_loop(Box::new(FakeEngine::new()), rx));
        drop(tx);
        thread.join().expect("the loop returned");
    }
}
