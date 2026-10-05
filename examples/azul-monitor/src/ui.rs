//! The window's parts: the tool row (tabs, filter, End process), the LIVE
//! VIEWS (the cards strip, the process table, the performance page - each a
//! `VirtualView` a tick re-renders in place), the status bar with its live
//! labels, the end-process question and the settings section.
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
        Button, Chart, ChartKind, ChartPoint, ChartSeries, Gauge, GaugeBand, GaugeBandKind,
        GaugeKind, MessageBox, MessageBoxKind, Modal, ModalState, OnTextInputReturn, Segmented,
        SegmentedState, StandardDialogEvent,
        StandardDialogEventKind, StatusBar, StatusBarSegment, TabHeader, TabHeaderState, TextInput,
        TextInputState, TextInputValid,
    },
};
use azul_appkit::{files::FileJob, ui as kit};

use crate::{
    history::History,
    ids,
    model::{format_percent, format_uptime},
    table::{self, format_bytes, format_rate},
    ticks::{LiveView, Screen},
    Confirm, Monitor, SPEEDS,
};

/// The gap between cards and charts, px.
const GAP: f32 = 10.0;
/// A card's headline row, px.
const HEADLINE: f32 = 22.0;
/// The cards strip's height, px.
pub const CARDS_HEIGHT: f32 = 132.0;
/// The write-back tag of the history export.
const EXPORT_TAG: u64 = 1;
/// The space around a core's gauge, px.
const CORE_GAP: f32 = 4.0;

/// The largest core gauge (a diameter from 40 to 96 px) that fits `cores` of them into a box of
/// `width` x `height` px (the smallest when even those do not fit: the box clips the rest).
fn core_gauge_size(cores: usize, width: f32, height: f32) -> f32 {
    let mut size = 96.0_f32;
    while size > 40.0 {
        let cell = size + 2.0 * CORE_GAP;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few cells
        let fits = (width / cell).floor().max(0.0) as usize * (height / cell).floor().max(0.0) as usize;
        if fits >= cores {
            break;
        }
        size -= 4.0;
    }
    size
}

// ==== Helpers ====

/// A line of text in a block (a `<p>` without the UA margin).
fn line(text: impl Into<AzString>, css: &str) -> Dom {
    Dom::create_p_with_text(text).with_css(format!("margin: 0px; {css}"))
}

/// The points of a history for a chart: x in seconds before the newest
/// reading (negative, the newest at 0), `seconds_per_reading` apart.
#[must_use]
#[allow(clippy::cast_precision_loss)] // at most a few hundred readings
pub fn chart_points(values: &[f64], seconds_per_reading: f64) -> Vec<ChartPoint> {
    let newest = values.len().saturating_sub(1);
    values
        .iter()
        .enumerate()
        .map(|(i, v)| ChartPoint::create(-((newest - i) as f64) * seconds_per_reading, *v))
        .collect()
}

/// The top of a rate chart's y axis: a little over the largest reading,
/// never under 1 KB/s (an idle disk would show noise as cliffs).
#[must_use]
pub fn rate_axis_top(histories: &[&History]) -> f64 {
    let peak = histories
        .iter()
        .filter_map(|h| h.max())
        .fold(0.0_f64, f64::max);
    (peak * 1.2).max(1024.0)
}

/// A chart of `series` (name, history) over the last minute.
fn chart(
    kind: ChartKind,
    series: &[(&str, &History)],
    secs: f64,
    size: (f32, f32),
    y_range: (f64, f64),
    title: &str,
) -> Dom {
    let mut c = Chart::create(kind, size.0.max(80.0), size.1.max(48.0))
        .with_y_range(y_range.0, y_range.1)
        .with_shell_accent(ShellThemeAccent::Slate)
        .with_show_legend(series.len() > 1);
    if !title.is_empty() {
        c = c.with_title(title);
    }
    for (name, history) in series {
        c = c.with_added_series(ChartSeries::create(
            *name,
            chart_points(&history.to_vec(), secs),
        ));
    }
    c.dom()
}

/// "9.8 GB of 16 GB".
fn memory_text(used: u64, total: u64) -> String {
    format!("{} of {}", format_bytes(used), format_bytes(total))
}

// ==== The tool row ====

