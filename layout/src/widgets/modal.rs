//! Modal widget — HTML `<dialog>` shown with `showModal()`: a titled panel
//! holding arbitrary content, centred over a dimmed `::backdrop` that covers
//! the whole window, with the rest of the window inert while it is open.
//! (Not the native OS file / message dialogs - those live in the `dialog`
//! module of the desktop crate.)
//!
//! A front-end over [`crate::widgets::dialog`]: the modal is a
//! `<transient-window anchor="viewport">` - a real OS window exactly over its
//! parent, so it is in the top layer by construction. Everything the old
//! in-window overlay could not do, it does:
//!
//! - **Escape closes it** (a close request, HTML `closedby` default for a modal dialog). Use
//!   [`crate::widgets::dialog::Dialog`] for a cancelable `cancel`, another `closedby`, a return
//!   value, or an invoker.
//! - **The background is inert**: the window covers the parent, and holds the keyboard.
//! - **Focus** moves to the modal's first control when it opens and back when it closes.
//! - **It is announced** as a dialog, named by its title.
//!
//! Why it is not a `display`-toggled overlay any more (2026-09-28): its close
//! button hid the backdrop with a runtime `display: none` override, which
//! survives the app's rebuilds - so a modal the app reopened later stayed
//! hidden.
//!
//! Key types: [`Modal`], [`ModalState`], [`ModalOnClose`].

use azul_core::{
    callbacks::Update,
    dom::{Dom, IdOrClass, IdOrClass::Class},
    refany::RefAny,
    transient::TransientAnchor,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditionsVec, OptionCssPropertyWithConditionsVec},
    impl_option_inner, AzString,
};

use crate::{
    callbacks::CallbackInfo,
    widgets::dialog::{
        build_dialog, default_backdrop_style, DialogClasses, DialogClosedBy, DialogCompat,
        DialogParts, OptionDialogOnCancel, OptionDialogOnClose,
    },
};

static MODAL_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-modal"))];
static MODAL_PANEL_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-modal-panel"))];
static MODAL_TITLE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-modal-title"))];
static MODAL_CLOSE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-modal-close"))];
static MODAL_CONTENT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-modal-content",
))];

/// Callback invoked when the modal closes - by its "×", by Escape. The
/// [`ModalState`] carries the *new* (`false`) open value.
pub type ModalOnCloseCallbackType = extern "C" fn(RefAny, CallbackInfo, ModalState) -> Update;
impl_widget_callback!(
    ModalOnClose,
    OptionModalOnClose,
    ModalOnCloseCallback,
    ModalOnCloseCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ModalOnCloseCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: MODAL_ON_CLOSE_INVOKER,
    invoker_ty:     AzModalOnCloseCallbackInvoker,
    thunk_fn:       az_modal_on_close_callback_thunk,
    setter_fn:      AzApp_setModalOnCloseCallbackInvoker,
    from_handle_fn: AzModalOnCloseCallback_createFromHostHandle,
    from_handle_byref_fn: AzModalOnCloseCallback_createFromHostHandleByref,
    extra_args:     [ state: ModalState ],
}

/// A modal dialog holding arbitrary content, with an optional title and
/// close button.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Modal {
    /// The declared `open` plus the optional close callback.
    pub modal_state: ModalStateWrapper,
    /// The dialog title (empty = no title row; the dialog is then named
    /// "Dialog").
    pub title: AzString,
    /// The arbitrary content shown inside the panel.
    pub content: Dom,
    /// Whether to render the "×" close button.
    pub show_close_button: bool,
    /// Style of the `::backdrop` - the modal window's root, which fills the
    /// window and centres the panel - or `None` for the default dim.
    pub backdrop_style: OptionCssPropertyWithConditionsVec,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct ModalStateWrapper {
    /// Whether the dialog is declared open.
    pub inner: ModalState,
    /// Optional: function to call when the dialog is closed.
    pub on_close: OptionModalOnClose,
}

/// The open/closed state of a [`Modal`].
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct ModalState {
    /// `true` = dialog shown, `false` (default) = dialog hidden.
    pub open: bool,
}

