//! Rich-text editor widget: ONE editor for AzNotes, AzMail's compose window
//! and AzWriter (scripts/DEDUP_EDITORS A2, C2).
//!
//! The document is a [`RichTextDoc`] (`widgets::rich_text`): a flat list of
//! blocks, each a list of styled runs. The editor renders it as ONE
//! `contenteditable` host whose children are the blocks (one element per
//! block) and whose blocks' children are the runs (one node per run: a bare
//! text node for plain text, a `span` / an `a` with the run's formats
//! otherwise), so the engine's child-index paths ARE model indices:
//! `[block]` from the host, `[block, run]` below it (`[block, row, cell]` in a
//! table). A check item's box is the one extra child, an island AFTER the
//! runs (`contenteditable=false`, absolutely placed), so the runs keep their
//! indices.
//!
//! The engine edits, the editor follows (the engine's "Path 2" contract):
//! - typing: `TextChanged` -> `get_unsynced_text_edits` (each block's text as
//!   the user sees it) -> [`RichTextDoc::sync_block_text`] (the runs keep
//!   their formats; typing continues the format on the left) -> ack the
//!   revision; no rebuild. A Markdown shortcut typed at a block's start
//!   (`# `, `- `, `1. `, `[ ] `, `> `, ` ``` `) changes the block.
//! - Enter / Backspace at a block's start / a delete or paste across blocks:
//!   `DocumentEdit` -> the split / merge / replace applied to the model ->
//!   acknowledged (WITHOUT an inverse: the editor's history is the only one)
//!   -> rebuild; the engine puts the caret at the edit's resume point.
//! - the keys the browser model leaves to an editor: Ctrl/Cmd+B / I / U over
//!   a selection, Ctrl/Cmd+Shift+X (strike), Ctrl/Cmd+E (code), Ctrl/Cmd+0..3
//!   (paragraph, headings), Ctrl/Cmd+Shift+7 / 8 / 9 (lists, checks),
//!   Ctrl/Cmd+Enter (tick), Enter in a code block, Enter on an empty list item
//!   or quoted line, Backspace at a list item's / heading's / quote's start,
//!   Tab / Shift+Tab in a list.
//!
//! The app owns the [`RichTextEditorState`] (the document, its ONE undo
//! history, the typing style, where the caret was): it builds the editor
//! from it, and stores what [`RichTextEditor::with_on_change`] hands back
//! after every change. Its own toolbar or ribbon runs commands on the state
//! it keeps ([`RichTextEditorState::apply_command`]); the editor's optional
//! built-in toolbar ([`RichTextToolbar`]) runs the same commands.
//!
//! Key types: [`RichTextEditor`], [`RichTextEditorState`],
//! [`RichTextCommand`], [`RichTextToolbar`].

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use azul_core::{
    callbacks::{FocusTarget, Update},
    dom::{AttributeType, Dom, DomId, DomNodeId, EventFilter, HoverEventFilter},
    events::{FocusEventFilter, TextFormat},
    refany::RefAny,
    resources::ImageRef,
    styled_dom::NodeHierarchyItemId,
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::CssPropertyWithConditions, impl_option, impl_option_inner, impl_vec,
    impl_vec_clone, impl_vec_debug, impl_vec_eq, impl_vec_mut, impl_vec_partialeq, AzString,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::rich_text::{
        doc::{
            typed_shortcut, RichAlign, RichBlock, RichBlockKind, RichCheck, RichFormat,
            RichFormats, RichImage, RichRun, RichTable, RichTextDoc,
        },
        history::{RichEditGroup, RichTextHistory},
        html as rich_html,
    },
};

/// The DOM id the editing host carries unless the app names another
/// ([`RichTextEditor::with_id`]).
pub const DEFAULT_HOST_ID: &str = "az-rich-text";

/// The class of the editor's outer frame.
pub const RICH_TEXT_EDITOR_CLASS: &str = "__azul-native-rich-text-editor";
/// The class of the built-in toolbar strip.
pub const RICH_TEXT_TOOLBAR_CLASS: &str = "__azul-native-rich-text-editor-toolbar";
/// The class of the page the host sits on.
pub const RICH_TEXT_PAGE_CLASS: &str = "__azul-native-rich-text-editor-page";
/// The class of the editing host.
pub const RICH_TEXT_HOST_CLASS: &str = "__azul-native-rich-text-editor-host";

// ==== The state the app keeps ====

/// The typing style: the formats text typed at a caret takes after a format
/// toggle at that caret (Ctrl/Cmd+B with nothing selected). Kept while text
/// is inserted where the last insertion ended, dropped by anything else.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct RichTypingStyle {
    /// The block the caret is in.
    pub block: usize,
    /// The caret's byte in the block's text.
    pub at: usize,
    pub formats: RichFormats,
}

impl_option!(
    RichTypingStyle,
    OptionRichTypingStyle,
    [Debug, Clone, PartialEq, Eq]
);

/// Everything an editor remembers between builds - the app keeps it (and
/// stores what `on_change` hands back).
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichTextEditorState {
    /// The document.
    pub doc: RichTextDoc,
    /// The ONE undo / redo history (typing, structure, formats).
    pub history: RichTextHistory,
    /// The DOM id of the editing host (unique in its window).
    pub host_id: AzString,
    /// The block the caret was last seen in (a toolbar button's target when
    /// the button took the focus).
    pub caret_block: usize,
    /// The caret's byte in that block's text.
    pub caret_byte: usize,
    /// Counts the changes: an app can tell whether anything changed since
    /// it last saved.
    pub revision: u64,
    /// The typing style at the caret, if a format was toggled there.
    pub typing: OptionRichTypingStyle,
}

impl Default for RichTextEditorState {
    fn default() -> Self {
        Self::create(RichTextDoc::create())
    }
}

/// A command the editor runs on its document: a toolbar button, a ribbon,
/// a shortcut.
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RichTextCommand {
    /// Over the selection: set when a part lacks it, else clear. At a caret:
    /// the typing style of what is typed next.
    ToggleFormat(RichFormat),
    /// The target blocks become this kind, or paragraphs again when the
    /// first already is one of that family (a list item keeps its indent).
    ToggleKind(RichBlockKind),
    /// The target blocks are quoted, or unquoted when the first is quoted.
    ToggleQuote,
    /// List items one level deeper.
    Indent,
    /// List items one level up (out of the list at level 0).
    Outdent,
    SetAlign(RichAlign),
    /// A horizontal rule after the caret's block (and an empty paragraph
    /// for the caret).
    InsertRule,
    /// A page break after the caret's block.
    InsertPageBreak,
    InsertImage(RichImage),
    InsertTable(RichTableSize),
    /// The selection becomes a link to this address; with nothing
    /// selected, the address is inserted at the caret as a link.
    SetLink(AzString),
    /// The selection (or the link under the caret) is a link no more.
    RemoveLink,
    /// Ticks or unticks the caret's check item.
    ToggleCheck,
    Undo,
    Redo,
}

/// The size of a new table.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct RichTableSize {
    pub rows: usize,
    pub columns: usize,
}

/// A selected range of one block's text (bytes `start..end`): what
/// [`RichTextEditorState::get_selection`] hands an app that acts on the
/// selection after its own UI took the focus (a link sheet).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct RichTextSpan {
    pub block: usize,
    pub start: usize,
    pub end: usize,
}

impl_option!(
    RichTextSpan,
    OptionRichTextSpan,
    [Debug, Clone, Copy, PartialEq, Eq]
);
impl_vec!(
    RichTextSpan,
    RichTextSpanVec,
    RichTextSpanVecDestructor,
    RichTextSpanVecDestructorType,
    RichTextSpanVecSlice,
    OptionRichTextSpan
);
impl_vec_clone!(RichTextSpan, RichTextSpanVec, RichTextSpanVecDestructor);
impl_vec_debug!(RichTextSpan, RichTextSpanVec);
impl_vec_partialeq!(RichTextSpan, RichTextSpanVec);
impl_vec_eq!(RichTextSpan, RichTextSpanVec);
impl_vec_mut!(RichTextSpan, RichTextSpanVec);

/// Which groups of buttons the built-in toolbar shows (none by default:
/// apps with a ribbon or their own toolbar run [`RichTextCommand`]s).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct RichTextToolbar {
    /// Bold, italic, underline, strikethrough, inline code.
    pub formats: bool,
    /// Heading 1 - 3.
    pub headings: bool,
    /// Bullets, numbering, check items.
    pub lists: bool,
    /// Quote, code block, horizontal rule.
    pub blocks: bool,
    /// Indent, outdent.
    pub indent: bool,
    /// Left, center, right, justify.
    pub align: bool,
    /// Undo, redo.
    pub history: bool,
}

impl RichTextToolbar {
    /// No toolbar.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            formats: false,
            headings: false,
            lists: false,
            blocks: false,
            indent: false,
            align: false,
            history: false,
        }
    }

    /// Formats and lists: a notes field (a task's notes, a contact's).
    #[must_use]
    pub const fn minimal() -> Self {
        Self {
            formats: true,
            lists: true,
            ..Self::none()
        }
    }

    /// Every group.
    #[must_use]
    pub const fn full() -> Self {
        Self {
            formats: true,
            headings: true,
            lists: true,
            blocks: true,
            indent: true,
            align: true,
            history: true,
        }
    }

    /// No group is shown.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        !(self.formats
            || self.headings
            || self.lists
            || self.blocks
            || self.indent
            || self.align
            || self.history)
    }
}

/// A picture the app resolved for an image block's `src`.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct RichImageSource {
    pub src: AzString,
    pub image: ImageRef,
}

impl_option!(
    RichImageSource,
    OptionRichImageSource,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    RichImageSource,
    RichImageSourceVec,
    RichImageSourceVecDestructor,
    RichImageSourceVecDestructorType,
    RichImageSourceVecSlice,
    OptionRichImageSource
);
impl_vec_clone!(RichImageSource, RichImageSourceVec, RichImageSourceVecDestructor);
impl_vec_debug!(RichImageSource, RichImageSourceVec);
impl_vec_mut!(RichImageSource, RichImageSourceVec);

impl PartialEq for RichImageSourceVec {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

// ==== Callbacks ====

/// Invoked after every change of the document (typing, a structural edit,
/// a command, an undo) with the editor's new state: the app stores it.
pub type RichTextEditorOnChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, RichTextEditorState) -> Update;
impl_widget_callback!(
    RichTextEditorOnChange,
    OptionRichTextEditorOnChange,
    RichTextEditorOnChangeCallback,
    RichTextEditorOnChangeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        RichTextEditorOnChangeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: RICH_TEXT_EDITOR_ON_CHANGE_INVOKER,
    invoker_ty:     AzRichTextEditorOnChangeCallbackInvoker,
    thunk_fn:       az_rich_text_editor_on_change_callback_thunk,
    setter_fn:      AzApp_setRichTextEditorOnChangeCallbackInvoker,
    from_handle_fn: AzRichTextEditorOnChangeCallback_createFromHostHandle,
    from_handle_byref_fn: AzRichTextEditorOnChangeCallback_createFromHostHandleByref,
    extra_args:     [ state: RichTextEditorState ],
}

