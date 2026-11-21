//! Settings > Navigation > Offline areas: the area shown on the map kept for routes without
//! the network.
//!
//! The user frames the area on the map, names it and asks to keep it. A worker reads the
//! hosting's tile list (which routing tiles the area has and how big they are), downloads
//! them into the routing tile cache in the data folder and pins them there
//! (`azul_routing::offline`), so pruning the cache never deletes them. The section lists the
//! kept areas with their size and a Delete button, which lets their tiles go again. Only
//! downloaded routing tiles have offline areas: a folder of tiles is on this computer already.
//! The map's own tiles are not part of an area; the map keeps on disk the tiles it has shown.
//!
//! Lines for scripts: `AZMAPS_OFFLINE_AREAS <n> <names>` at the start, `AZMAPS_OFFLINE_PLAN
//! <tiles> <bytes>`, `AZMAPS_OFFLINE_AREA <tiles> <bytes> <failed> <name>` once kept,
//! `AZMAPS_OFFLINE_FAILED <why>`, `AZMAPS_OFFLINE_DELETED <name>`.

use std::{
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use azul::{
    callbacks::{ButtonOnClickCallbackType, TextInputOnTextInputCallbackType},
    prelude::*,
    str::String as AzString,
    task::{Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg},
    widgets::{Button, ButtonType, MapViewport, MapWidget, OnTextInputReturn, TextInputState, TextInputValid},
};
use azul_appkit::{
    l10n::{label, t_args, Arg},
    ui as kit,
};
use azul_routing::offline::{self, Area, DownloadStep, OfflineArea};

use crate::{
    ids,
    route::{RoutingTiles, Stop, Tiles},
    MapState,
};

const COLUMN: &str = "display: flex; flex-direction: column; gap: 6px;";
const ROW: &str = "display: flex; flex-direction: row; align-items: center; gap: 6px;";
const AREA_ROW: &str = "display: flex; flex-direction: row; align-items: center; gap: 6px; padding: 4px 6px; \
                        border-radius: 6px; background: system:control-background;";
const GROW: &str = "flex-grow: 1; min-width: 0px;";
const QUIET: &str = "font-size: 12px; color: system:secondary-text; white-space: normal;";

/// How many tiles a download fetches at once.
const DOWNLOAD_THREADS: usize = 4;

/// How often the worker tells the section how far it got, at most.
const PROGRESS_EVERY: Duration = Duration::from_millis(150);

/// The offline areas' state.
pub(crate) struct OfflineState {
    /// The name field's text: the name the next area is kept under.
    pub name: String,
    /// The download under way, how far it got; `None` while none runs.
    pub running: Option<DownloadStep>,
    /// What the last download came to, under the button; empty for nothing.
    pub note: String,
    /// The kept areas, by name: read at the start, then kept in step by the downloads and the
    /// deletes.
    pub areas: Vec<OfflineArea>,
    /// The newest download's number: Stop and a new download let the one before go.
    pub latest: Arc<AtomicU64>,
}

impl OfflineState {
    /// The areas kept under the routing tiles' cache folder, read once at the start.
    pub(crate) fn new(routing: Option<&Tiles>) -> Self {
        let areas = routing
            .and_then(|tiles| tiles.cache_dir().map(offline::areas))
            .unwrap_or_default();
        if routing.is_some_and(|tiles| tiles.cache_dir().is_some()) {
            let names: Vec<&str> = areas.iter().map(|a| a.name.as_str()).collect();
            let names = if names.is_empty() { "-".to_string() } else { names.join(",") };
            println!("AZMAPS_OFFLINE_AREAS {} {names}", areas.len());
        }
        Self {
            name: String::new(),
            running: None,
            note: String::new(),
            areas,
            latest: Arc::new(AtomicU64::new(0)),
        }
    }
}

/// The rectangle the map shows in a view of `size` px at `viewport`: the box around the
/// view's four corners on the ground, so a turned or tilted map keeps all it shows. A view
/// across the antimeridian keeps the side its centre is on.
pub(crate) fn shown_area(viewport: MapViewport, size: (f32, f32)) -> Area {
    let (w, h) = (size.0.max(1.0), size.1.max(1.0));
    let container = LogicalSize { width: w, height: h };
    let mut area = Area { south: 90.0, west: 180.0, north: -90.0, east: -180.0 };
    for (x, y) in [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)] {
        let corner = MapWidget::latlon_at_px(viewport, LogicalPosition { x, y }, container);
        // The corner's longitude the short way from the centre, then back onto the world.
        let lon = viewport.centre_lon_deg + (corner.lon_deg - viewport.centre_lon_deg + 540.0).rem_euclid(360.0) - 180.0;
        let lon = lon.clamp(-180.0, 180.0);
        area.south = area.south.min(corner.lat_deg);
        area.north = area.north.max(corner.lat_deg);
        area.west = area.west.min(lon);
        area.east = area.east.max(lon);
    }
    area
}

