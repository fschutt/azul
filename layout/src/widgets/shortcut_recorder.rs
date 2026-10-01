//! Shortcut recorder widget - the field a settings page records a keyboard
//! shortcut with: it shows the shortcut ("Ctrl+Shift+K"), and a click or
//! Enter makes it LISTEN ("Press a shortcut..."): the next key with its
//! modifiers is the new shortcut. Escape (or leaving the field) cancels,
//! Backspace or Delete clears it. VS Code's keybinding field, the macOS
//! "Record Shortcut" control.
//!
//! The value is a [`GlobalHotkey`] - modifiers plus one key, the type an
//! app already declares its hotkeys with - shown with its
//! `to_display_string` (Cmd / Option on a Mac). The widget owns nothing:
//! the app keeps the shortcut and whether the field listens, hears every
//! request through ONE callback ([`ShortcutRecorder::on_event`], a
//! [`ShortcutRecorderEvent`]) and rebuilds; [`ShortcutRecorder::apply`] is
//! the rule an app keeps the two with. For assistive technology the field
//! is a hotkey field named by [`ShortcutRecorder::accessibility_name`]
//! whose value is the text it shows; it is one Tab stop.
//!
//! Key types: [`ShortcutRecorder`], [`ShortcutRecorderEvent`].

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole, AccessibilityState, AccessibilityStateVec},
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, EventFilter, HoverEventFilter, TabIndex},
    events::FocusEventFilter,
    global_hotkey::{key_display_name, GlobalHotkey, HotkeyModifiers},
    refany::{OptionRefAny, RefAny},
    window::VirtualKeyCode,
};
use azul_css::AzString;

use crate::{
    callbacks::CallbackInfo,
    widgets::{
        dialog_kit::{self, DialogKitLook, FIELD_BASE},
        roving,
        shells::GROW_LABEL_BASE,
        themes::{OptionUiTheme, UiTheme},
    },
};

/// The widget's class.
pub const RECORDER_CLASS: &str = "__azul-native-shortcut-recorder";
/// Added while the field listens.
pub const RECORDING_CLASS: &str = "__azul-native-shortcut-recorder-recording";

/// What the user asked of the field.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShortcutRecorderEventKind {
    /// Listen for a shortcut (a click, Enter or Space).
    StartRecording,
    /// A shortcut was pressed while listening: `hotkey` is it.
    Recorded,
    /// Stop listening, keep the shortcut (Escape, leaving the field).
    Cancelled,
    /// Remove the shortcut (Backspace, Delete).
    Cleared,
}

/// One request from the field.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortcutRecorderEvent {
    /// The recorded shortcut (`Recorded`), else the current one.
    pub hotkey: GlobalHotkey,
    /// What was asked.
    pub kind: ShortcutRecorderEventKind,
}

/// Callback invoked for a request from the field.
pub type ShortcutRecorderOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, ShortcutRecorderEvent) -> Update;
impl_widget_callback!(
    ShortcutRecorderOnEvent,
    OptionShortcutRecorderOnEvent,
    ShortcutRecorderOnEventCallback,
    ShortcutRecorderOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ShortcutRecorderOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: SHORTCUT_RECORDER_ON_EVENT_INVOKER,
    invoker_ty:     AzShortcutRecorderOnEventCallbackInvoker,
    thunk_fn:       az_shortcut_recorder_on_event_callback_thunk,
    setter_fn:      AzApp_setShortcutRecorderOnEventCallbackInvoker,
    from_handle_fn: AzShortcutRecorderOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzShortcutRecorderOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: ShortcutRecorderEvent ],
}

/// A field that records a keyboard shortcut.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct ShortcutRecorder {
    /// The text while there is no shortcut ("None").
    pub placeholder: AzString,
    /// The text while the field listens ("Press a shortcut...").
    pub prompt: AzString,
    /// What the field is called, for assistive technology.
    pub accessibility_name: AzString,
    /// Hears every request.
    pub on_event: OptionShortcutRecorderOnEvent,
    /// The shortcut (meaningful while `has_hotkey`).
    pub hotkey: GlobalHotkey,
    /// The widget theme this widget is PINNED to, or `None` to follow the
    /// app theme.
    pub theme: OptionUiTheme,
    /// Whether there is a shortcut.
    pub has_hotkey: bool,
    /// Whether the field listens for a shortcut.
    pub recording: bool,
}

