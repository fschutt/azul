//! A window to look at the whiteboard in: a page of code on the left (a file, 46 lines of it), the
//! board's bar on the right. Draw a loop round some code with the mouse or the pen: the region arms
//! (its outline and its grip show); press the grip, or press and hold inside the loop, then drag it onto the
//! bar: the cutout is drawn by the CPU renderer on a Thread and lies where it was dropped. Drag a rim to
//! draw an arrow, drag the body to move, the corner to resize, double-click to jump back to the lines
//! (they flash), Delete takes the selection off, Mod+Z steps back; the board is kept in the data folder and
//! comes back at the next start.
//!
//! It is the whiteboard's own test window until the review window has its right bar: everything it does
//! goes through [`Whiteboard`] and [`crate::whiteboard_ui`], the very calls the review window makes.
//! `scripts/azwhiteboard_e2e.py` drives it. stdout, for scripts: `WBDEMO_ARMED <first> <last>`,
//! `WBDEMO_HELD` (a press inside a loop was held), `WBDEMO_LIFTED` (the region follows the pointer),
//! `WBDEMO_DROPPED`, `WBDEMO_RENDERED <w>x<h> <ms>ms | <why>`, `WBDEMO_SAVED <path>`,
//! `WBDEMO_LOADED <items>`, `WBDEMO_JUMP <file> <first> <last> <freshness>`, `WBDEMO_KEY <key> <items>`.

use std::path::PathBuf;

use azul::{
    callbacks::{RenderImageCallbackInfo, WriteBackCallbackType},
    dom::{DomNodeId, RenderImageCallback},
    image::{ImageRef, RawImage, RawImageFormat},
    prelude::*,
    task::TerminateTimer,
    time::SystemTimeDiff,
    vec::U8VecRef,
};
use azul_appkit::{
    args::AppArgs,
    l10n::{begin_layout},
    ui as kit,
};

use crate::{
    ink::{self, Layer},
    model::{InkPoint, Semantic, Stroke},
    ui::{LINE_H, PAGE_PAD},
    whiteboard::{lines_hash, ItemId},
    whiteboard_app::SourceFacts,
    whiteboard_archive,
    whiteboard_drag::{Motion, Pointer, Press, Release},
    whiteboard_geom::{Pt, Size},
    whiteboard_jump::{find_block_in, jump, FileView},
    whiteboard_lasso::PageLines,
    whiteboard_render::{self, RenderRequest},
    whiteboard_store,
    whiteboard_ui::{self, Host, Panel},
};

const PAGE_W: f32 = 1000.0;
const PAGE_H: f32 = 46.0 * LINE_H + 40.0;
/// The width of the bar on the right.
const BAR_W: f32 = 340.0;
/// The room over the page and the bar (none: the window is the demo's).
const TOP: f32 = 0.0;
/// The margin round the sheet: the sheet and its margins are 746 px high, which a laptop's 748 px window holds.
const SHEET_MARGIN: f32 = 8.0;
/// The font's advance of the page's monospace, px (what the lines' text boxes are measured with).
const CHAR_W: f32 = 6.6;

struct Demo {
    kit: RefAny,
    panel: Panel,
    file: String,
    lines: Vec<String>,
    strokes: Vec<Stroke>,
    live: Option<Stroke>,
    next_id: u64,
    /// The lines of the last jump and until when they flash (ms of the panel's clock).
    flash: Option<(u32, u32, u64)>,
    board_path: PathBuf,
    timer: Option<TimerId>,
    /// `WBDEMO_PROFILE`: the frame times are printed every two seconds.
    profile: bool,
    ticks: u32,
    /// The pictures being drawn on a Thread, and since when (the time a drop took to become a picture is told).
    render_started: Vec<(ItemId, std::time::Instant)>,
}

fn demo_text() -> (String, Vec<String>) {
    let text = include_str!("whiteboard_demo.rs");
    ("whiteboard_demo.rs".to_string(), text.lines().take(46).map(|l| l.replace('\t', "    ")).collect())
}

