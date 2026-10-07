//! The window's parts, laid out like the old Windows Task Manager (XP / 7):
//! the tab row (Processes, Performance, Networking, Users), the Processes
//! tab (the dense process table over a row with the filter and "End
//! Process" at the right), the LIVE VIEWS (the table and the three pages -
//! each a `VirtualView` a tick re-renders in place), the status bar
//! ("Processes: N", "CPU Usage: x%", "Physical Memory: y%"), the end-process
//! question and the settings section.
//!
//! A live view's DOM is a document of its own: nothing cascades into it from
//! the page. So every live view wraps its content in its own
//! `ShellThemeScope` (the Slate accent, the theme's ground and ink) at the
//! view's exact size.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, ModalOnCloseCallbackType, SegmentedOnChangeCallbackType,
        StandardDialogOnEventCallbackType, TabOnClickCallbackType,
        TextInputOnTextInputCallbackType, WriteBackCallbackType,
    },
    option::OptionString,
    prelude::*,
    shells::{ShellEmptyState, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    vec::StringVec,
    widgets::{
        Button, MessageBox, MessageBoxKind, Modal, ModalState, OnTextInputReturn, Segmented,
        SegmentedState, StandardDialogEvent, StandardDialogEventKind, StatusBar,
        StatusBarSegment, TabHeader, TabHeaderState, TextInput, TextInputState, TextInputValid,
    },
};
use azul_appkit::{files::FileJob, ui as kit};

use crate::{
    graph::{self, Line, Strips},
    ids,
    model::{format_k, format_uptime, format_whole_percent, percent_of, Model},
    table::{self, format_bytes, format_rate},
    ticks::{LiveView, Screen},
    Confirm, Monitor, SPEEDS,
};

/// The space around and between a page's group boxes, px.
const GAP: f32 = 8.0;
/// A group box's caption line, px.
const CAPTION: f32 = 18.0;
/// A group box's inner padding, px.
const PAD: f32 = 6.0;
/// The meter column of the Performance page, px.
const METER_W: f32 = 112.0;
/// The figures under the Performance page's graphs, px.
const STATS_H: f32 = 96.0;
/// The gap between two per-core graphs, px.
const CORE_GAP: f32 = 4.0;
/// A line of a dense table (the Users page), px.
const ROW_PX: f32 = 20.0;
/// The write-back tag of the history export.
const EXPORT_TAG: u64 = 1;

// ==== Helpers ====

/// A line of text in a block (a `<p>` without the UA margin).
fn line(text: impl Into<AzString>, css: &str) -> Dom {
    Dom::create_p_with_text(text).with_css(format!("margin: 0px; {css}"))
}

/// A group box `width` x `height` px: a thin frame, its caption on top, the
/// `body` under it.
fn group(caption: &str, body: Dom, width: f32, height: f32) -> Dom {
    Dom::create_div()
        .with_class(ids::GROUP)
        .with_css(format!(
            "display: flex; flex-direction: column; width: {width}px; height: {height}px; \
             flex-shrink: 0; box-sizing: border-box; padding: 2px {PAD}px {PAD}px {PAD}px; \
             border: 1px solid system:separator; overflow: hidden;"
        ))
        .with_child(line(
            caption,
            &format!("height: {CAPTION}px; font-size: 12px; white-space: nowrap;"),
        ))
        .with_child(body)
}

/// The inner size of a [`group`] `width` x `height` px.
fn group_inner(width: f32, height: f32) -> (f32, f32) {
    (
        (width - 2.0 - 2.0 * PAD).max(16.0),
        (height - 2.0 - 2.0 - PAD - CAPTION).max(16.0),
    )
}

/// A figure: its label at the left, its value at the right.
fn figure(label: &str, value: String) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; height: 17px; font-size: 12px;")
        .with_child(line(label, "flex-grow: 1; white-space: nowrap;"))
        .with_child(line(value, "white-space: nowrap; text-align: right;"))
}