/// A size as the list says it: megabytes with one decimal.
fn size_text(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

/// Settings > Navigation > Offline areas.
pub(crate) fn section(s: &MapState, app: &RefAny) -> kit::AppSection {
    let mut content = Dom::create_div().with_css(COLUMN);
    if s.routing.as_ref().and_then(|tiles| tiles.cache_dir()).is_none() {
        content.add_child(kit::note("azmaps-offline-needs-tiles"));
        return kit::AppSection {
            category: 0,
            title: t_args("azmaps-offline-title", &[]),
            content,
        };
    }
    for (index, area) in s.offline_areas.areas.iter().enumerate() {
        content.add_child(area_row(area, index, app));
    }
    let action = if s.offline_areas.running.is_some() {
        Button::create(label("azmaps-offline-stop"))
            .with_on_click(app.clone(), on_stop as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::OFFLINE_STOP)
    } else {
        Button::with_type(label("azmaps-offline-download"), ButtonType::Primary)
            .with_icon("download")
            .with_on_click(app.clone(), on_download as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::OFFLINE_DOWNLOAD)
    };
    content.add_child(
        Dom::create_div()
            .with_css(ROW)
            .with_child(
                Dom::create_div().with_css(GROW).with_child(
                    TextInput::create()
                        .with_text(s.offline_areas.name.as_str())
                        .with_placeholder(label("azmaps-offline-name"))
                        .with_accessibility_name(label("azmaps-offline-name"))
                        .with_on_text_input(app.clone(), on_name as TextInputOnTextInputCallbackType)
                        .dom()
                        .with_id(ids::OFFLINE_NAME),
                ),
            )
            .with_child(action),
    );
    let line = match s.offline_areas.running {
        Some(step) if step.total == 0 => t_args("azmaps-offline-planning", &[]),
        Some(step) => t_args(
            "azmaps-offline-progress",
            &[
                ("done", Arg::from(step.done)),
                ("total", Arg::from(step.total)),
                ("size", Arg::from(size_text(step.bytes))),
            ],
        ),
        None => s.offline_areas.note.clone(),
    };
    if !line.is_empty() {
        content.add_child(
            Dom::create_div()
                .with_css(QUIET)
                .with_id(ids::OFFLINE_NOTE)
                .with_child(Dom::create_span_with_text(line)),
        );
    }
    content.add_child(kit::note("azmaps-offline-about"));
    kit::AppSection {
        category: 0,
        title: t_args("azmaps-offline-title", &[]),
        content,
    }
}

/// What a row's Delete button carries: the app and the area's place in the list.
struct AreaRef {
    app: RefAny,
    index: usize,
}

/// One kept area: its name, its road tiles and their size, Delete.
fn area_row(area: &OfflineArea, index: usize, app: &RefAny) -> Dom {
    let text = t_args(
        "azmaps-offline-row",
        &[
            ("name", Arg::from(area.name.clone())),
            ("tiles", Arg::from(area.tiles.len())),
            ("size", Arg::from(size_text(area.bytes))),
        ],
    );
    Dom::create_div()
        .with_css(AREA_ROW)
        .with_id(ids::indexed("offline-area", index))
        .with_child(Dom::create_div().with_css(GROW).with_child(Dom::create_span_with_text(text)))
        .with_child(
            Button::create(label("azmaps-offline-delete"))
                .with_icon("delete")
                .with_on_click(
                    RefAny::new(AreaRef {
                        app: app.clone(),
                        index,
                    }),
                    on_delete as ButtonOnClickCallbackType,
                )
                .dom()
                .with_id(ids::indexed("offline-delete", index)),
        )
}

extern "C" fn on_name(mut app: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    if let Some(mut s) = app.downcast_mut::<MapState>() {
        s.offline_areas.name = state.get_text().as_str().to_string();
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// What the offline worker is asked: an area to keep.
struct OfflineRequest {
    /// The download's number; the newest one's is in `latest`.
    id: u64,
    /// The name it is kept under.
    name: String,
    /// The rectangle.
    area: Area,
    /// The routing tiles, whose cache folder keeps the area.
    tiles: Tiles,
    /// The newest download's number, shared with the app.
    latest: Arc<AtomicU64>,
}

/// What the worker tells the section.
#[derive(Debug, Clone)]
enum OfflineKind {
    /// The plan: this many tiles of this many bytes.
    Plan { tiles: usize, bytes: u64 },
    /// How far the download got.
    Progress(DownloadStep),
    /// The routing tiles have no roads in the area.
    Empty,
    /// The area is kept (with the tiles that could not be had), and every area kept now; or why
    /// it could not be.
    Done(Result<(OfflineArea, usize, Vec<OfflineArea>), String>),
}

/// One message of the worker, for download `id`.
#[derive(Debug, Clone)]
struct OfflineMsg {
    id: u64,
    kind: OfflineKind,
}

extern "C" fn on_download(mut app: RefAny, mut info: CallbackInfo) -> Update {
    let request = {
        let Some(mut s) = app.downcast_mut::<MapState>() else {
            return Update::DoNothing;
        };
        let name = s.offline_areas.name.trim().to_string();
        if name.is_empty() {
            s.offline_areas.note = t_args("azmaps-offline-no-name", &[]);
            return Update::RefreshDom;
        }
        let Some(tiles) = s.routing.clone() else {
            return Update::DoNothing;
        };
        let area = shown_area(s.viewport, s.size.get());
        let id = s.offline_areas.latest.fetch_add(1, Ordering::AcqRel) + 1;
        s.offline_areas.running = Some(DownloadStep { done: 0, total: 0, bytes: 0, failed: 0 });
        s.offline_areas.note.clear();
        println!(
            "AZMAPS_OFFLINE_ASK {:.4},{:.4},{:.4},{:.4} {name}",
            area.south, area.west, area.north, area.east
        );
        OfflineRequest {
            id,
            name,
            area,
            tiles,
            latest: Arc::clone(&s.offline_areas.latest),
        }
    };
    info.add_thread(
        ThreadId::unique(),
        Thread::create(RefAny::new(request), app.clone(), offline_worker),
    );
    Update::RefreshDom
}

extern "C" fn on_stop(mut app: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = app.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    if s.offline_areas.running.take().is_none() {
        return Update::DoNothing;
    }
    // A newer number lets the worker go; it keeps nothing.
    s.offline_areas.latest.fetch_add(1, Ordering::AcqRel);
    s.offline_areas.note = t_args("azmaps-offline-stopped", &[]);
    println!("AZMAPS_OFFLINE_STOPPED");
    Update::RefreshDom
}

extern "C" fn on_delete(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<AreaRef>().map(|r| (r.app.clone(), r.index)) else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    let Some(cache) = s.routing.as_ref().and_then(|tiles| tiles.cache_dir().map(Path::to_path_buf)) else {
        return Update::DoNothing;
    };
    if index >= s.offline_areas.areas.len() {
        return Update::DoNothing;
    }
    let name = s.offline_areas.areas[index].name.clone();
    // One small file: its tiles may be pruned again from now on.
    match offline::unpin(&cache, &name) {
        Ok(()) => {
            s.offline_areas.areas.remove(index);
            s.offline_areas.note.clear();
            println!("AZMAPS_OFFLINE_DELETED {name}");
        }
        Err(why) => {
            s.offline_areas.note = t_args("azmaps-offline-failed", &[("why", Arg::from(why))]);
        }
    }
    Update::RefreshDom
}

/// The offline worker, an azul `Thread`: plans the area from the hosting's tile list,
/// downloads its tiles into the cache, pins it, and tells the section on the way - unless a
/// Stop or a newer download let it go first.
extern "C" fn offline_worker(mut init: RefAny, mut sender: ThreadSender, mut receiver: ThreadReceiver) {
    let Some((id, name, area, tiles, latest)) = init
        .downcast_ref::<OfflineRequest>()
        .map(|r| (r.id, r.name.clone(), r.area, r.tiles.clone(), r.latest.clone()))
    else {
        return;
    };
    let send = |sender: &mut ThreadSender, kind: OfflineKind| {
        let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
            on_offline_msg,
            RefAny::new(OfflineMsg { id, kind }),
        )));
    };
    let mut stop = Stop::new(id, &latest, &mut receiver);
    if stop.now() {
        return;
    }
    let outcome = keep_area(&tiles, name, area, &mut |kind| send(&mut sender, kind), &mut || stop.now());
    if let Some(kind) = outcome {
        send(&mut sender, kind);
    }
}

