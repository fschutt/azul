//! Seek bar widget - where a track or a video is, and the way to go elsewhere in it: the time
//! played, a trough with the played part (and the buffered part) filled and a thumb at the play
//! head, the length; chapter ticks on the trough.
//!
//! A press on the trough seeks there, a drag scrubs (each move is reported with
//! `SeekBarState::dragging` set, the release with it cleared: a player moves only the thumb while
//! the pointer is down and seeks the audio once on release), the arrow keys step five seconds
//! (thirty with Ctrl / Cmd), Home and End go to the ends. For assistive technology it is a slider
//! whose value reads "1:12 of 9:22".
//!
//! A player moves the bar a few times a second; [`SeekBar::update_position`] moves the fill, the
//! thumb and the time label of a built bar in place, without a rebuild.
//!
//! [`media_time`] is the one media clock format ("1:12", "1:02:11") - for the bar's labels and an
//! app's lists of durations.
//!
//! Key types: [`SeekBar`], [`SeekBarState`], [`SeekBarOnSeek`].

use alloc::{string::String, vec::Vec};

use azul_core::{
    callbacks::Update,
    dom::{Dom, DomNodeId, DomVec, IdOrClass, IdOrClass::Class, IdOrClassVec},
    refany::RefAny,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option,
    props::{
        basic::{length::FloatValue, pixel::PixelValue},
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow, LayoutFlexShrink,
            LayoutHeight, LayoutLeft, LayoutMarginLeft, LayoutMinWidth, LayoutPosition, LayoutTop,
            LayoutWidth,
        },
        property::CssProperty,
        style::{StyleCursor, StyleUserSelect},
    },
    AzString, F32Vec, OptionString,
};

use crate::{
    callbacks::CallbackInfo,
    widgets::themes::{OptionUiTheme, UiTheme},
};

/// Where a seek bar is: what its `on_seek` hook is told.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct SeekBarState {
    /// The position asked for (or shown), seconds.
    pub position_s: f64,
    /// The length, seconds (0 = unknown).
    pub duration_s: f64,
    /// The pointer is down: a scrub in progress (the release reports `false`).
    pub dragging: bool,
}

/// Callback function type invoked when the user seeks.
pub type SeekBarOnSeekCallbackType = extern "C" fn(RefAny, CallbackInfo, SeekBarState) -> Update;
impl_widget_callback!(
    SeekBarOnSeek,
    OptionSeekBarOnSeek,
    SeekBarOnSeekCallback,
    SeekBarOnSeekCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        SeekBarOnSeekCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: SEEK_BAR_ON_SEEK_INVOKER,
    invoker_ty:     AzSeekBarOnSeekCallbackInvoker,
    thunk_fn:       az_seek_bar_on_seek_callback_thunk,
    setter_fn:      AzApp_setSeekBarOnSeekCallbackInvoker,
    from_handle_fn: AzSeekBarOnSeekCallback_createFromHostHandle,
    from_handle_byref_fn: AzSeekBarOnSeekCallback_createFromHostHandleByref,
    extra_args:     [ state: SeekBarState ],
}

/// `seconds` as a media clock: "m:ss", "h:mm:ss" from an hour on; "--:--" when unknown
/// (negative, NaN, infinite).
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn media_time(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return String::from("--:--");
    }
    let total = seconds.floor() as u64;
    let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60);
    if h > 0 {
        alloc::format!("{h}:{m:02}:{s:02}")
    } else {
        alloc::format!("{m}:{s:02}")
    }
}

/// How far into `duration_s` the position `position_s` is, `0.0..=1.0` (0 for an unknown length).
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn seek_fraction(position_s: f64, duration_s: f64) -> f32 {
    if !(duration_s > 0.0) || !position_s.is_finite() {
        return 0.0;
    }
    (position_s / duration_s).clamp(0.0, 1.0) as f32
}

/// The time under the pointer `x` px into a trough `width` px wide, of `duration_s`.
#[must_use]
pub fn time_at(x: f32, width: f32, duration_s: f64) -> f64 {
    if !(width > 0.0) || !(duration_s > 0.0) || !x.is_finite() {
        return 0.0;
    }
    f64::from((x / width).clamp(0.0, 1.0)) * duration_s
}

