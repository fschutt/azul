//! Dialog widget — HTML `<dialog>` semantics on a `<transient-window>`.
//!
//! A dialog is a real OS window drawn from a subtree of the app's one DOM
//! (see `azul_core::transient`), so it sits in the TOP LAYER by construction:
//! above everything in its parent, never clipped by an `overflow: hidden`
//! ancestor.
//!
//! | HTML                                   | here                                                    |
//! |----------------------------------------|---------------------------------------------------------|
//! | `<dialog open>`, `show()`              | [`Dialog::with_open`], [`Dialog::show`]                 |
//! | `showModal()`                          | [`Dialog::show_modal`] (`with_open` + `with_modal`)     |
//! | `<button commandfor command=show-modal>` | [`Dialog::with_invoker`]: clicking it shows the dialog |
//! | `close(returnValue)`                   | [`Dialog::close_from`] from a control inside it         |
//! | `requestClose(returnValue)`            | [`Dialog::request_close_from`] (`cancel` first)         |
//! | `returnValue`                          | [`DialogState::return_value`]                           |
//! | `cancel` event (cancelable)            | [`Dialog::with_on_cancel`]; `info.prevent_default()` keeps it open |
//! | `close` event                          | [`Dialog::with_on_close`]                               |
//! | `closedby="any\|closerequest\|none"`    | [`DialogClosedBy`]                                      |
//! | `::backdrop`                           | a modal dialog's window root, [`Dialog::with_backdrop_style`] |
//! | `role=dialog` + name                   | the panel: `AccessibilityRole::Dialog`, named by the title |
//!
//! ## How the pieces map onto the transient-window machinery
//!
//! - **Open / closed.** The dialog is open while its `<transient-window>` is: the app's `open`
//!   (declarative, like the attribute) or a click on the invoker
//!   (`set_transient_window_open`). The widget keeps NO open flag of its own: it asks the engine
//!   ([`CallbackInfo::is_transient_window_open`]). A private flag in a callback payload is what
//!   made the old popover impossible to close - the app's rebuild re-minted it as "closed" while
//!   the panel stayed visible.
//! - **Modal.** `anchor="viewport"`: the window covers the parent window exactly. Its root is the
//!   `::backdrop` (dimmed, flex-centred) and the dialog panel sits in it. The parent is inert to
//!   the pointer because it is covered, and to the keyboard because the dialog's window holds it
//!   (it is the key window; X11 forwards the parent's keys to it).
//! - **Non-modal.** A popup below the invoker (or at `with_anchor`).
//! - **Escape (close request).** The engine leaves Escape to the dialog (`TransientDismiss::None`
//!   / `OutsideOnly`). The dialog's window root answers it: `cancel` runs first, and unless the
//!   app prevented it, the dialog closes. `closedby="none"` ignores Escape.
//! - **Light dismiss (`closedby="any"`).** A modal dialog closes on a press on its backdrop,
//!   through the same cancelable request. A non-modal one closes on a press outside or when its
//!   window loses focus - that is the engine's (`OutsideOnly`), and it is not cancelable (neither
//!   is an HTML popover's).
//! - **Focus.** The engine autofocuses the dialog's `autofocus` control, else its first tab stop,
//!   and hands focus back to wherever it was (the invoker) when the dialog closes. The close
//!   button is LAST in tree order, so it is not what gets focus first.
//! - **State across rebuilds.** The wrapper carries the dialog's state (`DialogData`) as its
//!   dataset with a merge callback, so the return value survives the app's rebuilds; the app's
//!   new callbacks and `closedby` are adopted on every build.
//!
//! [`crate::widgets::popover::Popover`] and [`crate::widgets::modal::Modal`] are front-ends over
//! the same core ([`build_dialog`]).
//!
//! Key types: [`Dialog`], [`DialogState`], [`DialogClosedBy`], [`DialogOnCancel`],
//! [`DialogOnClose`].

use alloc::vec::Vec;

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    callbacks::Update,
    dom::{
        AttributeType, ComponentEventFilter, DatasetMergeCallback, Dom, DomNodeId, DomVec,
        EventFilter, FocusEventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class,
        IdOrClassVec, NodeData, NodeType, OptionDom, TabIndex,
    },
    refany::{OptionRefAny, RefAny},
    transient::{TransientAnchor, TransientDismiss, TransientWindowConfig},
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::{
        CssPropertyWithConditions, CssPropertyWithConditionsVec, OptionCssPropertyWithConditionsVec,
    },
    impl_option_inner,
    props::{
        basic::{
            color::ColorU,
            font::{StyleFontFamily, StyleFontFamilyVec},
            PixelValue, StyleFontSize,
        },
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow, LayoutHeight,
            LayoutJustifyContent, LayoutLeft, LayoutMaxWidth, LayoutMinHeight, LayoutMinWidth,
            LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop,
            LayoutPosition, LayoutRight, LayoutTop, LayoutWidth,
        },
        property::{CssProperty, *},
        style::{
            BorderStyle, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth,
            LayoutBorderTopWidth, StyleBackgroundContent, StyleBackgroundContentVec,
            StyleBorderBottomColor, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius,
            StyleBorderBottomStyle, StyleBorderLeftColor, StyleBorderLeftStyle,
            StyleBorderRightColor, StyleBorderRightStyle, StyleBorderTopColor,
            StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderTopStyle, StyleCursor,
            StyleTextAlign, StyleTextColor, StyleUserSelect,
        },
    },
    AzString,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::{
        modal::{ModalOnClose, ModalState, OptionModalOnClose},
        popover::{OptionPopoverOnToggle, PopoverOnToggle, PopoverState},
        themes::system_palette,
    },
};

// ---- classes ----
const DIALOG_CLASS: &str = "__azul-native-dialog";
const DIALOG_INVOKER_CLASS: &str = "__azul-native-dialog-invoker";
const DIALOG_WINDOW_CLASS: &str = "__azul-native-dialog-window";
const DIALOG_PANEL_CLASS: &str = "__azul-native-dialog-panel";
const DIALOG_TITLE_CLASS: &str = "__azul-native-dialog-title";
const DIALOG_CONTENT_CLASS: &str = "__azul-native-dialog-content";
const DIALOG_CLOSE_CLASS: &str = "__azul-native-dialog-close";

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

// ---- layout (logical px) ----
const PANEL_MIN_WIDTH: isize = 280;
const PANEL_MAX_WIDTH: isize = 520;
const PANEL_RADIUS: isize = 8;
/// Room for the close button when the dialog has no title row.
const CLOSE_ROW_HEIGHT: isize = 24;

// ---- colours ----
/// The `::backdrop`: semi-transparent black (rgba(0,0,0,0.5)).
const BACKDROP_COLOR: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 0,
    a: 128,
};
const PANEL_BG_COLOR: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
const PANEL_BORDER_COLOR: ColorU = ColorU {
    r: 204,
    g: 204,
    b: 204,
    a: 255,
}; // #cccccc
const TITLE_COLOR: ColorU = ColorU {
    r: 33,
    g: 37,
    b: 41,
    a: 255,
}; // #212529
const CLOSE_COLOR: ColorU = ColorU {
    r: 108,
    g: 117,
    b: 125,
    a: 255,
}; // #6c757d

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// HTML `closedby`: which user actions close a dialog without the app.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum DialogClosedBy {
    /// No `closedby`: [`Self::CloseRequest`] for a modal dialog, [`Self::None`]
    /// for a non-modal one - the HTML default.
    #[default]
    Auto,
    /// Light dismiss (a press outside a non-modal dialog, a press on a modal
    /// dialog's backdrop), a close request (Escape), or the app.
    Any,
    /// A close request (Escape), or the app.
    CloseRequest,
    /// Only the app: a button inside the dialog, or `open` going false.
    None,
}

impl DialogClosedBy {
    /// `Auto` resolved for a modal (`true`) or a non-modal dialog.
    #[must_use]
    pub const fn resolve(self, modal: bool) -> Self {
        match self {
            Self::Auto => {
                if modal {
                    Self::CloseRequest
                } else {
                    Self::None
                }
            }
            other => other,
        }
    }

