//! The public transport part of the travel panel: when to travel (leave now, depart at, arrive
//! by), the journeys - each row its departure and arrival, how long it takes, its changes and a
//! delay -, the picked journey's legs with their lines and platforms, earlier and later
//! journeys, and the credit of the timetable service (its usage policy asks for a visible link
//! to its sources). The picked journey is drawn on the map leg by leg, each in its line's
//! colour, the walks thin and grey.
//!
//! The journeys are asked of a worker (`journeys.rs`) when the panel's ends, mode or time
//! change; a newer question supersedes an older one.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use azul::{
    callbacks::{ButtonOnClickCallbackType, SegmentedOnChangeCallbackType, TimePickerOnChangeCallbackType},
    css::ColorU,
    prelude::*,
    str::String as AzString,
    task::{Thread, ThreadId},
    vec::MapLatLonVec,
    widgets::{
        Button, MapLineDash, MapMarker, MapOverlay, MapPolyline, Segmented, SegmentedState, TimePicker,
        TimePickerState,
    },
};
use azul_appkit::l10n::{label, labels, t_args, Arg};
use azul_routing::{
    fetch::Origin,
    transit::{Journey, JourneyQuery, LegKind, TransitError, TransitProvider, When},
};
use chrono::Timelike;

use crate::{
    at, ids,
    journeys::{
        clock, delay_text, journey_worker, leg_title, script_lines, span_text, transfers_text, upcoming,
        when_of, JourneyReply, JourneyRequest, WhenKind,
    },
    model::TravelMode,
    MapState,
};

/// The part under the modes.
const PANEL: &str = "display: flex; flex-direction: column; gap: 4px;";
const ROW: &str = "display: flex; flex-direction: row; align-items: center; gap: 6px;";
const NOTE: &str = "font-size: 11px; color: system:secondary-text; white-space: normal;";
const JOURNEYS: &str = "display: flex; flex-direction: column; align-items: stretch; gap: 2px; max-height: 260px; \
                        overflow-y: auto; min-height: 0px;";
const JOURNEY_ROW: &str = "min-width: 0px; height: auto; justify-content: flex-start; text-align: left; \
                           white-space: normal;";
const LEGS: &str = "display: flex; flex-direction: column; gap: 3px; padding: 4px 6px 6px 22px;";
const LEG_ROW: &str = "display: flex; flex-direction: row; align-items: center; gap: 6px; font-size: 12px;";
const LEG_SWATCH: &str = "width: 4px; height: 16px; border-radius: 2px;";
const DELAY: &str = "color: #c62828; font-weight: bold; @theme(flora) { color: #7E4A42; \
                     @media (prefers-color-scheme: dark) { color: #E39A8F; } }";
const CHANGED: &str = "color: #c62828;";

/// The marker ids of a journey's stops: far above the pins' indexes, below the travel line's
/// ends (`model::FROM_MARKER`).
const STOP_MARKER_BASE: u64 = u64::MAX - 1_000;

/// The panel's state: the time asked for and the journeys of the newest question.
pub(crate) struct TransitState {
    /// Leave now, depart at, arrive by.
    pub kind: WhenKind,
    /// The time picked for "depart at" / "arrive by", local.
    pub hour: u32,
    pub minute: u32,
    /// The newest question; a panel that still asks it asks nothing.
    pub asked: Option<JourneyQuery>,
    /// The answer to the newest question.
    pub reply: Option<JourneyReply>,
    /// The journey shown with its legs and drawn on the map (an index into the shown ones).
    pub picked: usize,
    /// The newest question's number, shared with the workers.
    pub latest: Arc<AtomicU64>,
    /// Who plans the journeys; `None` with `--transit off`.
    pub provider: Option<Arc<dyn TransitProvider>>,
}

