//! Timeline widget - tracks of clips under a time ruler, with a playhead:
//! a video editor's timeline (Premiere's V1..Vn / A1..An), a calendar's day
//! lanes, a slide show's animation pane.
//!
//! ```text
//! ┌ 00:00:02:10 ┬ 00:00 ──── 00:01 ──── 00:02 ─▼── 00:03 ──── ┐  the ruler row
//! │ V1  👁 🔒   │ [ pier     ][ market            ]│           │
//! │ V2  👁 🔒   │                                  │           │  the lanes
//! │ A1  🔊 🔒   │        [ voice           ]       │           │
//! ├─────────────┴──[======]─────────────────────────────────────┤  the scroll bar
//! ```
//!
//! THE APP OWNS EVERYTHING: the tracks and their clips (with every clip's
//! selection), the playhead, the view (`view_start` and the zoom,
//! `pixels_per_second`), the snapping switch. The widget reports what the
//! user asked for through ONE hook, `on_event` ([`TimelineEvent`]): seek the
//! playhead, scroll or zoom the view, select / open / move / trim a clip,
//! click an empty lane, delete, toggle a track's mute or lock - and the app
//! changes its model and rebuilds.
//!
//! THOUSANDS OF CLIPS: the app hands over every clip (data is cheap); the
//! widget renders only the clips that intersect the view
//! ([`visible_window`], from `view_start` over `view_width` pixels) and only
//! the ruler ticks in view. Scrolling is the scroll bar under the lanes (a
//! press or drag reports `Scroll` with the new `view_start`) and the keys;
//! the widget never takes the wheel (`widgets::wheel_ownership`): an app that
//! wants wheel-scrolling or Ctrl+wheel zoom listens on its own container and
//! reports through the same model.
//!
//! POINTER: a press or drag on the ruler seeks (scrubs); a press on a clip
//! selects it (Shift / Ctrl reported) and a drag moves it - to another track
//! too - or, near its left / right edge, trims it; the clip follows the
//! pointer live and snaps to the playhead, the sequence's start and every
//! other clip edge within [`SNAP_PX`] when `snapping` is on; the release
//! reports ONE `Move` or `Trim`. A double-click opens a clip (a source
//! monitor). A locked track's clips do not take presses.
//!
//! KEYBOARD (the lanes are the one Tab stop, a slider named "Timeline"
//! valued by the playhead's timecode): Left / Right step the playhead a
//! frame (Shift: a second), Home / End go to the ends, Up / Down to the
//! previous / next edit point, `+` / `-` zoom about the playhead, `\` zooms
//! to fit, Delete / Backspace delete (Shift: ripple delete). Letters and
//! Space are left to the app (its tools, J / K / L, I / O).
//!
//! Key types: [`Timeline`], [`TimelineTrack`], [`TimelineClip`],
//! [`TimelineEvent`].

use alloc::{format, string::String, vec::Vec};

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole, AccessibilityState, AccessibilityStateVec},
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, DomNodeId, DomVec, EventFilter, HoverEventFilter, TabIndex},
    events::FocusEventFilter,
    refany::{OptionRefAny, RefAny},
    resources::{ImageRef, OptionImageRef},
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    props::{
        basic::{length::FloatValue, pixel::PixelValue},
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow, LayoutFlexShrink,
            LayoutHeight, LayoutLeft, LayoutMinHeight, LayoutMinWidth, LayoutOverflow,
            LayoutPosition, LayoutTop, LayoutWidth,
        },
        property::{CssProperty, StyleWhiteSpaceValue},
        style::{StyleUserSelect, StyleWhiteSpace},
    },
    AzString,
};

use crate::{
    callbacks::CallbackInfo,
    widgets::{
        button::{Button, ButtonOnClickCallbackType, ButtonType},
        themes::{
            decl::{
                display_flex, flex_direction, grow, no_shrink, nowrap, overflow_x_hidden,
                overflow_y_hidden, position, px_bottom, px_height, px_left, px_top, px_width,
            },
            OptionUiTheme, UiTheme,
        },
    },
};

// ---- classes ----

/// The widget's root.
pub const TIMELINE_CLASS: &str = "__azul-native-timeline";
/// The ruler row: the corner over the headers, then the ruler.
pub const HEAD_CLASS: &str = "__azul-native-timeline-head";
/// The corner over the track headers (the playhead's timecode).
pub const CORNER_CLASS: &str = "__azul-native-timeline-corner";
/// The time ruler.
pub const RULER_CLASS: &str = "__azul-native-timeline-ruler";
/// A ruler tick (a labelled major one or a minor one).
pub const TICK_CLASS: &str = "__azul-native-timeline-tick";
/// A major tick's label.
pub const TICK_LABEL_CLASS: &str = "__azul-native-timeline-tick-label";
/// The playhead's head on the ruler.
pub const RULER_HEAD_CLASS: &str = "__azul-native-timeline-ruler-head";
/// The row of the track headers and the lanes.
pub const BODY_CLASS: &str = "__azul-native-timeline-body";
/// The column of track headers.
pub const HEADERS_CLASS: &str = "__azul-native-timeline-headers";
/// One track's header.
pub const HEADER_CLASS: &str = "__azul-native-timeline-track-header";
/// A track header's name.
pub const TRACK_NAME_CLASS: &str = "__azul-native-timeline-track-name";
/// A track header's mute (audio) / show (video) toggle.
pub const MUTE_CLASS: &str = "__azul-native-timeline-mute";
/// A track header's lock toggle.
pub const LOCK_CLASS: &str = "__azul-native-timeline-lock";
/// The lanes: the keyboard stop, holding a lane per track and the playhead.
pub const LANES_CLASS: &str = "__azul-native-timeline-lanes";
/// One track's lane.
pub const LANE_CLASS: &str = "__azul-native-timeline-lane";
/// A clip.
pub const CLIP_CLASS: &str = "__azul-native-timeline-clip";
/// A selected clip.
pub const CLIP_SELECTED_CLASS: &str = "__azul-native-timeline-clip-selected";
/// A disabled clip (drawn dimmed).
pub const CLIP_DISABLED_CLASS: &str = "__azul-native-timeline-clip-disabled";
/// A clip's thumbnail.
pub const CLIP_THUMB_CLASS: &str = "__azul-native-timeline-clip-thumbnail";
/// A clip's label.
pub const CLIP_LABEL_CLASS: &str = "__azul-native-timeline-clip-label";
/// A clip's detail line.
pub const CLIP_DETAIL_CLASS: &str = "__azul-native-timeline-clip-detail";
/// The playhead line over the lanes.
pub const PLAYHEAD_CLASS: &str = "__azul-native-timeline-playhead";
/// The scroll bar under the lanes.
pub const SCROLL_CLASS: &str = "__azul-native-timeline-scroll";
/// The scroll bar's track.
pub const SCROLL_TRACK_CLASS: &str = "__azul-native-timeline-scroll-track";
/// The scroll bar's thumb: the view's share of the sequence.
pub const THUMB_CLASS: &str = "__azul-native-timeline-thumb";

// ---- metrics ----

/// The least distance between two labelled ruler ticks, in px.
pub const MIN_TICK_PX: f32 = 64.0;
/// How near (in px) a dragged edge must come to a snap point to snap.
pub const SNAP_PX: f32 = 8.0;
/// How near (in px) to a clip's edge a press trims instead of moving.
pub const EDGE_PX: f32 = 6.0;
/// The zoom step of `+` / `-`.
pub const ZOOM_STEP: f32 = 1.25;
/// The zoom limits, px per second.
pub const MIN_PPS: f32 = 0.05;
/// See [`MIN_PPS`].
pub const MAX_PPS: f32 = 4000.0;
/// A track's height when the app sets none.
pub const DEFAULT_TRACK_HEIGHT: f32 = 44.0;
/// The ruler row's height.
pub const RULER_HEIGHT: f32 = 26.0;
/// The scroll bar's height.
pub const SCROLL_HEIGHT: f32 = 12.0;
/// The track headers' width when the app sets none.
pub const DEFAULT_HEADER_WIDTH: f32 = 112.0;

// ---- data ----

/// What a track holds: picture, sound, or anything else (a calendar's
/// lane). Decides the header's first toggle: "Show / Hide" for video,
/// "Mute / Unmute" for the rest.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum TimelineTrackKind {
    /// A video track (V1, V2, ...).
    #[default]
    Video,
    /// An audio track (A1, A2, ...).
    Audio,
    /// Any other lane.
    Generic,
}

/// A clip's colour family, painted by the theme in both modes (Premiere's
/// label colours, kept to what the themes can pair).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum TimelineClipTint {
    /// A video clip.
    #[default]
    Video,
    /// An audio clip.
    Audio,
    /// A title / generated clip.
    Title,
    /// The accent (a nested sequence, a highlighted span).
    Accent,
    /// A quiet clip (a gap filler, a placeholder).
    Muted,
}

/// One clip: a span of a track.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineClip {
    /// Where the clip starts on the timeline, in seconds.
    pub start: f64,
    /// How long it is, in seconds.
    pub duration: f64,
    /// The app's id, reported back with every action on the clip.
    pub id: u64,
    /// The clip's name ("pier.mp4").
    pub label: AzString,
    /// A second line ("00:00:03:12", "Cross dissolve"), or empty for none.
    pub detail: AzString,
    /// A picture at the clip's head (a video frame), or none.
    pub thumbnail: OptionImageRef,
    /// The clip's colour family.
    pub tint: TimelineClipTint,
    /// Drawn and announced as selected.
    pub selected: bool,
    /// Disabled (it does not play): drawn dimmed.
    pub disabled: bool,
}

