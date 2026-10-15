//! Navigation on screen: the Start button under a road route, then - while navigating - a
//! large next-maneuver banner over the map (the turn's arrow, the distance counting down, the
//! instruction), a bar with the arrival time, the time and the distance left, and the
//! switches (heading-up or north-up, the voice, the big-type car layout), night colours after
//! dark, the screen kept awake, the prompts said aloud (`voice.rs`), a new route asked for
//! when the car leaves its route (`navigate.rs` spaces the requests), and arrival.
//!
//! The position comes from the source of Settings > Location (`location.rs`): the platform's,
//! gpsd, an NMEA device or a replay file - or from a simulated drive (`--simulate-route`).
//! The app's timer ([`tick`]) feeds it about once a second. The map follows the car in place
//! (`MapWidget::update_camera` / `update_overlays`) and the banner's and the bar's texts are
//! rewritten in place; the window is rebuilt only when what they look like changes (night, the
//! car layout, the distance line, the offline tag, the fix indicator, arrival).

use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, DropDownOnChoiceChangeCallbackType, SegmentedOnChangeCallbackType,
        TextInputOnTextInputCallbackType,
    },
    option::OptionString,
    prelude::*,
    str::String as AzString,
    task::{Thread, ThreadId},
    vec::{MapLatLonVec, MapOverlayVec, StringVec},
    widgets::{
        Button, ButtonType, DropDown, MapMarker, MapOverlay, MapWidget, OnTextInputReturn, Segmented,
        SegmentedState, TextInputState, TextInputValid,
    },
};
use azul_appkit::{
    l10n::{label, labels, t_args, Arg},
    ui as kit,
};
use azul_routing::{
    gnss::{self, GnssFix, Source},
    navigation::Fix,
    online::Maneuver,
};
use chrono::Timelike;

use crate::{
    at, ids,
    journeys::{clock, span_text},
    location::{
        fix_of_gnss, fix_of_platform, grade_colour, location_worker, platform_grade, platform_quality_text,
        quality_text, ExternalFix, LocationRequest, SourceEnded, SOURCE_KEY,
    },
    model::{distance_text, TravelMode},
    navigate::{banner_distance, is_night, Event, Navigation},
    route::{route_worker, RouteKind, RouteReply, RouteRequest},
    voice, MapState,
};

/// How often the navigation shows where the car is.
const SHOW_EVERY: Duration = Duration::from_millis(950);

/// How old an external fix may be and still be the position, seconds.
const FRESH_S: u64 = 5;

/// The marker id of the car.
const CAR_MARKER: u64 = u64::MAX - 10;

/// The banner and the bar, by day and by night; the car layout's type is bigger.
const BANNER: &str = "position: absolute; left: 50%; top: 48px; width: 460px; margin-left: -230px; \
                      display: flex; flex-direction: row; align-items: center; gap: 14px; \
                      padding: 12px 16px; border-radius: 12px; background: #0b6e4f; color: #ffffff; \
                      box-shadow: 0px 2px 10px rgba(0,0,0,0.35);";
const BANNER_NIGHT: &str = "background: #1d2226; color: #e8eaed;";
const BANNER_ICON: &str = "font-size: 44px;";
const BANNER_ICON_CAR: &str = "font-size: 64px;";
const BANNER_DISTANCE: &str = "font-size: 26px; font-weight: bold;";
const BANNER_DISTANCE_CAR: &str = "font-size: 38px; font-weight: bold;";
const BANNER_TEXT: &str = "font-size: 15px; white-space: normal;";
const BANNER_TEXT_CAR: &str = "font-size: 22px; white-space: normal;";
const BAR: &str = "position: absolute; left: 50%; bottom: 26px; width: 560px; margin-left: -280px; \
                   display: flex; flex-direction: row; align-items: center; gap: 8px; padding: 8px 12px; \
                   border-radius: 12px; background: system:window-background; color: system:text; \
                   box-shadow: 0px 2px 10px rgba(0,0,0,0.3);";
const BAR_NIGHT: &str = "background: #202124; color: #e8eaed;";
const BAR_TEXT: &str = "flex-grow: 1; font-size: 15px; font-weight: bold;";
const BAR_TEXT_CAR: &str = "flex-grow: 1; font-size: 22px; font-weight: bold;";
const OFFLINE_TAG: &str = "font-size: 11px; padding: 2px 6px; border-radius: 4px; background: #9e9e9e; color: #ffffff;";
const INDICATOR: &str = "display: flex; flex-direction: row; align-items: center; gap: 6px; font-size: 11px; \
                         padding: 3px 8px; border-radius: 10px; background: system:window-background; \
                         color: system:secondary-text; box-shadow: 0px 1px 3px rgba(0,0,0,0.25);";
const INDICATOR_SPOT: &str = "position: absolute; left: 12px; bottom: 26px;";
const DOT: &str = "width: 8px; height: 8px; border-radius: 4px;";
const START_ROW: &str = "display: flex; flex-direction: row; align-items: center; gap: 6px;";
const CREDIT: &str = "font-size: 11px; color: system:secondary-text; white-space: normal;";
const SETTINGS_COLUMN: &str = "display: flex; flex-direction: column;";

