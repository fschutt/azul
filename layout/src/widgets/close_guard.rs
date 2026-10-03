//! Close guard - "Save changes?" before a window with unsaved work closes
//! (DEDUP_OFFICE N14 / A16: no Office app asked).
//!
//! The guard wraps the window's content. While the app says its document is
//! [`CloseGuard::dirty`], a close request (the title bar's close button,
//! Alt+F4, the app's own `close_window`) is VETOED
//! (`CallbackInfo::prevent_window_close`) and the app hears
//! [`CloseGuardEventKind::Ask`]: it sets [`CloseGuard::asking`] and rebuilds,
//! and the guard shows the standard question over the window - a
//! `MessageBox` (Question: Save / Don't Save / Cancel) in a `Modal`. The
//! answer comes back as one event:
//!
//! - [`CloseGuardEventKind::Save`]: the app saves, then closes the window
//!   (`close_window` - once the save is done, when it runs on a thread);
//! - [`CloseGuardEventKind::Discard`]: the app drops its changes (`dirty`
//!   false) and the guard closes the window;
//! - [`CloseGuardEventKind::Cancel`] (the button, Escape, the modal's close):
//!   the app clears `asking`; the window stays.
//!
//! A clean document closes at once; a guard without `on_event` never holds
//! a close (nobody could answer). The guard owns nothing: the app keeps
//! `dirty` and `asking`.
//!
//! `dirty` is what the app knew when it built the DOM. An app whose
//! document can change state between builds and the close (saved and
//! closed in one callback, a save that lands on a thread) gives the guard a
//! [`CloseGuard::with_dirty_check`] callback instead: the guard asks it
//! when the close request arrives ([`CloseGuardDocumentState`]), and the
//! static flag is not read.
//!
//! Every backend delivers every close through the same protocol (the window
//! manager's close button, Alt+F4, `close_window`, the CSD titlebar's close
//! button), and judges it by the DOM the app's LAST callback asked for: a
//! rebuild that callback requested is built first. So after Save or Discard
//! the app clears `dirty` before, or in the same callback as, the
//! `close_window` - a thread's writeback can mark the document saved, return
//! `RefreshDom` and close in one go.
//!
//! ```text
//! body
//!  └ CloseGuard (CloseRequested -> veto + Ask while dirty)
//!     ├ the window's content
//!     └ Modal (open while asking)
//!        └ MessageBox  ? Save changes to "Report"?
//!                        Your changes will be lost if you don't save them.
//!                        [Save] [Don't Save] [Cancel]
//! ```
//!
//! Key types: [`CloseGuard`], [`CloseGuardEvent`], [`CloseGuardOnEvent`].

use alloc::{format, vec::Vec};

use azul_core::{
    callbacks::{CoreCallback, Update},
    dom::{Dom, DomVec, EventFilter},
    events::WindowEventFilter,
    refany::{OptionRefAny, RefAny},
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option_inner,
    props::{
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow, LayoutMinHeight},
        property::CssProperty,
    },
    AzString, StringVec,
};

use crate::{
    callbacks::CallbackInfo,
    widgets::{
        modal::{Modal, ModalOnCloseCallbackType, ModalState},
        standard_dialogs::{
            MessageBox, MessageBoxKind, StandardDialogEvent, StandardDialogEventKind,
            StandardDialogOnEventCallbackType,
        },
        themes::{OptionUiTheme, UiTheme},
    },
};

/// The guard's class (the content's wrapper).
pub const CLOSE_GUARD_CLASS: &str = "__azul-native-close-guard";

/// What the user asked of the guard.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CloseGuardEventKind {
    /// A close was requested while the document is dirty, and held: show
    /// the question (`asking`).
    Ask,
    /// "Save": save, then close the window.
    Save,
    /// "Don't Save": the window closes; drop the changes.
    Discard,
    /// "Cancel" (or Escape, or the modal's close): the window stays; hide
    /// the question.
    Cancel,
}