/// A group box of figures.
fn figures(caption: &str, rows: Vec<(&str, String)>, width: f32, height: f32) -> Dom {
    let mut body = Dom::create_div().with_css("display: flex; flex-direction: column;");
    for (label, value) in rows {
        body.add_child(figure(label, value));
    }
    group(caption, body, width, height)
}

// ==== The tab row ====

/// The tab row: Processes, Performance, Networking, Users.
pub fn tools(s: &Monitor, app: &RefAny) -> Dom {
    let tabs: Vec<AzString> = Screen::ALL
        .iter()
        .map(|t| AzString::from(t.title()))
        .collect();
    Dom::create_div()
        .with_id(ids::TOOLS)
        .with_css(
            "display: flex; flex-direction: row; align-items: flex-end; padding: 4px 8px 0px \
             8px;",
        )
        .with_child(
            TabHeader::create(StringVec::from_vec(tabs))
                .with_active_tab(s.screen.index())
                .with_on_click(app.clone(), on_tab as TabOnClickCallbackType)
                .dom(),
        )
}

/// The Processes tab: the table (a live view) over the row with the filter
/// at the left and "End Process" at the right.
pub fn process_page(s: &Monitor, app: &RefAny) -> Dom {
    let mut end = Button::create("End Process")
        .with_on_click(app.clone(), on_end_clicked as ButtonOnClickCallbackType);
    if s.model.selected_row().is_none() {
        end = end.with_disabled("Select a process first");
    }
    let actions = Dom::create_div()
        .with_id(ids::PROCESS_ACTIONS)
        .with_css(
            "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; \
             padding: 6px 8px;",
        )
        .with_child(
            TextInput::create()
                .with_text(s.model.filter())
                .with_placeholder("Filter by name, user or PID")
                .with_accessibility_name("Filter processes")
                .with_on_text_input(app.clone(), on_filter as TextInputOnTextInputCallbackType)
                .dom()
                .with_id(ids::FILTER)
                .with_css("width: 220px;"),
        )
        .with_child(Dom::create_div().with_css("flex-grow: 1;"))
        .with_child(end.dom().with_id(ids::END_PROCESS));
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; width: 100%;",
        )
        .with_child(live_view(app, LiveView::Table))
        .with_child(actions)
}

/// The empty state until the first reading.
pub fn waiting(s: &Monitor) -> Dom {
    let source = if s.sample {
        "The sample machine (--sample)."
    } else {
        "Processes, CPU, memory, disks and networks, on a background thread."
    };
    ShellEmptyState::create("Reading the system\u{2026}")
        .with_icon("monitor_heart")
        .with_detail(source)
        .dom()
        .with_id(ids::WAITING)
}

extern "C" fn on_tab(mut app: RefAny, mut info: CallbackInfo, state: TabHeaderState) -> Update {
    let handle = app.clone();
    let Some(mut s) = app.downcast_mut::<Monitor>() else {
        return Update::DoNothing;
    };
    let screen = Screen::at(state.active_tab);
    if screen == s.screen {
        return Update::DoNothing;
    }
    s.screen = screen;
    println!("AZMON_SCREEN {}", screen.name());
    // The graphs of the new tab scroll between readings.
    crate::ensure_frames(&handle, &mut s, &mut info);
    Update::RefreshDom
}