impl TransitState {
    /// The panel with `provider`, asking for "now" at the current time.
    pub(crate) fn new(provider: Option<Arc<dyn TransitProvider>>) -> Self {
        let now = chrono::Local::now();
        Self {
            kind: WhenKind::Now,
            hour: now.hour(),
            minute: now.minute(),
            asked: None,
            reply: None,
            picked: 0,
            latest: Arc::new(AtomicU64::new(0)),
            provider,
        }
    }

    /// The question of the panel as it is: its ends, its time, `page`.
    fn query(&self, s: &MapState, page: Option<String>) -> Option<JourneyQuery> {
        let (Some(from), Some(to)) = s.travel_ends() else {
            return None;
        };
        let text = format!("{:02}:{:02}", self.hour, self.minute);
        let when = when_of(self.kind, &text, &chrono::Local::now()).unwrap_or(When::Now);
        Some(JourneyQuery {
            from,
            to,
            when,
            page,
            language: azul_appkit::l10n::locale().chars().take(2).collect(),
        })
    }
}

/// The journeys of the travel panel as it is now are asked of a worker - when the mode is
/// public transport, both ends are places, and they were not asked already.
pub(crate) fn ask(app: &RefAny, info: &mut CallbackInfo) {
    ask_page(app, info, None, false);
}

/// [`ask`] for the page `page` (earlier or later journeys), or again (`again`).
fn ask_page(app: &RefAny, info: &mut CallbackInfo, page: Option<String>, again: bool) {
    let mut state = app.clone();
    let request = {
        let Some(mut s) = state.downcast_mut::<MapState>() else {
            return;
        };
        if s.travel.mode != TravelMode::Transit {
            return;
        }
        let Some(provider) = s.transit.provider.clone() else {
            return;
        };
        let Some(query) = s.transit.query(&s, page) else {
            s.transit.asked = None;
            s.transit.reply = None;
            return;
        };
        if !again && s.transit.asked.as_ref() == Some(&query) {
            return;
        }
        s.transit.asked = Some(query.clone());
        s.transit.reply = None;
        s.transit.picked = 0;
        let id = s.transit.latest.fetch_add(1, Ordering::AcqRel) + 1;
        JourneyRequest {
            id,
            query,
            provider,
            latest: Arc::clone(&s.transit.latest),
            on_done: on_journeys,
        }
    };
    info.add_thread(
        ThreadId::unique(),
        Thread::create(RefAny::new(request), app.clone(), journey_worker),
    );
}

/// The journey worker answered: shown when it answers the newest question.
extern "C" fn on_journeys(mut app: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(reply) = msg.downcast_ref::<JourneyReply>().map(|r| r.clone()) else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    if s.transit.latest.load(Ordering::Acquire) != reply.id {
        return Update::DoNothing;
    }
    match &reply.answer {
        Ok(page) => {
            for line in script_lines(page) {
                println!("{line}");
            }
        }
        Err(why) => println!("AZMAPS_NO_JOURNEY {}", error_key(why)),
    }
    s.transit.reply = Some(reply);
    s.transit.picked = 0;
    Update::RefreshDom
}

/// The word scripts read for why there is no journey.
fn error_key(error: &TransitError) -> &'static str {
    match error {
        TransitError::Offline(_) => "offline",
        TransitError::Busy => "busy",
        TransitError::NoJourney(_) => "none",
        TransitError::Server(_) => "service-failed",
    }
}

/// The panel's words for why there is no journey.
fn error_label(error: &TransitError) -> &'static str {
    match error {
        TransitError::Offline(_) => "azmaps-transit-offline",
        TransitError::Busy => "azmaps-transit-busy",
        TransitError::NoJourney(_) => "azmaps-transit-none",
        TransitError::Server(_) => "azmaps-transit-failed",
    }
}

/// A row's or a button's handle: which one, and the app.
struct RowRef {
    app: RefAny,
    index: usize,
}