/// The tab row: Processes / Performance, the filter, End process.
pub fn tools(s: &Monitor, app: &RefAny) -> Dom {
    let tabs: Vec<AzString> = Screen::ALL
        .iter()
        .map(|t| AzString::from(t.title()))
        .collect();
    let mut end = Button::create("End process")
        .with_icon("close")
        .with_on_click(app.clone(), on_end_clicked as ButtonOnClickCallbackType);
    if s.model.selected_row().is_none() {
        end = end.with_disabled("Select a process first");
    }
    let mut row = Dom::create_div()
        .with_id(ids::TOOLS)
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 4px 8px;")
        .with_child(
            TabHeader::create(StringVec::from_vec(tabs))
                .with_active_tab(s.screen.index())
                .with_on_click(app.clone(), on_tab as TabOnClickCallbackType)
                .dom(),
        )
        .with_child(Dom::create_div().with_css("flex-grow: 1;"));
    if s.screen == Screen::Processes {
        row.add_child(
            TextInput::create()
                .with_text(s.model.filter())
                .with_placeholder("Filter by name, user or PID")
                .with_accessibility_name("Filter processes")
                .with_on_text_input(app.clone(), on_filter as TextInputOnTextInputCallbackType)
                .dom()
                .with_id(ids::FILTER)
                .with_css("width: 240px; margin-right: 8px;"),
        );
        row.add_child(end.dom().with_id(ids::END_PROCESS));
    }
    row
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

extern "C" fn on_tab(mut app: RefAny, _info: CallbackInfo, state: TabHeaderState) -> Update {
    let Some(mut s) = app.downcast_mut::<Monitor>() else {
        return Update::DoNothing;
    };
    let screen = Screen::at(state.active_tab);
    if screen == s.screen {
        return Update::DoNothing;
    }
    s.screen = screen;
    println!("AZMON_SCREEN {}", screen.name());
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
        let selected = s.model.selected_position();
        let shown = s.model.shown_count();
        s.table.top = 0;
        table::follow_selection(&mut s.table, selected, shown);
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
        LiveView::Cards => ids::CARDS,
        LiveView::Table => ids::TABLE_VIEW,
        LiveView::Performance => ids::PERFORMANCE,
    }
}

/// A live view: a VirtualView whose callback reads the model, re-rendered
/// in place by every tick.
pub fn live_view(app: &RefAny, view: LiveView) -> Dom {
    let css = match view {
        LiveView::Cards => format!("width: 100%; height: {CARDS_HEIGHT}px; flex-shrink: 0;"),
        LiveView::Table | LiveView::Performance => {
            "width: 100%; flex-grow: 1; min-height: 0px;".to_string()
        }
    };
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
    .with_css(css)
}