impl ShortcutRecorder {
    /// A field with no shortcut, not listening.
    #[must_use]
    pub fn create() -> Self {
        Self {
            placeholder: AzString::from_const_str("None"),
            prompt: AzString::from_const_str("Press a shortcut..."),
            accessibility_name: AzString::from_const_str("Shortcut"),
            on_event: None.into(),
            hotkey: GlobalHotkey::create(HotkeyModifiers::NONE, VirtualKeyCode::Escape),
            theme: OptionUiTheme::None,
            has_hotkey: false,
            recording: false,
        }
    }

    /// The shortcut.
    pub fn set_hotkey(&mut self, hotkey: GlobalHotkey) {
        self.hotkey = hotkey;
        self.has_hotkey = true;
    }

    /// [`Self::set_hotkey`] for the builder chain.
    #[must_use]
    pub fn with_hotkey(mut self, hotkey: GlobalHotkey) -> Self {
        self.set_hotkey(hotkey);
        self
    }

    /// Removes the shortcut.
    pub fn clear_hotkey(&mut self) {
        self.has_hotkey = false;
    }

    /// Whether the field listens.
    pub const fn set_recording(&mut self, recording: bool) {
        self.recording = recording;
    }

    /// [`Self::set_recording`] for the builder chain.
    #[must_use]
    pub const fn with_recording(mut self, recording: bool) -> Self {
        self.set_recording(recording);
        self
    }

    /// The texts: with no shortcut, and while listening.
    pub fn set_texts(&mut self, placeholder: AzString, prompt: AzString) {
        self.placeholder = placeholder;
        self.prompt = prompt;
    }

    /// [`Self::set_texts`] for the builder chain.
    #[must_use]
    pub fn with_texts(mut self, placeholder: AzString, prompt: AzString) -> Self {
        self.set_texts(placeholder, prompt);
        self
    }

    /// What the field is called, for assistive technology.
    pub fn set_accessibility_name(&mut self, name: AzString) {
        self.accessibility_name = name;
    }

    /// [`Self::set_accessibility_name`] for the builder chain.
    #[must_use]
    pub fn with_accessibility_name(mut self, name: AzString) -> Self {
        self.set_accessibility_name(name);
        self
    }

    /// The callback that hears every request.
    pub fn set_on_event<C: Into<ShortcutRecorderOnEventCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.on_event = Some(ShortcutRecorderOnEvent {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<ShortcutRecorderOnEventCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// Pin the widget theme; unset, the field follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// The rule an app keeps the field with: listening starts on
    /// `StartRecording` and ends on everything else; `Recorded` sets the
    /// shortcut, `Cleared` removes it.
    pub fn apply(&mut self, event: ShortcutRecorderEvent) {
        if true {
            return;
        } // RED stub
        match event.kind {
            ShortcutRecorderEventKind::StartRecording => self.recording = true,
            ShortcutRecorderEventKind::Recorded => {
                self.hotkey = event.hotkey;
                self.has_hotkey = true;
                self.recording = false;
            }
            ShortcutRecorderEventKind::Cancelled => self.recording = false,
            ShortcutRecorderEventKind::Cleared => {
                self.has_hotkey = false;
                self.recording = false;
            }
        }
    }

    /// The text the field shows: the prompt while listening, else the
    /// shortcut, else the placeholder.
    #[must_use]
    pub fn display_text(&self) -> AzString {
        if true {
            return AzString::from_const_str("");
        } // RED stub
        if self.recording {
            self.prompt.clone()
        } else if self.has_hotkey {
            self.hotkey.to_display_string()
        } else {
            self.placeholder.clone()
        }
    }

    /// Replaces `self` with an empty field and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create();
        core::mem::swap(&mut s, self);
        s
    }

    /// The field's DOM.
    #[must_use]
    pub fn dom(self) -> Dom {
        let look = dialog_kit::look_for(self.theme);
        build(self, &look)
    }
}

impl Default for ShortcutRecorder {
    fn default() -> Self {
        Self::create()
    }
}

impl From<ShortcutRecorder> for Dom {
    fn from(r: ShortcutRecorder) -> Self {
        r.dom()
    }
}

// ---------------------------------------------------------------------------
// The handlers
// ---------------------------------------------------------------------------

/// What the field's handlers share.
struct RecorderRef {
    on_event: OptionShortcutRecorderOnEvent,
    hotkey: GlobalHotkey,
    has_hotkey: bool,
    recording: bool,
}

/// Hands `kind` (with `hotkey`) to the app's callback.
fn emit(
    on_event: &OptionShortcutRecorderOnEvent,
    info: CallbackInfo,
    kind: ShortcutRecorderEventKind,
    hotkey: GlobalHotkey,
) -> Update {
    match on_event.as_ref() {
        Some(ShortcutRecorderOnEvent { callback, refany }) => {
            callback.invoke(refany.clone(), info, ShortcutRecorderEvent { hotkey, kind })
        }
        None => Update::DoNothing,
    }
}

/// Whether `key` is a modifier: pressed alone it starts a chord, it is
/// never one.
const fn is_modifier(key: VirtualKeyCode) -> bool {
    matches!(
        key,
        VirtualKeyCode::LControl
            | VirtualKeyCode::RControl
            | VirtualKeyCode::LShift
            | VirtualKeyCode::RShift
            | VirtualKeyCode::LAlt
            | VirtualKeyCode::RAlt
            | VirtualKeyCode::LWin
            | VirtualKeyCode::RWin
    )
}

extern "C" fn on_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(r) = data.downcast_ref::<RecorderRef>() else {
        return Update::DoNothing;
    };
    if r.recording {
        return Update::DoNothing;
    }
    emit(
        &r.on_event,
        info,
        ShortcutRecorderEventKind::StartRecording,
        r.hotkey,
    )
}