impl Modal {
    /// Creates a new (closed) modal holding `content`, with a "×" close button
    /// and no title.
    #[must_use]
    pub fn create(content: Dom) -> Self {
        Self {
            modal_state: ModalStateWrapper::default(),
            title: AzString::from_const_str(""),
            content,
            show_close_button: true,
            backdrop_style: OptionCssPropertyWithConditionsVec::None,
        }
    }

    /// The `::backdrop` CSS this modal renders with.
    ///
    /// `None` means no opinion, so the default dim applies. Open or closed is
    /// not a style: the backdrop is the root of the modal's own window.
    #[must_use]
    pub fn resolved_backdrop_style(&self) -> CssPropertyWithConditionsVec {
        self.backdrop_style
            .clone()
            .into_option()
            .unwrap_or_else(default_backdrop_style)
    }

    /// Sets the dialog title (empty = no title).
    #[inline]
    pub fn set_title(&mut self, title: AzString) {
        self.title = title;
    }

    /// Builder-style setter for the title.
    #[inline]
    #[must_use]
    pub fn with_title(mut self, title: AzString) -> Self {
        self.set_title(title);
        self
    }

    /// Replaces the content shown inside the panel.
    #[inline]
    pub fn set_content(&mut self, content: Dom) {
        self.content = content;
    }

    /// Builder-style setter for the content.
    #[inline]
    #[must_use]
    pub fn with_content(mut self, content: Dom) -> Self {
        self.set_content(content);
        self
    }

    /// Declares the dialog open. A CHANGE of it shows or closes the dialog;
    /// while it stays `true`, a dialog the user closed stays closed until it
    /// goes `false` and `true` again - drop the flag in `on_close`.
    #[inline]
    pub const fn set_open(&mut self, open: bool) {
        self.modal_state.inner.open = open;
    }

    /// Builder-style setter for the declared open state.
    #[inline]
    #[must_use]
    pub const fn with_open(mut self, open: bool) -> Self {
        self.set_open(open);
        self
    }

    /// Sets whether the "×" close button is shown.
    #[inline]
    pub const fn set_close_button(&mut self, show: bool) {
        self.show_close_button = show;
    }

    /// Builder-style setter for the close-button flag.
    #[inline]
    #[must_use]
    pub const fn with_close_button(mut self, show: bool) -> Self {
        self.set_close_button(show);
        self
    }

    /// Sets the close callback (invoked with `open: false` whenever the user
    /// closes the dialog: the "×", Escape).
    #[inline]
    pub fn set_on_close<C: Into<ModalOnCloseCallback>>(&mut self, data: RefAny, on_close: C) {
        self.modal_state.on_close = Some(ModalOnClose {
            callback: on_close.into(),
            refany: data,
        })
        .into();
    }

    /// Builder-style setter for the close callback.
    #[inline]
    #[must_use]
    pub fn with_on_close<C: Into<ModalOnCloseCallback>>(
        mut self,
        data: RefAny,
        on_close: C,
    ) -> Self {
        self.set_on_close(data, on_close);
        self
    }

    /// Replaces `self` with a default (empty, closed) modal and returns the original.
    #[inline]
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(Dom::default());
        core::mem::swap(&mut s, self);
        s
    }

    /// Renders the modal: a `__azul-native-modal` wrapper holding the modal's
    /// `<transient-window>`, whose root is the backdrop and whose child is
    /// the `__azul-native-modal-panel`.
    #[must_use]
    pub fn dom(self) -> Dom {
        let backdrop_style = self.resolved_backdrop_style();
        build_dialog(DialogParts {
            declared_open: self.modal_state.inner.open,
            modal: true,
            return_value: AzString::from_const_str(""),
            // HTML's default for a modal dialog: Escape closes it, the
            // backdrop does not.
            closed_by: DialogClosedBy::Auto,
            on_cancel: OptionDialogOnCancel::None,
            on_close: OptionDialogOnClose::None,
            compat: DialogCompat::Modal(self.modal_state.on_close),
            title: self.title,
            content: self.content,
            invoker: None,
            show_close_button: self.show_close_button,
            anchor: TransientAnchor::Viewport,
            wrapper_style: None,
            panel_style: None,
            backdrop_style: Some(backdrop_style),
            classes: DialogClasses {
                wrapper: MODAL_CLASS,
                invoker: &[],
                window: &[],
                panel: MODAL_PANEL_CLASS,
                title: MODAL_TITLE_CLASS,
                content: MODAL_CONTENT_CLASS,
                close: MODAL_CLOSE_CLASS,
            },
        })
    }
}