/// The one file of the demo, as the jump asks about files.
struct OneFile<'a> {
    name: &'a str,
    lines: &'a [String],
}

impl FileView for OneFile<'_> {
    fn file_hash(&self, file: &str) -> Option<u64> {
        (file == self.name).then(|| lines_hash(self.lines))
    }

    fn line_count(&self, file: &str) -> Option<u32> {
        (file == self.name).then(|| u32::try_from(self.lines.len()).unwrap_or(u32::MAX))
    }

    fn lines(&self, file: &str, first: u32, last: u32) -> Option<Vec<String>> {
        (file == self.name).then(|| self.lines.get((first as usize).saturating_sub(1)..(last as usize))).flatten().map(<[String]>::to_vec)
    }

    fn find_block(&self, file: &str, block: &[String], near: u32) -> Option<u32> {
        (file == self.name).then(|| find_block_in(self.lines, block, near)).flatten()
    }
}

fn with_panel(app: &mut RefAny, f: &mut dyn FnMut(&mut Panel)) -> bool {
    match app.downcast_mut::<Demo>() {
        Some(mut d) => {
            f(&mut d.panel);
            true
        }
        None => false,
    }
}

fn host(app: &RefAny) -> Host {
    Host { app: app.clone(), with: with_panel, on_jump, on_changed }
}

/// The bar as the window has it now (the window is `window` px; the bar is its right edge).
fn bar_of(window: Size) -> crate::whiteboard_drag::Bar {
    crate::whiteboard_drag::Bar {
        rect: crate::whiteboard_geom::Rect::new(window.w - BAR_W, TOP, BAR_W, window.h - TOP),
        open: true,
        window_w: window.w,
    }
}

fn say(line: &str) {
    println!("{line}");
}

// ---- the window

extern "C" fn layout(data: RefAny, info: LayoutCallbackInfo) -> Dom {
    begin_layout(&info);
    let mut d = data.clone();
    let Some(mut demo) = d.downcast_mut::<Demo>() else {
        return Dom::create_body();
    };
    let host = host(&data);
    let page = page_dom(&demo, &data);
    // The bar lies against the window's right edge and takes its whole height, whatever the window's width: that
    // is where `bar_of` (the drag's idea of the bar) looks for it. A window narrower than the page and the bar
    // (a laptop's screen) shows the page's left part; the body does not scroll.
    let bar = {
        let panel_dom = whiteboard_ui::panel_dom(&mut demo.panel, &host);
        Dom::create_div()
            .with_css(
                format!(
                    "position: absolute; right: 0px; top: {TOP}px; bottom: 0px; width: {BAR_W}px; display: flex; \
                     flex-direction: column;"
                )
                .as_str(),
            )
            .with_child(panel_dom)
    };
    Dom::create_body()
        .with_css("position: relative; width: 100%; height: 100%; overflow: hidden; background: #e9e5dc; margin: 0px;")
        .with_child(page)
        .with_child(bar)
        .with_callback(EventFilter::Window(WindowEventFilter::VirtualKeyDown), data.clone(), on_key)
}

/// The panel's keys (Delete, Escape, the two steps): what the review window routes to the panel when the
/// whiteboard has the keyboard.
extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(code) = info.get_current_keyboard_state().current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let mods = info.get_key_modifiers();
    let Some(key) = whiteboard_ui::key_of(code, mods.primary_down(), mods.shift) else {
        return Update::DoNothing;
    };
    // While a label is typed in its text area the keys are the text's (Delete, Backspace, Mod+Z).
    if data.downcast_ref::<Demo>().is_some_and(|d| d.panel.editing.is_some()) {
        return Update::DoNothing;
    }
    info.prevent_default();
    let acted = {
        let Some(mut d) = data.downcast_mut::<Demo>() else {
            return Update::DoNothing;
        };
        let acted = whiteboard_ui::on_key(&mut d.panel, key);
        if acted {
            say(&format!("WBDEMO_KEY {key:?} {}", d.panel.wb.board.len()));
        }
        acted
    };
    if !acted {
        return Update::DoNothing;
    }
    on_changed(&mut data, &mut info);
    redraw_surface(&mut info);
    Update::RefreshDom
}