impl TimelineClip {
    /// Clip `id` named `label` from `start` for `duration` seconds.
    #[must_use]
    pub fn create(id: u64, start: f64, duration: f64, label: AzString) -> Self {
        Self {
            start,
            duration,
            id,
            label,
            detail: AzString::from_const_str(""),
            thumbnail: OptionImageRef::None,
            tint: TimelineClipTint::Video,
            selected: false,
            disabled: false,
        }
    }

    /// The second line.
    #[must_use]
    pub fn with_detail(mut self, detail: AzString) -> Self {
        self.detail = detail;
        self
    }

    /// The picture at the clip's head.
    #[must_use]
    pub fn with_thumbnail(mut self, thumbnail: ImageRef) -> Self {
        self.thumbnail = OptionImageRef::Some(thumbnail);
        self
    }

    /// The colour family.
    #[must_use]
    pub const fn with_tint(mut self, tint: TimelineClipTint) -> Self {
        self.tint = tint;
        self
    }

    /// Selected.
    #[must_use]
    pub const fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Disabled.
    #[must_use]
    pub const fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Where the clip ends, in seconds.
    #[must_use]
    pub fn end(&self) -> f64 {
        self.start + self.duration
    }
}

impl_option!(
    TimelineClip,
    OptionTimelineClip,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    TimelineClip,
    TimelineClipVec,
    TimelineClipVecDestructor,
    TimelineClipVecDestructorType,
    TimelineClipVecSlice,
    OptionTimelineClip
);
impl_vec_clone!(TimelineClip, TimelineClipVec, TimelineClipVecDestructor);
impl_vec_debug!(TimelineClip, TimelineClipVec);
impl_vec_mut!(TimelineClip, TimelineClipVec);

azul_css::impl_vec_partialeq!(TimelineClip, TimelineClipVec);

/// One track: a header and a lane of clips.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineTrack {
    /// The app's id.
    pub id: u64,
    /// The clips, in any order (they should not overlap).
    pub clips: TimelineClipVec,
    /// The header's name ("V1", "A2").
    pub name: AzString,
    /// The lane's height in px (0: [`DEFAULT_TRACK_HEIGHT`]).
    pub height: f32,
    /// Picture, sound or other.
    pub kind: TimelineTrackKind,
    /// Muted (audio) or hidden (video): the header's first toggle is on.
    pub muted: bool,
    /// Locked: its clips take no presses, the header's lock is on.
    pub locked: bool,
}

impl TimelineTrack {
    /// An empty track `id` named `name`.
    #[must_use]
    pub fn create(id: u64, name: AzString, kind: TimelineTrackKind) -> Self {
        Self {
            id,
            clips: TimelineClipVec::from_const_slice(&[]),
            name,
            height: 0.0,
            kind,
            muted: false,
            locked: false,
        }
    }

    /// The clips.
    #[must_use]
    pub fn with_clips(mut self, clips: TimelineClipVec) -> Self {
        self.clips = clips;
        self
    }

    /// Adds a clip.
    pub fn add_clip(&mut self, clip: TimelineClip) {
        let mut clips = core::mem::replace(&mut self.clips, TimelineClipVec::from_const_slice(&[]))
            .into_library_owned_vec();
        clips.push(clip);
        self.clips = TimelineClipVec::from_vec(clips);
    }

    /// [`Self::add_clip`] for the builder chain.
    #[must_use]
    pub fn with_clip(mut self, clip: TimelineClip) -> Self {
        self.add_clip(clip);
        self
    }

    /// The lane's height in px.
    #[must_use]
    pub const fn with_height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// Muted / hidden.
    #[must_use]
    pub const fn with_muted(mut self, muted: bool) -> Self {
        self.muted = muted;
        self
    }

    /// Locked.
    #[must_use]
    pub const fn with_locked(mut self, locked: bool) -> Self {
        self.locked = locked;
        self
    }

    /// The lane's height in px, the default when unset.
    #[must_use]
    pub fn lane_height(&self) -> f32 {
        if self.height > 0.0 {
            self.height
        } else {
            DEFAULT_TRACK_HEIGHT
        }
    }
}

impl_option!(
    TimelineTrack,
    OptionTimelineTrack,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    TimelineTrack,
    TimelineTrackVec,
    TimelineTrackVecDestructor,
    TimelineTrackVecDestructorType,
    TimelineTrackVecSlice,
    OptionTimelineTrack
);
impl_vec_clone!(TimelineTrack, TimelineTrackVec, TimelineTrackVecDestructor);
impl_vec_debug!(TimelineTrack, TimelineTrackVec);
impl_vec_mut!(TimelineTrack, TimelineTrackVec);

azul_css::impl_vec_partialeq!(TimelineTrack, TimelineTrackVec);

// ---- events ----

/// What the user asked the timeline for.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum TimelineEventKind {
    /// Move the playhead to `time` (the ruler, the keys).
    #[default]
    Seek,
    /// Scroll the view: `time` is the new `view_start`.
    Scroll,
    /// Zoom: `value` is the new px per second, `time` the new `view_start`
    /// that keeps the anchor (the playhead) where it is on screen.
    Zoom,
    /// Zoom so the whole sequence fits the view (the app knows its width).
    ZoomToFit,
    /// A clip was pressed: `clip_id` on `track`, `time` where (the
    /// playhead when unknown), `shift` / `ctrl` held.
    Select,
    /// A clip was double-clicked: open it (`clip_id`, `track`).
    Open,
    /// A clip was dragged: `clip_id` from `track` (its index) to the track
    /// with index `value`, starting at `time`.
    Move,
    /// A clip's `edge` was dragged to `time`.
    Trim,
    /// An empty part of lane `track` was pressed at `time`.
    LaneClick,
    /// Delete / Backspace on the timeline; `shift`: ripple delete.
    Delete,
    /// Track `track`'s mute (audio) / show (video) toggle was clicked.
    ToggleMute,
    /// Track `track`'s lock toggle was clicked.
    ToggleLock,
}

/// Which edge of a clip a trim moved.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum TimelineEdge {
    /// The in point (the clip's start).
    #[default]
    Start,
    /// The out point (the clip's end).
    End,
}

/// One request of the user, for the app's `on_event`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimelineEvent {
    /// A time in seconds (see [`TimelineEventKind`]).
    pub time: f64,
    /// A second value: the zoom's px per second, a move's target track.
    pub value: f64,
    /// The clip, or 0.
    pub clip_id: u64,
    /// The track's index, or 0.
    pub track: usize,
    /// What was asked.
    pub kind: TimelineEventKind,
    /// The edge a trim moved.
    pub edge: TimelineEdge,
    /// Shift was held.
    pub shift: bool,
    /// The primary modifier was held: Cmd on macOS, Ctrl elsewhere.
    pub ctrl: bool,
}

impl TimelineEvent {
    /// A `kind` event at `time`, nothing else set.
    #[must_use]
    pub const fn create(kind: TimelineEventKind, time: f64) -> Self {
        Self {
            time,
            value: 0.0,
            clip_id: 0,
            track: 0,
            kind,
            edge: TimelineEdge::Start,
            shift: false,
            ctrl: false,
        }
    }
}

/// Callback invoked for a request of the user.
pub type TimelineOnEventCallbackType = extern "C" fn(RefAny, CallbackInfo, TimelineEvent) -> Update;
impl_widget_callback!(
    TimelineOnEvent,
    OptionTimelineOnEvent,
    TimelineOnEventCallback,
    TimelineOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        TimelineOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: TIMELINE_ON_EVENT_INVOKER,
    invoker_ty:     AzTimelineOnEventCallbackInvoker,
    thunk_fn:       az_timeline_on_event_callback_thunk,
    setter_fn:      AzApp_setTimelineOnEventCallbackInvoker,
    from_handle_fn: AzTimelineOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzTimelineOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: TimelineEvent ],
}

// ---- the widget ----

/// The timeline: a ruler row over the track headers and the lanes, a
/// scroll bar under them.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct Timeline {
    /// The sequence's length in seconds.
    pub duration: f64,
    /// The playhead, in seconds.
    pub playhead: f64,
    /// The first second in view.
    pub view_start: f64,
    /// The tracks, top to bottom.
    pub tracks: TimelineTrackVec,
    /// The user's requests.
    pub on_event: OptionTimelineOnEvent,
    /// The lanes' accessible name ("Timeline").
    pub accessibility_name: AzString,
    /// The zoom: px per second.
    pub pixels_per_second: f32,
    /// Frames per second: the keys' frame step and the timecodes.
    pub fps: f32,
    /// How wide the lanes are on screen, in px - the app's estimate (its
    /// window's width will do): the widget renders the clips and ticks of
    /// that much time from `view_start`, and a clip past it is cut off
    /// where the lanes end anyway.
    pub view_width: f32,
    /// The track headers' width in px (0: [`DEFAULT_HEADER_WIDTH`]).
    pub header_width: f32,
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
    /// Dragged edges snap to the playhead and the other clips' edges.
    pub snapping: bool,
}

