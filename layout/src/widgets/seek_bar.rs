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
    dom::{Dom, DomNodeId},
    refany::RefAny,
};
use azul_css::{
    dynamic_selector::CssPropertyWithConditions, impl_option, AzString, F32Vec, OptionString,
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
pub fn media_time(seconds: f64) -> String {
    let _ = seconds;
    String::new()
}

/// How far into `duration_s` the position `position_s` is, `0.0..=1.0` (0 for an unknown length).
#[must_use]
pub fn seek_fraction(position_s: f64, duration_s: f64) -> f32 {
    let _ = (position_s, duration_s);
    0.0
}

/// The time under the pointer `x` px into a trough `width` px wide, of `duration_s`.
#[must_use]
pub fn time_at(x: f32, width: f32, duration_s: f64) -> f64 {
    let _ = (x, width, duration_s);
    0.0
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
    let _ = (key, primary, position_s, duration_s);
    None
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

    /// The bar's DOM.
    #[must_use]
    pub fn dom(self) -> Dom {
        Dom::create_div()
    }

    /// Moves a built bar (its root `node`) to `position_s` in place: the played fill, the thumb,
    /// the time label and the accessibility value - no rebuild (a player ticks it a few times a
    /// second). Skipped while the user drags it. False when `node` is not a seek bar's root.
    pub fn update_position(info: &mut CallbackInfo, node: DomNodeId, position_s: f64) -> bool {
        let _ = (info, node, position_s);
        false
    }
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