/// Not listening: Enter / Space listen, Backspace / Delete clear. Listening:
/// Escape cancels, a modifier waits for its key, Tab leaves, anything else
/// with its modifiers is the shortcut.
extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let ks = info.get_current_keyboard_state();
    let Some(key) = ks.current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let plain = roving::plain_key(&ks);
    let Some(r) = data.downcast_ref::<RecorderRef>() else {
        return Update::DoNothing;
    };
    if !r.recording {
        return match plain {
            Some(VirtualKeyCode::Return | VirtualKeyCode::Space) => {
                info.prevent_default();
                emit(
                    &r.on_event,
                    info,
                    ShortcutRecorderEventKind::StartRecording,
                    r.hotkey,
                )
            }
            Some(VirtualKeyCode::Back | VirtualKeyCode::Delete) if r.has_hotkey => {
                info.prevent_default();
                emit(
                    &r.on_event,
                    info,
                    ShortcutRecorderEventKind::Cleared,
                    r.hotkey,
                )
            }
            _ => Update::DoNothing,
        };
    }
    if plain == Some(VirtualKeyCode::Escape) {
        info.prevent_default();
        return emit(
            &r.on_event,
            info,
            ShortcutRecorderEventKind::Cancelled,
            r.hotkey,
        );
    }
    let chord_held = ks.ctrl_down() || ks.alt_down() || ks.super_down();
    if is_modifier(key)
        || (key == VirtualKeyCode::Tab && !chord_held)
        || key_display_name(key).is_none()
    {
        return Update::DoNothing;
    }
    let hotkey = GlobalHotkey::create(
        HotkeyModifiers {
            ctrl: ks.ctrl_down(),
            alt: ks.alt_down(),
            shift: ks.shift_down(),
            meta: ks.super_down(),
        },
        key,
    );
    info.prevent_default();
    emit(
        &r.on_event,
        info,
        ShortcutRecorderEventKind::Recorded,
        hotkey,
    )
}

/// Leaving a listening field cancels.
extern "C" fn on_blur(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(r) = data.downcast_ref::<RecorderRef>() else {
        return Update::DoNothing;
    };
    if !r.recording {
        return Update::DoNothing;
    }
    emit(
        &r.on_event,
        info,
        ShortcutRecorderEventKind::Cancelled,
        r.hotkey,
    )
}

// ---------------------------------------------------------------------------
// The build
// ---------------------------------------------------------------------------

fn hook(event: EventFilter, cb: usize, refany: RefAny) -> CoreCallbackData {
    CoreCallbackData {
        event,
        callback: CoreCallback {
            cb,
            ctx: OptionRefAny::None,
        },
        refany,
    }
}

