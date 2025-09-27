#[cfg(not(feature = "std"))]
use alloc::string::ToString;
use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::String, vec::Vec};
use core::{
    fmt,
    hash::{Hash, Hasher},
    iter::FromIterator,
    mem,
    sync::atomic::{AtomicUsize, Ordering},
};

use azul_css::{AzString, Css, CssProperty, FontRef, LayoutDisplay, NodeTypeTag, OptionAzString};

pub use crate::id_tree::{Node, NodeHierarchy, NodeId};
use crate::{
    app_resources::{ImageCallback, ImageMask, ImageRef, ImageRefHash, RendererResources},
    callbacks::{Callback, CallbackType, IFrameCallback, IFrameCallbackType, OptionRefAny, RefAny},
    id_tree::{NodeDataContainer, NodeDataContainerRef, NodeDataContainerRefMut},
    styled_dom::{
        CssPropertyCache, CssPropertyCachePtr, NodeHierarchyItemId, StyleFontFamilyHash, StyledDom,
        StyledNode, StyledNodeState,
    },
    ui_solver::FormattingContext,
    window::{Menu, OptionVirtualKeyCodeCombo},
};

static TAG_ID: AtomicUsize = AtomicUsize::new(1);

/// Unique tag that is used to annotate which rectangles are relevant for hit-testing
#[derive(Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct TagId(pub u64);

impl ::core::fmt::Display for TagId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "TagId({})", self.0)
    }
}

impl ::core::fmt::Debug for TagId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl TagId {
    /// Creates a new, unique hit-testing tag ID
    pub fn unique() -> Self {
        TagId(TAG_ID.fetch_add(1, Ordering::SeqCst) as u64)
    }

    /// Resets the counter (usually done after each frame) so that we can
    /// track hit-testing Tag IDs of subsequent frames
    pub fn reset() {
        TAG_ID.swap(1, Ordering::SeqCst);
    }
}

/// Same as the `TagId`, but only for scrollable nodes
#[derive(Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct ScrollTagId(pub TagId);

impl ::core::fmt::Display for ScrollTagId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ScrollTagId({})", (self.0).0)
    }
}

impl ::core::fmt::Debug for ScrollTagId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl ScrollTagId {
    /// Creates a new, unique scroll tag ID. Note that this should not
    /// be used for identifying nodes, use the `DomNodeHash` instead.
    pub fn unique() -> ScrollTagId {
        ScrollTagId(TagId::unique())
    }
}

/// Calculated hash of a DOM node, used for identifying identical DOM
/// nodes across frames
#[derive(Copy, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct DomNodeHash(pub u64);

impl ::core::fmt::Debug for DomNodeHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DomNodeHash({})", self.0)
    }
}

/// List of core DOM node types built-into by `azul`.
/// List of core DOM node types built into `azul`.
#[derive(Debug, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum NodeType {
    // Root and container elements
    /// Root element of the document
    Body,
    /// Generic block-level container
    Div,
    /// Paragraph
    P,
    /// Headings
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    /// Line break
    Br,
    /// Horizontal rule
    Hr,
    /// Preformatted text
    Pre,
    /// Block quote
    BlockQuote,
    /// Address
    Address,

    // List elements
    /// Unordered list
    Ul,
    /// Ordered list
    Ol,
    /// List item
    Li,
    /// Definition list
    Dl,
    /// Definition term
    Dt,
    /// Definition description
    Dd,

    // Table elements
    /// Table container
    Table,
    /// Table caption
    Caption,
    /// Table header
    THead,
    /// Table body
    TBody,
    /// Table footer
    TFoot,
    /// Table row
    Tr,
    /// Table header cell
    Th,
    /// Table data cell
    Td,
    /// Table column group
    ColGroup,
    /// Table column
    Col,

    // Form elements
    /// Form container
    Form,
    /// Form fieldset
    FieldSet,
    /// Fieldset legend
    Legend,
    /// Label for form controls
    Label,
    /// Input control
    Input,
    /// Button control
    Button,
    /// Select dropdown
    Select,
    /// Option group
    OptGroup,
    /// Select option
    SelectOption,
    /// Multiline text input
    TextArea,

    // Inline elements
    /// Generic inline container
    Span,
    /// Anchor/hyperlink
    A,
    /// Emphasized text
    Em,
    /// Strongly emphasized text
    Strong,
    /// Bold text
    B,
    /// Italic text
    I,
    /// Code
    Code,
    /// Sample output
    Samp,
    /// Keyboard input
    Kbd,
    /// Variable
    Var,
    /// Citation
    Cite,
    /// Abbreviation
    Abbr,
    /// Acronym
    Acronym,
    /// Quotation
    Q,
    /// Subscript
    Sub,
    /// Superscript
    Sup,
    /// Small text
    Small,
    /// Big text
    Big,

    // Pseudo-elements (transformed into real elements)
    /// ::before pseudo-element
    Before,
    /// ::after pseudo-element
    After,
    /// ::marker pseudo-element
    Marker,
    /// ::placeholder pseudo-element
    Placeholder,

    // Special content types
    /// Text content
    Text(AzString),
    /// Image element
    Image(ImageRef),
    /// IFrame (embedded content)
    IFrame(IFrameNode),
}

impl NodeType {
    /// Determines the default display value for a node type according to HTML standards
    pub fn get_default_display(&self) -> LayoutDisplay {
        match self {
            // Block-level elements
            NodeType::Body
            | NodeType::Div
            | NodeType::P
            | NodeType::H1
            | NodeType::H2
            | NodeType::H3
            | NodeType::H4
            | NodeType::H5
            | NodeType::H6
            | NodeType::Pre
            | NodeType::BlockQuote
            | NodeType::Address
            | NodeType::Hr
            | NodeType::Ul
            | NodeType::Ol
            | NodeType::Li
            | NodeType::Dl
            | NodeType::Dt
            | NodeType::Dd
            | NodeType::Form
            | NodeType::FieldSet
            | NodeType::Legend => LayoutDisplay::Block,

            // Table elements
            NodeType::Table => LayoutDisplay::Table,
            NodeType::Caption => LayoutDisplay::TableCaption,
            NodeType::THead | NodeType::TBody | NodeType::TFoot => LayoutDisplay::TableRowGroup,
            NodeType::Tr => LayoutDisplay::TableRow,
            NodeType::Th | NodeType::Td => LayoutDisplay::TableCell,
            NodeType::ColGroup => LayoutDisplay::TableColumnGroup,
            NodeType::Col => LayoutDisplay::TableColumn,

            // Inline elements
            NodeType::Text(_)
            | NodeType::Br
            | NodeType::Image(_)
            | NodeType::Span
            | NodeType::A
            | NodeType::Em
            | NodeType::Strong
            | NodeType::B
            | NodeType::I
            | NodeType::Code
            | NodeType::Samp
            | NodeType::Kbd
            | NodeType::Var
            | NodeType::Cite
            | NodeType::Abbr
            | NodeType::Acronym
            | NodeType::Q
            | NodeType::Sub
            | NodeType::Sup
            | NodeType::Small
            | NodeType::Big
            | NodeType::Label
            | NodeType::Input
            | NodeType::Button
            | NodeType::Select
            | NodeType::OptGroup
            | NodeType::SelectOption
            | NodeType::TextArea => LayoutDisplay::Inline,

            // Special cases
            NodeType::IFrame(_) => LayoutDisplay::Block,

            // Pseudo-elements
            NodeType::Before | NodeType::After => LayoutDisplay::Inline,
            NodeType::Marker => LayoutDisplay::Marker,
            NodeType::Placeholder => LayoutDisplay::Inline,
        }
    }
    /// Returns the formatting context that this node type establishes by default.
    pub fn default_formatting_context(&self) -> FormattingContext {
        use self::NodeType::*;

        match self {
            // Regular block elements
            Body | Div | P | H1 | H2 | H3 | H4 | H5 | H6 | Pre | BlockQuote | Address | Hr | Ul
            | Ol | Li | Dl | Dt | Dd | Form | FieldSet | Legend => FormattingContext::Block {
                establishes_new_context: false,
            },

            // Table elements with specific formatting contexts
            Table => FormattingContext::Table,
            Caption => FormattingContext::TableCaption,
            THead | TBody | TFoot => FormattingContext::TableRowGroup,
            Tr => FormattingContext::TableRow,
            Th | Td => FormattingContext::TableCell,
            ColGroup => FormattingContext::TableColumnGroup,
            Col => FormattingContext::TableColumnGroup,

            // Inline elements
            Span | A | Em | Strong | B | I | Code | Samp | Kbd | Var | Cite | Abbr | Acronym
            | Q | Sub | Sup | Small | Big | Label | Input | Button | Select | OptGroup
            | SelectOption | TextArea | Text(_) | Br => FormattingContext::Inline,

            // Special elements
            Image(_) => FormattingContext::Inline,
            IFrame(_) => FormattingContext::Block {
                establishes_new_context: true,
            },

            // Pseudo-elements
            Before | After | Marker | Placeholder => FormattingContext::Inline,
        }
    }

