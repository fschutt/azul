//! Media controls widget - the transport of a player: previous, play / pause, next; for a podcast
//! back 15 s and forward 30 s; shuffle and repeat toggles for a music queue; a volume slider.
//!
//! Every control is an icon [`Button`] named by what it does now ("Play" or "Pause", "Shuffle on"),
//! the volume a [`Slider`]; the row is a toolbar. One hook, [`MediaControls::on_action`], hears
//! every control as a [`MediaControlsEvent`] (the action, and the volume for `Volume`), so a
//! player wires one handler. The controls hold no playback state of their own: the app builds
//! them from its player's state (`AudioPlayer::get_state`, a video's status).
//!
//! Key types: [`MediaControls`], [`MediaControlsAction`], [`MediaRepeat`],
//! [`MediaControlsEvent`].

use alloc::vec::Vec;

use azul_core::{callbacks::Update, dom::Dom, refany::RefAny};
use azul_css::{dynamic_selector::CssPropertyWithConditions, impl_option, AzString, OptionString};

use crate::{
    callbacks::CallbackInfo,
    widgets::themes::{OptionUiTheme, UiTheme},
};

/// What a control of a [`MediaControls`] asks for.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MediaControlsAction {
    /// The previous track (or the start of this one).
    Previous,
    /// Play when paused, pause when playing.
    PlayPause,
    /// The next track.
    Next,
    /// Back 15 seconds (a podcast's).
    SkipBack,
    /// Forward 30 seconds (a podcast's).
    SkipForward,
    /// Shuffle on / off.
    Shuffle,
    /// Repeat off -> all -> one -> off.
    Repeat,
    /// The volume slider moved (`MediaControlsEvent::value`, `0.0..=1.0`).
    Volume,
}

/// How a queue repeats.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MediaRepeat {
    /// Stop after the last track.
    #[default]
    Off,
    /// Start the queue over.
    All,
    /// Play this track again.
    One,
}

impl MediaRepeat {
    /// The mode the repeat button goes to next: off -> all -> one -> off.
    #[must_use]
    pub const fn next(self) -> Self {
        self
    }
}

/// What the hook of a [`MediaControls`] is told.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MediaControlsEvent {
    /// The volume for [`MediaControlsAction::Volume`] (`0.0..=1.0`), 0 otherwise.
    pub value: f32,
    /// What the user asked for.
    pub action: MediaControlsAction,
}

/// Callback function type invoked when a control is used.
pub type MediaControlsOnActionCallbackType =
    extern "C" fn(RefAny, CallbackInfo, MediaControlsEvent) -> Update;
impl_widget_callback!(
    MediaControlsOnAction,
    OptionMediaControlsOnAction,
    MediaControlsOnActionCallback,
    MediaControlsOnActionCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        MediaControlsOnActionCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: MEDIA_CONTROLS_ON_ACTION_INVOKER,
    invoker_ty:     AzMediaControlsOnActionCallbackInvoker,
    thunk_fn:       az_media_controls_on_action_callback_thunk,
    setter_fn:      AzApp_setMediaControlsOnActionCallbackInvoker,
    from_handle_fn: AzMediaControlsOnActionCallback_createFromHostHandle,
    from_handle_byref_fn: AzMediaControlsOnActionCallback_createFromHostHandleByref,
    extra_args:     [ event: MediaControlsEvent ],
}

/// A player's transport: previous, play / pause, next, and the optional skips, toggles and
/// volume.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct MediaControls {
    /// Told every control.
    pub on_action: OptionMediaControlsOnAction,
    /// What the row is, for assistive technology ("Player").
    pub accessibility_name: OptionString,
    /// The widget theme the controls are pinned to, or `None` to follow the app theme.
    pub theme: OptionUiTheme,
    /// The volume, `0.0..=1.0`; negative: no volume slider.
    pub volume: f32,
    /// How the queue repeats (the repeat button's state).
    pub repeat: MediaRepeat,
    /// Playing: the middle button pauses.
    pub playing: bool,
    /// Shuffle is on (the shuffle button's state).
    pub shuffle: bool,
    /// Show back 15 s / forward 30 s (a podcast) around play / pause.
    pub show_skip: bool,
    /// Show the shuffle and repeat toggles (a music queue).
    pub show_shuffle_repeat: bool,
}

