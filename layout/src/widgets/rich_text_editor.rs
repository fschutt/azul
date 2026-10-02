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
    dom::{AttributeType, Dom, DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId},
    events::{FocusEventFilter, TextFormat},
    refany::RefAny,
    resources::ImageRef,
    styled_dom::NodeHierarchyItemId,
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::CssPropertyWithConditions, impl_option, impl_option_inner, impl_vec,
    impl_vec_clone, impl_vec_debug, impl_vec_mut, AzString,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::rich_text::{
        doc::{
            shortcut_in as _, typed_shortcut, RichAlign, RichBlock, RichBlockKind, RichCheck,
            RichFormat, RichFormats, RichImage, RichRun, RichTable, RichTextDoc,
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
    pub toolbar: RichTextToolbar,
    /// Markdown shortcuts typed at a block's start change the block.
    pub markdown_shortcuts: bool,
    /// A read-only view (a version, a print): no editing, no callbacks.
    pub read_only: bool,
    /// The widget theme the frame and toolbar are PINNED to, or `None` to
    /// follow the app theme.
    pub theme: crate::widgets::themes::OptionUiTheme,
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
        let data = (!self.read_only).then(|| {
            RefAny::new(EditorData {
                state: self.state.clone(),
                on_change: self.on_change.clone(),
                on_link: self.on_link.clone(),
                markdown_shortcuts: self.markdown_shortcuts,
            })
        });
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
        frame.fixup_children_estimated();
        frame
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

// RTE-WIDGET-PART-C: the engine glue and the callbacks follow.