/// Plans, downloads and pins `area` as `name`, reporting the plan and the progress; what the
/// section is told last, or `None` when `stop` said to give up.
fn keep_area(
    tiles: &RoutingTiles,
    name: String,
    area: Area,
    report: &mut dyn FnMut(OfflineKind),
    stop: &mut dyn FnMut() -> bool,
) -> Option<OfflineKind> {
    let index = match tiles.index() {
        Ok(index) => index,
        Err(why) => return Some(OfflineKind::Done(Err(why))),
    };
    let (wanted, bytes) = offline::plan(area, &index);
    if wanted.is_empty() {
        return Some(OfflineKind::Empty);
    }
    let Some(cache) = tiles.cache_dir().map(Path::to_path_buf) else {
        return Some(OfflineKind::Done(Err("the routing tiles keep no downloads".to_string())));
    };
    report(OfflineKind::Plan {
        tiles: wanted.len(),
        bytes,
    });
    let mut last: Option<Instant> = None;
    let mut stopped = false;
    let result = offline::download(tiles, &wanted, DOWNLOAD_THREADS, &mut |step| {
        if stop() {
            stopped = true;
            return false;
        }
        if last.map_or(true, |at| at.elapsed() >= PROGRESS_EVERY) || step.done == step.total {
            last = Some(Instant::now());
            report(OfflineKind::Progress(step));
        }
        true
    });
    if stopped {
        return None;
    }
    let done = match result {
        Ok(done) => done,
        Err(why) => return Some(OfflineKind::Done(Err(why))),
    };
    let kept = OfflineArea {
        name,
        area,
        tiles: wanted,
        bytes,
    };
    Some(OfflineKind::Done(
        offline::pin(&cache, &kept).map(|_| (kept, done.failed, offline::areas(&cache))),
    ))
}