/// The navigation and the position source.
pub(crate) struct DriveState {
    /// The navigation under way.
    pub nav: Option<Navigation>,
    /// A simulated car drives the next navigation, at this speed (`--simulate-route`).
    pub simulate_mps: Option<f64>,
    /// Navigation starts as soon as a road route is there (`--navigate`).
    pub navigate_on_route: bool,
    /// Where the position comes from.
    pub source: Source,
    /// How much faster a replay runs.
    pub replay_speedup: f64,
    /// The external source's last fix and when it came.
    pub external: Option<(GnssFix, Instant)>,
    /// Why the external source ended, shown by the indicator (empty while it runs).
    pub source_note: String,
    /// The newest location reader's number: a change of source lets the old one go.
    pub location_latest: Arc<AtomicU64>,
    /// The newest request for a new route.
    pub reroute_latest: Arc<AtomicU64>,
    /// When the navigation last showed where the car is.
    pub last_shown: Option<Instant>,
    /// What the banner and the bar of the last window build show; `None` without them. A tick
    /// whose moment looks the same rewrites only the texts that differ. Written by
    /// [`banner`] during the build, hence the cell.
    pub banner_shown: RefCell<Option<Shown>>,
    /// Whether the sidebar was open when the navigation started (it is closed meanwhile).
    pub sidebar_was_open: bool,
    /// The settings' text field for a device or a replay file.
    pub source_text: String,
}

impl DriveState {
    pub(crate) fn new(source: Source, simulate_mps: Option<f64>, navigate_on_route: bool, replay_speedup: f64) -> Self {
        let source_text = match &source {
            Source::Nmea(path) | Source::Replay(path) => path.clone(),
            Source::Gpsd(address) => address.clone(),
            Source::Platform => String::new(),
        };
        Self {
            nav: None,
            simulate_mps,
            navigate_on_route,
            source,
            replay_speedup,
            external: None,
            source_note: String::new(),
            location_latest: Arc::new(AtomicU64::new(0)),
            reroute_latest: Arc::new(AtomicU64::new(0)),
            last_shown: None,
            banner_shown: RefCell::new(None),
            sidebar_was_open: false,
            source_text,
        }
    }
}

// ==== The position source ====

/// Starts the reader of the position source (a new one lets the old one go); the platform's
/// needs none.
pub(crate) fn start_source(app: &RefAny, info: &mut CallbackInfo) {
    let mut state = app.clone();
    let request = {
        let Some(mut s) = state.downcast_mut::<MapState>() else {
            return;
        };
        let id = s.drive.location_latest.fetch_add(1, Ordering::AcqRel) + 1;
        s.drive.external = None;
        s.drive.source_note.clear();
        if s.drive.source == Source::Platform {
            return;
        }
        println!("AZMAPS_GPS_SOURCE {}", s.drive.source.setting());
        LocationRequest {
            id,
            source: s.drive.source.clone(),
            speedup: s.drive.replay_speedup,
            latest: Arc::clone(&s.drive.location_latest),
            on_fix: on_external_fix,
            on_ended: on_source_ended,
        }
    };
    info.add_thread(
        ThreadId::unique(),
        Thread::create(RefAny::new(request), app.clone(), location_worker),
    );
}

/// A fix of the external source: kept as the position (where you are, the start of a route);
/// the map follows it while the location is shown.
extern "C" fn on_external_fix(mut app: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(fix) = msg.downcast_ref::<ExternalFix>().map(|f| *f) else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    if s.drive.location_latest.load(Ordering::Acquire) != fix.id {
        return Update::DoNothing;
    }
    s.drive.external = Some((fix.fix, Instant::now()));
    s.last_fix = Some(fix.fix.at);
    println!("AZMAPS_GPS {:.5},{:.5} {}", fix.fix.at.0, fix.fix.at.1, quality_text(&fix.fix));
    // While navigating the timer shows it; otherwise only where the location is shown.
    if s.drive.nav.is_none() && s.locating {
        s.centre_on(fix.fix.at.0, fix.fix.at.1);
        return Update::RefreshDom;
    }
    Update::DoNothing
}

extern "C" fn on_source_ended(mut app: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(ended) = msg.downcast_ref::<SourceEnded>().map(|e| e.clone()) else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    if s.drive.location_latest.load(Ordering::Acquire) != ended.id {
        return Update::DoNothing;
    }
    println!("AZMAPS_GPS_ENDED {}", if ended.why.is_empty() { "-" } else { ended.why.as_str() });
    s.drive.source_note = if ended.why.is_empty() {
        t_args("azmaps-gps-ended", &[])
    } else {
        t_args("azmaps-gps-failed", &[("why", Arg::from(ended.why))])
    };
    Update::RefreshDom
}

/// The fix-quality indicator: a dot of the grade's colour and the fix's line; while an
/// external source is chosen or a navigation runs.
pub(crate) fn indicator(s: &MapState, platform_accuracy: Option<f32>) -> Option<Dom> {
    let (colour, text) = indicator_line(s, platform_accuracy)?;
    Some(indicator_dom(colour, Dom::create_span_with_text(text)))
}

/// The indicator of a dot in `colour` and the line `text`.
fn indicator_dom((r, g, b): (u8, u8, u8), text: Dom) -> Dom {
    Dom::create_div()
        .with_css(INDICATOR)
        .with_id(ids::GPS_INDICATOR)
        .with_child(Dom::create_div().with_css(format!("{DOT} background: rgb({r}, {g}, {b});").as_str()))
        .with_child(text)
}