    /// Does Escape (a close request) close the dialog?
    #[must_use]
    pub const fn allows_close_request(self, modal: bool) -> bool {
        matches!(self.resolve(modal), Self::Any | Self::CloseRequest)
    }

    /// Does a press outside (non-modal) / on the backdrop (modal) close it?
    #[must_use]
    pub const fn allows_light_dismiss(self, modal: bool) -> bool {
        matches!(self.resolve(modal), Self::Any)
    }

    /// The dismiss policy the dialog's `<transient-window>` gets.
    ///
    /// Escape is never the engine's - the dialog answers it itself, so its
    /// `cancel` can run first. A NON-modal light dismiss is the engine's
    /// (`OutsideOnly`): only it sees a press in the parent. A modal dialog's
    /// light dismiss is a press on its own backdrop, which the dialog sees.
    #[must_use]
    pub const fn transient_dismiss(self, modal: bool) -> TransientDismiss {
        if !modal && self.allows_light_dismiss(false) {
            TransientDismiss::OutsideOnly
        } else {
            TransientDismiss::None
        }
    }
}

/// A dialog's state, as its callbacks see it: HTML's `open`, whether it is
/// shown modally, and `returnValue`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct DialogState {
    /// Whether the dialog is showing. `true` in a `cancel` callback (the
    /// dialog is still up), `false` in a `close` callback.
    pub open: bool,
    /// Shown with `show_modal` (covering its window) rather than `show`.
    pub modal: bool,
    /// What the dialog was closed with: the value [`Dialog::close_from`] was
    /// given. Unchanged by Escape, a light dismiss and the close button
    /// (HTML: `close()` without an argument). Empty again when the dialog is
    /// shown anew.
    pub return_value: AzString,
}

/// The `cancel` event: a close request (Escape, a light dismiss the dialog
/// sees, [`Dialog::request_close_from`]) is about to close the dialog. Call
/// `info.prevent_default()` to keep it open.
///
/// Runs in the dialog's own window: a `RefreshDom` it returns is widened to
/// `RefreshDomAllWindows`, so the app's window rebuilds too.
pub type DialogOnCancelCallbackType = extern "C" fn(RefAny, CallbackInfo, DialogState) -> Update;
impl_widget_callback!(
    DialogOnCancel,
    OptionDialogOnCancel,
    DialogOnCancelCallback,
    DialogOnCancelCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        DialogOnCancelCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: DIALOG_ON_CANCEL_INVOKER,
    invoker_ty:     AzDialogOnCancelCallbackInvoker,
    thunk_fn:       az_dialog_on_cancel_callback_thunk,
    setter_fn:      AzApp_setDialogOnCancelCallbackInvoker,
    from_handle_fn: AzDialogOnCancelCallback_createFromHostHandle,
    from_handle_byref_fn: AzDialogOnCancelCallback_createFromHostHandleByref,
    extra_args:     [ state: DialogState ],
}

/// The `close` event: the dialog closed (any way but the app's `open`
/// going false). [`DialogState::return_value`] says with what. Runs in the
/// app's window.
pub type DialogOnCloseCallbackType = extern "C" fn(RefAny, CallbackInfo, DialogState) -> Update;
impl_widget_callback!(
    DialogOnClose,
    OptionDialogOnClose,
    DialogOnCloseCallback,
    DialogOnCloseCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        DialogOnCloseCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: DIALOG_ON_CLOSE_INVOKER,
    invoker_ty:     AzDialogOnCloseCallbackInvoker,
    thunk_fn:       az_dialog_on_close_callback_thunk,
    setter_fn:      AzApp_setDialogOnCloseCallbackInvoker,
    from_handle_fn: AzDialogOnCloseCallback_createFromHostHandle,
    from_handle_byref_fn: AzDialogOnCloseCallback_createFromHostHandleByref,
    extra_args:     [ state: DialogState ],
}

/// The dialog's declared state plus how it closes and whom it tells.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct DialogStateWrapper {
    /// `open` / `modal` as the app declares them, and the initial return value.
    pub inner: DialogState,
    /// HTML `closedby`.
    pub closed_by: DialogClosedBy,
    /// The `cancel` event.
    pub on_cancel: OptionDialogOnCancel,
    /// The `close` event.
    pub on_close: OptionDialogOnClose,
}

/// An HTML `<dialog>`: a titled panel holding arbitrary content, shown
/// modally (covering its window, over a dimmed backdrop) or as a popup
/// below its invoker. See the module docs for the semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Dialog {
    /// Declared state, `closedby` and the callbacks.
    pub dialog_state: DialogStateWrapper,
    /// The title, which also names the dialog for assistive technology.
    /// Empty: no title row, and the dialog is named "Dialog".
    pub title: AzString,
    /// What the dialog shows.
    pub content: Dom,
    /// The element that shows the dialog when clicked (HTML `commandfor`),
    /// or `None`: the app shows it through `open`.
    pub invoker: OptionDom,
    /// Whether the dialog has a "x" close button (top right, last in tab
    /// order).
    pub show_close_button: bool,
    /// Where a NON-modal dialog opens, relative to its invoker. A modal one
    /// always covers its window.
    pub anchor: TransientAnchor,
    /// Style of the dialog panel, or `None` for the default panel.
    pub panel_style: OptionCssPropertyWithConditionsVec,
    /// Style of a modal dialog's `::backdrop` (its window's root), or `None`
    /// for the default dim.
    pub backdrop_style: OptionCssPropertyWithConditionsVec,
}

impl Default for Dialog {
    fn default() -> Self {
        Self::create(Dom::default())
    }
}

impl Dialog {
    /// A closed, non-modal dialog holding `content`, with a close button and
    /// no title. `closedby` is `Auto`.
    #[must_use]
    pub fn create(content: Dom) -> Self {
        Self {
            dialog_state: DialogStateWrapper::default(),
            title: AzString::from_const_str(""),
            content,
            invoker: OptionDom::None,
            show_close_button: true,
            anchor: TransientAnchor::Bottom,
            panel_style: OptionCssPropertyWithConditionsVec::None,
            backdrop_style: OptionCssPropertyWithConditionsVec::None,
        }
    }

    /// Sets the title (and accessible name).
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

    /// Replaces the content.
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

    /// Sets the element whose click shows the dialog (modal or not, as
    /// declared), and hides it again when clicked while it is showing.
    #[inline]
    pub fn set_invoker(&mut self, invoker: Dom) {
        self.invoker = OptionDom::Some(invoker);
    }

    /// Builder-style setter for the invoker.
    #[inline]
    #[must_use]
    pub fn with_invoker(mut self, invoker: Dom) -> Self {
        self.set_invoker(invoker);
        self
    }

    /// Declares the dialog open (the HTML `open` attribute). A change of it
    /// shows or closes the dialog; while it stays `true`, a dialog the user
    /// closed stays closed until it goes `false` and `true` again.
    #[inline]
    pub const fn set_open(&mut self, open: bool) {
        self.dialog_state.inner.open = open;
    }

    /// Builder-style setter for `open`.
    #[inline]
    #[must_use]
    pub const fn with_open(mut self, open: bool) -> Self {
        self.set_open(open);
        self
    }

    /// Whether the dialog shows modally: covering its window over a dimmed
    /// backdrop, with the rest of the window inert.
    #[inline]
    pub const fn set_modal(&mut self, modal: bool) {
        self.dialog_state.inner.modal = modal;
    }

    /// Builder-style setter for `modal`.
    #[inline]
    #[must_use]
    pub const fn with_modal(mut self, modal: bool) -> Self {
        self.set_modal(modal);
        self
    }

    /// HTML `show()`: open, non-modal.
    #[inline]
    #[must_use]
    pub const fn show(self) -> Self {
        self.with_modal(false).with_open(true)
    }

    /// HTML `showModal()`: open, modal.
    #[inline]
    #[must_use]
    pub const fn show_modal(self) -> Self {
        self.with_modal(true).with_open(true)
    }