/// A new frame of the bar's surface (the arrows are drawn on it, the cutouts are nodes of the new DOM).
fn redraw_surface(info: &mut CallbackInfo) {
    if let Some(node) = whiteboard_ui::surface_node(info) {
        kit::redraw_image(info, node);
    }
}

fn page_dom(demo: &Demo, app: &RefAny) -> Dom {
    let mut column = Dom::create_div().with_css(
        format!(
            "position: absolute; left: {PAGE_PAD}px; top: {PAGE_PAD}px; width: 708px; display: flex; flex-direction: column; \
             user-select: none;"
        )
        .as_str(),
    );
    for (i, line) in demo.lines.iter().enumerate() {
        column.add_child(whiteboard_render::code_row(i as u32 + 1, line));
    }
    let mut sheet = Dom::create_div().with_css(
        format!(
            "position: relative; width: {PAGE_W}px; height: {PAGE_H}px; background: #ffffff; flex-shrink: 0; margin: {SHEET_MARGIN}px; \
             overflow: hidden;"
        )
        .as_str(),
    );
    sheet.add_child(column);
    if let Some((first, last, _)) = demo.flash {
        sheet.add_child(
            Dom::create_div().with_css(
                format!(
                    "position: absolute; left: 0px; width: {PAGE_W}px; top: {}px; height: {}px; background: rgba(255,214,64,0.35);",
                    PAGE_PAD + (first as f32 - 1.0) * LINE_H,
                    (last - first + 1) as f32 * LINE_H
                )
                .as_str(),
            ),
        );
    }
    sheet.add_child(
        Dom::create_image(ImageRef::callback(RenderImageCallback::create(render_page_ink).to_core(), app.clone()))
            .with_id(azul::str::String::from_const_str("__azreview_demo-page"))
            .with_css(format!("position: absolute; left: 0px; top: 0px; width: {PAGE_W}px; height: {PAGE_H}px;").as_str())
            .with_callback(EventFilter::Hover(HoverEventFilter::MouseDown), app.clone(), on_page_down)
            .with_callback(EventFilter::Hover(HoverEventFilter::MouseMove), app.clone(), on_page_move)
            .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), app.clone(), on_page_up),
    );
    sheet
}

/// The page's ink and the armed regions, at the window's pixel scale.
extern "C" fn render_page_ink(mut data: RefAny, info: RenderImageCallbackInfo) -> ImageRef {
    let bounds = info.get_bounds();
    let (physical, logical) = (bounds.get_physical_size(), bounds.get_logical_size());
    let (w, h) = (physical.width.max(1), physical.height.max(1));
    let blank = || ImageRef::null_image(w as usize, h as usize, RawImageFormat::RGBA8, U8VecRef::from(&[][..]));
    let Some(mut demo) = data.downcast_mut::<Demo>() else {
        return blank();
    };
    let k = if logical.width > 0.0 { w as f32 / logical.width } else { 1.0 };
    demo.panel.dpr = k;
    let mut image = RawImage::create_rgba8(w, h, vec![0_u8; w as usize * h as usize * 4], true);
    for layer in [Layer::Marker, Layer::Pen] {
        for s in demo.strokes.iter().chain(demo.live.iter()).filter(|s| Layer::of(s.semantic) == layer) {
            whiteboard_render::paint_stroke(&mut image, k, Pt::new(0.0, 0.0), s);
        }
    }
    let now = demo.panel.now_ms();
    whiteboard_ui::paint_armed(&mut image, k, &demo.panel.wb.armed, 0, now);
    // A lifted region follows the pointer as its outline (over the bar the bar's frame takes over).
    if let Some(ghost) = demo.panel.wb.drag.ghost() {
        if let Some(region) = demo.panel.wb.armed.get(ghost.region) {
            whiteboard_ui::paint_ghost(&mut image, k, &region.polygon, ghost.offset());
        }
    }
    ImageRef::create_rawimage(image).into_option().unwrap_or_else(blank)
}