/// One answer of the guard.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CloseGuardEvent {
    /// What was asked.
    pub kind: CloseGuardEventKind,
}

impl CloseGuardEvent {
    /// An event of `kind`.
    #[must_use]
    pub const fn create(kind: CloseGuardEventKind) -> Self {
        Self { kind }
    }
}

/// Callback invoked for every answer of the guard.
pub type CloseGuardOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, CloseGuardEvent) -> Update;
impl_widget_callback!(
    CloseGuardOnEvent,
    OptionCloseGuardOnEvent,
    CloseGuardOnEventCallback,
    CloseGuardOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        CloseGuardOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: CLOSE_GUARD_ON_EVENT_INVOKER,
    invoker_ty:     AzCloseGuardOnEventCallbackInvoker,
    thunk_fn:       az_close_guard_on_event_callback_thunk,
    setter_fn:      AzApp_setCloseGuardOnEventCallbackInvoker,
    from_handle_fn: AzCloseGuardOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzCloseGuardOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: CloseGuardEvent ],
}

/// The app's answer when the guard asks, at the moment a close is
/// requested, whether the document has unsaved work.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum CloseGuardDocumentState {
    /// Nothing unsaved: the window closes.
    #[default]
    Saved,
    /// Unsaved work: the close is held and the app hears `Ask`.
    Unsaved,
}

impl azul_core::host_invoker::HostOut for CloseGuardDocumentState {
    /// A host that does not answer holds nothing: the window can close.
    fn unwritten() -> Self {
        Self::Saved
    }
}

/// Callback the guard asks when a close is requested: is the document
/// dirty NOW? (The `dirty` flag is what the app knew when it built the
/// DOM; this is what it knows when the close arrives.)
pub type CloseGuardDirtyCheckCallbackType =
    extern "C" fn(RefAny, CallbackInfo) -> CloseGuardDocumentState;
impl_widget_callback!(
    CloseGuardDirtyCheck,
    OptionCloseGuardDirtyCheck,
    CloseGuardDirtyCheckCallback,
    CloseGuardDirtyCheckCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        CloseGuardDirtyCheckCallback,
    info_ty:        CallbackInfo,
    return_ty:      CloseGuardDocumentState,
    default_ret:    CloseGuardDocumentState::Saved,
    invoker_static: CLOSE_GUARD_DIRTY_CHECK_INVOKER,
    invoker_ty:     AzCloseGuardDirtyCheckCallbackInvoker,
    thunk_fn:       az_close_guard_dirty_check_callback_thunk,
    setter_fn:      AzApp_setCloseGuardDirtyCheckCallbackInvoker,
    from_handle_fn: AzCloseGuardDirtyCheckCallback_createFromHostHandle,
    from_handle_byref_fn: AzCloseGuardDirtyCheckCallback_createFromHostHandleByref,
}

/// The "Save changes?" guard around a window's content.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct CloseGuard {
    /// The window's content.
    pub content: Dom,
    /// The question's window title: the app's name ("AzShow").
    pub title: AzString,
    /// The question ("Save changes to \"Report\"?").
    pub question: AzString,
    /// The line under it ("Your changes will be lost if you don't save
    /// them.").
    pub text: AzString,
    /// The buttons: "Save", "Don't Save", "Cancel".
    pub save_label: AzString,
    /// See `save_label`.
    pub discard_label: AzString,
    /// See `save_label`.
    pub cancel_label: AzString,
    /// Every answer.
    pub on_event: OptionCloseGuardOnEvent,
    /// Asked when a close is requested: does the document have unsaved
    /// work now? Set, it decides instead of `dirty`.
    pub dirty_check: OptionCloseGuardDirtyCheck,
    /// The widget theme the question is PINNED to, or `None` to follow the
    /// app theme.
    pub theme: OptionUiTheme,
    /// The document has unsaved work: a close request is held and asked.
    pub dirty: bool,
    /// The question is shown.
    pub asking: bool,
}