    fn into_library_owned_nodetype(&self) -> Self {
        use self::NodeType::*;
        match self {
            Body => Body,
            Div => Div,
            P => P,
            H1 => H1,
            H2 => H2,
            H3 => H3,
            H4 => H4,
            H5 => H5,
            H6 => H6,
            Br => Br,
            Hr => Hr,
            Pre => Pre,
            BlockQuote => BlockQuote,
            Address => Address,
            Ul => Ul,
            Ol => Ol,
            Li => Li,
            Dl => Dl,
            Dt => Dt,
            Dd => Dd,
            Table => Table,
            Caption => Caption,
            THead => THead,
            TBody => TBody,
            TFoot => TFoot,
            Tr => Tr,
            Th => Th,
            Td => Td,
            ColGroup => ColGroup,
            Col => Col,
            Form => Form,
            FieldSet => FieldSet,
            Legend => Legend,
            Label => Label,
            Input => Input,
            Button => Button,
            Select => Select,
            OptGroup => OptGroup,
            SelectOption => SelectOption,
            TextArea => TextArea,
            Span => Span,
            A => A,
            Em => Em,
            Strong => Strong,
            B => B,
            I => I,
            Code => Code,
            Samp => Samp,
            Kbd => Kbd,
            Var => Var,
            Cite => Cite,
            Abbr => Abbr,
            Acronym => Acronym,
            Q => Q,
            Sub => Sub,
            Sup => Sup,
            Small => Small,
            Big => Big,
            Before => Before,
            After => After,
            Marker => Marker,
            Placeholder => Placeholder,

            Text(s) => Text(s.clone_self()),
            Image(i) => Image(i.clone()), // note: shallow clone
            IFrame(i) => IFrame(IFrameNode {
                callback: i.callback,
                data: i.data.clone(),
            }),
        }
    }

    pub(crate) fn format(&self) -> Option<String> {
        use self::NodeType::*;
        match self {
            Text(s) => Some(format!("{}", s)),
            Image(id) => Some(format!("image({:?})", id)),
            IFrame(i) => Some(format!("iframe({:?})", i)),
            _ => None,
        }
    }

    /// Returns the NodeTypeTag for CSS selector matching
    pub fn get_path(&self) -> NodeTypeTag {
        match self {
            Self::Body => NodeTypeTag::Body,
            Self::Div => NodeTypeTag::Div,
            Self::P => NodeTypeTag::P,
            Self::H1 => NodeTypeTag::H1,
            Self::H2 => NodeTypeTag::H2,
            Self::H3 => NodeTypeTag::H3,
            Self::H4 => NodeTypeTag::H4,
            Self::H5 => NodeTypeTag::H5,
            Self::H6 => NodeTypeTag::H6,
            Self::Br => NodeTypeTag::Br,
            Self::Hr => NodeTypeTag::Hr,
            Self::Pre => NodeTypeTag::Pre,
            Self::BlockQuote => NodeTypeTag::BlockQuote,
            Self::Address => NodeTypeTag::Address,
            Self::Ul => NodeTypeTag::Ul,
            Self::Ol => NodeTypeTag::Ol,
            Self::Li => NodeTypeTag::Li,
            Self::Dl => NodeTypeTag::Dl,
            Self::Dt => NodeTypeTag::Dt,
            Self::Dd => NodeTypeTag::Dd,
            Self::Table => NodeTypeTag::Table,
            Self::Caption => NodeTypeTag::Caption,
            Self::THead => NodeTypeTag::THead,
            Self::TBody => NodeTypeTag::TBody,
            Self::TFoot => NodeTypeTag::TFoot,
            Self::Tr => NodeTypeTag::Tr,
            Self::Th => NodeTypeTag::Th,
            Self::Td => NodeTypeTag::Td,
            Self::ColGroup => NodeTypeTag::ColGroup,
            Self::Col => NodeTypeTag::Col,
            Self::Form => NodeTypeTag::Form,
            Self::FieldSet => NodeTypeTag::FieldSet,
            Self::Legend => NodeTypeTag::Legend,
            Self::Label => NodeTypeTag::Label,
            Self::Input => NodeTypeTag::Input,
            Self::Button => NodeTypeTag::Button,
            Self::Select => NodeTypeTag::Select,
            Self::OptGroup => NodeTypeTag::OptGroup,
            Self::SelectOption => NodeTypeTag::SelectOption,
            Self::TextArea => NodeTypeTag::TextArea,
            Self::Span => NodeTypeTag::Span,
            Self::A => NodeTypeTag::A,
            Self::Em => NodeTypeTag::Em,
            Self::Strong => NodeTypeTag::Strong,
            Self::B => NodeTypeTag::B,
            Self::I => NodeTypeTag::I,
            Self::Code => NodeTypeTag::Code,
            Self::Samp => NodeTypeTag::Samp,
            Self::Kbd => NodeTypeTag::Kbd,
            Self::Var => NodeTypeTag::Var,
            Self::Cite => NodeTypeTag::Cite,
            Self::Abbr => NodeTypeTag::Abbr,
            Self::Acronym => NodeTypeTag::Acronym,
            Self::Q => NodeTypeTag::Q,
            Self::Sub => NodeTypeTag::Sub,
            Self::Sup => NodeTypeTag::Sup,
            Self::Small => NodeTypeTag::Small,
            Self::Big => NodeTypeTag::Big,
            Self::Text(_) => NodeTypeTag::Text,
            Self::Image(_) => NodeTypeTag::Img,
            Self::IFrame(_) => NodeTypeTag::IFrame,
            Self::Before => NodeTypeTag::Before,
            Self::After => NodeTypeTag::After,
            Self::Marker => NodeTypeTag::Marker,
            Self::Placeholder => NodeTypeTag::Placeholder,
        }
    }
}

/// When to call a callback action - `On::MouseOver`, `On::MouseOut`, etc.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub enum On {
    /// Mouse cursor is hovering over the element
    MouseOver,
    /// Mouse cursor has is over element and is pressed
    /// (not good for "click" events - use `MouseUp` instead)
    MouseDown,
    /// (Specialization of `MouseDown`). Fires only if the left mouse button
    /// has been pressed while cursor was over the element
    LeftMouseDown,
    /// (Specialization of `MouseDown`). Fires only if the middle mouse button
    /// has been pressed while cursor was over the element
    MiddleMouseDown,
    /// (Specialization of `MouseDown`). Fires only if the right mouse button
    /// has been pressed while cursor was over the element
    RightMouseDown,
    /// Mouse button has been released while cursor was over the element
    MouseUp,
    /// (Specialization of `MouseUp`). Fires only if the left mouse button has
    /// been released while cursor was over the element
    LeftMouseUp,
    /// (Specialization of `MouseUp`). Fires only if the middle mouse button has
    /// been released while cursor was over the element
    MiddleMouseUp,
    /// (Specialization of `MouseUp`). Fires only if the right mouse button has
    /// been released while cursor was over the element
    RightMouseUp,
    /// Mouse cursor has entered the element
    MouseEnter,
    /// Mouse cursor has left the element
    MouseLeave,
    /// Mousewheel / touchpad scrolling
    Scroll,
    /// The window received a unicode character (also respects the system locale).
    /// Check `keyboard_state.current_char` to get the current pressed character.
    TextInput,
    /// A **virtual keycode** was pressed. Note: This is only the virtual keycode,
    /// not the actual char. If you want to get the character, use `TextInput` instead.
    /// A virtual key does not have to map to a printable character.
    ///
    /// You can get all currently pressed virtual keycodes in the
    /// `keyboard_state.current_virtual_keycodes` and / or just the last keycode in the
    /// `keyboard_state.latest_virtual_keycode`.
    VirtualKeyDown,
    /// A **virtual keycode** was release. See `VirtualKeyDown` for more info.
    VirtualKeyUp,
    /// A file has been dropped on the element
    HoveredFile,
    /// A file is being hovered on the element
    DroppedFile,
    /// A file was hovered, but has exited the window
    HoveredFileCancelled,
    /// Equivalent to `onfocus`
    FocusReceived,
    /// Equivalent to `onblur`
    FocusLost,
}

/// Sets the target for what events can reach the callbacks specifically.
///
/// Filtering events can happen on several layers, depending on
/// if a DOM node is hovered over or actively focused. For example,
/// for text input, you wouldn't want to use hovering, because that
/// would mean that the user needs to hold the mouse over the text input
/// in order to enter text. To solve this, the DOM needs to fire events
/// for elements that are currently not part of the hit-test.
/// `EventFilter` implements `From<On>` as a shorthand (so that you can opt-in
/// to a more specific event) and use
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum EventFilter {
    /// Calls the attached callback when the mouse is actively over the
    /// given element.
    Hover(HoverEventFilter),
    /// Inverse of `Hover` - calls the attached callback if the mouse is **not**
    /// over the given element. This is particularly useful for popover menus
    /// where you want to close the menu when the user clicks anywhere else but
    /// the menu itself.
    Not(NotEventFilter),
    /// Calls the attached callback when the element is currently focused.
    Focus(FocusEventFilter),
    /// Calls the callback when anything related to the window is happening.
    /// The "hit item" will be the root item of the DOM.
    /// For example, this can be useful for tracking the mouse position
    /// (in relation to the window). In difference to `Desktop`, this only
    /// fires when the window is focused.
    ///
    /// This can also be good for capturing controller input, touch input
    /// (i.e. global gestures that aren't attached to any component, but rather
    /// the "window" itself).
    Window(WindowEventFilter),
    /// API stub: Something happened with the node itself (node resized, created or removed)
    Component(ComponentEventFilter),
    /// Something happened with the application (started, shutdown, device plugged in)
    Application(ApplicationEventFilter),
}