// ---- the page's pointer

fn pointer_of(info: &CallbackInfo) -> Option<(Pt, Pt)> {
    let local = info.get_cursor_relative_to_node().into_option()?;
    let window = info.get_cursor_relative_to_viewport().into_option()?;
    Some((Pt::new(local.x, local.y), Pt::new(window.x, window.y)))
}

fn window_size(info: &CallbackInfo) -> Size {
    let d = info.get_current_window_state().size.dimensions;
    Size::new(d.width, d.height)
}

fn redraw_page(info: &mut CallbackInfo) {
    let node = info.get_hit_node();
    kit::redraw_image(info, node);
}

fn chars_of(lines: &[String]) -> Vec<usize> {
    lines.iter().map(|l| l.chars().count()).collect()
}

extern "C" fn on_page_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((local, window)) = pointer_of(&info) else {
        return Update::DoNothing;
    };
    let Some(mut d) = data.downcast_mut::<Demo>() else {
        return Update::DoNothing;
    };
    let now = d.panel.now_ms();
    let press = d.panel.wb.page_press(Pointer { page: 0, in_page: local, in_window: window }, now);
    if !matches!(press, Press::Grip(_)) {
        let id = d.next_id;
        d.next_id += 1;
        let semantic = Semantic::Issue;
        d.live = Some(Stroke { page: 0, semantic, points: vec![ink::point_from(local.x, local.y, 0.0, 0.0, 0.0)], id, epoch: 0, started_unix_ms: 0 });
    }
    if !matches!(press, Press::Elsewhere) {
        let hit = info.get_hit_node();
        info.capture_pointer(hit);
    }
    drop(d);
    ensure_timer(&mut data, &mut info);
    redraw_page(&mut info);
    Update::DoNothing
}

extern "C" fn on_page_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((local, window)) = pointer_of(&info) else {
        return Update::DoNothing;
    };
    let size = window_size(&info);
    let Some(mut d) = data.downcast_mut::<Demo>() else {
        return Update::DoNothing;
    };
    let now = d.panel.now_ms();
    let bar = bar_of(size);
    let motion = d.panel.wb.page_motion(window, now, &bar);
    let mut update = Update::DoNothing;
    match motion {
        Motion::Held(_) => {
            // A held press was no stroke: the dab it began is dropped.
            d.live = None;
            say("WBDEMO_HELD");
        }
        Motion::Lifted(_) => {
            d.live = None;
            say("WBDEMO_LIFTED");
        }
        Motion::Dragging { over_bar, .. } => {
            // The bar's frame is a node: it comes and goes with a new DOM (twice in a drag, not at every move).
            if d.panel.drag_over != over_bar {
                update = Update::RefreshDom;
            }
            d.panel.drag_over = over_bar;
        }
        Motion::Inking | Motion::Nothing => {}
    }
    if d.panel.wb.drag.ghost().is_none() {
        if let Some(live) = d.live.as_mut() {
            live.points.push(ink::point_from(local.x, local.y, 0.0, 0.0, 0.0));
        }
    }
    drop(d);
    redraw_page(&mut info);
    update
}

