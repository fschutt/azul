//! What a phone's commands do to the review window, and what the window tells the phone of it: the executor of the companion's
//! [`Command`](crate::companion_plug::Command)s, run by the timer that polls the host (it has the window's state and a window's
//! callback info; the glue has neither).
//!
//! A viewport command moves the window's view of the pages by the zoom model (`page_view`): the pages' zoom is the app's (`AppState::zoom`,
//! the pages are laid out at it) and the strip is scrolled to where the model says. A change of the zoom lays the pages out again, and the
//! strip can only be scrolled to a place that is in the pages as they are laid out: the scroll waits a moment for them (`settle_later`).
//! What the phone is told (`publish`) is the pen state of `companion_pen`.

use azul::{error::CustomE2eOpResult, json::Json, prelude::*, str::String as AzString, task::TerminateTimer, time::SystemTimeDiff};
use azul_appkit::ui as kit;
use azul_review_link::{
    frame::{Entry, EntryKind, FileState, Focus, PageSurface, PenState, Phase, Rect, ReplyStatus, Rows, Surface, ViewportOp},
    limits::{cut_bytes, MAX_ENTRY_NAME_BYTES, MAX_ROWS},
};

use crate::{
    agent_window, board_view,
    companion_pen::{self, BoardInput, PenAt},
    companion_plug::with_companion,
    model::{Note, NoteState},
    page_view::{Applied, Context, Geometry, Looking},
    symbols_source::DirEntry,
    tree, ui,
    whiteboard_geom::{Pt, View},
    whiteboard_ui::SURFACE_ID,
    AppState,
};

/// What the window knows of the pages that the zoom model needs: its geometry from the layout, where the window is now, and where the pen is.
fn window_of(s: &AppState, strip: &ui::StripNow) -> Option<(Geometry, Looking, Option<(f32, f32)>)> {
    let file = s.file()?;
    let geometry = Geometry { page_w: ui::PAGE_W, page_h: ui::page_h_with(s.most_extra_rows()), gap: ui::PAGE_GAP, pages: file.page_count(), window: strip.size };
    let looking = Looking::from_scroll(&geometry, s.page_zoom(), strip.scroll);
    let pen = s.pen_seen.filter(|(_, at)| at.elapsed().as_millis() <= u128::from(companion_pen::PEN_AWAY_MS)).map(|(at, _)| crate::page_view::pen_in_column(&geometry, at.page, (at.x, at.y)));
    Some((geometry, looking, pen))
}

/// A command of the phone for the window's view of the pages: the model says where the window goes, the pages are laid out at the zoom it
/// says and the strip is scrolled there. `true` when the window changed.
pub fn viewport(app: &RefAny, info: &mut CallbackInfo, op: ViewportOp) -> bool {
    if board_is_surface(app) {
        return board_viewport(app, info, op);
    }
    let Some(strip) = ui::strip_now(info) else {
        return false;
    };
    let planned = with_companion(app, |g, s| {
        let (geometry, now, pen) = window_of(s, &strip)?;
        let context = Context { snap_to_page_top: g.host_settings().snap_to_page_top, pen };
        let applied = crate::page_view::apply(&geometry, now, &mut g.held, op, context);
        Some((geometry, now, applied))
    })
    .flatten();
    let Some((geometry, now, applied)) = planned else {
        return false;
    };
    let (changed, zoom_changed) = settle(app, &geometry, now, &applied);
    let target = applied.looking.scroll(&geometry);
    if zoom_changed {
        // the pages are laid out again at the new zoom; the strip is scrolled to its place when they are
        info.trigger_all_virtual_view_rerender();
        settle_later(app, info, target);
    } else if changed {
        ui::scroll_strip_to(info, target.0, target.1);
    }
    with_companion(app, |g, _| g.touch());
    changed || zoom_changed
}

// ==== the whiteboard under the pen ====

/// The whiteboard's canvas as a timer sees the window: where it is in the window and how large it is.
#[derive(Debug, Clone, Copy)]
pub struct Canvas {
    pub origin: (f32, f32),
    pub size: (f32, f32),
    node: DomNodeId,
}