/// Where a key moves a bar at `position_s` of `duration_s`: Left / Down 5 s back, Right / Up 5 s
/// on (30 s with `primary`, Ctrl or Cmd), Home the start, End the end; `None` for other keys.
#[must_use]
pub fn key_target(
    key: azul_core::window::VirtualKeyCode,
    primary: bool,
    position_s: f64,
    duration_s: f64,
) -> Option<f64> {
    use azul_core::window::VirtualKeyCode as K;
    if !(duration_s > 0.0) {
        return None;
    }
    let step = if primary { 30.0 } else { 5.0 };
    let target = match key {
        K::Left | K::Down => position_s - step,
        K::Right | K::Up => position_s + step,
        K::Home => 0.0,
        K::End => duration_s,
        _ => return None,
    };
    Some(target.clamp(0.0, duration_s))
}

/// A seek bar: the time played, the trough, the length.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct SeekBar {
    /// The play head, seconds.
    pub position_s: f64,
    /// The length, seconds (0 = unknown: the bar is empty and cannot seek).
    pub duration_s: f64,
    /// How far the media is loaded, seconds (a stream; 0 = not shown).
    pub buffered_s: f64,
    /// Chapter starts, seconds: a tick on the trough each.
    pub chapters: F32Vec,
    /// Called when the user seeks (press, drag, release, keys).
    pub on_seek: OptionSeekBarOnSeek,
    /// What the bar is the position of, for assistive technology ("Playback position").
    pub accessibility_name: OptionString,
    /// The widget theme this bar is pinned to, or `None` to follow the app theme.
    pub theme: OptionUiTheme,
    /// The time labels either side of the trough (on by default).
    pub show_times: bool,
}

impl SeekBar {
    /// A bar at `position_s` of `duration_s`.
    #[must_use]
    pub fn create(position_s: f64, duration_s: f64) -> Self {
        Self {
            position_s,
            duration_s,
            buffered_s: 0.0,
            chapters: F32Vec::from_const_slice(&[]),
            on_seek: OptionSeekBarOnSeek::None,
            accessibility_name: OptionString::None,
            theme: OptionUiTheme::None,
            show_times: true,
        }
    }

    /// How far the media is loaded.
    pub fn set_buffered(&mut self, buffered_s: f64) {
        self.buffered_s = buffered_s;
    }

    /// [`Self::set_buffered`] for the builder chain.
    #[must_use]
    pub fn with_buffered(mut self, buffered_s: f64) -> Self {
        self.set_buffered(buffered_s);
        self
    }

    /// The chapter starts (seconds).
    pub fn set_chapters(&mut self, chapters: F32Vec) {
        self.chapters = chapters;
    }

    /// [`Self::set_chapters`] for the builder chain.
    #[must_use]
    pub fn with_chapters(mut self, chapters: F32Vec) -> Self {
        self.set_chapters(chapters);
        self
    }

    /// Show (or hide) the time labels.
    pub fn set_show_times(&mut self, show: bool) {
        self.show_times = show;
    }

    /// [`Self::set_show_times`] for the builder chain.
    #[must_use]
    pub fn with_show_times(mut self, show: bool) -> Self {
        self.set_show_times(show);
        self
    }

    /// The hook told every seek.
    pub fn set_on_seek<C: Into<SeekBarOnSeekCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_seek = Some(SeekBarOnSeek {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// [`Self::set_on_seek`] for the builder chain.
    #[must_use]
    pub fn with_on_seek<C: Into<SeekBarOnSeekCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_seek(data, callback);
        self
    }

    /// Name the bar for assistive technology.
    #[must_use]
    pub fn with_accessibility_name<S: Into<AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    /// Pin the widget theme; unset, the bar follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty bar and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(0.0, 0.0);
        core::mem::swap(&mut s, self);
        s
    }

    /// The bar's DOM: the pinned theme's look, or (unpinned) both looks in their `@theme`
    /// blocks, the app theme picking.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::{flat, flora, theme_blocks};
        match self.theme.into_option() {
            Some(UiTheme::Flat) => flat::seek_bar(self),
            Some(UiTheme::Flora) => flora::seek_bar(self),
            None => theme_blocks::follow_app_theme(self, flat::seek_bar, flora::seek_bar),
        }
    }

    /// Moves a built bar (its root `node`) to `position_s` in place: the played fill, the thumb,
    /// the time label and the accessibility value - no rebuild (a player ticks it a few times a
    /// second). Skipped while the user drags it. False when `node` is not a seek bar's root.
    pub fn update_position(info: &mut CallbackInfo, node: DomNodeId, position_s: f64) -> bool {
        let Some(first) = info.get_first_child(node) else {
            return false;
        };
        // The trough is the first child without labels, the second with them.
        let track = if is_track(info, first) {
            first
        } else {
            match info.get_next_sibling(first) {
                Some(t) if is_track(info, t) => t,
                _ => return false,
            }
        };
        move_surface(info, track, position_s)
    }
}