/// The worker's news, on the UI thread: the section shows it.
extern "C" fn on_offline_msg(mut app: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(msg) = msg.downcast_ref::<OfflineMsg>().map(|m| m.clone()) else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    if s.offline_areas.latest.load(Ordering::Acquire) != msg.id {
        return Update::DoNothing;
    }
    match msg.kind {
        OfflineKind::Plan { tiles, bytes } => {
            println!("AZMAPS_OFFLINE_PLAN {tiles} {bytes}");
            s.offline_areas.running = Some(DownloadStep { done: 0, total: tiles, bytes: 0, failed: 0 });
        }
        OfflineKind::Progress(step) => s.offline_areas.running = Some(step),
        OfflineKind::Empty => {
            println!("AZMAPS_OFFLINE_FAILED empty");
            s.offline_areas.running = None;
            s.offline_areas.note = t_args("azmaps-offline-empty", &[]);
        }
        OfflineKind::Done(Ok((area, failed, areas))) => {
            println!("AZMAPS_OFFLINE_AREA {} {} {failed} {}", area.tiles.len(), area.bytes, area.name);
            let args = [
                ("name", Arg::from(area.name.clone())),
                ("tiles", Arg::from(area.tiles.len())),
                ("size", Arg::from(size_text(area.bytes))),
                ("failed", Arg::from(failed)),
            ];
            s.offline_areas.note = if failed == 0 {
                t_args("azmaps-offline-kept", &args)
            } else {
                t_args("azmaps-offline-kept-failed", &args)
            };
            s.offline_areas.running = None;
            s.offline_areas.name.clear();
            s.offline_areas.areas = areas;
        }
        OfflineKind::Done(Err(why)) => {
            println!("AZMAPS_OFFLINE_FAILED {why}");
            s.offline_areas.running = None;
            s.offline_areas.note = t_args("azmaps-offline-failed", &[("why", Arg::from(why))]);
        }
    }
    Update::RefreshDom
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vienna(zoom: f32, bearing_deg: f32) -> MapViewport {
        MapViewport { centre_lat_deg: 48.2082, centre_lon_deg: 16.3738, zoom, bearing_deg, pitch_deg: 0.0 }
    }

    #[test]
    fn the_area_shown_is_the_rectangle_around_the_maps_corners() {
        let area = shown_area(vienna(12.0, 0.0), (1100.0, 720.0));
        assert!(area.south < 48.2082 && 48.2082 < area.north, "{area:?} holds the centre");
        assert!(area.west < 16.3738 && 16.3738 < area.east, "{area:?} holds the centre");
        // 1100 px at zoom 12 are 1100 / (256 * 4096) of the world's 360 degrees.
        let wide = 1100.0 / (256.0 * 4096.0) * 360.0;
        assert!(((area.east - area.west) - wide).abs() < wide * 0.01, "{area:?} is {wide} degrees wide");
    }

    #[test]
    fn a_turned_map_shows_more_than_its_unturned_rectangle() {
        let flat = shown_area(vienna(12.0, 0.0), (1100.0, 720.0));
        let turned = shown_area(vienna(12.0, 45.0), (1100.0, 720.0));
        assert!(turned.north - turned.south > flat.north - flat.south, "{turned:?} against {flat:?}");
    }

    #[test]
    fn a_size_is_said_in_megabytes() {
        assert_eq!(size_text(244_212), "0.2 MB");
        assert_eq!(size_text(12_500_000), "12.5 MB");
    }
}