/// A keystroke in the filter: the model filters, the table's live view
/// re-renders in place (the field keeps its own text: no rebuild).
extern "C" fn on_filter(
    mut app: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let query = state.get_text().as_str().to_string();
    let labels = {
        let Some(mut guard) = app.downcast_mut::<Monitor>() else {
            return OnTextInputReturn {
                update: Update::DoNothing,
                valid: TextInputValid::Yes,
            };
        };
        let s = &mut *guard;
        s.model.set_filter(&query);
        let shown = s.model.shown_count();
        // From the first row; the build carries the selection over.
        s.table.top = 0;
        println!("AZMON_SHOWN {shown}");
        status_labels(s)
    };
    crate::rerender(&mut info, &[LiveView::Table]);
    for (marker, label) in labels {
        if let Some(node) = info.get_node_id_by_marker(marker).into_option() {
            let _ = StatusBar::update_segment_label(info, node, label);
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_end_clicked(mut app: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = app.downcast_mut::<Monitor>() else {
        return Update::DoNothing;
    };
    if ask_to_end(&mut s) {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

// ==== The end-process question ====

/// Opens the question for the selected process (if one is shown).
pub fn ask_to_end(s: &mut Monitor) -> bool {
    let Some(row) = s.model.selected_row() else {
        return false;
    };
    let question = Confirm {
        pid: row.pid,
        name: row.name.clone(),
        user: row.user.clone(),
        other_user: s.model.summary.belongs_to_another_user(row),
    };
    println!("AZMON_ASK {} {}", question.pid, question.name);
    s.confirm = Some(question);
    true
}

/// The question: azul's `MessageBox` in a `Modal` - End process, Kill,
/// Cancel (the default).
pub fn confirm_dom(question: &Confirm, app: &RefAny) -> Dom {
    let mut text = format!("{} ({})", question.name, question.pid);
    if !question.user.is_empty() {
        text.push_str(&format!(", user {}", question.user));
    }
    let mut detail =
        "Unsaved work in it is lost. Kill ends it without asking it to quit.".to_string();
    if question.other_user {
        detail.push_str(&format!(
            " It belongs to {}: ending it needs administrator rights.",
            question.user
        ));
    }
    let message = MessageBox::create(
        MessageBoxKind::Question,
        format!("End {}?", question.name),
        text,
    )
    .with_detail(detail)
    .with_buttons(
        vec![
            AzString::from("End process"),
            AzString::from("Kill"),
            AzString::from("Cancel"),
        ],
        2,
    )
    .with_on_event(app.clone(), on_answer as StandardDialogOnEventCallbackType);
    Modal::create(message.dom())
        .with_title("AzMonitor")
        .with_open(true)
        .with_on_close(app.clone(), on_dismissed as ModalOnCloseCallbackType)
        .dom()
        .with_id(ids::CONFIRM)
}

extern "C" fn on_answer(
    mut app: RefAny,
    _info: CallbackInfo,
    event: StandardDialogEvent,
) -> Update {
    let Some(mut guard) = app.downcast_mut::<Monitor>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(question) = s.confirm.take() else {
        return Update::DoNothing;
    };
    if matches!(event.kind, StandardDialogEventKind::Button) && event.index < 2 {
        crate::end_process(s, question.pid, event.index == 1);
    }
    Update::RefreshDom
}

extern "C" fn on_dismissed(mut app: RefAny, _info: CallbackInfo, _state: ModalState) -> Update {
    if let Some(mut s) = app.downcast_mut::<Monitor>() {
        s.confirm = None;
    }
    Update::RefreshDom
}

// ==== The live views ====

/// What a live view's VirtualView carries: the app and which view it is.
struct Live {
    app: RefAny,
    view: LiveView,
}

/// The marker (and id) of a live view's node.
#[must_use]
pub fn marker_of(view: LiveView) -> AzString {
    match view {
        LiveView::Table => ids::TABLE_VIEW,
        LiveView::Performance => ids::PERFORMANCE,
        LiveView::Networking => ids::NETWORKING,
        LiveView::Users => ids::USERS,
    }
}

/// A live view: a VirtualView whose callback reads the model, re-rendered
/// in place by every tick.
pub fn live_view(app: &RefAny, view: LiveView) -> Dom {
    let marker = marker_of(view);
    Dom::create_virtual_view(
        RefAny::new(Live {
            app: app.clone(),
            view,
        }),
        render_live,
    )
    .with_marker(OptionString::Some(marker.clone()))
    .with_id(marker)
    .with_css("width: 100%; flex-grow: 1; min-height: 0px;")
}

/// A live view's callback: its content at the view's size, in its own
/// theme scope. The graphs it drew are what the frame timer slides.
extern "C" fn render_live(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let Some((app, view)) = data.downcast_ref::<Live>().map(|l| (l.app.clone(), l.view)) else {
        return VirtualViewReturn::default();
    };
    let size = info.bounds.get_logical_size();
    let (w, h) = (size.width.max(1.0), size.height.max(1.0));
    let mut strips = Strips::default();
    let content = match view {
        LiveView::Table => process_table(&app, w, h),
        LiveView::Performance => performance(&app, &mut strips, w, h),
        LiveView::Networking => networking(&app, &mut strips, w, h),
        LiveView::Users => users(&app, w, h),
    };
    {
        let mut handle = app.clone();
        if let Some(mut s) = handle.downcast_mut::<Monitor>() {
            s.strips = strips.steps;
        };
    }
    let dom = Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; width: {w}px; height: {h}px; overflow: hidden;"
        ))
        .with_child(
            ShellThemeScope::create(content)
                .with_accent(ShellThemeAccent::Slate)
                .dom(),
        );
    let rect = LogicalRect::create(LogicalPosition::create(0.0, 0.0), LogicalSize::create(w, h));
    VirtualViewReturn::with_dom(dom, rect, rect)
}

/// The process table at `w` x `h`: its view carried over to the rows of now
/// first (`table::sync`: the processes in view stay where they were). The
/// guard on the app is dropped before the table asks the app for its cells.
fn process_table(app: &RefAny, w: f32, h: f32) -> Dom {
    let mut handle = app.clone();
    let Some((view, rows)) = handle.downcast_mut::<Monitor>().map(|mut guard| {
        let s = &mut *guard;
        let count = s.model.shown_count();
        let page = table::rows_in_view(&s.table, count, w, h);
        let anchor = s.sort_anchor.take();
        table::sync(&mut s.table, &mut s.shown, &s.model, page, anchor);
        s.table_page = page;
        table::print_view(s, page);
        (s.table.clone(), u32::try_from(count).unwrap_or(u32::MAX))
    }) else {
        return Dom::create_div();
    };
    table::table(app, view, rows, w, h).dom()
}

/// The CPU usage history: one graph per core (two to sixteen of them, in a
/// grid), or the whole CPU's.
fn cpu_history(strips: &mut Strips, m: &Model, w: f32, h: f32) -> Dom {
    let n = m.cores.len();
    if !(2..=16).contains(&n) {
        let history = m.cpu.to_vec();
        let lines = [Line {
            values: &history,
            color: graph::LINE,
        }];
        return graph::graph(strips, &lines, 100.0, m.readings, w, h);
    }
    // One row of up to four, else two rows (Windows 7's "one graph per CPU").
    let columns = if n <= 4 { n } else { n.div_ceil(2) };
    let rows = n.div_ceil(columns);
    #[allow(clippy::cast_precision_loss)] // a few cores
    let (cell_w, cell_h) = (
        ((w - (columns - 1) as f32 * CORE_GAP) / columns as f32).floor(),
        ((h - (rows - 1) as f32 * CORE_GAP) / rows as f32).floor(),
    );
    let mut grid = Dom::create_div().with_css("display: flex; flex-direction: column;");
    for r in 0..rows {
        let top = if r == 0 { 0.0 } else { CORE_GAP };
        let mut row = Dom::create_div().with_css(format!(
            "display: flex; flex-direction: row; margin-top: {top}px;"
        ));
        for c in 0..columns {
            let i = r * columns + c;
            let Some(core) = m.cores.get(i) else {
                break;
            };
            let history = core.to_vec();
            let lines = [Line {
                values: &history,
                color: graph::LINE,
            }];
            let left = if c == 0 { 0.0 } else { CORE_GAP };
            row.add_child(
                Dom::create_div()
                    .with_css(format!("margin-left: {left}px; flex-shrink: 0;"))
                    .with_child(graph::graph(
                        strips, &lines, 100.0, m.readings, cell_w, cell_h,
                    )),
            );
        }
        grid.add_child(row);
    }
    grid
}

/// The Performance page: the CPU's and the memory's meter beside their
/// history, the machine's figures under them.
fn performance(app: &RefAny, strips: &mut Strips, w: f32, h: f32) -> Dom {
    let mut handle = app.clone();
    let Some(s) = handle.downcast_ref::<Monitor>() else {
        return Dom::create_div();
    };
    let m = &s.model;
    let sum = &m.summary;
    let row_h = ((h - 2.0 * GAP - STATS_H - 2.0 * GAP) / 2.0).floor().max(110.0);
    let history_w = (w - 2.0 * GAP - GAP - METER_W).floor().max(200.0);
    let memory_share = percent_of(sum.memory_used, sum.memory_total) / 100.0;

    let (mw, mh) = group_inner(METER_W, row_h);
    let (hw, hh) = group_inner(history_w, row_h);
    let cpu_caption = if sum.cpu_brand.trim().is_empty() {
        "CPU Usage History".to_string()
    } else {
        format!("CPU Usage History - {}", sum.cpu_brand.trim())
    };
    let cpu_row = Dom::create_div()
        .with_css("display: flex; flex-direction: row;")
        .with_child(
            group(
                "CPU Usage",
                graph::meter(sum.cpu / 100.0, &format_whole_percent(sum.cpu), mw, mh),
                METER_W,
                row_h,
            )
            .with_id(ids::CPU_USAGE),
        )
        .with_child(
            Dom::create_div()
                .with_css(format!("margin-left: {GAP}px;"))
                .with_child(
                    group(&cpu_caption, cpu_history(strips, m, hw, hh), history_w, row_h)
                        .with_id(ids::CPU_HISTORY),
                ),
        );
    let memory = m.memory.to_vec();
    let memory_lines = [Line {
        values: &memory,
        color: graph::LINE,
    }];
    let memory_row = Dom::create_div()
        .with_css(format!("display: flex; flex-direction: row; margin-top: {GAP}px;"))
        .with_child(
            group(
                "Memory",
                graph::meter(memory_share, &format_bytes(sum.memory_used), mw, mh),
                METER_W,
                row_h,
            )
            .with_id(ids::MEMORY_USAGE),
        )
        .with_child(
            Dom::create_div()
                .with_css(format!("margin-left: {GAP}px;"))
                .with_child(
                    group(
                        "Physical Memory Usage History",
                        graph::graph(strips, &memory_lines, 100.0, m.readings, hw, hh),
                        history_w,
                        row_h,
                    )
                    .with_id(ids::MEMORY_HISTORY),
                ),
        );

    let quarter = ((w - 2.0 * GAP - 3.0 * GAP) / 4.0).floor().max(120.0);
    let clock = if sum.cpu_mhz > 0 {
        format!("{} MHz", sum.cpu_mhz)
    } else {
        "-".to_string()
    };
    let available = sum.memory_total.saturating_sub(sum.memory_used);
    let swap_free = sum.swap_total.saturating_sub(sum.swap_used);
    let stats = Dom::create_div()
        .with_id(ids::STATS)
        .with_css(format!("display: flex; flex-direction: row; margin-top: {GAP}px;"))
        .with_child(figures(
            "Totals",
            vec![
                ("Processes", sum.processes.to_string()),
                ("Cores", m.cores.len().to_string()),
                ("Up Time", format_uptime(sum.uptime)),
                ("Clock", clock),
            ],
            quarter,
            STATS_H,
        ))
        .with_child(
            Dom::create_div()
                .with_css(format!("margin-left: {GAP}px;"))
                .with_child(figures(
                    "Physical Memory (K)",
                    vec![
                        ("Total", format_k(sum.memory_total)),
                        ("In Use", format_k(sum.memory_used)),
                        ("Available", format_k(available)),
                    ],
                    quarter,
                    STATS_H,
                )),
        )
        .with_child(
            Dom::create_div()
                .with_css(format!("margin-left: {GAP}px;"))
                .with_child(figures(
                    "Swap (K)",
                    vec![
                        ("Total", format_k(sum.swap_total)),
                        ("In Use", format_k(sum.swap_used)),
                        ("Free", format_k(swap_free)),
                    ],
                    quarter,
                    STATS_H,
                )),
        )
        .with_child(
            Dom::create_div()
                .with_css(format!("margin-left: {GAP}px;"))
                .with_child(figures(
                    "Disk",
                    vec![
                        ("Read", format_rate(sum.disk_read_rate)),
                        ("Write", format_rate(sum.disk_write_rate)),
                    ],
                    quarter,
                    STATS_H,
                )),
        );
    Dom::create_div()
        .with_css(format!("display: flex; flex-direction: column; padding: {GAP}px;"))
        .with_child(cpu_row)
        .with_child(memory_row)
        .with_child(stats)
}

/// A swatch and its word (a graph's legend).
fn swatch(color: &str, text: String) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; margin-right: 16px;")
        .with_child(Dom::create_div().with_css(format!(
            "width: 10px; height: 10px; margin-right: 6px; background-color: {color};"
        )))
        .with_child(line(text, "font-size: 12px; white-space: nowrap;"))
}

/// The Networking page: the network's history (received and sent, on a
/// scale that moves in calm steps), its legend, the figures as the old Task
/// Manager's adapter table.
fn networking(app: &RefAny, strips: &mut Strips, w: f32, h: f32) -> Dom {
    let mut handle = app.clone();
    let Some(s) = handle.downcast_ref::<Monitor>() else {
        return Dom::create_div();
    };
    let m = &s.model;
    let sum = &m.summary;
    let table_h = 3.0 * ROW_PX + 8.0;
    let legend_h = 22.0;
    let graph_w = (w - 2.0 * GAP).floor().max(200.0);
    let graph_h = (h - 2.0 * GAP - legend_h - table_h - GAP).floor().max(120.0);
    let (gw, gh) = group_inner(graph_w, graph_h);
    let received = m.net_in.to_vec();
    let sent = m.net_out.to_vec();
    let lines = [
        Line {
            values: &received,
            color: graph::LINE,
        },
        Line {
            values: &sent,
            color: graph::LINE_2,
        },
    ];
    let caption = format!("Network Utilization (scale {})", format_rate(s.net_top));
    let history = group(
        &caption,
        graph::graph(strips, &lines, s.net_top, m.readings, gw, gh),
        graph_w,
        graph_h,
    )
    .with_id(ids::NETWORK_HISTORY);
    let legend = Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; height: {legend_h}px;"
        ))
        .with_child(swatch(graph::LINE, format!("Received {}", format_rate(sum.net_in_rate))))
        .with_child(swatch(graph::LINE_2, format!("Sent {}", format_rate(sum.net_out_rate))));
    let columns: [(&str, f32); 4] = [
        ("Adapter", 0.40),
        ("Received", 0.20),
        ("Sent", 0.20),
        ("Total", 0.20),
    ];
    let cells = [
        "All network interfaces".to_string(),
        format_rate(sum.net_in_rate),
        format_rate(sum.net_out_rate),
        format_rate(sum.net_in_rate + sum.net_out_rate),
    ];
    let header: Vec<String> = columns.iter().map(|(t, _)| (*t).to_string()).collect();
    let widths: Vec<f32> = columns.iter().map(|(_, share)| graph_w * share).collect();
    let figures = Dom::create_div()
        .with_id(ids::NETWORK_STATS)
        .with_css("display: flex; flex-direction: column; border: 1px solid system:separator;")
        .with_child(table_row(&header, &widths, true))
        .with_child(table_row(&cells, &widths, false));
    Dom::create_div()
        .with_css(format!("display: flex; flex-direction: column; padding: {GAP}px;"))
        .with_child(history)
        .with_child(legend)
        .with_child(figures)
}