/// Moves the seek surface (a seek bar's trough, a waveform) at `node` to `position_s` in place;
/// skipped while the user drags it. False when `node` carries no seek surface. The one in-place
/// move for both widgets.
pub(crate) fn move_surface(info: &mut CallbackInfo, node: DomNodeId, position_s: f64) -> bool {
    let Some(mut data) = info.get_dataset(node) else {
        return false;
    };
    let (state, surface) = {
        let Some(mut w) = data.downcast_mut::<SeekBarWrapper>() else {
            return false;
        };
        if w.inner.dragging {
            // The user holds the thumb: the player must not pull it away.
            return true;
        }
        w.inner.position_s = position_s;
        let now = (w.inner, w.surface);
        now
    };
    show_position(info, node, state, surface);
    true
}

impl Default for SeekBar {
    fn default() -> Self {
        Self::create(0.0, 0.0)
    }
}

impl From<SeekBar> for Dom {
    fn from(b: SeekBar) -> Self {
        b.dom()
    }
}

impl_option!(
    SeekBar,
    OptionSeekBar,
    copy = false,
    [Debug, Clone, PartialEq]
);

// ==== Interaction ====

/// Which widget a seek surface is: its parts differ, its interaction does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SeekSurface {
    /// A seek bar's trough: [buffered, played, ticks.., thumb].
    Bar,
    /// A waveform: [bars.., playhead].
    Waveform,
}

/// The seek surface's dataset: the hook, where it is, and which widget it is.
pub(crate) struct SeekBarWrapper {
    pub on_seek: OptionSeekBarOnSeek,
    pub inner: SeekBarState,
    pub surface: SeekSurface,
}

/// Whether `node` is a seek bar's trough (it carries the bar's dataset).
fn is_track(info: &mut CallbackInfo, node: DomNodeId) -> bool {
    let Some(mut data) = info.get_dataset(node) else {
        return false;
    };
    let found = data.downcast_ref::<SeekBarWrapper>().is_some();
    found
}

/// What a screen reader says for a bar at `state`: "1:12 of 9:22".
pub(crate) fn value_text(state: SeekBarState) -> String {
    alloc::format!(
        "{} of {}",
        media_time(state.position_s),
        media_time(state.duration_s)
    )
}

/// Moves the played part, the thumb and the time label of the bar whose trough is `track` to
/// `state`, in place (a waveform: its playhead).
fn show_position(
    info: &mut CallbackInfo,
    track: DomNodeId,
    state: SeekBarState,
    surface: SeekSurface,
) {
    use azul_css::props::{
        basic::pixel::PixelValue,
        layout::{LayoutLeft, LayoutWidth},
        property::CssProperty,
    };
    let percent = seek_fraction(state.position_s, state.duration_s) * 100.0;
    if surface == SeekSurface::Waveform {
        // [bars.., playhead]: the playhead moves; the bars' colours follow at the next build.
        if let Some(head) = info.get_last_child(track) {
            info.set_css_property(head, CssProperty::const_left(LayoutLeft::percent(percent)));
        }
        info.set_accessibility_value(track, AzString::from(value_text(state)));
        return;
    }
    // [buffered, played, ticks.., thumb]
    if let Some(played) = info
        .get_first_child(track)
        .and_then(|buffered| info.get_next_sibling(buffered))
    {
        info.set_css_property(
            played,
            CssProperty::const_width(LayoutWidth::Px(PixelValue::percent(percent))),
        );
    }
    if let Some(thumb) = info.get_last_child(track) {
        info.set_css_property(thumb, CssProperty::const_left(LayoutLeft::percent(percent)));
    }
    info.set_accessibility_value(track, AzString::from(value_text(state)));
    // The time label: the root's first child when it is not the trough itself.
    if let Some(root) = info.get_parent(track) {
        if let Some(label) = info.get_first_child(root).filter(|l| *l != track) {
            if let Some(text) = info.get_first_child(label) {
                info.change_node_text(text, AzString::from(media_time(state.position_s)));
            }
        }
    }
}