/// The indicator's colour and line, when it shows.
fn indicator_line(s: &MapState, platform_accuracy: Option<f32>) -> Option<((u8, u8, u8), String)> {
    let external = s.drive.source != Source::Platform;
    if !external && s.drive.nav.is_none() {
        return None;
    }
    let line = if s.drive.nav.as_ref().is_some_and(|n| n.drive.is_some()) {
        (grade_colour(gnss::Grade::Good), t_args("azmaps-gps-simulated", &[]))
    } else if external {
        match &s.drive.external {
            Some((fix, at)) if at.elapsed().as_secs() <= FRESH_S => (grade_colour(fix.grade()), quality_text(fix)),
            _ if !s.drive.source_note.is_empty() => (grade_colour(gnss::Grade::None), s.drive.source_note.clone()),
            _ => (grade_colour(gnss::Grade::None), t_args("azmaps-gps-waiting", &[])),
        }
    } else {
        let accuracy = platform_accuracy.unwrap_or(f32::NAN);
        (grade_colour(platform_grade(accuracy)), platform_quality_text(accuracy))
    };
    Some(line)
}

/// The indicator where it floats on the map, bottom left.
pub(crate) fn indicator_on_map(s: &MapState, platform_accuracy: Option<f32>) -> Option<Dom> {
    indicator(s, platform_accuracy).map(|d| Dom::create_div().with_css(INDICATOR_SPOT).with_child(d))
}

// ==== Starting and ending ====

/// The road route of the travel panel as it is, when there is one to navigate.
fn road_route(s: &MapState) -> Option<&RouteReply> {
    let (Some(from), Some(to)) = s.travel_ends() else {
        return None;
    };
    s.route
        .as_ref()
        .filter(|r| r.from == from && r.to == to && r.mode == s.travel.mode)
        .filter(|r| r.kind == RouteKind::Road && r.road.len() >= 2 && r.mode != TravelMode::Transit)
}

/// The Start button under a road route, and the routing server's credit.
pub(crate) fn start_row(s: &MapState, app: &RefAny) -> Option<Dom> {
    if s.drive.nav.is_some() {
        return None;
    }
    let reply = road_route(s)?;
    let mut row = Dom::create_div().with_css(START_ROW).with_child(
        Button::with_type(label("azmaps-nav-start"), ButtonType::Primary)
            .with_icon("navigation")
            .with_on_click(app.clone(), on_start as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::NAV_START),
    );
    // The routing data's credit next to the route: the server's, or the routing tiles' own.
    let credit = match reply.source {
        crate::route::RouteSource::Online => Some(label("azmaps-routing-credit")),
        crate::route::RouteSource::Tiles if !reply.credit.is_empty() => {
            Some(AzString::from(reply.credit.clone()))
        }
        _ => None,
    };
    if let Some(credit) = credit {
        row.add_child(
            Dom::create_div()
                .with_css(CREDIT)
                .with_id(ids::ROUTE_CREDIT)
                .with_child(Dom::create_span_with_text(credit)),
        );
    }
    Some(row)
}

/// The choices of Settings > Navigation > Routing tiles after "as the tile set suggests":
/// days a downloaded road tile is kept before it is fetched again.
const KEEP_DAYS: [u32; 4] = [7, 14, 28, 56];

/// The routing tiles' keep setting as settings.json holds it (`None`: as the tile set suggests).
fn keep_days_setting(s: &MapState) -> Option<String> {
    let mut kit_ref = s.kit.clone();
    let k = kit_ref.downcast_ref::<kit::Kit>()?;
    k.settings
        .get(azul_routing::source::SETTINGS_KEY)
        .map(str::to_string)
}

/// Settings > Navigation > Routing tiles: how long downloaded roads are kept. Applies from the
/// next start, when the tile cache is made.
pub(crate) fn routing_tiles_section(s: &MapState, app: &RefAny) -> kit::AppSection {
    let mut names: Vec<AzString> = vec![label("azmaps-routing-keep-suggested")];
    names.extend(
        KEEP_DAYS
            .iter()
            .map(|days| AzString::from(t_args("azmaps-routing-keep-days", &[("days", Arg::from(i64::from(*days)))]))),
    );
    let selected = keep_days_setting(s)
        .and_then(|text| text.trim().parse::<u32>().ok())
        .and_then(|days| KEEP_DAYS.iter().position(|d| *d == days))
        .map_or(0, |i| i + 1);
    let content = Dom::create_div()
        .with_css(SETTINGS_COLUMN)
        .with_child(kit::row(
            "azmaps-routing-keep",
            DropDown::create(StringVec::from(names))
                .with_selected(selected)
                .with_on_choice_change(app.clone(), on_keep_days as DropDownOnChoiceChangeCallbackType)
                .dom()
                .with_id(ids::ROUTING_KEEP),
        ))
        .with_child(kit::note("azmaps-routing-keep-about"));
    kit::AppSection {
        category: 0,
        title: t_args("azmaps-routing-tiles-title", &[]),
        content,
    }
}