impl MediaControls {
    /// Previous, play (or pause when `playing`), next; no volume, no toggles.
    #[must_use]
    pub fn create(playing: bool) -> Self {
        Self {
            on_action: OptionMediaControlsOnAction::None,
            accessibility_name: OptionString::None,
            theme: OptionUiTheme::None,
            volume: -1.0,
            repeat: MediaRepeat::Off,
            playing,
            shuffle: false,
            show_skip: false,
            show_shuffle_repeat: false,
        }
    }

    /// A volume slider at `volume` (`0.0..=1.0`; negative hides it).
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
    }

    /// [`Self::set_volume`] for the builder chain.
    #[must_use]
    pub fn with_volume(mut self, volume: f32) -> Self {
        self.set_volume(volume);
        self
    }

    /// The shuffle and repeat toggles, in these states.
    pub fn set_shuffle_repeat(&mut self, shuffle: bool, repeat: MediaRepeat) {
        self.show_shuffle_repeat = true;
        self.shuffle = shuffle;
        self.repeat = repeat;
    }

    /// [`Self::set_shuffle_repeat`] for the builder chain.
    #[must_use]
    pub fn with_shuffle_repeat(mut self, shuffle: bool, repeat: MediaRepeat) -> Self {
        self.set_shuffle_repeat(shuffle, repeat);
        self
    }

    /// Back 15 s / forward 30 s around play / pause (a podcast).
    pub fn set_show_skip(&mut self, show: bool) {
        self.show_skip = show;
    }

    /// [`Self::set_show_skip`] for the builder chain.
    #[must_use]
    pub fn with_show_skip(mut self, show: bool) -> Self {
        self.set_show_skip(show);
        self
    }

    /// The hook told every control.
    pub fn set_on_action<C: Into<MediaControlsOnActionCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.on_action = Some(MediaControlsOnAction {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// [`Self::set_on_action`] for the builder chain.
    #[must_use]
    pub fn with_on_action<C: Into<MediaControlsOnActionCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_action(data, callback);
        self
    }

    /// Name the row for assistive technology.
    #[must_use]
    pub fn with_accessibility_name<S: Into<AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    /// Pin the widget theme; unset, the controls follow the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with paused controls and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(false);
        core::mem::swap(&mut s, self);
        s
    }

    /// The controls' DOM.
    #[must_use]
    pub fn dom(self) -> Dom {
        Dom::create_div()
    }
}

impl Default for MediaControls {
    fn default() -> Self {
        Self::create(false)
    }
}

impl From<MediaControls> for Dom {
    fn from(c: MediaControls) -> Self {
        c.dom()
    }
}