/// Invoked when a link in the text is opened (Ctrl/Cmd + click) with its
/// address. Without one the editor opens it with the system's handler.
pub type RichTextEditorOnLinkCallbackType =
    extern "C" fn(RefAny, CallbackInfo, AzString) -> Update;
impl_widget_callback!(
    RichTextEditorOnLink,
    OptionRichTextEditorOnLink,
    RichTextEditorOnLinkCallback,
    RichTextEditorOnLinkCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        RichTextEditorOnLinkCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: RICH_TEXT_EDITOR_ON_LINK_INVOKER,
    invoker_ty:     AzRichTextEditorOnLinkCallbackInvoker,
    thunk_fn:       az_rich_text_editor_on_link_callback_thunk,
    setter_fn:      AzApp_setRichTextEditorOnLinkCallbackInvoker,
    from_handle_fn: AzRichTextEditorOnLinkCallback_createFromHostHandle,
    from_handle_byref_fn: AzRichTextEditorOnLinkCallback_createFromHostHandleByref,
    extra_args:     [ url: AzString ],
}

// ==== The widget ====

/// The rich-text editor: built from the state the app keeps, rendered by
/// [`Self::dom`].
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct RichTextEditor {
    pub state: RichTextEditorState,
    /// What the editing host is called for assistive technology.
    pub accessibility_name: AzString,
    /// The pictures the app resolved for image blocks.
    pub images: RichImageSourceVec,
    pub on_change: OptionRichTextEditorOnChange,
    pub on_link: OptionRichTextEditorOnLink,
    /// The body text size in px.
    pub font_size: f32,
    /// The space under a paragraph in px (a note's 6, a mail's 0).
    pub paragraph_spacing: f32,
    /// The widget theme the frame and toolbar are PINNED to, or `None` to
    /// follow the app theme.
    pub theme: crate::widgets::themes::OptionUiTheme,
    pub toolbar: RichTextToolbar,
    /// Markdown shortcuts typed at a block's start change the block.
    pub markdown_shortcuts: bool,
    /// A read-only view (a version, a print): no editing, no callbacks.
    pub read_only: bool,
}

impl RichTextEditor {
    /// An editor for `state` (14 px text, Markdown shortcuts on, no
    /// toolbar).
    #[must_use]
    pub fn create(state: RichTextEditorState) -> Self {
        Self {
            state,
            accessibility_name: AzString::from_const_str("Text"),
            images: RichImageSourceVec::from_const_slice(&[]),
            on_change: OptionRichTextEditorOnChange::None,
            on_link: OptionRichTextEditorOnLink::None,
            font_size: 14.0,
            paragraph_spacing: 6.0,
            toolbar: RichTextToolbar::none(),
            markdown_shortcuts: true,
            read_only: false,
            theme: crate::widgets::themes::OptionUiTheme::None,
        }
    }

    /// The editing host's DOM id (default [`DEFAULT_HOST_ID`]; unique in
    /// its window).
    pub fn set_id(&mut self, id: AzString) {
        self.state.host_id = id;
    }

    /// [`Self::set_id`] for the builder chain.
    #[must_use]
    pub fn with_id(mut self, id: AzString) -> Self {
        self.set_id(id);
        self
    }

    /// What the editing host is called for assistive technology.
    pub fn set_accessibility_name(&mut self, name: AzString) {
        self.accessibility_name = name;
    }

    /// [`Self::set_accessibility_name`] for the builder chain.
    #[must_use]
    pub fn with_accessibility_name(mut self, name: AzString) -> Self {
        self.set_accessibility_name(name);
        self
    }

    /// The picture to show for image blocks whose `src` is `src`.
    pub fn add_image(&mut self, src: AzString, image: ImageRef) {
        self.images.push(RichImageSource { src, image });
    }

    /// [`Self::add_image`] for the builder chain.
    #[must_use]
    pub fn with_image(mut self, src: AzString, image: ImageRef) -> Self {
        self.add_image(src, image);
        self
    }

    /// Called after every change of the document with the new state.
    pub fn set_on_change<C: Into<RichTextEditorOnChangeCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.on_change = OptionRichTextEditorOnChange::Some(RichTextEditorOnChange {
            refany: data,
            callback: callback.into(),
        });
    }

    /// [`Self::set_on_change`] for the builder chain.
    #[must_use]
    pub fn with_on_change<C: Into<RichTextEditorOnChangeCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_change(data, callback);
        self
    }

    /// Called when a link is opened (Ctrl/Cmd + click) instead of the
    /// system's handler.
    pub fn set_on_link<C: Into<RichTextEditorOnLinkCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_link = OptionRichTextEditorOnLink::Some(RichTextEditorOnLink {
            refany: data,
            callback: callback.into(),
        });
    }

    /// [`Self::set_on_link`] for the builder chain.
    #[must_use]
    pub fn with_on_link<C: Into<RichTextEditorOnLinkCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_link(data, callback);
        self
    }

    /// The body text size in px.
    pub fn set_font_size(&mut self, px: f32) {
        self.font_size = px;
    }

    /// [`Self::set_font_size`] for the builder chain.
    #[must_use]
    pub fn with_font_size(mut self, px: f32) -> Self {
        self.set_font_size(px);
        self
    }

    /// The space under a paragraph in px.
    pub fn set_paragraph_spacing(&mut self, px: f32) {
        self.paragraph_spacing = px;
    }

    /// [`Self::set_paragraph_spacing`] for the builder chain.
    #[must_use]
    pub fn with_paragraph_spacing(mut self, px: f32) -> Self {
        self.set_paragraph_spacing(px);
        self
    }

    /// Which groups the built-in toolbar shows.
    pub fn set_toolbar(&mut self, toolbar: RichTextToolbar) {
        self.toolbar = toolbar;
    }

    /// [`Self::set_toolbar`] for the builder chain.
    #[must_use]
    pub fn with_toolbar(mut self, toolbar: RichTextToolbar) -> Self {
        self.set_toolbar(toolbar);
        self
    }

    /// Markdown shortcuts typed at a block's start change the block.
    pub fn set_markdown_shortcuts(&mut self, on: bool) {
        self.markdown_shortcuts = on;
    }

    /// [`Self::set_markdown_shortcuts`] for the builder chain.
    #[must_use]
    pub fn with_markdown_shortcuts(mut self, on: bool) -> Self {
        self.set_markdown_shortcuts(on);
        self
    }

    /// A read-only view (a version, a print preview).
    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    /// [`Self::set_read_only`] for the builder chain.
    #[must_use]
    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.set_read_only(read_only);
        self
    }

    /// Pin the widget theme of the frame and the toolbar.
    pub fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty editor and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(RichTextEditorState::default());
        core::mem::swap(&mut s, self);
        s
    }
}

impl Default for RichTextEditor {
    fn default() -> Self {
        Self::create(RichTextEditorState::default())
    }
}

impl_option!(
    RichTextEditor,
    OptionRichTextEditor,
    copy = false,
    [Debug, Clone, PartialEq]
);

// ==== Rendering: the chrome (themed) ====

/// What a theme decides about the editor: the SKIN of the frame, the
/// toolbar strip and the page the host sits on, laid over their bases (the
/// structure, the same in every theme) by [`build_chrome`]. The document
/// itself is the user's content: it takes the page's ink and the mode's
/// system colours, the same in every theme.
pub(crate) struct RichTextEditorLook {
    pub frame: Vec<CssPropertyWithConditions>,
    pub toolbar: Vec<CssPropertyWithConditions>,
    pub page: Vec<CssPropertyWithConditions>,
}

/// The part of the editor a theme builds: the frame, an empty toolbar strip
/// (when there is a toolbar) and an empty page. [`RichTextEditor::dom`]
/// fills them, so the document is built once, not once per theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RichTextEditorChrome {
    pub toolbar: bool,
}

/// The frame: a column that takes the room it is given.
pub(crate) static RICH_TEXT_FRAME_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(azul_css::props::property::CssProperty::const_display(
        azul_css::props::layout::LayoutDisplay::Flex,
    )),
    CssPropertyWithConditions::simple(
        azul_css::props::property::CssProperty::const_flex_direction(
            azul_css::props::layout::LayoutFlexDirection::Column,
        ),
    ),
    CssPropertyWithConditions::simple(azul_css::props::property::CssProperty::const_flex_grow(
        azul_css::props::layout::LayoutFlexGrow::const_new(1),
    )),
    CssPropertyWithConditions::simple(azul_css::props::property::CssProperty::const_min_height(
        azul_css::props::layout::LayoutMinHeight::const_px(0),
    )),
];

/// The toolbar strip: one wrapping row that keeps its height.
pub(crate) static RICH_TEXT_TOOLBAR_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(azul_css::props::property::CssProperty::const_display(
        azul_css::props::layout::LayoutDisplay::Flex,
    )),
    CssPropertyWithConditions::simple(
        azul_css::props::property::CssProperty::const_flex_direction(
            azul_css::props::layout::LayoutFlexDirection::Row,
        ),
    ),
    CssPropertyWithConditions::simple(azul_css::props::property::CssProperty::const_flex_wrap(
        azul_css::props::layout::LayoutFlexWrap::Wrap,
    )),
    CssPropertyWithConditions::simple(azul_css::props::property::CssProperty::const_align_items(
        azul_css::props::layout::LayoutAlignItems::Center,
    )),
    CssPropertyWithConditions::simple(azul_css::props::property::CssProperty::const_flex_shrink(
        azul_css::props::layout::LayoutFlexShrink {
            inner: azul_css::props::basic::length::FloatValue::const_new(0),
        },
    )),
];

/// The page: the rest of the frame, scrolling the document.
pub(crate) static RICH_TEXT_PAGE_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(azul_css::props::property::CssProperty::const_display(
        azul_css::props::layout::LayoutDisplay::Block,
    )),
    CssPropertyWithConditions::simple(azul_css::props::property::CssProperty::const_flex_grow(
        azul_css::props::layout::LayoutFlexGrow::const_new(1),
    )),
    CssPropertyWithConditions::simple(azul_css::props::property::CssProperty::const_min_height(
        azul_css::props::layout::LayoutMinHeight::const_px(0),
    )),
    CssPropertyWithConditions::simple(azul_css::props::property::CssProperty::const_overflow_y(
        azul_css::props::layout::LayoutOverflow::Auto,
    )),
];

/// The chrome in `look`: frame [toolbar strip?, page], both empty.
pub(crate) fn build_chrome(chrome: RichTextEditorChrome, look: &RichTextEditorLook) -> Dom {
    let part = |base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]| {
        azul_css::dynamic_selector::CssPropertyWithConditionsVec::from_vec(
            crate::widgets::themes::decl::on_base(base, skin),
        )
    };
    let mut frame = Dom::create_div()
        .with_class(AzString::from_const_str(RICH_TEXT_EDITOR_CLASS))
        .with_css_props(part(RICH_TEXT_FRAME_BASE, &look.frame));
    if chrome.toolbar {
        frame.add_child(
            Dom::create_div()
                .with_class(AzString::from_const_str(RICH_TEXT_TOOLBAR_CLASS))
                .with_css_props(part(RICH_TEXT_TOOLBAR_BASE, &look.toolbar)),
        );
    }
    frame.add_child(
        Dom::create_div()
            .with_class(AzString::from_const_str(RICH_TEXT_PAGE_CLASS))
            .with_css_props(part(RICH_TEXT_PAGE_BASE, &look.page)),
    );
    frame
}