impl Timeline {
    /// A timeline of `tracks` over a sequence `duration` seconds long, the
    /// playhead and the view at 0, 50 px a second, 25 fps, snapping on.
    #[must_use]
    pub fn create(tracks: TimelineTrackVec, duration: f64) -> Self {
        Self {
            duration,
            playhead: 0.0,
            view_start: 0.0,
            tracks,
            on_event: None.into(),
            accessibility_name: AzString::from_const_str("Timeline"),
            pixels_per_second: 50.0,
            fps: 25.0,
            view_width: 1200.0,
            header_width: 0.0,
            theme: OptionUiTheme::None,
            snapping: true,
        }
    }

    /// The playhead, in seconds.
    pub fn set_playhead(&mut self, playhead: f64) {
        self.playhead = playhead;
    }

    /// [`Self::set_playhead`] for the builder chain.
    #[must_use]
    pub fn with_playhead(mut self, playhead: f64) -> Self {
        self.set_playhead(playhead);
        self
    }

    /// The view: the first second in view and the zoom (px per second).
    pub fn set_view(&mut self, view_start: f64, pixels_per_second: f32) {
        self.view_start = view_start;
        self.pixels_per_second = pixels_per_second;
    }

    /// [`Self::set_view`] for the builder chain.
    #[must_use]
    pub fn with_view(mut self, view_start: f64, pixels_per_second: f32) -> Self {
        self.set_view(view_start, pixels_per_second);
        self
    }

    /// How wide the lanes are on screen, in px (an estimate is fine).
    pub fn set_view_width(&mut self, view_width: f32) {
        self.view_width = view_width;
    }

    /// [`Self::set_view_width`] for the builder chain.
    #[must_use]
    pub fn with_view_width(mut self, view_width: f32) -> Self {
        self.set_view_width(view_width);
        self
    }

    /// Frames per second.
    pub fn set_fps(&mut self, fps: f32) {
        self.fps = fps;
    }

    /// [`Self::set_fps`] for the builder chain.
    #[must_use]
    pub fn with_fps(mut self, fps: f32) -> Self {
        self.set_fps(fps);
        self
    }

    /// The track headers' width in px.
    pub fn set_header_width(&mut self, header_width: f32) {
        self.header_width = header_width;
    }

    /// [`Self::set_header_width`] for the builder chain.
    #[must_use]
    pub fn with_header_width(mut self, header_width: f32) -> Self {
        self.set_header_width(header_width);
        self
    }

    /// Snapping on or off.
    pub fn set_snapping(&mut self, snapping: bool) {
        self.snapping = snapping;
    }

    /// [`Self::set_snapping`] for the builder chain.
    #[must_use]
    pub fn with_snapping(mut self, snapping: bool) -> Self {
        self.set_snapping(snapping);
        self
    }

    /// The lanes' accessible name.
    pub fn set_accessibility_name(&mut self, name: AzString) {
        self.accessibility_name = name;
    }

    /// [`Self::set_accessibility_name`] for the builder chain.
    #[must_use]
    pub fn with_accessibility_name(mut self, name: AzString) -> Self {
        self.set_accessibility_name(name);
        self
    }

    /// The user's requests.
    pub fn set_on_event<C: Into<TimelineOnEventCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_event = Some(TimelineOnEvent {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<TimelineOnEventCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// Pin the widget theme; unset, the timeline follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty timeline and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(TimelineTrackVec::from_const_slice(&[]), 0.0);
        core::mem::swap(&mut s, self);
        s
    }

    /// `seconds` as a timecode at `fps`: `HH:MM:SS:FF` (negative times read
    /// as zero). The same text the corner and the lanes' value show, for
    /// an app's monitors.
    #[must_use]
    pub fn format_timecode(seconds: f64, fps: f32) -> AzString {
        AzString::from(timecode(seconds, fps))
    }

    /// Where `time` is, in px from the lanes' left edge, in this view.
    #[must_use]
    pub fn x_of(&self, time: f64) -> f32 {
        x_of(time, self.view_start, self.pixels_per_second)
    }

    /// The time at `x` px from the lanes' left edge, in this view.
    #[must_use]
    pub fn time_at(&self, x: f32) -> f64 {
        time_at(x, self.view_start, self.pixels_per_second)
    }

    /// The timeline's DOM. The look comes from the theme module
    /// (`themes::flat::timeline` / `themes::flora::timeline`); `None`
    /// carries both looks, each in its `@theme(<name>)` block, and the app
    /// theme picks.
    #[must_use]
    pub fn dom(self) -> Dom {
        match self.theme.into_option() {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::timeline(self),
            Some(UiTheme::Flat) => crate::widgets::themes::flat::timeline(self),
            None => crate::widgets::themes::theme_blocks::follow_app_theme(
                self,
                crate::widgets::themes::flat::timeline,
                crate::widgets::themes::flora::timeline,
            ),
        }
    }
}

impl Default for Timeline {
    fn default() -> Self {
        Self::create(TimelineTrackVec::from_const_slice(&[]), 0.0)
    }
}

impl From<Timeline> for Dom {
    fn from(t: Timeline) -> Self {
        t.dom()
    }
}

// ==== the time math (pure, unit-tested) ====

/// What a clip drag does: move the clip, or move one of its edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineDragMode {
    /// The whole clip moves (and may change track).
    Move,
    /// The in point moves; the end stays.
    TrimStart,
    /// The out point moves; the start stays.
    TrimEnd,
}

/// `time`, in px from the lanes' left edge, in the view from `view_start`
/// at `pps` px a second.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn x_of(time: f64, view_start: f64, pps: f32) -> f32 {
    ((time - view_start) * f64::from(pps)) as f32
}

/// The time at `x` px from the lanes' left edge.
#[must_use]
pub fn time_at(x: f32, view_start: f64, pps: f32) -> f64 {
    view_start + f64::from(x) / f64::from(pps.max(MIN_PPS))
}

/// The span of time the widget renders for a view from `view_start` over
/// `view_width` px at `pps`: the view and a quarter of it on each side, so
/// a small scroll shows rendered clips before the app rebuilds.
#[must_use]
pub fn visible_window(view_start: f64, view_width: f32, pps: f32) -> (f64, f64) {
    let span = f64::from(view_width.max(1.0)) / f64::from(pps.max(MIN_PPS));
    let margin = span * 0.25;
    (view_start - margin, view_start + span + margin)
}

/// A clip's `(left, width)` in px in the lanes: at its time, as wide as its
/// duration, never narrower than 2 px.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn clip_geometry(start: f64, duration: f64, view_start: f64, pps: f32) -> (f32, f32) {
    let left = x_of(start, view_start, pps);
    let width = ((duration.max(0.0) * f64::from(pps)) as f32).max(2.0);
    (left, width)
}

/// The scroll bar thumb's `(left, width)` as fractions of the bar: the view
/// (`span` seconds from `view_start`) over the sequence (`duration`, or
/// the view's end when that is further).
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn thumb_span(view_start: f64, span: f64, duration: f64) -> (f32, f32) {
    let total = duration.max(view_start + span);
    if total <= 0.0 {
        return (0.0, 1.0);
    }
    let left = (view_start / total).clamp(0.0, 1.0);
    let width = (span / total).clamp(0.0, 1.0 - left);
    (left as f32, width as f32)
}

/// The step between two labelled ruler ticks, in seconds: the finest of
/// whole frames (1, 2, 5, 10 at `fps`) and of round seconds and minutes
/// that leaves [`MIN_TICK_PX`] between two labels at `pps`.
#[must_use]
pub fn tick_step(pps: f32, fps: f32) -> f64 {
    let pps = f64::from(pps.max(MIN_PPS));
    let min = f64::from(MIN_TICK_PX);
    let mut candidates: Vec<f64> = Vec::new();
    if fps > 0.0 {
        for frames in [1.0, 2.0, 5.0, 10.0] {
            candidates.push(frames / f64::from(fps));
        }
    } else {
        candidates.extend_from_slice(&[0.01, 0.02, 0.05, 0.1, 0.2, 0.5]);
    }
    candidates.extend_from_slice(&[
        1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 900.0, 1800.0, 3600.0,
    ]);
    for step in &candidates {
        if step * pps >= min {
            return *step;
        }
    }
    // Hours: as many as it takes.
    let hours = (min / pps / 3600.0).ceil().max(1.0);
    hours * 3600.0
}

/// `seconds` at `fps` as `HH:MM:SS:FF` (the frame count of `fps` rounded
/// to a whole rate; negative times read as zero).
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn timecode(seconds: f64, fps: f32) -> String {
    let rate = f64::from(fps.max(1.0));
    let nominal = (rate.round() as u64).max(1);
    let frames = (seconds.max(0.0) * rate + 1e-6).floor() as u64;
    let ff = frames % nominal;
    let total_seconds = frames / nominal;
    let ss = total_seconds % 60;
    let mm = (total_seconds / 60) % 60;
    let hh = total_seconds / 3600;
    format!("{hh:02}:{mm:02}:{ss:02}:{ff:02}")
}