/// Tells the app where the bar is now.
fn report(w: &mut SeekBarWrapper, info: &mut CallbackInfo) -> Update {
    let state = w.inner;
    match w.on_seek.as_mut() {
        Some(SeekBarOnSeek { callback, refany }) => callback.invoke(refany.clone(), *info, state),
        None => Update::DoNothing,
    }
}

/// The pointer's place on the trough becomes the position.
fn seek_to_pointer(w: &mut SeekBarWrapper, info: &mut CallbackInfo) -> Update {
    let Some(pos) = info.get_cursor_relative_to_node().into_option() else {
        return Update::DoNothing;
    };
    let width = info.get_hit_node_rect().map_or(0.0, |r| r.size.width);
    if !(width > 0.0) || !(w.inner.duration_s > 0.0) {
        return Update::DoNothing;
    }
    w.inner.position_s = time_at(pos.x, width, w.inner.duration_s);
    let track = info.get_hit_node();
    show_position(info, track, w.inner, w.surface);
    report(w, info)
}

/// Pointer down on the trough: a scrub starts there.
pub extern "C" fn on_seek_bar_pointer_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut w) = data.downcast_mut::<SeekBarWrapper>() else {
        return Update::DoNothing;
    };
    w.inner.dragging = true;
    seek_to_pointer(&mut w, &mut info)
}

/// Pointer move: the scrub follows the pointer.
pub extern "C" fn on_seek_bar_pointer_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut w) = data.downcast_mut::<SeekBarWrapper>() else {
        return Update::DoNothing;
    };
    if !w.inner.dragging {
        return Update::DoNothing;
    }
    seek_to_pointer(&mut w, &mut info)
}

/// Pointer up: the scrub ends where it is (reported with `dragging` false: seek now).
pub extern "C" fn on_seek_bar_pointer_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut w) = data.downcast_mut::<SeekBarWrapper>() else {
        return Update::DoNothing;
    };
    if !w.inner.dragging {
        return Update::DoNothing;
    }
    w.inner.dragging = false;
    report(&mut w, &mut info)
}

/// Pointer leave: a scrub ends when the pointer left the TROUGH (not just its thumb - the
/// slider's rule: every event bubbles here, the cursor decides).
pub extern "C" fn on_seek_bar_pointer_leave(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let still_inside = match (
        info.get_cursor_relative_to_node().into_option(),
        info.get_hit_node_rect(),
    ) {
        (Some(pos), Some(rect)) => {
            pos.x >= 0.0 && pos.y >= 0.0 && pos.x < rect.size.width && pos.y < rect.size.height
        }
        _ => false,
    };
    if still_inside {
        return Update::DoNothing;
    }
    on_seek_bar_pointer_up(data, info)
}

/// The keys: 5 s steps, 30 s with Ctrl / Cmd, Home / End.
pub extern "C" fn on_seek_bar_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut w) = data.downcast_mut::<SeekBarWrapper>() else {
        return Update::DoNothing;
    };
    let ks = info.get_current_keyboard_state();
    let Some(key) = ks.current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let Some(target) = key_target(
        key,
        ks.primary_down(),
        w.inner.position_s,
        w.inner.duration_s,
    ) else {
        return Update::DoNothing;
    };
    // The arrow must not also scroll the page under the bar.
    info.prevent_default();
    w.inner.position_s = target;
    w.inner.dragging = false;
    let track = info.get_hit_node();
    show_position(&mut info, track, w.inner, w.surface);
    report(&mut w, &mut info)
}

/// Carries a scrub across a parent rebuild (the slider's rule: while the pointer is down the
/// pointer wins - `dragging` and the scrubbed position carry over; once it is up, the app's
/// position is the truth). The hook is the fresh build's.
pub extern "C" fn merge_seek_bar_state(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    {
        let new_guard = new_data.downcast_mut::<SeekBarWrapper>();
        let old_guard = old_data.downcast_ref::<SeekBarWrapper>();
        if let (Some(mut new_w), Some(old_w)) = (new_guard, old_guard) {
            if old_w.inner.dragging {
                new_w.inner.dragging = true;
                new_w.inner.position_s = old_w.inner.position_s;
            }
        }
    }
    new_data
}

// ==== The DOM ====