impl RichTextEditor {
    /// The editor's DOM: the frame (the theme's: `themes::flat::
    /// rich_text_editor` / `themes::flora::rich_text_editor`; unpinned, both
    /// in their `@theme` blocks), the toolbar, and the editing host with the
    /// document, built once.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::{flat, flora, theme_blocks, UiTheme};
        let chrome = RichTextEditorChrome {
            toolbar: !self.read_only && !self.toolbar.is_empty(),
        };
        let mut frame = match self.theme.into_option() {
            Some(UiTheme::Flat) => flat::rich_text_editor(chrome),
            Some(UiTheme::Flora) => flora::rich_text_editor(chrome),
            None => theme_blocks::follow_app_theme(
                chrome,
                flat::rich_text_editor,
                flora::rich_text_editor,
            ),
        };
        let data = self.editor_data();
        let host = host_dom(&self, data.as_ref());
        let toolbar = if chrome.toolbar {
            data.as_ref()
                .map(|data| toolbar_buttons(&self, data))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        {
            let slots = frame.children.as_mut();
            let page = if chrome.toolbar { 1 } else { 0 };
            if chrome.toolbar {
                if let Some(strip) = slots.get_mut(0) {
                    for button in toolbar {
                        strip.add_child(button);
                    }
                }
            }
            if let Some(page) = slots.get_mut(page) {
                page.add_child(host);
            }
        }
        let _ = frame.fixup_children_estimated();
        frame
    }
}

impl RichTextEditor {
    /// The editing host alone: the document without the frame and the
    /// toolbar (a print, a PDF export, an app that draws its own paper).
    #[must_use]
    pub fn content_dom(self) -> Dom {
        let data = self.editor_data();
        host_dom(&self, data.as_ref())
    }

    /// The data every callback of this editor shares (none read-only).
    fn editor_data(&self) -> Option<RefAny> {
        (!self.read_only).then(|| {
            RefAny::new(EditorData {
                state: self.state.clone(),
                on_change: self.on_change.clone(),
                on_link: self.on_link.clone(),
                markdown_shortcuts: self.markdown_shortcuts,
            })
        })
    }
}

impl From<RichTextEditor> for Dom {
    fn from(editor: RichTextEditor) -> Self {
        editor.dom()
    }
}

// ==== Rendering: the document ====

/// What the blocks are drawn with.
struct RenderCtx<'a> {
    doc: &'a RichTextDoc,
    host_id: &'a str,
    images: &'a [RichImageSource],
    font_px: f32,
    spacing: f32,
    /// The editor's shared data (`None` read-only: no callbacks).
    data: Option<&'a RefAny>,
}

/// The editing host with every block of the document.
fn host_dom(editor: &RichTextEditor, data: Option<&RefAny>) -> Dom {
    let ctx = RenderCtx {
        doc: &editor.state.doc,
        host_id: editor.state.host_id.as_str(),
        images: editor.images.as_ref(),
        font_px: editor.font_size,
        spacing: editor.paragraph_spacing,
        data,
    };
    let mut host = Dom::create_div()
        .with_id(editor.state.host_id.clone())
        .with_class(AzString::from_const_str(RICH_TEXT_HOST_CLASS))
        .with_accessibility_name(editor.accessibility_name.clone())
        .with_css(&format!(
            "display: block; padding: 10px 14px 40px 14px; font-size: {}px; cursor: text;",
            ctx.font_px
        ));
    if data.is_some() {
        host = host.with_contenteditable(true);
    }
    for (index, block) in ctx.doc.blocks().iter().enumerate() {
        host.add_child(block_dom(&ctx, index, block));
    }
    if let Some(data) = data {
        host = host
            .with_callback(
                EventFilter::Focus(FocusEventFilter::TextChanged),
                data.clone(),
                Callback::from_ptr(on_text_changed),
            )
            .with_callback(
                EventFilter::Focus(FocusEventFilter::DocumentEdit),
                data.clone(),
                Callback::from_ptr(on_document_edit),
            )
            .with_callback(
                EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                data.clone(),
                Callback::from_ptr(on_key_down),
            );
    }
    host
}

/// The payload of a link in the text.
struct LinkRef {
    editor: RefAny,
    url: String,
}

/// The payload of a check item's box: which block it ticks.
struct CheckRef {
    editor: RefAny,
    block: usize,
}

/// The payload of a toolbar button: which command it runs.
struct ToolbarRef {
    editor: RefAny,
    command: RichTextCommand,
}

/// A run as ONE child of its block: a bare text node for plain text, else
/// a `span` (an `a` for a link) carrying the run's formats as classes (read
/// back from the engine's clones) and as style.
fn run_dom(ctx: &RenderCtx<'_>, run: &RichRun) -> Dom {
    let text = Dom::create_text_do_not_use_without_block_level_wrapper(run.text.clone());
    if run.is_plain() {
        return text;
    }
    let f = run.formats;
    let mut css = String::new();
    let mut classes: Vec<&'static str> = Vec::new();
    if f.bold {
        css.push_str("font-weight: bold;");
        classes.push(rich_html::RUN_BOLD_CLASS);
    }
    if f.italic {
        css.push_str("font-style: italic;");
        classes.push(rich_html::RUN_ITALIC_CLASS);
    }
    if f.underline {
        classes.push(rich_html::RUN_UNDERLINE_CLASS);
    }
    if f.strike {
        classes.push(rich_html::RUN_STRIKE_CLASS);
    }
    match (f.underline || run.link.is_some(), f.strike) {
        (true, true) => css.push_str("text-decoration: underline line-through;"),
        (true, false) => css.push_str("text-decoration: underline;"),
        (false, true) => css.push_str("text-decoration: line-through;"),
        (false, false) => {}
    }
    if f.code {
        css.push_str(
            "font-family: monospace; background: rgba(127, 127, 127, 0.16); border-radius: 3px;",
        );
        classes.push(rich_html::RUN_CODE_CLASS);
    }
    let mut node = match run.link_str() {
        Some(url) => {
            css.push_str("color: system:link;");
            let link = Dom::create_a_no_a11y(AzString::from(url), azul_css::OptionString::None);
            match ctx.data {
                // Ctrl / Cmd + click opens the link (a plain click places the
                // caret).
                Some(data) => link.with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    RefAny::new(LinkRef {
                        editor: data.clone(),
                        url: url.to_string(),
                    }),
                    Callback::from_ptr(on_link_click),
                ),
                None => link,
            }
        }
        None => Dom::create_span(),
    };
    for class in classes {
        node = node.with_class(AzString::from_const_str(class));
    }
    node.with_css(&css).with_child(text)
}

/// `node` with the runs of `block` as its children (none for an empty
/// block: its min-height keeps its line).
fn with_runs(mut node: Dom, ctx: &RenderCtx<'_>, block: &RichBlock) -> Dom {
    for run in block.runs.as_ref() {
        if !run.text.as_str().is_empty() {
            node.add_child(run_dom(ctx, run));
        }
    }
    node
}

/// The marker of a bullet at `indent`.
const fn bullet_style(indent: u8) -> &'static str {
    match indent % 3 {
        0 => "disc",
        1 => "circle",
        _ => "square",
    }
}

/// The counter style of a numbered item at `indent`.
const fn number_style(indent: u8) -> &'static str {
    match indent % 3 {
        0 => "decimal",
        1 => "lower-alpha",
        _ => "lower-roman",
    }
}