/// The public transport panel, under the travel panel's modes.
pub(crate) fn panel(s: &MapState, app: &RefAny) -> Dom {
    let t = &s.transit;
    let mut panel = Dom::create_div().with_css(PANEL).with_id(ids::TRANSIT);
    if t.provider.is_none() {
        return panel.with_child(note(label("azmaps-transit-off").as_str().to_string()));
    }
    let kinds: Vec<&str> = WhenKind::ALL.iter().map(|k| k.label()).collect();
    let mut when = Dom::create_div().with_css(ROW).with_child(
        Segmented::create(labels(&kinds))
            .with_selected_index(WhenKind::ALL.iter().position(|k| *k == t.kind).unwrap_or(0))
            .with_on_change(app.clone(), on_when_kind as SegmentedOnChangeCallbackType)
            .dom()
            .with_id(ids::TRANSIT_WHEN),
    );
    if t.kind != WhenKind::Now {
        when.add_child(
            TimePicker::create(t.hour, t.minute)
                .with_24h(true)
                .with_accessibility_name(label("azmaps-transit-time"))
                .with_on_change(app.clone(), on_time as TimePickerOnChangeCallbackType)
                .dom()
                .with_id(ids::TRANSIT_TIME),
        );
    }
    panel.add_child(when);
    if s.travel_ends().1.is_none() || s.travel_ends().0.is_none() {
        return panel;
    }
    let Some(reply) = &t.reply else {
        return panel.with_child(note(t_args("azmaps-transit-asking", &[])));
    };
    let page = match &reply.answer {
        Ok(page) => page,
        Err(why) => {
            return panel
                .with_child(note(t_args(error_label(why), &[])))
                .with_child(credit(reply, app));
        }
    };
    if page.origin == Origin::Offline {
        panel.add_child(note(t_args("azmaps-transit-offline-shown", &[])));
    }
    let now = chrono::Local::now().timestamp();
    let shown = upcoming(page, now);
    let mut list = Dom::create_div().with_css(JOURNEYS).with_id(ids::JOURNEYS);
    for (index, journey) in shown.iter().enumerate() {
        list.add_child(
            Button::create(journey_text(journey))
                .with_icon(journey.rides().next().map_or("directions_transit", |l| l.kind.icon()))
                .with_toggled(index == t.picked)
                .with_on_click(
                    RefAny::new(RowRef {
                        app: app.clone(),
                        index,
                    }),
                    on_journey as ButtonOnClickCallbackType,
                )
                .dom()
                .with_id(ids::indexed("journey", index))
                .with_css(JOURNEY_ROW),
        );
        if index == t.picked {
            list.add_child(legs(journey));
        }
    }
    panel.add_child(list);
    let mut pages = Dom::create_div().with_css(ROW);
    if page.earlier.is_some() {
        pages.add_child(page_button(app, "azmaps-transit-earlier", "arrow_upward", ids::TRANSIT_EARLIER, on_earlier));
    }
    if page.later.is_some() {
        pages.add_child(page_button(app, "azmaps-transit-later", "arrow_downward", ids::TRANSIT_LATER, on_later));
    }
    panel.with_child(pages).with_child(credit(reply, app))
}

/// A line of secondary text.
fn note(text: String) -> Dom {
    Dom::create_div()
        .with_css(NOTE)
        .with_child(Dom::create_span_with_text(text))
}

/// A journey's row: `07:27 – 08:42 · 1 h 15 min · direct`, its first ride's delay.
fn journey_text(journey: &Journey) -> String {
    let mut text = format!(
        "{} \u{2013} {} \u{b7} {} \u{b7} {}",
        clock(journey.departure),
        clock(journey.arrival),
        span_text(journey.duration_s()),
        transfers_text(journey.transfers)
    );
    if let Some(delay) = journey.rides().next().and_then(|l| delay_text(l.departure_delay_min())) {
        text.push_str(&format!(" ({delay})"));
    }
    text
}