extern "C" fn on_page_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((_, window)) = pointer_of(&info) else {
        return Update::DoNothing;
    };
    let size = window_size(&info);
    info.release_pointer_capture();
    let app = data.clone();
    let mut job = None;
    {
        let Some(mut d) = data.downcast_mut::<Demo>() else {
            return Update::DoNothing;
        };
        let now = d.panel.now_ms();
        let bar = bar_of(size);
        d.panel.drag_over = false;
        match d.panel.wb.page_release(window, &bar) {
            Release::Dropped { region, at_bar } => {
                say("WBDEMO_DROPPED");
                let facts = SourceFacts {
                    file: d.file.clone(),
                    file_hash: lines_hash(&d.lines),
                    covered: d.panel.wb.armed.get(region).map_or_else(Vec::new, |r| {
                        let (a, b) = (r.first_line as usize, r.last_line as usize);
                        d.lines.get(a.saturating_sub(1)..b.min(d.lines.len())).map(<[String]>::to_vec).unwrap_or_default()
                    }),
                };
                let dpr = d.panel.dpr;
                match d.panel.wb.drop_region(region, at_bar, facts, dpr, Size::new(PAGE_W, PAGE_H)) {
                    Ok(made) => job = Some(made),
                    Err(n) => d.panel.hint = Some(whiteboard_ui::say(&n)),
                }
            }
            Release::Cancelled | Release::Nothing => {
                if let Some(stroke) = d.live.take() {
                    if stroke.points.len() >= 3 {
                        let points: Vec<Pt> = stroke.points.iter().map(|p: &InkPoint| Pt::new(p.x, p.y)).collect();
                        let chars = chars_of(&d.lines);
                        let lines = PageLines { first_number: 1, chars: &chars, top: PAGE_PAD, line_h: LINE_H, text_x: PAGE_PAD + 52.0, char_w: CHAR_W, max_w: 600.0 };
                        let armed = d.panel.wb.arm_stroke(0, &points, &lines, Size::new(PAGE_W, PAGE_H), stroke.id, now);
                        if let Some(id) = armed {
                            let r = d.panel.wb.armed.get(id).map(|r| (r.first_line, r.last_line));
                            if let Some((a, b)) = r {
                                say(&format!("WBDEMO_ARMED {a} {b}"));
                            }
                        }
                        d.strokes.push(stroke);
                    }
                }
            }
        }
        d.live = None;
        if let Some(made) = &job {
            let first_line = 1;
            let request = RenderRequest {
                plan: made.plan.clone(),
                first_number: first_line,
                lines: d.lines.clone(),
                strokes: d.strokes.clone(),
                fonts: info.get_font_cache_clone(),
                images: info.get_image_cache_clone(),
            };
            let id = made.id;
            d.render_started.push((id, std::time::Instant::now()));
            drop(d);
            whiteboard_render::spawn_render(&mut info, &app, id, request, on_rendered as WriteBackCallbackType);
        }
    }
    redraw_page(&mut info);
    Update::RefreshDom
}

extern "C" fn on_rendered(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(done) = whiteboard_render::take_rendered(&mut msg) else {
        return Update::DoNothing;
    };
    let took = app
        .downcast_mut::<Demo>()
        .and_then(|mut d| {
            let at = d.render_started.iter().position(|(id, _)| *id == done.id)?;
            Some(d.render_started.remove(at).1.elapsed().as_millis())
        })
        .map_or_else(String::new, |ms| format!(" {ms}ms"));
    match &done.image {
        Ok(img) => say(&format!("WBDEMO_RENDERED {}x{}{took}", img.width, img.height)),
        Err(why) => say(&format!("WBDEMO_RENDERED {why}")),
    }
    if let Some(mut d) = app.downcast_mut::<Demo>() {
        d.panel.wb.image_ready(done.id, done.image);
    }
    on_changed(&mut app, &mut info);
    Update::RefreshDom
}

/// Keeps the board: its zip is made and written on a Thread ([`whiteboard_archive::spawn_save`]; the owner's gate lets one
/// write run at a time, and says when another is wanted after it).
fn on_changed(app: &mut RefAny, info: &mut CallbackInfo) {
    let (entries, path) = {
        let Some(mut d) = app.downcast_mut::<Demo>() else {
            return;
        };
        let now = d.panel.now_ms();
        let Some(job) = d.panel.wb.save_job(now) else {
            return;
        };
        (job.entries, d.board_path.clone())
    };
    whiteboard_archive::spawn_save(info, app, entries, path, on_saved as WriteBackCallbackType);
}