static ROOT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-seek-bar"))];
static TIME_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-seek-bar-time",
))];
static TRACK_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-seek-bar-track",
))];
static BUFFERED_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-seek-bar-buffered",
))];
static PLAYED_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-seek-bar-played",
))];
static TICK_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-seek-bar-tick",
))];
static THUMB_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-seek-bar-thumb",
))];

const fn simple(p: CssProperty) -> CssPropertyWithConditions {
    CssPropertyWithConditions::simple(p)
}

/// The bar: one row, its parts on one midline, no text selection.
pub(crate) static SEEK_BAR_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// A time label keeps its width.
pub(crate) static SEEK_TIME_BASE: &[CssPropertyWithConditions] =
    &[simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    }))];

/// The trough: takes the row, 6 px thick, the positioning context of its parts.
pub(crate) static SEEK_TRACK_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_position(LayoutPosition::Relative)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(40))),
    simple(CssProperty::const_height(LayoutHeight::const_px(6))),
    simple(CssProperty::const_cursor(StyleCursor::Pointer)),
];

/// A part laid on the trough from its left edge, its full height.
pub(crate) static SEEK_FILL_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_position(LayoutPosition::Absolute)),
    simple(CssProperty::const_left(LayoutLeft::const_px(0))),
    simple(CssProperty::const_top(LayoutTop::const_px(0))),
    simple(CssProperty::const_height(LayoutHeight::const_px(6))),
];

/// A chapter tick: 2 px of the trough's height at its time.
pub(crate) static SEEK_TICK_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_position(LayoutPosition::Absolute)),
    simple(CssProperty::const_top(LayoutTop::const_px(0))),
    simple(CssProperty::const_width(LayoutWidth::const_px(2))),
    simple(CssProperty::const_height(LayoutHeight::const_px(6))),
];

/// The thumb: a 12 px knob centred on the play head.
pub(crate) static SEEK_THUMB_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_position(LayoutPosition::Absolute)),
    simple(CssProperty::const_top(LayoutTop::const_px(-3))),
    simple(CssProperty::const_margin_left(LayoutMarginLeft::const_px(
        -6,
    ))),
    simple(CssProperty::const_width(LayoutWidth::const_px(12))),
    simple(CssProperty::const_height(LayoutHeight::const_px(12))),
];

/// The pointer, touch and key callbacks of a seek surface (a seek bar's trough, a waveform) on
/// its dataset `data` - one set for both widgets.
pub(crate) fn seek_callbacks(data: &RefAny) -> Vec<azul_core::callbacks::CoreCallbackData> {
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{EventFilter, FocusEventFilter, HoverEventFilter},
        refany::OptionRefAny,
    };
    let mk = |event: EventFilter, cb: usize| CoreCallbackData {
        event,
        callback: CoreCallback {
            cb,
            ctx: OptionRefAny::None,
        },
        refany: data.clone(),
    };
    alloc::vec![
        mk(
            EventFilter::Hover(HoverEventFilter::MouseDown),
            on_seek_bar_pointer_down as usize
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::MouseMove),
            on_seek_bar_pointer_move as usize
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            on_seek_bar_pointer_up as usize
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::MouseLeave),
            on_seek_bar_pointer_leave as usize
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::TouchStart),
            on_seek_bar_pointer_down as usize
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::TouchMove),
            on_seek_bar_pointer_move as usize
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::TouchEnd),
            on_seek_bar_pointer_up as usize
        ),
        mk(
            EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
            on_seek_bar_key as usize
        ),
    ]
}