    /// HTML `closedby`.
    #[inline]
    pub const fn set_closed_by(&mut self, closed_by: DialogClosedBy) {
        self.dialog_state.closed_by = closed_by;
    }

    /// Builder-style setter for `closedby`.
    #[inline]
    #[must_use]
    pub const fn with_closed_by(mut self, closed_by: DialogClosedBy) -> Self {
        self.set_closed_by(closed_by);
        self
    }

    /// The return value the dialog starts with (HTML: `returnValue` set
    /// before it is shown).
    #[inline]
    pub fn set_return_value(&mut self, return_value: AzString) {
        self.dialog_state.inner.return_value = return_value;
    }

    /// Builder-style setter for the initial return value.
    #[inline]
    #[must_use]
    pub fn with_return_value(mut self, return_value: AzString) -> Self {
        self.set_return_value(return_value);
        self
    }

    /// Whether the "x" close button is shown.
    #[inline]
    pub const fn set_close_button(&mut self, show: bool) {
        self.show_close_button = show;
    }

    /// Builder-style setter for the close button.
    #[inline]
    #[must_use]
    pub const fn with_close_button(mut self, show: bool) -> Self {
        self.set_close_button(show);
        self
    }

    /// Where a non-modal dialog opens relative to its invoker.
    #[inline]
    pub const fn set_anchor(&mut self, anchor: TransientAnchor) {
        self.anchor = anchor;
    }

    /// Builder-style setter for the anchor edge.
    #[inline]
    #[must_use]
    pub const fn with_anchor(mut self, anchor: TransientAnchor) -> Self {
        self.set_anchor(anchor);
        self
    }

    /// Sets the `cancel` callback.
    #[inline]
    pub fn set_on_cancel<C: Into<DialogOnCancelCallback>>(&mut self, data: RefAny, on_cancel: C) {
        self.dialog_state.on_cancel = Some(DialogOnCancel {
            callback: on_cancel.into(),
            refany: data,
        })
        .into();
    }

    /// Builder-style setter for the `cancel` callback.
    #[inline]
    #[must_use]
    pub fn with_on_cancel<C: Into<DialogOnCancelCallback>>(
        mut self,
        data: RefAny,
        on_cancel: C,
    ) -> Self {
        self.set_on_cancel(data, on_cancel);
        self
    }

    /// Sets the `close` callback.
    #[inline]
    pub fn set_on_close<C: Into<DialogOnCloseCallback>>(&mut self, data: RefAny, on_close: C) {
        self.dialog_state.on_close = Some(DialogOnClose {
            callback: on_close.into(),
            refany: data,
        })
        .into();
    }

    /// Builder-style setter for the `close` callback.
    #[inline]
    #[must_use]
    pub fn with_on_close<C: Into<DialogOnCloseCallback>>(
        mut self,
        data: RefAny,
        on_close: C,
    ) -> Self {
        self.set_on_close(data, on_close);
        self
    }

    /// Replaces the panel style (`None` restores the default panel).
    #[inline]
    pub fn set_panel_style(&mut self, style: OptionCssPropertyWithConditionsVec) {
        self.panel_style = style;
    }

    /// Builder-style setter for the panel style.
    #[inline]
    #[must_use]
    pub fn with_panel_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_panel_style(OptionCssPropertyWithConditionsVec::Some(style));
        self
    }

    /// Replaces the `::backdrop` style of a modal dialog (`None` restores
    /// the default dim).
    #[inline]
    pub fn set_backdrop_style(&mut self, style: OptionCssPropertyWithConditionsVec) {
        self.backdrop_style = style;
    }

    /// Builder-style setter for the `::backdrop` style.
    #[inline]
    #[must_use]
    pub fn with_backdrop_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_backdrop_style(OptionCssPropertyWithConditionsVec::Some(style));
        self
    }

    /// Replaces `self` with a default dialog and returns the original.
    #[inline]
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    /// Renders the dialog: a wrapper holding the invoker (if any) and the
    /// dialog's `<transient-window>`.
    #[must_use]
    pub fn dom(self) -> Dom {
        let inner = self.dialog_state.inner;
        build_dialog(DialogParts {
            declared_open: inner.open,
            modal: inner.modal,
            return_value: inner.return_value,
            closed_by: self.dialog_state.closed_by,
            on_cancel: self.dialog_state.on_cancel,
            on_close: self.dialog_state.on_close,
            compat: DialogCompat::None,
            title: self.title,
            content: self.content,
            invoker: self.invoker.into_option(),
            show_close_button: self.show_close_button,
            anchor: self.anchor,
            wrapper_style: None,
            panel_style: self.panel_style.into_option(),
            backdrop_style: self.backdrop_style.into_option(),
            classes: DialogClasses::NONE,
        })
    }

    /// HTML `dialog.close(returnValue)`, from a callback of a control INSIDE
    /// the dialog (`node`, usually `info.get_hit_node()`): records the return
    /// value and closes the dialog. No `cancel`. The `close` callback then
    /// runs in the app's window with this value. Returns whether `node` was
    /// inside a dialog.
    pub fn close_from(info: &mut CallbackInfo, node: DomNodeId, return_value: AzString) -> bool {
        let Some((root, mut data)) = find_dialog(info, node) else {
            return false;
        };
        if let Some(mut d) = data.downcast_mut::<DialogData>() {
            d.return_value = return_value;
        }
        info.set_transient_window_open(root, false);
        true
    }

    /// HTML `dialog.requestClose(returnValue)`, from a control inside the
    /// dialog: runs `cancel` first, and closes the dialog with the value
    /// unless the app called `info.prevent_default()` there. Returns what the
    /// `cancel` callback returned.
    pub fn request_close_from(
        info: &mut CallbackInfo,
        node: DomNodeId,
        return_value: AzString,
    ) -> Update {
        let Some((root, mut data)) = find_dialog(info, node) else {
            return Update::DoNothing;
        };
        everywhere(request_close(&mut data, info, root, Some(return_value)))
    }
}

impl From<Dialog> for Dom {
    fn from(d: Dialog) -> Self {
        d.dom()
    }
}

// ---------------------------------------------------------------------------
// The persistent state
// ---------------------------------------------------------------------------

/// What a dialog keeps across the app's rebuilds: the wrapper's dataset,
/// kept by a merge callback, and shared (as one allocation) by the invoker,
/// the window and every control inside the dialog, in the app's window and
/// in the dialog's own.
#[derive(Debug)]
pub struct DialogData {
    modal: bool,
    return_value: AzString,
    closed_by: DialogClosedBy,
    /// The app's `open` as last built: its false -> true edge is a fresh
    /// showing, which forgets the last return value.
    declared_open: bool,
    on_cancel: OptionDialogOnCancel,
    on_close: OptionDialogOnClose,
    compat: DialogCompat,
}

impl DialogData {
    /// The return value the dialog holds (tests / diagnostics).
    #[must_use]
    pub const fn return_value(&self) -> &AzString {
        &self.return_value
    }

    /// Whether it was built modal (tests / diagnostics).
    #[must_use]
    pub const fn is_modal(&self) -> bool {
        self.modal
    }

    /// Its `closedby` (tests / diagnostics).
    #[must_use]
    pub const fn closed_by(&self) -> DialogClosedBy {
        self.closed_by
    }

    fn state(&self, open: bool) -> DialogState {
        DialogState {
            open,
            modal: self.modal,
            return_value: self.return_value.clone(),
        }
    }
}

/// The callbacks of the older front-ends, which predate `cancel` / `close`:
/// told after the dialog's own.
#[derive(Debug, Clone, Default)]
pub(crate) enum DialogCompat {
    #[default]
    None,
    /// `Popover::on_toggle`: every show and close, with the new state.
    Popover(OptionPopoverOnToggle),
    /// `Modal::on_close`: every close.
    Modal(OptionModalOnClose),
}

// ---------------------------------------------------------------------------
// The core: one builder for Dialog, Popover and Modal
// ---------------------------------------------------------------------------