impl EventFilter {
    pub const fn is_focus_callback(&self) -> bool {
        match self {
            EventFilter::Focus(_) => true,
            _ => false,
        }
    }
    pub const fn is_window_callback(&self) -> bool {
        match self {
            EventFilter::Window(_) => true,
            _ => false,
        }
    }
}

/// Creates a function inside an impl <enum type> block that returns a single
/// variant if the enum is that variant.
///
/// ```rust,no_run,ignore
/// # use azul_core::dom::get_single_enum_type;
/// enum A {
///     Abc(AbcType),
/// }
///
/// struct AbcType {}
///
/// impl A {
///     // fn as_abc_type(&self) -> Option<AbcType>
///     get_single_enum_type!(as_abc_type, A::Abc(AbcType));
/// }
/// ```
macro_rules! get_single_enum_type {
    ($fn_name:ident, $enum_name:ident:: $variant:ident($return_type:ty)) => {
        pub fn $fn_name(&self) -> Option<$return_type> {
            use self::$enum_name::*;
            match self {
                $variant(e) => Some(*e),
                _ => None,
            }
        }
    };
}

impl EventFilter {
    get_single_enum_type!(as_hover_event_filter, EventFilter::Hover(HoverEventFilter));
    get_single_enum_type!(as_focus_event_filter, EventFilter::Focus(FocusEventFilter));
    get_single_enum_type!(as_not_event_filter, EventFilter::Not(NotEventFilter));
    get_single_enum_type!(
        as_window_event_filter,
        EventFilter::Window(WindowEventFilter)
    );
}

impl From<On> for EventFilter {
    fn from(input: On) -> EventFilter {
        use self::On::*;
        match input {
            MouseOver => EventFilter::Hover(HoverEventFilter::MouseOver),
            MouseDown => EventFilter::Hover(HoverEventFilter::MouseDown),
            LeftMouseDown => EventFilter::Hover(HoverEventFilter::LeftMouseDown),
            MiddleMouseDown => EventFilter::Hover(HoverEventFilter::MiddleMouseDown),
            RightMouseDown => EventFilter::Hover(HoverEventFilter::RightMouseDown),
            MouseUp => EventFilter::Hover(HoverEventFilter::MouseUp),
            LeftMouseUp => EventFilter::Hover(HoverEventFilter::LeftMouseUp),
            MiddleMouseUp => EventFilter::Hover(HoverEventFilter::MiddleMouseUp),
            RightMouseUp => EventFilter::Hover(HoverEventFilter::RightMouseUp),

            MouseEnter => EventFilter::Hover(HoverEventFilter::MouseEnter),
            MouseLeave => EventFilter::Hover(HoverEventFilter::MouseLeave),
            Scroll => EventFilter::Hover(HoverEventFilter::Scroll),
            TextInput => EventFilter::Focus(FocusEventFilter::TextInput), // focus!
            VirtualKeyDown => EventFilter::Window(WindowEventFilter::VirtualKeyDown), // window!
            VirtualKeyUp => EventFilter::Window(WindowEventFilter::VirtualKeyUp), // window!
            HoveredFile => EventFilter::Hover(HoverEventFilter::HoveredFile),
            DroppedFile => EventFilter::Hover(HoverEventFilter::DroppedFile),
            HoveredFileCancelled => EventFilter::Hover(HoverEventFilter::HoveredFileCancelled),
            FocusReceived => EventFilter::Focus(FocusEventFilter::FocusReceived), // focus!
            FocusLost => EventFilter::Focus(FocusEventFilter::FocusLost),         // focus!
        }
    }
}

/// Event filter that only fires when an element is hovered over
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub enum HoverEventFilter {
    MouseOver,
    MouseDown,
    LeftMouseDown,
    RightMouseDown,
    MiddleMouseDown,
    MouseUp,
    LeftMouseUp,
    RightMouseUp,
    MiddleMouseUp,
    MouseEnter,
    MouseLeave,
    Scroll,
    ScrollStart,
    ScrollEnd,
    TextInput,
    VirtualKeyDown,
    VirtualKeyUp,
    HoveredFile,
    DroppedFile,
    HoveredFileCancelled,
    TouchStart,
    TouchMove,
    TouchEnd,
    TouchCancel,
}

impl HoverEventFilter {
    pub fn to_focus_event_filter(&self) -> Option<FocusEventFilter> {
        match self {
            HoverEventFilter::MouseOver => Some(FocusEventFilter::MouseOver),
            HoverEventFilter::MouseDown => Some(FocusEventFilter::MouseDown),
            HoverEventFilter::LeftMouseDown => Some(FocusEventFilter::LeftMouseDown),
            HoverEventFilter::RightMouseDown => Some(FocusEventFilter::RightMouseDown),
            HoverEventFilter::MiddleMouseDown => Some(FocusEventFilter::MiddleMouseDown),
            HoverEventFilter::MouseUp => Some(FocusEventFilter::MouseUp),
            HoverEventFilter::LeftMouseUp => Some(FocusEventFilter::LeftMouseUp),
            HoverEventFilter::RightMouseUp => Some(FocusEventFilter::RightMouseUp),
            HoverEventFilter::MiddleMouseUp => Some(FocusEventFilter::MiddleMouseUp),
            HoverEventFilter::MouseEnter => Some(FocusEventFilter::MouseEnter),
            HoverEventFilter::MouseLeave => Some(FocusEventFilter::MouseLeave),
            HoverEventFilter::Scroll => Some(FocusEventFilter::Scroll),
            HoverEventFilter::ScrollStart => Some(FocusEventFilter::ScrollStart),
            HoverEventFilter::ScrollEnd => Some(FocusEventFilter::ScrollEnd),
            HoverEventFilter::TextInput => Some(FocusEventFilter::TextInput),
            HoverEventFilter::VirtualKeyDown => Some(FocusEventFilter::VirtualKeyDown),
            HoverEventFilter::VirtualKeyUp => Some(FocusEventFilter::VirtualKeyDown),
            HoverEventFilter::HoveredFile => None,
            HoverEventFilter::DroppedFile => None,
            HoverEventFilter::HoveredFileCancelled => None,
            HoverEventFilter::TouchStart => None,
            HoverEventFilter::TouchMove => None,
            HoverEventFilter::TouchEnd => None,
            HoverEventFilter::TouchCancel => None,
        }
    }
}

/// The inverse of an `onclick` event filter, fires when an item is *not* hovered / focused.
/// This is useful for cleanly implementing things like popover dialogs or dropdown boxes that
/// want to close when the user clicks any where *but* the item itself.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum NotEventFilter {
    Hover(HoverEventFilter),
    Focus(FocusEventFilter),
}

impl NotEventFilter {
    pub fn as_event_filter(&self) -> EventFilter {
        match self {
            NotEventFilter::Hover(e) => EventFilter::Hover(*e),
            NotEventFilter::Focus(e) => EventFilter::Focus(*e),
        }
    }
}

/// Event filter similar to `HoverEventFilter` that only fires when the element is focused
///
/// **Important**: In order for this to fire, the item must have a `tabindex` attribute
/// (to indicate that the item is focus-able).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub enum FocusEventFilter {
    MouseOver,
    MouseDown,
    LeftMouseDown,
    RightMouseDown,
    MiddleMouseDown,
    MouseUp,
    LeftMouseUp,
    RightMouseUp,
    MiddleMouseUp,
    MouseEnter,
    MouseLeave,
    Scroll,
    ScrollStart,
    ScrollEnd,
    TextInput,
    VirtualKeyDown,
    VirtualKeyUp,
    FocusReceived,
    FocusLost,
}

/// Event filter that fires when any action fires on the entire window
/// (regardless of whether any element is hovered or focused over).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub enum WindowEventFilter {
    MouseOver,
    MouseDown,
    LeftMouseDown,
    RightMouseDown,
    MiddleMouseDown,
    MouseUp,
    LeftMouseUp,
    RightMouseUp,
    MiddleMouseUp,
    MouseEnter,
    MouseLeave,
    Scroll,
    ScrollStart,
    ScrollEnd,
    TextInput,
    VirtualKeyDown,
    VirtualKeyUp,
    HoveredFile,
    DroppedFile,
    HoveredFileCancelled,
    Resized,
    Moved,
    TouchStart,
    TouchMove,
    TouchEnd,
    TouchCancel,
    FocusReceived,
    FocusLost,
    CloseRequested,
    ThemeChanged,
    WindowFocusReceived,
    WindowFocusLost,
}