/// One block as ONE child of the host.
#[allow(clippy::too_many_lines)]
fn block_dom(ctx: &RenderCtx<'_>, index: usize, block: &RichBlock) -> Dom {
    let px = ctx.font_px;
    let line = (px * 1.5).round();
    let depth = block.quote_depth;
    // A quoted block: a bar on its left, one level further in per level.
    let quote_left = if depth > 0 {
        16.0 * f32::from(depth - 1)
    } else {
        0.0
    };
    let quote_css = if depth > 0 {
        "border-left: 3px solid system:separator; padding-left: 10px; color: system:secondary-text;"
    } else {
        ""
    };
    let align = if block.align == RichAlign::Left {
        String::new()
    } else {
        format!("text-align: {};", block.align.css())
    };
    let base = format!(
        "margin: 0px; padding: 0px; white-space: pre-wrap; font-size: {px}px; \
         line-height: {line}px; min-height: {line}px; {align}"
    );
    let indent_px = |indent: u8| quote_left + 26.0 + 24.0 * f32::from(indent);
    let mut classes: Vec<String> = vec![rich_html::BLOCK_CLASS.to_string()];
    if depth > 0 {
        classes.push(rich_html::quote_class(depth));
    }
    let node = match &block.kind {
        RichBlockKind::Paragraph => with_runs(Dom::create_p(), ctx, block).with_css(&format!(
            "{base} {quote_css} margin-left: {quote_left}px; margin-bottom: {}px;",
            ctx.spacing
        )),
        RichBlockKind::Heading(level) => {
            let (node, scale) = match *level {
                1 => (Dom::create_h1(), 1.75),
                2 => (Dom::create_h2(), 1.4),
                3 => (Dom::create_h3(), 1.2),
                4 => (Dom::create_h4(), 1.1),
                5 => (Dom::create_h5(), 1.0),
                _ => (Dom::create_h6(), 0.95),
            };
            let size = (px * scale).round();
            with_runs(node, ctx, block).with_css(&format!(
                "margin: 0px; padding: 0px; white-space: pre-wrap; font-size: {size}px; \
                 line-height: {}px; font-weight: bold; margin-top: 10px; margin-bottom: 6px; \
                 margin-left: {quote_left}px; {quote_css} {align}",
                (size * 1.3).round()
            ))
        }
        RichBlockKind::Bullet(indent) => {
            classes.push(rich_html::BULLET_CLASS.to_string());
            classes.push(rich_html::indent_class(*indent));
            with_runs(Dom::create_li(), ctx, block).with_css(&format!(
                "{base} {quote_css} display: list-item; list-style-type: {}; margin-left: {}px; \
                 margin-bottom: 2px;",
                bullet_style(*indent),
                indent_px(*indent)
            ))
        }
        RichBlockKind::Numbered(indent) => {
            classes.push(rich_html::NUMBERED_CLASS.to_string());
            classes.push(rich_html::indent_class(*indent));
            // The item's own number: a flat list has no `ol` to count in,
            // so each item resets the list-item counter to the number before
            // it (CSS Lists 3: the marker shows the incremented value).
            let number = ctx.doc.number_of(index);
            with_runs(Dom::create_li(), ctx, block).with_css(&format!(
                "{base} {quote_css} display: list-item; list-style-type: {}; \
                 counter-reset: list-item {}; margin-left: {}px; margin-bottom: 2px;",
                number_style(*indent),
                number.saturating_sub(1),
                indent_px(*indent)
            ))
        }
        RichBlockKind::Check(check) => {
            classes.push(rich_html::CHECK_CLASS.to_string());
            classes.push(rich_html::indent_class(check.indent));
            if check.checked {
                classes.push(rich_html::CHECKED_CLASS.to_string());
            }
            let checked_css = if check.checked {
                "color: system:secondary-text; text-decoration: line-through;"
            } else {
                ""
            };
            let item = with_runs(Dom::create_li(), ctx, block).with_css(&format!(
                "{base} {quote_css} display: list-item; list-style-type: none; position: relative; \
                 padding-left: 28px; margin-left: {}px; margin-bottom: 2px; {checked_css}",
                indent_px(check.indent) - 26.0
            ));
            // The box comes AFTER the runs (they keep their child indices),
            // out of the text flow and out of the editable text.
            let (icon, name) = if check.checked {
                ("check_box", "Uncheck")
            } else {
                ("check_box_outline_blank", "Check")
            };
            let top = ((line - 20.0) / 2.0).max(0.0);
            let mut check_box = Dom::create_div()
                .with_class(AzString::from_const_str(rich_html::ISLAND_CLASS))
                .with_attribute(AttributeType::ContentEditable(false))
                .with_css(&format!(
                    "position: absolute; left: 2px; top: {top}px; width: 20px; height: 20px; \
                     cursor: pointer; color: {};",
                    if check.checked {
                        "system:secondary-text"
                    } else {
                        "system:accent"
                    }
                ))
                .with_accessibility_name(name)
                .with_child(Dom::create_icon(icon).with_css("font-size: 20px;"));
            if let Some(data) = ctx.data {
                check_box = check_box.with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    RefAny::new(CheckRef {
                        editor: data.clone(),
                        block: index,
                    }),
                    Callback::from_ptr(on_check_click),
                );
            }
            item.with_child(check_box)
        }
        RichBlockKind::Code(_) => with_runs(Dom::create_pre(), ctx, block).with_css(&format!(
            "margin: 0px; margin-top: 4px; margin-bottom: 8px; margin-left: {quote_left}px; \
             padding: 10px 12px; white-space: pre; font-family: monospace; font-size: {}px; \
             line-height: {}px; background: rgba(127, 127, 127, 0.12); border-radius: 6px;",
            (px * 0.9).round(),
            (px * 1.35).round()
        )),
        RichBlockKind::Rule => Dom::create_hr().with_css(&format!(
            "margin: 12px 0px 12px {quote_left}px; border: none; \
             border-top: 1px solid system:separator; height: 0px;"
        )),
        RichBlockKind::Image(image) => {
            let found = ctx
                .images
                .iter()
                .find(|i| i.src.as_str() == image.src.as_str());
            let inner = match found {
                Some(source) => Dom::create_image(source.image.clone()).with_css("max-width: 100%;"),
                None => Dom::create_div()
                    .with_css(
                        "padding: 16px; border: 1px dashed system:separator; \
                         color: system:secondary-text; font-size: 13px;",
                    )
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                        format!(
                            "Image: {}",
                            if image.alt.as_str().is_empty() {
                                image.src.as_str()
                            } else {
                                image.alt.as_str()
                            }
                        ),
                    )),
            };
            let name = if image.alt.as_str().is_empty() {
                AzString::from_const_str("Image")
            } else {
                image.alt.clone()
            };
            Dom::create_div()
                .with_class(AzString::from_const_str(rich_html::ISLAND_CLASS))
                .with_attribute(AttributeType::ContentEditable(false))
                .with_css(&format!(
                    "margin-top: 6px; margin-bottom: 10px; margin-left: {quote_left}px;"
                ))
                .with_accessibility_name(name)
                .with_child(inner)
        }
        RichBlockKind::PageBreak => Dom::create_page_break()
            .with_attribute(AttributeType::ContentEditable(false))
            .with_css("margin: 10px 0px; border-top: 1px dashed system:separator; height: 0px;"),
        RichBlockKind::Table(table) => table_dom(table, quote_left),
    };
    let mut node = node.with_id(AzString::from(format!("{}-{index}", ctx.host_id)));
    for class in classes {
        node = node.with_class(AzString::from(class));
    }
    node
}

/// A table block: `table > tr > th|td`, each cell's text one text node.
fn table_dom(table: &RichTable, quote_left: f32) -> Dom {
    let mut node = Dom::create_table_no_a11y().with_css(&format!(
        "border-collapse: collapse; margin: 6px 0px 10px {quote_left}px;"
    ));
    for (r, row) in table.rows.as_ref().iter().enumerate() {
        let header = table.has_header && r == 0;
        let mut tr = Dom::create_tr();
        for cell in row.cells.as_ref() {
            let cell_node = if header {
                Dom::create_th()
            } else {
                Dom::create_td()
            };
            let mut cell_node = cell_node.with_css(&format!(
                "border: 1px solid system:separator; padding: 4px 8px; white-space: pre-wrap; \
                 min-width: 40px;{}",
                if header { " font-weight: bold;" } else { "" }
            ));
            if !cell.as_str().is_empty() {
                cell_node.add_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    cell.clone(),
                ));
            }
            tr.add_child(cell_node);
        }
        node.add_child(tr);
    }
    node
}

/// The built-in toolbar's buttons for the groups `editor.toolbar` shows; a
/// format or a kind that holds at the caret is drawn pressed.
fn toolbar_buttons(editor: &RichTextEditor, data: &RefAny) -> Vec<Dom> {
    use crate::widgets::button::{Button, ButtonOnClickCallbackType, ButtonType};
    let state = &editor.state;
    let formats = state.current_formats();
    let kind = state
        .doc
        .block(state.caret_block)
        .map(|b| b.kind.clone())
        .unwrap_or_default();
    let quoted = state
        .doc
        .block(state.caret_block)
        .is_some_and(|b| b.quote_depth > 0);
    let align = state
        .doc
        .block(state.caret_block)
        .map_or(RichAlign::Left, |b| b.align);
    let bar = editor.toolbar;
    let mut items: Vec<(&'static str, &'static str, RichTextCommand, bool)> = Vec::new();
    if bar.formats {
        for (icon, label, format) in [
            ("format_bold", "Bold", RichFormat::Bold),
            ("format_italic", "Italic", RichFormat::Italic),
            ("format_underlined", "Underline", RichFormat::Underline),
            ("format_strikethrough", "Strikethrough", RichFormat::Strike),
            ("code", "Code", RichFormat::Code),
        ] {
            items.push((
                icon,
                label,
                RichTextCommand::ToggleFormat(format),
                formats.has(format),
            ));
        }
    }
    if bar.headings {
        for (label, level) in [("H1", 1u8), ("H2", 2), ("H3", 3)] {
            let heading = RichBlockKind::Heading(level);
            items.push((
                "",
                label,
                RichTextCommand::ToggleKind(heading.clone()),
                kind == heading,
            ));
        }
    }
    if bar.lists {
        items.push((
            "format_list_bulleted",
            "Bullets",
            RichTextCommand::ToggleKind(RichBlockKind::Bullet(0)),
            matches!(kind, RichBlockKind::Bullet(_)),
        ));
        items.push((
            "format_list_numbered",
            "Numbering",
            RichTextCommand::ToggleKind(RichBlockKind::Numbered(0)),
            matches!(kind, RichBlockKind::Numbered(_)),
        ));
        items.push((
            "checklist",
            "Checklist",
            RichTextCommand::ToggleKind(RichBlockKind::Check(RichCheck::default())),
            matches!(kind, RichBlockKind::Check(_)),
        ));
    }
    if bar.blocks {
        items.push(("format_quote", "Quote", RichTextCommand::ToggleQuote, quoted));
        items.push((
            "data_object",
            "Code block",
            RichTextCommand::ToggleKind(RichBlockKind::Code(AzString::from_const_str(""))),
            kind.is_code(),
        ));
        items.push(("horizontal_rule", "Rule", RichTextCommand::InsertRule, false));
    }
    if bar.indent {
        items.push(("format_indent_decrease", "Outdent", RichTextCommand::Outdent, false));
        items.push(("format_indent_increase", "Indent", RichTextCommand::Indent, false));
    }
    if bar.align {
        for (icon, label, a) in [
            ("format_align_left", "Left", RichAlign::Left),
            ("format_align_center", "Center", RichAlign::Center),
            ("format_align_right", "Right", RichAlign::Right),
            ("format_align_justify", "Justify", RichAlign::Justify),
        ] {
            items.push((icon, label, RichTextCommand::SetAlign(a), align == a));
        }
    }
    if bar.history {
        items.push(("undo", "Undo", RichTextCommand::Undo, false));
        items.push(("redo", "Redo", RichTextCommand::Redo, false));
    }
    items
        .into_iter()
        .map(|(icon, label, command, pressed)| {
            let kind = if pressed {
                ButtonType::Primary
            } else {
                ButtonType::Default
            };
            let mut button = Button::with_type(AzString::from_const_str(label), kind);
            if !icon.is_empty() {
                button = button.with_icon(AzString::from_const_str(icon));
            }
            if let Some(theme) = editor.theme.into_option() {
                button = button.with_theme(theme);
            }
            button
                .with_on_click(
                    RefAny::new(ToolbarRef {
                        editor: data.clone(),
                        command,
                    }),
                    on_toolbar_click as ButtonOnClickCallbackType,
                )
                .dom()
                .with_css("margin: 2px;")
        })
        .collect()
}

// ==== The state: the app's side ====

impl RichTextEditorState {
    /// The state of an editor showing `doc` (an empty history, the host id
    /// [`DEFAULT_HOST_ID`]).
    #[must_use]
    pub fn create(doc: RichTextDoc) -> Self {
        let mut doc = doc;
        doc.normalize();
        Self {
            doc,
            history: RichTextHistory::create(),
            host_id: AzString::from_const_str(DEFAULT_HOST_ID),
            caret_block: 0,
            caret_byte: 0,
            revision: 0,
            typing: OptionRichTypingStyle::None,
        }
    }

    /// The document (a copy).
    #[must_use]
    pub fn get_doc(&self) -> RichTextDoc {
        self.doc.clone()
    }

    /// Shows another document (another note was opened, a draft was
    /// loaded): the history starts over. Call
    /// `CallbackInfo::reset_editor_content` for a live editor (or use
    /// [`Self::replace_doc`]).
    pub fn set_doc(&mut self, doc: RichTextDoc) {
        let mut doc = doc;
        doc.normalize();
        self.doc = doc;
        self.history.clear();
        self.typing = OptionRichTypingStyle::None;
        self.caret_block = 0;
        self.caret_byte = 0;
        self.revision += 1;
    }