/// Extra classes a front-end puts on the dialog's parts next to the
/// dialog's own (the popover's and the modal's historical class names).
#[derive(Debug, Clone, Copy)]
pub(crate) struct DialogClasses {
    pub wrapper: &'static [IdOrClass],
    pub invoker: &'static [IdOrClass],
    pub window: &'static [IdOrClass],
    pub panel: &'static [IdOrClass],
    pub title: &'static [IdOrClass],
    pub content: &'static [IdOrClass],
    pub close: &'static [IdOrClass],
}

impl DialogClasses {
    pub(crate) const NONE: Self = Self {
        wrapper: &[],
        invoker: &[],
        window: &[],
        panel: &[],
        title: &[],
        content: &[],
        close: &[],
    };
}

/// Everything [`build_dialog`] needs; `Dialog`, `Popover` and `Modal` fill
/// it in.
pub(crate) struct DialogParts {
    pub declared_open: bool,
    pub modal: bool,
    pub return_value: AzString,
    pub closed_by: DialogClosedBy,
    pub on_cancel: OptionDialogOnCancel,
    pub on_close: OptionDialogOnClose,
    pub compat: DialogCompat,
    pub title: AzString,
    pub content: Dom,
    pub invoker: Option<Dom>,
    pub show_close_button: bool,
    pub anchor: TransientAnchor,
    /// `None`: the default wrapper (an inline-block around the invoker).
    pub wrapper_style: Option<CssPropertyWithConditionsVec>,
    /// `None`: the default panel.
    pub panel_style: Option<CssPropertyWithConditionsVec>,
    /// `None`: the default `::backdrop` (modal only).
    pub backdrop_style: Option<CssPropertyWithConditionsVec>,
    pub classes: DialogClasses,
}

/// The dialog's own class plus a front-end's.
fn classes(own: &'static str, extra: &'static [IdOrClass]) -> IdOrClassVec {
    let mut v = Vec::with_capacity(1 + extra.len());
    v.push(Class(AzString::from_const_str(own)));
    v.extend(extra.iter().cloned());
    IdOrClassVec::from_vec(v)
}

/// Builds a dialog:
///
/// ```text
/// wrapper                      dataset = DialogData (+ merge callback)
///  ├─ invoker (optional)       Click -> show / close
///  └─ <transient-window>       Dismissed -> close event; KeyDown -> Escape;
///      │                       (modal) LeftMouseDown -> backdrop light dismiss
///      └─ panel                role Dialog, named by the title; dataset = DialogData
///          ├─ title row        (title, or a spacer for the close button)
///          ├─ content
///          └─ close "×"        Click -> close()
/// ```
///
/// In the dialog's own window the `<transient-window>` node is the root, so
/// every key bubbles to it and, modal, it is the `::backdrop`.
pub(crate) fn build_dialog(parts: DialogParts) -> Dom {
    let DialogParts {
        declared_open,
        modal,
        return_value,
        closed_by,
        on_cancel,
        on_close,
        compat,
        title,
        content,
        invoker,
        show_close_button,
        anchor,
        wrapper_style,
        panel_style,
        backdrop_style,
        classes: extra,
    } = parts;

    let data = RefAny::new(DialogData {
        modal,
        return_value,
        closed_by,
        declared_open,
        on_cancel,
        on_close,
        compat,
    });

    // ---- the panel ----
    let name = if title.as_str().is_empty() {
        AzString::from_const_str("Dialog")
    } else {
        title.clone()
    };
    let mut panel_children = Vec::new();
    if !title.as_str().is_empty() {
        panel_children.push(
            crate::widgets::widget_p_with_text(title.clone())
                .with_ids_and_classes(classes(DIALOG_TITLE_CLASS, extra.title))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                    DIALOG_TITLE_STYLE,
                )),
        );
    } else if show_close_button {
        // No title row: keep the close button's corner clear of the content.
        panel_children.push(Dom::create_div().with_css_props(
            CssPropertyWithConditionsVec::from_const_slice(DIALOG_CLOSE_ROW_STYLE),
        ));
    }
    panel_children.push(
        Dom::create_div()
            .with_ids_and_classes(classes(DIALOG_CONTENT_CLASS, extra.content))
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                DIALOG_CONTENT_STYLE,
            ))
            .with_children(DomVec::from_vec(alloc::vec![content])),
    );
    if show_close_button {
        // LAST in tree order: the dialog autofocuses its first tab stop, and
        // that should be the content's first control, not "close".
        panel_children.push(
            crate::widgets::widget_p_with_text(AzString::from_const_str("\u{00D7}"))
                .with_ids_and_classes(classes(DIALOG_CLOSE_CLASS, extra.close))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                    DIALOG_CLOSE_STYLE,
                ))
                .with_tab_index(TabIndex::Auto)
                // The label is a multiplication sign - a picture of an X, not
                // a name.
                .with_accessibility_info(AccessibilityInfo {
                    role: AccessibilityRole::PushButton,
                    accessibility_name: Some(AzString::from_const_str("Close")).into(),
                    ..Default::default()
                })
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    data.clone(),
                    Callback::from_ptr(on_dialog_close_button).to_core(),
                ),
        );
    }
    let panel = Dom::create_div()
        .with_ids_and_classes(classes(DIALOG_PANEL_CLASS, extra.panel))
        .with_css_props(panel_style.unwrap_or_else(|| {
            CssPropertyWithConditionsVec::from_const_slice(DIALOG_PANEL_STYLE)
        }))
        // `Dialog::close_from` finds the dialog's state here, from a control
        // inside it, in the dialog's own window.
        .with_dataset(OptionRefAny::Some(data.clone()))
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Dialog,
            accessibility_name: Some(name).into(),
            ..Default::default()
        })
        .with_children(DomVec::from_vec(panel_children));

    // ---- the window ----
    let config = if declared_open {
        TransientWindowConfig::opened()
    } else {
        TransientWindowConfig::closed()
    }
    .with_anchor(if modal {
        TransientAnchor::Viewport
    } else {
        anchor
    })
    .with_dismiss(closed_by.transient_dismiss(modal))
    // Per-pixel alpha: a modal backdrop lets the app show through dimmed,
    // and a popup panel's rounded corners are real corners.
    .with_material(azul_core::window::WindowBackgroundMaterial::Transparent);
    let mut window = NodeData::create_node(NodeType::TransientWindow(config));
    if !title.as_str().is_empty() {
        window.set_attributes(alloc::vec![AttributeType::Title(title)].into());
    }
    window.add_callback(
        EventFilter::Component(ComponentEventFilter::Dismissed),
        data.clone(),
        Callback::from_ptr(on_dialog_dismissed).to_core(),
    );
    window.add_callback(
        EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
        data.clone(),
        Callback::from_ptr(on_dialog_key).to_core(),
    );
    if modal {
        window.add_callback(
            EventFilter::Hover(HoverEventFilter::LeftMouseDown),
            data.clone(),
            Callback::from_ptr(on_dialog_backdrop_press).to_core(),
        );
    }
    let mut window =
        Dom::create_from_data(window).with_ids_and_classes(classes(DIALOG_WINDOW_CLASS, extra.window));
    if modal {
        window = window.with_css_props(backdrop_style.unwrap_or_else(default_backdrop_style));
    }
    let window = window.with_child(panel);

    // ---- the wrapper ----
    let mut wrapper_children = Vec::new();
    if let Some(invoker) = invoker {
        wrapper_children.push(
            Dom::create_div()
                .with_ids_and_classes(classes(DIALOG_INVOKER_CLASS, extra.invoker))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                    DIALOG_INVOKER_STYLE,
                ))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    data.clone(),
                    Callback::from_ptr(on_dialog_invoker_click).to_core(),
                )
                .with_children(DomVec::from_vec(alloc::vec![invoker])),
        );
    }
    wrapper_children.push(window);

    Dom::create_div()
        .with_ids_and_classes(classes(DIALOG_CLASS, extra.wrapper))
        .with_css_props(wrapper_style.unwrap_or_else(|| {
            CssPropertyWithConditionsVec::from_const_slice(DIALOG_WRAPPER_STYLE)
        }))
        .with_dataset(OptionRefAny::Some(data))
        .with_merge_callback(DatasetMergeCallback::from_ptr(merge_dialog_data))
        .with_children(DomVec::from_vec(wrapper_children))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// A rebuild the app asked for from the dialog's own window must reach the