/// The picked journey's legs: each its colour, its departure, its title, its platforms.
fn legs(journey: &Journey) -> Dom {
    let mut legs = Dom::create_div().with_css(LEGS).with_id(ids::JOURNEY_LEGS);
    for (index, leg) in journey.legs.iter().enumerate() {
        let (r, g, b) = leg.line_colour();
        let mut row = Dom::create_div()
            .with_css(LEG_ROW)
            .with_id(ids::indexed("leg", index))
            .with_child(
                Dom::create_div()
                    .with_css(format!("{LEG_SWATCH} background: rgb({r}, {g}, {b});").as_str()),
            )
            .with_child(Dom::create_span_with_text(clock(leg.departure)));
        if let Some(delay) = delay_text(leg.departure_delay_min()).filter(|_| leg.kind.is_transit()) {
            row.add_child(Dom::create_div().with_css(DELAY).with_child(Dom::create_span_with_text(delay)));
        }
        row.add_child(Dom::create_span_with_text(leg_title(leg)));
        if leg.kind.is_transit() {
            if !leg.from.name.is_empty() {
                row.add_child(Dom::create_span_with_text(format!("\u{b7} {}", leg.from.name)));
            }
            if let Some(platform) = &leg.from.platform {
                let text = t_args("azmaps-transit-platform", &[("platform", Arg::from(platform.clone()))]);
                let css = if leg.from.platform_changed() { CHANGED } else { "" };
                row.add_child(Dom::create_div().with_css(css).with_child(Dom::create_span_with_text(text)));
            }
            if leg.cancelled {
                row.add_child(Dom::create_div().with_css(DELAY).with_child(Dom::create_span_with_text(t_args("azmaps-transit-cancelled", &[]))));
            }
        }
        legs.add_child(row);
    }
    legs
}

/// The earlier / later journeys' button.
fn page_button(app: &RefAny, name: &str, icon: &str, id: AzString, on_click: ButtonOnClickCallbackType) -> Dom {
    Button::create(label(name))
        .with_icon(icon)
        .with_on_click(app.clone(), on_click)
        .dom()
        .with_id(id)
}

/// The timetable service's credit and the link to its sources.
fn credit(reply: &JourneyReply, app: &RefAny) -> Dom {
    let words = t_args("azmaps-transit-credit", &[("provider", Arg::from(reply.credit.to_string()))]);
    Dom::create_div()
        .with_css(ROW)
        .with_id(ids::TRANSIT_CREDIT)
        .with_child(note(words))
        .with_child(
            Button::create(label("azmaps-transit-sources"))
                .with_on_click(app.clone(), on_sources as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::TRANSIT_SOURCES),
        )
}

/// The picked journey on the map, leg by leg: rides in their line's colour on a white casing,
/// walks thin and grey; a dot where each ride starts.
pub(crate) fn overlays(s: &MapState) -> Vec<MapOverlay> {
    let mut out = Vec::new();
    if s.travel.mode != TravelMode::Transit {
        return out;
    }
    let Some(Ok(page)) = s.transit.reply.as_ref().map(|r| &r.answer) else {
        return out;
    };
    let now = chrono::Local::now().timestamp();
    let Some(journey) = upcoming(page, now).get(s.transit.picked).copied() else {
        return out;
    };
    for leg in &journey.legs {
        if leg.points.len() < 2 {
            continue;
        }
        let (r, g, b) = leg.line_colour();
        let colour = ColorU { r, g, b, a: 0xff };
        let points = MapLatLonVec::from_vec(leg.points.iter().copied().map(at).collect());
        out.push(MapOverlay::Polyline(leg_line(points, leg.kind, colour)));
    }
    for (i, leg) in journey.rides().enumerate() {
        let (r, g, b) = leg.line_colour();
        out.push(MapOverlay::Marker(
            MapMarker::create_dot(STOP_MARKER_BASE + i as u64, at(leg.from.at), ColorU { r, g, b, a: 0xff })
                .with_name(AzString::from(leg.from.name.clone())),
        ));
    }
    out
}