/// The canvas now (`None`: the panel is closed, or the window has not laid it out).
#[must_use]
pub fn canvas_now(info: &CallbackInfo) -> Option<Canvas> {
    let dom = DomId { inner: 0 };
    let node = info.get_node_id_by_id_attribute(dom, SURFACE_ID);
    if node.into_raw() == 0 {
        return None;
    }
    let at = DomNodeId { dom, node };
    let rect = info.get_node_rect(at).into_option()?;
    Some(Canvas { origin: (rect.origin.x, rect.origin.y), size: (rect.size.width, rect.size.height), node: at })
}

/// Whether the pad is over the whiteboard: the pointer was last over its panel, and the panel is open.
fn board_is_surface(app: &RefAny) -> bool {
    let mut handle = app.clone();
    handle.downcast_ref::<AppState>().is_some_and(|s| s.over_board && s.panels.right_open)
}

/// A number for a board's name, for the phone (its id is words): the same name is always the same number.
#[must_use]
pub fn board_number(id: &str) -> u32 {
    id.bytes().fold(2_166_136_261_u32, |hash, byte| (hash ^ u32::from(byte)).wrapping_mul(16_777_619))
}

/// The board under the pen as the pen state says it: its geometry (the box round its cutouts, the canvas, the zoom the board may be
/// drawn at), its view, which board it is and where the pointer is over its canvas.
fn board_input(s: &AppState, canvas: &Canvas) -> Option<BoardInput> {
    let glue = s.wb.try_borrow().ok()?;
    let board = &glue.panel.wb.board;
    let limits = *board.limits();
    let geometry = board_view::Geometry { items: board_view::box_round(board.cutouts().map(|c| c.rect)), window: canvas.size, zoom: (limits.zoom_min, limits.zoom_max) };
    let view = board.view();
    let (id, name) = glue.panel.wb.active_board();
    let pointer = s.pointer_at.filter(|_| s.over_board).map(|(x, y)| view.to_board(Pt::new(x - canvas.origin.0, y - canvas.origin.1)));
    Some(BoardInput { geometry, view, board_id: board_number(id), name: name.to_string(), items: board.len(), pointer })
}

/// A command of the phone for the whiteboard's view (the pen is over it): a free pan and zoom of the board's canvas by the board's zoom
/// model (`board_view`). `true` when the window changed.
fn board_viewport(app: &RefAny, info: &mut CallbackInfo, op: ViewportOp) -> bool {
    let Some(canvas) = canvas_now(info) else {
        return false;
    };
    let applied = with_companion(app, |g, s| {
        let mut glue = s.wb.try_borrow_mut().ok()?;
        let board = &mut glue.panel.wb.board;
        let limits = *board.limits();
        let geometry = board_view::Geometry { items: board_view::box_round(board.cutouts().map(|c| c.rect)), window: canvas.size, zoom: (limits.zoom_min, limits.zoom_max) };
        let now = board.view();
        let pen = s.pointer_at.map(|(x, y)| geometry.to_units(now.to_board(Pt::new(x - canvas.origin.0, y - canvas.origin.1))));
        let applied = board_view::apply(&geometry, now, &mut g.held, op, pen);
        board.set_view(applied.view);
        Some(applied)
    })
    .flatten();
    let Some(applied) = applied else {
        return false;
    };
    let _ = applied.zoom_held;
    kit::redraw_image(info, canvas.node);
    with_companion(app, |g, _| g.touch());
    true
}

/// The pointer moved anywhere in the window: when the companion is on, where it is and whether it is over the whiteboard's panel is what the
/// phone's pad is about (the pages' or the board's), and it is told when that changes.
pub extern "C" fn on_window_move(mut data: RefAny, info: CallbackInfo) -> Update {
    let on = data.downcast_ref::<AppState>().is_some_and(|s| s.companion.try_borrow().is_ok_and(|g| g.stored.enabled));
    if !on {
        return Update::DoNothing;
    }
    let Some(at) = info.get_cursor_position().into_option() else {
        return Update::DoNothing;
    };
    let size = info.get_current_window_state().size.dimensions;
    let over = crate::whiteboard_plug::point_is_on_panel(&data, size.width, size.height, at.x, at.y);
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        let changed = s.over_board != over;
        s.over_board = over;
        s.pointer_at = Some((at.x, at.y));
        if changed {
            // a gesture that was under way belongs to the surface the pointer has left
            if let Ok(mut companion) = s.companion.try_borrow_mut() {
                companion.held = crate::page_view::Held::default();
            }
        }
        if changed || over {
            s.touch_companion();
        }
    }
    Update::DoNothing
}