/// A live view's callback: its content at the view's size, in its own
/// theme scope.
extern "C" fn render_live(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let Some((app, view)) = data.downcast_ref::<Live>().map(|l| (l.app.clone(), l.view)) else {
        return VirtualViewReturn::default();
    };
    let size = info.bounds.get_logical_size();
    let (w, h) = (size.width.max(1.0), size.height.max(1.0));
    let content = match view {
        LiveView::Cards => cards(&app, w, h),
        LiveView::Table => process_table(&app, w, h),
        LiveView::Performance => performance(&app, w, h),
    };
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

/// The process table at `w` x `h` (the guard on the app is dropped before
/// the table asks the app for its cells).
fn process_table(app: &RefAny, w: f32, h: f32) -> Dom {
    let mut handle = app.clone();
    let Some((view, rows)) = handle.downcast_ref::<Monitor>().map(|s| {
        (
            s.table.clone(),
            u32::try_from(s.model.shown_count()).unwrap_or(u32::MAX),
        )
    }) else {
        return Dom::create_div();
    };
    table::table(app, view, rows, w, h).dom()
}

/// The cards strip: CPU, memory, disk, network - the value now and a
/// minute of history each.
fn cards(app: &RefAny, w: f32, h: f32) -> Dom {
    let mut handle = app.clone();
    let Some(s) = handle.downcast_ref::<Monitor>() else {
        return Dom::create_div();
    };
    let m = &s.model;
    let sum = &m.summary;
    let secs = s.seconds_per_reading();
    let card_w = ((w - 5.0 * GAP) / 4.0).max(120.0);
    let chart_h = (h - 2.0 * GAP - HEADLINE).max(48.0);
    let disk_top = rate_axis_top(&[&m.disk_read, &m.disk_write]);
    let net_top = rate_axis_top(&[&m.net_in, &m.net_out]);
    let card = |id: AzString, headline: String, body: Dom| {
        Dom::create_div()
            .with_id(id)
            .with_class(ids::CARD)
            .with_css(format!(
                "display: flex; flex-direction: column; width: {card_w}px; margin-left: {GAP}px;"
            ))
            .with_child(
                line(
                    headline,
                    &format!("height: {HEADLINE}px; font-size: 13px; font-weight: 600;"),
                )
                .with_class(ids::CARD_VALUE),
            )
            .with_child(body)
    };
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; padding-top: {GAP}px;"
        ))
        .with_child(card(
            ids::CARD_CPU,
            format!("CPU {}", format_percent(sum.cpu)),
            chart(
                ChartKind::Area,
                &[("CPU", &m.cpu)],
                secs,
                (card_w, chart_h),
                (0.0, 100.0),
                "",
            ),
        ))
        .with_child(card(
            ids::CARD_MEMORY,
            format!("Memory {}", memory_text(sum.memory_used, sum.memory_total)),
            chart(
                ChartKind::Area,
                &[("Memory", &m.memory)],
                secs,
                (card_w, chart_h),
                (0.0, 100.0),
                "",
            ),
        ))
        .with_child(card(
            ids::CARD_DISK,
            format!(
                "Disk {}",
                format_rate(sum.disk_read_rate + sum.disk_write_rate)
            ),
            chart(
                ChartKind::Line,
                &[("Read", &m.disk_read), ("Write", &m.disk_write)],
                secs,
                (card_w, chart_h),
                (0.0, disk_top),
                "",
            ),
        ))
        .with_child(card(
            ids::CARD_NETWORK,
            format!(
                "Network {}",
                format_rate(sum.net_in_rate + sum.net_out_rate)
            ),
            chart(
                ChartKind::Line,
                &[("In", &m.net_in), ("Out", &m.net_out)],
                secs,
                (card_w, chart_h),
                (0.0, net_top),
                "",
            ),
        ))
}