/// The wrapper: a column that fills the window like the content would.
static CLOSE_GUARD_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
];

impl CloseGuard {
    /// A guard around `content` whose work is the document `document`
    /// ("Report"): not dirty, not asking, the English question and buttons.
    #[must_use]
    pub fn create(content: Dom, document: AzString) -> Self {
        Self {
            content,
            title: AzString::from_const_str(""),
            question: AzString::from(format!("Save changes to \"{}\"?", document.as_str())),
            text: AzString::from_const_str("Your changes will be lost if you don't save them."),
            save_label: AzString::from_const_str("Save"),
            discard_label: AzString::from_const_str("Don't Save"),
            cancel_label: AzString::from_const_str("Cancel"),
            on_event: None.into(),
            dirty_check: None.into(),
            theme: OptionUiTheme::None,
            dirty: false,
            asking: false,
        }
    }

    /// The document has unsaved work - as known when the DOM is built. For
    /// a document that can be saved and closed in one callback, use
    /// [`Self::set_dirty_check`]: it is asked when the close arrives.
    pub const fn set_dirty(&mut self, dirty: bool) {
        self.dirty = dirty;
    }

    /// [`Self::set_dirty`] for the builder chain.
    #[must_use]
    pub const fn with_dirty(mut self, dirty: bool) -> Self {
        self.set_dirty(dirty);
        self
    }