impl WindowEventFilter {
    pub fn to_hover_event_filter(&self) -> Option<HoverEventFilter> {
        match self {
            WindowEventFilter::MouseOver => Some(HoverEventFilter::MouseOver),
            WindowEventFilter::MouseDown => Some(HoverEventFilter::MouseDown),
            WindowEventFilter::LeftMouseDown => Some(HoverEventFilter::LeftMouseDown),
            WindowEventFilter::RightMouseDown => Some(HoverEventFilter::RightMouseDown),
            WindowEventFilter::MiddleMouseDown => Some(HoverEventFilter::MiddleMouseDown),
            WindowEventFilter::MouseUp => Some(HoverEventFilter::MouseUp),
            WindowEventFilter::LeftMouseUp => Some(HoverEventFilter::LeftMouseUp),
            WindowEventFilter::RightMouseUp => Some(HoverEventFilter::RightMouseUp),
            WindowEventFilter::MiddleMouseUp => Some(HoverEventFilter::MiddleMouseUp),
            WindowEventFilter::Scroll => Some(HoverEventFilter::Scroll),
            WindowEventFilter::ScrollStart => Some(HoverEventFilter::ScrollStart),
            WindowEventFilter::ScrollEnd => Some(HoverEventFilter::ScrollEnd),
            WindowEventFilter::TextInput => Some(HoverEventFilter::TextInput),
            WindowEventFilter::VirtualKeyDown => Some(HoverEventFilter::VirtualKeyDown),
            WindowEventFilter::VirtualKeyUp => Some(HoverEventFilter::VirtualKeyDown),
            WindowEventFilter::HoveredFile => Some(HoverEventFilter::HoveredFile),
            WindowEventFilter::DroppedFile => Some(HoverEventFilter::DroppedFile),
            WindowEventFilter::HoveredFileCancelled => Some(HoverEventFilter::HoveredFileCancelled),
            // MouseEnter and MouseLeave on the **window** - does not mean a mouseenter
            // and a mouseleave on the hovered element
            WindowEventFilter::MouseEnter => None,
            WindowEventFilter::MouseLeave => None,
            WindowEventFilter::Resized => None,
            WindowEventFilter::Moved => None,
            WindowEventFilter::TouchStart => Some(HoverEventFilter::TouchStart),
            WindowEventFilter::TouchMove => Some(HoverEventFilter::TouchMove),
            WindowEventFilter::TouchEnd => Some(HoverEventFilter::TouchEnd),
            WindowEventFilter::TouchCancel => Some(HoverEventFilter::TouchCancel),
            WindowEventFilter::FocusReceived => None,
            WindowEventFilter::FocusLost => None,
            WindowEventFilter::CloseRequested => None,
            WindowEventFilter::ThemeChanged => None,
            WindowEventFilter::WindowFocusReceived => None, // specific to window!
            WindowEventFilter::WindowFocusLost => None,     // specific to window!
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum ComponentEventFilter {
    AfterMount,
    BeforeUnmount,
    NodeResized,
    DefaultAction,
    Selected,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum ApplicationEventFilter {
    DeviceConnected,
    DeviceDisconnected,
    // ... TODO: more events
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct IFrameNode {
    pub callback: IFrameCallback,
    pub data: RefAny,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CallbackData {
    pub event: EventFilter,
    pub callback: Callback,
    pub data: RefAny,
}

impl_vec!(CallbackData, CallbackDataVec, CallbackDataVecDestructor);
impl_vec_clone!(CallbackData, CallbackDataVec, CallbackDataVecDestructor);
impl_vec_mut!(CallbackData, CallbackDataVec);
impl_vec_debug!(CallbackData, CallbackDataVec);
impl_vec_partialord!(CallbackData, CallbackDataVec);
impl_vec_ord!(CallbackData, CallbackDataVec);
impl_vec_partialeq!(CallbackData, CallbackDataVec);
impl_vec_eq!(CallbackData, CallbackDataVec);
impl_vec_hash!(CallbackData, CallbackDataVec);

impl CallbackDataVec {
    #[inline]
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, CallbackData> {
        NodeDataContainerRef {
            internal: self.as_ref(),
        }
    }
    #[inline]
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, CallbackData> {
        NodeDataContainerRefMut {
            internal: self.as_mut(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IdOrClass {
    Id(AzString),
    Class(AzString),
}

impl_vec!(IdOrClass, IdOrClassVec, IdOrClassVecDestructor);
impl_vec_debug!(IdOrClass, IdOrClassVec);
impl_vec_partialord!(IdOrClass, IdOrClassVec);
impl_vec_ord!(IdOrClass, IdOrClassVec);
impl_vec_clone!(IdOrClass, IdOrClassVec, IdOrClassVecDestructor);
impl_vec_partialeq!(IdOrClass, IdOrClassVec);
impl_vec_eq!(IdOrClass, IdOrClassVec);
impl_vec_hash!(IdOrClass, IdOrClassVec);

impl IdOrClass {
    pub fn as_id(&self) -> Option<&str> {
        match self {
            IdOrClass::Id(s) => Some(s.as_str()),
            IdOrClass::Class(_) => None,
        }
    }
    pub fn as_class(&self) -> Option<&str> {
        match self {
            IdOrClass::Class(s) => Some(s.as_str()),
            IdOrClass::Id(_) => None,
        }
    }
}

// memory optimization: store all inline-normal / inline-hover / inline-* attributes
// as one Vec instad of 4 separate Vecs
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum NodeDataInlineCssProperty {
    Normal(CssProperty),
    Active(CssProperty),
    Focus(CssProperty),
    Hover(CssProperty),
}

macro_rules! parse_from_str {
    ($s:expr, $prop_type:ident) => {{
        use azul_css::{parser::ErrorLocation, CssDeclaration, CssKeyMap};

        let s = $s.trim();
        let css_key_map = CssKeyMap::get();

        let v = s
            .split(";")
            .filter_map(|kv| {
                let mut kv_iter = kv.split(":");
                let key = kv_iter.next()?;
                let value = kv_iter.next()?;
                let mut declarations = Vec::new();
                let mut warnings = Vec::new();

                azul_css::parser::parse_css_declaration(
                    key,
                    value,
                    (ErrorLocation::default(), ErrorLocation::default()),
                    &css_key_map,
                    &mut warnings,
                    &mut declarations,
                )
                .ok()?;

                let declarations = declarations
                    .iter()
                    .filter_map(|c| match c {
                        CssDeclaration::Static(d) => {
                            Some(NodeDataInlineCssProperty::$prop_type(d.clone()))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();

                if declarations.is_empty() {
                    None
                } else {
                    Some(declarations)
                }
            })
            .collect::<Vec<Vec<NodeDataInlineCssProperty>>>();

        v.into_iter()
            .flat_map(|k| k.into_iter())
            .collect::<Vec<_>>()
            .into()
    }};
}

impl NodeDataInlineCssPropertyVec {
    // given "flex-directin: row", returns
    // vec![NodeDataInlineCssProperty::Normal(FlexDirection::Row)]
    pub fn parse_normal(s: &str) -> Self {
        return parse_from_str!(s, Normal);
    }

    // given "flex-directin: row", returns
    // vec![NodeDataInlineCssProperty::Hover(FlexDirection::Row)]
    pub fn parse_hover(s: &str) -> Self {
        return parse_from_str!(s, Hover);
    }

    // given "flex-directin: row", returns
    // vec![NodeDataInlineCssProperty::Active(FlexDirection::Row)]
    pub fn parse_active(s: &str) -> Self {
        return parse_from_str!(s, Active);
    }

    // given "flex-directin: row", returns
    // vec![NodeDataInlineCssProperty::Focus(FlexDirection::Row)]
    pub fn parse_focus(s: &str) -> Self {
        return parse_from_str!(s, Focus);
    }

    // appends two NodeDataInlineCssPropertyVec, even if both are &'static arrays
    pub fn with_append(&self, mut other: Self) -> Self {
        let mut m = self.clone().into_library_owned_vec();
        m.append(&mut other.into_library_owned_vec());
        m.into()
    }
}

impl fmt::Debug for NodeDataInlineCssProperty {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::NodeDataInlineCssProperty::*;
        match self {
            Normal(p) => write!(f, "Normal({}: {})", p.key(), p.value()),
            Active(p) => write!(f, "Active({}: {})", p.key(), p.value()),
            Focus(p) => write!(f, "Focus({}: {})", p.key(), p.value()),
            Hover(p) => write!(f, "Hover({}: {})", p.key(), p.value()),
        }
    }
}

impl_vec!(
    NodeDataInlineCssProperty,
    NodeDataInlineCssPropertyVec,
    NodeDataInlineCssPropertyVecDestructor
);
impl_vec_debug!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec);
impl_vec_partialord!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec);
impl_vec_ord!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec);
impl_vec_clone!(
    NodeDataInlineCssProperty,
    NodeDataInlineCssPropertyVec,
    NodeDataInlineCssPropertyVecDestructor
);
impl_vec_partialeq!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec);
impl_vec_eq!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec);
impl_vec_hash!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec);

/// Represents one single DOM node (node type, classes, ids and callbacks are stored here)
#[repr(C)]
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeData {
    /// `div`
    pub(crate) node_type: NodeType,
    /// data-* attributes for this node, useful to store UI-related data on the node itself
    pub(crate) dataset: OptionRefAny,
    /// Stores all ids and classes as one vec - size optimization since
    /// most nodes don't have any classes or IDs
    pub(crate) ids_and_classes: IdOrClassVec,
    /// `On::MouseUp` -> `Callback(my_button_click_handler)`
    pub(crate) callbacks: CallbackDataVec,
    /// Stores the inline CSS properties, same as in HTML
    pub(crate) inline_css_props: NodeDataInlineCssPropertyVec,
    /// Tab index (commonly used property)
    pub(crate) tab_index: OptionTabIndex,
    /// Stores "extra", not commonly used data of the node: accessibility, clip-mask, tab-index,
    /// etc.
    ///
    /// SHOULD NOT EXPOSED IN THE API - necessary to retroactively add functionality
    /// to the node without breaking the ABI
    extra: Option<Box<NodeDataExt>>,
}

impl Hash for NodeData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node_type.hash(state);
        self.dataset.hash(state);
        self.ids_and_classes.as_ref().hash(state);

        // NOTE: callbacks are NOT hashed regularly, otherwise
        // they'd cause inconsistencies because of the scroll callback
        for callback in self.callbacks.as_ref().iter() {
            callback.event.hash(state);
            callback.callback.hash(state);
            callback.data.get_type_id().hash(state);
        }

        self.inline_css_props.as_ref().hash(state);
        if let Some(ext) = self.extra.as_ref() {
            if let Some(c) = ext.clip_mask.as_ref() {
                c.hash(state);
            }
            if let Some(c) = ext.accessibility.as_ref() {
                c.hash(state);
            }
            if let Some(c) = ext.menu_bar.as_ref() {
                c.hash(state);
            }
            if let Some(c) = ext.context_menu.as_ref() {
                c.hash(state);
            }
        }
    }
}

/// NOTE: NOT EXPOSED IN THE API! Stores extra,
/// not commonly used information for the NodeData.
#[repr(C)]
#[derive(Debug, Default, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct NodeDataExt {
    /// Optional clip mask for this DOM node
    pub(crate) clip_mask: Option<ImageMask>,
    /// Optional extra accessibility information about this DOM node (MSAA, AT-SPI, UA)
    pub(crate) accessibility: Option<Box<AccessibilityInfo>>,
    /// Menu bar that should be displayed at the top of this nodes rect
    pub(crate) menu_bar: Option<Box<Menu>>,
    /// Context menu that should be opened when the item is left-clicked
    pub(crate) context_menu: Option<Box<Menu>>,
    // ... insert further API extensions here...
}

/// Accessibility information (MSAA wrapper). See `NodeData.set_accessibility_info()`
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct AccessibilityInfo {
    /// Get the "name" of the `IAccessible`, for example the
    /// name of a button, checkbox or menu item. Try to use unique names
    /// for each item in a dialog so that voice dictation software doesn't
    /// have to deal with extra ambiguity
    pub name: OptionAzString,
    /// Get the "value" of the `IAccessible`, for example a number in a slider,
    /// a URL for a link, the text a user entered in a field.
    pub value: OptionAzString,
    /// Get an enumerated value representing what this IAccessible is used for,
    /// for example is it a link, static text, editable text, a checkbox, or a table cell, etc.
    pub role: AccessibilityRole,
    /// Possible on/off states, such as focused, focusable, selected, selectable,
    /// visible, protected (for passwords), checked, etc.
    pub states: AccessibilityStateVec,
    /// Optional keyboard accelerator
    pub accelerator: OptionVirtualKeyCodeCombo,
    /// Optional "default action" description. Only used when there is at least
    /// one `ComponentEventFilter::DefaultAction` callback present on this node
    pub default_action: OptionAzString,
}

/// MSAA Accessibility role constants. For information on what each role does,
/// see the [MSDN Role Constants page](https://docs.microsoft.com/en-us/windows/win32/winauto/object-roles).
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum AccessibilityRole {
    /// Inserted by operating system
    TitleBar,
    MenuBar,
    ScrollBar,
    Grip,
    Sound,
    Cursor,
    Caret,
    Alert,
    /// Inserted by operating system
    Window,
    Client,
    MenuPopup,
    MenuItem,
    Tooltip,
    Application,
    Document,
    Pane,
    Chart,
    Dialog,
    Border,
    Grouping,
    Separator,
    Toolbar,
    StatusBar,
    Table,
    ColumnHeader,
    RowHeader,
    Column,
    Row,
    Cell,
    Link,
    HelpBalloon,
    Character,
    List,
    ListItem,
    Outline,
    OutlineItem,
    Pagetab,
    PropertyPage,
    Indicator,
    Graphic,
    StaticText,
    Text,
    PushButton,
    CheckButton,
    RadioButton,
    ComboBox,
    DropList,
    ProgressBar,
    Dial,
    HotkeyField,
    Slider,
    SpinButton,
    Diagram,
    Animation,
    Equation,
    ButtonDropdown,
    ButtonMenu,
    ButtonDropdownGrid,
    Whitespace,
    PageTabList,
    Clock,
    SplitButton,
    IpAddress,
    Nothing,
}

/// MSAA accessibility state. For information on what each state does, see the
/// [MSDN State Constants](https://docs.microsoft.com/en-us/windows/win32/winauto/object-state-constants\) page.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum AccessibilityState {
    Unavailable,
    Selected,
    Focused,
    Checked,
    Readonly,
    Default,
    Expanded,
    Collapsed,
    Busy,
    Offscreen,
    Focusable,
    Selectable,
    Linked,
    Traversed,
    Multiselectable,
    Protected,
}

impl_vec!(
    AccessibilityState,
    AccessibilityStateVec,
    AccessibilityStateVecDestructor
);
impl_vec_clone!(
    AccessibilityState,
    AccessibilityStateVec,
    AccessibilityStateVecDestructor
);
impl_vec_debug!(AccessibilityState, AccessibilityStateVec);
impl_vec_partialeq!(AccessibilityState, AccessibilityStateVec);
impl_vec_partialord!(AccessibilityState, AccessibilityStateVec);
impl_vec_eq!(AccessibilityState, AccessibilityStateVec);
impl_vec_ord!(AccessibilityState, AccessibilityStateVec);
impl_vec_hash!(AccessibilityState, AccessibilityStateVec);

impl Clone for NodeData {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            node_type: self.node_type.into_library_owned_nodetype(),
            dataset: match &self.dataset {
                OptionRefAny::None => OptionRefAny::None,
                OptionRefAny::Some(s) => OptionRefAny::Some(s.clone()),
            },
            ids_and_classes: self.ids_and_classes.clone(), /* do not clone the IDs and classes if
                                                            * they are &'static */
            inline_css_props: self.inline_css_props.clone(), /* do not clone the inline CSS props
                                                              * if they are &'static */
            callbacks: self.callbacks.clone(),
            tab_index: self.tab_index,
            extra: self.extra.clone(),
        }
    }
}