/// app's window too.
const fn everywhere(update: Update) -> Update {
    match update {
        Update::RefreshDom => Update::RefreshDomAllWindows,
        other => other,
    }
}

/// The dialog `node` sits in: its window root (what closes it) and its
/// state. Walks up to the panel, which carries the state as its dataset.
fn find_dialog(info: &mut CallbackInfo, node: DomNodeId) -> Option<(DomNodeId, RefAny)> {
    let mut current = Some(node);
    for _ in 0..256 {
        let n = current?;
        if let Some(mut dataset) = info.get_dataset(n) {
            let is_dialog = dataset.downcast_ref::<DialogData>().is_some();
            if is_dialog {
                let root = info.get_parent(n)?;
                return Some((root, dataset));
            }
        }
        current = info.get_parent(n);
    }
    None
}

/// The `close` event.
fn fire_on_close(on_close: OptionDialogOnClose, info: CallbackInfo, state: DialogState) -> Update {
    match on_close.into_option() {
        Some(DialogOnClose { callback, refany }) => callback.invoke(refany, info, state),
        None => Update::DoNothing,
    }
}

impl DialogCompat {
    /// Tell an older front-end's callback that the dialog opened / closed.
    fn notify(&self, info: CallbackInfo, open: bool) -> Update {
        match self {
            Self::None => Update::DoNothing,
            Self::Popover(on_toggle) => match on_toggle.as_ref() {
                Some(PopoverOnToggle { callback, refany }) => {
                    callback.invoke(refany.clone(), info, PopoverState { open })
                }
                None => Update::DoNothing,
            },
            Self::Modal(on_close) => match on_close.as_ref() {
                Some(ModalOnClose { callback, refany }) if !open => {
                    callback.invoke(refany.clone(), info, ModalState { open: false })
                }
                _ => Update::DoNothing,
            },
        }
    }
}

/// A close request (Escape, a backdrop press, `request_close_from`): run
/// `cancel`, and close the dialog - its window root `root` - unless the app
/// prevented the default there.
fn request_close(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    root: DomNodeId,
    return_value: Option<AzString>,
) -> Update {
    let (on_cancel, state) = {
        let Some(d) = data.downcast_ref::<DialogData>() else {
            return Update::DoNothing;
        };
        (d.on_cancel.clone(), d.state(true))
    };
    let update = match on_cancel.into_option() {
        Some(DialogOnCancel { callback, refany }) => callback.invoke(refany, *info, state),
        None => Update::DoNothing,
    };
    if info.is_default_prevented() {
        return update; // the app keeps it open
    }
    if let Some(value) = return_value {
        if let Some(mut d) = data.downcast_mut::<DialogData>() {
            d.return_value = value;
        }
    }
    info.set_transient_window_open(root, false);
    update
}

/// The invoker was clicked (app window): show the dialog, or close it if it
/// is showing (the invoker of a modal one is covered, so that is a popup).
pub(crate) extern "C" fn on_dialog_invoker_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let invoker = info.get_hit_node();
    let Some(window) = info.get_next_sibling(invoker) else {
        return Update::DoNothing;
    };
    let open = !info.is_transient_window_open(window);
    let (on_close, compat, state) = {
        let Some(mut d) = data.downcast_mut::<DialogData>() else {
            return Update::DoNothing;
        };
        if open {
            // A fresh showing: the last showing's value is not this one's.
            d.return_value = AzString::from_const_str("");
        }
        (d.on_close.clone(), d.compat.clone(), d.state(open))
    };
    info.set_transient_window_open(window, open);
    let mut update = compat.notify(info, open);
    if !open {
        // Closed through the API, which fires no `Dismissed`: this is the
        // close event.
        update = update.max(fire_on_close(on_close, info, state));
    }
    update
}

/// Escape (the dialog's window root, where every key bubbles): a close
/// request, unless `closedby="none"`.
pub(crate) extern "C" fn on_dialog_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    if key != Some(VirtualKeyCode::Escape) {
        return Update::DoNothing;
    }
    let closes = {
        let Some(d) = data.downcast_ref::<DialogData>() else {
            return Update::DoNothing;
        };
        d.closed_by.allows_close_request(d.modal)
    };
    if !closes {
        return Update::DoNothing;
    }
    let root = info.get_hit_node();
    let update = request_close(&mut data, &mut info, root, None);
    // The Escape was the dialog's: no default action (ClearFocus) on it.
    info.prevent_default();
    everywhere(update)
}

/// A press in a modal dialog's window: on the backdrop (outside the panel)
/// it is a light dismiss, if `closedby="any"`.
pub(crate) extern "C" fn on_dialog_backdrop_press(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let allowed = {
        let Some(d) = data.downcast_ref::<DialogData>() else {
            return Update::DoNothing;
        };
        d.modal && d.closed_by.allows_light_dismiss(true)
    };
    if !allowed {
        return Update::DoNothing;
    }
    let root = info.get_hit_node();
    // A press inside the panel bubbles here too: only one OUTSIDE it counts.
    // No geometry (nothing laid out yet): not a light dismiss.
    let Some(panel) = info.get_first_child(root) else {
        return Update::DoNothing;
    };
    let (Some(cursor), Some(rect)) = (info.get_cursor_position(), info.get_node_rect(panel)) else {
        return Update::DoNothing;
    };
    if rect.contains(cursor) {
        return Update::DoNothing;
    }
    everywhere(request_close(&mut data, &mut info, root, None))
}

/// The "×" button: HTML `close()` - no `cancel`, the return value unchanged.
pub(crate) extern "C" fn on_dialog_close_button(_data: RefAny, mut info: CallbackInfo) -> Update {
    let close = info.get_hit_node();
    if let Some((root, _)) = find_dialog(&mut info, close) {
        info.set_transient_window_open(root, false);
    }
    Update::DoNothing
}

/// The engine closed the dialog (its own close request, the close button,
/// `close_from`, a light dismiss): the `close` event, in the app's window.
pub(crate) extern "C" fn on_dialog_dismissed(mut data: RefAny, info: CallbackInfo) -> Update {
    let (on_close, compat, state) = {
        let Some(d) = data.downcast_ref::<DialogData>() else {
            return Update::DoNothing;
        };
        (d.on_close.clone(), d.compat.clone(), d.state(false))
    };
    let update = fire_on_close(on_close, info, state);
    update.max(compat.notify(info, false))
}

/// Reconcile: the old allocation survives (its return value), adopting the
/// app's configuration and callbacks from the new build. The app's `open`
/// going false -> true is a fresh showing: the old return value goes.
extern "C" fn merge_dialog_data(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    let merged = {
        let new_guard = new_data.downcast_ref::<DialogData>();
        let old_guard = old_data.downcast_mut::<DialogData>();
        if let (Some(new_g), Some(mut old_g)) = (new_guard, old_guard) {
            old_g.modal = new_g.modal;
            old_g.closed_by = new_g.closed_by;
            old_g.on_cancel = new_g.on_cancel.clone();
            old_g.on_close = new_g.on_close.clone();
            old_g.compat = new_g.compat.clone();
            if new_g.declared_open && !old_g.declared_open {
                old_g.return_value = AzString::from_const_str("");
            }
            old_g.declared_open = new_g.declared_open;
            true
        } else {
            false
        }
    };
    if merged {
        old_data
    } else {
        new_data
    }
}

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

/// The wrapper: an inline-block around the invoker, the anchor a non-modal
/// dialog opens from.
static DIALOG_WRAPPER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

/// The clickable invoker.
static DIALOG_INVOKER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
];