/// The bar's DOM in `look`.
pub(crate) fn build(bar: SeekBar, look: &SeekBarLook) -> Dom {
    use azul_core::{dom::TabIndex, refany::OptionRefAny};
    let part = |base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]| {
        crate::widgets::themes::decl::on_base(base, skin)
    };
    let props = |v: Vec<CssPropertyWithConditions>| CssPropertyWithConditionsVec::from_vec(v);
    let SeekBar {
        position_s,
        duration_s,
        buffered_s,
        chapters,
        on_seek,
        accessibility_name,
        theme: _,
        show_times,
    } = bar;
    crate::widgets::warn_widget_needs_a_name("SeekBar", accessibility_name.is_some());
    let state = SeekBarState {
        position_s,
        duration_s,
        dragging: false,
    };
    let percent = |t: f64| seek_fraction(t, duration_s) * 100.0;
    let width = |p: f32| {
        simple(CssProperty::const_width(LayoutWidth::Px(
            PixelValue::percent(p),
        )))
    };
    let left = |p: f32| simple(CssProperty::const_left(LayoutLeft::percent(p)));

    let mut parts: Vec<Dom> = Vec::new();
    let mut buffered = part(SEEK_FILL_BASE, &look.buffered);
    buffered.push(width(percent(buffered_s)));
    parts.push(
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(BUFFERED_CLASS))
            .with_css_props(props(buffered)),
    );
    let mut played = part(SEEK_FILL_BASE, &look.played);
    played.push(width(percent(position_s)));
    parts.push(
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(PLAYED_CLASS))
            .with_css_props(props(played)),
    );
    for chapter in chapters.as_ref() {
        let mut tick = part(SEEK_TICK_BASE, &look.tick);
        tick.push(left(percent(f64::from(*chapter))));
        parts.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(TICK_CLASS))
                .with_css_props(props(tick)),
        );
    }
    let mut thumb = part(SEEK_THUMB_BASE, &look.thumb);
    thumb.push(left(percent(position_s)));
    parts.push(
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(THUMB_CLASS))
            .with_css_props(props(thumb)),
    );

    let data = RefAny::new(SeekBarWrapper {
        on_seek,
        inner: state,
        surface: SeekSurface::Bar,
    });
    let callbacks = seek_callbacks(&data);
    let track = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(TRACK_CLASS))
        .with_css_props(props(part(SEEK_TRACK_BASE, &look.track)))
        .with_callbacks(callbacks.into())
        .with_dataset(OptionRefAny::Some(data.clone()))
        .with_merge_callback(azul_core::dom::DatasetMergeCallback::from_ptr(
            merge_seek_bar_state,
        ))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::Slider,
            accessibility_name,
            accessibility_value: Some(AzString::from(value_text(state))).into(),
            ..Default::default()
        })
        .with_children(DomVec::from_vec(parts));

    let time = |t: f64| {
        crate::widgets::widget_p_with_text(AzString::from(media_time(t)))
            .with_ids_and_classes(IdOrClassVec::from_const_slice(TIME_CLASS))
            .with_css_props(props(part(SEEK_TIME_BASE, &look.time)))
    };
    let mut children: Vec<Dom> = Vec::with_capacity(3);
    if show_times {
        children.push(time(position_s));
    }
    children.push(track);
    if show_times {
        children.push(time(duration_s));
    }
    let mut classes: Vec<IdOrClass> = ROOT_CLASS.to_vec();
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(props(SEEK_BAR_BASE.to_vec()))
        .with_children(DomVec::from_vec(children))
}

/// What a theme decides about a seek bar: the SKIN of each part.
#[derive(Debug, Clone, Default)]
pub(crate) struct SeekBarLook {
    /// The time labels.
    pub time: Vec<CssPropertyWithConditions>,
    /// The trough.
    pub track: Vec<CssPropertyWithConditions>,
    /// The buffered part.
    pub buffered: Vec<CssPropertyWithConditions>,
    /// The played part.
    pub played: Vec<CssPropertyWithConditions>,
    /// A chapter tick.
    pub tick: Vec<CssPropertyWithConditions>,
    /// The thumb (its focus ring included).
    pub thumb: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the root, if it has one.
    pub marker: Option<&'static str>,
}

#[cfg(test)]
mod seek_bar_tests {
    use azul_core::{dom::NodeType, window::VirtualKeyCode as K};

    use super::*;
    use crate::widgets::themes::{theme_blocks::checks, theme_checks};

    #[test]
    fn media_time_reads_like_a_player_clock() {
        assert_eq!(media_time(0.0), "0:00");
        assert_eq!(media_time(72.9), "1:12", "whole seconds, rounded down");
        assert_eq!(media_time(562.0), "9:22");
        assert_eq!(media_time(3599.0), "59:59");
        assert_eq!(media_time(3731.0), "1:02:11");
        assert_eq!(media_time(-1.0), "--:--");
        assert_eq!(media_time(f64::NAN), "--:--");
        assert_eq!(media_time(f64::INFINITY), "--:--");
    }