// Clone, PartialEq, Eq, Hash, PartialOrd, Ord
impl_vec!(NodeData, NodeDataVec, NodeDataVecDestructor);
impl_vec_clone!(NodeData, NodeDataVec, NodeDataVecDestructor);
impl_vec_mut!(NodeData, NodeDataVec);
impl_vec_debug!(NodeData, NodeDataVec);
impl_vec_partialord!(NodeData, NodeDataVec);
impl_vec_ord!(NodeData, NodeDataVec);
impl_vec_partialeq!(NodeData, NodeDataVec);
impl_vec_eq!(NodeData, NodeDataVec);
impl_vec_hash!(NodeData, NodeDataVec);

impl NodeDataVec {
    #[inline]
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, NodeData> {
        NodeDataContainerRef {
            internal: self.as_ref(),
        }
    }
    #[inline]
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, NodeData> {
        NodeDataContainerRefMut {
            internal: self.as_mut(),
        }
    }
}

unsafe impl Send for NodeData {}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C, u8)]
pub enum TabIndex {
    /// Automatic tab index, similar to simply setting `focusable = "true"` or `tabindex = 0`
    /// (both have the effect of making the element focusable).
    ///
    /// Sidenote: See https://www.w3.org/TR/html5/editing.html#sequential-focus-navigation-and-the-tabindex-attribute
    /// for interesting notes on tabindex and accessibility
    Auto,
    /// Set the tab index in relation to its parent element. I.e. if you have a list of elements,
    /// the focusing order is restricted to the current parent.
    ///
    /// Ex. a div might have:
    ///
    /// ```no_run,ignore
    /// div (Auto)
    /// |- element1 (OverrideInParent 0) <- current focus
    /// |- element2 (OverrideInParent 5)
    /// |- element3 (OverrideInParent 2)
    /// |- element4 (Global 5)
    /// ```
    ///
    /// When pressing tab repeatedly, the focusing order will be
    /// "element3, element2, element4, div", since OverrideInParent elements
    /// take precedence among global order.
    OverrideInParent(u32),
    /// Elements can be focused in callbacks, but are not accessible via
    /// keyboard / tab navigation (-1)
    NoKeyboardFocus,
}