/// A row of a dense table: `cells` in columns of `widths` px (`header`: the
/// header's look).
fn table_row(cells: &[String], widths: &[f32], header: bool) -> Dom {
    let look = if header {
        "font-weight: 600; border-bottom: 1px solid system:separator;"
    } else {
        ""
    };
    let mut row = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: row; align-items: center; height: {ROW_PX}px; \
         flex-shrink: 0; {look}"
    ));
    for (i, (cell, width)) in cells.iter().zip(widths.iter()).enumerate() {
        let align = if i == 0 { "left" } else { "right" };
        row.add_child(line(
            cell.clone(),
            &format!(
                "width: {width}px; padding: 0px 6px; box-sizing: border-box; font-size: 12px; \
                 white-space: nowrap; overflow: hidden; text-align: {align};"
            ),
        ));
    }
    row
}

/// The Users page: who runs the processes - one row each, the busiest first.
fn users(app: &RefAny, w: f32, h: f32) -> Dom {
    let mut handle = app.clone();
    let Some(s) = handle.downcast_ref::<Monitor>() else {
        return Dom::create_div();
    };
    let m = &s.model;
    let width = (w - 2.0 * GAP).floor().max(200.0);
    let shares: [f32; 5] = [0.36, 0.16, 0.16, 0.14, 0.18];
    let widths: Vec<f32> = shares.iter().map(|share| width * share).collect();
    let header: Vec<String> = ["User", "Status", "Processes", "CPU", "Memory"]
        .iter()
        .map(|t| (*t).to_string())
        .collect();
    let mut rows = Dom::create_div()
        .with_id(ids::USERS_TABLE)
        .with_css(format!(
            "display: flex; flex-direction: column; width: {width}px; max-height: {}px; \
             overflow: hidden; border: 1px solid system:separator;",
            (h - 2.0 * GAP).max(ROW_PX)
        ))
        .with_child(table_row(&header, &widths, true));
    for user in m.users() {
        let name = if user.name.is_empty() {
            "(unknown)".to_string()
        } else {
            user.name.clone()
        };
        let status = if !user.name.is_empty() && user.name == m.summary.user {
            "Active"
        } else {
            ""
        };
        let cells = [
            name,
            status.to_string(),
            user.processes.to_string(),
            format_whole_percent(user.cpu),
            format_k(user.memory),
        ];
        rows.add_child(table_row(&cells, &widths, false).with_class(ids::USER_ROW));
    }
    Dom::create_div()
        .with_css(format!("display: flex; flex-direction: column; padding: {GAP}px;"))
        .with_child(rows)
}