/// `t` on the nearest frame boundary at `fps` (as it is when `fps` is 0).
#[must_use]
pub fn on_frame(t: f64, fps: f32) -> f64 {
    if fps > 0.0 {
        (t * f64::from(fps)).round() / f64::from(fps)
    } else {
        t
    }
}

/// The frame `frames` frames from the one at `t` (at `fps`), clamped to
/// `0..=duration`.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn step_frames(t: f64, fps: f32, frames: i64, duration: f64) -> f64 {
    let rate = f64::from(if fps > 0.0 { fps } else { 25.0 });
    (((t * rate).round() + frames as f64) / rate).clamp(0.0, duration.max(0.0))
}

/// `t` snapped to the nearest of `points` within [`SNAP_PX`] at `pps`, or
/// `t` itself when none is that near.
#[must_use]
pub fn snap_time(t: f64, points: &[f64], pps: f32) -> f64 {
    let reach = f64::from(SNAP_PX) / f64::from(pps.max(MIN_PPS));
    let mut best: Option<(f64, f64)> = None;
    for p in points {
        let d = (p - t).abs();
        if d <= reach && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, *p));
        }
    }
    best.map_or(t, |(_, p)| p)
}

/// What a dragged edge snaps to: the sequence's start, the playhead and
/// every clip's start and end - but those of the clip being dragged
/// (`skip_clip`).
#[must_use]
pub fn snap_points(tracks: &[TimelineTrack], playhead: f64, skip_clip: u64) -> Vec<f64> {
    let mut points = alloc::vec![0.0, playhead];
    for track in tracks {
        for c in track.clips.as_ref() {
            if c.id != skip_clip {
                points.push(c.start);
                points.push(c.end());
            }
        }
    }
    points
}

/// The edit points Up / Down jump between: the start, the end of the
/// sequence (`duration`) and every clip edge, ascending, each once.
#[must_use]
pub fn edit_points(tracks: &[TimelineTrack], duration: f64) -> Vec<f64> {
    let mut points = alloc::vec![0.0, duration.max(0.0)];
    for track in tracks {
        for c in track.clips.as_ref() {
            points.push(c.start);
            points.push(c.end());
        }
    }
    points.sort_by(f64::total_cmp);
    points.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    points
}

/// A clip `start` + `duration` seconds after a drag of `dt` seconds in
/// `mode`: a move keeps the length and stops at 0; a trim moves one edge and
/// never brings it closer than `min_duration` to the other.
#[must_use]
pub fn drag_result(
    mode: TimelineDragMode,
    start: f64,
    duration: f64,
    dt: f64,
    min_duration: f64,
) -> (f64, f64) {
    let end = start + duration;
    match mode {
        TimelineDragMode::Move => ((start + dt).max(0.0), duration),
        TimelineDragMode::TrimStart => {
            let new_start = (start + dt).clamp(0.0, (end - min_duration).max(0.0));
            (new_start, end - new_start)
        }
        TimelineDragMode::TrimEnd => {
            let new_end = (end + dt).max(start + min_duration);
            (start, new_end - start)
        }
    }
}

// ==== the callbacks ====

/// A bar the pointer is dragging along.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scrub {
    /// The ruler: the playhead follows.
    Ruler,
    /// The scroll bar: the view follows.
    ScrollBar,
}

/// A clip drag in flight.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ClipDrag {
    clip_id: u64,
    /// The clip's track (index).
    track: usize,
    mode: TimelineDragMode,
    /// The clip before the drag.
    start: f64,
    duration: f64,
    /// Where the press was, in window coordinates.
    press_x: f32,
    press_y: f32,
    /// The pointer left the dead zone around the press.
    moved: bool,
    /// The clip after the drag so far.
    now_start: f64,
    now_duration: f64,
    now_track: usize,
}

/// What every part of one timeline shares: the app's hook, the model the
/// keys and drags compute with, and the drag in flight. The lanes' DATASET,
/// so a rebuild in the middle of a drag (the app redraws the selection a
/// press reported) carries the drag over ([`merge_timeline_state`]).
struct TimelineShared {
    on_event: OptionTimelineOnEvent,
    tracks: Vec<TimelineTrack>,
    duration: f64,
    playhead: f64,
    view_start: f64,
    pps: f32,
    fps: f32,
    view_width: f32,
    snapping: bool,
    scrub: Option<Scrub>,
    drag: Option<ClipDrag>,
}

impl TimelineShared {
    /// Seconds in view.
    fn span(&self) -> f64 {
        f64::from(self.view_width.max(1.0)) / f64::from(self.pps.max(MIN_PPS))
    }

    /// The shortest clip a trim leaves: one frame.
    fn min_duration(&self) -> f64 {
        if self.fps > 0.0 {
            1.0 / f64::from(self.fps)
        } else {
            0.04
        }
    }
}

/// A clip's payload: which clip it is, on which track.
struct ClipData {
    shared: RefAny,
    track: usize,
    clip_id: u64,
}

/// A lane's payload: which track.
struct LaneData {
    shared: RefAny,
    track: usize,
}

/// A track toggle's payload: which track, which toggle.
struct ToggleData {
    shared: RefAny,
    track: usize,
    kind: TimelineEventKind,
}

/// Carry a drag across a rebuild: the pointer wins while it is down (the
/// split pane's rule, `merge_split_pane_state`). Everything else is the
/// fresh build's.
#[must_use]
pub extern "C" fn merge_timeline_state(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    {
        let old = old_data.downcast_ref::<TimelineShared>().map(|o| (o.scrub, o.drag));
        if let (Some(mut fresh), Some((scrub, drag))) = (new_data.downcast_mut::<TimelineShared>(), old) {
            if fresh.scrub.is_none() {
                fresh.scrub = scrub;
            }
            if fresh.drag.is_none() {
                fresh.drag = drag;
            }
        }
    }
    new_data
}

/// Hands `event` to the app's hook.
fn fire(hook: &OptionTimelineOnEvent, info: CallbackInfo, event: TimelineEvent) -> Update {
    match hook.as_ref() {
        Some(TimelineOnEvent { refany, callback }) => callback.invoke(refany.clone(), info, event),
        None => Update::DoNothing,
    }
}


/// Moves the keyboard focus to the lanes `node` belongs to (its ancestor
/// carrying [`LANES_CLASS`]), so the keys work after a press.
fn focus_lanes(info: &mut CallbackInfo, node: DomNodeId) {
    let mut at = Some(node);
    for _ in 0..4 {
        let Some(n) = at else { return };
        if crate::widgets::roving::has_class(info, n, LANES_CLASS) {
            info.set_focus(azul_core::callbacks::FocusTarget::Id(n));
            return;
        }
        at = info.get_parent(n);
    }
}

/// The keys on the lanes (see the module's KEYBOARD).
extern "C" fn on_lanes_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    use VirtualKeyCode as K;

    let ks = info.get_current_keyboard_state();
    if ks.alt_down() {
        return Update::DoNothing;
    }
    let Some(key) = ks.current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let shift = ks.shift_down();
    let Some(s) = data.downcast_ref::<TimelineShared>() else {
        return Update::DoNothing;
    };
    #[allow(clippy::cast_possible_truncation)]
    let second = i64::from((s.fps.round() as i32).max(1));
    let step = |frames: i64| step_frames(s.playhead, s.fps, frames, s.duration);
    let zoom = |factor: f32| {
        let pps = (s.pps * factor).clamp(MIN_PPS, MAX_PPS);
        let x = (s.playhead - s.view_start) * f64::from(s.pps);
        let mut e = TimelineEvent::create(
            TimelineEventKind::Zoom,
            (s.playhead - x / f64::from(pps)).max(0.0),
        );
        e.value = f64::from(pps);
        e
    };
    let event = match key {
        K::Left => TimelineEvent::create(TimelineEventKind::Seek, step(if shift { -second } else { -1 })),
        K::Right => TimelineEvent::create(TimelineEventKind::Seek, step(if shift { second } else { 1 })),
        K::Home => TimelineEvent::create(TimelineEventKind::Seek, 0.0),
        K::End => TimelineEvent::create(TimelineEventKind::Seek, s.duration.max(0.0)),
        K::Up | K::Down => {
            let points = edit_points(&s.tracks, s.duration);
            let target = if key == K::Up {
                points.iter().rev().find(|p| **p < s.playhead - 1e-6).copied().unwrap_or(0.0)
            } else {
                points
                    .iter()
                    .find(|p| **p > s.playhead + 1e-6)
                    .copied()
                    .unwrap_or(s.duration.max(0.0))
            };
            TimelineEvent::create(TimelineEventKind::Seek, target)
        }
        K::Equals | K::Plus | K::NumpadAdd => zoom(ZOOM_STEP),
        K::Minus | K::NumpadSubtract => zoom(1.0 / ZOOM_STEP),
        K::Backslash => TimelineEvent::create(TimelineEventKind::ZoomToFit, s.view_start),
        K::Delete | K::Back => {
            let mut e = TimelineEvent::create(TimelineEventKind::Delete, s.playhead);
            e.shift = shift;
            e
        }
        _ => return Update::DoNothing,
    };
    // The widget is controlled: the app moves the playhead and the view
    // and rebuilds; every key computes from the values it was built with.
    let hook = s.on_event.clone();
    drop(s);
    // The key is the timeline's: spatial navigation must not walk out of it.
    info.prevent_default();
    fire(&hook, info, event)
}