impl_option!(
    TabIndex,
    OptionTabIndex,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl TabIndex {
    /// Returns the HTML-compatible number of the `tabindex` element
    pub fn get_index(&self) -> isize {
        use self::TabIndex::*;
        match self {
            Auto => 0,
            OverrideInParent(x) => *x as isize,
            NoKeyboardFocus => -1,
        }
    }
}

impl Default for TabIndex {
    fn default() -> Self {
        TabIndex::Auto
    }
}

impl Default for NodeData {
    fn default() -> Self {
        NodeData::new(NodeType::Div)
    }
}

/*
impl fmt::Debug for NodeData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}
*/

impl fmt::Display for NodeData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let html_type = self.node_type.get_path();
        let attributes_string = node_data_to_string(&self);

        match self.node_type.format() {
            Some(content) => write!(
                f,
                "<{}{}>{}</{}>",
                html_type, attributes_string, content, html_type
            ),
            None => write!(f, "<{}{}/>", html_type, attributes_string),
        }
    }
}

fn node_data_to_string(node_data: &NodeData) -> String {
    let mut id_string = String::new();
    let ids = node_data
        .ids_and_classes
        .as_ref()
        .iter()
        .filter_map(|s| s.as_id())
        .collect::<Vec<_>>()
        .join(" ");

    if !ids.is_empty() {
        id_string = format!(" id=\"{}\" ", ids);
    }

    let mut class_string = String::new();
    let classes = node_data
        .ids_and_classes
        .as_ref()
        .iter()
        .filter_map(|s| s.as_class())
        .collect::<Vec<_>>()
        .join(" ");

    if !classes.is_empty() {
        class_string = format!(" class=\"{}\" ", classes);
    }

    let mut tabindex_string = String::new();
    if let Some(tab_index) = node_data.get_tab_index() {
        tabindex_string = format!(" tabindex=\"{}\" ", tab_index.get_index());
    };

    format!("{}{}{}", id_string, class_string, tabindex_string)
}

impl NodeData {
    /// Creates a new `NodeData` instance from a given `NodeType`
    #[inline]
    pub const fn new(node_type: NodeType) -> Self {
        Self {
            node_type,
            dataset: OptionRefAny::None,
            ids_and_classes: IdOrClassVec::from_const_slice(&[]),
            callbacks: CallbackDataVec::from_const_slice(&[]),
            inline_css_props: NodeDataInlineCssPropertyVec::from_const_slice(&[]),
            tab_index: OptionTabIndex::None,
            extra: None,
        }
    }

    /// Shorthand for `NodeData::new(NodeType::Body)`.
    #[inline(always)]
    pub const fn body() -> Self {
        Self::new(NodeType::Body)
    }

    /// Shorthand for `NodeData::new(NodeType::Div)`.
    #[inline(always)]
    pub const fn div() -> Self {
        Self::new(NodeType::Div)
    }

    /// Shorthand for `NodeData::new(NodeType::Br)`.
    #[inline(always)]
    pub const fn br() -> Self {
        Self::new(NodeType::Br)
    }

    /// Shorthand for `NodeData::new(NodeType::Text(value.into()))`
    #[inline(always)]
    pub fn text<S: Into<AzString>>(value: S) -> Self {
        Self::new(NodeType::Text(value.into()))
    }

    /// Shorthand for `NodeData::new(NodeType::Image(image_id))`
    #[inline(always)]
    pub fn image(image: ImageRef) -> Self {
        Self::new(NodeType::Image(image))
    }

    #[inline(always)]
    pub fn iframe(data: RefAny, callback: IFrameCallbackType) -> Self {
        Self::new(NodeType::IFrame(IFrameNode {
            callback: IFrameCallback { cb: callback },
            data,
        }))
    }

    /// Checks whether this node is of the given node type (div, image, text)
    #[inline]
    pub fn is_node_type(&self, searched_type: NodeType) -> bool {
        self.node_type == searched_type
    }

    /// Checks whether this node has the searched ID attached
    pub fn has_id(&self, id: &str) -> bool {
        self.ids_and_classes
            .iter()
            .any(|id_or_class| id_or_class.as_id() == Some(id))
    }

    /// Checks whether this node has the searched class attached
    pub fn has_class(&self, class: &str) -> bool {
        self.ids_and_classes
            .iter()
            .any(|id_or_class| id_or_class.as_class() == Some(class))
    }

    pub fn has_context_menu(&self) -> bool {
        self.extra
            .as_ref()
            .map(|m| m.context_menu.is_some())
            .unwrap_or(false)
    }

    pub fn is_text_node(&self) -> bool {
        match self.node_type {
            NodeType::Text(_) => true,
            _ => false,
        }
    }

    // NOTE: Getters are used here in order to allow changing the memory allocator for the NodeData
    // in the future (which is why the fields are all private).

    #[inline(always)]
    pub const fn get_node_type(&self) -> &NodeType {
        &self.node_type
    }
    #[inline(always)]
    pub fn get_dataset_mut(&mut self) -> &mut OptionRefAny {
        &mut self.dataset
    }
    #[inline(always)]
    pub const fn get_dataset(&self) -> &OptionRefAny {
        &self.dataset
    }
    #[inline(always)]
    pub const fn get_ids_and_classes(&self) -> &IdOrClassVec {
        &self.ids_and_classes
    }
    #[inline(always)]
    pub const fn get_callbacks(&self) -> &CallbackDataVec {
        &self.callbacks
    }
    #[inline(always)]
    pub const fn get_inline_css_props(&self) -> &NodeDataInlineCssPropertyVec {
        &self.inline_css_props
    }

    #[inline]
    pub fn get_clip_mask(&self) -> Option<&ImageMask> {
        self.extra.as_ref().and_then(|e| e.clip_mask.as_ref())
    }
    #[inline]
    pub fn get_tab_index(&self) -> Option<&TabIndex> {
        self.tab_index.as_ref()
    }
    #[inline]
    pub fn get_accessibility_info(&self) -> Option<&Box<AccessibilityInfo>> {
        self.extra.as_ref().and_then(|e| e.accessibility.as_ref())
    }
    #[inline]
    pub fn get_menu_bar(&self) -> Option<&Box<Menu>> {
        self.extra.as_ref().and_then(|e| e.menu_bar.as_ref())
    }
    #[inline]
    pub fn get_context_menu(&self) -> Option<&Box<Menu>> {
        self.extra.as_ref().and_then(|e| e.context_menu.as_ref())
    }

    #[inline(always)]
    pub fn set_node_type(&mut self, node_type: NodeType) {
        self.node_type = node_type;
    }
    #[inline(always)]
    pub fn set_dataset(&mut self, data: OptionRefAny) {
        self.dataset = data;
    }
    #[inline(always)]
    pub fn set_ids_and_classes(&mut self, ids_and_classes: IdOrClassVec) {
        self.ids_and_classes = ids_and_classes;
    }
    #[inline(always)]
    pub fn set_callbacks(&mut self, callbacks: CallbackDataVec) {
        self.callbacks = callbacks;
    }
    #[inline(always)]
    pub fn set_inline_css_props(&mut self, inline_css_props: NodeDataInlineCssPropertyVec) {
        self.inline_css_props = inline_css_props;
    }
    #[inline]
    pub fn set_clip_mask(&mut self, clip_mask: ImageMask) {
        self.extra
            .get_or_insert_with(|| Box::new(NodeDataExt::default()))
            .clip_mask = Some(clip_mask);
    }
    #[inline]
    pub fn set_tab_index(&mut self, tab_index: TabIndex) {
        self.tab_index = Some(tab_index).into();
    }
    #[inline]
    pub fn set_accessibility_info(&mut self, accessibility_info: AccessibilityInfo) {
        self.extra
            .get_or_insert_with(|| Box::new(NodeDataExt::default()))
            .accessibility = Some(Box::new(accessibility_info));
    }
    #[inline]
    pub fn set_menu_bar(&mut self, menu_bar: Menu) {
        self.extra
            .get_or_insert_with(|| Box::new(NodeDataExt::default()))
            .menu_bar = Some(Box::new(menu_bar));
    }
    #[inline]
    pub fn set_context_menu(&mut self, context_menu: Menu) {
        self.extra
            .get_or_insert_with(|| Box::new(NodeDataExt::default()))
            .context_menu = Some(Box::new(context_menu));
    }

    #[inline]
    pub fn with_context_menu(mut self, context_menu: Menu) -> Self {
        self.set_context_menu(context_menu);
        self
    }