// ==== The status bar ====

/// The status bar's live labels and what they say now (rewritten in place
/// by every tick): the old Task Manager's "Processes: N", "CPU Usage: x%",
/// "Physical Memory: y%", then the last word and the update speed.
#[must_use]
pub fn status_labels(s: &Monitor) -> Vec<(AzString, String)> {
    let m = &s.model;
    let processes = if m.shown_count() == m.process_count() {
        format!("Processes: {}", m.process_count())
    } else {
        format!("Processes: {} of {}", m.shown_count(), m.process_count())
    };
    let mut word = s.last_word();
    if word.is_empty() {
        word = if s.sample {
            "Sample machine".to_string()
        } else {
            "This computer".to_string()
        };
    }
    let memory = percent_of(m.summary.memory_used, m.summary.memory_total);
    vec![
        (ids::STATUS_PROCESSES, processes),
        (
            ids::STATUS_CPU,
            format!("CPU Usage: {}", format_whole_percent(m.summary.cpu)),
        ),
        (
            ids::STATUS_MEMORY,
            format!("Physical Memory: {}", format_whole_percent(memory)),
        ),
        (ids::STATUS_NOTICE, word),
        (ids::STATUS_SPEED, crate::speed_text(s.interval_ms)),
    ]
}

/// The status bar: every segment a live label.
pub fn status_bar(s: &Monitor) -> Dom {
    let segments: Vec<StatusBarSegment> = status_labels(s)
        .into_iter()
        .map(|(marker, label)| StatusBarSegment::create(label).with_marker(marker))
        .collect();
    StatusBar::create(segments).dom()
}