/// How long the pages are given to be laid out at their new zoom before the strip is scrolled, ms.
const SETTLE_MS: u64 = 30;

/// The strip is to be scrolled to `target` (px of the screen) once the pages are laid out at their new zoom: a timer that fires once does
/// it, and a target that comes before it fires replaces the one that waits (the newest of a pinch's steps is the one that counts).
fn settle_later(app: &RefAny, info: &mut CallbackInfo, target: (f32, f32)) {
    let start = with_companion(app, |g, _| {
        g.pending_scroll = Some(target);
        !std::mem::replace(&mut g.settling, true)
    })
    .unwrap_or(false);
    if start {
        let after = || Duration::System(SystemTimeDiff::from_millis(SETTLE_MS));
        let timer = Timer::create(app.clone(), on_settle, info.get_system_time_fn()).with_delay(after()).with_interval(after());
        info.add_timer(TimerId::unique(), timer);
    }
}

extern "C" fn on_settle(data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let target = with_companion(&data, |g, _| {
        g.settling = false;
        g.pending_scroll.take()
    })
    .flatten();
    if let Some((x, y)) = target {
        ui::scroll_strip_to(&mut info.callback_info, x, y);
        with_companion(&data, |g, _| g.touch());
    }
    TimerCallbackReturn { should_update: Update::DoNothing, should_terminate: TerminateTimer::Terminate }
}

/// The app's state takes what the model said: the zoom and the page in view. `(the window moved, the zoom changed)`.
fn settle(app: &RefAny, geometry: &Geometry, now: Looking, applied: &Applied) -> (bool, bool) {
    let mut handle = app.clone();
    let Some(mut s) = handle.downcast_mut::<AppState>() else {
        return (false, false);
    };
    let zoom_changed = (applied.looking.zoom - s.page_zoom()).abs() > 1e-4;
    if zoom_changed {
        s.zoom = applied.looking.zoom;
    }
    s.visible_page = applied.looking.page;
    let moved = zoom_changed || applied.looking.page != now.page || scroll_differs(applied.looking.scroll(geometry), now.scroll(geometry));
    (moved, zoom_changed)
}

/// Whether two scroll offsets differ by more than a tenth of a px.
fn scroll_differs(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() > 0.1 || (a.1 - b.1).abs() > 0.1
}

/// Where the conversation of a quick button is anchored: at the marked region if a loop waits (the page it is on, the middle of its
/// height), else at the line under the pen (where it was last seen on a page), else at the head of the page in view. Held to the page's
/// own px as a dictated note's anchor is.
#[must_use]
pub fn anchor_of(region: Option<(usize, f32)>, pen: Option<(usize, f32)>, visible_page: usize, pages: usize) -> (usize, f32) {
    let (page, y) = region.or(pen).unwrap_or((visible_page, ui::PAGE_PAD));
    let y = if y.is_finite() { y } else { ui::PAGE_PAD };
    (page.min(pages.saturating_sub(1)), y.clamp(ui::PAGE_PAD, ui::page_h() - ui::PAGE_PAD))
}

/// A quick button of a phone: its instruction is a note of a conversation at the context of the pen, as a dictated one is, and the agent
/// is asked about it; the phone is told an agent is at work (and how it ends, when the agents' window tells that the conversation finished:
/// `Glue::tell_finished`). `true` when the window changed.
pub fn button(app: &RefAny, info: &mut CallbackInfo, peer: &str, request_id: u32, text: &str) -> bool {
    let mut data = app.clone();
    let made = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return false;
        };
        let Some((file, pages)) = s.file().map(|f| (f.display.clone(), f.page_count())) else {
            drop(s);
            with_companion(app, |g, _| g.button_refused(peer, request_id));
            return false;
        };
        let region = s.wb.try_borrow().ok().and_then(|g| g.panel.wb.armed.all().first().map(|r| (r.page, r.bounds.y + r.bounds.h / 2.0)));
        let pen = s
            .pen_seen
            .filter(|(_, at)| at.elapsed().as_millis() <= u128::from(companion_pen::PEN_AWAY_MS))
            .map(|(at, _): (PenAt, _)| (at.page, at.y));
        let (page, y) = anchor_of(region, pen, s.visible_page, pages);
        let id = s.next_note_id;
        s.next_note_id += 1;
        let language = crate::voice::language_of(&s.kit);
        let mut note = Note::listening(id, page, 120.0, y, &language);
        note.text = text.to_string();
        note.state = NoteState::Done;
        s.start_conversation(note);
        s.sync_chip(id);
        (id, file)
    };
    let (id, file) = made;
    let _ = agent_window::ask(&mut data, info, id);
    // did the agent start? (a pool that is full, or no agent, is the conversation's own failure, which it shows)
    let working = data.downcast_ref::<AppState>().is_some_and(|s| s.hub.is_working_on(&file, id));
    with_companion(app, |g, _| if working { g.button_started(id, peer, request_id) } else { g.button_refused(peer, request_id) });
    info.trigger_all_virtual_view_rerender();
    true
}