/// The field's DOM in `look`: field [text].
pub(crate) fn build(recorder: ShortcutRecorder, look: &DialogKitLook) -> Dom {
    if true {
        return Dom::create_div();
    } // RED stub
    let text = recorder.display_text();
    let ShortcutRecorder {
        placeholder: _,
        prompt: _,
        accessibility_name,
        on_event,
        hotkey,
        theme: _,
        has_hotkey,
        recording,
    } = recorder;
    let base = dialog_kit::part(FIELD_BASE, &look.recorder);
    let css = if recording {
        dialog_kit::stack_state(&base, &look.recorder_recording)
    } else {
        base
    };
    let mut classes = alloc::vec![azul_core::dom::IdOrClass::Class(AzString::from_const_str(
        RECORDER_CLASS
    ))];
    if recording {
        classes.push(azul_core::dom::IdOrClass::Class(AzString::from_const_str(
            RECORDING_CLASS,
        )));
    }
    if let Some(marker) = look.marker {
        classes.push(azul_core::dom::IdOrClass::Class(AzString::from_const_str(
            marker,
        )));
    }
    let shared = RefAny::new(RecorderRef {
        on_event,
        hotkey,
        has_hotkey,
        recording,
    });
    Dom::create_div()
        .with_ids_and_classes(azul_core::dom::IdOrClassVec::from_vec(classes))
        .with_css_props(css)
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo {
            accessibility_value: Some(text.clone()).into(),
            states: if recording {
                AccessibilityStateVec::from_vec(alloc::vec![AccessibilityState::Busy])
            } else {
                AccessibilityStateVec::from_const_slice(&[])
            },
            ..AccessibilityInfo::named(accessibility_name, AccessibilityRole::HotkeyField)
        })
        .with_callbacks(
            alloc::vec![
                hook(
                    EventFilter::Hover(HoverEventFilter::Click),
                    on_click as usize,
                    shared.clone()
                ),
                hook(
                    EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                    on_key as usize,
                    shared.clone()
                ),
                hook(
                    EventFilter::Focus(FocusEventFilter::FocusLost),
                    on_blur as usize,
                    shared
                ),
            ]
            .into(),
        )
        .with_child(dialog_kit::line(text, GROW_LABEL_BASE, &[]))
}

