//! The timeline widget: tracks of clips under a time ruler, a playhead, zoom,
//! snapping, the keyboard, and only the clips in view rendered.

use std::sync::{Arc, Mutex};

use azul_core::{
    a11y::{AccessibilityRole, AccessibilityState},
    callbacks::Update,
    dom::{Dom, DomId, DomNodeId, EventFilter, HoverEventFilter, IdOrClass::Class, NodeId},
    refany::RefAny,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::VirtualKeyCode as K,
};
use azul_css::AzString;

use super::*;
use crate::{
    callbacks::CallbackInfo,
    widgets::{
        roving::test_support as rv,
        themes::{theme_blocks::checks, theme_checks, UiTheme},
    },
};

type Log = Arc<Mutex<Vec<TimelineEvent>>>;

extern "C" fn record(mut data: RefAny, _: CallbackInfo, event: TimelineEvent) -> Update {
    if let Some(log) = data.downcast_ref::<Log>() {
        log.lock().expect("log").push(event);
    }
    Update::RefreshDom
}

fn clip(id: u64, start: f64, duration: f64, label: &str) -> TimelineClip {
    TimelineClip::create(id, start, duration, AzString::from(label))
}

/// V1: "pier" 0..2 (selected), "market" 2..5; V2 empty; A1: "voice" 1..4.
fn tracks() -> TimelineTrackVec {
    TimelineTrackVec::from_vec(vec![
        TimelineTrack::create(1, AzString::from("V1"), TimelineTrackKind::Video).with_clips(
            TimelineClipVec::from_vec(vec![
                clip(11, 0.0, 2.0, "pier").with_selected(true),
                clip(12, 2.0, 3.0, "market"),
            ]),
        ),
        TimelineTrack::create(2, AzString::from("V2"), TimelineTrackKind::Video),
        TimelineTrack::create(3, AzString::from("A1"), TimelineTrackKind::Audio).with_clips(
            TimelineClipVec::from_vec(vec![clip(31, 1.0, 3.0, "voice")
                .with_tint(TimelineClipTint::Audio)]),
        ),
    ])
}

/// A 10 s sequence at 25 fps, 100 px a second from 0, the playhead at 2.4 s
/// (frame 60).
fn timeline(log: &Log) -> Timeline {
    Timeline::create(tracks(), 10.0)
        .with_playhead(2.4)
        .with_view(0.0, 100.0)
        .with_view_width(1000.0)
        .with_fps(25.0)
        .with_snapping(true)
        .with_on_event(RefAny::new(log.clone()), record as TimelineOnEventCallbackType)
}

fn no_log() -> Log {
    Arc::new(Mutex::new(Vec::new()))
}

fn id(n: NodeId) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(n)),
    }
}

/// The nodes of `styled` carrying `class`, in document order.
fn nodes_with(styled: &StyledDom, class: &str) -> Vec<NodeId> {
    styled
        .node_data
        .as_ref()
        .iter()
        .enumerate()
        .filter(|(_, nd)| {
            nd.get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|c| matches!(c, Class(s) if s.as_str() == class))
        })
        .map(|(i, _)| NodeId::new(i))
        .collect()
}

/// The one keyboard stop of the timeline: the lanes.
fn lanes(styled: &StyledDom) -> DomNodeId {
    let found = nodes_with(styled, LANES_CLASS);
    assert_eq!(found.len(), 1, "one lanes box");
    id(found[0])
}

fn events(log: &Log) -> Vec<TimelineEvent> {
    log.lock().expect("log").clone()
}