// ==== the files ====

/// The rows of a folder as the phone is given them: what the tree would show of `entries` (hidden files and the folders nobody reviews are
/// left out, folders come first, the order is the tree's), from row `offset` on, at most [`MAX_ROWS`] to a frame (`more` says there are
/// rows after them), each name cut to what a name may be and each row with the state `state_of(name, is_folder)` says.
#[must_use]
pub fn rows_of(request_id: u32, dir: &str, offset: u32, entries: &[DirEntry], state_of: &dyn Fn(&str, bool) -> FileState) -> Rows {
    // the tree's own rules and order: a listing of the folder in a tree of its own
    let mut shown = tree::Tree::new();
    let root = shown.root();
    let _listed = shown.list(root, entries, &tree::Rules::default());
    let all: Vec<(String, bool)> = shown
        .children(root)
        .iter()
        .filter_map(|id| shown.node(*id))
        .map(|node| (node.name.clone(), node.kind == tree::Kind::Dir))
        .collect();
    let start = usize::try_from(offset).unwrap_or(usize::MAX).min(all.len());
    let end = start.saturating_add(MAX_ROWS).min(all.len());
    let entries = all[start..end]
        .iter()
        .map(|(name, is_dir)| Entry {
            name: cut_bytes(name, MAX_ENTRY_NAME_BYTES, "file name").0,
            kind: if *is_dir { EntryKind::Dir } else { EntryKind::File },
            state: state_of(name, *is_dir),
        })
        .collect();
    Rows { request_id, dir: dir.to_string(), offset: u32::try_from(start).unwrap_or(u32::MAX), entries, more: end < all.len() }
}

/// The phone's colour of a file or folder: what AzReview shows in its own tree (blue an agent at work, yellow a suggestion that waits, green
/// all done), nothing for what nothing was done to.
#[must_use]
pub fn file_state_of(mark: Option<crate::chips::State>) -> FileState {
    match mark {
        Some(crate::chips::State::Working) => FileState::Working,
        Some(crate::chips::State::Suggested) => FileState::Unresolved,
        Some(crate::chips::State::Done) => FileState::Clean,
        _ => FileState::None,
    }
}

/// The rows of folder `path` for the phone that asked: listed from the workspace's source (the host has checked that the path is inside it),
/// answered with what the tree would show, or `Failed` when the folder cannot be listed.
pub fn list_dir(app: &RefAny, peer: &str, request_id: u32, path: &str, offset: u32) -> bool {
    let mut data = app.clone();
    let rows = {
        let Some(s) = data.downcast_ref::<AppState>() else {
            return false;
        };
        let listed = s.source.list(path);
        listed.ok().map(|entries| {
            let state_of = |name: &str, _is_dir: bool| {
                let at = if path.is_empty() { name.to_string() } else { format!("{path}/{name}") };
                file_state_of(s.tree.find(&at).and_then(|id| s.file_mark(id)))
            };
            rows_of(request_id, path, offset, &entries, &state_of)
        })
    };
    with_companion(app, |g, _| g.answer_rows(peer, request_id, rows));
    false
}