#[cfg(test)]
mod shortcut_recorder_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId, NodeType},
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::widgets::{
        roving::test_support as rv,
        themes::{theme_blocks::checks, theme_checks as tc},
    };

    type Log = Arc<Mutex<Vec<ShortcutRecorderEvent>>>;

    extern "C" fn record(
        mut data: RefAny,
        _: CallbackInfo,
        event: ShortcutRecorderEvent,
    ) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(event);
        }
        Update::RefreshDom
    }

    fn ctrl_shift_k() -> GlobalHotkey {
        GlobalHotkey::create(
            HotkeyModifiers {
                ctrl: true,
                alt: false,
                shift: true,
                meta: false,
            },
            VirtualKeyCode::K,
        )
    }

    fn recorder(log: &Log) -> ShortcutRecorder {
        ShortcutRecorder::create()
            .with_accessibility_name(AzString::from("Command palette"))
            .with_hotkey(ctrl_shift_k())
            .with_on_event(
                RefAny::new(log.clone()),
                record as ShortcutRecorderOnEventCallbackType,
            )
    }

    fn root() -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0))),
        }
    }

    fn texts(node: &Dom) -> Vec<String> {
        let mut out = Vec::new();
        for (_, n) in tc::nodes(node) {
            if let NodeType::Text(s) = n.root.get_node_type() {
                out.push(s.as_str().to_string());
            }
        }
        out
    }

    fn kinds(log: &Log) -> Vec<ShortcutRecorderEventKind> {
        log.lock().expect("log").iter().map(|e| e.kind).collect()
    }

    #[test]
    fn the_field_shows_the_shortcut_the_placeholder_or_the_prompt() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        assert_eq!(recorder(&log).display_text().as_str(), "Ctrl+Shift+K");
        assert_eq!(ShortcutRecorder::create().display_text().as_str(), "None");
        assert_eq!(
            recorder(&log).with_recording(true).display_text().as_str(),
            "Press a shortcut..."
        );
        for theme in checks::BOTH {
            let dom = recorder(&log).with_theme(theme).dom();
            assert_eq!(texts(&dom), vec!["Ctrl+Shift+K"], "{}", theme.name());
            let info = dom.root.get_accessibility_info().expect("a role");
            assert_eq!(info.role, AccessibilityRole::HotkeyField);
            assert_eq!(
                info.accessibility_name.as_ref().map(|s| s.as_str()),
                Some("Command palette")
            );
            assert_eq!(
                info.accessibility_value.as_ref().map(|s| s.as_str()),
                Some("Ctrl+Shift+K")
            );
            assert_eq!(
                dom.root.get_tab_index(),
                Some(TabIndex::Auto),
                "one Tab stop"
            );
            assert!(tc::has_focus_ring(&dom, false) && tc::has_focus_ring(&dom, true));
            let listening = recorder(&log).with_recording(true).with_theme(theme).dom();
            assert!(tc::has_class(&listening, RECORDING_CLASS));
            assert_eq!(texts(&listening), vec!["Press a shortcut..."]);
        }
    }

    #[test]
    fn a_click_or_enter_listens_and_backspace_clears() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(recorder(&log).with_theme(UiTheme::Flat).dom());
        rv::fire(&styled, root(), EventFilter::Hover(HoverEventFilter::Click)).expect("click");
        let (_, changes) = rv::press(&styled, root(), VirtualKeyCode::Return, &[]).expect("key");
        assert!(rv::prevented(&changes));
        rv::press(&styled, root(), VirtualKeyCode::Back, &[]).expect("key");
        rv::press(&styled, root(), VirtualKeyCode::K, &[]).expect("key");
        assert_eq!(
            kinds(&log),
            vec![
                ShortcutRecorderEventKind::StartRecording,
                ShortcutRecorderEventKind::StartRecording,
                ShortcutRecorderEventKind::Cleared
            ],
            "a bare K does nothing while the field does not listen"
        );
    }

    #[test]
    fn a_listening_field_records_the_chord_and_escape_or_leaving_cancels() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(
            ShortcutRecorder::create()
                .with_recording(true)
                .with_on_event(
                    RefAny::new(log.clone()),
                    record as ShortcutRecorderOnEventCallbackType,
                )
                .with_theme(UiTheme::Flat)
                .dom(),
        );
        rv::press(
            &styled,
            root(),
            VirtualKeyCode::LShift,
            &[VirtualKeyCode::LControl],
        )
        .expect("key");
        rv::press(&styled, root(), VirtualKeyCode::Tab, &[]).expect("key");
        assert!(
            log.lock().expect("log").is_empty(),
            "a modifier waits, Tab leaves"
        );
        let (_, changes) = rv::press(
            &styled,
            root(),
            VirtualKeyCode::K,
            &[VirtualKeyCode::LControl, VirtualKeyCode::LShift],
        )
        .expect("key");
        assert!(rv::prevented(&changes), "the chord is the field's");
        let got = log.lock().expect("log").clone();
        assert_eq!(got[0].kind, ShortcutRecorderEventKind::Recorded);
        assert_eq!(got[0].hotkey, ctrl_shift_k());
        rv::press(&styled, root(), VirtualKeyCode::Escape, &[]).expect("key");
        rv::fire(
            &styled,
            root(),
            EventFilter::Focus(FocusEventFilter::FocusLost),
        )
        .expect("blur");
        assert_eq!(
            kinds(&log)[1..],
            [
                ShortcutRecorderEventKind::Cancelled,
                ShortcutRecorderEventKind::Cancelled
            ]
        );
        rv::press(&styled, root(), VirtualKeyCode::F5, &[]).expect("key");
        assert_eq!(
            log.lock().expect("log").last().map(|e| e.hotkey.key),
            Some(VirtualKeyCode::F5),
            "a function key stands alone"
        );
    }

    #[test]
    fn apply_keeps_the_shortcut_and_the_listening_state() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let mut r = recorder(&log);
        let event = |kind, hotkey| ShortcutRecorderEvent { hotkey, kind };
        r.apply(event(ShortcutRecorderEventKind::StartRecording, r.hotkey));
        assert!(r.recording);
        let f5 = GlobalHotkey::create(HotkeyModifiers::NONE, VirtualKeyCode::F5);
        r.apply(event(ShortcutRecorderEventKind::Recorded, f5));
        assert!(!r.recording && r.has_hotkey && r.hotkey == f5);
        r.apply(event(ShortcutRecorderEventKind::Cleared, f5));
        assert!(!r.has_hotkey);
        assert_eq!(r.display_text().as_str(), "None");
    }

    #[test]
    fn an_unpinned_recorder_follows_the_app_theme() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        checks::assert_follows_the_app_theme(
            "shortcut_recorder",
            || recorder(&log).with_recording(true).dom(),
            |t: UiTheme| recorder(&log).with_recording(true).with_theme(t).dom(),
        );
    }
}