/// A leg's line in its `colour` on a white outline: a ride is a wide line, a walk dots.
fn leg_line(points: MapLatLonVec, kind: LegKind, colour: ColorU) -> MapPolyline {
    let casing = ColorU { r: 0xff, g: 0xff, b: 0xff, a: 0xff };
    if kind == LegKind::Walk {
        MapPolyline::create(points)
            .with_color(colour)
            .with_width(5.0)
            .with_casing(casing, 8.0)
            .with_dash(MapLineDash::Dotted)
    } else {
        MapPolyline::create(points).with_color(colour).with_width(6.0).with_casing(casing, 9.0)
    }
}

extern "C" fn on_when_kind(mut app: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    {
        let Some(mut s) = app.downcast_mut::<MapState>() else {
            return Update::DoNothing;
        };
        s.transit.kind = WhenKind::ALL.get(state.selected_index).copied().unwrap_or_default();
    }
    ask(&app, &mut info);
    Update::RefreshDom
}

extern "C" fn on_time(mut app: RefAny, mut info: CallbackInfo, state: TimePickerState) -> Update {
    {
        let Some(mut s) = app.downcast_mut::<MapState>() else {
            return Update::DoNothing;
        };
        s.transit.hour = if state.is_24h { state.hour } else { state.hour % 12 + if state.is_pm { 12 } else { 0 } };
        s.transit.minute = state.minute;
    }
    ask(&app, &mut info);
    Update::RefreshDom
}

extern "C" fn on_journey(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<RowRef>().map(|r| (r.app.clone(), r.index)) else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    s.transit.picked = index;
    println!("AZMAPS_JOURNEY_PICKED {index}");
    Update::RefreshDom
}

/// The earlier or the later page of the newest question.
fn turn_page(app: &mut RefAny, info: &mut CallbackInfo, later: bool) -> Update {
    let page = app.downcast_ref::<MapState>().and_then(|s| {
        let page = s.transit.reply.as_ref()?.answer.as_ref().ok()?;
        if later {
            page.later.clone()
        } else {
            page.earlier.clone()
        }
    });
    if page.is_none() {
        return Update::DoNothing;
    }
    ask_page(app, info, page, true);
    Update::RefreshDom
}

extern "C" fn on_earlier(mut app: RefAny, mut info: CallbackInfo) -> Update {
    turn_page(&mut app, &mut info, false)
}

extern "C" fn on_later(mut app: RefAny, mut info: CallbackInfo) -> Update {
    turn_page(&mut app, &mut info, true)
}

/// The timetable service's sources page, in the browser.
extern "C" fn on_sources(mut app: RefAny, _info: CallbackInfo) -> Update {
    let url = app
        .downcast_ref::<MapState>()
        .and_then(|s| s.transit.reply.as_ref().map(|r| r.credit_url))
        .unwrap_or(azul_routing::transit::transitous::SOURCES);
    if let Ok(link) = azul::url::Url::parse(url).into_result() {
        let _opened = link.open();
    }
    Update::DoNothing
}

#[cfg(test)]
mod leg_line_tests {
    use super::*;

    fn two_stops() -> MapLatLonVec {
        MapLatLonVec::from_vec(vec![at((52.5251, 13.3694)), at((52.5219, 13.4132))])
    }

    #[test]
    fn a_walk_between_rides_is_drawn_dotted_and_a_ride_solid() {
        let grey = ColorU { r: 0x75, g: 0x75, b: 0x75, a: 0xff };
        let walk = leg_line(two_stops(), LegKind::Walk, grey);
        assert_eq!(walk.dash, MapLineDash::Dotted, "a walk is dots, not a thin grey line");
        let ride = leg_line(two_stops(), LegKind::RegionalTrain, grey);
        assert_eq!(ride.dash, MapLineDash::Solid);
        assert!(ride.width > walk.width);
    }
}