    /// Replaces the document of a LIVE editor from a callback as one undoable
    /// step (a signature inserted, a template applied): the engine drops the
    /// old content's editing state. Returns `RefreshDom`.
    pub fn replace_doc(&mut self, mut info: CallbackInfo, doc: RichTextDoc) -> Update {
        let host = self.host_node(&info);
        if let Some(host) = host {
            let _ = self.sync_text(&mut info, host, false);
        }
        let mut doc = doc;
        doc.normalize();
        self.history.record(&self.doc, RichEditGroup::None);
        self.doc = doc;
        self.after_history(&mut info, host);
        Update::RefreshDom
    }

    /// The formats text typed at the caret takes: the typing style when one
    /// was set there, else the run before the caret's (a toolbar's pressed
    /// buttons).
    #[must_use]
    pub fn current_formats(&self) -> RichFormats {
        self.formats_at(self.caret_block, self.caret_byte)
    }

    /// The kind of the caret's block.
    #[must_use]
    pub fn current_kind(&self) -> RichBlockKind {
        self.doc
            .block(self.caret_block)
            .map(|b| b.kind.clone())
            .unwrap_or_default()
    }

    /// Whether the caret's block is of `kind`'s family (a toolbar's or a
    /// ribbon's block button shows pressed).
    #[must_use]
    pub fn is_current_kind(&self, kind: RichBlockKind) -> bool {
        self.current_kind().same_family(&kind)
    }

    /// Whether text typed at the caret takes `format` (a toolbar's or a
    /// ribbon's format button shows pressed).
    #[must_use]
    pub fn is_current_format(&self, format: RichFormat) -> bool {
        self.current_formats().has(format)
    }

    /// Whether the caret's block is quoted.
    #[must_use]
    pub fn is_current_quoted(&self) -> bool {
        self.doc
            .block(self.caret_block)
            .is_some_and(|b| b.quote_depth > 0)
    }

    /// Whether there is a step to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Whether there is an undone step to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// The editing host in the window `info` runs in (looked up by
    /// [`Self::host_id`]), if the editor is there.
    #[must_use]
    pub fn host_node(&self, info: &CallbackInfo) -> Option<DomNodeId> {
        let id = self.host_id.as_str();
        let hit_dom = info.get_hit_node().dom;
        [hit_dom, DomId::ROOT_ID].into_iter().find_map(|dom| {
            let node = info.get_node_id_by_id_attribute(dom, id)?;
            Some(DomNodeId {
                dom,
                node: NodeHierarchyItemId::from_crate_internal(Some(node)),
            })
        })
    }

    /// Puts the keyboard focus into the editor (after a button of the app
    /// took it).
    pub fn focus(&self, mut info: CallbackInfo) {
        if let Some(host) = self.host_node(&info) {
            info.set_focus(FocusTarget::Id(host));
        }
    }

    /// Folds what was typed and not reported yet into the document - call
    /// it before reading [`Self::doc`] in a callback that is not the
    /// editor's (Send, Save). Returns whether the document changed.
    pub fn sync(&mut self, mut info: CallbackInfo) -> bool {
        match self.host_node(&info) {
            Some(host) => self.sync_text(&mut info, host, false).0,
            None => false,
        }
    }

    /// The selection in the editor now, as spans of block text (empty for a
    /// caret) - for an app that acts on it after its own UI takes the focus:
    /// [`Self::set_link_on`].
    #[must_use]
    pub fn get_selection(&self, info: CallbackInfo) -> RichTextSpanVec {
        let spans: Vec<RichTextSpan> = match self.host_node(&info) {
            Some(host) => selection_in(&info, host)
                .into_iter()
                .map(|(block, start, end)| RichTextSpan { block, start, end })
                .collect(),
            None => Vec::new(),
        };
        RichTextSpanVec::from_vec(spans)
    }

    /// Links `spans` (taken earlier with [`Self::get_selection`]) to `url`,
    /// or unlinks them when `url` is empty; with no span the address goes in
    /// at the caret as its own linked text. Returns `RefreshDom` when the
    /// document changed.
    pub fn set_link_on(
        &mut self,
        mut info: CallbackInfo,
        spans: RichTextSpanVec,
        url: AzString,
    ) -> Update {
        if let Some(host) = self.host_node(&info) {
            let _ = self.sync_text(&mut info, host, false);
        }
        let spans: Vec<(usize, usize, usize)> = spans
            .as_ref()
            .iter()
            .map(|s| (s.block, s.start, s.end))
            .collect();
        let (block, byte) = (self.caret_block, self.caret_byte);
        let url = url.as_str().trim();
        let link = if url.is_empty() { None } else { Some(url) };
        let before = self.doc.clone();
        if !self.link_spans(&spans, block, byte, link) {
            return Update::DoNothing;
        }
        self.history.record(&before, RichEditGroup::None);
        self.revision += 1;
        Update::RefreshDom
    }

    /// Runs `command` (a ribbon's or a toolbar's) on the document, after
    /// folding in what was typed; returns `RefreshDom` when the document
    /// changed (the app rebuilds the editor from this state).
    pub fn apply_command(&mut self, mut info: CallbackInfo, command: RichTextCommand) -> Update {
        let host = self.host_node(&info);
        if let Some(host) = host {
            let _ = self.sync_text(&mut info, host, false);
        }
        self.run_command(&mut info, host, &command, true)
    }
}

// ==== The state: the engine's side ====

/// The child-index path from `host` down to `node` (empty: the host).
fn path_in_host(info: &CallbackInfo, host: DomNodeId, node: DomNodeId) -> Option<Vec<u32>> {
    if node == host {
        return Some(Vec::new());
    }
    info.get_node_child_index_path(host, node)
        .into_option()
        .map(|path| path.as_ref().to_vec())
}

/// The block `node` is (or is in).
fn block_of(info: &CallbackInfo, host: DomNodeId, node: DomNodeId) -> Option<usize> {
    path_in_host(info, host, node)?
        .first()
        .map(|b| *b as usize)
}

/// The caret as `(block, byte in the block's text)`, when there is one in
/// the host.
fn caret_in(info: &CallbackInfo, host: DomNodeId) -> Option<(usize, usize)> {
    let position = info.get_document_caret().into_option()?;
    let block = block_of(info, host, position.node)?;
    Some((block, position.text_byte as usize))
}

/// The caret's child-index path below the host (`[block, row, cell]` in a
/// table).
fn caret_path(info: &CallbackInfo, host: DomNodeId) -> Option<(Vec<u32>, usize)> {
    let position = info.get_document_caret().into_option()?;
    let path = path_in_host(info, host, position.node)?;
    Some((path, position.text_byte as usize))
}

/// The selection as `(block, start, end)` spans; empty for a caret.
fn selection_in(info: &CallbackInfo, host: DomNodeId) -> Vec<(usize, usize, usize)> {
    info.get_document_selection()
        .as_ref()
        .iter()
        .filter(|s| s.end_byte > s.start_byte)
        .filter_map(|s| {
            block_of(info, host, s.node).map(|b| (b, s.start_byte as usize, s.end_byte as usize))
        })
        .collect()
}

/// The bytes before run `child` of `block` (a split position given as a
/// child index).
fn bytes_before(block: &RichBlock, child: usize) -> usize {
    block
        .runs
        .as_ref()
        .iter()
        .filter(|r| !r.as_str().is_empty())
        .take(child)
        .map(|r| r.as_str().len())
        .sum()
}

/// The engine's typing-style format for a model format (inline code has
/// none: the editor rebuilds instead).
const fn engine_format(format: RichFormat) -> Option<TextFormat> {
    match format {
        RichFormat::Bold => Some(TextFormat::Bold),
        RichFormat::Italic => Some(TextFormat::Italic),
        RichFormat::Underline => Some(TextFormat::Underline),
        RichFormat::Strike => Some(TextFormat::Strikethrough),
        RichFormat::Code => None,
    }
}

impl RichTextEditorState {
    /// The formats text typed at byte `at` of block `block` takes.
    fn formats_at(&self, block: usize, at: usize) -> RichFormats {
        if let Some(typing) = self.typing.as_ref() {
            if typing.block == block && typing.at == at {
                return typing.formats;
            }
        }
        self.doc.formats_at(block, at)
    }

    /// Remembers where the engine's caret is.
    fn track_caret(&mut self, info: &CallbackInfo, host: DomNodeId) {
        if let Some((block, byte)) = caret_in(info, host) {
            self.caret_block = block;
            self.caret_byte = byte;
        }
    }

    /// The typing style for an edit of `block` from `old` to `new`, moved
    /// past the inserted text; `None` (and dropped) for any other edit.
    fn typing_for(&mut self, block: usize, old: &str, new: &str) -> Option<RichFormats> {
        let typing = self.typing.as_ref().copied()?;
        self.typing = OptionRichTypingStyle::None;
        let (prefix, suffix) = crate::widgets::rich_text::doc::text_diff(old, new);
        let removed = old.len() - prefix - suffix;
        let inserted = new.len() - prefix - suffix;
        if typing.block != block || removed != 0 || inserted == 0 || prefix != typing.at {
            return None;
        }
        self.typing = OptionRichTypingStyle::Some(RichTypingStyle {
            at: prefix + inserted,
            ..typing
        });
        Some(typing.formats)
    }

    /// Folds the engine's unsynced text edits of the host into the document
    /// and acks them. Returns `(changed, rebuild)`: a Markdown shortcut (with
    /// `shortcuts`) or a typing style the engine cannot paint (inline code)
    /// asks for a new DOM.
    pub(crate) fn sync_text(
        &mut self,
        info: &mut CallbackInfo,
        host: DomNodeId,
        shortcuts: bool,
    ) -> (bool, bool) {
        let edits = info.get_unsynced_text_edits();
        let edits = edits.as_ref();
        if edits.is_empty() {
            return (false, false);
        }
        let mut max_revision = 0u64;
        let mut changed = false;
        let mut rebuild = false;
        for edit in edits {
            max_revision = max_revision.max(edit.revision);
            let Some(path) = path_in_host(info, host, edit.node) else {
                continue; // not this editor's text (a text field beside it)
            };
            let Some(&first) = path.first() else {
                continue;
            };
            let block = first as usize;
            let new = edit.text.as_str();
            if let Some(RichBlockKind::Table(table)) = self.doc.block(block).map(|b| &b.kind) {
                // Typing into a cell: `[block, row, cell]`.
                if let (Some(&row), Some(&cell)) = (path.get(1), path.get(2)) {
                    let old = table
                        .rows
                        .as_ref()
                        .get(row as usize)
                        .map(|r| r.cell(cell as usize).to_string());
                    if old.as_deref().is_some_and(|old| old != new) {
                        self.history.record(&self.doc, RichEditGroup::Typing(block));
                        changed |= self
                            .doc
                            .set_table_cell(block, row as usize, cell as usize, new);
                    }
                }
                continue;
            }
            let Some(old) = self.doc.block(block).map(RichBlock::flat) else {
                continue;
            };
            if old == new {
                continue;
            }
            let typing = self.typing_for(block, &old, new);
            self.history.record(&self.doc, RichEditGroup::Typing(block));
            if self.doc.sync_block_text(block, new, typing) {
                changed = true;
                if typing.is_some_and(|t| t.code) {
                    rebuild = true;
                }
                if shortcuts {
                    let kind = self.doc.block(block).map(|b| b.kind.clone());
                    if let Some(shortcut) = kind.and_then(|k| typed_shortcut(&k, &old, new)) {
                        self.doc.apply_shortcut(block, &shortcut);
                        self.history.break_group();
                        rebuild = true;
                    }
                }
            }
            self.caret_block = block;
        }
        if max_revision > 0 {
            info.mark_text_revision_synced(max_revision);
        }
        if changed {
            self.revision += 1;
        }
        (changed, rebuild)
    }