impl Default for Modal {
    fn default() -> Self {
        Self::create(Dom::default())
    }
}

impl From<Modal> for Dom {
    fn from(m: Modal) -> Self {
        m.dom()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        a11y::AccessibilityRole,
        dom::{DomId, DomNodeId, NodeId, NodeType},
        geom::{LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        transient::TransientDismiss,
        window::{KeyboardState, MonitorVec, RawWindowHandle, VirtualKeyCode},
    };
    use azul_css::{props::property::CssProperty, system::SystemStyle};
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        widgets::dialog::{on_dialog_dismissed, on_dialog_key},
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    fn has_class(node: &Dom, name: &str) -> bool {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, Class(s) if s.as_str() == name))
    }

    fn index_of_class(styled: &StyledDom, class: &str) -> usize {
        styled
            .node_data
            .as_ref()
            .iter()
            .position(|nd| {
                nd.get_ids_and_classes()
                    .as_ref()
                    .iter()
                    .any(|c| matches!(c, Class(s) if s.as_str() == class))
            })
            .unwrap_or_else(|| panic!("no node with class {class}"))
    }

    fn node(i: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i))),
        }
    }

    fn layout_result(styled_dom: StyledDom) -> DomLayoutResult {
        DomLayoutResult {
            styled_dom,
            layout_tree: LayoutTree {
                nodes: Vec::new(),
                warm: Vec::new(),
                cold: Vec::new(),
                root: 0,
                dom_to_layout: BTreeMap::new(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            },
            calculated_positions: Vec::new(),
            viewport: LogicalRect::zero(),
            display_list: Arc::new(DisplayList::default()),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// Runs `f` on a `CallbackInfo` over `styled` (hit node `hit`, keyboard
    /// `keys`); returns `f`'s result and the changes it queued.
    fn with_info<R>(
        styled: StyledDom,
        hit: usize,
        keys: KeyboardState,
        f: impl FnOnce(CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        layout_window
            .layout_results
            .insert(DomId::ROOT_ID, layout_result(styled));

        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let mut current_window_state = FullWindowState::default();
        current_window_state.keyboard_state = keys;
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: core::cell::RefCell::new(OptionRefAny::None),
        };
        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));
        let info = CallbackInfo::new(
            &ref_data,
            &changes,
            node(hit),
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );
        let r = f(info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (r, recorded)
    }

    fn rendered(modal: Modal) -> (StyledDom, RefAny) {
        let dom = modal.dom();
        let data = dom
            .root
            .get_dataset()
            .cloned()
            .expect("the wrapper carries the modal's state");
        (StyledDom::create_from_dom(dom), data)
    }

    /// Records every close; no asserts in an `extern "C" fn` (a panic there
    /// aborts the test binary).
    struct CloseLog {
        closes: Vec<bool>,
    }

    extern "C" fn record_close(mut data: RefAny, _: CallbackInfo, state: ModalState) -> Update {
        if let Some(mut log) = data.downcast_mut::<CloseLog>() {
            log.closes.push(state.open);
        }
        Update::RefreshDom
    }

    fn closes(log: &RefAny) -> Vec<bool> {
        let mut log = log.clone();
        let v = log
            .downcast_ref::<CloseLog>()
            .map(|l| l.closes.clone())
            .unwrap_or_default();
        v
    }

    #[test]
    fn create_is_closed_with_a_close_button_and_no_title() {
        let m = Modal::create(Dom::create_p_with_text("body"));
        assert!(!m.modal_state.inner.open);
        assert!(m.show_close_button);
        assert!(m.title.as_str().is_empty());
        assert!(m.modal_state.on_close.is_none());
        assert_eq!(m.resolved_backdrop_style(), default_backdrop_style());
        assert_eq!(Modal::default(), Modal::create(Dom::default()));
    }

    #[test]
    fn builders_match_setters() {
        let built = Modal::create(Dom::default())
            .with_title("T".into())
            .with_open(true)
            .with_close_button(false);
        let mut set = Modal::create(Dom::default());
        set.set_title("T".into());
        set.set_open(true);
        set.set_close_button(false);
        assert_eq!(built, set);
        let mut s = built.clone();
        let original = s.swap_with_default();
        assert_eq!(original, built);
        assert_eq!(s, Modal::default());
    }

    /// The modal is a modal dialog: a window over the whole parent whose
    /// root is the backdrop, the panel named by the title, with the modal's
    /// historical classes on its parts.
    #[test]
    fn the_modal_is_a_viewport_window_with_a_named_panel() {
        for open in [false, true] {
            let dom = Modal::create(Dom::create_p_with_text("body"))
                .with_title("Example dialog".into())
                .with_open(open)
                .dom();
            assert!(has_class(&dom, "__azul-native-modal"));
            let window = &dom.children.as_ref()[0];
            let NodeType::TransientWindow(cfg) = window.root.get_node_type() else {
                panic!("the wrapper holds the modal's window");
            };
            assert_eq!(cfg.open, open);
            assert_eq!(cfg.anchor, TransientAnchor::Viewport);
            assert_eq!(
                cfg.dismiss,
                TransientDismiss::None,
                "Escape is the modal's own"
            );
            assert!(window
                .root
                .style
                .iter_inline_properties()
                .any(|(p, _)| matches!(p, CssProperty::BackgroundContent(_))));
            let panel = &window.children.as_ref()[0];
            assert!(has_class(panel, "__azul-native-modal-panel"));
            let info = panel.root.get_accessibility_info().expect("a role");
            assert_eq!(info.role, AccessibilityRole::Dialog);
            assert_eq!(
                info.accessibility_name.as_ref().map(|s| s.as_str()),
                Some("Example dialog")
            );
            let kids = panel.children.as_ref();
            assert!(has_class(&kids[0], "__azul-native-modal-title"));
            assert!(has_class(&kids[1], "__azul-native-modal-content"));
            assert!(has_class(&kids[2], "__azul-native-modal-close"));
        }
    }

    /// Escape closes a modal (HTML's default for a modal dialog).
    #[test]
    fn escape_closes_the_modal() {
        let (styled, data) = rendered(Modal::create(Dom::default()).with_open(true));
        let window = index_of_class(&styled, "__azul-native-dialog-window");
        let mut keys = KeyboardState::default();
        keys.current_virtual_keycode = Some(VirtualKeyCode::Escape).into();
        keys.pressed_virtual_keycodes = vec![VirtualKeyCode::Escape].into();
        let (_, changes) = with_info(styled, window, keys, |info| {
            on_dialog_key(data.clone(), info)
        });
        assert!(changes.iter().any(|c| matches!(
            c,
            CallbackChange::SetTransientWindowOpen { open: false, .. }
        )));
    }

    /// Any close reaches `on_close` with `open: false`.
    #[test]
    fn a_close_tells_on_close() {
        let log = RefAny::new(CloseLog { closes: Vec::new() });
        let on_close: ModalOnCloseCallbackType = record_close;
        let (styled, data) = rendered(
            Modal::create(Dom::default())
                .with_open(true)
                .with_on_close(log.clone(), on_close),
        );
        let window = index_of_class(&styled, "__azul-native-dialog-window");
        let (update, _) = with_info(styled, window, KeyboardState::default(), |info| {
            on_dialog_dismissed(data.clone(), info)
        });
        assert_eq!(closes(&log), vec![false]);
        assert_eq!(update, Update::RefreshDom);
    }
}