/// The performance page: the CPU over the last minute beside every core's
/// usage, memory / disk / network under it, the machine's figures last.
fn performance(app: &RefAny, w: f32, h: f32) -> Dom {
    let mut handle = app.clone();
    let Some(s) = handle.downcast_ref::<Monitor>() else {
        return Dom::create_div();
    };
    let m = &s.model;
    let sum = &m.summary;
    let secs = s.seconds_per_reading();
    let stats_h = 28.0;
    let row_h = ((h - stats_h - 4.0 * GAP) / 2.0).max(120.0);
    let cpu_w = ((w - 3.0 * GAP) * 0.62).max(200.0);
    let cores_w = (w - 3.0 * GAP - cpu_w).max(160.0);
    let third = ((w - 4.0 * GAP) / 3.0).max(160.0);

    // One compact ring gauge per core: warm from 70 %, hot from 90 %.
    let mut cores = Dom::create_div().with_id(ids::CORES).with_css(format!(
        "display: flex; flex-direction: row; flex-wrap: wrap; align-content: flex-start; width: \
         {cores_w}px; height: {row_h}px; overflow: hidden; margin-left: {GAP}px;"
    ));
    let size = core_gauge_size(m.cores.len(), cores_w, row_h);
    for (i, core) in m.cores.iter().enumerate() {
        let pct = core.latest().filter(|v| v.is_finite()).unwrap_or(0.0).clamp(0.0, 100.0);
        cores.add_child(
            Gauge::create(pct, 0.0, 100.0)
                .with_kind(GaugeKind::Ring)
                .with_size(size)
                .with_thickness((size / 9.0).max(4.0))
                .with_value_text(format_percent(pct))
                .with_label(format!("Core {i}"))
                .with_band(GaugeBand::create(70.0, 90.0, GaugeBandKind::Warn))
                .with_band(GaugeBand::create(90.0, 100.0, GaugeBandKind::Bad))
                .dom()
                .with_class(ids::CORE)
                .with_css(format!("margin: {CORE_GAP}px;")),
        );
    }
    let disk_top = rate_axis_top(&[&m.disk_read, &m.disk_write]);
    let net_top = rate_axis_top(&[&m.net_in, &m.net_out]);
    let mut brand = sum.cpu_brand.clone();
    if sum.cpu_mhz > 0 {
        brand.push_str(&format!(", {} MHz", sum.cpu_mhz));
    }
    let stats = format!(
        "{}   Processes {}   Uptime {}   Memory {}   Swap {}",
        brand,
        sum.processes,
        format_uptime(sum.uptime),
        memory_text(sum.memory_used, sum.memory_total),
        memory_text(sum.swap_used, sum.swap_total),
    );
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; padding: {GAP}px;"
        ))
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row;")
                .with_child(
                    chart(
                        ChartKind::Area,
                        &[("CPU", &m.cpu)],
                        secs,
                        (cpu_w, row_h),
                        (0.0, 100.0),
                        &format!("CPU {}", format_percent(sum.cpu)),
                    )
                    .with_id(ids::CHART_CPU),
                )
                .with_child(cores),
        )
        .with_child(
            Dom::create_div()
                .with_css(format!(
                    "display: flex; flex-direction: row; margin-top: {GAP}px;"
                ))
                .with_child(
                    chart(
                        ChartKind::Area,
                        &[("Memory", &m.memory)],
                        secs,
                        (third, row_h),
                        (0.0, 100.0),
                        &format!("Memory {}", memory_text(sum.memory_used, sum.memory_total)),
                    )
                    .with_id(ids::CHART_MEMORY),
                )
                .with_child(
                    chart(
                        ChartKind::Line,
                        &[("Read", &m.disk_read), ("Write", &m.disk_write)],
                        secs,
                        (third, row_h),
                        (0.0, disk_top),
                        &format!(
                            "Disk {}",
                            format_rate(sum.disk_read_rate + sum.disk_write_rate)
                        ),
                    )
                    .with_id(ids::CHART_DISK)
                    .with_css(format!("margin-left: {GAP}px;")),
                )
                .with_child(
                    chart(
                        ChartKind::Line,
                        &[("In", &m.net_in), ("Out", &m.net_out)],
                        secs,
                        (third, row_h),
                        (0.0, net_top),
                        &format!(
                            "Network {}",
                            format_rate(sum.net_in_rate + sum.net_out_rate)
                        ),
                    )
                    .with_id(ids::CHART_NETWORK)
                    .with_css(format!("margin-left: {GAP}px;")),
                ),
        )
        .with_child(
            line(
                stats,
                &format!("height: {stats_h}px; padding-top: 8px; font-size: 12px;"),
            )
            .with_id(ids::STATS),
        )
}

// ==== The status bar ====

/// The status bar's live labels and what they say now (rewritten in place
/// by every tick).
#[must_use]
pub fn status_labels(s: &Monitor) -> Vec<(AzString, String)> {
    let m = &s.model;
    let processes = if m.shown_count() == m.process_count() {
        format!("{} processes", m.process_count())
    } else {
        format!("{} of {} processes", m.shown_count(), m.process_count())
    };
    let mut word = s.last_word();
    if word.is_empty() {
        word = if s.sample {
            "Sample machine".to_string()
        } else {
            "This computer".to_string()
        };
    }
    vec![
        (ids::STATUS_PROCESSES, processes),
        (
            ids::STATUS_CPU,
            format!("CPU {}", format_percent(m.summary.cpu)),
        ),
        (
            ids::STATUS_MEMORY,
            format!(
                "Memory {}",
                memory_text(m.summary.memory_used, m.summary.memory_total)
            ),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_history_charts_as_seconds_before_now() {
        let p = chart_points(&[5.0, 6.0, 7.0], 1.0);
        let xy: Vec<(f64, f64)> = p.iter().map(|p| (p.x, p.y)).collect();
        assert_eq!(xy, vec![(-2.0, 5.0), (-1.0, 6.0), (0.0, 7.0)]);
        let slow = chart_points(&[1.0, 2.0], 2.0);
        assert_eq!(slow[0].x, -2.0);
        assert!(chart_points(&[], 1.0).is_empty());
    }

    #[test]
    fn a_rate_axis_leaves_room_over_the_peak_and_never_shows_noise_as_cliffs() {
        let mut a = History::new(4);
        let mut b = History::new(4);
        assert_eq!(rate_axis_top(&[&a, &b]), 1024.0);
        a.push(10.0);
        b.push(100.0);
        assert_eq!(rate_axis_top(&[&a, &b]), 1024.0);
        a.push(10_000.0);
        assert_eq!(rate_axis_top(&[&a, &b]), 12_000.0);
    }
}