    /// The engine's structural edit (Enter's split, Backspace's / Delete's
    /// merge, a delete, type-over or paste across blocks) applied to the
    /// document and acknowledged - WITHOUT an inverse: the editor's history
    /// is the one history. Returns whether the document changed (the edit,
    /// or typing folded in first).
    fn apply_document_edit(&mut self, info: &mut CallbackInfo, host: DomNodeId) -> bool {
        use crate::managers::changeset::DocumentOperation;

        let Some(changeset) = info.get_document_edit_clone().into_option() else {
            return false;
        };
        let (synced, _) = self.sync_text(info, host, false);
        let caret = caret_in(info, host);
        let before = self.doc.clone();
        let applied = match &changeset.operation {
            DocumentOperation::SplitNode(split) => {
                let target = block_of(info, host, split.node);
                target
                    .and_then(|b| {
                        let at = match caret {
                            Some((cb, byte)) if cb == b => byte,
                            _ => self.doc.block(b).map_or(0, |block| {
                                bytes_before(block, split.at.child_index as usize)
                                    + split.at.text_byte.into_option().unwrap_or(0) as usize
                            }),
                        };
                        self.doc.split_block(b, at)
                    })
                    .is_some()
            }
            DocumentOperation::MergeNodes(merge) => {
                match (
                    block_of(info, host, merge.first),
                    block_of(info, host, merge.second),
                ) {
                    (Some(first), Some(second)) if second == first + 1 => self
                        .doc
                        .merge_into_previous(second, merge.join.text_byte.into_option().is_some())
                        .is_some(),
                    _ => false,
                }
            }
            DocumentOperation::ReplaceChildren(replace)
                if path_in_host(info, host, replace.parent).is_some_and(|p| p.is_empty()) =>
            {
                let (start, end) = (replace.start as usize, replace.end as usize);
                let parts: &[Dom] = replace.content.children.as_ref();
                if parts.len() <= 1 {
                    // A delete or a type-over across blocks: one block whose
                    // text is the head of the first and the tail of the last;
                    // the model keeps both ends' formats.
                    let mut joined = String::new();
                    for part in parts {
                        rich_html::dom_text(part, &mut joined);
                    }
                    self.doc.replace_blocks(start, end, &joined)
                } else {
                    // A paste of several blocks.
                    let pasted = rich_html::blocks_from_doms(parts);
                    !pasted.is_empty() && self.doc.replace_with(start, end, pasted)
                }
            }
            _ => false,
        };
        if applied {
            self.history.record(&before, RichEditGroup::None);
            info.mark_document_edit_applied(changeset.id);
            self.typing = OptionRichTypingStyle::None;
            self.revision += 1;
        }
        applied || synced
    }

    /// The blocks a block command acts on: those of the selection, else the
    /// caret's.
    fn command_blocks(&self, spans: &[(usize, usize, usize)], caret: usize) -> Vec<usize> {
        let mut blocks: Vec<usize> = spans.iter().map(|(b, _, _)| *b).collect();
        blocks.dedup();
        if blocks.is_empty() {
            blocks.push(caret);
        }
        blocks
    }

    /// The caret's block and byte (the engine's, else the last seen).
    fn caret_target(&mut self, info: &CallbackInfo, host: Option<DomNodeId>) -> (usize, usize) {
        if let Some(host) = host {
            self.track_caret(info, host);
        }
        let last = self.doc.block_count().saturating_sub(1);
        if self.caret_block > last {
            self.caret_block = last;
            self.caret_byte = 0;
        }
        (self.caret_block, self.caret_byte)
    }

    /// After an undo, a redo or a replaced document: the typing style goes,
    /// the caret stays in range, and the engine drops the editing state of
    /// the old content (its overlay, its per-host undo, its structural
    /// history) - no second history survives.
    fn after_history(&mut self, info: &mut CallbackInfo, host: Option<DomNodeId>) {
        self.typing = OptionRichTypingStyle::None;
        self.revision += 1;
        let last = self.doc.block_count().saturating_sub(1);
        if self.caret_block > last {
            self.caret_block = last;
        }
        self.caret_byte = 0;
        if let Some(host) = host {
            info.reset_editor_content(host, true);
        }
    }

    /// Links `spans` to `url` (`None`: unlinks them). With no span: a link
    /// is inserted at the caret (`block`, `byte`) as its own text, an unlink
    /// takes the link under the caret. Returns whether the document changed.
    fn link_spans(
        &mut self,
        spans: &[(usize, usize, usize)],
        block: usize,
        byte: usize,
        url: Option<&str>,
    ) -> bool {
        if spans.is_empty() {
            return match url {
                Some(url) => {
                    let run = RichRun::plain(url).with_link(AzString::from(url));
                    self.doc.insert_run(block, byte, run)
                }
                None => match self.doc.link_range_at(block, byte) {
                    Some((s, e)) => self.doc.set_link(block, s, e, None),
                    None => false,
                },
            };
        }
        let mut changed = false;
        for (b, s, e) in spans {
            changed |= self.doc.set_link(*b, *s, *e, url);
        }
        changed
    }

    /// Runs `command`. `paint`: at a caret, a format also asks the engine to
    /// paint the typing style (a button; a key's default action does it
    /// itself). Returns `RefreshDom` when the document changed.
    #[allow(clippy::too_many_lines)]
    fn run_command(
        &mut self,
        info: &mut CallbackInfo,
        host: Option<DomNodeId>,
        command: &RichTextCommand,
        paint: bool,
    ) -> Update {
        let (block, byte) = self.caret_target(info, host);
        let spans = host.map(|h| selection_in(info, h)).unwrap_or_default();
        let before = self.doc.clone();
        let depth = self.doc.block(block).map_or(0, |b| b.quote_depth);
        let changed = match command {
            RichTextCommand::Undo => {
                return match self.history.undo(&self.doc) {
                    Some(previous) => {
                        self.doc = previous;
                        self.after_history(info, host);
                        Update::RefreshDom
                    }
                    None => Update::DoNothing,
                };
            }
            RichTextCommand::Redo => {
                return match self.history.redo(&self.doc) {
                    Some(next) => {
                        self.doc = next;
                        self.after_history(info, host);
                        Update::RefreshDom
                    }
                    None => Update::DoNothing,
                };
            }
            RichTextCommand::ToggleFormat(format) => {
                if spans.is_empty() {
                    // At a caret: the typing style of what is typed next.
                    let mut formats = self.formats_at(block, byte);
                    formats.set(*format, !formats.has(*format));
                    self.typing = OptionRichTypingStyle::Some(RichTypingStyle {
                        block,
                        at: byte,
                        formats,
                    });
                    if paint {
                        if let (Some(host), Some(f)) = (host, engine_format(*format)) {
                            info.toggle_text_format(host, f);
                        }
                    }
                    false
                } else {
                    // Set everywhere when one span lacks it, else clear
                    // everywhere (Bold twice is plain again).
                    let on = !spans
                        .iter()
                        .all(|(b, s, e)| self.doc.has_format(*b, *s, *e, *format));
                    let mut changed = false;
                    for (b, s, e) in &spans {
                        changed |= self.doc.set_format(*b, *s, *e, *format, on);
                    }
                    changed
                }
            }
            RichTextCommand::ToggleKind(kind) => {
                let blocks = self.command_blocks(&spans, block);
                let undo = blocks
                    .first()
                    .and_then(|b| self.doc.block(*b))
                    .is_some_and(|b| b.kind.same_family(kind));
                let mut changed = false;
                for b in blocks {
                    let Some(current) = self.doc.block(b).map(|x| x.kind.clone()) else {
                        continue;
                    };
                    let next = if undo {
                        RichBlockKind::Paragraph
                    } else if current.is_list() && kind.is_list() {
                        kind.with_indent(current.indent())
                    } else {
                        kind.clone()
                    };
                    changed |= self.doc.set_kind(b, next);
                }
                if changed {
                    self.doc.normalize();
                }
                changed
            }
            RichTextCommand::ToggleQuote => {
                let blocks = self.command_blocks(&spans, block);
                let quoted = blocks
                    .first()
                    .and_then(|b| self.doc.block(*b))
                    .is_some_and(|b| b.quote_depth > 0);
                let mut changed = false;
                for b in blocks {
                    changed |= self.doc.set_quote_depth(b, if quoted { 0 } else { 1 });
                }
                changed
            }
            RichTextCommand::Indent | RichTextCommand::Outdent => {
                let delta = if *command == RichTextCommand::Indent { 1 } else { -1 };
                let mut changed = false;
                for b in self.command_blocks(&spans, block) {
                    changed |= self.doc.indent(b, delta);
                }
                changed
            }
            RichTextCommand::SetAlign(align) => {
                let mut changed = false;
                for b in self.command_blocks(&spans, block) {
                    changed |= self.doc.set_align(b, *align);
                }
                changed
            }
            RichTextCommand::InsertRule
            | RichTextCommand::InsertPageBreak
            | RichTextCommand::InsertImage(_)
            | RichTextCommand::InsertTable(_) => {
                let kind = match command {
                    RichTextCommand::InsertRule => RichBlockKind::Rule,
                    RichTextCommand::InsertPageBreak => RichBlockKind::PageBreak,
                    RichTextCommand::InsertImage(image) => RichBlockKind::Image(image.clone()),
                    RichTextCommand::InsertTable(size) => RichBlockKind::Table(RichTable::empty(
                        size.rows.max(1),
                        size.columns.max(1),
                    )),
                    _ => RichBlockKind::Rule,
                };
                let at = self.doc.insert_after(
                    block,
                    RichBlock::new(kind, Vec::new()).with_quote_depth(depth),
                );
                let after = self
                    .doc
                    .insert_after(at, RichBlock::paragraph("").with_quote_depth(depth));
                self.caret_block = after;
                self.caret_byte = 0;
                true
            }
            RichTextCommand::SetLink(url) => {
                let url = url.as_str().trim();
                !url.is_empty() && self.link_spans(&spans, block, byte, Some(url))
            }
            RichTextCommand::RemoveLink => self.link_spans(&spans, block, byte, None),
            RichTextCommand::ToggleCheck => self.doc.toggle_check(block),
        };
        if !changed {
            return Update::DoNothing;
        }
        self.history.record(&before, RichEditGroup::None);
        self.revision += 1;
        if !matches!(command, RichTextCommand::ToggleFormat(_)) {
            self.typing = OptionRichTypingStyle::None;
        }
        Update::RefreshDom
    }
}

// ==== The callbacks (one RefAny per editor) ====