// ==== The settings ====

/// The "Monitor" settings: the update speed, the history export.
pub fn settings_sections(s: &Monitor, app: &RefAny) -> Vec<kit::AppSection> {
    let labels: Vec<AzString> = SPEEDS.iter().map(|(l, _)| AzString::from(*l)).collect();
    let source = if s.sample {
        "The readings come from the sample machine (--sample): nothing on this computer is ended."
    } else {
        "The readings come from this computer (processes, CPU, memory, disks, networks)."
    };
    let content = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(kit::row(
            "Update speed",
            Segmented::create(StringVec::from_vec(labels))
                .with_selected_index(crate::speed_index(s.interval_ms))
                .with_on_change(app.clone(), on_speed as SegmentedOnChangeCallbackType)
                .dom()
                .with_id(ids::SPEED),
        ))
        .with_child(kit::row(
            "History",
            Button::create("Export the last minute")
                .with_icon("download")
                .with_on_click(app.clone(), on_export as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::EXPORT),
        ))
        .with_child(kit::note(
            "The export is a CSV file in the data folder: monitor/history/<date>.csv.",
        ))
        .with_child(kit::note(source));
    vec![kit::AppSection {
        category: 0,
        title: "Monitor".to_string(),
        content,
    }]
}

extern "C" fn on_speed(mut app: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let Some(mut s) = app.downcast_mut::<Monitor>() else {
        return Update::DoNothing;
    };
    let interval = SPEEDS[state.selected_index.min(SPEEDS.len() - 1)].1;
    crate::set_speed(&mut s, &mut info, interval);
    Update::RefreshDom
}