impl_option!(
    MediaControls,
    OptionMediaControls,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// What a theme decides about the controls: the SKIN of each part.
#[derive(Debug, Clone, Default)]
pub(crate) struct MediaControlsLook {
    /// The row.
    pub row: Vec<CssPropertyWithConditions>,
    /// The box around the volume slider (its spacing).
    pub volume: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the row, if it has one.
    pub marker: Option<&'static str>,
}

#[cfg(test)]
mod media_controls_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId},
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::widgets::{
        roving::test_support as rv,
        themes::{theme_blocks::checks, theme_checks},
    };

    /// The accessible names of the controls, in order.
    fn names(dom: &Dom) -> Vec<String> {
        theme_checks::focusable(dom)
            .into_iter()
            .filter_map(|(_, n)| {
                n.root.get_accessibility_info().and_then(|i| {
                    i.accessibility_name
                        .as_ref()
                        .map(|s| s.as_str().to_string())
                })
            })
            .collect()
    }

    #[test]
    fn repeat_goes_off_all_one_off() {
        assert_eq!(MediaRepeat::Off.next(), MediaRepeat::All);
        assert_eq!(MediaRepeat::All.next(), MediaRepeat::One);
        assert_eq!(MediaRepeat::One.next(), MediaRepeat::Off);
    }

    #[test]
    fn the_controls_are_named_by_what_they_do_now() {
        let paused = MediaControls::create(false).with_theme(UiTheme::Flat).dom();
        assert_eq!(names(&paused), vec!["Previous", "Play", "Next"]);
        let playing = MediaControls::create(true).with_theme(UiTheme::Flat).dom();
        assert_eq!(names(&playing), vec!["Previous", "Pause", "Next"]);
        let podcast = MediaControls::create(true)
            .with_show_skip(true)
            .with_theme(UiTheme::Flat)
            .dom();
        assert_eq!(
            names(&podcast),
            vec![
                "Previous",
                "Back 15 seconds",
                "Pause",
                "Forward 30 seconds",
                "Next"
            ]
        );
        let music = MediaControls::create(false)
            .with_shuffle_repeat(true, MediaRepeat::One)
            .with_volume(0.5)
            .with_theme(UiTheme::Flat)
            .dom();
        assert_eq!(
            names(&music),
            vec![
                "Shuffle on",
                "Previous",
                "Play",
                "Next",
                "Repeat one",
                "Volume"
            ]
        );
    }

    #[test]
    fn the_row_is_a_toolbar() {
        let dom = MediaControls::create(false)
            .with_accessibility_name("Player")
            .with_theme(UiTheme::Flora)
            .dom();
        let info = dom.root.get_accessibility_info().expect("a role");
        assert_eq!(info.role, azul_core::a11y::AccessibilityRole::Toolbar);
        assert_eq!(
            info.accessibility_name.as_ref().map(|s| s.as_str()),
            Some("Player")
        );
        assert!(theme_checks::has_class(
            &dom,
            "__azul-native-media-controls"
        ));
    }

    type Log = Arc<Mutex<Vec<MediaControlsEvent>>>;

    extern "C" fn record(mut data: RefAny, _: CallbackInfo, event: MediaControlsEvent) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(event);
        }
        Update::RefreshDom
    }

    #[test]
    fn a_click_on_a_control_reports_its_action() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = MediaControls::create(false)
            .with_on_action(
                RefAny::new(log.clone()),
                record as MediaControlsOnActionCallbackType,
            )
            .with_theme(UiTheme::Flat)
            .dom();
        let styled = StyledDom::create_from_dom(dom);
        let buttons: Vec<usize> = styled
            .node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter(|(_, n)| n.get_tab_index().is_some())
            .map(|(i, _)| i)
            .collect();
        assert_eq!(buttons.len(), 3);
        for (index, want) in buttons.iter().zip([
            MediaControlsAction::Previous,
            MediaControlsAction::PlayPause,
            MediaControlsAction::Next,
        ]) {
            let target = DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(*index))),
            };
            let (update, _) =
                rv::fire(&styled, target, EventFilter::Hover(HoverEventFilter::Click))
                    .expect("the button takes the click");
            assert_eq!(update, Update::RefreshDom);
            assert_eq!(
                log.lock().expect("log").last().map(|e| e.action),
                Some(want)
            );
        }
    }

    #[test]
    fn the_controls_without_a_theme_follow_the_app_theme() {
        let controls = || {
            MediaControls::create(true)
                .with_shuffle_repeat(false, MediaRepeat::All)
                .with_volume(0.8)
        };
        checks::assert_follows_the_app_theme(
            "media_controls",
            || controls().dom(),
            |t: UiTheme| controls().with_theme(t).dom(),
        );
    }
}
