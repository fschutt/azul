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

use azul_core::{
    callbacks::Update,
    dom::{Dom, DomVec, IdOrClass, IdOrClass::Class, IdOrClassVec},
    refany::RefAny,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option,
    props::{
        basic::length::FloatValue,
        layout::{LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexShrink},
        property::CssProperty,
        style::StyleUserSelect,
    },
    AzString, OptionString,
};

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
        match self {
            MediaRepeat::Off => MediaRepeat::All,
            MediaRepeat::All => MediaRepeat::One,
            MediaRepeat::One => MediaRepeat::Off,
        }
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

    /// The controls' DOM: the pinned theme's look, or (unpinned) both looks in their `@theme`
    /// blocks, the app theme picking.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::{flat, flora, theme_blocks};
        match self.theme.into_option() {
            Some(UiTheme::Flat) => flat::media_controls(self),
            Some(UiTheme::Flora) => flora::media_controls(self),
            None => {
                theme_blocks::follow_app_theme(self, flat::media_controls, flora::media_controls)
            }
        }
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

// ==== Interaction ====

/// What every control of one row shares: the app's hook.
struct ControlsShared {
    on_action: OptionMediaControlsOnAction,
}

/// A button's payload: the row's shared part and what the button asks for.
struct ActionData {
    shared: RefAny,
    action: MediaControlsAction,
}

/// Tells the app `event`.
fn fire(mut shared: RefAny, info: &mut CallbackInfo, event: MediaControlsEvent) -> Update {
    let Some(mut s) = shared.downcast_mut::<ControlsShared>() else {
        return Update::DoNothing;
    };
    let result = match s.on_action.as_mut() {
        Some(MediaControlsOnAction { callback, refany }) => {
            callback.invoke(refany.clone(), *info, event)
        }
        None => Update::DoNothing,
    };
    result
}

/// A control button was clicked.
pub extern "C" fn on_media_button(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((shared, action)) = data
        .downcast_ref::<ActionData>()
        .map(|d| (d.shared.clone(), d.action))
    else {
        return Update::DoNothing;
    };
    fire(shared, &mut info, MediaControlsEvent { value: 0.0, action })
}

/// The volume slider moved.
pub extern "C" fn on_media_volume(
    data: RefAny,
    mut info: CallbackInfo,
    state: crate::widgets::slider::SliderState,
) -> Update {
    let span = state.max - state.min;
    let value = if span > 0.0 {
        ((state.value - state.min) / span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    fire(
        data,
        &mut info,
        MediaControlsEvent {
            value,
            action: MediaControlsAction::Volume,
        },
    )
}

// ==== The DOM ====

static ROW_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-media-controls",
))];
static VOLUME_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-media-controls-volume",
))];

/// The row: the controls side by side on one midline, no text selection.
pub(crate) static MEDIA_CONTROLS_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// The volume box keeps its size.
pub(crate) static MEDIA_VOLUME_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// The controls' DOM in `look`: [shuffle] previous [back] play [forward] next [repeat] [volume].
pub(crate) fn build(controls: MediaControls, look: &MediaControlsLook) -> Dom {
    use crate::widgets::{
        button::{Button, ButtonOnClickCallbackType, ButtonType},
        slider::{Slider, SliderOnValueChangeCallbackType},
    };
    let MediaControls {
        on_action,
        accessibility_name,
        theme,
        volume,
        repeat,
        playing,
        shuffle,
        show_skip,
        show_shuffle_repeat,
    } = controls;
    let shared = RefAny::new(ControlsShared { on_action });
    let button = |icon: &'static str,
                  name: &'static str,
                  action: MediaControlsAction,
                  kind: ButtonType,
                  toggled: Option<bool>| {
        let mut b = Button::with_type(AzString::from_const_str(""), kind)
            .with_icon(AzString::from_const_str(icon));
        b.alt = AzString::from_const_str(name);
        if let Some(on) = toggled {
            b = b.with_toggled(on);
        }
        b.set_on_click(
            RefAny::new(ActionData {
                shared: shared.clone(),
                action,
            }),
            on_media_button as ButtonOnClickCallbackType,
        );
        if let Some(t) = theme.into_option() {
            b = b.with_theme(t);
        }
        b.dom()
    };
    use MediaControlsAction as A;
    let mut children: Vec<Dom> = Vec::with_capacity(8);
    if show_shuffle_repeat {
        let name = if shuffle { "Shuffle on" } else { "Shuffle off" };
        children.push(button(
            "shuffle",
            name,
            A::Shuffle,
            ButtonType::Link,
            Some(shuffle),
        ));
    }
    children.push(button(
        "skip_previous",
        "Previous",
        A::Previous,
        ButtonType::Link,
        None,
    ));
    if show_skip {
        children.push(button(
            "replay",
            "Back 15 seconds",
            A::SkipBack,
            ButtonType::Link,
            None,
        ));
    }
    let (icon, name) = if playing {
        ("pause", "Pause")
    } else {
        ("play_arrow", "Play")
    };
    children.push(button(icon, name, A::PlayPause, ButtonType::Primary, None));
    if show_skip {
        children.push(button(
            "forward_30",
            "Forward 30 seconds",
            A::SkipForward,
            ButtonType::Link,
            None,
        ));
    }
    children.push(button("skip_next", "Next", A::Next, ButtonType::Link, None));
    if show_shuffle_repeat {
        let (icon, name) = match repeat {
            MediaRepeat::Off => ("repeat", "Repeat off"),
            MediaRepeat::All => ("repeat", "Repeat all"),
            MediaRepeat::One => ("repeat_one", "Repeat one"),
        };
        children.push(button(
            icon,
            name,
            A::Repeat,
            ButtonType::Link,
            Some(repeat != MediaRepeat::Off),
        ));
    }
    if volume >= 0.0 {
        let mut slider = Slider::create(volume.clamp(0.0, 1.0) * 100.0, 0.0, 100.0)
            .with_accessibility_name("Volume")
            .with_on_value_change(
                shared.clone(),
                on_media_volume as SliderOnValueChangeCallbackType,
            );
        if let Some(t) = theme.into_option() {
            slider = slider.with_theme(t);
        }
        children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(VOLUME_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(
                    crate::widgets::themes::decl::on_base(MEDIA_VOLUME_BASE, &look.volume),
                ))
                .with_child(slider.dom()),
        );
    }
    let mut classes: Vec<IdOrClass> = ROW_CLASS.to_vec();
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(
            crate::widgets::themes::decl::on_base(MEDIA_CONTROLS_BASE, &look.row),
        ))
        // A toolbar: arrow keys between the controls are the buttons' own Tab order today.
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::Toolbar,
            accessibility_name,
            ..Default::default()
        })
        .with_children(DomVec::from_vec(children))
}

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