/// "Export the last minute": the CSV into the data tree, on a Thread.
extern "C" fn on_export(mut app: RefAny, mut info: CallbackInfo) -> Update {
    let reply_to = app.clone();
    let Some((kit_ref, csv)) = app
        .downcast_ref::<Monitor>()
        .map(|s| (s.kit.clone(), s.model.history_csv(s.seconds_per_reading())))
    else {
        return Update::DoNothing;
    };
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let mut kit_handle = kit_ref.clone();
    let Some((root, key)) = kit_handle
        .downcast_ref::<kit::Kit>()
        .map(|k| (k.data_root.clone(), k.key(&format!("history/{stamp}.csv"))))
    else {
        return Update::DoNothing;
    };
    kit::spawn_file_jobs(
        &mut info,
        &root,
        vec![FileJob::Put {
            key,
            bytes: csv.into_bytes(),
        }],
        reply_to,
        EXPORT_TAG,
        on_exported as WriteBackCallbackType,
    );
    Update::DoNothing
}

extern "C" fn on_exported(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let labels = {
        let Some(mut guard) = app.downcast_mut::<Monitor>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let error = reply
            .outcomes
            .iter()
            .find_map(azul_appkit::FileOutcome::error);
        s.notice = match error {
            Some(e) => format!("The history could not be exported: {e}"),
            None => {
                let key = reply
                    .outcomes
                    .iter()
                    .find_map(|o| match o {
                        azul_appkit::FileOutcome::Put { key, .. } => Some(key.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                println!("AZMON_EXPORTED {key}");
                format!("Exported {key}")
            }
        };
        status_labels(s)
    };
    for (marker, label) in labels {
        if let Some(node) = info.get_node_id_by_marker(marker).into_option() {
            let _ = StatusBar::update_segment_label(info, node, label);
        }
    }
    Update::DoNothing
}