    #[inline]
    pub fn add_callback(&mut self, event: EventFilter, data: RefAny, callback: CallbackType) {
        let mut v: CallbackDataVec = Vec::new().into();
        mem::swap(&mut v, &mut self.callbacks);
        let mut v = v.into_library_owned_vec();
        v.push(CallbackData {
            event,
            data,
            callback: Callback { cb: callback },
        });
        self.callbacks = v.into();
    }
    #[inline]
    pub fn add_id(&mut self, s: AzString) {
        let mut v: IdOrClassVec = Vec::new().into();
        mem::swap(&mut v, &mut self.ids_and_classes);
        let mut v = v.into_library_owned_vec();
        v.push(IdOrClass::Id(s));
        self.ids_and_classes = v.into();
    }
    #[inline]
    pub fn add_class(&mut self, s: AzString) {
        let mut v: IdOrClassVec = Vec::new().into();
        mem::swap(&mut v, &mut self.ids_and_classes);
        let mut v = v.into_library_owned_vec();
        v.push(IdOrClass::Class(s));
        self.ids_and_classes = v.into();
    }
    #[inline]
    pub fn add_normal_css_property(&mut self, p: CssProperty) {
        let mut v: NodeDataInlineCssPropertyVec = Vec::new().into();
        mem::swap(&mut v, &mut self.inline_css_props);
        let mut v = v.into_library_owned_vec();
        v.push(NodeDataInlineCssProperty::Normal(p));
        self.inline_css_props = v.into();
    }
    #[inline]
    pub fn add_hover_css_property(&mut self, p: CssProperty) {
        let mut v: NodeDataInlineCssPropertyVec = Vec::new().into();
        mem::swap(&mut v, &mut self.inline_css_props);
        let mut v = v.into_library_owned_vec();
        v.push(NodeDataInlineCssProperty::Hover(p));
        self.inline_css_props = v.into();
    }
    #[inline]
    pub fn add_active_css_property(&mut self, p: CssProperty) {
        let mut v: NodeDataInlineCssPropertyVec = Vec::new().into();
        mem::swap(&mut v, &mut self.inline_css_props);
        let mut v = v.into_library_owned_vec();
        v.push(NodeDataInlineCssProperty::Active(p));
        self.inline_css_props = v.into();
    }
    #[inline]
    pub fn add_focus_css_property(&mut self, p: CssProperty) {
        let mut v: NodeDataInlineCssPropertyVec = Vec::new().into();
        mem::swap(&mut v, &mut self.inline_css_props);
        let mut v = v.into_library_owned_vec();
        v.push(NodeDataInlineCssProperty::Focus(p));
        self.inline_css_props = v.into();
    }

    /// Calculates a deterministic node hash for this node
    pub fn calculate_node_data_hash(&self) -> DomNodeHash {
        use highway::{HighwayHash, HighwayHasher, Key};
        let mut hasher = HighwayHasher::new(Key([0; 4]));
        self.hash(&mut hasher);
        let h = hasher.finalize64();
        DomNodeHash(h)
    }

    #[inline(always)]
    pub fn with_tab_index(mut self, tab_index: TabIndex) -> Self {
        self.set_tab_index(tab_index);
        self
    }
    #[inline(always)]
    pub fn with_dataset(mut self, data: OptionRefAny) -> Self {
        self.dataset = data;
        self
    }
    #[inline(always)]
    pub fn with_ids_and_classes(mut self, ids_and_classes: IdOrClassVec) -> Self {
        self.ids_and_classes = ids_and_classes;
        self
    }
    #[inline(always)]
    pub fn with_callbacks(mut self, callbacks: CallbackDataVec) -> Self {
        self.callbacks = callbacks;
        self
    }
    #[inline(always)]
    pub fn with_inline_css_props(mut self, inline_css_props: NodeDataInlineCssPropertyVec) -> Self {
        self.inline_css_props = inline_css_props;
        self
    }

    #[inline(always)]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = NodeData::div();
        mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn copy_special(&self) -> Self {
        Self {
            node_type: self.node_type.into_library_owned_nodetype(),
            dataset: match &self.dataset {
                OptionRefAny::None => OptionRefAny::None,
                OptionRefAny::Some(s) => OptionRefAny::Some(s.clone()),
            },
            ids_and_classes: self.ids_and_classes.clone(), /* do not clone the IDs and classes if
                                                            * they are &'static */
            inline_css_props: self.inline_css_props.clone(), /* do not clone the inline CSS props
                                                              * if they are &'static */
            callbacks: self.callbacks.clone(),
            tab_index: self.tab_index,
            extra: self.extra.clone(),
        }
    }

    pub fn is_focusable(&self) -> bool {
        // TODO: do some better analysis of next / first / item
        self.get_tab_index().is_some()
            || self
                .get_callbacks()
                .iter()
                .any(|cb| cb.event.is_focus_callback())
    }

    pub fn get_iframe_node(&mut self) -> Option<&mut IFrameNode> {
        match &mut self.node_type {
            NodeType::IFrame(i) => Some(i),
            _ => None,
        }
    }

    pub fn get_render_image_callback_node<'a>(
        &'a mut self,
    ) -> Option<(&'a mut ImageCallback, ImageRefHash)> {
        match &mut self.node_type {
            NodeType::Image(img) => {
                let hash = img.get_hash();
                img.get_image_callback_mut().map(|r| (r, hash))
            }
            _ => None,
        }
    }

    pub fn debug_print_start(
        &self,
        css_cache: &CssPropertyCache,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> String {
        let html_type = self.node_type.get_path();
        let attributes_string = node_data_to_string(&self);
        let style = css_cache.get_computed_css_style_string(&self, node_id, node_state);
        format!(
            "<{} data-az-node-id=\"{}\" {} {style}>",
            html_type,
            node_id.index(),
            attributes_string,
            style = if style.trim().is_empty() {
                String::new()
            } else {
                format!("style=\"{style}\"")
            }
        )
    }

    pub fn debug_print_end(&self) -> String {
        let html_type = self.node_type.get_path();
        format!("</{}>", html_type)
    }
}

/// The document model, similar to HTML. This is a create-only structure, you don't actually read
/// anything back
#[repr(C)]
#[derive(PartialEq, Clone, Eq, Hash, PartialOrd, Ord)]
pub struct Dom {
    pub root: NodeData,
    pub children: DomVec,
    // Tracks the number of sub-children of the current children, so that
    // the `Dom` can be converted into a `CompactDom`
    estimated_total_children: usize,
}

impl_option!(
    Dom,
    OptionDom,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(Dom, DomVec, DomVecDestructor);
impl_vec_clone!(Dom, DomVec, DomVecDestructor);
impl_vec_mut!(Dom, DomVec);
impl_vec_debug!(Dom, DomVec);
impl_vec_partialord!(Dom, DomVec);
impl_vec_ord!(Dom, DomVec);
impl_vec_partialeq!(Dom, DomVec);
impl_vec_eq!(Dom, DomVec);
impl_vec_hash!(Dom, DomVec);

impl Dom {
    // ----- DOM CONSTRUCTORS

    /// Creates an empty DOM with a give `NodeType`. Note: This is a `const fn` and
    /// doesn't allocate, it only allocates once you add at least one child node.
    #[inline(always)]
    pub fn new(node_type: NodeType) -> Self {
        Self {
            root: NodeData::new(node_type),
            children: Vec::new().into(),
            estimated_total_children: 0,
        }
    }
    #[inline(always)]
    pub fn from_data(node_data: NodeData) -> Self {
        Self {
            root: node_data,
            children: Vec::new().into(),
            estimated_total_children: 0,
        }
    }
    #[inline(always)]
    pub fn div() -> Self {
        Self::new(NodeType::Div)
    }
    #[inline(always)]
    pub fn body() -> Self {
        Self::new(NodeType::Body)
    }
    #[inline(always)]
    pub fn br() -> Self {
        Self::new(NodeType::Br)
    }
    #[inline(always)]
    pub fn text<S: Into<AzString>>(value: S) -> Self {
        Self::new(NodeType::Text(value.into()))
    }
    #[inline(always)]
    pub fn image(image: ImageRef) -> Self {
        Self::new(NodeType::Image(image))
    }
    #[inline(always)]
    pub fn iframe(data: RefAny, callback: IFrameCallbackType) -> Self {
        Self::new(NodeType::IFrame(IFrameNode {
            callback: IFrameCallback { cb: callback },
            data,
        }))
    }

    // Swaps `self` with a default DOM, necessary for builder methods
    #[inline(always)]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self {
            root: NodeData::div(),
            children: DomVec::from_const_slice(&[]),
            estimated_total_children: 0,
        };
        mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn add_child(&mut self, child: Dom) {
        let mut v: DomVec = Vec::new().into();
        mem::swap(&mut v, &mut self.children);
        let mut v = v.into_library_owned_vec();
        v.push(child);
        self.children = v.into();
        self.estimated_total_children += 1;
    }

    #[inline(always)]
    pub fn set_children(&mut self, children: DomVec) {
        let children_estimated = children
            .iter()
            .map(|s| s.estimated_total_children + 1)
            .sum();
        self.children = children;
        self.estimated_total_children = children_estimated;
    }

    pub fn copy_except_for_root(&mut self) -> Self {
        Self {
            root: self.root.copy_special(),
            children: self.children.clone(),
            estimated_total_children: self.estimated_total_children,
        }
    }
    pub fn node_count(&self) -> usize {
        self.estimated_total_children + 1
    }

    pub fn style(&mut self, css: azul_css::parser::CssApiWrapper) -> StyledDom {
        StyledDom::new(self, css)
    }
    #[inline(always)]
    pub fn with_children(mut self, children: DomVec) -> Self {
        self.children = children;
        self
    }
    #[inline(always)]
    pub fn with_child(&mut self, child: Self) -> Self {
        let mut dom = self.swap_with_default();
        dom.add_child(child);
        dom
    }
    #[inline(always)]
    pub fn with_tab_index(mut self, tab_index: TabIndex) -> Self {
        self.root.set_tab_index(tab_index);
        self
    }
    #[inline(always)]
    pub fn with_dataset(mut self, data: OptionRefAny) -> Self {
        self.root.dataset = data;
        self
    }
    #[inline(always)]
    pub fn with_ids_and_classes(mut self, ids_and_classes: IdOrClassVec) -> Self {
        self.root.ids_and_classes = ids_and_classes;
        self
    }
    #[inline(always)]
    pub fn with_callbacks(mut self, callbacks: CallbackDataVec) -> Self {
        self.root.callbacks = callbacks;
        self
    }
    #[inline(always)]
    pub fn with_inline_css_props(mut self, inline_css_props: NodeDataInlineCssPropertyVec) -> Self {
        self.root.inline_css_props = inline_css_props;
        self
    }

    pub fn set_inline_style(&mut self, style: &str) {
        self.root.set_inline_css_props(
            self.root
                .get_inline_css_props()
                .with_append(NodeDataInlineCssPropertyVec::parse_normal(style)),
        )
    }

    pub fn with_inline_style(mut self, style: &str) -> Self {
        self.set_inline_style(style);
        self
    }

    #[inline]
    pub fn with_context_menu(mut self, context_menu: Menu) -> Self {
        self.root.set_context_menu(context_menu);
        self
    }

    fn fixup_children_estimated(&mut self) -> usize {
        if self.children.is_empty() {
            self.estimated_total_children = 0;
        } else {
            self.estimated_total_children = self
                .children
                .iter_mut()
                .map(|s| s.fixup_children_estimated() + 1)
                .sum();
        }
        return self.estimated_total_children;
    }
}