/// Keeps the chosen keep time in settings.json; the first choice clears it.
extern "C" fn on_keep_days(app: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let mut state = app;
    let Some(kit_ref) = state.downcast_ref::<MapState>().map(|s| s.kit.clone()) else {
        return Update::DoNothing;
    };
    let value = index
        .checked_sub(1)
        .and_then(|i| KEEP_DAYS.get(i))
        .map_or_else(String::new, u32::to_string);
    kit::set_value(&kit_ref, &mut info, azul_routing::source::SETTINGS_KEY, &value);
    Update::RefreshDom
}

/// Starts navigating the travel panel's road route, when there is one; whether it started.
pub(crate) fn start(app: &RefAny, info: &mut CallbackInfo) -> bool {
    let mut state = app.clone();
    {
        let Some(mut s) = state.downcast_mut::<MapState>() else {
            return false;
        };
        if s.drive.nav.is_some() {
            return false;
        }
        let Some(reply) = road_route(&s).cloned() else {
            return false;
        };
        let nav = Navigation::new(
            reply.road.clone(),
            reply.steps.clone(),
            reply.route.minutes * 60.0,
            reply.to,
            reply.mode,
            s.drive.simulate_mps,
        );
        let driven_by = if nav.drive.is_some() {
            "simulated".to_string()
        } else {
            s.drive.source.setting().split(':').next().unwrap_or("platform").to_string()
        };
        println!(
            "AZMAPS_NAV_START {} {} {driven_by} {}",
            reply.road.len(),
            reply.steps.len(),
            if reply.offline { "offline" } else { "online" }
        );
        s.drive.nav = Some(nav);
        s.drive.last_shown = None;
        s.drive.sidebar_was_open = s.sidebar_open;
        s.sidebar_open = false;
        s.selected = None;
        // The platform's position needs its probe mounted (the location shown).
        if s.drive.simulate_mps.is_none() && s.drive.source == Source::Platform {
            s.locating = true;
        }
    }
    keep_awake(info, true);
    // A replay drives from its start with the navigation.
    let replay = app
        .clone()
        .downcast_ref::<MapState>()
        .is_some_and(|s| matches!(s.drive.source, Source::Replay(_)));
    if replay {
        start_source(app, info);
    }
    true
}

/// Ends the navigation.
pub(crate) fn end(app: &RefAny, info: &mut CallbackInfo) {
    let mut state = app.clone();
    {
        let Some(mut s) = state.downcast_mut::<MapState>() else {
            return;
        };
        if s.drive.nav.take().is_none() {
            return;
        }
        s.sidebar_open = s.drive.sidebar_was_open;
        s.viewport.bearing_deg = 0.0;
        s.drive.reroute_latest.fetch_add(1, Ordering::AcqRel);
    }
    voice::stop();
    keep_awake(info, false);
    println!("AZMAPS_NAV_END");
}

/// Keeps the screen awake while navigating.
fn keep_awake(info: &mut CallbackInfo, on: bool) {
    let mut window = info.get_current_window_state();
    window.flags.prevent_system_sleep = on;
    info.modify_window_state(window);
}

/// `--navigate`: navigation starts as soon as the road route is there.
pub(crate) fn maybe_autostart(app: &RefAny, info: &mut CallbackInfo) {
    let wanted = {
        let mut state = app.clone();
        let wanted = state
            .downcast_ref::<MapState>()
            .is_some_and(|s| s.drive.navigate_on_route && s.drive.nav.is_none() && road_route(&s).is_some());
        wanted
    };
    if wanted {
        let _started = start(app, info);
    }
}