    #[test]
    fn the_fraction_and_the_time_under_the_pointer_are_clamped_to_the_track() {
        assert!((seek_fraction(30.0, 120.0) - 0.25).abs() < 1e-6);
        assert_eq!(seek_fraction(200.0, 120.0), 1.0);
        assert_eq!(seek_fraction(-5.0, 120.0), 0.0);
        assert_eq!(seek_fraction(10.0, 0.0), 0.0, "an unknown length is empty");
        assert!((time_at(50.0, 200.0, 120.0) - 30.0).abs() < 1e-9);
        assert_eq!(time_at(-10.0, 200.0, 120.0), 0.0);
        assert_eq!(time_at(500.0, 200.0, 120.0), 120.0);
        assert_eq!(
            time_at(50.0, 0.0, 120.0),
            0.0,
            "an unlaid trough seeks nowhere"
        );
    }

    #[test]
    fn the_keys_step_five_seconds_thirty_with_ctrl_and_home_and_end_go_to_the_ends() {
        assert_eq!(key_target(K::Right, false, 10.0, 100.0), Some(15.0));
        assert_eq!(key_target(K::Left, false, 10.0, 100.0), Some(5.0));
        assert_eq!(key_target(K::Left, false, 2.0, 100.0), Some(0.0));
        assert_eq!(key_target(K::Up, true, 10.0, 100.0), Some(40.0));
        assert_eq!(key_target(K::Right, true, 90.0, 100.0), Some(100.0));
        assert_eq!(key_target(K::Home, false, 50.0, 100.0), Some(0.0));
        assert_eq!(key_target(K::End, false, 50.0, 100.0), Some(100.0));
        assert_eq!(key_target(K::A, false, 50.0, 100.0), None);
        assert_eq!(
            key_target(K::Right, false, 0.0, 0.0),
            None,
            "nothing to seek in"
        );
    }

    fn texts(node: &Dom, out: &mut Vec<String>) {
        if let NodeType::Text(s) = node.root.get_node_type() {
            out.push(s.as_ref().as_str().to_string());
        }
        for c in node.children.as_ref() {
            texts(c, out);
        }
    }

    fn bar() -> SeekBar {
        SeekBar::create(72.0, 562.0)
            .with_buffered(300.0)
            .with_chapters(F32Vec::from_vec(alloc::vec![0.0, 120.0, 400.0]))
            .with_accessibility_name("Playback position")
    }

    #[test]
    fn the_bar_is_the_time_played_the_trough_and_the_length() {
        for theme in checks::BOTH {
            let dom = bar().with_theme(theme).dom();
            assert!(theme_checks::has_class(&dom, "__azul-native-seek-bar"));
            let parts = dom.children.as_ref();
            assert_eq!(parts.len(), 3, "{}: time, trough, length", theme.name());
            let mut t = Vec::new();
            texts(&dom, &mut t);
            assert_eq!(t, vec!["1:12".to_string(), "9:22".to_string()]);
            let track = &parts[1];
            assert!(theme_checks::has_class(
                track,
                "__azul-native-seek-bar-track"
            ));
            let classes = |cls: &str| {
                theme_checks::nodes(track)
                    .into_iter()
                    .filter(|(_, n)| theme_checks::has_class(n, cls))
                    .count()
            };
            assert_eq!(classes("__azul-native-seek-bar-played"), 1);
            assert_eq!(classes("__azul-native-seek-bar-buffered"), 1);
            assert_eq!(classes("__azul-native-seek-bar-thumb"), 1);
            assert_eq!(
                classes("__azul-native-seek-bar-tick"),
                3,
                "a tick per chapter"
            );
        }
        let bare = SeekBar::create(10.0, 60.0)
            .with_show_times(false)
            .with_theme(UiTheme::Flat)
            .dom();
        assert_eq!(bare.children.as_ref().len(), 1, "the trough alone");
    }

    #[test]
    fn the_bar_is_one_slider_stop_reading_the_time_of_the_length() {
        let dom = bar().with_theme(UiTheme::Flat).dom();
        let stops = theme_checks::focusable(&dom);
        assert_eq!(stops.len(), 1, "{stops:?}");
        let info = stops[0].1.root.get_accessibility_info().expect("a role");
        assert_eq!(info.role, azul_core::a11y::AccessibilityRole::Slider);
        assert_eq!(
            info.accessibility_value.as_ref().map(|v| v.as_str()),
            Some("1:12 of 9:22")
        );
        assert_eq!(
            info.accessibility_name.as_ref().map(|v| v.as_str()),
            Some("Playback position")
        );
    }

    #[test]
    fn a_bar_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "seek_bar",
            || bar().dom(),
            |t: UiTheme| bar().with_theme(t).dom(),
        );
    }
}