fn last(log: &Log) -> TimelineEvent {
    *events(log).last().expect("an event")
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

// ---- structure ----

#[test]
fn the_timeline_is_the_ruler_row_over_the_track_headers_and_lanes_and_the_scroll_bar() {
    for theme in checks::BOTH {
        let dom = timeline(&no_log()).with_theme(theme).dom();
        assert!(theme_checks::has_class(&dom, TIMELINE_CLASS), "{}", theme.name());
        let parts = dom.children.as_ref();
        assert_eq!(parts.len(), 3, "{}: the ruler row, the body, the scroll bar", theme.name());
        assert!(theme_checks::has_class(&parts[0], HEAD_CLASS));
        assert!(theme_checks::has_class(&parts[1], BODY_CLASS));
        assert!(theme_checks::has_class(&parts[2], SCROLL_CLASS));
        assert_eq!(theme_checks::find_all(&dom, HEADER_CLASS).len(), 3, "a header per track");
        assert_eq!(theme_checks::find_all(&dom, LANE_CLASS).len(), 3, "a lane per track");
        assert!(theme_checks::find(&dom, RULER_CLASS).is_some());
        assert!(theme_checks::find(&dom, PLAYHEAD_CLASS).is_some());
        assert!(theme_checks::find(&dom, THUMB_CLASS).is_some());
        assert_eq!(theme_checks::find_all(&dom, CLIP_CLASS).len(), 3, "every clip in view");
    }
}

#[test]
fn only_the_clips_in_view_are_rendered() {
    // A thousand one-second clips; 50 px a second over 500 px shows ten
    // seconds, from 100 s.
    let many: Vec<TimelineClip> = (0..1000u64)
        .map(|i| clip(i, i as f64, 1.0, "shot"))
        .collect();
    let track = TimelineTrack::create(1, AzString::from("V1"), TimelineTrackKind::Video)
        .with_clips(TimelineClipVec::from_vec(many));
    let dom = Timeline::create(TimelineTrackVec::from_vec(vec![track]), 1000.0)
        .with_view(100.0, 50.0)
        .with_view_width(500.0)
        .with_theme(UiTheme::Flat)
        .dom();
    let rendered = theme_checks::find_all(&dom, CLIP_CLASS).len();
    assert!(rendered >= 10, "the view's ten clips at least: {rendered}");
    assert!(rendered <= 40, "a window of clips, not the thousand: {rendered}");
    let (from, to) = visible_window(100.0, 500.0, 50.0);
    assert!(from <= 100.0 && to >= 110.0, "the window covers the view: {from}..{to}");
    assert!(to - from < 40.0, "and not much more: {from}..{to}");
}

#[test]
fn a_clip_sits_at_its_time_and_is_as_wide_as_its_duration() {
    let (left, width) = clip_geometry(2.0, 3.0, 0.5, 100.0);
    assert!((left - 150.0).abs() < 1e-3, "left {left}");
    assert!((width - 300.0).abs() < 1e-3, "width {width}");
    let (_, tiny) = clip_geometry(0.0, 0.001, 0.0, 10.0);
    assert!(tiny >= 2.0, "a clip never vanishes: {tiny}");
    // A clip that starts before the view starts left of the lane.
    let (left, _) = clip_geometry(1.0, 3.0, 2.0, 100.0);
    assert!((left + 100.0).abs() < 1e-3, "left {left}");
}

#[test]
fn the_scroll_bar_thumb_covers_the_view() {
    let (left, width) = thumb_span(2.0, 4.0, 20.0);
    assert!((left - 0.1).abs() < 1e-6 && (width - 0.2).abs() < 1e-6, "{left} {width}");
    let (left, width) = thumb_span(0.0, 50.0, 20.0);
    assert!(left.abs() < 1e-6 && (width - 1.0).abs() < 1e-6, "a view wider than the sequence");
}

// ---- time ----

#[test]
fn a_timecode_reads_hours_minutes_seconds_and_frames() {
    assert_eq!(Timeline::format_timecode(0.0, 25.0).as_str(), "00:00:00:00");
    assert_eq!(Timeline::format_timecode(3723.5, 25.0).as_str(), "01:02:03:12");
    assert_eq!(Timeline::format_timecode(1.0 / 30.0, 30.0).as_str(), "00:00:00:01");
    assert_eq!(Timeline::format_timecode(-4.0, 25.0).as_str(), "00:00:00:00");
    assert_eq!(Timeline::format_timecode(59.999, 25.0).as_str(), "00:00:59:24", "not 60:25");
}

#[test]
fn the_ruler_ticks_are_far_enough_apart_to_label_and_no_further() {
    for pps in [2.0f32, 10.0, 37.0, 100.0, 400.0, 2500.0] {
        let step = tick_step(pps, 25.0);
        assert!(
            step * f64::from(pps) >= f64::from(MIN_TICK_PX),
            "{pps} px/s: a step of {step} s is too tight"
        );
        assert!(
            step * f64::from(pps) < f64::from(MIN_TICK_PX) * 6.0,
            "{pps} px/s: a step of {step} s is needlessly wide"
        );
    }
    let fine = tick_step(2500.0, 25.0);
    assert!(close((fine * 25.0).round(), fine * 25.0), "fine steps are whole frames: {fine}");
}

#[test]
fn a_time_snaps_to_the_nearest_edge_within_reach() {
    let points = [0.0, 2.0, 5.0];
    // 100 px a second: 2.05 s is 5 px from 2 s.
    assert!(close(snap_time(2.05, &points, 100.0), 2.0));
    assert!(close(snap_time(4.97, &points, 100.0), 5.0));
    // 3.5 s is far from every edge.
    assert!(close(snap_time(3.5, &points, 100.0), 3.5));
    // Zoomed out, the same 0.3 s is within reach.
    assert!(close(snap_time(2.3, &points, 20.0), 2.0));
}

#[test]
fn the_snap_points_are_the_start_the_playhead_and_every_clip_edge_but_the_dragged_ones() {
    let t = tracks();
    let points = snap_points(t.as_ref(), 2.5, 12);
    for want in [0.0, 2.5, 2.0, 1.0, 4.0] {
        assert!(points.iter().any(|p| close(*p, want)), "{want} in {points:?}");
    }
    assert!(
        !points.iter().any(|p| close(*p, 5.0)),
        "the dragged clip's own end is not a target: {points:?}"
    );
}

#[test]
fn a_drag_moves_or_trims_and_never_turns_a_clip_inside_out() {
    let min = 1.0 / 25.0;
    let (start, duration) = drag_result(TimelineDragMode::Move, 2.0, 3.0, 1.5, min);
    assert!(close(start, 3.5) && close(duration, 3.0));
    let (start, _) = drag_result(TimelineDragMode::Move, 2.0, 3.0, -9.0, min);
    assert!(close(start, 0.0), "a clip stops at the start of the sequence");
    let (start, duration) = drag_result(TimelineDragMode::TrimStart, 2.0, 3.0, 1.0, min);
    assert!(close(start, 3.0) && close(duration, 2.0), "the end stays");
    let (start, duration) = drag_result(TimelineDragMode::TrimStart, 2.0, 3.0, 9.0, min);
    assert!(close(start + duration, 5.0) && close(duration, min), "the start stops short of the end");
    let (start, duration) = drag_result(TimelineDragMode::TrimEnd, 2.0, 3.0, -1.0, min);
    assert!(close(start, 2.0) && close(duration, 2.0));
    let (_, duration) = drag_result(TimelineDragMode::TrimEnd, 2.0, 3.0, -9.0, min);
    assert!(close(duration, min));
}

// ---- keyboard ----

#[test]
fn the_lanes_are_one_tab_stop_named_by_the_timeline_and_valued_by_the_playhead() {
    let dom = timeline(&no_log()).with_theme(UiTheme::Flat).dom();
    let stops: Vec<_> = theme_checks::focusable(&dom)
        .into_iter()
        .filter(|(_, n)| theme_checks::has_class(n, LANES_CLASS))
        .collect();
    assert_eq!(stops.len(), 1);
    let info = stops[0].1.root.get_accessibility_info().expect("a role");
    assert_eq!(info.role, AccessibilityRole::Slider);
    assert_eq!(info.accessibility_name.as_ref().map(|n| n.as_str()), Some("Timeline"));
    assert_eq!(
        info.accessibility_value.as_ref().map(|n| n.as_str()),
        Some("00:00:02:10")
    );
}

#[test]
fn arrows_step_the_playhead_by_a_frame_and_shift_by_a_second_home_and_end_go_to_the_ends() {
    let log = no_log();
    let styled = StyledDom::create_from_dom(timeline(&log).with_theme(UiTheme::Flat).dom());
    let at = lanes(&styled);
    let seek = |key: K, held: &[K]| {
        let (_, changes) = rv::press(&styled, at, key, held).expect("a key handler");
        assert!(rv::prevented(&changes), "{key:?} is the timeline's");
        let e = last(&log);
        assert_eq!(e.kind, TimelineEventKind::Seek, "{key:?}");
        e.time
    };
    assert!(close(seek(K::Right, &[]), 2.44), "a frame on at 25 fps");
    assert!(close(seek(K::Left, &[]), 2.36));
    assert!(close(seek(K::Right, &[K::LShift]), 3.4), "a second on");
    assert!(close(seek(K::Left, &[K::LShift]), 1.4));
    assert!(close(seek(K::Home, &[]), 0.0));
    assert!(close(seek(K::End, &[]), 10.0));
}

#[test]
fn up_and_down_jump_to_the_previous_and_next_edit_point() {
    let log = no_log();
    let styled = StyledDom::create_from_dom(timeline(&log).with_theme(UiTheme::Flat).dom());
    let at = lanes(&styled);
    rv::press(&styled, at, K::Down, &[]).expect("a key handler");
    assert!(close(last(&log).time, 4.0), "from 2.4 s the next edit is the voice's end");
    rv::press(&styled, at, K::Up, &[]).expect("a key handler");
    assert!(close(last(&log).time, 2.0), "the previous one is the cut between pier and market");
    assert_eq!(edit_points(tracks().as_ref(), 10.0), vec![0.0, 1.0, 2.0, 4.0, 5.0, 10.0]);
}

#[test]
fn plus_and_minus_zoom_around_the_playhead() {
    let log = no_log();
    // The playhead at 10 s, the view from 5 s at 100 px a second: 500 px in.
    let tl = Timeline::create(tracks(), 60.0)
        .with_playhead(10.0)
        .with_view(5.0, 100.0)
        .with_view_width(1000.0)
        .with_fps(25.0)
        .with_on_event(RefAny::new(log.clone()), record as TimelineOnEventCallbackType)
        .with_theme(UiTheme::Flat);
    let styled = StyledDom::create_from_dom(tl.dom());
    let at = lanes(&styled);
    rv::press(&styled, at, K::Equals, &[]).expect("a key handler");
    let e = last(&log);
    assert_eq!(e.kind, TimelineEventKind::Zoom);
    assert!((e.value - 125.0).abs() < 1e-3, "zoomed in by a quarter: {}", e.value);
    assert!(close(e.time, 6.0), "the playhead stays 500 px in: view from {}", e.time);
    rv::press(&styled, at, K::Minus, &[]).expect("a key handler");
    let e = last(&log);
    assert!((e.value - 80.0).abs() < 1e-3, "{}", e.value);
    assert!(close(e.time, 3.75), "{}", e.time);
    rv::press(&styled, at, K::Backslash, &[]).expect("a key handler");
    assert_eq!(last(&log).kind, TimelineEventKind::ZoomToFit);
}

#[test]
fn delete_reports_a_delete_and_shift_makes_it_a_ripple_delete() {
    let log = no_log();
    let styled = StyledDom::create_from_dom(timeline(&log).with_theme(UiTheme::Flat).dom());
    let at = lanes(&styled);
    rv::press(&styled, at, K::Delete, &[]).expect("a key handler");
    let e = last(&log);
    assert_eq!((e.kind, e.shift), (TimelineEventKind::Delete, false));
    rv::press(&styled, at, K::Back, &[K::LShift]).expect("a key handler");
    let e = last(&log);
    assert_eq!((e.kind, e.shift), (TimelineEventKind::Delete, true));
}

#[test]
fn letters_and_space_are_left_to_the_app() {
    let log = no_log();
    let styled = StyledDom::create_from_dom(timeline(&log).with_theme(UiTheme::Flat).dom());
    let at = lanes(&styled);
    for key in [K::C, K::V, K::Space, K::J, K::L, K::I, K::O] {
        let (_, changes) = rv::press(&styled, at, key, &[]).expect("a key handler");
        assert!(!rv::prevented(&changes), "{key:?} belongs to the app's shortcuts");
    }
    assert!(events(&log).is_empty(), "{:?}", events(&log));
}

// ---- clips and tracks ----

#[test]
fn a_selected_clip_carries_its_class_and_state_and_every_clip_is_named() {
    let dom = timeline(&no_log()).with_theme(UiTheme::Flat).dom();
    let clips = theme_checks::find_all(&dom, CLIP_CLASS);
    let selected: Vec<_> = clips
        .iter()
        .filter(|c| theme_checks::has_class(c, CLIP_SELECTED_CLASS))
        .collect();
    assert_eq!(selected.len(), 1);
    let info = selected[0].root.get_accessibility_info().expect("a role");
    assert!(info.states.as_ref().contains(&AccessibilityState::Selected));
    for c in &clips {
        let name = c
            .root
            .get_accessibility_info()
            .and_then(|i| i.accessibility_name.as_ref().map(|n| n.as_str().to_string()))
            .unwrap_or_default();
        assert!(!name.is_empty(), "a clip is named");
    }
    let mut texts = Vec::new();
    for c in &clips {
        for (_, n) in theme_checks::nodes(c) {
            if let azul_core::dom::NodeType::Text(s) = n.root.get_node_type() {
                texts.push(s.as_ref().as_str().to_string());
            }
        }
    }
    for label in ["pier", "market", "voice"] {
        assert!(texts.iter().any(|t| t == label), "{label} in {texts:?}");
    }
}

#[test]
fn the_track_headers_toggle_mute_and_lock_for_their_track() {
    let log = no_log();
    let styled = StyledDom::create_from_dom(timeline(&log).with_theme(UiTheme::Flat).dom());
    let mutes = nodes_with(&styled, MUTE_CLASS);
    let locks = nodes_with(&styled, LOCK_CLASS);
    assert_eq!((mutes.len(), locks.len()), (3, 3), "a toggle pair per track");
    rv::fire(&styled, id(mutes[2]), EventFilter::Hover(HoverEventFilter::Click)).expect("a click");
    let e = last(&log);
    assert_eq!((e.kind, e.track), (TimelineEventKind::ToggleMute, 2));
    rv::fire(&styled, id(locks[0]), EventFilter::Hover(HoverEventFilter::Click)).expect("a click");
    let e = last(&log);
    assert_eq!((e.kind, e.track), (TimelineEventKind::ToggleLock, 0));
}

#[test]
fn a_press_on_a_clip_selects_it_and_a_double_click_opens_it() {
    let log = no_log();
    let styled = StyledDom::create_from_dom(timeline(&log).with_theme(UiTheme::Flat).dom());
    let clips = nodes_with(&styled, CLIP_CLASS);
    assert_eq!(clips.len(), 3);
    // Document order: V1's pier, market; A1's voice.
    rv::fire(&styled, id(clips[1]), EventFilter::Hover(HoverEventFilter::MouseDown))
        .expect("a press handler");
    let e = last(&log);
    assert_eq!((e.kind, e.clip_id, e.track), (TimelineEventKind::Select, 12, 0));
    rv::fire(&styled, id(clips[2]), EventFilter::Hover(HoverEventFilter::DoubleClick))
        .expect("a double-click handler");
    let e = last(&log);
    assert_eq!((e.kind, e.clip_id, e.track), (TimelineEventKind::Open, 31, 2));
}

#[test]
fn a_press_on_a_locked_track_selects_nothing() {
    let log = no_log();
    let mut t = tracks().into_library_owned_vec();
    t[0].locked = true;
    let tl = Timeline::create(TimelineTrackVec::from_vec(t), 10.0)
        .with_on_event(RefAny::new(log.clone()), record as TimelineOnEventCallbackType)
        .with_theme(UiTheme::Flat);
    let styled = StyledDom::create_from_dom(tl.dom());
    let clips = nodes_with(&styled, CLIP_CLASS);
    rv::fire(&styled, id(clips[0]), EventFilter::Hover(HoverEventFilter::MouseDown))
        .expect("a press handler");
    assert!(events(&log).is_empty(), "{:?}", events(&log));
}

// ---- themes ----

#[test]
fn a_timeline_follows_the_app_theme_keeps_its_invariants_and_declares_its_structure_once() {
    checks::assert_follows_the_app_theme(
        "timeline",
        || timeline(&no_log()).dom(),
        |t: UiTheme| timeline(&no_log()).with_theme(t).dom(),
    );
    for theme in checks::BOTH {
        theme_checks::assert_theme_invariants(
            &format!("timeline pinned to {}", theme.name()),
            &timeline(&no_log()).with_theme(theme).dom(),
        );
        let dom = checks::under(theme, || timeline(&no_log()).dom());
        theme_checks::assert_structure_is_shared(
            &format!("timeline built for {}", theme.name()),
            &dom,
            &[],
        );
    }
}

#[test]
fn every_clip_tint_paints_its_own_block() {
    let face = |tint: TimelineClipTint| {
        let track = TimelineTrack::create(1, AzString::from("V1"), TimelineTrackKind::Video)
            .with_clips(TimelineClipVec::from_vec(vec![clip(1, 0.0, 1.0, "x").with_tint(tint)]));
        let dom = Timeline::create(TimelineTrackVec::from_vec(vec![track]), 2.0)
            .with_theme(UiTheme::Flat)
            .dom();
        let c = theme_checks::find(&dom, CLIP_CLASS).expect("the clip");
        format!("{:?}", theme_checks::background(c, false))
    };
    let tints = [
        TimelineClipTint::Video,
        TimelineClipTint::Audio,
        TimelineClipTint::Title,
        TimelineClipTint::Accent,
        TimelineClipTint::Muted,
    ];
    for (i, a) in tints.iter().enumerate() {
        for b in &tints[i + 1..] {
            assert_ne!(face(*a), face(*b), "{a:?} and {b:?} look alike");
        }
    }
}