extern "C" fn on_start(app: RefAny, mut info: CallbackInfo) -> Update {
    if start(&app, &mut info) {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

extern "C" fn on_end(app: RefAny, mut info: CallbackInfo) -> Update {
    end(&app, &mut info);
    Update::RefreshDom
}

/// Flips one of the navigation's switches.
fn flip(app: &mut RefAny, f: impl FnOnce(&mut Navigation)) -> Update {
    let Some(mut s) = app.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    match s.drive.nav.as_mut() {
        Some(nav) => {
            f(nav);
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

extern "C" fn on_heading(mut app: RefAny, _info: CallbackInfo) -> Update {
    flip(&mut app, |n| n.heading_up = !n.heading_up)
}

extern "C" fn on_voice(mut app: RefAny, _info: CallbackInfo) -> Update {
    let update = flip(&mut app, |n| n.voice = !n.voice);
    let silent = app
        .downcast_ref::<MapState>()
        .is_some_and(|s| s.drive.nav.as_ref().is_some_and(|n| !n.voice));
    if silent {
        voice::stop();
    }
    update
}

extern "C" fn on_car_mode(mut app: RefAny, _info: CallbackInfo) -> Update {
    flip(&mut app, |n| n.car_mode = !n.car_mode)
}

// ==== The timer ====

/// One tick of the app's timer while navigating: the fix of the moment (the simulated car's,
/// else the external source's, else the platform's `platform`), the guidance, what to say, a
/// new route to ask for, the camera. Whether the window is to be rebuilt: only when the moment
/// cannot be shown in place ([`show_in_place`]).
pub(crate) fn tick(app: &RefAny, info: &mut CallbackInfo, platform: Option<(f64, f64, f32, f32, f32)>) -> bool {
    let mut state = app.clone();
    let (events, voice_on, reroute) = {
        let Some(mut s) = state.downcast_mut::<MapState>() else {
            return false;
        };
        let due = s.drive.last_shown.map_or(true, |at| at.elapsed() >= SHOW_EVERY);
        if !due {
            return false;
        }
        let external = s
            .drive
            .external
            .filter(|(_, at)| at.elapsed().as_secs() <= FRESH_S)
            .map(|(fix, _)| fix);
        let use_external = s.drive.source != Source::Platform;
        let Some(nav) = s.drive.nav.as_mut() else {
            return false;
        };
        let now = nav.clock();
        let fix: Option<Fix> = if nav.drive.is_some() {
            nav.simulated_fix(now)
        } else if use_external {
            external.map(|f| fix_of_gnss(&f, now))
        } else {
            platform.map(|(lat, lon, accuracy, heading, speed)| fix_of_platform(lat, lon, accuracy, heading, speed, now))
        };
        let Some(fix) = fix else {
            return false;
        };
        let events = nav.update(&fix, now);
        let voice_on = nav.voice;
        let camera = nav.camera();
        let progress = nav.progress;
        let (mode, destination) = (nav.mode, nav.destination);
        if let Some(p) = progress {
            println!(
                "AZMAPS_NAV {} {:.0} {:.0} {:?} {:.5},{:.5}",
                p.next_step.map_or(-1, |n| n as i64),
                p.to_next_m,
                p.remaining_m,
                p.state,
                p.shown_at.0,
                p.shown_at.1
            );
        }
        if let Some(camera) = camera {
            s.viewport.centre_lat_deg = camera.at.0;
            s.viewport.centre_lon_deg = camera.at.1;
            s.viewport.zoom = camera.zoom;
            s.viewport.bearing_deg = camera.bearing_deg;
        }
        s.last_fix = Some(fix.at);
        s.drive.last_shown = Some(Instant::now());
        // A request for a new route, from where the car is.
        let reroute = events.iter().find_map(|e| match e {
            Event::Reroute { from } => Some(*from),
            _ => None,
        });
        let reroute = reroute.map(|from| {
            let id = s.drive.reroute_latest.fetch_add(1, Ordering::AcqRel) + 1;
            println!("AZMAPS_REROUTE {:.5},{:.5}", from.0, from.1);
            RouteRequest {
                id,
                from,
                to: destination,
                mode,
                tiles: s.routing.clone(),
                online: s.online.clone(),
                language: azul_appkit::l10n::locale().chars().take(2).collect(),
                latest: Arc::clone(&s.drive.reroute_latest),
                on_done: on_rerouted,
                on_progress: on_reroute_progress,
            }
        });
        (events, voice_on, reroute)
    };
    for event in &events {
        match event {
            Event::Say { text, interrupt } => {
                if voice_on {
                    let _taken = voice::say(text, *interrupt);
                } else {
                    println!("AZMAPS_SAY_SILENT {text}");
                }
            }
            Event::Arrived => {
                println!("AZMAPS_ARRIVED");
                keep_awake(info, false);
            }
            Event::Reroute { .. } => {}
        }
    }
    if let Some(request) = reroute {
        info.add_thread(
            ThreadId::unique(),
            Thread::create(RefAny::new(request), app.clone(), route_worker),
        );
    }
    !show_in_place(app, info)
}

/// The new route arrived: the guidance follows it; without one the next request waits longer.
extern "C" fn on_rerouted(mut app: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(reply) = msg.downcast_ref::<RouteReply>().map(|r| r.clone()) else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    if s.drive.reroute_latest.load(Ordering::Acquire) != reply.id {
        return Update::DoNothing;
    }
    let Some(nav) = s.drive.nav.as_mut() else {
        return Update::DoNothing;
    };
    if reply.kind == RouteKind::Road && reply.road.len() >= 2 {
        let now = nav.clock();
        nav.rerouted(reply.road.clone(), reply.steps.clone(), reply.route.minutes * 60.0, now, reply.offline);
        println!(
            "AZMAPS_REROUTED {} {} {}",
            nav.reroutes,
            reply.road.len(),
            if reply.offline { "offline" } else { "online" }
        );
    } else {
        nav.reroute_failed();
        println!("AZMAPS_REROUTE_FAILED");
    }
    Update::RefreshDom
}

/// A new route's tile progress is not shown: the banner stays as it is.
extern "C" fn on_reroute_progress(_app: RefAny, _msg: RefAny, _info: CallbackInfo) -> Update {
    Update::DoNothing
}

// ==== On screen ====

/// Whether it is night for the car's colours: the window is dark, or the local hour is.
fn night(dark_window: bool) -> bool {
    dark_window || is_night(chrono::Local::now().hour())
}

/// What the banner and the bar show at one moment of the navigation: the texts a tick
/// rewrites in place, and the look that needs the window rebuilt when it changes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Shown {
    /// What decides the banner's and the bar's nodes and styles.
    pub look: Look,
    /// How far the next maneuver is; empty when there is none to count down to.
    pub distance: String,
    /// What to do there.
    pub instruction: String,
    /// The bar's arrival time, time left and distance left; empty after arrival.
    pub summary: String,
    /// The fix indicator's line.
    pub indicator: String,
}

/// What the banner and the bar look like, apart from their texts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Look {
    /// Whether the window was dark at the build: a tick has no other way to know it, and a
    /// switch of mode rebuilds the window anyway.
    pub dark_window: bool,
    /// The night colours.
    pub night: bool,
    /// The big-type car layout.
    pub car: bool,
    /// Whether the banner has a distance line.
    pub distance: bool,
    /// Whether the bar has the offline tag.
    pub offline: bool,
    /// The fix indicator's dot colour; `None` without an indicator.
    pub indicator: Option<(u8, u8, u8)>,
    /// The next maneuver's arrow, an icon of the icon set. An icon node is drawn at the
    /// banner's size, which an icon that can be swapped in place is not, so a new maneuver
    /// rebuilds the window: once per maneuver, not once per position.
    pub icon: &'static str,
}

/// The banner and the bar at the moment `now_ts` (Unix seconds, for the arrival time); `None`
/// while not navigating.
pub(crate) fn shown(s: &MapState, dark_window: bool, now_ts: i64) -> Option<Shown> {
    let nav = s.drive.nav.as_ref()?;
    let progress = nav.progress;
    // The next maneuver: its arrow, how far, what to do.
    let (icon, distance, instruction) = match progress {
        _ if nav.arrived() => ("flag", String::new(), t_args("azmaps-nav-arrived", &[])),
        None => ("navigation", String::new(), t_args("azmaps-nav-waiting", &[])),
        Some(p) => {
            let step = p.next_step.and_then(|n| nav.guidance.steps().get(n));
            let icon = step.map_or("straight", |st| st.kind.icon());
            let text = match step {
                Some(st) if st.kind == Maneuver::Destination => t_args("azmaps-nav-destination", &[]),
                Some(st) => st.instruction.clone(),
                None if nav.rerouting => t_args("azmaps-nav-rerouting", &[]),
                None => t_args("azmaps-nav-follow", &[]),
            };
            (icon, banner_distance(p.to_next_m), text)
        }
    };
    // The bar: arrival, time and distance left.
    let summary = match progress {
        Some(p) if !nav.arrived() => {
            let arrival = now_ts + p.remaining_s.round() as i64;
            t_args(
                "azmaps-nav-summary",
                &[
                    ("arrival", Arg::from(clock(arrival))),
                    ("left", Arg::from(span_text(p.remaining_s.round() as i64))),
                    ("distance", Arg::from(distance_text(p.remaining_m / 1000.0))),
                ],
            )
        }
        _ => String::new(),
    };
    let indicator = indicator_line(s, None);
    Some(Shown {
        look: Look {
            dark_window,
            night: night(dark_window),
            car: nav.car_mode,
            distance: !distance.is_empty(),
            offline: nav.offline,
            indicator: indicator.as_ref().map(|(colour, _)| *colour),
            icon,
        },
        distance,
        instruction,
        summary,
        indicator: indicator.map(|(_, line)| line).unwrap_or_default(),
    })
}

/// A text the navigation rewrites in place: a span around the text node marked `marker`.
fn live_text(text: &str, marker: AzString) -> Dom {
    Dom::create_span().with_child(
        Dom::create_text_do_not_use_without_block_level_wrapper(text).with_marker(OptionString::Some(marker)),
    )
}

/// The banner and the bar while navigating; nothing otherwise. What they show is kept for
/// the ticks that follow ([`show_in_place`]).
pub(crate) fn banner(s: &MapState, app: &RefAny, dark_window: bool) -> Vec<Dom> {
    let shown = shown(s, dark_window, chrono::Local::now().timestamp());
    *s.drive.banner_shown.borrow_mut() = shown.clone();
    let (Some(nav), Some(shown)) = (s.drive.nav.as_ref(), shown) else {
        return Vec::new();
    };
    let look = shown.look;
    let car = look.car;
    let banner_css = if look.night { format!("{BANNER} {BANNER_NIGHT}") } else { BANNER.to_string() };
    let mut words = Dom::create_div().with_css("display: flex; flex-direction: column; flex-grow: 1; min-width: 0px;");
    if look.distance {
        words.add_child(
            Dom::create_div()
                .with_css(if car { BANNER_DISTANCE_CAR } else { BANNER_DISTANCE })
                .with_id(ids::NAV_DISTANCE)
                .with_child(live_text(&shown.distance, ids::NAV_DISTANCE_TEXT)),
        );
    }
    words.add_child(
        Dom::create_div()
            .with_css(if car { BANNER_TEXT_CAR } else { BANNER_TEXT })
            .with_id(ids::NAV_INSTRUCTION)
            .with_child(live_text(&shown.instruction, ids::NAV_INSTRUCTION_TEXT)),
    );
    let top = Dom::create_div()
        .with_css(banner_css.as_str())
        .with_id(ids::NAV_BANNER)
        .with_accessibility_name(label("azmaps-nav-banner"))
        .with_child(
            Dom::create_div()
                .with_css(if car { BANNER_ICON_CAR } else { BANNER_ICON })
                // The maneuver as the icon set's glyph: a span in an icon font family shows
                // the icon's name as text ("turn_right").
                .with_child(Dom::create_icon(look.icon)),
        )
        .with_child(words);

    // The bar: arrival, time and distance left, the switches.
    let bar_css = if look.night { format!("{BAR} {BAR_NIGHT}") } else { BAR.to_string() };
    let mut bar = Dom::create_div()
        .with_css(bar_css.as_str())
        .with_id(ids::NAV_BAR)
        .with_child(
            Dom::create_div()
                .with_css(if car { BAR_TEXT_CAR } else { BAR_TEXT })
                .with_id(ids::NAV_SUMMARY)
                .with_child(live_text(&shown.summary, ids::NAV_SUMMARY_TEXT)),
        );
    if look.offline {
        bar.add_child(Dom::create_div().with_css(OFFLINE_TAG).with_child(Dom::create_span_with_text(t_args("azmaps-offline", &[]))));
    }
    if let Some(colour) = look.indicator {
        bar.add_child(indicator_dom(colour, live_text(&shown.indicator, ids::NAV_INDICATOR_TEXT)));
    }
    let switch = |icon: &str, name: &str, on: bool, id: AzString, cb: ButtonOnClickCallbackType| {
        Button::create("")
            .with_icon(icon)
            .with_toggled(on)
            .with_on_click(app.clone(), cb)
            .dom()
            .with_id(id)
            .with_accessibility_name(label(name))
    };
    bar.add_child(switch(
        if nav.heading_up { "navigation" } else { "explore" },
        if nav.heading_up { "azmaps-nav-heading-up" } else { "azmaps-nav-north-up" },
        nav.heading_up,
        ids::NAV_HEADING,
        on_heading,
    ));
    bar.add_child(switch(
        if nav.voice { "volume_up" } else { "volume_off" },
        "azmaps-nav-voice",
        nav.voice,
        ids::NAV_VOICE,
        on_voice,
    ));
    bar.add_child(switch("directions_car", "azmaps-nav-car-mode", nav.car_mode, ids::NAV_CAR, on_car_mode));
    bar.add_child(
        Button::with_type(label("azmaps-nav-end"), ButtonType::Primary)
            .with_on_click(app.clone(), on_end as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::NAV_END),
    );
    vec![top, bar]
}

/// Shows the moment of the navigation without a window rebuild: the map's camera and
/// overlays move in place and the banner's and the bar's texts that changed are rewritten.
/// False when that cannot be done (the banner looks different now, or the map or a text is
/// not in the window): the window is rebuilt then.
fn show_in_place(app: &RefAny, info: &mut CallbackInfo) -> bool {
    let mut state = app.clone();
    let (before, now, viewport, overlays) = {
        let Some(s) = state.downcast_ref::<MapState>() else {
            return false;
        };
        let Some(before) = s.drive.banner_shown.borrow().clone() else {
            return false;
        };
        let Some(now) = shown(&s, before.look.dark_window, chrono::Local::now().timestamp()) else {
            return false;
        };
        if now.look != before.look {
            return false;
        }
        (before, now, s.viewport, crate::map_overlays(&s, app))
    };
    let Some(map) = info.get_node_id_by_marker(ids::MAP_VIEW).into_option() else {
        return false;
    };
    if !MapWidget::update_camera(*info, map, viewport) {
        return false;
    }
    let _drawn = MapWidget::update_overlays(*info, map, MapOverlayVec::from_vec(overlays));
    let texts = [
        (ids::NAV_DISTANCE_TEXT, &before.distance, &now.distance),
        (ids::NAV_INSTRUCTION_TEXT, &before.instruction, &now.instruction),
        (ids::NAV_SUMMARY_TEXT, &before.summary, &now.summary),
        (ids::NAV_INDICATOR_TEXT, &before.indicator, &now.indicator),
    ];
    for (marker, old, new) in texts {
        if old == new {
            continue;
        }
        let Some(node) = info.get_node_id_by_marker(marker).into_option() else {
            return false;
        };
        info.change_node_text(node, AzString::from(new.as_str()));
    }
    if let Some(s) = state.downcast_ref::<MapState>() {
        *s.drive.banner_shown.borrow_mut() = Some(now);
    }
    true
}

/// The navigation on the map: its route (the new one after a reroute) and the car, an arrow
/// pointing where it goes.
pub(crate) fn overlays(s: &MapState) -> Vec<MapOverlay> {
    let Some(nav) = s.drive.nav.as_ref() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let line = &nav.guidance.line().points;
    if line.len() >= 2 {
        let points = MapLatLonVec::from_vec(line.iter().copied().map(at).collect());
        out.push(MapOverlay::Polyline(crate::travel_polyline(points, true, nav.mode)));
    }
    if let Some(p) = nav.progress {
        // The arrow turns by the heading less the map's own turn.
        let turn = p.heading_deg as f32 - s.viewport.bearing_deg;
        let arrow = Dom::create_div()
            .with_css(format!(
                "width: 26px; height: 26px; border-radius: 13px; background: #ffffff; display: flex; \
                 align-items: center; justify-content: center; box-shadow: 0px 1px 4px rgba(0,0,0,0.4); \
                 transform: rotate({turn:.0}deg);"
            ).as_str())
            .with_child(
                Dom::create_div().with_css(
                    "width: 0px; height: 0px; border-left: 7px solid transparent; border-right: 7px solid \
                     transparent; border-bottom: 16px solid #1a73e8; margin-top: -3px;",
                ),
            );
        out.push(MapOverlay::Marker(
            MapMarker::create(CAR_MARKER, at(p.shown_at), arrow)
                .with_anchor(13.0, 13.0)
                .with_name(label("azmaps-you-are-here")),
        ));
    }
    out
}

// ==== Settings > Location ====

/// The sources the settings offer, in order.
const SOURCES: [&str; 4] = ["azmaps-gps-platform", "azmaps-gps-gpsd", "azmaps-gps-device", "azmaps-gps-replay"];

fn source_index(source: &Source) -> usize {
    match source {
        Source::Platform => 0,
        Source::Gpsd(_) => 1,
        Source::Nmea(_) => 2,
        Source::Replay(_) => 3,
    }
}

/// Settings > Navigation > Location: where the position comes from, the device or the file,
/// the live fix-quality indicator, and the note on what each source is.
pub(crate) fn location_section(s: &MapState, app: &RefAny, platform_accuracy: Option<f32>) -> kit::AppSection {
    let mut content = Dom::create_div().with_css(SETTINGS_COLUMN).with_child(kit::row(
        "azmaps-gps-source",
        Segmented::create(labels(&SOURCES))
            .with_selected_index(source_index(&s.drive.source))
            .with_on_change(app.clone(), on_source_kind as SegmentedOnChangeCallbackType)
            .dom()
            .with_id(ids::GPS_SOURCE),
    ));
    match &s.drive.source {
        Source::Nmea(_) => {
            let devices = gnss::serial_devices();
            if devices.is_empty() {
                content.add_child(kit::note("azmaps-gps-no-devices"));
            } else {
                let names: Vec<AzString> = devices.iter().map(|d| AzString::from(d.clone())).collect();
                let selected = devices.iter().position(|d| *d == s.drive.source_text).unwrap_or(0);
                content.add_child(kit::row(
                    "azmaps-gps-device",
                    DropDown::create(StringVec::from(names))
                        .with_selected(selected)
                        .with_on_choice_change(app.clone(), on_device as DropDownOnChoiceChangeCallbackType)
                        .dom()
                        .with_id(ids::GPS_DEVICE),
                ));
            }
        }
        Source::Replay(_) | Source::Gpsd(_) => {
            let placeholder = if matches!(s.drive.source, Source::Gpsd(_)) { "azmaps-gps-gpsd-address" } else { "azmaps-gps-replay-file" };
            content.add_child(kit::row(
                placeholder,
                TextInput::create()
                    .with_text(s.drive.source_text.as_str())
                    .with_placeholder(label(placeholder))
                    .with_on_text_input(app.clone(), on_source_text as TextInputOnTextInputCallbackType)
                    .dom()
                    .with_id(ids::GPS_TEXT),
            ));
            content.add_child(
                Button::create(label("azmaps-gps-apply"))
                    .with_on_click(app.clone(), on_source_apply as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::GPS_APPLY),
            );
        }
        Source::Platform => {}
    }
    if let Some(indicator) = indicator(s, platform_accuracy) {
        content.add_child(indicator);
    }
    content.add_child(kit::note("azmaps-gps-about"));
    kit::AppSection {
        category: 0,
        title: t_args("azmaps-gps-title", &[]),
        content,
    }
}

/// Chooses `source`: kept in settings.json and read from now on.
fn choose_source(app: &mut RefAny, info: &mut CallbackInfo, source: Source) {
    let kit_ref = {
        let Some(mut s) = app.downcast_mut::<MapState>() else {
            return;
        };
        s.drive.source = source.clone();
        s.kit.clone()
    };
    kit::set_value(&kit_ref, info, SOURCE_KEY, &source.setting());
    start_source(app, info);
}

extern "C" fn on_source_kind(mut app: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let text = app.downcast_ref::<MapState>().map(|s| s.drive.source_text.clone()).unwrap_or_default();
    let source = match state.selected_index {
        1 => Source::Gpsd(if text.contains(':') { text } else { gnss::GPSD_ADDRESS.to_string() }),
        2 => match gnss::serial_devices().into_iter().next() {
            Some(device) => Source::Nmea(device),
            None => Source::Nmea(text),
        },
        3 => Source::Replay(text),
        _ => Source::Platform,
    };
    // A device or a file not named yet waits for its name.
    if matches!(&source, Source::Nmea(p) | Source::Replay(p) if p.trim().is_empty()) {
        if let Some(mut s) = app.downcast_mut::<MapState>() {
            s.drive.source = source;
        }
        return Update::RefreshDom;
    }
    choose_source(&mut app, &mut info, source);
    Update::RefreshDom
}

extern "C" fn on_device(mut app: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let Some(device) = gnss::serial_devices().into_iter().nth(index) else {
        return Update::DoNothing;
    };
    if let Some(mut s) = app.downcast_mut::<MapState>() {
        s.drive.source_text = device.clone();
    }
    choose_source(&mut app, &mut info, Source::Nmea(device));
    Update::RefreshDom
}

extern "C" fn on_source_text(mut app: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    if let Some(mut s) = app.downcast_mut::<MapState>() {
        s.drive.source_text = state.get_text().as_str().to_string();
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_source_apply(mut app: RefAny, mut info: CallbackInfo) -> Update {
    let Some(source) = app.downcast_ref::<MapState>().map(|s| {
        let text = s.drive.source_text.trim().to_string();
        match s.drive.source {
            Source::Gpsd(_) => Source::Gpsd(if text.is_empty() { gnss::GPSD_ADDRESS.to_string() } else { text }),
            Source::Replay(_) => Source::Replay(text),
            Source::Nmea(_) => Source::Nmea(text),
            Source::Platform => Source::Platform,
        }
    }) else {
        return Update::DoNothing;
    };
    choose_source(&mut app, &mut info, source);
    Update::RefreshDom
}