/// The time under the pointer in the node the callback runs on, whose
/// left edge is at `view_start` (the ruler, a lane), on a frame and inside
/// the sequence; `None` when the engine has no pointer position.
fn pointer_time(info: &CallbackInfo, s: &TimelineShared) -> Option<f64> {
    let pos = info.get_cursor_relative_to_node().into_option()?;
    Some(on_frame(time_at(pos.x, s.view_start, s.pps), s.fps).clamp(0.0, s.duration.max(0.0)))
}

/// A press on the ruler: the playhead jumps there and follows the pointer
/// until the button is up.
extern "C" fn on_ruler_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<TimelineShared>() else {
        return Update::DoNothing;
    };
    s.scrub = Some(Scrub::Ruler);
    let Some(t) = pointer_time(&info, &s) else {
        return Update::DoNothing;
    };
    s.playhead = t;
    let hook = s.on_event.clone();
    drop(s);
    let node = info.get_hit_node();
    info.capture_pointer(node);
    fire(&hook, info, TimelineEvent::create(TimelineEventKind::Seek, t))
}

/// The pointer moves over (or, captured, away from) the ruler.
extern "C" fn on_ruler_move(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<TimelineShared>() else {
        return Update::DoNothing;
    };
    if s.scrub != Some(Scrub::Ruler) {
        return Update::DoNothing;
    }
    let Some(t) = pointer_time(&info, &s) else {
        return Update::DoNothing;
    };
    if (t - s.playhead).abs() < 1e-9 {
        return Update::DoNothing;
    }
    s.playhead = t;
    let hook = s.on_event.clone();
    drop(s);
    fire(&hook, info, TimelineEvent::create(TimelineEventKind::Seek, t))
}

/// The button is up: a scrub (ruler or scroll bar) ends.
extern "C" fn on_bar_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<TimelineShared>() {
        if s.scrub.take().is_some() {
            drop(s);
            info.release_pointer_capture();
        }
    }
    Update::DoNothing
}

/// The pointer left a bar with the button already up: a release that never
/// arrived ends the scrub.
extern "C" fn on_bar_leave(mut data: RefAny, info: CallbackInfo) -> Update {
    if info.get_current_mouse_state().left_down {
        return Update::DoNothing;
    }
    if let Some(mut s) = data.downcast_mut::<TimelineShared>() {
        s.scrub = None;
    }
    Update::DoNothing
}

/// The view start the scroll bar's pointer asks for: the view centred on
/// the pointer's share of the sequence, kept inside it.
fn scroll_target(info: &CallbackInfo, s: &TimelineShared) -> Option<f64> {
    let pos = info.get_cursor_relative_to_node().into_option()?;
    let width = info.get_hit_node_rect()?.size.width;
    if width <= 0.0 {
        return None;
    }
    let span = s.span();
    let total = s.duration.max(span);
    let at = f64::from((pos.x / width).clamp(0.0, 1.0)) * total;
    Some((at - span / 2.0).clamp(0.0, (total - span).max(0.0)))
}

/// A press on the scroll bar: the view jumps there and follows the pointer.
extern "C" fn on_scroll_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<TimelineShared>() else {
        return Update::DoNothing;
    };
    s.scrub = Some(Scrub::ScrollBar);
    let Some(start) = scroll_target(&info, &s) else {
        return Update::DoNothing;
    };
    s.view_start = start;
    let hook = s.on_event.clone();
    drop(s);
    let node = info.get_hit_node();
    info.capture_pointer(node);
    fire(&hook, info, TimelineEvent::create(TimelineEventKind::Scroll, start))
}

/// The pointer moves over (or, captured, away from) the scroll bar.
extern "C" fn on_scroll_move(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<TimelineShared>() else {
        return Update::DoNothing;
    };
    if s.scrub != Some(Scrub::ScrollBar) {
        return Update::DoNothing;
    }
    let Some(start) = scroll_target(&info, &s) else {
        return Update::DoNothing;
    };
    if (start - s.view_start).abs() < 1e-9 {
        return Update::DoNothing;
    }
    s.view_start = start;
    let hook = s.on_event.clone();
    drop(s);
    fire(&hook, info, TimelineEvent::create(TimelineEventKind::Scroll, start))
}

/// A press on an empty part of a lane.
extern "C" fn on_lane_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (mut shared, track) = {
        let Some(d) = data.downcast_ref::<LaneData>() else {
            return Update::DoNothing;
        };
        (d.shared.clone(), d.track)
    };
    let Some(s) = shared.downcast_ref::<TimelineShared>() else {
        return Update::DoNothing;
    };
    let t = pointer_time(&info, &s).unwrap_or(s.playhead);
    let hook = s.on_event.clone();
    drop(s);
    let node = info.get_hit_node();
    focus_lanes(&mut info, node);
    let ks = info.get_current_keyboard_state();
    let (shift, ctrl) = (ks.shift_down(), ks.primary_down());
    let mut event = TimelineEvent::create(TimelineEventKind::LaneClick, t);
    event.track = track;
    event.shift = shift;
    event.ctrl = ctrl;
    fire(&hook, info, event)
}

/// A press on a clip: select it; a drag from here moves it, or trims it
/// when the press is at an edge. A locked track's clip takes nothing.
extern "C" fn on_clip_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    let (mut shared, track, clip_id) = {
        let Some(d) = data.downcast_ref::<ClipData>() else {
            return Update::DoNothing;
        };
        (d.shared.clone(), d.track, d.clip_id)
    };
    let Some(mut s) = shared.downcast_mut::<TimelineShared>() else {
        return Update::DoNothing;
    };
    let Some(t) = s.tracks.get(track) else {
        return Update::DoNothing;
    };
    if t.locked {
        return Update::DoNothing;
    }
    let Some(clip) = t.clips.as_ref().iter().find(|c| c.id == clip_id).cloned() else {
        return Update::DoNothing;
    };
    let within = info.get_cursor_relative_to_node().into_option();
    let width = info.get_hit_node_rect().map_or(0.0, |r| r.size.width);
    let mode = match within {
        Some(p) if p.x <= EDGE_PX && width > 3.0 * EDGE_PX => TimelineDragMode::TrimStart,
        Some(p) if p.x >= width - EDGE_PX && width > 3.0 * EDGE_PX => TimelineDragMode::TrimEnd,
        _ => TimelineDragMode::Move,
    };
    let time = within.map_or(s.playhead, |p| {
        on_frame(clip.start + f64::from(p.x) / f64::from(s.pps.max(MIN_PPS)), s.fps)
    });
    if let Some(press) = info.get_cursor_relative_to_viewport().into_option() {
        s.drag = Some(ClipDrag {
            clip_id,
            track,
            mode,
            start: clip.start,
            duration: clip.duration,
            press_x: press.x,
            press_y: press.y,
            moved: false,
            now_start: clip.start,
            now_duration: clip.duration,
            now_track: track,
        });
    }
    let hook = s.on_event.clone();
    drop(s);
    let node = info.get_hit_node();
    info.capture_pointer(node);
    focus_lanes(&mut info, node);
    let ks = info.get_current_keyboard_state();
    let (shift, ctrl) = (ks.shift_down(), ks.primary_down());
    let mut event = TimelineEvent::create(TimelineEventKind::Select, time);
    event.clip_id = clip_id;
    event.track = track;
    event.shift = shift;
    event.ctrl = ctrl;
    fire(&hook, info, event)
}

/// The pointer moves while a clip is pressed: the clip follows it live
/// (snapped), the app hears of it on the release.
extern "C" fn on_clip_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (mut shared, clip_id) = {
        let Some(d) = data.downcast_ref::<ClipData>() else {
            return Update::DoNothing;
        };
        (d.shared.clone(), d.clip_id)
    };
    let Some(mut s) = shared.downcast_mut::<TimelineShared>() else {
        return Update::DoNothing;
    };
    let Some(mut drag) = s.drag.filter(|d| d.clip_id == clip_id) else {
        return Update::DoNothing;
    };
    let Some(at) = info.get_cursor_relative_to_viewport().into_option() else {
        return Update::DoNothing;
    };
    let (dx, dy) = (at.x - drag.press_x, at.y - drag.press_y);
    if !drag.moved && dx.abs() < 3.0 && dy.abs() < 3.0 {
        return Update::DoNothing;
    }
    drag.moved = true;
    let dt = f64::from(dx) / f64::from(s.pps.max(MIN_PPS));
    let (mut start, mut duration) = drag_result(drag.mode, drag.start, drag.duration, dt, s.min_duration());
    if s.snapping {
        let points = snap_points(&s.tracks, s.playhead, clip_id);
        match drag.mode {
            TimelineDragMode::Move => {
                let snapped = snap_time(start, &points, s.pps);
                if (snapped - start).abs() > 1e-12 {
                    start = snapped;
                } else {
                    start = (snap_time(start + duration, &points, s.pps) - duration).max(0.0);
                }
            }
            TimelineDragMode::TrimStart => {
                let end = start + duration;
                start = snap_time(start, &points, s.pps).min(end - s.min_duration()).max(0.0);
                duration = end - start;
            }
            TimelineDragMode::TrimEnd => {
                let end = snap_time(start + duration, &points, s.pps).max(start + s.min_duration());
                duration = end - start;
            }
        }
    }
    let lane = s.tracks.get(drag.track).map_or(DEFAULT_TRACK_HEIGHT, TimelineTrack::lane_height);
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    let now_track = if drag.mode == TimelineDragMode::Move {
        let rows = (dy / lane.max(1.0)).round() as i64;
        (drag.track as i64 + rows).clamp(0, s.tracks.len().saturating_sub(1) as i64) as usize
    } else {
        drag.track
    };
    drag.now_start = start;
    drag.now_duration = duration;
    drag.now_track = now_track;
    let (left, width) = clip_geometry(start, duration, s.view_start, s.pps);
    #[allow(clippy::cast_precision_loss)]
    let top = CLIP_INSET + (now_track as f32 - drag.track as f32) * lane;
    s.drag = Some(drag);
    drop(s);
    let node = info.get_hit_node();
    info.set_css_property(node, CssProperty::left(LayoutLeft::px(left)));
    info.set_css_property(node, CssProperty::width(LayoutWidth::Px(PixelValue::px(width))));
    info.set_css_property(node, CssProperty::top(LayoutTop::px(top)));
    Update::DoNothing
}