    /// The callback asked when a close is requested: does the document have
    /// unsaved work NOW? It decides instead of [`Self::set_dirty`], so an
    /// app that saves and closes in one callback is not held by the state
    /// its DOM was built with.
    pub fn set_dirty_check<C: Into<CloseGuardDirtyCheckCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.dirty_check = Some(CloseGuardDirtyCheck {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// [`Self::set_dirty_check`] for the builder chain.
    #[must_use]
    pub fn with_dirty_check<C: Into<CloseGuardDirtyCheckCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_dirty_check(data, callback);
        self
    }

    /// The question is shown (after `Ask`, until an answer).
    pub const fn set_asking(&mut self, asking: bool) {
        self.asking = asking;
    }

    /// [`Self::set_asking`] for the builder chain.
    #[must_use]
    pub const fn with_asking(mut self, asking: bool) -> Self {
        self.set_asking(asking);
        self
    }

    /// The question's window title (the app's name).
    pub fn set_title(&mut self, title: AzString) {
        self.title = title;
    }

    /// [`Self::set_title`] for the builder chain.
    #[must_use]
    pub fn with_title(mut self, title: AzString) -> Self {
        self.set_title(title);
        self
    }

    /// The question and the line under it (another language).
    pub fn set_question(&mut self, question: AzString, text: AzString) {
        self.question = question;
        self.text = text;
    }

    /// [`Self::set_question`] for the builder chain.
    #[must_use]
    pub fn with_question(mut self, question: AzString, text: AzString) -> Self {
        self.set_question(question, text);
        self
    }

    /// The three buttons' labels.
    pub fn set_labels(&mut self, save: AzString, discard: AzString, cancel: AzString) {
        self.save_label = save;
        self.discard_label = discard;
        self.cancel_label = cancel;
    }

    /// [`Self::set_labels`] for the builder chain.
    #[must_use]
    pub fn with_labels(mut self, save: AzString, discard: AzString, cancel: AzString) -> Self {
        self.set_labels(save, discard, cancel);
        self
    }

    /// The callback that hears every answer.
    pub fn set_on_event<C: Into<CloseGuardOnEventCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_event = Some(CloseGuardOnEvent {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<CloseGuardOnEventCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// Pin the widget theme of the question; unset, it follows the app
    /// theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty guard and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    /// The guard's DOM: the content in a column that holds the window's
    /// close requests, and the question while `asking`.
    #[must_use]
    pub fn dom(self) -> Dom {
        build(self)
    }
}

impl Default for CloseGuard {
    fn default() -> Self {
        Self::create(Dom::create_div(), AzString::from_const_str(""))
    }
}

impl From<CloseGuard> for Dom {
    fn from(g: CloseGuard) -> Self {
        g.dom()
    }
}

// ---------------------------------------------------------------------------
// The behaviour
// ---------------------------------------------------------------------------

/// What every part of one guard shares: the app's callback, whether the
/// document was dirty when the window was built, and whether an answer
/// already let the window go (a `close_window` from that answer then
/// passes).
struct GuardRef {
    on_event: OptionCloseGuardOnEvent,
    dirty_check: OptionCloseGuardDirtyCheck,
    dirty: bool,
    confirmed: bool,
}

fn build(guard: CloseGuard) -> Dom {
    let CloseGuard {
        content,
        title,
        question,
        text,
        save_label,
        discard_label,
        cancel_label,
        on_event,
        dirty_check,
        theme,
        dirty,
        asking,
    } = guard;
    let theme = theme.into_option();
    let shared = RefAny::new(GuardRef {
        on_event,
        dirty_check,
        dirty,
        confirmed: false,
    });
    let mut children: Vec<Dom> = Vec::with_capacity(2);
    children.push(content);
    if asking {
        let mut message = MessageBox::create(MessageBoxKind::Question, question, text)
            .with_buttons(
                StringVec::from_vec(alloc::vec![save_label, discard_label, cancel_label]),
                0,
            )
            .with_on_event(
                shared.clone(),
                on_answer as StandardDialogOnEventCallbackType,
            );
        if let Some(t) = theme {
            message = message.with_theme(t);
        }
        let mut modal = Modal::create(message.dom())
            .with_title(title)
            .with_open(true)
            .with_on_close(shared.clone(), on_modal_close as ModalOnCloseCallbackType);
        if let Some(t) = theme {
            modal = modal.with_theme(t);
        }
        children.push(modal.dom());
    }
    Dom::create_div()
        .with_class(AzString::from_const_str(CLOSE_GUARD_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
            CLOSE_GUARD_BASE,
        ))
        .with_callback(
            EventFilter::Window(WindowEventFilter::CloseRequested),
            shared,
            CoreCallback {
                cb: on_close_requested as usize,
                ctx: OptionRefAny::None,
            },
        )
        .with_children(DomVec::from_vec(children))
}

/// Hands `kind` to the app's callback.
fn report(
    on_event: &OptionCloseGuardOnEvent,
    info: CallbackInfo,
    kind: CloseGuardEventKind,
) -> Update {
    match on_event.as_ref() {
        Some(CloseGuardOnEvent { callback, refany }) => {
            callback.invoke(refany.clone(), info, CloseGuardEvent::create(kind))
        }
        None => Update::DoNothing,
    }
}

/// The window is asked to close: held while the document is dirty (and
/// someone listens), and the app asked. "Dirty" is the app's answer NOW
/// when it gave a dirty check, else the flag the DOM was built with.
extern "C" fn on_close_requested(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (on_event, dirty_check, dirty) = {
        let Some(g) = data.downcast_ref::<GuardRef>() else {
            return Update::DoNothing;
        };
        if g.confirmed || g.on_event.is_none() {
            return Update::DoNothing;
        }
        (g.on_event.clone(), g.dirty_check.clone(), g.dirty)
    };
    let dirty = match dirty_check.as_ref() {
        Some(CloseGuardDirtyCheck { refany, callback }) => {
            callback.invoke(refany.clone(), info) == CloseGuardDocumentState::Unsaved
        }
        None => dirty,
    };
    if !dirty {
        return Update::DoNothing;
    }
    let update = report(&on_event, info, CloseGuardEventKind::Ask);
    // Last: the veto rides on whatever window state the app queued.
    info.prevent_window_close();
    update
}

/// An answer: button 0 saves, 1 discards, 2 cancels.
extern "C" fn on_answer(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: StandardDialogEvent,
) -> Update {
    let kind = match (event.kind, event.index) {
        (StandardDialogEventKind::Button, 0) => CloseGuardEventKind::Save,
        (StandardDialogEventKind::Button, 1) => CloseGuardEventKind::Discard,
        (StandardDialogEventKind::Button | StandardDialogEventKind::Cancel, _) => {
            CloseGuardEventKind::Cancel
        }
        _ => return Update::DoNothing,
    };
    let on_event = {
        let Some(mut g) = data.downcast_mut::<GuardRef>() else {
            return Update::DoNothing;
        };
        if kind != CloseGuardEventKind::Cancel {
            // The close this answer leads to (the guard's own, or the app's
            // after its save) passes the guard.
            g.confirmed = true;
        }
        g.on_event.clone()
    };
    let update = report(&on_event, info, kind);
    if kind == CloseGuardEventKind::Discard {
        info.close_window();
    }
    update
}

/// The question's modal closed (Escape, its close button): Cancel.
extern "C" fn on_modal_close(mut data: RefAny, info: CallbackInfo, _state: ModalState) -> Update {
    let on_event = match data.downcast_ref::<GuardRef>() {
        Some(g) => g.on_event.clone(),
        None => return Update::DoNothing,
    };
    report(&on_event, info, CloseGuardEventKind::Cancel)
}

#[cfg(test)]
mod close_guard_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, HoverEventFilter, NodeType},
        id::NodeId,
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::{
        callbacks::CallbackChange,
        widgets::{roving::test_support as rv, themes::theme_checks as tc},
    };

    type Log = Arc<Mutex<Vec<CloseGuardEventKind>>>;

    extern "C" fn record(mut data: RefAny, _info: CallbackInfo, event: CloseGuardEvent) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(event.kind);
        }
        Update::RefreshDom
    }

    fn guard(log: &Log, dirty: bool, asking: bool) -> CloseGuard {
        CloseGuard::create(
            Dom::create_div().with_child(Dom::create_p_with_text("the document")),
            AzString::from("Report"),
        )
        .with_title(AzString::from("AzWriter"))
        .with_dirty(dirty)
        .with_asking(asking)
        .with_on_event(
            RefAny::new(log.clone()),
            record as CloseGuardOnEventCallbackType,
        )
        .with_theme(UiTheme::Flat)
    }

    fn node(index: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
        }
    }

    const CLOSE: EventFilter = EventFilter::Window(WindowEventFilter::CloseRequested);

    /// Whether `changes` hold the window open: a queued window state with
    /// the close request cleared.
    fn vetoed(changes: &[CallbackChange]) -> bool {
        changes.iter().any(|c| {
            matches!(c, CallbackChange::ModifyWindowState { state } if !state.flags.close_requested)
        })
    }

    fn closes(changes: &[CallbackChange]) -> bool {
        changes
            .iter()
            .any(|c| matches!(c, CallbackChange::CloseWindow))
    }

    fn texts(dom: &Dom) -> Vec<String> {
        tc::nodes(dom)
            .into_iter()
            .filter_map(|(_, n)| match n.root.get_node_type() {
                NodeType::Text(s) if !s.as_str().is_empty() => Some(s.as_str().to_string()),
                _ => None,
            })
            .collect()
    }

    /// The node that takes the click on the button reading `label`.
    fn button(styled: &StyledDom, label: &str) -> DomNodeId {
        let hierarchy = styled.node_hierarchy.as_ref();
        let nodes = styled.node_data.as_ref();
        let text = nodes
            .iter()
            .position(|n| matches!(n.get_node_type(), NodeType::Text(s) if s.as_str() == label))
            .unwrap_or_else(|| panic!("no text {label:?}"));
        let mut at = Some(NodeId::new(text));
        while let Some(n) = at {
            if nodes[n.index()]
                .get_callbacks()
                .as_ref()
                .iter()
                .any(|c| c.event == EventFilter::Hover(HoverEventFilter::Click))
            {
                return node(n.index());
            }
            at = hierarchy[n.index()].parent_id();
        }
        panic!("the {label:?} button takes no click");
    }

    fn click(styled: &StyledDom, label: &str) -> Vec<CallbackChange> {
        rv::fire(
            styled,
            button(styled, label),
            EventFilter::Hover(HoverEventFilter::Click),
        )
        .expect("the button takes the click")
        .1
    }

    #[test]
    fn a_close_request_on_a_dirty_window_is_held_and_the_app_is_asked() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(guard(&log, true, false).dom());
        let (_, changes) = rv::fire(&styled, node(0), CLOSE).expect("the guard hears the close");
        assert!(vetoed(&changes), "the window stays open");
        assert_eq!(
            log.lock().expect("log").clone(),
            vec![CloseGuardEventKind::Ask]
        );
    }

    #[test]
    fn a_clean_window_closes_without_a_question() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(guard(&log, false, false).dom());
        let changes = rv::fire(&styled, node(0), CLOSE)
            .map(|(_, c)| c)
            .unwrap_or_default();
        assert!(!vetoed(&changes));
        assert!(log.lock().expect("log").is_empty());
    }

    #[test]
    fn asking_shows_the_save_question_with_save_dont_save_and_cancel() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let quiet = texts(&guard(&log, true, false).dom());
        assert!(
            !quiet.iter().any(|t| t == "Don't Save"),
            "no question until asked"
        );
        let shown = texts(&guard(&log, true, true).dom());
        assert!(
            shown.iter().any(|t| t == "the document"),
            "the content stays"
        );
        assert!(shown.iter().any(|t| t.contains("\"Report\"")), "{shown:?}");
        for label in ["Save", "Don't Save", "Cancel"] {
            assert!(shown.iter().any(|t| t == label), "{label} in {shown:?}");
        }
        let dom = guard(&log, true, true).dom();
        assert!(tc::find(&dom, "__azul-native-message-box").is_some());
    }

    #[test]
    fn dont_save_reports_discard_closes_the_window_and_the_close_then_passes() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(guard(&log, true, true).dom());
        let changes = click(&styled, "Don't Save");
        assert!(closes(&changes), "Don't Save closes the window");
        let changes = rv::fire(&styled, node(0), CLOSE)
            .map(|(_, c)| c)
            .unwrap_or_default();
        assert!(
            !vetoed(&changes),
            "the close it asked for is not held again"
        );
        assert_eq!(
            log.lock().expect("log").clone(),
            vec![CloseGuardEventKind::Discard]
        );
    }

    #[test]
    fn save_reports_save_and_lets_the_apps_close_through() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(guard(&log, true, true).dom());
        let changes = click(&styled, "Save");
        assert!(!closes(&changes), "the app closes once it has saved");
        let changes = rv::fire(&styled, node(0), CLOSE)
            .map(|(_, c)| c)
            .unwrap_or_default();
        assert!(!vetoed(&changes), "a close after Save is not held again");
        assert_eq!(
            log.lock().expect("log").clone(),
            vec![CloseGuardEventKind::Save]
        );
    }

    #[test]
    fn cancel_reports_cancel_and_the_window_stays() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(guard(&log, true, true).dom());
        let changes = click(&styled, "Cancel");
        assert!(!closes(&changes));
        assert_eq!(
            log.lock().expect("log").clone(),
            vec![CloseGuardEventKind::Cancel]
        );
        let (_, changes) = rv::fire(&styled, node(0), CLOSE).expect("still guarded");
        assert!(vetoed(&changes), "still dirty: the next close asks again");
    }

    #[test]
    fn a_guard_nobody_listens_to_never_holds_a_close() {
        let styled = StyledDom::create_from_dom(
            CloseGuard::create(Dom::create_div(), AzString::from("Report"))
                .with_dirty(true)
                .dom(),
        );
        let changes = rv::fire(&styled, node(0), CLOSE)
            .map(|(_, c)| c)
            .unwrap_or_default();
        assert!(!vetoed(&changes));
    }

    /// What the app knows now: its document's dirty flag, shared with the
    /// guard's check.
    type Dirty = Arc<Mutex<bool>>;

    extern "C" fn dirty_now(mut data: RefAny, _info: CallbackInfo) -> CloseGuardDocumentState {
        let dirty = data
            .downcast_ref::<Dirty>()
            .is_some_and(|d| *d.lock().expect("dirty"));
        if dirty {
            CloseGuardDocumentState::Unsaved
        } else {
            CloseGuardDocumentState::Saved
        }
    }

    fn checked_guard(log: &Log, dirty: &Dirty, built_dirty: bool) -> CloseGuard {
        guard(log, built_dirty, false).with_dirty_check(
            RefAny::new(dirty.clone()),
            dirty_now as CloseGuardDirtyCheckCallbackType,
        )
    }

    #[test]
    fn a_document_saved_after_the_dom_was_built_closes_without_a_question() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dirty: Dirty = Arc::new(Mutex::new(true));
        // Built while dirty, then saved in the callback that also closes.
        let styled = StyledDom::create_from_dom(checked_guard(&log, &dirty, true).dom());
        *dirty.lock().expect("dirty") = false;
        let changes = rv::fire(&styled, node(0), CLOSE)
            .map(|(_, c)| c)
            .unwrap_or_default();
        assert!(!vetoed(&changes), "the saved document is not held");
        assert!(log.lock().expect("log").is_empty(), "nobody is asked");
    }

    #[test]
    fn a_document_changed_after_the_dom_was_built_holds_the_close_and_asks() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dirty: Dirty = Arc::new(Mutex::new(false));
        let styled = StyledDom::create_from_dom(checked_guard(&log, &dirty, false).dom());
        *dirty.lock().expect("dirty") = true;
        let (_, changes) = rv::fire(&styled, node(0), CLOSE).expect("the guard hears the close");
        assert!(vetoed(&changes), "the unsaved document is held");
        assert_eq!(
            log.lock().expect("log").clone(),
            vec![CloseGuardEventKind::Ask]
        );
    }

    #[test]
    fn the_dirty_check_still_lets_an_answered_close_through() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dirty: Dirty = Arc::new(Mutex::new(true));
        let styled =
            StyledDom::create_from_dom(checked_guard(&log, &dirty, true).with_asking(true).dom());
        let changes = click(&styled, "Don't Save");
        assert!(closes(&changes), "Don't Save closes the window");
        let changes = rv::fire(&styled, node(0), CLOSE)
            .map(|(_, c)| c)
            .unwrap_or_default();
        assert!(
            !vetoed(&changes),
            "the close it asked for is not held again"
        );
    }

    extern "C" fn retitle_then_veto(_data: RefAny, mut info: CallbackInfo) -> Update {
        let mut state = info.get_current_window_state().clone();
        state.title = AzString::from("Saving");
        info.modify_window_state(state);
        info.prevent_window_close();
        Update::DoNothing
    }

    #[test]
    fn prevent_window_close_keeps_what_the_callback_queued_before_it() {
        let dom = Dom::create_div().with_callback(
            CLOSE,
            RefAny::new(()),
            CoreCallback {
                cb: retitle_then_veto as usize,
                ctx: OptionRefAny::None,
            },
        );
        let styled = StyledDom::create_from_dom(dom);
        let (_, changes) = rv::fire(&styled, node(0), CLOSE).expect("the callback runs");
        let last = changes
            .iter()
            .rev()
            .find_map(|c| match c {
                CallbackChange::ModifyWindowState { state } => Some(state.clone()),
                _ => None,
            })
            .expect("a queued window state");
        assert!(!last.flags.close_requested, "the close is vetoed");
        assert_eq!(last.title.as_str(), "Saving", "the earlier change is kept");
    }
}