/// A phone asks for a file: it opens (its folders are listed if the tree has not yet), at the line's page, or at the first page that was
/// not seen; the phone is told it is done, or that it could not be. `true` when the window changed.
pub fn open_file(app: &RefAny, info: &mut CallbackInfo, peer: &str, request_id: u32, path: &str, line: Option<u32>) -> bool {
    let mut data = app.clone();
    let page = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return false;
        };
        let open_already = s.file().is_some_and(|f| f.display == path);
        if open_already || s.open_path_listed(path) {
            let pages = s.file().map_or(1, crate::code::SourceFile::page_count);
            let unseen = |s: &mut AppState| s.progress.entry(path).and_then(|e| (0..pages).find(|p| !e.seen.is_seen(*p)));
            let page = match line {
                Some(line) => crate::code::SourceFile::page_of_line(line as usize),
                None => unseen(&mut s).unwrap_or(0),
            };
            let page = page.min(pages.saturating_sub(1));
            s.visible_page = page;
            s.touch_companion();
            Some(page)
        } else {
            None
        }
    };
    let status = if page.is_some() { ReplyStatus::Done } else { ReplyStatus::Failed };
    with_companion(app, |g, _| g.answer(peer, request_id, status));
    let Some(page) = page else {
        return false;
    };
    ui::scroll_to_page(info, page, ui::zoom_of(&data));
    info.trigger_all_virtual_view_rerender();
    true
}

/// "Continue reviewing" on a phone: the window goes back to the place the jump to a conversation began (and the place is forgotten, so the
/// next pen state has none). `true` when it went.
pub fn continue_reviewing(app: &RefAny, info: &mut CallbackInfo) -> bool {
    let mut data = app.clone();
    let went = matches!(crate::come_back(&mut data, info), Update::RefreshDom);
    with_companion(app, |g, _| g.touch());
    went
}

/// Makes the pen state of the window as it is now and gives it to the host (which sends it at most thirty times a second, only when it
/// changed). Nothing without a file: the phone keeps what it had.
pub fn publish(app: &RefAny, info: &CallbackInfo) {
    let strip = ui::strip_now(info);
    let canvas = canvas_now(info);
    with_companion(app, |g, s| {
        let state = pen_state_of(s, strip.as_ref(), canvas.as_ref());
        if let Some(host) = g.host_mut() {
            host.publish_pen(state);
        }
    });
}

/// The pen state of the window: the file and the page in view, the view of it, the pen and what a button would act on, and the place to go
/// back to.
#[must_use]
pub fn pen_state_of(s: &AppState, strip: Option<&ui::StripNow>, canvas: Option<&Canvas>) -> PenState {
    let folder = s.root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    // over the whiteboard the pad is the board's
    let board = canvas.filter(|_| s.over_board && s.panels.right_open).and_then(|c| board_input(s, c));
    let (Some(file), Some(strip)) = (s.file(), strip) else {
        // nothing to look at: no file is open, or the pages are not laid out yet
        if let Some(board) = board {
            let back = s.progress.back().map(|b| (b.file.clone(), b.page, b.line, b.words()));
            let region = s.wb.try_borrow().is_ok_and(|g| !g.panel.wb.armed.is_empty());
            let geometry = Geometry { page_w: ui::PAGE_W, page_h: ui::page_h(), gap: ui::PAGE_GAP, pages: 1, window: (1.0, 1.0) };
            return companion_pen::pen_state(&companion_pen::Input { board: Some(board), folder: &folder, file: "", line: 0, line_count: 0, geometry, looking: Looking::HOME, pen: None, region, back });
        }
        return PenState { folder, surface: Surface::Page(PageSurface::empty()), view: Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }, ..PenState::empty() };
    };
    let geometry = Geometry { page_w: ui::PAGE_W, page_h: ui::page_h_with(s.most_extra_rows()), gap: ui::PAGE_GAP, pages: file.page_count(), window: strip.size };
    let looking = Looking::from_scroll(&geometry, s.page_zoom(), strip.scroll);
    let pen = s.pen_seen.map(|(at, when)| (at, u64::try_from(when.elapsed().as_millis()).unwrap_or(u64::MAX)));
    // the line under the pen: the page's first line and the rows down to it
    let line = pen
        .map(|(at, _): (PenAt, u64)| crate::code::SourceFile::first_line_of(at.page) + ui::line_of(at.y))
        .filter(|line| *line <= file.line_count())
        .unwrap_or(0);
    let region = s.wb.try_borrow().is_ok_and(|g| !g.panel.wb.armed.is_empty());
    let back = s.progress.back().map(|b| (b.file.clone(), b.page, b.line, b.words()));
    companion_pen::pen_state(&companion_pen::Input {
        board,
        folder: &folder,
        file: &file.display,
        line,
        line_count: file.line_count(),
        geometry,
        looking,
        pen,
        region,
        back,
    })
}