/// The Thread has written the file (or could not).
extern "C" fn on_saved(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(result) = whiteboard_archive::take_saved(&mut msg) else {
        return Update::DoNothing;
    };
    let again = {
        let Some(mut d) = app.downcast_mut::<Demo>() else {
            return Update::DoNothing;
        };
        match &result {
            Ok(()) => say(&format!("WBDEMO_SAVED {}", d.board_path.display())),
            Err(why) => say(&format!("WBDEMO_SAVE_ERROR {why}")),
        }
        d.panel.wb.save_done(&result)
    };
    if again {
        on_changed(&mut app, &mut info);
    }
    Update::DoNothing
}

fn on_jump(app: &mut RefAny, id: ItemId, _info: &mut CallbackInfo) -> Update {
    let Some(mut d) = app.downcast_mut::<Demo>() else {
        return Update::DoNothing;
    };
    let (name, lines) = (d.file.clone(), d.lines.clone());
    let files = OneFile { name: &name, lines: &lines };
    match jump(&mut d.panel.wb.board, id, &files) {
        Ok(target) => {
            say(&format!("WBDEMO_JUMP {} {} {} {:?}", target.file, target.first_line, target.last_line, target.freshness));
            let until = d.panel.now_ms() + 1_500;
            d.flash = Some((target.first_line, target.last_line, until));
            d.panel.tell_jump(&target);
        }
        Err(refused) => d.panel.hint = Some(whiteboard_ui::say(&refused)),
    }
    Update::RefreshDom
}

// ---- time

fn ensure_timer(data: &mut RefAny, info: &mut CallbackInfo) {
    let Some(mut d) = data.downcast_mut::<Demo>() else {
        return;
    };
    if d.timer.is_some() {
        return;
    }
    let id = TimerId::unique();
    d.timer = Some(id);
    drop(d);
    let timer = Timer::create(data.clone(), on_tick, info.get_system_time_fn())
        .with_interval(Duration::System(SystemTimeDiff::from_millis(100)));
    info.add_timer(id, timer);
}

extern "C" fn on_tick(mut data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
    let mut update = Update::DoNothing;
    if let Some(mut d) = data.downcast_mut::<Demo>() {
        let now = d.panel.now_ms();
        if d.panel.wb.page_tick(now).is_some() {
            d.live = None;
            say("WBDEMO_HELD");
            update = Update::RefreshDom;
        }
        if d.panel.wb.expire(now) {
            update = Update::RefreshDom;
        }
        d.ticks += 1;
        if d.profile && d.ticks % 20 == 0 {
            if let Some(line) = d.panel.perf.take_summary() {
                say(&format!("WBDEMO_PERF items={} {line}", d.panel.wb.board.len()));
            }
        }
        if d.flash.is_some_and(|(_, _, until)| now >= until) {
            d.flash = None;
            update = Update::RefreshDom;
        }
    }
    let _ = info;
    TimerCallbackReturn { should_update: update, should_terminate: TerminateTimer::Continue }
}

// ---- the start

extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let kit_ref = data.downcast_ref::<Demo>().map(|d| d.kit.clone());
    if let Some(kit_ref) = kit_ref {
        kit::on_window_created(&kit_ref, &mut info);
    }
    Update::DoNothing
}