/// The button is up over (or, captured, away from) a pressed clip: ONE
/// `Move` or `Trim` for the whole drag.
extern "C" fn on_clip_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (mut shared, clip_id) = {
        let Some(d) = data.downcast_ref::<ClipData>() else {
            return Update::DoNothing;
        };
        (d.shared.clone(), d.clip_id)
    };
    let Some(mut s) = shared.downcast_mut::<TimelineShared>() else {
        return Update::DoNothing;
    };
    let Some(drag) = s.drag.take().filter(|d| d.clip_id == clip_id) else {
        return Update::DoNothing;
    };
    let hook = s.on_event.clone();
    drop(s);
    info.release_pointer_capture();
    if !drag.moved {
        return Update::DoNothing;
    }
    let mut event = match drag.mode {
        TimelineDragMode::Move => {
            let mut e = TimelineEvent::create(TimelineEventKind::Move, drag.now_start);
            #[allow(clippy::cast_precision_loss)]
            let target = drag.now_track as f64;
            e.value = target;
            e
        }
        TimelineDragMode::TrimStart => TimelineEvent::create(TimelineEventKind::Trim, drag.now_start),
        TimelineDragMode::TrimEnd => {
            let mut e = TimelineEvent::create(TimelineEventKind::Trim, drag.now_start + drag.now_duration);
            e.edge = TimelineEdge::End;
            e
        }
    };
    event.clip_id = drag.clip_id;
    event.track = drag.track;
    fire(&hook, info, event)
}

/// The pointer left a clip with the button up: a lost release ends the drag.
extern "C" fn on_clip_leave(mut data: RefAny, info: CallbackInfo) -> Update {
    if info.get_current_mouse_state().left_down {
        return Update::DoNothing;
    }
    let mut shared = match data.downcast_ref::<ClipData>() {
        Some(d) => d.shared.clone(),
        None => return Update::DoNothing,
    };
    if let Some(mut s) = shared.downcast_mut::<TimelineShared>() {
        s.drag = None;
    }
    Update::DoNothing
}

/// A double-click on a clip: open it.
extern "C" fn on_clip_double_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    let (mut shared, track, clip_id) = {
        let Some(d) = data.downcast_ref::<ClipData>() else {
            return Update::DoNothing;
        };
        (d.shared.clone(), d.track, d.clip_id)
    };
    let Some(s) = shared.downcast_ref::<TimelineShared>() else {
        return Update::DoNothing;
    };
    let mut event = TimelineEvent::create(TimelineEventKind::Open, s.playhead);
    let hook = s.on_event.clone();
    drop(s);
    event.clip_id = clip_id;
    event.track = track;
    fire(&hook, info, event)
}

/// A track toggle (mute / show, lock) was clicked.
extern "C" fn on_track_toggle(mut data: RefAny, info: CallbackInfo) -> Update {
    let (mut shared, track, kind) = {
        let Some(d) = data.downcast_ref::<ToggleData>() else {
            return Update::DoNothing;
        };
        (d.shared.clone(), d.track, d.kind)
    };
    let Some(s) = shared.downcast_ref::<TimelineShared>() else {
        return Update::DoNothing;
    };
    let mut event = TimelineEvent::create(kind, s.playhead);
    let hook = s.on_event.clone();
    drop(s);
    event.track = track;
    fire(&hook, info, event)
}

// ==== the build ====

/// How far a clip sits inside its lane, top and bottom, in px.
pub const CLIP_INSET: f32 = 3.0;

/// What a theme decides about a timeline: the SKIN of each part, laid over
/// the part's base (the structure, the same in every theme: the
/// `TIMELINE_*_BASE` statics) by [`build`].
pub(crate) struct TimelineLook {
    /// The widget: the UI face, the ink, the surface.
    pub root: Vec<CssPropertyWithConditions>,
    /// The ruler row.
    pub head: Vec<CssPropertyWithConditions>,
    /// The corner over the headers (the playhead's timecode).
    pub corner: Vec<CssPropertyWithConditions>,
    /// The ruler.
    pub ruler: Vec<CssPropertyWithConditions>,
    /// A labelled (major) tick.
    pub tick: Vec<CssPropertyWithConditions>,
    /// A minor tick.
    pub tick_minor: Vec<CssPropertyWithConditions>,
    /// A major tick's label.
    pub tick_label: Vec<CssPropertyWithConditions>,
    /// The playhead's head on the ruler.
    pub ruler_head: Vec<CssPropertyWithConditions>,
    /// The column of track headers.
    pub headers: Vec<CssPropertyWithConditions>,
    /// One track header.
    pub header: Vec<CssPropertyWithConditions>,
    /// A track header's name.
    pub track_name: Vec<CssPropertyWithConditions>,
    /// The lanes (the keyboard stop: its focus ring is here).
    pub lanes: Vec<CssPropertyWithConditions>,
    /// One lane.
    pub lane: Vec<CssPropertyWithConditions>,
    /// A clip of a tint: its face, rule, radius and ink.
    pub clip: fn(TimelineClipTint) -> Vec<CssPropertyWithConditions>,
    /// A selected clip, over its face.
    pub clip_selected: Vec<CssPropertyWithConditions>,
    /// A clip's thumbnail.
    pub clip_thumb: Vec<CssPropertyWithConditions>,
    /// A clip's label.
    pub clip_label: Vec<CssPropertyWithConditions>,
    /// A clip's detail line.
    pub clip_detail: Vec<CssPropertyWithConditions>,
    /// The playhead line over the lanes.
    pub playhead: Vec<CssPropertyWithConditions>,
    /// The scroll bar row.
    pub scroll: Vec<CssPropertyWithConditions>,
    /// The scroll bar's track.
    pub scroll_track: Vec<CssPropertyWithConditions>,
    /// The scroll bar's thumb.
    pub thumb: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the root, if it has one.
    pub marker: Option<&'static str>,
}

type P = CssPropertyWithConditions;