/// The default `::backdrop`: the modal dialog's whole window, dimmed, with
/// the panel centred on it.
pub(crate) fn default_backdrop_style() -> CssPropertyWithConditionsVec {
    let bg_vec = StyleBackgroundContentVec::from_vec(alloc::vec![StyleBackgroundContent::Color(
        BACKDROP_COLOR
    )]);
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
        CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Absolute)),
        CssPropertyWithConditions::simple(CssProperty::const_top(LayoutTop::const_px(0))),
        CssPropertyWithConditions::simple(CssProperty::const_left(LayoutLeft::const_px(0))),
        CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::Px(
            PixelValue::const_percent(100),
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::Px(
            PixelValue::const_percent(100),
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
            LayoutFlexDirection::Row,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_justify_content(
            LayoutJustifyContent::Center,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(bg_vec)),
    ])
}

/// The dialog panel: a bordered, rounded surface in the window's own colours.
static DIALOG_PANEL_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(
        PANEL_MIN_WIDTH,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_max_width(LayoutMaxWidth::const_px(
        PANEL_MAX_WIDTH,
    ))),
    // padding: 20px
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        20,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(20),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(
        LayoutPaddingLeft::const_px(20),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(20),
    )),
    // border: 1px solid #cccccc
    CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
        LayoutBorderTopWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
        LayoutBorderLeftWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
        LayoutBorderRightWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_style(StyleBorderTopStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
        StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: PANEL_BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: PANEL_BORDER_COLOR,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: PANEL_BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: PANEL_BORDER_COLOR,
        },
    )),
    // Dark theme: the dialog is a panel on the desktop's window surface,
    // outlined with its separator.
    system_palette::DARK_SEPARATOR_BORDER_TOP,
    system_palette::DARK_SEPARATOR_BORDER_BOTTOM,
    system_palette::DARK_SEPARATOR_BORDER_LEFT,
    system_palette::DARK_SEPARATOR_BORDER_RIGHT,
    // border-radius: 8px
    CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
        StyleBorderTopLeftRadius::const_px(PANEL_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
        StyleBorderTopRightRadius::const_px(PANEL_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
        StyleBorderBottomLeftRadius::const_px(PANEL_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
        StyleBorderBottomRightRadius::const_px(PANEL_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(14))),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(&[StyleBackgroundContent::Color(
            PANEL_BG_COLOR,
        )]),
    )),
    system_palette::DARK_WINDOW_BACKGROUND,
];

/// The title row: larger, dark text; the right padding keeps it clear of
/// the absolutely-positioned "×".
static DIALOG_TITLE_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(18))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: TITLE_COLOR,
    })),
    system_palette::DARK_TEXT,
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Left)),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(24),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(12),
    )),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// With no title: an empty row as tall as the close button, so the button
/// never covers the content.
static DIALOG_CLOSE_ROW_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_min_height(LayoutMinHeight::const_px(
        CLOSE_ROW_HEIGHT,
    ))),
];