// ==== the debug server's op ====

/// The op that plays a viewport command of a phone, for the scripts: the same executor, with no phone and no network.
pub const OP_NAME: &str = "azreview_companion_viewport";

/// The op's arguments as the command they stand for.
///
/// # Errors
/// A sentence for the script: arguments that are no JSON, or an op that is none of the four.
pub fn parse_op(args: &str) -> Result<ViewportOp, String> {
    let json: serde_json::Value = serde_json::from_str(args).map_err(|e| format!("the arguments are no JSON: {e}"))?;
    let number = |key: &str, default: f32| json.get(key).and_then(serde_json::Value::as_f64).map_or(default, |v| v as f32);
    match json.get("op").and_then(serde_json::Value::as_str) {
        Some("fit") => Ok(ViewportOp::Fit),
        Some("reset") => Ok(ViewportOp::Reset),
        Some("page_step") => {
            let steps = json.get("steps").and_then(serde_json::Value::as_i64).unwrap_or(1).clamp(-8, 8);
            Ok(ViewportOp::PageStep(i8::try_from(steps).unwrap_or(0)))
        }
        Some("gesture") => {
            let phase = match json.get("phase").and_then(serde_json::Value::as_str) {
                Some("begin") => Phase::Begin,
                Some("end") => Phase::End,
                _ => Phase::Update,
            };
            let focus = match json.get("focus").and_then(serde_json::Value::as_str) {
                Some("center") => Focus::Center,
                _ => Focus::Pen,
            };
            let id = json.get("id").and_then(serde_json::Value::as_u64).map_or(1, |id| u32::try_from(id).unwrap_or(u32::MAX));
            Ok(ViewportOp::Gesture { id, phase, pan_x: number("pan_x", 0.0), pan_y: number("pan_y", 0.0), zoom: number("zoom", 1.0), focus })
        }
        other => Err(format!("\"op\" is one of gesture, fit, page_step, reset, not {other:?}")),
    }
}

/// Adds the op to `config`.
pub fn register_ops(config: &mut AppConfig) {
    let def = format!(
        "{{\"name\":\"{OP_NAME}\",\"summary\":\"play a viewport command of the phone companion (no phone, no network)\",\
         \"description\":\"op: gesture, fit, page_step or reset; for a gesture: id, phase (begin, update, end), pan_x and pan_y (sheet widths), \
         zoom (a factor since the begin), focus (pen or center); for page_step: steps. Answers where the window is.\",\
         \"args\":[{{\"name\":\"op\",\"type\":\"string\"}},{{\"name\":\"id\",\"type\":\"number\"}},{{\"name\":\"phase\",\"type\":\"string\"}},\
         {{\"name\":\"pan_x\",\"type\":\"number\"}},{{\"name\":\"pan_y\",\"type\":\"number\"}},{{\"name\":\"zoom\",\"type\":\"number\"}},\
         {{\"name\":\"focus\",\"type\":\"string\"}},{{\"name\":\"steps\",\"type\":\"number\"}}],\"examples\":[]}}"
    );
    let json = Json::parse(AzString::from(def.as_str())).into_result().unwrap_or_else(|_| Json::null());
    config.add_e2e_command(AzString::from(OP_NAME), json, on_op);
}

fn answer(json: &str) -> CustomE2eOpResult {
    CustomE2eOpResult { handled: true, json: AzString::from(json) }
}

extern "C" fn on_op(data: RefAny, mut info: CallbackInfo, name: AzString, args: AzString) -> CustomE2eOpResult {
    if name.as_str() != OP_NAME {
        return CustomE2eOpResult { handled: false, json: AzString::from("") };
    }
    let op = match parse_op(args.as_str()) {
        Ok(op) => op,
        Err(why) => return answer(&serde_json::json!({"success": false, "error": why}).to_string()),
    };
    let moved = viewport(&data, &mut info, op);
    let (zoom, page) = {
        let mut handle = data.clone();
        handle.downcast_ref::<AppState>().map_or((1.0, 0), |s| (s.page_zoom(), s.visible_page))
    };
    answer(&serde_json::json!({"success": true, "moved": moved, "zoom": zoom, "page": page}).to_string())
}

#[cfg(test)]
#[path = "companion_cmd_test.rs"]
mod companion_cmd_test;