/// Starts the demo window: `whiteboard_demo [--size WxH] [--data-dir DIR] [--language en|de] [--shot FILE]`.
pub fn run() {
    let args = match AppArgs::parse(&crate::SPEC, std::env::args().skip(1).collect::<Vec<String>>()) {
        Ok(a) => a,
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(2);
        }
    };
    let kit_ref = kit::create_kit(crate::SPEC, crate::ABOUT, &crate::SHORTCUTS, &[], args);
    let mut config = kit::app_config(&kit_ref);
    crate::l10n::register(&mut config);
    let data_root = {
        let mut k = kit_ref.clone();
        k.downcast_ref::<kit::Kit>().map(|k| k.data_root.clone()).unwrap_or_default()
    };
    // The environment is read once, here: `WBDEMO_PROFILE=1` prints the frame times, `WBDEMO_FILL=n` puts `n`
    // small cutouts and the arrows between them on a fresh board (a full board to time).
    let profile = std::env::var_os("WBDEMO_PROFILE").is_some();
    let fill = std::env::var("WBDEMO_FILL").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
    let (file, lines) = demo_text();
    let board_path = data_root.join("review").join(whiteboard_store::board_name("whiteboard-demo"));
    let mut panel = Panel::new("whiteboard-demo");
    panel.wb.set_bar(true, BAR_W, 1_280.0);
    panel.perf_on = profile;
    match std::fs::read(&board_path) {
        Ok(bytes) => {
            let loaded = whiteboard_archive::load(&bytes, &crate::whiteboard::LIMITS);
            let items = loaded.as_ref().map_or(0, |l| l.data.items.len());
            let effects = panel.wb.loaded(loaded, Some(bytes), 0);
            for e in effects {
                let _ = std::fs::write(board_path.with_file_name(e.name.rsplit('/').next().unwrap_or("corrupt")), e.bytes);
            }
            say(&format!("WBDEMO_LOADED {items}"));
        }
        Err(_) => {
            panel.wb.loaded_none();
        }
    }
    if fill > 0 {
        fill_board(&mut panel, fill);
        say(&format!("WBDEMO_FILLED {}", panel.wb.board.len()));
    }
    let state = Demo {
        kit: kit_ref.clone(),
        panel,
        file,
        lines,
        strokes: Vec::new(),
        live: None,
        next_id: 1,
        flash: None,
        board_path,
        timer: None,
        profile,
        ticks: 0,
        render_started: Vec::new(),
    };
    let app = App::create(RefAny::new(state), config);
    let window = kit::window_options(&kit_ref, layout, (1_400.0, 800.0), (900.0, 500.0), on_window_created);
    app.run(window);
}

/// `n` small cutouts in a grid, each of a flat colour with a few lines of text behind it, and an arrow from each
/// to the next: a full board to time.
fn fill_board(panel: &mut Panel, n: usize) {
    use std::sync::Arc;

    use crate::whiteboard::{CutoutDraft, CutoutImage, End, Source};
    let mut ids = Vec::new();
    for i in 0..n {
        let (w, h) = (160_u32, 100_u32);
        let tint = [180 + (i * 7 % 70) as u8, 190 + (i * 13 % 60) as u8, 200 + (i * 5 % 50) as u8];
        let rgba: Vec<u8> = (0..w * h).flat_map(|_| [tint[0], tint[1], tint[2], 255]).collect();
        let image = Arc::new(CutoutImage { width: w, height: h, scale: 2.0, rgba, png: Vec::new() });
        let source = Source {
            file: "whiteboard_demo.rs".to_string(),
            page: 0,
            first_line: 1,
            last_line: 2,
            shape: Vec::new(),
            file_hash: 0,
            range_hash: 0,
            lines: vec!["a piece of the board".to_string()],
        };
        let at = Pt::new((i % 12) as f32 * 100.0, (i / 12) as f32 * 70.0);
        if let Ok(id) = panel.wb.board.add_cutout(CutoutDraft::ready(at, image, source)) {
            ids.push(id);
        }
    }
    for w in ids.windows(2) {
        let _ = panel.wb.board.add_arrow(End::item(w[0]), End::item(w[1]), 0.1, "");
    }
}

/// The surface's id for scripts.
#[must_use]
pub fn surface_node(info: &CallbackInfo) -> Option<DomNodeId> {
    whiteboard_ui::surface_node(info)
}

#[cfg(test)]
#[path = "whiteboard_demo_test.rs"]
mod whiteboard_demo_test;