/// What every callback of one editor shares: the editor's live copy of the
/// state (the app's copy follows through `on_change`).
struct EditorData {
    state: RichTextEditorState,
    on_change: OptionRichTextEditorOnChange,
    on_link: OptionRichTextEditorOnLink,
    markdown_shortcuts: bool,
}

impl EditorData {
    /// Hands the new state to the app.
    fn notify(&self, info: CallbackInfo) -> Update {
        match self.on_change.as_ref() {
            Some(on_change) => {
                on_change
                    .callback
                    .invoke(on_change.refany.clone(), info, self.state.clone())
            }
            None => Update::DoNothing,
        }
    }
}

/// `TextChanged` on the host: the typing goes into the document (no
/// rebuild, unless a Markdown shortcut changed a block).
extern "C" fn on_text_changed(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<EditorData>() else {
        return Update::DoNothing;
    };
    let editor = &mut *guard;
    let Some(host) = editor.state.host_node(&info) else {
        return Update::DoNothing;
    };
    let shortcuts = editor.markdown_shortcuts;
    let (changed, rebuild) = editor.state.sync_text(&mut info, host, shortcuts);
    editor.state.track_caret(&info, host);
    let mut update = if rebuild {
        Update::RefreshDom
    } else {
        Update::DoNothing
    };
    if changed {
        update.max_self(editor.notify(info));
    }
    update
}

/// `DocumentEdit` on the host: the structural edit goes into the document,
/// acknowledged so the engine places the caret at its resume point; a new
/// DOM either way (the applied edit, or - an edit the model cannot take -
/// the engine's preview dropped).
extern "C" fn on_document_edit(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<EditorData>() else {
        return Update::DoNothing;
    };
    let editor = &mut *guard;
    let Some(host) = editor.state.host_node(&info) else {
        return Update::DoNothing;
    };
    if editor.state.apply_document_edit(&mut info, host) {
        let _ = editor.notify(info);
    }
    Update::RefreshDom
}

/// A click on a check item's box: tick or untick it.
extern "C" fn on_check_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut editor_data, block)) = data
        .downcast_ref::<CheckRef>()
        .map(|r| (r.editor.clone(), r.block))
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = editor_data.downcast_mut::<EditorData>() else {
        return Update::DoNothing;
    };
    let editor = &mut *guard;
    let host = editor.state.host_node(&info);
    if let Some(host) = host {
        // Typing not folded in yet goes in first.
        let _ = editor.state.sync_text(&mut info, host, false);
    }
    let before = editor.state.doc.clone();
    if !editor.state.doc.toggle_check(block) {
        return Update::DoNothing;
    }
    editor.state.history.record(&before, RichEditGroup::None);
    editor.state.revision += 1;
    let mut update = Update::RefreshDom;
    update.max_self(editor.notify(info));
    update
}

/// Ctrl / Cmd + click on a link in the text opens it: the app's `on_link`,
/// else the system's handler.
extern "C" fn on_link_click(mut data: RefAny, info: CallbackInfo) -> Update {
    if !info.get_current_keyboard_state().primary_down() {
        return Update::DoNothing;
    }
    let Some((mut editor_data, url)) = data
        .downcast_ref::<LinkRef>()
        .map(|l| (l.editor.clone(), l.url.clone()))
    else {
        return Update::DoNothing;
    };
    let on_link = editor_data
        .downcast_ref::<EditorData>()
        .and_then(|e| e.on_link.as_ref().cloned());
    match on_link {
        Some(on_link) => {
            on_link
                .callback
                .invoke(on_link.refany.clone(), info, AzString::from(url))
        }
        None => {
            if let Ok(parsed) = azul_core::url::Url::parse(&url) {
                let _ = parsed.open();
            }
            Update::DoNothing
        }
    }
}

/// A button of the built-in toolbar: its command, then the focus back into
/// the editor.
extern "C" fn on_toolbar_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut editor_data, command)) = data
        .downcast_ref::<ToolbarRef>()
        .map(|r| (r.editor.clone(), r.command.clone()))
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = editor_data.downcast_mut::<EditorData>() else {
        return Update::DoNothing;
    };
    let editor = &mut *guard;
    let host = editor.state.host_node(&info);
    let mut synced = false;
    if let Some(host) = host {
        synced = editor.state.sync_text(&mut info, host, false).0;
    }
    let typing_before = editor.state.typing.clone();
    let mut update = editor
        .state
        .run_command(&mut info, host, &command, true);
    if synced || update == Update::RefreshDom || editor.state.typing != typing_before {
        update.max_self(editor.notify(info));
    }
    if let Some(host) = host {
        info.set_focus(FocusTarget::Id(host));
    }
    update
}

/// The keys the editor handles itself (the engine's defaults do the rest):
/// the format and block shortcuts, Enter in a code block, Enter on an empty
/// list item or quoted line, Backspace at the start of a list item,
/// heading, code block or quoted line, Tab / Shift+Tab in a list, and the
/// keys that would split or merge table cells.
#[allow(clippy::too_many_lines)]
extern "C" fn on_key_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let keyboard = info.get_current_keyboard_state();
    let Some(key) = keyboard.current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let primary = keyboard.primary_down();
    let shift = keyboard.shift_down();
    let alt = keyboard.alt_down();
    let Some(mut guard) = data.downcast_mut::<EditorData>() else {
        return Update::DoNothing;
    };
    let editor = &mut *guard;
    let Some(host) = editor.state.host_node(&info) else {
        return Update::DoNothing;
    };
    editor.state.track_caret(&info, host);
    let collapsed = selection_in(&info, host).is_empty();

    if primary && !alt {
        // `(command, paint)`: paint = no key default of the engine paints
        // the typing style at a caret, the editor asks for it.
        let command = match (key, shift) {
            (VirtualKeyCode::B, false) => Some((RichTextCommand::ToggleFormat(RichFormat::Bold), false)),
            (VirtualKeyCode::I, false) => {
                Some((RichTextCommand::ToggleFormat(RichFormat::Italic), false))
            }
            (VirtualKeyCode::U, false) => {
                Some((RichTextCommand::ToggleFormat(RichFormat::Underline), false))
            }
            (VirtualKeyCode::X, true) => {
                Some((RichTextCommand::ToggleFormat(RichFormat::Strike), true))
            }
            (VirtualKeyCode::E, false) => {
                Some((RichTextCommand::ToggleFormat(RichFormat::Code), true))
            }
            (VirtualKeyCode::Key0, false) => {
                Some((RichTextCommand::ToggleKind(RichBlockKind::Paragraph), true))
            }
            (VirtualKeyCode::Key1, false) => {
                Some((RichTextCommand::ToggleKind(RichBlockKind::Heading(1)), true))
            }
            (VirtualKeyCode::Key2, false) => {
                Some((RichTextCommand::ToggleKind(RichBlockKind::Heading(2)), true))
            }
            (VirtualKeyCode::Key3, false) => {
                Some((RichTextCommand::ToggleKind(RichBlockKind::Heading(3)), true))
            }
            (VirtualKeyCode::Key7, true) => {
                Some((RichTextCommand::ToggleKind(RichBlockKind::Numbered(0)), true))
            }
            (VirtualKeyCode::Key8, true) => {
                Some((RichTextCommand::ToggleKind(RichBlockKind::Bullet(0)), true))
            }
            (VirtualKeyCode::Key9, true) => Some((
                RichTextCommand::ToggleKind(RichBlockKind::Check(RichCheck::default())),
                true,
            )),
            (VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter, false) => {
                Some((RichTextCommand::ToggleCheck, true))
            }
            _ => None,
        };
        let Some((command, paint)) = command else {
            return Update::DoNothing;
        };
        let synced = editor.state.sync_text(&mut info, host, false).0;
        // B / I / U at a caret: the engine's default action paints the
        // typing style, the model records it. Everything else - a format
        // over a selection in particular - is the editor's, and the
        // engine's default is cancelled.
        let is_format = matches!(command, RichTextCommand::ToggleFormat(_));
        if !is_format || !collapsed || paint {
            info.prevent_default();
        }
        let typing_before = editor.state.typing.clone();
        let mut update = editor.state.run_command(&mut info, Some(host), &command, paint);
        if synced || update == Update::RefreshDom || editor.state.typing != typing_before {
            update.max_self(editor.notify(info));
        }
        return update;
    }

    let Some((path, byte)) = caret_path(&info, host) else {
        return Update::DoNothing;
    };
    let Some(&first) = path.first() else {
        return Update::DoNothing;
    };
    let block = first as usize;
    let Some(current) = editor.state.doc.block(block) else {
        return Update::DoNothing;
    };
    let kind = current.kind.clone();
    let empty = current.is_empty();
    let depth = current.quote_depth;

    if let RichBlockKind::Table(table) = &kind {
        // A cell never splits or merges: Enter, and Backspace / Delete at
        // its edges, stop at the cell.
        let cell_len = match (path.get(1), path.get(2)) {
            (Some(&row), Some(&cell)) => table
                .rows
                .as_ref()
                .get(row as usize)
                .map_or(0, |r| r.cell(cell as usize).len()),
            _ => 0,
        };
        let stop = match key {
            VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter => true,
            VirtualKeyCode::Back => collapsed && byte == 0,
            VirtualKeyCode::Delete => collapsed && byte >= cell_len,
            _ => false,
        };
        if stop {
            info.prevent_default();
        }
        return Update::DoNothing;
    }

    let action = match key {
        VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter if !shift && collapsed => {
            if kind.is_code() {
                // A code block takes line breaks; Enter on its empty last
                // line (the text ends in "\n") leaves it.
                info.prevent_default();
                let _ = editor.state.sync_text(&mut info, host, false);
                let text = editor
                    .state
                    .doc
                    .block(block)
                    .map(RichBlock::flat)
                    .unwrap_or_default();
                if byte >= text.len() && text.ends_with('\n') {
                    let trimmed = text.trim_end_matches('\n').to_string();
                    let before = editor.state.doc.clone();
                    editor.state.doc.sync_block_text(block, &trimmed, None);
                    let at = editor
                        .state
                        .doc
                        .insert_after(block, RichBlock::paragraph("").with_quote_depth(depth));
                    editor.state.history.record(&before, RichEditGroup::None);
                    editor.state.revision += 1;
                    editor.state.caret_block = at;
                    editor.state.caret_byte = 0;
                    let mut update = Update::RefreshDom;
                    update.max_self(editor.notify(info));
                    return update;
                }
                if let Some(node) = info
                    .get_document_caret()
                    .into_option()
                    .and_then(|p| p.node.node.into_crate_internal())
                {
                    info.insert_text(host.dom, node, AzString::from_const_str("\n"));
                }
                return Update::DoNothing;
            }
            if empty && kind.is_list() {
                // Enter on an empty item leaves the list (a level at a time).
                KeyAction::Indent(-1)
            } else if empty && depth > 0 {
                // Enter on an empty quoted line leaves the quote (a level at
                // a time).
                KeyAction::Unquote
            } else {
                KeyAction::None
            }
        }
        VirtualKeyCode::Back if collapsed && byte == 0 => {
            if kind.is_list() {
                // Backspace at the start of a list item outdents it ...
                KeyAction::Indent(-1)
            } else if kind != RichBlockKind::Paragraph && kind.has_text() {
                // ... at the start of a heading or a code block turns it
                // back into a paragraph instead of merging it into the block
                // above ...
                KeyAction::ToParagraph
            } else if depth > 0 {
                // ... and at the start of a quoted line unquotes it a level.
                KeyAction::Unquote
            } else {
                KeyAction::None
            }
        }
        VirtualKeyCode::Tab if kind.is_list() => KeyAction::Indent(if shift { -1 } else { 1 }),
        _ => KeyAction::None,
    };
    if action == KeyAction::None {
        return Update::DoNothing;
    }
    info.prevent_default();
    let synced = editor.state.sync_text(&mut info, host, false).0;
    let before = editor.state.doc.clone();
    let changed = match action {
        KeyAction::Indent(delta) => editor.state.doc.indent(block, delta),
        KeyAction::Unquote => editor
            .state
            .doc
            .set_quote_depth(block, depth.saturating_sub(1)),
        KeyAction::ToParagraph => editor.state.doc.set_kind(block, RichBlockKind::Paragraph),
        KeyAction::None => false,
    };
    if changed {
        editor.state.history.record(&before, RichEditGroup::None);
        editor.state.revision += 1;
        editor.state.typing = OptionRichTypingStyle::None;
    }
    if !changed && !synced {
        return Update::DoNothing;
    }
    let mut update = if changed {
        Update::RefreshDom
    } else {
        Update::DoNothing
    };
    update.max_self(editor.notify(info));
    update
}