/// The widget: a column that fills its host, chrome text not selectable.
pub(crate) static TIMELINE_BASE: &[P] = &[
    display_flex(),
    flex_direction(LayoutFlexDirection::Column),
    grow(1),
    P::simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
    P::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    overflow_x_hidden(),
    overflow_y_hidden(),
    P::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// The ruler row: the corner, then the ruler.
pub(crate) static TIMELINE_HEAD_BASE: &[P] = &[
    display_flex(),
    flex_direction(LayoutFlexDirection::Row),
    no_shrink(),
];

/// The corner: its timecode on the midline.
pub(crate) static TIMELINE_CORNER_BASE: &[P] = &[
    display_flex(),
    flex_direction(LayoutFlexDirection::Row),
    P::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    no_shrink(),
    overflow_x_hidden(),
    overflow_y_hidden(),
];

/// The ruler: the ticks are placed in it.
pub(crate) static TIMELINE_RULER_BASE: &[P] = &[
    position(LayoutPosition::Relative),
    grow(1),
    P::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    overflow_x_hidden(),
    overflow_y_hidden(),
];

/// A tick, a ruler head, the playhead: a line placed at its time.
pub(crate) static TIMELINE_MARK_BASE: &[P] = &[position(LayoutPosition::Absolute)];

/// A tick's label, beside its line.
pub(crate) static TIMELINE_TICK_LABEL_BASE: &[P] = &[position(LayoutPosition::Absolute), nowrap()];

/// The headers and the lanes, side by side.
pub(crate) static TIMELINE_BODY_BASE: &[P] = &[
    display_flex(),
    flex_direction(LayoutFlexDirection::Row),
    grow(1),
    P::simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
    overflow_x_hidden(),
    overflow_y_hidden(),
];

/// The column of headers.
pub(crate) static TIMELINE_HEADERS_BASE: &[P] = &[
    display_flex(),
    flex_direction(LayoutFlexDirection::Column),
    no_shrink(),
];

/// A header: the name, then the toggles, on one midline.
pub(crate) static TIMELINE_HEADER_BASE: &[P] = &[
    display_flex(),
    flex_direction(LayoutFlexDirection::Row),
    P::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    no_shrink(),
    overflow_x_hidden(),
    overflow_y_hidden(),
];

/// A header's name takes the room the toggles leave.
pub(crate) static TIMELINE_TRACK_NAME_BASE: &[P] = &[
    grow(1),
    P::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    nowrap(),
];

/// The lanes: a column of lanes, the playhead placed over them.
pub(crate) static TIMELINE_LANES_BASE: &[P] = &[
    position(LayoutPosition::Relative),
    display_flex(),
    flex_direction(LayoutFlexDirection::Column),
    grow(1),
    P::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    overflow_x_hidden(),
    overflow_y_hidden(),
];

/// A lane: its clips are placed in it (and may be dragged out of it).
pub(crate) static TIMELINE_LANE_BASE: &[P] = &[position(LayoutPosition::Relative), no_shrink()];

/// A clip: a block at its time, its thumbnail and lines in a row.
pub(crate) static TIMELINE_CLIP_BASE: &[P] = &[
    position(LayoutPosition::Absolute),
    display_flex(),
    flex_direction(LayoutFlexDirection::Row),
    P::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    overflow_x_hidden(),
    overflow_y_hidden(),
];

/// A clip's thumbnail keeps its size.
pub(crate) static TIMELINE_CLIP_THUMB_BASE: &[P] = &[no_shrink()];

/// A clip's label and detail: one line each, cut at the clip's end.
pub(crate) static TIMELINE_CLIP_TEXT_BASE: &[P] = &[
    P::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    nowrap(),
    overflow_x_hidden(),
    overflow_y_hidden(),
];

/// The scroll bar row: a spacer under the headers, then the track.
pub(crate) static TIMELINE_SCROLL_BASE: &[P] = &[
    display_flex(),
    flex_direction(LayoutFlexDirection::Row),
    no_shrink(),
];

/// The scroll bar's spacer under the headers.
pub(crate) static TIMELINE_SPACER_BASE: &[P] = &[no_shrink()];

/// The scroll bar's track: the thumb is placed in it.
pub(crate) static TIMELINE_SCROLL_TRACK_BASE: &[P] = &[position(LayoutPosition::Relative), grow(1)];

use crate::widgets::themes::decl::classes;

/// A ruler label: `MM:SS:FF` steps of frames in the first hour, else the
/// media clock (`M:SS`, `H:MM:SS` past the first hour - the seek bar's
/// [`crate::widgets::seek_bar::media_time`]).
fn tick_label(t: f64, fps: f32, step: f64) -> String {
    if step < 1.0 && t < 3600.0 {
        // "00:MM:SS:FF" without the hours.
        String::from(&timecode(t, fps)[3..])
    } else {
        crate::widgets::seek_bar::media_time(t.max(0.0))
    }
}

/// A track toggle: a link button with a glyph, named by what it does.
fn toggle(
    shared: &RefAny,
    track: usize,
    kind: TimelineEventKind,
    icon: &'static str,
    name: String,
    class: &'static str,
    theme: OptionUiTheme,
) -> Dom {
    let mut b = Button::with_type(AzString::from_const_str(""), ButtonType::Link)
        .with_icon(AzString::from_const_str(icon));
    b.alt = AzString::from(name);
    b.set_on_click(
        RefAny::new(ToggleData {
            shared: shared.clone(),
            track,
            kind,
        }),
        on_track_toggle as ButtonOnClickCallbackType,
    );
    if let Some(t) = theme.into_option() {
        b = b.with_theme(t);
    }
    b.dom().with_class(AzString::from_const_str(class))
}

/// The timeline's DOM in `look`: root [head [corner, ruler [ticks..,
/// head]], body [headers [header..], lanes [lane [clip..].., playhead]],
/// scroll [spacer, track [thumb]]]. Every part is its base (the
/// structure), then the look's skin, then its place (left / width /
/// height), which is the same in every theme.
#[allow(clippy::too_many_lines)]
pub(crate) fn build(t: Timeline, look: &TimelineLook) -> Dom {
    let part = |base: &[P], skin: &[P], place: &[P]| {
        let mut v = crate::widgets::themes::decl::on_base(base, skin);
        v.extend_from_slice(place);
        CssPropertyWithConditionsVec::from_vec(v)
    };
    let Timeline {
        duration,
        playhead,
        view_start,
        tracks,
        on_event,
        accessibility_name,
        pixels_per_second,
        fps,
        view_width,
        header_width,
        theme,
        snapping,
    } = t;
    let pps = pixels_per_second.max(MIN_PPS);
    let header_width = if header_width > 0.0 {
        header_width
    } else {
        DEFAULT_HEADER_WIDTH
    };
    let tracks = tracks.into_library_owned_vec();
    let (from, to) = visible_window(view_start, view_width, pps);
    let shared = RefAny::new(TimelineShared {
        on_event,
        tracks: tracks.clone(),
        duration,
        playhead,
        view_start,
        pps,
        fps,
        view_width,
        snapping,
        scrub: None,
        drag: None,
    });
    let bar = |event: HoverEventFilter, cb: extern "C" fn(RefAny, CallbackInfo) -> Update| {
        CoreCallbackData::create(EventFilter::Hover(event), shared.clone(), cb as usize)
    };

    // ---- the ruler row ----
    let corner = Dom::create_div()
        .with_ids_and_classes(classes(&[CORNER_CLASS]))
        .with_css_props(part(TIMELINE_CORNER_BASE, &look.corner, &[px_width(header_width)]))
        .with_child(crate::widgets::widget_p_with_text(timecode(playhead, fps)));
    let step = tick_step(pps, fps);
    let mut ticks: Vec<Dom> = Vec::new();
    #[allow(clippy::cast_possible_truncation)]
    let minor_ok = {
        let minor = step / 5.0;
        minor * f64::from(pps) >= 6.0 && (fps <= 0.0 || minor + 1e-9 >= 1.0 / f64::from(fps))
    };
    let first = (from.max(0.0) / step).floor() * step;
    let mut at = first;
    while at <= to && ticks.len() < 400 {
        let x = x_of(at, view_start, pps);
        ticks.push(
            Dom::create_div()
                .with_ids_and_classes(classes(&[TICK_CLASS]))
                .with_css_props(part(
                    TIMELINE_MARK_BASE,
                    &look.tick,
                    &[px_left(x), px_top(0.0), px_bottom(0.0)],
                )),
        );
        ticks.push(
            crate::widgets::widget_p_with_text(tick_label(at, fps, step))
                .with_ids_and_classes(classes(&[TICK_LABEL_CLASS]))
                .with_css_props(part(TIMELINE_TICK_LABEL_BASE, &look.tick_label, &[px_left(x + 4.0), px_top(2.0)])),
        );
        if minor_ok {
            for k in 1..5 {
                let m = at + step * f64::from(k) / 5.0;
                ticks.push(
                    Dom::create_div()
                        .with_ids_and_classes(classes(&[TICK_CLASS]))
                        .with_css_props(part(
                            TIMELINE_MARK_BASE,
                            &look.tick_minor,
                            &[
                                px_left(x_of(m, view_start, pps)),
                                px_top(RULER_HEIGHT * 0.65),
                                px_bottom(0.0),
                            ],
                        )),
                );
            }
        }
        at += step;
    }
    let playhead_x = x_of(playhead, view_start, pps);
    ticks.push(
        Dom::create_div()
            .with_ids_and_classes(classes(&[RULER_HEAD_CLASS]))
            .with_css_props(part(
                TIMELINE_MARK_BASE,
                &look.ruler_head,
                &[px_left(playhead_x - 1.0), px_top(0.0), px_bottom(0.0)],
            )),
    );
    let ruler = Dom::create_div()
        .with_ids_and_classes(classes(&[RULER_CLASS]))
        .with_css_props(part(TIMELINE_RULER_BASE, &look.ruler, &[]))
        .with_callbacks(
            alloc::vec![
                bar(HoverEventFilter::MouseDown, on_ruler_down),
                bar(HoverEventFilter::MouseMove, on_ruler_move),
                bar(HoverEventFilter::MouseUp, on_bar_up),
                bar(HoverEventFilter::MouseLeave, on_bar_leave),
            ]
            .into(),
        )
        .with_children(DomVec::from_vec(ticks));
    let head = Dom::create_div()
        .with_ids_and_classes(classes(&[HEAD_CLASS]))
        .with_css_props(part(TIMELINE_HEAD_BASE, &look.head, &[px_height(RULER_HEIGHT)]))
        .with_children(DomVec::from_vec(alloc::vec![corner, ruler]));

    // ---- the headers and the lanes ----
    let mut headers: Vec<Dom> = Vec::with_capacity(tracks.len());
    let mut lanes: Vec<Dom> = Vec::with_capacity(tracks.len() + 1);
    for (index, track) in tracks.iter().enumerate() {
        let height = track.lane_height();
        let name = track.name.as_str();
        let (mute_icon, mute_name) = match (track.kind, track.muted) {
            (TimelineTrackKind::Video, true) => ("visibility_off", format!("Show {name}")),
            (TimelineTrackKind::Video, false) => ("visibility", format!("Hide {name}")),
            (_, true) => ("volume_off", format!("Unmute {name}")),
            (_, false) => ("volume_up", format!("Mute {name}")),
        };
        let (lock_icon, lock_name) = if track.locked {
            ("lock", format!("Unlock {name}"))
        } else {
            ("lock_open", format!("Lock {name}"))
        };
        headers.push(
            Dom::create_div()
                .with_ids_and_classes(classes(&[HEADER_CLASS]))
                .with_css_props(part(TIMELINE_HEADER_BASE, &look.header, &[px_height(height)]))
                .with_children(DomVec::from_vec(alloc::vec![
                    crate::widgets::widget_p_with_text(track.name.clone())
                        .with_ids_and_classes(classes(&[TRACK_NAME_CLASS]))
                        .with_css_props(part(TIMELINE_TRACK_NAME_BASE, &look.track_name, &[])),
                    toggle(
                        &shared,
                        index,
                        TimelineEventKind::ToggleMute,
                        mute_icon,
                        mute_name,
                        MUTE_CLASS,
                        theme,
                    ),
                    toggle(
                        &shared,
                        index,
                        TimelineEventKind::ToggleLock,
                        lock_icon,
                        lock_name,
                        LOCK_CLASS,
                        theme,
                    ),
                ])),
        );

        let mut clips: Vec<Dom> = Vec::new();
        for c in track.clips.as_ref() {
            if c.end() <= from || c.start >= to {
                continue;
            }
            let (left, width) = clip_geometry(c.start, c.duration, view_start, pps);
            let mut names = alloc::vec![CLIP_CLASS];
            if c.selected {
                names.push(CLIP_SELECTED_CLASS);
            }
            if c.disabled {
                names.push(CLIP_DISABLED_CLASS);
            }
            let tint = if c.disabled {
                TimelineClipTint::Muted
            } else {
                c.tint
            };
            let mut skin = (look.clip)(tint);
            if c.selected {
                skin.extend_from_slice(&look.clip_selected);
            }
            let mut parts: Vec<Dom> = Vec::with_capacity(3);
            if let Some(image) = c.thumbnail.clone().into_option() {
                let thumb_h = (height - 2.0 * CLIP_INSET - 6.0).max(8.0);
                parts.push(
                    Dom::create_image(image)
                        .with_ids_and_classes(classes(&[CLIP_THUMB_CLASS]))
                        .with_css_props(part(
                            TIMELINE_CLIP_THUMB_BASE,
                            &look.clip_thumb,
                            &[px_height(thumb_h), px_width(thumb_h * 16.0 / 9.0)],
                        )),
                );
            }
            parts.push(
                crate::widgets::widget_p_with_text(c.label.clone())
                    .with_ids_and_classes(classes(&[CLIP_LABEL_CLASS]))
                    .with_css_props(part(TIMELINE_CLIP_TEXT_BASE, &look.clip_label, &[])),
            );
            if !c.detail.as_str().is_empty() {
                parts.push(
                    crate::widgets::widget_p_with_text(c.detail.clone())
                        .with_ids_and_classes(classes(&[CLIP_DETAIL_CLASS]))
                        .with_css_props(part(TIMELINE_CLIP_TEXT_BASE, &look.clip_detail, &[])),
                );
            }
            let data = RefAny::new(ClipData {
                shared: shared.clone(),
                track: index,
                clip_id: c.id,
            });
            let on = |event: HoverEventFilter, cb: extern "C" fn(RefAny, CallbackInfo) -> Update| {
                CoreCallbackData::create(EventFilter::Hover(event), data.clone(), cb as usize)
            };
            let a11y_name = format!(
                "{}, {} to {}, {}",
                c.label.as_str(),
                timecode(c.start, fps),
                timecode(c.end(), fps),
                name
            );
            let states = if c.selected {
                AccessibilityStateVec::from_vec(alloc::vec![AccessibilityState::Selected])
            } else {
                AccessibilityStateVec::from_const_slice(&[])
            };
            clips.push(
                Dom::create_div()
                    .with_ids_and_classes(classes(&names))
                    .with_css_props(part(
                        TIMELINE_CLIP_BASE,
                        &skin,
                        &[
                            px_left(left),
                            px_width(width),
                            px_top(CLIP_INSET),
                            px_bottom(CLIP_INSET),
                        ],
                    ))
                    .with_accessibility_info(AccessibilityInfo {
                        role: AccessibilityRole::ListItem,
                        accessibility_name: Some(AzString::from(a11y_name)).into(),
                        states,
                        ..Default::default()
                    })
                    .with_callbacks(
                        alloc::vec![
                            on(HoverEventFilter::MouseDown, on_clip_down),
                            on(HoverEventFilter::MouseMove, on_clip_move),
                            on(HoverEventFilter::MouseUp, on_clip_up),
                            on(HoverEventFilter::MouseLeave, on_clip_leave),
                            on(HoverEventFilter::DoubleClick, on_clip_double_click),
                        ]
                        .into(),
                    )
                    .with_children(DomVec::from_vec(parts)),
            );
        }
        lanes.push(
            Dom::create_div()
                .with_ids_and_classes(classes(&[LANE_CLASS]))
                .with_css_props(part(TIMELINE_LANE_BASE, &look.lane, &[px_height(height)]))
                .with_callbacks(
                    alloc::vec![CoreCallbackData::create(
                        EventFilter::Hover(HoverEventFilter::MouseDown),
                        RefAny::new(LaneData {
                            shared: shared.clone(),
                            track: index,
                        }),
                        on_lane_down as usize,
                    )]
                    .into(),
                )
                .with_children(DomVec::from_vec(clips)),
        );
    }
    lanes.push(
        Dom::create_div()
            .with_ids_and_classes(classes(&[PLAYHEAD_CLASS]))
            .with_css_props(part(
                TIMELINE_MARK_BASE,
                &look.playhead,
                &[px_left(playhead_x - 1.0), px_top(0.0), px_bottom(0.0)],
            )),
    );
    let headers = Dom::create_div()
        .with_ids_and_classes(classes(&[HEADERS_CLASS]))
        .with_css_props(part(TIMELINE_HEADERS_BASE, &look.headers, &[px_width(header_width)]))
        .with_children(DomVec::from_vec(headers));
    let lanes = Dom::create_div()
        .with_ids_and_classes(classes(&[LANES_CLASS]))
        .with_css_props(part(TIMELINE_LANES_BASE, &look.lanes, &[]))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Slider,
            accessibility_name: Some(accessibility_name).into(),
            accessibility_value: Some(AzString::from(timecode(playhead, fps))).into(),
            ..Default::default()
        })
        .with_callbacks(
            alloc::vec![CoreCallbackData::create(
                EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                shared.clone(),
                on_lanes_key as usize,
            )]
            .into(),
        )
        // The shared state is the lanes' dataset, so a rebuild in the
        // middle of a drag carries the drag over.
        .with_dataset(OptionRefAny::Some(shared.clone()))
        .with_merge_callback(azul_core::dom::DatasetMergeCallback::from_ptr(merge_timeline_state))
        .with_children(DomVec::from_vec(lanes));
    let body = Dom::create_div()
        .with_ids_and_classes(classes(&[BODY_CLASS]))
        .with_css_props(part(TIMELINE_BODY_BASE, &[], &[]))
        .with_children(DomVec::from_vec(alloc::vec![headers, lanes]));

    // ---- the scroll bar ----
    let span = f64::from(view_width.max(1.0)) / f64::from(pps);
    let (thumb_left, thumb_width) = thumb_span(view_start, span, duration);
    let thumb = Dom::create_div()
        .with_ids_and_classes(classes(&[THUMB_CLASS]))
        .with_css_props(part(
            TIMELINE_MARK_BASE,
            &look.thumb,
            &[
                P::simple(CssProperty::left(LayoutLeft::percent(thumb_left * 100.0))),
                P::simple(CssProperty::width(LayoutWidth::Px(PixelValue::percent(
                    (thumb_width * 100.0).max(1.0),
                )))),
                px_top(2.0),
                px_bottom(2.0),
            ],
        ));
    let scroll_track = Dom::create_div()
        .with_ids_and_classes(classes(&[SCROLL_TRACK_CLASS]))
        .with_css_props(part(TIMELINE_SCROLL_TRACK_BASE, &look.scroll_track, &[]))
        .with_callbacks(
            alloc::vec![
                bar(HoverEventFilter::MouseDown, on_scroll_down),
                bar(HoverEventFilter::MouseMove, on_scroll_move),
                bar(HoverEventFilter::MouseUp, on_bar_up),
                bar(HoverEventFilter::MouseLeave, on_bar_leave),
            ]
            .into(),
        )
        .with_child(thumb);
    let scroll = Dom::create_div()
        .with_ids_and_classes(classes(&[SCROLL_CLASS]))
        .with_css_props(part(TIMELINE_SCROLL_BASE, &look.scroll, &[px_height(SCROLL_HEIGHT)]))
        .with_children(DomVec::from_vec(alloc::vec![
            Dom::create_div().with_css_props(part(TIMELINE_SPACER_BASE, &[], &[px_width(header_width)])),
            scroll_track,
        ]));

    let mut root_classes = alloc::vec![TIMELINE_CLASS];
    if let Some(marker) = look.marker {
        root_classes.push(marker);
    }
    Dom::create_div()
        .with_ids_and_classes(classes(&root_classes))
        .with_css_props(part(TIMELINE_BASE, &look.root, &[]))
        .with_children(DomVec::from_vec(alloc::vec![head, body, scroll]))
}

#[cfg(test)]
#[path = "timeline_tests.rs"]
mod timeline_tests;