/// The "×" close button: absolutely positioned in the panel's top-right
/// corner.
static DIALOG_CLOSE_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Absolute)),
    CssPropertyWithConditions::simple(CssProperty::const_top(LayoutTop::const_px(8))),
    CssPropertyWithConditions::simple(CssProperty::const_right(LayoutRight::const_px(12))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(22))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: CLOSE_COLOR,
    })),
    system_palette::DARK_SECONDARY_TEXT,
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// The content wrapper: takes the remaining height.
static DIALOG_CONTENT_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
];

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, NodeId},
        geom::{LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{KeyboardState, MonitorVec, RawWindowHandle},
    };
    use azul_css::system::SystemStyle;
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Harness
    // ------------------------------------------------------------------

    fn node(i: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i))),
        }
    }

    /// A `DomLayoutResult` with an EMPTY layout tree: the handlers only walk
    /// the node hierarchy and read datasets, so no real layout is needed.
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

    /// Runs `f` on a `CallbackInfo` whose window holds `styled` (hit node
    /// `hit`, keyboard `keys`), after `prepare` ran on the `LayoutWindow`.
    /// Returns `f`'s result and every change the callback queued.
    fn with_info<R>(
        styled: StyledDom,
        hit: usize,
        keys: KeyboardState,
        prepare: impl FnOnce(&mut LayoutWindow),
        f: impl FnOnce(CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        layout_window
            .layout_results
            .insert(DomId::ROOT_ID, layout_result(styled));
        prepare(&mut layout_window);

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

    fn no_keys() -> KeyboardState {
        KeyboardState::default()
    }

    fn escape() -> KeyboardState {
        let mut ks = KeyboardState::default();
        ks.pressed_virtual_keycodes = vec![VirtualKeyCode::Escape].into();
        ks.current_virtual_keycode = Some(VirtualKeyCode::Escape).into();
        ks
    }

    /// Index of the first node carrying `class` in `styled`.
    fn index_of(styled: &StyledDom, class: &str) -> usize {
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

    /// The dialog rendered, its flattened DOM, and the state `RefAny` its
    /// handlers share (the wrapper's dataset).
    fn rendered(dialog: Dialog) -> (StyledDom, RefAny) {
        let dom = dialog.dom();
        let data = dom
            .root
            .get_dataset()
            .cloned()
            .expect("the wrapper carries the dialog's state");
        (StyledDom::create_from_dom(dom), data)
    }

    /// Every `SetTransientWindowOpen` in `changes`, as `(node index, open)`.
    fn opens(changes: &[CallbackChange]) -> Vec<(usize, bool)> {
        changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::SetTransientWindowOpen { node, open } => {
                    node.node.into_crate_internal().map(|n| (n.index(), *open))
                }
                _ => None,
            })
            .collect()
    }

    fn prevented(changes: &[CallbackChange]) -> bool {
        changes
            .iter()
            .any(|c| matches!(c, CallbackChange::PreventDefault))
    }

    /// What the app's `cancel` / `close` callbacks saw.
    /// No asserts in here: a panic cannot unwind out of an `extern "C" fn`,
    /// it would abort the whole test binary. The tests assert on the log.
    #[derive(Default)]
    struct Log {
        cancels: usize,
        /// `DialogState::open` as each `cancel` saw it.
        cancel_saw_open: Vec<bool>,
        closes: Vec<String>,
        /// `DialogState::open` as each `close` saw it.
        close_saw_open: Vec<bool>,
        /// Make `cancel` call `prevent_default`.
        keep_open: bool,
    }

    extern "C" fn log_cancel(mut data: RefAny, mut info: CallbackInfo, state: DialogState) -> Update {
        let keep_open = match data.downcast_mut::<Log>() {
            Some(mut log) => {
                log.cancels += 1;
                log.cancel_saw_open.push(state.open);
                log.keep_open
            }
            None => false,
        };
        if keep_open {
            info.prevent_default();
        }
        Update::RefreshDom
    }

    extern "C" fn log_close(mut data: RefAny, _info: CallbackInfo, state: DialogState) -> Update {
        if let Some(mut log) = data.downcast_mut::<Log>() {
            log.closes.push(state.return_value.as_str().to_string());
            log.close_saw_open.push(state.open);
        }
        Update::RefreshDom
    }

    fn saw_open(log: &RefAny) -> (Vec<bool>, Vec<bool>) {
        let mut log = log.clone();
        let v = log
            .downcast_ref::<Log>()
            .map(|l| (l.cancel_saw_open.clone(), l.close_saw_open.clone()))
            .unwrap_or_default();
        v
    }

    /// A dialog whose `cancel` / `close` report into `log`.
    fn logged(dialog: Dialog, log: &RefAny) -> Dialog {
        let on_cancel: DialogOnCancelCallbackType = log_cancel;
        let on_close: DialogOnCloseCallbackType = log_close;
        dialog
            .with_on_cancel(log.clone(), on_cancel)
            .with_on_close(log.clone(), on_close)
    }

    fn cancels(log: &RefAny) -> usize {
        let mut log = log.clone();
        let n = log.downcast_ref::<Log>().map_or(usize::MAX, |l| l.cancels);
        n
    }

    fn closes(log: &RefAny) -> Vec<String> {
        let mut log = log.clone();
        let v = log
            .downcast_ref::<Log>()
            .map(|l| l.closes.clone())
            .unwrap_or_default();
        v
    }

    fn return_value(data: &RefAny) -> String {
        let mut data = data.clone();
        let v = data
            .downcast_ref::<DialogData>()
            .map(|d| d.return_value().as_str().to_string())
            .expect("the state is a DialogData");
        v
    }

    fn body() -> Dom {
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(vec![Class("body".into())]))
            .with_child(Dom::create_p_with_text("Really delete?"))
    }

    // ------------------------------------------------------------------
    // closedby
    // ------------------------------------------------------------------

    /// HTML: no `closedby` means "closerequest" for a modal dialog and "none"
    /// for a non-modal one. Escape is always the dialog's own (its `cancel`
    /// runs first); only a non-modal light dismiss is the engine's.
    #[test]
    fn closed_by_resolves_like_the_html_attribute() {
        use DialogClosedBy::{Any, Auto, CloseRequest, None as Never};
        assert_eq!(Auto.resolve(true), CloseRequest);
        assert_eq!(Auto.resolve(false), Never);
        for modal in [false, true] {
            assert_eq!(Any.resolve(modal), Any);
            assert!(Any.allows_close_request(modal) && Any.allows_light_dismiss(modal));
            assert!(CloseRequest.allows_close_request(modal));
            assert!(!CloseRequest.allows_light_dismiss(modal));
            assert!(!Never.allows_close_request(modal) && !Never.allows_light_dismiss(modal));
        }
        assert_eq!(Any.transient_dismiss(false), TransientDismiss::OutsideOnly);
        for c in [Auto, CloseRequest, Never] {
            assert_eq!(c.transient_dismiss(false), TransientDismiss::None, "{c:?}");
        }
        for c in [Auto, Any, CloseRequest, Never] {
            assert_eq!(
                c.transient_dismiss(true),
                TransientDismiss::None,
                "a modal dialog's backdrop press is its own ({c:?})"
            );
        }
    }

    // ------------------------------------------------------------------
    // Structure
    // ------------------------------------------------------------------

    /// `show_modal()`: the `<transient-window>` covers the parent window
    /// (`anchor="viewport"`), is open, leaves Escape to the dialog, has
    /// per-pixel alpha, and is itself the `::backdrop` - it answers Escape and
    /// backdrop presses and reports the close.
    #[test]
    fn a_modal_dialog_is_a_viewport_window_whose_root_is_the_backdrop() {
        let dom = Dialog::create(body())
            .with_title("Delete file".into())
            .show_modal()
            .dom();
        let window = &dom.children.as_ref()[0];
        let NodeType::TransientWindow(cfg) = window.root.get_node_type() else {
            panic!("the wrapper's only child (no invoker) is the dialog's window");
        };
        assert!(cfg.open);
        assert_eq!(cfg.anchor, TransientAnchor::Viewport);
        assert_eq!(cfg.dismiss, TransientDismiss::None);
        assert_eq!(
            cfg.material,
            azul_core::window::WindowBackgroundMaterial::Transparent
        );
        let events: Vec<EventFilter> = window
            .root
            .get_callbacks()
            .as_ref()
            .iter()
            .map(|c| c.event)
            .collect();
        for e in [
            EventFilter::Component(ComponentEventFilter::Dismissed),
            EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
            EventFilter::Hover(HoverEventFilter::LeftMouseDown),
        ] {
            assert!(events.contains(&e), "{e:?} missing from {events:?}");
        }
        assert!(
            window.root.style.iter_inline_properties().any(|(p, _)| matches!(
                p,
                CssProperty::BackgroundContent(_)
            )),
            "the window root paints the backdrop"
        );
    }

    /// `show()` with an invoker: a popup below the invoker, light-dismissed
    /// by the engine only with `closedby="any"`, and no backdrop handler.
    #[test]
    fn a_non_modal_dialog_opens_below_its_invoker() {
        for (closed_by, dismiss) in [
            (DialogClosedBy::Auto, TransientDismiss::None),
            (DialogClosedBy::Any, TransientDismiss::OutsideOnly),
        ] {
            let dom = Dialog::create(body())
                .with_invoker(Dom::create_p_with_text("Open"))
                .with_closed_by(closed_by)
                .dom();
            let kids = dom.children.as_ref();
            assert_eq!(kids.len(), 2, "[invoker, window]");
            assert!(kids[0]
                .root
                .get_callbacks()
                .as_ref()
                .iter()
                .any(|c| c.event == EventFilter::Hover(HoverEventFilter::Click)));
            let NodeType::TransientWindow(cfg) = kids[1].root.get_node_type() else {
                panic!("the invoker's next sibling is the dialog's window");
            };
            assert!(!cfg.open, "closed until shown");
            assert_eq!(cfg.anchor, TransientAnchor::Bottom);
            assert_eq!(cfg.dismiss, dismiss, "{closed_by:?}");
            assert!(!kids[1]
                .root
                .get_callbacks()
                .as_ref()
                .iter()
                .any(|c| c.event == EventFilter::Hover(HoverEventFilter::LeftMouseDown)));
        }
    }

    /// The panel is announced as a dialog, named by its title ("Dialog"
    /// without one), and the close button - a named push button - comes LAST
    /// in tree order, so the dialog's first tab stop is its content's.
    #[test]
    fn the_panel_is_a_named_dialog_and_its_close_button_comes_last() {
        for (title, name) in [("Delete file", "Delete file"), ("", "Dialog")] {
            let dom = Dialog::create(body()).with_title(title.into()).dom();
            let panel = &dom.children.as_ref()[0].children.as_ref()[0];
            let info = panel
                .root
                .get_accessibility_info()
                .expect("the panel declares its role");
            assert_eq!(info.role, AccessibilityRole::Dialog);
            assert_eq!(
                info.accessibility_name.as_ref().map(|s| s.as_str()),
                Some(name)
            );
            let last = panel.children.as_ref().last().expect("children");
            let close = last
                .root
                .get_accessibility_info()
                .expect("the close button is named");
            assert_eq!(close.role, AccessibilityRole::PushButton);
            assert_eq!(
                close.accessibility_name.as_ref().map(|s| s.as_str()),
                Some("Close")
            );
        }
    }

    // ------------------------------------------------------------------
    // Showing and closing
    // ------------------------------------------------------------------

    /// A click on the invoker of a closed dialog shows it.
    #[test]
    fn a_click_on_the_invoker_shows_the_dialog() {
        let (styled, data) = rendered(
            Dialog::create(body())
                .with_invoker(Dom::create_p_with_text("Open"))
                .with_modal(true),
        );
        let invoker = index_of(&styled, DIALOG_INVOKER_CLASS);
        let window = index_of(&styled, DIALOG_WINDOW_CLASS);
        let (_, changes) = with_info(styled, invoker, no_keys(), |_| {}, |info| {
            on_dialog_invoker_click(data.clone(), info)
        });
        assert_eq!(opens(&changes), vec![(window, true)]);
    }

    /// A click on the invoker of a dialog that is showing closes it, and -
    /// no `Dismissed` follows a close through the API - reports the close
    /// itself.
    #[test]
    fn a_click_on_the_invoker_of_a_showing_dialog_closes_it_and_reports_close() {
        let log = RefAny::new(Log::default());
        let (styled, data) = rendered(logged(
            Dialog::create(body()).with_invoker(Dom::create_p_with_text("Open")),
            &log,
        ));
        let invoker = index_of(&styled, DIALOG_INVOKER_CLASS);
        let window = index_of(&styled, DIALOG_WINDOW_CLASS);
        let (update, changes) = with_info(
            styled,
            invoker,
            no_keys(),
            |lw| {
                let _ = lw
                    .transient_windows
                    .set_forced_open(NodeId::new(window), true);
            },
            |info| on_dialog_invoker_click(data.clone(), info),
        );
        assert_eq!(opens(&changes), vec![(window, false)]);
        assert_eq!(closes(&log), vec![String::new()], "one close event");
        assert_eq!(update, Update::RefreshDom, "the close callback's update");
    }

    /// Escape in a modal dialog is a close request: `cancel` runs, then the
    /// dialog closes its own window, and the Escape is spent (no default
    /// action). The app's `RefreshDom` reaches every window.
    #[test]
    fn escape_runs_cancel_then_closes_the_dialog() {
        let log = RefAny::new(Log::default());
        let (styled, data) = rendered(logged(Dialog::create(body()).show_modal(), &log));
        let window = index_of(&styled, DIALOG_WINDOW_CLASS);
        let (update, changes) = with_info(styled, window, escape(), |_| {}, |info| {
            on_dialog_key(data.clone(), info)
        });
        assert_eq!(cancels(&log), 1, "cancel ran once");
        assert_eq!(saw_open(&log).0, vec![true], "while the dialog was open");
        assert_eq!(opens(&changes), vec![(window, false)], "and the dialog closed");
        assert!(prevented(&changes), "the Escape is the dialog's");
        assert_eq!(update, Update::RefreshDomAllWindows);
    }

    /// `event.preventDefault()` in `cancel` keeps the dialog open.
    #[test]
    fn a_cancel_handler_that_prevents_default_keeps_the_dialog_open() {
        let log = RefAny::new(Log {
            keep_open: true,
            ..Log::default()
        });
        let (styled, data) = rendered(logged(Dialog::create(body()).show_modal(), &log));
        let window = index_of(&styled, DIALOG_WINDOW_CLASS);
        let (_, changes) = with_info(styled, window, escape(), |_| {}, |info| {
            on_dialog_key(data.clone(), info)
        });
        assert_eq!(cancels(&log), 1, "cancel ran");
        assert!(opens(&changes).is_empty(), "and kept the dialog open");
    }

    /// `closedby="none"` (a non-modal dialog's default): Escape does nothing
    /// - no cancel, no close, and the key is left alone. Other keys never
    /// close a dialog.
    #[test]
    fn escape_is_ignored_with_closedby_none_and_other_keys_always() {
        let log = RefAny::new(Log::default());
        let (styled, data) = rendered(logged(Dialog::create(body()).show(), &log));
        let window = index_of(&styled, DIALOG_WINDOW_CLASS);
        let (_, changes) = with_info(styled, window, escape(), |_| {}, |info| {
            on_dialog_key(data.clone(), info)
        });
        assert!(changes.is_empty(), "{changes:?}");
        assert_eq!(cancels(&log), 0);

        let (styled, data) = rendered(logged(Dialog::create(body()).show_modal(), &log));
        let window = index_of(&styled, DIALOG_WINDOW_CLASS);
        let mut enter = KeyboardState::default();
        enter.current_virtual_keycode = Some(VirtualKeyCode::Return).into();
        let (_, changes) = with_info(styled, window, enter, |_| {}, |info| {
            on_dialog_key(data.clone(), info)
        });
        assert!(changes.is_empty(), "{changes:?}");
        assert_eq!(cancels(&log), 0);
    }

    /// The "×" is HTML `close()`: no `cancel`, the return value unchanged.
    #[test]
    fn the_close_button_closes_without_a_cancel() {
        let log = RefAny::new(Log::default());
        let (styled, data) = rendered(logged(
            Dialog::create(body())
                .show_modal()
                .with_return_value("kept".into()),
            &log,
        ));
        let window = index_of(&styled, DIALOG_WINDOW_CLASS);
        let close = index_of(&styled, DIALOG_CLOSE_CLASS);
        let (_, changes) = with_info(styled, close, no_keys(), |_| {}, |info| {
            on_dialog_close_button(data.clone(), info)
        });
        assert_eq!(opens(&changes), vec![(window, false)]);
        assert_eq!(cancels(&log), 0, "no cancel");
        assert_eq!(return_value(&data), "kept");
    }

    /// `Dialog::close_from` from a control inside the dialog records the
    /// return value and closes the dialog, without a `cancel`.
    #[test]
    fn close_from_a_control_inside_sets_the_return_value_and_closes() {
        let log = RefAny::new(Log::default());
        let (styled, data) = rendered(logged(Dialog::create(body()).show_modal(), &log));
        let window = index_of(&styled, DIALOG_WINDOW_CLASS);
        let inside = index_of(&styled, "body");
        let (found, changes) = with_info(styled, inside, no_keys(), |_| {}, |mut info| {
            Dialog::close_from(&mut info, node(inside), "ok".into())
        });
        assert!(found, "the control is inside a dialog");
        assert_eq!(opens(&changes), vec![(window, false)]);
        assert_eq!(return_value(&data), "ok");
        assert_eq!(cancels(&log), 0);
    }

    /// `Dialog::request_close_from` runs `cancel` first; prevented, nothing
    /// closes and the return value stays.
    #[test]
    fn request_close_from_runs_cancel_first() {
        let log = RefAny::new(Log {
            keep_open: true,
            ..Log::default()
        });
        let (styled, data) = rendered(logged(Dialog::create(body()).show_modal(), &log));
        let inside = index_of(&styled, "body");
        let (_, changes) = with_info(styled, inside, no_keys(), |_| {}, |mut info| {
            Dialog::request_close_from(&mut info, node(inside), "ok".into())
        });
        assert_eq!(cancels(&log), 1);
        assert!(opens(&changes).is_empty());
        assert_eq!(return_value(&data), "", "a prevented close records nothing");
    }

    /// The engine closed the dialog (`Dismissed`, in the app's window): the
    /// `close` event carries the return value `close_from` recorded.
    #[test]
    fn the_close_event_carries_the_return_value() {
        let log = RefAny::new(Log::default());
        let (styled, data) = rendered(logged(Dialog::create(body()).show_modal(), &log));
        let window = index_of(&styled, DIALOG_WINDOW_CLASS);
        let inside = index_of(&styled, "body");
        let _ = with_info(styled.clone(), inside, no_keys(), |_| {}, |mut info| {
            Dialog::close_from(&mut info, node(inside), "ok".into())
        });
        let (update, _) = with_info(styled, window, no_keys(), |_| {}, |info| {
            on_dialog_dismissed(data.clone(), info)
        });
        assert_eq!(closes(&log), vec!["ok".to_string()]);
        assert_eq!(saw_open(&log).1, vec![false], "once it is closed");
        assert_eq!(update, Update::RefreshDom);
    }

    // ------------------------------------------------------------------
    // Across rebuilds
    // ------------------------------------------------------------------

    fn dataset_of(dialog: Dialog) -> RefAny {
        dialog
            .dom()
            .root
            .get_dataset()
            .cloned()
            .expect("dataset")
    }

    /// The app's rebuild keeps the dialog's return value (the old
    /// allocation) and adopts the new build's configuration.
    #[test]
    fn a_rebuild_keeps_the_return_value_and_adopts_the_new_configuration() {
        let mut old = dataset_of(Dialog::create(body()).show_modal());
        if let Some(mut d) = old.downcast_mut::<DialogData>() {
            d.return_value = "ok".into();
        }
        let new = dataset_of(
            Dialog::create(body())
                .show_modal()
                .with_closed_by(DialogClosedBy::Any),
        );
        let merged = merge_dialog_data(new, old.clone());
        assert_eq!(return_value(&merged), "ok", "the return value survives");
        let mut merged = merged;
        let closed_by = merged
            .downcast_ref::<DialogData>()
            .map(|d| d.closed_by())
            .expect("DialogData");
        assert_eq!(closed_by, DialogClosedBy::Any, "the new closedby is adopted");
    }

    /// The app's `open` going false -> true is a fresh showing: the last
    /// return value is forgotten.
    #[test]
    fn showing_again_through_open_forgets_the_last_return_value() {
        let mut old = dataset_of(Dialog::create(body()).show_modal().with_open(false));
        if let Some(mut d) = old.downcast_mut::<DialogData>() {
            d.return_value = "ok".into();
        }
        let merged = merge_dialog_data(dataset_of(Dialog::create(body()).show_modal()), old);
        assert_eq!(return_value(&merged), "");
    }
}