/// What a key the editor handles does to the caret's block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyAction {
    None,
    /// A list item one level deeper (`1`) or up (`-1`; out of the list at
    /// level 0).
    Indent(i8),
    /// One quote level less.
    Unquote,
    /// Back to a paragraph.
    ToParagraph,
}

/// The editor the lints and the showcase build: a toolbar and a block of
/// every kind.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// A note with a block of every kind.
    pub(crate) fn sample_doc() -> RichTextDoc {
        let bold = RichRun::plain("bold").with_format(RichFormat::Bold);
        let link = RichRun::plain("the plan").with_link(AzString::from("https://example.org"));
        RichTextDoc::from_blocks(vec![
            RichBlock::text(RichBlockKind::Heading(1), "Offsite agenda"),
            RichBlock::new(
                RichBlockKind::Paragraph,
                vec![RichRun::plain("Bring "), bold, RichRun::plain(" and "), link],
            ),
            RichBlock::text(RichBlockKind::Bullet(0), "Agree on priorities"),
            RichBlock::text(RichBlockKind::Bullet(1), "before the holidays"),
            RichBlock::text(RichBlockKind::Numbered(0), "Book the room"),
            RichBlock::text(
                RichBlockKind::Check(RichCheck {
                    indent: 0,
                    checked: true,
                }),
                "Send the invite",
            ),
            RichBlock::paragraph("Quoted words").with_quote_depth(1),
            RichBlock::text(RichBlockKind::Code(AzString::from("rust")), "let x = 1;"),
            RichBlock::new(RichBlockKind::Rule, vec![]),
            RichBlock::new(RichBlockKind::Table(RichTable::empty(2, 2)), vec![]),
        ])
    }

    /// The sample note in an editor with every toolbar group.
    pub(crate) fn sample() -> RichTextEditor {
        RichTextEditor::create(RichTextEditorState::create(sample_doc()))
            .with_toolbar(RichTextToolbar::full())
            .with_accessibility_name(AzString::from("Note text"))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId, NodeType},
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::widgets::{
        roving::test_support as rv,
        themes::{theme_blocks::checks, theme_checks, UiTheme},
    };

    type Log = Arc<Mutex<Vec<RichTextEditorState>>>;

    extern "C" fn record(mut data: RefAny, _: CallbackInfo, state: RichTextEditorState) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(state);
        }
        Update::DoNothing
    }

    fn logged(editor: RichTextEditor) -> (RichTextEditor, Log) {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let editor = editor.with_on_change(
            RefAny::new(log.clone()),
            record as RichTextEditorOnChangeCallbackType,
        );
        (editor, log)
    }

    /// The host of a built editor.
    fn host_of(dom: &Dom) -> Option<&Dom> {
        if dom.root.has_id(DEFAULT_HOST_ID) {
            return Some(dom);
        }
        dom.children.as_ref().iter().find_map(host_of)
    }

    fn node(index: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
        }
    }

    /// The nodes that take a click, in document order.
    fn clickables(styled: &StyledDom) -> Vec<usize> {
        styled
            .node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                n.get_callbacks()
                    .as_ref()
                    .iter()
                    .any(|cb| cb.event == EventFilter::Hover(HoverEventFilter::Click))
            })
            .map(|(i, _)| i)
            .collect()
    }

    #[test]
    fn the_host_holds_one_element_per_block_and_one_node_per_run() {
        let dom = fixtures::sample().with_theme(UiTheme::Flat).dom();
        let host = host_of(&dom).expect("the host carries its id");
        assert!(host.root.is_contenteditable(), "the host is the editing host");
        let blocks = host.children.as_ref();
        assert_eq!(blocks.len(), fixtures::sample_doc().block_count());
        let paragraph = blocks[1].children.as_ref();
        assert_eq!(paragraph.len(), 4, "one child per run: {paragraph:?}");
        assert!(matches!(paragraph[0].root.get_node_type(), NodeType::Text(_)));
        assert!(paragraph[1].root.has_class(rich_html::RUN_BOLD_CLASS));
        assert!(
            matches!(paragraph[3].root.get_node_type(), NodeType::A),
            "a link is an <a>"
        );
        // The check item: its runs, then the box, an island after them.
        let item = blocks[5].children.as_ref();
        assert_eq!(item.len(), 2);
        assert!(item[1].root.has_class(rich_html::ISLAND_CLASS));
        assert!(blocks[5].root.has_class(rich_html::CHECKED_CLASS));
        assert!(blocks[6].root.has_class(&rich_html::quote_class(1)));
        assert!(blocks[0].root.has_id("az-rich-text-0"), "a block is named by its index");
    }

    #[test]
    fn what_the_editor_renders_reads_back_as_the_same_blocks() {
        // The engine's structural edits clone the editor's own elements:
        // reading them back must give the blocks they came from.
        let dom = fixtures::sample().with_theme(UiTheme::Flat).dom();
        let host = host_of(&dom).expect("the host");
        let blocks = rich_html::blocks_from_doms(host.children.as_ref());
        let doc = fixtures::sample_doc();
        let kinds: Vec<Option<RichBlockKind>> = blocks.iter().map(|b| b.kind.clone()).collect();
        let want: Vec<Option<RichBlockKind>> = doc
            .blocks()
            .iter()
            .map(|b| match &b.kind {
                // A code block reads back without its language.
                RichBlockKind::Code(_) => Some(RichBlockKind::Code(AzString::from_const_str(""))),
                other => Some(other.clone()),
            })
            .collect();
        assert_eq!(kinds, want);
        assert_eq!(blocks[1].runs, doc.blocks()[1].runs_vec());
        assert_eq!(blocks[6].quote_depth, 1);
    }

    #[test]
    fn a_read_only_editor_takes_no_input() {
        let dom = fixtures::sample()
            .with_read_only(true)
            .with_theme(UiTheme::Flat)
            .dom();
        let host = host_of(&dom).expect("the host");
        assert!(!host.root.is_contenteditable());
        assert!(host.root.get_callbacks().as_ref().is_empty());
        let styled = StyledDom::create_from_dom(dom);
        assert!(clickables(&styled).is_empty(), "no toolbar, no check box clicks");
    }

    #[test]
    fn the_toolbar_shows_the_groups_asked_for() {
        let count = |toolbar: RichTextToolbar| {
            let dom = RichTextEditor::create(RichTextEditorState::default())
                .with_toolbar(toolbar)
                .with_theme(UiTheme::Flat)
                .dom();
            clickables(&StyledDom::create_from_dom(dom)).len()
        };
        assert_eq!(count(RichTextToolbar::none()), 0);
        assert_eq!(count(RichTextToolbar::minimal()), 5 + 3);
        assert_eq!(count(RichTextToolbar::full()), 5 + 3 + 3 + 3 + 2 + 4 + 2);
    }

    #[test]
    fn a_toolbar_format_at_a_caret_is_the_typing_style_and_a_kind_changes_the_block() {
        let (editor, log) = logged(fixtures::sample().with_theme(UiTheme::Flat));
        let styled = StyledDom::create_from_dom(editor.dom());
        let buttons = clickables(&styled);
        // Bold first; with nothing selected it is the typing style.
        let (update, _) = rv::fire(
            &styled,
            node(buttons[0]),
            EventFilter::Hover(HoverEventFilter::Click),
        )
        .expect("Bold takes the click");
        assert_eq!(update, Update::DoNothing, "the document did not change");
        let typing = log
            .lock()
            .expect("log")
            .last()
            .and_then(|s| s.typing.as_ref().copied())
            .expect("the typing style was reported");
        assert!(typing.formats.bold);
        // Heading 1 (after the five formats): the caret's block - the
        // first - is a heading 1 already, so it turns back to a paragraph.
        let (update, _) = rv::fire(
            &styled,
            node(buttons[5]),
            EventFilter::Hover(HoverEventFilter::Click),
        )
        .expect("H1 takes the click");
        assert_eq!(update, Update::RefreshDom);
        let state = log.lock().expect("log").last().cloned().expect("a state");
        assert_eq!(state.doc.blocks()[0].kind, RichBlockKind::Paragraph);
        assert!(state.history.can_undo(), "the change is one undo step");
    }

    #[test]
    fn a_click_on_a_check_box_ticks_its_item_off() {
        let (editor, log) = logged(fixtures::sample().with_theme(UiTheme::Flat));
        let styled = StyledDom::create_from_dom(editor.dom());
        let island = styled
            .node_data
            .as_ref()
            .iter()
            .position(|n| n.has_class(rich_html::ISLAND_CLASS) && !n.get_callbacks().as_ref().is_empty())
            .expect("the check box");
        let (update, _) = rv::fire(
            &styled,
            node(island),
            EventFilter::Hover(HoverEventFilter::Click),
        )
        .expect("the box takes the click");
        assert_eq!(update, Update::RefreshDom);
        let state = log.lock().expect("log").last().cloned().expect("a state");
        assert_eq!(
            state.doc.blocks()[5].kind,
            RichBlockKind::Check(RichCheck {
                indent: 0,
                checked: false
            })
        );
    }

    #[test]
    fn an_editor_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "rich_text_editor",
            || fixtures::sample().dom(),
            |t: UiTheme| fixtures::sample().with_theme(t).dom(),
        );
        for theme in checks::BOTH {
            let dom = checks::under(theme, || fixtures::sample().dom());
            assert!(theme_checks::has_class(&dom, RICH_TEXT_EDITOR_CLASS));
        }
    }
}