impl core::iter::FromIterator<Dom> for Dom {
    fn from_iter<I: IntoIterator<Item = Dom>>(iter: I) -> Self {
        let mut estimated_total_children = 0;
        let children = iter
            .into_iter()
            .map(|c| {
                estimated_total_children += c.estimated_total_children + 1;
                c
            })
            .collect::<Vec<Dom>>();

        Dom {
            root: NodeData::div(),
            children: children.into(),
            estimated_total_children,
        }
    }
}

impl fmt::Debug for Dom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fn print_dom(d: &Dom, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "Dom {{\r\n")?;
            write!(f, "\troot: {:#?}\r\n", d.root)?;
            write!(
                f,
                "\testimated_total_children: {:#?}\r\n",
                d.estimated_total_children
            )?;
            write!(f, "\tchildren: [\r\n")?;
            for c in d.children.iter() {
                print_dom(c, f)?;
            }
            write!(f, "\t]\r\n")?;
            write!(f, "}}\r\n")?;
            Ok(())
        }

        print_dom(self, f)
    }
}

/// Same as `Dom`, but arena-based for more efficient memory layout
#[derive(Debug, PartialEq, PartialOrd, Eq)]
pub struct CompactDom {
    pub node_hierarchy: NodeHierarchy,
    pub node_data: NodeDataContainer<NodeData>,
    pub root: NodeId,
}

impl CompactDom {
    /// Returns the number of nodes in this DOM
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.node_hierarchy.as_ref().len()
    }
}

impl From<Dom> for CompactDom {
    fn from(dom: Dom) -> Self {
        convert_dom_into_compact_dom(dom)
    }
}

// NEW: Helper function to flatten a Dom tree into the arena recursively.
// It populates `node_hierarchy_arena` and `node_data_arena`.
fn flatten_dom_to_arena_recursive(
    dom_node: Dom, // Takes ownership of the Dom subtree.
    node_hierarchy_arena: &mut [Node],
    node_data_arena: &mut [NodeData],
    // This is the arena ID of the *actual parent* in the flattened tree.
    parent_arena_id_for_linking: NodeId,
    arena_index_counter: &mut usize, // Global counter for arena indices.
) {
    let self_arena_id = NodeId::new(*arena_index_counter);
    *arena_index_counter += 1; // Claim this index for the current node.

    // Copy the node's data.
    node_data_arena[self_arena_id.index()] = dom_node.root; // dom_node.root is NodeData, no copy_special needed as it's moved

    // Link the current node (self_arena_id) to its parent in the arena.
    // The real root (NodeId(0)) has no parent.
    if self_arena_id.index() != NodeId::ZERO.index() { // NodeId::ZERO is NodeId(0)
        node_hierarchy_arena[self_arena_id.index()].parent = Some(parent_arena_id_for_linking);
    } else {
        node_hierarchy_arena[self_arena_id.index()].parent = None; // Explicit None for the root.
    }

    let mut prev_child_arena_id_for_linking: Option<NodeId> = None; // Tracks the last child added for sibling linking.
    let children_vec = dom_node.children.into_library_owned_vec(); // Consume children

    for (child_idx, child_dom_subtree) in children_vec.into_iter().enumerate() {
        let child_arena_id_will_be = NodeId::new(*arena_index_counter); // The arena ID the child *will* get.

        // Recursively flatten the child subtree. This call increments `arena_index_counter`.
        flatten_dom_to_arena_recursive(
            child_dom_subtree,
            node_hierarchy_arena,
            node_data_arena,
            self_arena_id, // The current node is the parent for this child.
            arena_index_counter,
        );

        // After the child has been flattened, its actual arena ID is `child_arena_id_will_be`.

        // Link the parent (current node) to its children: first_child and last_child.
        if child_idx == 0 {
            node_hierarchy_arena[self_arena_id.index()].first_child = Some(child_arena_id_will_be);
        }
        // Always update last_child to point to the latest child flattened in this parent's list.
        node_hierarchy_arena[self_arena_id.index()].last_child = Some(child_arena_id_will_be);

        // Link the children to each other: previous_sibling and next_sibling.
        if let Some(prev_child_id) = prev_child_arena_id_for_linking {
            node_hierarchy_arena[prev_child_id.index()].next_sibling = Some(child_arena_id_will_be);
        }
        node_hierarchy_arena[child_arena_id_will_be.index()].previous_sibling = prev_child_arena_id_for_linking;

        // Update `prev_child_arena_id_for_linking` for the next iteration.
        prev_child_arena_id_for_linking = Some(child_arena_id_will_be);
    }
}

pub fn convert_dom_into_compact_dom(dom: Dom) -> CompactDom {
    // Stage 1: Generate anonymous table elements in the `Dom` tree.
    // `total_nodes_after_anon` gives the exact size needed for the arena.
    let (processed_dom, total_nodes_after_anon) = generate_anonymous_table_elements(dom);

    // Initialize arenas with the exact size needed.
    // The counter starts at 0, so total_nodes_after_anon is the count of nodes.
    // The arrays need to be sized for 0 to N-1 nodes.
    // If total_nodes_after_anon is 0 (e.g. empty Dom), create empty arenas.
    let mut node_hierarchy_arena = if total_nodes_after_anon > 0 {
        vec![Node::ROOT; total_nodes_after_anon]
    } else {
        Vec::new()
    };
    let mut node_data_arena = if total_nodes_after_anon > 0 {
        vec![NodeData::new(NodeType::Div); total_nodes_after_anon] // Use NodeData::new for default
    } else {
        Vec::new()
    };

    let mut arena_index_counter = 0; // Tracks the next available index for placement.

    // If there are nodes to process (i.e., total_nodes_after_anon > 0),
    // flatten the processed `Dom` into the arena, starting from the root at index 0.
    if total_nodes_after_anon > 0 {
        flatten_dom_to_arena_recursive(
            processed_dom,
            &mut node_hierarchy_arena,
            &mut node_data_arena,
            NodeId::new(0), // Parent of root is conceptually NodeId(0) for initial call context.
                            // flatten_dom_to_arena_recursive handles NodeId::ZERO correctly for parent linking.
            &mut arena_index_counter,
        );
    }

    CompactDom {
        node_hierarchy: NodeHierarchy {
            internal: node_hierarchy_arena,
        },
        node_data: NodeDataContainer {
            internal: node_data_arena,
        },
        // The root is always at index 0 if there are any nodes.
        // If total_nodes_after_anon is 0, the arenas are empty, and a root of NodeId::ZERO
        // would be out of bounds. However, CompactDom expects a root.
        // A truly empty DOM might be better represented by Option<CompactDom> or
        // CompactDom having Option<NodeId> for root.
        // For now, assume if total_nodes_after_anon is 0, it implies an empty CompactDom where root is somewhat conceptual.
        // Let's ensure NodeId::ZERO is valid only if nodes exist.
        // If no nodes, perhaps root should be a "null" or sentinel NodeId if the type allows,
        // or the caller handles an empty CompactDom.
        // Given current CompactDom structure, root must be a NodeId.
        // If total_nodes_after_anon is 0, this means no nodes, so NodeId::ZERO is technically invalid.
        // This indicates a potential design consideration for "empty" CompactDoms.
        // However, generate_anonymous_table_elements always returns at least 1 for the root node itself.
        root: NodeId::ZERO,
    }
}
