//! Defines the core Document Object Model (DOM) structures.
//!
//! This module is responsible for representing the UI as a tree of nodes,
//! similar to the HTML DOM. It includes definitions for node types, event handling,
//! accessibility, and the main `Dom` and `CompactDom` structures.

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

use azul_css::{
    css::{Css, NodeTypeTag},
    format_rust_code::GetHash,
    props::{
        basic::{FloatValue, FontRef},
        layout::{LayoutDisplay, LayoutFloat, LayoutPosition},
        property::CssProperty,
    },
    AzString, OptionString,
};

// Re-export event filters from events module (moved in Phase 3.5)
pub use crate::events::{
    ApplicationEventFilter, ComponentEventFilter, EventFilter, FocusEventFilter, HoverEventFilter,
    NotEventFilter, WindowEventFilter,
};
pub use crate::id::{Node, NodeHierarchy, NodeId};
use crate::{
    callbacks::{
        CoreCallback, CoreCallbackData, CoreCallbackDataVec, CoreCallbackType, VirtualizedViewCallback,
        VirtualizedViewCallbackType,
    },
    geom::LogicalPosition,
    id::{NodeDataContainer, NodeDataContainerRef, NodeDataContainerRefMut},
    menu::Menu,
    prop_cache::{CssPropertyCache, CssPropertyCachePtr},
    refany::{OptionRefAny, RefAny},
    resources::{
        image_ref_get_hash, CoreImageCallback, ImageMask, ImageRef, ImageRefHash, RendererResources,
    },
    styled_dom::{
        CompactDom, NodeHierarchyItemId, StyleFontFamilyHash, StyledDom, StyledNode,
        StyledNodeState,
    },
    window::OptionVirtualKeyCodeCombo,
};
pub use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};

static TAG_ID: AtomicUsize = AtomicUsize::new(1);

/// Strongly-typed input element types for HTML `<input>` elements.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum InputType {
    /// Text input (default)
    Text,
    /// Button
    Button,
    /// Checkbox
    Checkbox,
    /// Color picker
    Color,
    /// Date picker
    Date,
    /// Date and time picker
    Datetime,
    /// Date and time picker (local)
    DatetimeLocal,
    /// Email address input
    Email,
    /// File upload
    File,
    /// Hidden input
    Hidden,
    /// Image button
    Image,
    /// Month picker
    Month,
    /// Number input
    Number,
    /// Password input
    Password,
    /// Radio button
    Radio,
    /// Range slider
    Range,
    /// Reset button
    Reset,
    /// Search input
    Search,
    /// Submit button
    Submit,
    /// Telephone number input
    Tel,
    /// Time picker
    Time,
    /// URL input
    Url,
    /// Week picker
    Week,
}

impl InputType {
    /// Returns the HTML attribute value for this input type
    pub const fn as_str(&self) -> &'static str {
        match self {
            InputType::Text => "text",
            InputType::Button => "button",
            InputType::Checkbox => "checkbox",
            InputType::Color => "color",
            InputType::Date => "date",
            InputType::Datetime => "datetime",
            InputType::DatetimeLocal => "datetime-local",
            InputType::Email => "email",
            InputType::File => "file",
            InputType::Hidden => "hidden",
            InputType::Image => "image",
            InputType::Month => "month",
            InputType::Number => "number",
            InputType::Password => "password",
            InputType::Radio => "radio",
            InputType::Range => "range",
            InputType::Reset => "reset",
            InputType::Search => "search",
            InputType::Submit => "submit",
            InputType::Tel => "tel",
            InputType::Time => "time",
            InputType::Url => "url",
            InputType::Week => "week",
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct TagId {
    pub inner: u64,
}

impl ::core::fmt::Display for TagId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("TagId").field("inner", &self.inner).finish()
    }
}

impl_option!(
    TagId,
    OptionTagId,
    [Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash]
);

impl TagId {
    pub const fn into_crate_internal(&self) -> TagId {
        TagId { inner: self.inner }
    }
    pub const fn from_crate_internal(t: TagId) -> Self {
        TagId { inner: t.inner }
    }

    /// Creates a new, unique hit-testing tag ID.
    /// Wraps around to 1 on overflow (0 is reserved for "no tag").
    pub fn unique() -> Self {
        loop {
            let current = TAG_ID.load(Ordering::SeqCst);
            let next = if current == usize::MAX { 1 } else { current + 1 };
            if TAG_ID.compare_exchange(current, next, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                return TagId { inner: current as u64 };
            }
        }
    }
}

/// Same as the `TagId`, but only for scrollable nodes.
/// This provides a typed distinction for tags associated with scrolling containers.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[repr(C)]
pub struct ScrollTagId {
    pub inner: TagId,
}

impl ::core::fmt::Display for ScrollTagId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ScrollTagId")
            .field("inner", &self.inner)
            .finish()
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
        ScrollTagId {
            inner: TagId::unique(),
        }
    }
}

/// Orientation of a scrollbar.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum ScrollbarOrientation {
    Horizontal,
    Vertical,
}

/// Calculated hash of a DOM node, used for identifying identical DOM
/// nodes across frames for efficient diffing and state preservation.
#[derive(Copy, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
#[repr(C)]
pub struct DomNodeHash {
    pub inner: u64,
}

impl ::core::fmt::Debug for DomNodeHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DomNodeHash({})", self.inner)
    }
}

/// List of core DOM node types built into `azul`.
/// This enum defines the building blocks of the UI, similar to HTML tags.
#[derive(Debug, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum NodeType {
    // Root and container elements
    /// Root HTML element.
    Html,
    /// Document head (metadata container).
    Head,
    /// Root element of the document body.
    Body,
    /// Generic block-level container.
    Div,
    /// Paragraph.
    P,
    /// Article content.
    Article,
    /// Section of a document.
    Section,
    /// Navigation links.
    Nav,
    /// Sidebar/tangential content.
    Aside,
    /// Header section.
    Header,
    /// Footer section.
    Footer,
    /// Main content.
    Main,
    /// Figure with optional caption.
    Figure,
    /// Caption for figure element.
    FigCaption,
    /// Headings.
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    /// Line break.
    Br,
    /// Horizontal rule.
    Hr,
    /// Preformatted text.
    Pre,
    /// Block quote.
    BlockQuote,
    /// Address.
    Address,
    /// Details disclosure widget.
    Details,
    /// Summary for details element.
    Summary,
    /// Dialog box or window.
    Dialog,

    // List elements
    /// Unordered list.
    Ul,
    /// Ordered list.
    Ol,
    /// List item.
    Li,
    /// Definition list.
    Dl,
    /// Definition term.
    Dt,
    /// Definition description.
    Dd,
    /// Menu list.
    Menu,
    /// Menu item.
    MenuItem,
    /// Directory list (deprecated).
    Dir,

    // Table elements
    /// Table container.
    Table,
    /// Table caption.
    Caption,
    /// Table header.
    THead,
    /// Table body.
    TBody,
    /// Table footer.
    TFoot,
    /// Table row.
    Tr,
    /// Table header cell.
    Th,
    /// Table data cell.
    Td,
    /// Table column group.
    ColGroup,
    /// Table column.
    Col,

    // Form elements
    /// Form container.
    Form,
    /// Form fieldset.
    FieldSet,
    /// Fieldset legend.
    Legend,
    /// Label for form controls.
    Label,
    /// Input control.
    Input,
    /// Button control.
    Button,
    /// Select dropdown.
    Select,
    /// Option group.
    OptGroup,
    /// Select option.
    SelectOption,
    /// Multiline text input.
    TextArea,
    /// Form output element.
    Output,
    /// Progress indicator.
    Progress,
    /// Scalar measurement within a known range.
    Meter,
    /// List of predefined options for input.
    DataList,

    // Inline elements
    /// Generic inline container.
    Span,
    /// Anchor/hyperlink.
    A,
    /// Emphasized text.
    Em,
    /// Strongly emphasized text.
    Strong,
    /// Bold text (deprecated - use `Dom::create_strong()` for semantic importance).
    B,
    /// Italic text (deprecated - use `Dom::create_em()` for emphasis or `Dom::create_cite()` for citations).
    I,
    /// Underline text.
    U,
    /// Strikethrough text.
    S,
    /// Marked/highlighted text.
    Mark,
    /// Deleted text.
    Del,
    /// Inserted text.
    Ins,
    /// Code.
    Code,
    /// Sample output.
    Samp,
    /// Keyboard input.
    Kbd,
    /// Variable.
    Var,
    /// Citation.
    Cite,
    /// Defining instance of a term.
    Dfn,
    /// Abbreviation.
    Abbr,
    /// Acronym.
    Acronym,
    /// Inline quotation.
    Q,
    /// Date/time.
    Time,
    /// Subscript.
    Sub,
    /// Superscript.
    Sup,
    /// Small text (deprecated - use CSS `font-size` instead).
    Small,
    /// Big text (deprecated - use CSS `font-size` instead).
    Big,
    /// Bi-directional override.
    Bdo,
    /// Bi-directional isolate.
    Bdi,
    /// Word break opportunity.
    Wbr,
    /// Ruby annotation.
    Ruby,
    /// Ruby text.
    Rt,
    /// Ruby text container.
    Rtc,
    /// Ruby parenthesis.
    Rp,
    /// Machine-readable data.
    Data,

    // Embedded content
    /// Canvas for graphics.
    Canvas,
    /// Embedded object.
    Object,
    /// Embedded object parameter.
    Param,
    /// External resource embed.
    Embed,
    /// Audio content.
    Audio,
    /// Video content.
    Video,
    /// Media source.
    Source,
    /// Text track for media.
    Track,
    /// Image map.
    Map,
    /// Image map area.
    Area,
    /// SVG graphics.
    Svg,

    // Metadata elements
    /// Document title.
    Title,
    /// Metadata.
    Meta,
    /// External resource link.
    Link,
    /// Embedded or referenced script.
    Script,
    /// Style information.
    Style,
    /// Base URL for relative URLs.
    Base,

    // Pseudo-elements (transformed into real elements)
    /// ::before pseudo-element.
    Before,
    /// ::after pseudo-element.
    After,
    /// ::marker pseudo-element.
    Marker,
    /// ::placeholder pseudo-element.
    Placeholder,

    // Special content types
    /// Text content, ::text
    Text(AzString),
    /// Image element, ::image
    Image(ImageRef),
    /// VirtualizedView (embedded content) - payload stored in NodeDataExt.virtualized_view
    VirtualizedView,
    /// Icon element - resolved to actual content by IconProvider
    /// The string is the icon name (e.g., "home", "settings", "search")
    Icon(AzString),
}

impl_option!(NodeType, OptionNodeType, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl NodeType {
    fn into_library_owned_nodetype(&self) -> Self {
        use self::NodeType::*;
        match self {
            Html => Html,
            Head => Head,
            Body => Body,
            Div => Div,
            P => P,
            Article => Article,
            Section => Section,
            Nav => Nav,
            Aside => Aside,
            Header => Header,
            Footer => Footer,
            Main => Main,
            Figure => Figure,
            FigCaption => FigCaption,
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
            Details => Details,
            Summary => Summary,
            Dialog => Dialog,
            Ul => Ul,
            Ol => Ol,
            Li => Li,
            Dl => Dl,
            Dt => Dt,
            Dd => Dd,
            Menu => Menu,
            MenuItem => MenuItem,
            Dir => Dir,
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
            Output => Output,
            Progress => Progress,
            Meter => Meter,
            DataList => DataList,
            Span => Span,
            A => A,
            Em => Em,
            Strong => Strong,
            B => B,
            I => I,
            U => U,
            S => S,
            Mark => Mark,
            Del => Del,
            Ins => Ins,
            Code => Code,
            Samp => Samp,
            Kbd => Kbd,
            Var => Var,
            Cite => Cite,
            Dfn => Dfn,
            Abbr => Abbr,
            Acronym => Acronym,
            Q => Q,
            Time => Time,
            Sub => Sub,
            Sup => Sup,
            Small => Small,
            Big => Big,
            Bdo => Bdo,
            Bdi => Bdi,
            Wbr => Wbr,
            Ruby => Ruby,
            Rt => Rt,
            Rtc => Rtc,
            Rp => Rp,
            Data => Data,
            Canvas => Canvas,
            Object => Object,
            Param => Param,
            Embed => Embed,
            Audio => Audio,
            Video => Video,
            Source => Source,
            Track => Track,
            Map => Map,
            Area => Area,
            Svg => Svg,
            Title => Title,
            Meta => Meta,
            Link => Link,
            Script => Script,
            Style => Style,
            Base => Base,
            Before => Before,
            After => After,
            Marker => Marker,
            Placeholder => Placeholder,

            Text(s) => Text(s.clone_self()),
            Image(i) => Image(i.clone()), // note: shallow clone
            VirtualizedView => VirtualizedView,
            Icon(s) => Icon(s.clone_self()),
        }
    }

    pub fn format(&self) -> Option<String> {
        use self::NodeType::*;
        match self {
            Text(s) => Some(format!("{}", s)),
            Image(id) => Some(format!("image({:?})", id)),
            VirtualizedView => Some("virtualized-view".to_string()),
            Icon(s) => Some(format!("icon({})", s)),
            _ => None,
        }
    }

    /// Returns the NodeTypeTag for CSS selector matching.
    pub fn get_path(&self) -> NodeTypeTag {
        match self {
            Self::Html => NodeTypeTag::Html,
            Self::Head => NodeTypeTag::Head,
            Self::Body => NodeTypeTag::Body,
            Self::Div => NodeTypeTag::Div,
            Self::P => NodeTypeTag::P,
            Self::Article => NodeTypeTag::Article,
            Self::Section => NodeTypeTag::Section,
            Self::Nav => NodeTypeTag::Nav,
            Self::Aside => NodeTypeTag::Aside,
            Self::Header => NodeTypeTag::Header,
            Self::Footer => NodeTypeTag::Footer,
            Self::Main => NodeTypeTag::Main,
            Self::Figure => NodeTypeTag::Figure,
            Self::FigCaption => NodeTypeTag::FigCaption,
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
            Self::Details => NodeTypeTag::Details,
            Self::Summary => NodeTypeTag::Summary,
            Self::Dialog => NodeTypeTag::Dialog,
            Self::Ul => NodeTypeTag::Ul,
            Self::Ol => NodeTypeTag::Ol,
            Self::Li => NodeTypeTag::Li,
            Self::Dl => NodeTypeTag::Dl,
            Self::Dt => NodeTypeTag::Dt,
            Self::Dd => NodeTypeTag::Dd,
            Self::Menu => NodeTypeTag::Menu,
            Self::MenuItem => NodeTypeTag::MenuItem,
            Self::Dir => NodeTypeTag::Dir,
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
            Self::Output => NodeTypeTag::Output,
            Self::Progress => NodeTypeTag::Progress,
            Self::Meter => NodeTypeTag::Meter,
            Self::DataList => NodeTypeTag::DataList,
            Self::Span => NodeTypeTag::Span,
            Self::A => NodeTypeTag::A,
            Self::Em => NodeTypeTag::Em,
            Self::Strong => NodeTypeTag::Strong,
            Self::B => NodeTypeTag::B,
            Self::I => NodeTypeTag::I,
            Self::U => NodeTypeTag::U,
            Self::S => NodeTypeTag::S,
            Self::Mark => NodeTypeTag::Mark,
            Self::Del => NodeTypeTag::Del,
            Self::Ins => NodeTypeTag::Ins,
            Self::Code => NodeTypeTag::Code,
            Self::Samp => NodeTypeTag::Samp,
            Self::Kbd => NodeTypeTag::Kbd,
            Self::Var => NodeTypeTag::Var,
            Self::Cite => NodeTypeTag::Cite,
            Self::Dfn => NodeTypeTag::Dfn,
            Self::Abbr => NodeTypeTag::Abbr,
            Self::Acronym => NodeTypeTag::Acronym,
            Self::Q => NodeTypeTag::Q,
            Self::Time => NodeTypeTag::Time,
            Self::Sub => NodeTypeTag::Sub,
            Self::Sup => NodeTypeTag::Sup,
            Self::Small => NodeTypeTag::Small,
            Self::Big => NodeTypeTag::Big,
            Self::Bdo => NodeTypeTag::Bdo,
            Self::Bdi => NodeTypeTag::Bdi,
            Self::Wbr => NodeTypeTag::Wbr,
            Self::Ruby => NodeTypeTag::Ruby,
            Self::Rt => NodeTypeTag::Rt,
            Self::Rtc => NodeTypeTag::Rtc,
            Self::Rp => NodeTypeTag::Rp,
            Self::Data => NodeTypeTag::Data,
            Self::Canvas => NodeTypeTag::Canvas,
            Self::Object => NodeTypeTag::Object,
            Self::Param => NodeTypeTag::Param,
            Self::Embed => NodeTypeTag::Embed,
            Self::Audio => NodeTypeTag::Audio,
            Self::Video => NodeTypeTag::Video,
            Self::Source => NodeTypeTag::Source,
            Self::Track => NodeTypeTag::Track,
            Self::Map => NodeTypeTag::Map,
            Self::Area => NodeTypeTag::Area,
            Self::Svg => NodeTypeTag::Svg,
            Self::Title => NodeTypeTag::Title,
            Self::Meta => NodeTypeTag::Meta,
            Self::Link => NodeTypeTag::Link,
            Self::Script => NodeTypeTag::Script,
            Self::Style => NodeTypeTag::Style,
            Self::Base => NodeTypeTag::Base,
            Self::Text(_) => NodeTypeTag::Text,
            Self::Image(_) => NodeTypeTag::Img,
            Self::VirtualizedView => NodeTypeTag::VirtualizedView,
            Self::Icon(_) => NodeTypeTag::Icon,
            Self::Before => NodeTypeTag::Before,
            Self::After => NodeTypeTag::After,
            Self::Marker => NodeTypeTag::Marker,
            Self::Placeholder => NodeTypeTag::Placeholder,
        }
    }

    /// Returns whether this node type is a semantic HTML element that should
    /// automatically generate an accessibility tree node.
    ///
    /// These are elements with inherent semantic meaning that assistive
    /// technologies should be aware of, even without explicit ARIA attributes.
    pub const fn is_semantic_for_accessibility(&self) -> bool {
        matches!(
            self,
            Self::Button
                | Self::Input
                | Self::TextArea
                | Self::Select
                | Self::A
                | Self::H1
                | Self::H2
                | Self::H3
                | Self::H4
                | Self::H5
                | Self::H6
                | Self::Article
                | Self::Section
                | Self::Nav
                | Self::Main
                | Self::Header
                | Self::Footer
                | Self::Aside
        )
    }
}

/// Represents the CSS formatting context for an element
#[derive(Clone, PartialEq)]
pub enum FormattingContext {
    /// Block-level formatting context
    Block {
        /// Whether this element establishes a new block formatting context
        establishes_new_context: bool,
    },
    /// Inline-level formatting context
    Inline,
    /// Inline-block (participates in an IFC but creates a BFC)
    InlineBlock,
    /// Flex formatting context
    Flex,
    /// Float (left or right)
    Float(LayoutFloat),
    /// Absolutely positioned (out of flow)
    OutOfFlow(LayoutPosition),
    /// Table formatting context (container)
    Table,
    /// Table row group formatting context (thead, tbody, tfoot)
    TableRowGroup,
    /// Table row formatting context
    TableRow,
    /// Table cell formatting context (td, th)
    TableCell,
    /// Table column group formatting context
    TableColumnGroup,
    /// Table caption formatting context
    TableCaption,
    /// Grid formatting context
    Grid,
    /// No formatting context (display: none)
    None,
}

impl fmt::Debug for FormattingContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormattingContext::Block {
                establishes_new_context,
            } => write!(
                f,
                "Block {{ establishes_new_context: {establishes_new_context:?} }}"
            ),
            FormattingContext::Inline => write!(f, "Inline"),
            FormattingContext::InlineBlock => write!(f, "InlineBlock"),
            FormattingContext::Flex => write!(f, "Flex"),
            FormattingContext::Float(layout_float) => write!(f, "Float({layout_float:?})"),
            FormattingContext::OutOfFlow(layout_position) => {
                write!(f, "OutOfFlow({layout_position:?})")
            }
            FormattingContext::Grid => write!(f, "Grid"),
            FormattingContext::None => write!(f, "None"),
            FormattingContext::Table => write!(f, "Table"),
            FormattingContext::TableRowGroup => write!(f, "TableRowGroup"),
            FormattingContext::TableRow => write!(f, "TableRow"),
            FormattingContext::TableCell => write!(f, "TableCell"),
            FormattingContext::TableColumnGroup => write!(f, "TableColumnGroup"),
            FormattingContext::TableCaption => write!(f, "TableCaption"),
        }
    }
}

impl Default for FormattingContext {
    fn default() -> Self {
        FormattingContext::Block {
            establishes_new_context: false,
        }
    }
}

/// Defines the type of event that can trigger a callback action.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub enum On {
    /// Mouse cursor is hovering over the element.
    MouseOver,
    /// Mouse cursor has is over element and is pressed
    /// (not good for "click" events - use `MouseUp` instead).
    MouseDown,
    /// (Specialization of `MouseDown`). Fires only if the left mouse button
    /// has been pressed while cursor was over the element.
    LeftMouseDown,
    /// (Specialization of `MouseDown`). Fires only if the middle mouse button
    /// has been pressed while cursor was over the element.
    MiddleMouseDown,
    /// (Specialization of `MouseDown`). Fires only if the right mouse button
    /// has been pressed while cursor was over the element.
    RightMouseDown,
    /// Mouse button has been released while cursor was over the element.
    MouseUp,
    /// (Specialization of `MouseUp`). Fires only if the left mouse button has
    /// been released while cursor was over the element.
    LeftMouseUp,
    /// (Specialization of `MouseUp`). Fires only if the middle mouse button has
    /// been released while cursor was over the element.
    MiddleMouseUp,
    /// (Specialization of `MouseUp`). Fires only if the right mouse button has
    /// been released while cursor was over the element.
    RightMouseUp,
    /// Mouse cursor has entered the element.
    MouseEnter,
    /// Mouse cursor has left the element.
    MouseLeave,
    /// Mousewheel / touchpad scrolling.
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
    /// A file has been dropped on the element.
    HoveredFile,
    /// A file is being hovered on the element.
    DroppedFile,
    /// A file was hovered, but has exited the window.
    HoveredFileCancelled,
    /// Equivalent to `onfocus`.
    FocusReceived,
    /// Equivalent to `onblur`.
    FocusLost,

    // Accessibility-specific events
    /// Default action triggered by screen reader (usually same as click/activate)
    Default,
    /// Element should collapse (e.g., accordion panel, tree node)
    Collapse,
    /// Element should expand (e.g., accordion panel, tree node)
    Expand,
    /// Increment value (e.g., number input, slider)
    Increment,
    /// Decrement value (e.g., number input, slider)
    Decrement,
}

// NOTE: EventFilter types moved to core/src/events.rs (Phase 3.5)
//
// The following types are now defined in events.rs and re-exported above:
// - EventFilter
// - HoverEventFilter
// - FocusEventFilter
// - WindowEventFilter
// - NotEventFilter
// - ComponentEventFilter
// - ApplicationEventFilter
//
// This consolidates all event-related logic in one place.

/// Contains the necessary information to render an embedded `VirtualizedView` node.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct VirtualizedViewNode {
    /// The callback function that returns the DOM for the virtualized view's content.
    pub callback: VirtualizedViewCallback,
    /// The application data passed to the virtualized view's layout callback.
    pub refany: RefAny,
}

/// An enum that holds either a CSS ID or a class name as a string.
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IdOrClass {
    Id(AzString),
    Class(AzString),
}

impl_option!(
    IdOrClass,
    OptionIdOrClass,
    copy = false,
    [Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord]
);

impl_vec!(IdOrClass, IdOrClassVec, IdOrClassVecDestructor, IdOrClassVecDestructorType, IdOrClassVecSlice, OptionIdOrClass);
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

/// Name-value pair for custom attributes (data-*, aria-*, etc.)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct AttributeNameValue {
    pub attr_name: AzString,
    pub value: AzString,
}

/// Strongly-typed HTML attribute with type-safe values.
///
/// This enum provides a type-safe way to represent HTML attributes, ensuring that
/// values are validated at compile-time and properly converted to their string
/// representations at runtime.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum AttributeType {
    /// Element ID attribute (`id="..."`)
    Id(AzString),
    /// CSS class attribute (`class="..."`)
    Class(AzString),
    /// Accessible name/label (`aria-label="..."`)
    AriaLabel(AzString),
    /// Element that labels this one (`aria-labelledby="..."`)
    AriaLabelledBy(AzString),
    /// Element that describes this one (`aria-describedby="..."`)
    AriaDescribedBy(AzString),
    /// Role for accessibility (`role="..."`)
    AriaRole(AzString),
    /// Current state of an element (`aria-checked`, `aria-selected`, etc.)
    AriaState(AttributeNameValue),
    /// ARIA property (`aria-*`)
    AriaProperty(AttributeNameValue),

    /// Hyperlink target URL (`href="..."`)
    Href(AzString),
    /// Link relationship (`rel="..."`)
    Rel(AzString),
    /// Link target frame (`target="..."`)
    Target(AzString),

    /// Image source URL (`src="..."`)
    Src(AzString),
    /// Alternative text for images (`alt="..."`)
    Alt(AzString),
    /// Image title (tooltip) (`title="..."`)
    Title(AzString),

    /// Form input name (`name="..."`)
    Name(AzString),
    /// Form input value (`value="..."`)
    Value(AzString),
    /// Input type (`type="text|password|email|..."`)
    InputType(AzString),
    /// Placeholder text (`placeholder="..."`)
    Placeholder(AzString),
    /// Input is required (`required`)
    Required,
    /// Input is disabled (`disabled`)
    Disabled,
    /// Input is readonly (`readonly`)
    Readonly,
    /// Input is checked (checkbox/radio) (`checked`)
    Checked,
    /// Input is selected (option) (`selected`)
    Selected,
    /// Maximum value for number inputs (`max="..."`)
    Max(AzString),
    /// Minimum value for number inputs (`min="..."`)
    Min(AzString),
    /// Step value for number inputs (`step="..."`)
    Step(AzString),
    /// Input pattern for validation (`pattern="..."`)
    Pattern(AzString),
    /// Minimum length (`minlength="..."`)
    MinLength(i32),
    /// Maximum length (`maxlength="..."`)
    MaxLength(i32),
    /// Autocomplete behavior (`autocomplete="on|off|..."`)
    Autocomplete(AzString),

    /// Table header scope (`scope="row|col|rowgroup|colgroup"`)
    Scope(AzString),
    /// Number of columns to span (`colspan="..."`)
    ColSpan(i32),
    /// Number of rows to span (`rowspan="..."`)
    RowSpan(i32),

    /// Tab index for keyboard navigation (`tabindex="..."`)
    TabIndex(i32),
    /// Element can receive focus (`tabindex="0"` equivalent)
    Focusable,

    /// Language code (`lang="..."`)
    Lang(AzString),
    /// Text direction (`dir="ltr|rtl|auto"`)
    Dir(AzString),

    /// Content is editable (`contenteditable="true|false"`)
    ContentEditable(bool),
    /// Element is draggable (`draggable="true|false"`)
    Draggable(bool),
    /// Element is hidden (`hidden`)
    Hidden,

    /// Generic data attribute (`data-*="..."`)
    Data(AttributeNameValue),
    /// Generic custom attribute (for future extensibility)
    Custom(AttributeNameValue),
}

impl_option!(
    AttributeType,
    OptionAttributeType,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(AttributeType, AttributeTypeVec, AttributeTypeVecDestructor, AttributeTypeVecDestructorType, AttributeTypeVecSlice, OptionAttributeType);
impl_vec_debug!(AttributeType, AttributeTypeVec);
impl_vec_partialord!(AttributeType, AttributeTypeVec);
impl_vec_ord!(AttributeType, AttributeTypeVec);
impl_vec_clone!(AttributeType, AttributeTypeVec, AttributeTypeVecDestructor);
impl_vec_partialeq!(AttributeType, AttributeTypeVec);
impl_vec_eq!(AttributeType, AttributeTypeVec);
impl_vec_hash!(AttributeType, AttributeTypeVec);

impl AttributeType {
    /// Returns the id string if this is an `Id` attribute, `None` otherwise.
    pub fn as_id(&self) -> Option<&str> {
        match self {
            AttributeType::Id(s) => Some(s.as_str()),
            _ => None,
        }
    }
    /// Returns the class string if this is a `Class` attribute, `None` otherwise.
    pub fn as_class(&self) -> Option<&str> {
        match self {
            AttributeType::Class(s) => Some(s.as_str()),
            _ => None,
        }
    }
    /// Get the attribute name (e.g., "href", "aria-label", "data-foo")
    pub fn name(&self) -> &str {
        match self {
            AttributeType::Id(_) => "id",
            AttributeType::Class(_) => "class",
            AttributeType::AriaLabel(_) => "aria-label",
            AttributeType::AriaLabelledBy(_) => "aria-labelledby",
            AttributeType::AriaDescribedBy(_) => "aria-describedby",
            AttributeType::AriaRole(_) => "role",
            AttributeType::AriaState(nv) => nv.attr_name.as_str(),
            AttributeType::AriaProperty(nv) => nv.attr_name.as_str(),
            AttributeType::Href(_) => "href",
            AttributeType::Rel(_) => "rel",
            AttributeType::Target(_) => "target",
            AttributeType::Src(_) => "src",
            AttributeType::Alt(_) => "alt",
            AttributeType::Title(_) => "title",
            AttributeType::Name(_) => "name",
            AttributeType::Value(_) => "value",
            AttributeType::InputType(_) => "type",
            AttributeType::Placeholder(_) => "placeholder",
            AttributeType::Required => "required",
            AttributeType::Disabled => "disabled",
            AttributeType::Readonly => "readonly",
            AttributeType::Checked => "checked",
            AttributeType::Selected => "selected",
            AttributeType::Max(_) => "max",
            AttributeType::Min(_) => "min",
            AttributeType::Step(_) => "step",
            AttributeType::Pattern(_) => "pattern",
            AttributeType::MinLength(_) => "minlength",
            AttributeType::MaxLength(_) => "maxlength",
            AttributeType::Autocomplete(_) => "autocomplete",
            AttributeType::Scope(_) => "scope",
            AttributeType::ColSpan(_) => "colspan",
            AttributeType::RowSpan(_) => "rowspan",
            AttributeType::TabIndex(_) => "tabindex",
            AttributeType::Focusable => "tabindex",
            AttributeType::Lang(_) => "lang",
            AttributeType::Dir(_) => "dir",
            AttributeType::ContentEditable(_) => "contenteditable",
            AttributeType::Draggable(_) => "draggable",
            AttributeType::Hidden => "hidden",
            AttributeType::Data(nv) => nv.attr_name.as_str(),
            AttributeType::Custom(nv) => nv.attr_name.as_str(),
        }
    }

    /// Get the attribute value as a string
    pub fn value(&self) -> AzString {
        match self {
            AttributeType::Id(v)
            | AttributeType::Class(v)
            | AttributeType::AriaLabel(v)
            | AttributeType::AriaLabelledBy(v)
            | AttributeType::AriaDescribedBy(v)
            | AttributeType::AriaRole(v)
            | AttributeType::Href(v)
            | AttributeType::Rel(v)
            | AttributeType::Target(v)
            | AttributeType::Src(v)
            | AttributeType::Alt(v)
            | AttributeType::Title(v)
            | AttributeType::Name(v)
            | AttributeType::Value(v)
            | AttributeType::InputType(v)
            | AttributeType::Placeholder(v)
            | AttributeType::Max(v)
            | AttributeType::Min(v)
            | AttributeType::Step(v)
            | AttributeType::Pattern(v)
            | AttributeType::Autocomplete(v)
            | AttributeType::Scope(v)
            | AttributeType::Lang(v)
            | AttributeType::Dir(v) => v.clone(),

            AttributeType::AriaState(nv)
            | AttributeType::AriaProperty(nv)
            | AttributeType::Data(nv)
            | AttributeType::Custom(nv) => nv.value.clone(),

            AttributeType::MinLength(n)
            | AttributeType::MaxLength(n)
            | AttributeType::ColSpan(n)
            | AttributeType::RowSpan(n)
            | AttributeType::TabIndex(n) => n.to_string().into(),

            AttributeType::Focusable => "0".into(),
            AttributeType::ContentEditable(b) | AttributeType::Draggable(b) => {
                if *b {
                    "true".into()
                } else {
                    "false".into()
                }
            }

            AttributeType::Required
            | AttributeType::Disabled
            | AttributeType::Readonly
            | AttributeType::Checked
            | AttributeType::Selected
            | AttributeType::Hidden => "".into(), // Boolean attributes
        }
    }

    /// Check if this is a boolean attribute (present = true, absent = false)
    pub fn is_boolean(&self) -> bool {
        matches!(
            self,
            AttributeType::Required
                | AttributeType::Disabled
                | AttributeType::Readonly
                | AttributeType::Checked
                | AttributeType::Selected
                | AttributeType::Hidden
        )
    }
}

/// Compact accessibility information for common use cases.
///
/// This is a lighter-weight alternative to `AccessibilityInfo` for cases where
/// only basic accessibility properties are needed. Developers must explicitly
/// pass `None` if they choose not to provide accessibility information.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct SmallAriaInfo {
    /// Accessible label/name
    pub label: OptionString,
    /// Element's role (button, link, etc.)
    pub role: OptionAccessibilityRole,
    /// Additional description
    pub description: OptionString,
}

impl_option!(
    SmallAriaInfo,
    OptionSmallAriaInfo,
    copy = false,
    [Debug, Clone, PartialEq, Eq, Hash]
);

impl SmallAriaInfo {
    pub fn label<S: Into<AzString>>(text: S) -> Self {
        Self {
            label: OptionString::Some(text.into()),
            role: OptionAccessibilityRole::None,
            description: OptionString::None,
        }
    }

    pub fn with_role(mut self, role: AccessibilityRole) -> Self {
        self.role = OptionAccessibilityRole::Some(role);
        self
    }

    pub fn with_description<S: Into<AzString>>(mut self, desc: S) -> Self {
        self.description = OptionString::Some(desc.into());
        self
    }

    /// Convert to full `AccessibilityInfo`
    pub fn to_full_info(&self) -> AccessibilityInfo {
        AccessibilityInfo {
            accessibility_name: self.label.clone(),
            accessibility_value: OptionString::None,
            role: match self.role {
                OptionAccessibilityRole::Some(r) => r,
                OptionAccessibilityRole::None => AccessibilityRole::Unknown,
            },
            states: Vec::new().into(),
            accelerator: OptionVirtualKeyCodeCombo::None,
            default_action: OptionString::None,
            supported_actions: Vec::new().into(),
            is_live_region: false,
            labelled_by: OptionDomNodeId::None,
            described_by: OptionDomNodeId::None,
        }
    }
}

/// Represents all data associated with a single DOM node, such as its type,
/// classes, IDs, callbacks, and inline styles.
#[repr(C)]
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeData {
    /// `div`, `p`, `img`, etc.
    pub node_type: NodeType,
    /// Strongly-typed HTML attributes (aria-*, href, alt, etc.)
    /// IDs and classes are now stored as `AttributeType::Id` and `AttributeType::Class` entries.
    pub attributes: AttributeTypeVec,
    /// Callbacks attached to this node:
    ///
    /// `On::MouseUp` -> `Callback(my_button_click_handler)`
    pub callbacks: CoreCallbackDataVec,
    /// Conditional CSS properties with dynamic selectors.
    /// These are evaluated at runtime based on OS, viewport, container, theme, and pseudo-state.
    /// Uses "last wins" semantics - properties are evaluated in order, last match wins.
    pub css_props: CssPropertyWithConditionsVec,
    /// Packed flags: tab_index + contenteditable + is_anonymous.
    pub flags: NodeFlags,
    /// Optional extra accessibility information about this DOM node (MSAA, AT-SPI, UA).
    /// 8 bytes (Option<Box<T>> is pointer-sized).
    pub accessibility: Option<Box<AccessibilityInfo>>,
    /// Stores "extra", not commonly used data of the node: clip-mask, menus, etc.
    ///
    /// SHOULD NOT EXPOSED IN THE API - necessary to retroactively add functionality
    /// to the node without breaking the ABI.
    extra: Option<Box<NodeDataExt>>,
}

impl_option!(
    NodeData,
    OptionNodeData,
    copy = false,
    [Debug, PartialEq, Eq, PartialOrd, Ord]
);

impl Hash for NodeData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node_type.hash(state);
        self.attributes.as_ref().hash(state);
        self.flags.hash(state);

        // NOTE: callbacks are NOT hashed regularly, otherwise
        // they'd cause inconsistencies because of the scroll callback
        for callback in self.callbacks.as_ref().iter() {
            callback.event.hash(state);
            callback.callback.hash(state);
            callback.refany.get_type_id().hash(state);
        }

        // Hash CSS props (conditional CSS with dynamic selectors)
        for prop in self.css_props.as_ref().iter() {
            // Hash property type as a simple discriminant
            core::mem::discriminant(&prop.property).hash(state);
        }
        if let Some(ext) = self.extra.as_ref() {
            if let Some(ds) = ext.dataset.as_ref() {
                ds.hash(state);
            }
            if let Some(c) = ext.clip_mask.as_ref() {
                c.hash(state);
            }
            if let Some(c) = ext.menu_bar.as_ref() {
                c.hash(state);
            }
            if let Some(c) = ext.context_menu.as_ref() {
                c.hash(state);
            }
            if let Some(vv) = ext.virtualized_view.as_ref() {
                vv.hash(state);
            }
        }
    }
}

/// Tracks which component rendered a DOM subtree.
///
/// When a component's `render_fn` returns a `StyledDom`, the framework stamps the
/// root node(s) of the output with a `ComponentOrigin`. This enables:
/// - The debugger to show a "Component Tree" alongside the DOM tree
/// - Code generation roundtrips (rendered DOM → component invocations → code)
/// - Clicking a DOM node to navigate to the component that produced it
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentOrigin {
    /// Qualified component name, e.g. "shadcn:card", "builtin:div"
    pub component_id: AzString,
    /// Snapshot of the data model at render time, stored as a JSON value.
    /// The debug server can inspect typed values; the frontend serializes
    /// them back to JSON for display and editing.
    pub data_model_json: crate::json::Json,
}

// Manual impls because Json contains f64 (no Eq/Ord/Hash derive),
// but we need them for NodeDataExt. We compare on the Display string.
impl Eq for ComponentOrigin {}

impl PartialOrd for ComponentOrigin {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ComponentOrigin {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.component_id.cmp(&other.component_id)
            .then_with(|| {
                let a = alloc::format!("{}", self.data_model_json);
                let b = alloc::format!("{}", other.data_model_json);
                a.cmp(&b)
            })
    }
}

impl core::hash::Hash for ComponentOrigin {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.component_id.hash(state);
        let s = alloc::format!("{}", self.data_model_json);
        s.hash(state);
    }
}

impl Default for ComponentOrigin {
    fn default() -> Self {
        Self {
            component_id: AzString::from_const_str(""),
            data_model_json: crate::json::Json::null(),
        }
    }
}

/// NOTE: NOT EXPOSED IN THE API! Stores extra,
/// not commonly used information for the NodeData.
/// This helps keep the primary `NodeData` struct smaller for common cases.
#[repr(C)]
#[derive(Debug, Default, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct NodeDataExt {
    /// VirtualizedView callback data, only set when node_type == NodeType::VirtualizedView.
    pub virtualized_view: Option<VirtualizedViewNode>,
    /// `data-*` attributes for this node, useful to store UI-related data on the node itself.
    pub dataset: Option<RefAny>,
    /// Optional clip mask for this DOM node.
    pub clip_mask: Option<ImageMask>,
    /// Menu bar that should be displayed at the top of this nodes rect.
    pub menu_bar: Option<Box<Menu>>,
    /// Context menu that should be opened when the item is left-clicked.
    pub context_menu: Option<Box<Menu>>,
    /// Stable key for reconciliation. If provided, allows the framework to track
    /// this node across frames even if its position in the array changes.
    /// This is crucial for correct lifecycle events when lists are reordered.
    pub key: Option<u64>,
    /// Callback to merge dataset state from a previous frame's node into the current node.
    /// This enables heavy resource preservation (video decoders, GL textures) across frames.
    pub dataset_merge_callback: Option<DatasetMergeCallback>,
    /// Tracks which component rendered this DOM subtree.
    /// Set by the framework during component rendering — the root node(s) of a
    /// component's output DOM get stamped with the component's qualified name.
    /// Enables the debugger to reconstruct the component invocation tree from the
    /// flat rendered DOM, and enables code generation roundtrips.
    pub component_origin: Option<ComponentOrigin>,
}

/// A callback function used to merge the state of an old dataset into a new one.
///
/// This enables components with heavy internal state (video players, WebGL contexts)
/// to preserve their resources across frames, while the DOM tree is recreated.
///
/// The callback receives both the old and new datasets as `RefAny` (cheap shallow clones)
/// and returns the dataset that should be used for the new node.
///
/// # Example
///
/// ```rust,ignore
/// fn merge_video_state(new_data: RefAny, old_data: RefAny) -> RefAny {
///     // Transfer heavy resources from old to new
///     if let (Some(mut new), Some(old)) = (
///         new_data.downcast_mut::<VideoState>(),
///         old_data.downcast_ref::<VideoState>()
///     ) {
///         new.decoder = old.decoder.take();
///         new.gl_texture = old.gl_texture.take();
///     }
///     new_data // Return the merged state
/// }
/// ```
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct DatasetMergeCallback {
    /// The function pointer that performs the merge.
    /// Signature: `fn(new_data: RefAny, old_data: RefAny) -> RefAny`
    pub cb: DatasetMergeCallbackType,
    /// Optional callable for FFI language bindings (Python, etc.)
    /// When set, the FFI layer can invoke this instead of `cb`.
    pub callable: OptionRefAny,
}

impl core::fmt::Debug for DatasetMergeCallback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DatasetMergeCallback")
            .field("cb", &(self.cb as usize))
            .field("callable", &self.callable)
            .finish()
    }
}

/// Allow creating DatasetMergeCallback from a raw function pointer.
/// This enables the `Into<DatasetMergeCallback>` pattern for Python bindings.
impl From<DatasetMergeCallbackType> for DatasetMergeCallback {
    fn from(cb: DatasetMergeCallbackType) -> Self {
        DatasetMergeCallback { 
            cb,
            callable: OptionRefAny::None,
        }
    }
}

impl_option!(
    DatasetMergeCallback,
    OptionDatasetMergeCallback,
    copy = false,
    [Debug, Clone]
);

/// Function pointer type for dataset merge callbacks.
/// 
/// Arguments:
/// - `new_data`: The new node's dataset (shallow clone, cheap)
/// - `old_data`: The old node's dataset (shallow clone, cheap)
/// 
/// Returns:
/// - The `RefAny` that should be used as the dataset for the new node
pub type DatasetMergeCallbackType = extern "C" fn(RefAny, RefAny) -> RefAny;

/// Holds information about a UI element for accessibility purposes (e.g., screen readers).
/// This is a wrapper for platform-specific accessibility APIs like MSAA.
#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct AccessibilityInfo {
    /// Get the "name" of the `IAccessible`, for example the
    /// name of a button, checkbox or menu item. Try to use unique names
    /// for each item in a dialog so that voice dictation software doesn't
    /// have to deal with extra ambiguity.
    pub accessibility_name: OptionString,
    /// Get the "value" of the `IAccessible`, for example a number in a slider,
    /// a URL for a link, the text a user entered in a field.
    pub accessibility_value: OptionString,
    /// Optional keyboard accelerator.
    pub accelerator: OptionVirtualKeyCodeCombo,
    /// Optional "default action" description. Only used when there is at least
    /// one `ComponentEventFilter::DefaultAction` callback present on this node.
    pub default_action: OptionString,
    /// Possible on/off states, such as focused, focusable, selected, selectable,
    /// visible, protected (for passwords), checked, etc.
    pub states: AccessibilityStateVec,
    /// A list of actions the user can perform on this element.
    /// Maps to accesskit's Action enum.
    pub supported_actions: AccessibilityActionVec,
    /// ID of another node that labels this one (for `aria-labelledby`).
    pub labelled_by: OptionDomNodeId,
    /// ID of another node that describes this one (for `aria-describedby`).
    pub described_by: OptionDomNodeId,
    /// Get an enumerated value representing what this IAccessible is used for,
    /// for example is it a link, static text, editable text, a checkbox, or a table cell, etc.
    pub role: AccessibilityRole,
    /// For live regions that update automatically (e.g., chat messages, timers).
    /// Maps to accesskit's `Live` property.
    pub is_live_region: bool,
}

/// Actions that can be performed on an accessible element.
/// This is a simplified version of accesskit::Action to avoid direct dependency in core.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum AccessibilityAction {
    /// The default action for the element (usually a click).
    Default,
    /// Set focus to this element.
    Focus,
    /// Remove focus from this element.
    Blur,
    /// Collapse an expandable element (e.g., tree node, accordion).
    Collapse,
    /// Expand a collapsible element (e.g., tree node, accordion).
    Expand,
    /// Scroll this element into view.
    ScrollIntoView,
    /// Increment a numeric value (e.g., slider, spinner).
    Increment,
    /// Decrement a numeric value (e.g., slider, spinner).
    Decrement,
    /// Show a context menu.
    ShowContextMenu,
    /// Hide a tooltip.
    HideTooltip,
    /// Show a tooltip.
    ShowTooltip,
    /// Scroll up.
    ScrollUp,
    /// Scroll down.
    ScrollDown,
    /// Scroll left.
    ScrollLeft,
    /// Scroll right.
    ScrollRight,
    /// Replace selected text with new text.
    ReplaceSelectedText(AzString),
    /// Scroll to a specific point.
    ScrollToPoint(LogicalPosition),
    /// Set scroll offset.
    SetScrollOffset(LogicalPosition),
    /// Set text selection.
    SetTextSelection(TextSelectionStartEnd),
    /// Set sequential focus navigation starting point.
    SetSequentialFocusNavigationStartingPoint,
    /// Set the value of a control.
    SetValue(AzString),
    /// Set numeric value of a control.
    SetNumericValue(FloatValue),
    /// Custom action with ID.
    CustomAction(i32),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct TextSelectionStartEnd {
    pub selection_start: usize,
    pub selection_end: usize,
}

impl_vec!(AccessibilityAction, AccessibilityActionVec, AccessibilityActionVecDestructor, AccessibilityActionVecDestructorType, AccessibilityActionVecSlice, OptionAccessibilityAction);
impl_vec_debug!(AccessibilityAction, AccessibilityActionVec);
impl_vec_clone!(
    AccessibilityAction,
    AccessibilityActionVec,
    AccessibilityActionVecDestructor
);
impl_vec_partialeq!(AccessibilityAction, AccessibilityActionVec);
impl_vec_eq!(AccessibilityAction, AccessibilityActionVec);
impl_vec_partialord!(AccessibilityAction, AccessibilityActionVec);
impl_vec_ord!(AccessibilityAction, AccessibilityActionVec);
impl_vec_hash!(AccessibilityAction, AccessibilityActionVec);

impl_option![
    AccessibilityAction,
    OptionAccessibilityAction,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
];

impl_option!(
    AccessibilityInfo,
    OptionAccessibilityInfo,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Defines the element's purpose for accessibility APIs, informing assistive technologies
/// like screen readers about the function of a UI element. Each variant corresponds to a
/// standard control type or UI structure.
///
/// For more details, see the [MSDN Role Constants page](https://docs.microsoft.com/en-us/windows/winauto/object-roles).
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum AccessibilityRole {
    /// Represents the title or caption bar of a window.
    /// - **Purpose**: To identify the title bar containing the window title and system commands.
    /// - **When to use**: This role is typically inserted by the operating system for standard
    ///   windows.
    /// - **Example**: The bar at the top of an application window displaying its name and the
    ///   minimize, maximize, and close buttons.
    TitleBar,

    /// Represents a menu bar at the top of a window.
    /// - **Purpose**: To contain a set of top-level menus for an application.
    /// - **When to use**: For the main menu bar of an application, such as one containing "File,"
    ///   "Edit," and "View."
    /// - **Example**: The "File", "Edit", "View" menu bar at the top of a text editor.
    MenuBar,

    /// Represents a vertical or horizontal scroll bar.
    /// - **Purpose**: To enable scrolling through content that is larger than the visible area.
    /// - **When to use**: For any scrollable region of content.
    /// - **Example**: The bar on the side of a web page that allows the user to scroll up and
    ///   down.
    ScrollBar,

    /// Represents a handle or grip used for moving or resizing.
    /// - **Purpose**: To provide a user interface element for manipulating another element's size
    ///   or position.
    /// - **When to use**: For handles that allow resizing of windows, panes, or other objects.
    /// - **Example**: The small textured area in the bottom-right corner of a window that can be
    ///   dragged to resize it.
    Grip,

    /// Represents a system sound indicating an event.
    /// - **Purpose**: To associate a sound with a UI event, providing an auditory cue.
    /// - **When to use**: When a sound is the primary representation of an event.
    /// - **Example**: A system notification sound that plays when a new message arrives.
    Sound,

    /// Represents the system's mouse pointer or other pointing device.
    /// - **Purpose**: To indicate the screen position of the user's pointing device.
    /// - **When to use**: This role is managed by the operating system.
    /// - **Example**: The arrow that moves on the screen as you move the mouse.
    Cursor,

    /// Represents the text insertion point indicator.
    /// - **Purpose**: To show the current text entry or editing position.
    /// - **When to use**: This role is typically managed by the operating system for text input
    ///   fields.
    /// - **Example**: The blinking vertical line in a text box that shows where the next character
    ///   will be typed.
    Caret,

    /// Represents an alert or notification.
    /// - **Purpose**: To convey an important, non-modal message to the user.
    /// - **When to use**: For non-intrusive notifications that do not require immediate user
    ///   interaction.
    /// - **Example**: A small, temporary "toast" notification that appears to confirm an action,
    ///   like "Email sent."
    Alert,

    /// Represents a window frame.
    /// - **Purpose**: To serve as the container for other objects like a title bar and client
    ///   area.
    /// - **When to use**: This is a fundamental role, typically managed by the windowing system.
    /// - **Example**: The main window of any application, which contains all other UI elements.
    Window,

    /// Represents a window's client area, where the main content is displayed.
    /// - **Purpose**: To define the primary content area of a window.
    /// - **When to use**: For the main content region of a window. It's often the default role for
    ///   a custom control container.
    /// - **Example**: The area of a web browser where the web page content is rendered.
    Client,

    /// Represents a pop-up menu.
    /// - **Purpose**: To display a list of `MenuItem` objects that appears when a user performs an
    ///   action.
    /// - **When to use**: For context menus (right-click menus) or drop-down menus.
    /// - **Example**: The menu that appears when you right-click on a file in a file explorer.
    MenuPopup,

    /// Represents an individual item within a menu.
    /// - **Purpose**: To represent a single command, option, or separator within a menu.
    /// - **When to use**: For individual options inside a `MenuBar` or `MenuPopup`.
    /// - **Example**: The "Save" option within the "File" menu.
    MenuItem,

    /// Represents a small pop-up window that provides information.
    /// - **Purpose**: To offer brief, contextual help or information about a UI element.
    /// - **When to use**: For informational pop-ups that appear on mouse hover.
    /// - **Example**: The small box of text that appears when you hover over a button in a
    ///   toolbar.
    Tooltip,

    /// Represents the main window of an application.
    /// - **Purpose**: To identify the top-level window of an application.
    /// - **When to use**: For the primary window that represents the application itself.
    /// - **Example**: The main window of a calculator or notepad application.
    Application,

    /// Represents a document window within an application.
    /// - **Purpose**: To represent a contained document, typically in a Multiple Document
    ///   Interface (MDI) application.
    /// - **When to use**: For individual document windows inside a larger application shell.
    /// - **Example**: In a photo editor that allows multiple images to be open in separate
    ///   windows, each image window would be a `Document`.
    Document,

    /// Represents a pane or a distinct section of a window.
    /// - **Purpose**: To divide a window into visually and functionally distinct areas.
    /// - **When to use**: For sub-regions of a window, like a navigation pane, preview pane, or
    ///   sidebar.
    /// - **Example**: The preview pane in an email client that shows the content of the selected
    ///   email.
    Pane,

    /// Represents a graphical chart or graph.
    /// - **Purpose**: To display data visually in a chart format.
    /// - **When to use**: For any type of chart, such as a bar chart, line chart, or pie chart.
    /// - **Example**: A bar chart displaying monthly sales figures.
    Chart,

    /// Represents a dialog box or message box.
    /// - **Purpose**: To create a secondary window that requires user interaction before returning
    ///   to the main application.
    /// - **When to use**: For modal or non-modal windows that prompt the user for information or a
    ///   response.
    /// - **Example**: The "Open File" or "Print" dialog in most applications.
    Dialog,

    /// Represents a window's border.
    /// - **Purpose**: To identify the border of a window, which is often used for resizing.
    /// - **When to use**: This role is typically managed by the windowing system.
    /// - **Example**: The decorative and functional frame around a window.
    Border,

    /// Represents a group of related controls.
    /// - **Purpose**: To logically group other objects that share a common purpose.
    /// - **When to use**: For grouping controls like a set of radio buttons or a fieldset with a
    ///   legend.
    /// - **Example**: A "Settings" group box in a dialog that contains several related checkboxes.
    Grouping,

    /// Represents a visual separator.
    /// - **Purpose**: To visually divide a space or a group of controls.
    /// - **When to use**: For visual separators in menus, toolbars, or between panes.
    /// - **Example**: The horizontal line in a menu that separates groups of related menu items.
    Separator,

    /// Represents a toolbar containing a group of controls.
    /// - **Purpose**: To group controls, typically buttons, for quick access to frequently used
    ///   functions.
    /// - **When to use**: For a bar of buttons or other controls, usually at the top of a window
    ///   or pane.
    /// - **Example**: The toolbar at the top of a word processor with buttons for "Bold,"
    ///   "Italic," and "Underline."
    Toolbar,

    /// Represents a status bar for displaying information.
    /// - **Purpose**: To display status information about the current state of the application.
    /// - **When to use**: For a bar, typically at the bottom of a window, that displays messages.
    /// - **Example**: The bar at the bottom of a web browser that shows the loading status of a
    ///   page.
    StatusBar,

    /// Represents a data table.
    /// - **Purpose**: To present data in a two-dimensional grid of rows and columns.
    /// - **When to use**: For grid-like data presentation.
    /// - **Example**: A spreadsheet or a table of data in a database application.
    Table,

    /// Represents a column header in a table.
    /// - **Purpose**: To provide a label for a column of data.
    /// - **When to use**: For the headers of columns in a `Table`.
    /// - **Example**: The header row in a spreadsheet with labels like "Name," "Date," and
    ///   "Amount."
    ColumnHeader,

    /// Represents a row header in a table.
    /// - **Purpose**: To provide a label for a row of data.
    /// - **When to use**: For the headers of rows in a `Table`.
    /// - **Example**: The numbered rows on the left side of a spreadsheet.
    RowHeader,

    /// Represents a full column of cells in a table.
    /// - **Purpose**: To represent an entire column as a single accessible object.
    /// - **When to use**: When it is useful to interact with a column as a whole.
    /// - **Example**: The "Amount" column in a financial data table.
    Column,

    /// Represents a full row of cells in a table.
    /// - **Purpose**: To represent an entire row as a single accessible object.
    /// - **When to use**: When it is useful to interact with a row as a whole.
    /// - **Example**: A row representing a single customer's information in a customer list.
    Row,

    /// Represents a single cell within a table.
    /// - **Purpose**: To represent a single data point or control within a `Table`.
    /// - **When to use**: For individual cells in a grid or table.
    /// - **Example**: A single cell in a spreadsheet containing a specific value.
    Cell,

    /// Represents a hyperlink to a resource.
    /// - **Purpose**: To provide a navigational link to another document or location.
    /// - **When to use**: For text or images that, when clicked, navigate to another resource.
    /// - **Example**: A clickable link on a web page.
    Link,

    /// Represents a help balloon or pop-up.
    /// - **Purpose**: To provide more detailed help information than a standard tooltip.
    /// - **When to use**: For a pop-up that offers extended help text, often initiated by a help
    ///   button.
    /// - **Example**: A pop-up balloon with a paragraph of help text that appears when a user
    ///   clicks a help icon.
    HelpBalloon,

    /// Represents an animated, character-like graphic object.
    /// - **Purpose**: To provide an animated agent for user assistance or entertainment.
    /// - **When to use**: For animated characters or avatars that provide help or guidance.
    /// - **Example**: An animated paperclip that offers tips in a word processor (e.g.,
    ///   Microsoft's Clippy).
    Character,

    /// Represents a list of items.
    /// - **Purpose**: To contain a set of `ListItem` objects.
    /// - **When to use**: For list boxes or similar controls that present a list of selectable
    ///   items.
    /// - **Example**: The list of files in a file selection dialog.
    List,

    /// Represents an individual item within a list.
    /// - **Purpose**: To represent a single, selectable item within a `List`.
    /// - **When to use**: For each individual item in a list box or combo box.
    /// - **Example**: A single file name in a list of files.
    ListItem,

    /// Represents an outline or tree structure.
    /// - **Purpose**: To display a hierarchical view of data.
    /// - **When to use**: For tree-view controls that show nested items.
    /// - **Example**: A file explorer's folder tree view.
    Outline,

    /// Represents an individual item within an outline or tree.
    /// - **Purpose**: To represent a single node (which can be a leaf or a branch) in an
    ///   `Outline`.
    /// - **When to use**: For each node in a tree view.
    /// - **Example**: A single folder in a file explorer's tree view.
    OutlineItem,

    /// Represents a single tab in a tabbed interface.
    /// - **Purpose**: To provide a control for switching between different `PropertyPage` views.
    /// - **When to use**: For the individual tabs that the user can click to switch pages.
    /// - **Example**: The "General" and "Security" tabs in a file properties dialog.
    PageTab,

    /// Represents the content of a page in a property sheet.
    /// - **Purpose**: To serve as a container for the controls displayed when a `PageTab` is
    ///   selected.
    /// - **When to use**: For the content area associated with a specific tab.
    /// - **Example**: The set of options displayed when the "Security" tab is active.
    PropertyPage,

    /// Represents a visual indicator, like a slider thumb.
    /// - **Purpose**: To visually indicate the current value or position of another control.
    /// - **When to use**: For a sub-element that indicates status, like the thumb of a scrollbar.
    /// - **Example**: The draggable thumb of a scrollbar that indicates the current scroll
    ///   position.
    Indicator,

    /// Represents a picture or graphical image.
    /// - **Purpose**: To display a non-interactive image.
    /// - **When to use**: For images and icons that are purely decorative or informational.
    /// - **Example**: A company logo displayed in an application's "About" dialog.
    Graphic,

    /// Represents read-only text.
    /// - **Purpose**: To provide a non-editable text label for another control or for displaying
    ///   information.
    /// - **When to use**: For text that the user cannot edit.
    /// - **Example**: The label "Username:" next to a text input field.
    StaticText,

    /// Represents editable text or a text area.
    /// - **Purpose**: To allow for user text input or selection.
    /// - **When to use**: For text input fields where the user can type.
    /// - **Example**: A text box for entering a username or password.
    Text,

    /// Represents a standard push button.
    /// - **Purpose**: To initiate an immediate action.
    /// - **When to use**: For standard buttons that perform an action when clicked.
    /// - **Example**: An "OK" or "Cancel" button in a dialog.
    PushButton,

    /// Represents a check box control.
    /// - **Purpose**: To allow the user to make a binary choice (checked or unchecked).
    /// - **When to use**: For options that can be toggled on or off independently.
    /// - **Example**: A "Remember me" checkbox on a login form.
    CheckButton,

    /// Represents a radio button.
    /// - **Purpose**: To allow the user to select one option from a mutually exclusive group.
    /// - **When to use**: For a choice where only one option from a `Grouping` can be selected.
    /// - **Example**: "Male" and "Female" radio buttons for selecting gender.
    RadioButton,

    /// Represents a combination of a text field and a drop-down list.
    /// - **Purpose**: To allow the user to either type a value or select one from a list.
    /// - **When to use**: For controls that offer a list of suggestions but also allow custom
    ///   input.
    /// - **Example**: A font selector that allows you to type a font name or choose one from a
    ///   list.
    ComboBox,

    /// Represents a drop-down list box.
    /// - **Purpose**: To allow the user to select an item from a non-editable list that drops
    ///   down.
    /// - **When to use**: For selecting a single item from a predefined list of options.
    /// - **Example**: A country selection drop-down menu.
    DropList,

    /// Represents a progress bar.
    /// - **Purpose**: To indicate the progress of a lengthy operation.
    /// - **When to use**: To provide feedback for tasks like file downloads or installations.
    /// - **Example**: The bar that fills up to show the progress of a file copy operation.
    ProgressBar,

    /// Represents a dial or knob.
    /// - **Purpose**: To allow selecting a value from a continuous or discrete range, often
    ///   circularly.
    /// - **When to use**: For controls that resemble real-world dials, like a volume knob.
    /// - **Example**: A volume control knob in a media player application.
    Dial,

    /// Represents a control for entering a keyboard shortcut.
    /// - **Purpose**: To capture a key combination from the user.
    /// - **When to use**: In settings where users can define their own keyboard shortcuts.
    /// - **Example**: A text field in a settings dialog where a user can press a key combination
    ///   to assign it to a command.
    HotkeyField,

    /// Represents a slider for selecting a value within a range.
    /// - **Purpose**: To allow the user to adjust a setting along a continuous or discrete range.
    /// - **When to use**: For adjusting values like volume, brightness, or zoom level.
    /// - **Example**: A slider to control the volume of a video.
    Slider,

    /// Represents a spin button (up/down arrows) for incrementing or decrementing a value.
    /// - **Purpose**: To provide fine-tuned adjustment of a value, typically numeric.
    /// - **When to use**: For controls that allow stepping through a range of values.
    /// - **Example**: The up and down arrows next to a number input for setting the font size.
    SpinButton,

    /// Represents a diagram or flowchart.
    /// - **Purpose**: To represent data or relationships in a schematic form.
    /// - **When to use**: For visual representations of structures that are not charts, like a
    ///   database schema diagram.
    /// - **Example**: A flowchart illustrating a business process.
    Diagram,

    /// Represents an animation control.
    /// - **Purpose**: To display a sequence of images or indicate an ongoing process.
    /// - **When to use**: For animations that show that an operation is in progress.
    /// - **Example**: The animation that plays while files are being copied.
    Animation,

    /// Represents a mathematical equation.
    /// - **Purpose**: To display a mathematical formula in the correct format.
    /// - **When to use**: For displaying mathematical equations.
    /// - **Example**: A rendered mathematical equation in a scientific document editor.
    Equation,

    /// Represents a button that drops down a list of items.
    /// - **Purpose**: To combine a default action button with a list of alternative actions.
    /// - **When to use**: For buttons that have a primary action and a secondary list of options.
    /// - **Example**: A "Send" button with a dropdown arrow that reveals "Send and Archive."
    ButtonDropdown,

    /// Represents a button that drops down a full menu.
    /// - **Purpose**: To provide a button that opens a menu of choices rather than performing a
    ///   single action.
    /// - **When to use**: When a button's primary purpose is to reveal a menu.
    /// - **Example**: A "Tools" button that opens a menu with various tool options.
    ButtonMenu,

    /// Represents a button that drops down a grid for selection.
    /// - **Purpose**: To allow selection from a two-dimensional grid of options.
    /// - **When to use**: For buttons that open a grid-based selection UI.
    /// - **Example**: A color picker button that opens a grid of color swatches.
    ButtonDropdownGrid,

    /// Represents blank space between other objects.
    /// - **Purpose**: To represent significant empty areas in a UI that are part of the layout.
    /// - **When to use**: Sparingly, to signify that a large area is intentionally blank.
    /// - **Example**: A large empty panel in a complex layout might use this role.
    Whitespace,

    /// Represents the container for a set of tabs.
    /// - **Purpose**: To group a set of `PageTab` elements.
    /// - **When to use**: To act as the parent container for a row or column of tabs.
    /// - **Example**: The entire row of tabs at the top of a properties dialog.
    PageTabList,

    /// Represents a clock control.
    /// - **Purpose**: To display the current time.
    /// - **When to use**: For any UI element that displays time.
    /// - **Example**: The clock in the system tray of the operating system.
    Clock,

    /// Represents a button with two parts: a default action and a dropdown.
    /// - **Purpose**: To combine a frequently used action with a set of related, less-used
    ///   actions.
    /// - **When to use**: When a button has a default action and other related actions available
    ///   in a dropdown.
    /// - **Example**: A "Save" split button where the primary part saves, and the dropdown offers
    ///   "Save As."
    SplitButton,

    /// Represents a control for entering an IP address.
    /// - **Purpose**: To provide a specialized input field for IP addresses, often with formatting
    ///   and validation.
    /// - **When to use**: For dedicated IP address input fields.
    /// - **Example**: A network configuration dialog with a field for entering a static IP
    ///   address.
    IpAddress,

    /// Represents an element with no specific role.
    /// - **Purpose**: To indicate an element that has no semantic meaning for accessibility.
    /// - **When to use**: Should be used sparingly for purely decorative elements that should be
    ///   ignored by assistive technologies.
    /// - **Example**: A decorative graphical flourish that has no function or information to
    ///   convey.
    Nothing,

    /// Unknown or unspecified role.
    /// - **Purpose**: Default fallback when no specific role is assigned.
    /// - **When to use**: As a default value or when role information is unavailable.
    Unknown,
}

impl_option!(
    AccessibilityRole,
    OptionAccessibilityRole,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Defines the current state of an element for accessibility APIs (e.g., focused, checked).
/// These states provide dynamic information to assistive technologies about the element's
/// condition.
///
/// See the [MSDN State Constants page](https://docs.microsoft.com/en-us/windows/win32/winauto/object-state-constants) for more details.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum AccessibilityState {
    /// The element is unavailable and cannot be interacted with.
    /// - **Purpose**: To indicate that a control is disabled or grayed out.
    /// - **When to use**: For disabled buttons, non-interactive menu items, or any control that is
    ///   temporarily non-functional.
    /// - **Example**: A "Save" button that is disabled until the user makes changes to a document.
    Unavailable,

    /// The element is selected.
    /// - **Purpose**: To indicate that an item is currently chosen or highlighted. This is
    ///   distinct from having focus.
    /// - **When to use**: For selected items in a list, highlighted text, or the currently active
    ///   tab in a tab list.
    /// - **Example**: A file highlighted in a file explorer, or multiple selected emails in an
    ///   inbox.
    Selected,

    /// The element has the keyboard focus.
    /// - **Purpose**: To identify the single element that will receive keyboard input.
    /// - **When to use**: For the control that is currently active and ready to be manipulated by
    ///   the keyboard.
    /// - **Example**: A text box with a blinking cursor, or a button with a dotted outline around
    ///   it.
    Focused,

    /// The element is checked, toggled, or in a mixed state.
    /// - **Purpose**: To represent the state of controls like checkboxes, radio buttons, and
    ///   toggle buttons.
    /// - **When to use**: For checkboxes that are ticked, selected radio buttons, or toggle
    ///   buttons that are "on."
    /// - **Example**: A checked "I agree" checkbox, a selected "Yes" radio button, or an active
    ///   "Bold" button in a toolbar.
    Checked,

    /// The element's content cannot be edited by the user.
    /// - **Purpose**: To indicate that the element's value can be viewed and copied, but not
    ///   modified.
    /// - **When to use**: For display-only text fields or documents.
    /// - **Example**: A text box displaying a license agreement that the user can scroll through
    ///   but cannot edit.
    Readonly,

    /// The element is the default action in a dialog or form.
    /// - **Purpose**: To identify the button that will be activated if the user presses the Enter
    ///   key.
    /// - **When to use**: For the primary confirmation button in a dialog.
    /// - **Example**: The "OK" button in a dialog box, which often has a thicker or colored
    ///   border.
    Default,

    /// The element is expanded, showing its child items.
    /// - **Purpose**: To indicate that a collapsible element is currently open and its contents
    ///   are visible.
    /// - **When to use**: For tree view nodes, combo boxes with their lists open, or expanded
    ///   accordion panels.
    /// - **Example**: A folder in a file explorer's tree view that has been clicked to show its
    ///   subfolders.
    Expanded,

    /// The element is collapsed, hiding its child items.
    /// - **Purpose**: To indicate that a collapsible element is closed and its contents are
    ///   hidden.
    /// - **When to use**: The counterpart to `Expanded` for any collapsible UI element.
    /// - **Example**: A closed folder in a file explorer's tree view, hiding its contents.
    Collapsed,

    /// The element is busy and cannot respond to user interaction.
    /// - **Purpose**: To indicate that the element or application is performing an operation and
    ///   is temporarily unresponsive.
    /// - **When to use**: When an application is loading, processing refany, or otherwise occupied.
    /// - **Example**: A window that is grayed out and shows a spinning cursor while saving a large
    ///   file.
    Busy,

    /// The element is not currently visible on the screen.
    /// - **Purpose**: To indicate that an element exists but is currently scrolled out of the
    ///   visible area.
    /// - **When to use**: For items in a long list or a large document that are not within the
    ///   current viewport.
    /// - **Example**: A list item in a long dropdown that you would have to scroll down to see.
    Offscreen,

    /// The element can accept keyboard focus.
    /// - **Purpose**: To indicate that the user can navigate to this element using the keyboard
    ///   (e.g., with the Tab key).
    /// - **When to use**: On all interactive elements like buttons, links, and input fields,
    ///   whether they currently have focus or not.
    /// - **Example**: A button that can receive focus, even if it is not the currently focused
    ///   element.
    Focusable,

    /// The element is a container whose children can be selected.
    /// - **Purpose**: To indicate that the element contains items that can be chosen.
    /// - **When to use**: On container controls like list boxes, tree views, or text spans where
    ///   text can be highlighted.
    /// - **Example**: A list box control is `Selectable`, while its individual list items have the
    ///   `Selected` state when chosen.
    Selectable,

    /// The element is a hyperlink.
    /// - **Purpose**: To identify an object that navigates to another resource or location when
    ///   activated.
    /// - **When to use**: On any object that functions as a hyperlink.
    /// - **Example**: Text or an image that, when clicked, opens a web page.
    Linked,

    /// The element is a hyperlink that has been visited.
    /// - **Purpose**: To indicate that a hyperlink has already been followed by the user.
    /// - **When to use**: On a `Linked` object that the user has previously activated.
    /// - **Example**: A hyperlink on a web page that has changed color to show it has been
    ///   visited.
    Traversed,

    /// The element allows multiple of its children to be selected at once.
    /// - **Purpose**: To indicate that a container control supports multi-selection.
    /// - **When to use**: On container controls like list boxes or file explorers that support
    ///   multiple selections (e.g., with Ctrl-click).
    /// - **Example**: A file list that allows the user to select several files at once for a copy
    ///   operation.
    Multiselectable,

    /// The element contains protected content that should not be read aloud.
    /// - **Purpose**: To prevent assistive technologies from speaking the content of a sensitive
    ///   field.
    /// - **When to use**: Primarily for password input fields.
    /// - **Example**: A password text box where typed characters are masked with asterisks or
    ///   dots.
    Protected,
}

impl_option!(
    AccessibilityState,
    OptionAccessibilityState,
    [Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash]
);

impl_vec!(AccessibilityState, AccessibilityStateVec, AccessibilityStateVecDestructor, AccessibilityStateVecDestructorType, AccessibilityStateVecSlice, OptionAccessibilityState);
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
            attributes: self.attributes.clone(),
            css_props: self.css_props.clone(),
            callbacks: self.callbacks.clone(),
            flags: self.flags,
            accessibility: self.accessibility.clone(),
            extra: self.extra.clone(),
        }
    }
}

// Clone, PartialEq, Eq, Hash, PartialOrd, Ord
impl_vec!(NodeData, NodeDataVec, NodeDataVecDestructor, NodeDataVecDestructorType, NodeDataVecSlice, OptionNodeData);
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

/// Determines the behavior of an element in sequential focus navigation
// (e.g., using the Tab key).
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
    /// When pressing tab repeatedly, the focusing order will be
    /// determined by OverrideInParent elements taking precedence among global order.
    OverrideInParent(u32),
    /// Elements can be focused in callbacks, but are not accessible via
    /// keyboard / tab navigation (-1).
    NoKeyboardFocus,
}

impl_option!(
    TabIndex,
    OptionTabIndex,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl TabIndex {
    /// Returns the HTML-compatible number of the `tabindex` element.
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

/// Packed representation of tab index + contenteditable flag.
///
/// Bit layout (32 bits):
///   [31]     contenteditable flag (1 = true)
///   [30:29]  tab_index variant:
///              00 = None (no tab index set)
///              01 = Auto
///              10 = OverrideInParent (value in bits [28:0])
///              11 = NoKeyboardFocus
///   [28]     is_anonymous (1 = anonymous box for table layout)
///   [27:0]   OverrideInParent value (max ~268 million)
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeFlags {
    pub inner: u32,
}

impl Default for NodeFlags {
    fn default() -> Self {
        NodeFlags { inner: 0 }
    }
}

impl NodeFlags {
    const CONTENTEDITABLE_BIT: u32 = 1 << 31;
    const TAB_INDEX_MASK: u32      = 0b11 << 29;
    const ANONYMOUS_BIT: u32       = 1 << 28;
    const TAB_VALUE_MASK: u32      = (1 << 28) - 1;

    const TAB_NONE: u32            = 0b00 << 29;
    const TAB_AUTO: u32            = 0b01 << 29;
    const TAB_OVERRIDE: u32        = 0b10 << 29;
    const TAB_NO_KEYBOARD: u32     = 0b11 << 29;

    pub const fn new() -> Self {
        NodeFlags { inner: 0 }
    }

    pub const fn is_contenteditable(&self) -> bool {
        (self.inner & Self::CONTENTEDITABLE_BIT) != 0
    }

    pub const fn set_contenteditable(mut self, v: bool) -> Self {
        if v {
            self.inner |= Self::CONTENTEDITABLE_BIT;
        } else {
            self.inner &= !Self::CONTENTEDITABLE_BIT;
        }
        self
    }

    pub fn set_contenteditable_mut(&mut self, v: bool) {
        if v {
            self.inner |= Self::CONTENTEDITABLE_BIT;
        } else {
            self.inner &= !Self::CONTENTEDITABLE_BIT;
        }
    }

    pub fn get_tab_index(&self) -> Option<TabIndex> {
        match self.inner & Self::TAB_INDEX_MASK {
            x if x == Self::TAB_NONE => None,
            x if x == Self::TAB_AUTO => Some(TabIndex::Auto),
            x if x == Self::TAB_OVERRIDE => {
                let val = self.inner & Self::TAB_VALUE_MASK;
                Some(TabIndex::OverrideInParent(val))
            }
            x if x == Self::TAB_NO_KEYBOARD => Some(TabIndex::NoKeyboardFocus),
            _ => None,
        }
    }

    /// Returns whether this node is an anonymous box generated for table layout.
    pub const fn is_anonymous(&self) -> bool {
        (self.inner & Self::ANONYMOUS_BIT) != 0
    }

    pub fn set_anonymous(&mut self, v: bool) {
        if v {
            self.inner |= Self::ANONYMOUS_BIT;
        } else {
            self.inner &= !Self::ANONYMOUS_BIT;
        }
    }

    pub fn set_tab_index(&mut self, tab_index: Option<TabIndex>) {
        // Clear tab index bits (bits 29-30) and value bits (bits 0-27)
        // keep contenteditable bit (31) and anonymous bit (28)
        self.inner &= Self::CONTENTEDITABLE_BIT | Self::ANONYMOUS_BIT;
        match tab_index {
            None => { /* TAB_NONE = 0, already cleared */ }
            Some(TabIndex::Auto) => {
                self.inner |= Self::TAB_AUTO;
            }
            Some(TabIndex::OverrideInParent(val)) => {
                self.inner |= Self::TAB_OVERRIDE | (val & Self::TAB_VALUE_MASK);
            }
            Some(TabIndex::NoKeyboardFocus) => {
                self.inner |= Self::TAB_NO_KEYBOARD;
            }
        }
    }
}

impl Default for NodeData {
    fn default() -> Self {
        NodeData::create_node(NodeType::Div)
    }
}

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
        .attributes
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
        .attributes
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
    /// Creates a new `NodeData` instance from a given `NodeType`.
    #[inline]
    pub const fn create_node(node_type: NodeType) -> Self {
        Self {
            node_type,
            attributes: AttributeTypeVec::from_const_slice(&[]),
            callbacks: CoreCallbackDataVec::from_const_slice(&[]),
            css_props: CssPropertyWithConditionsVec::from_const_slice(&[]),
            flags: NodeFlags::new(),
            accessibility: None,
            extra: None,
        }
    }

    /// Shorthand for `NodeData::create_node(NodeType::Body)`.
    #[inline(always)]
    pub const fn create_body() -> Self {
        Self::create_node(NodeType::Body)
    }

    /// Shorthand for `NodeData::create_node(NodeType::Div)`.
    #[inline(always)]
    pub const fn create_div() -> Self {
        Self::create_node(NodeType::Div)
    }

    /// Shorthand for `NodeData::create_node(NodeType::Br)`.
    #[inline(always)]
    pub const fn create_br() -> Self {
        Self::create_node(NodeType::Br)
    }

    /// Shorthand for `NodeData::create_node(NodeType::Text(value.into()))`.
    #[inline(always)]
    pub fn create_text<S: Into<AzString>>(value: S) -> Self {
        Self::create_node(NodeType::Text(value.into()))
    }

    /// Shorthand for `NodeData::create_node(NodeType::Image(image_id))`.
    #[inline(always)]
    pub fn create_image(image: ImageRef) -> Self {
        Self::create_node(NodeType::Image(image))
    }

    #[inline(always)]
    pub fn create_virtualized_view(data: RefAny, callback: impl Into<VirtualizedViewCallback>) -> Self {
        let mut nd = Self::create_node(NodeType::VirtualizedView);
        let ext = nd.extra.get_or_insert_with(|| Box::new(NodeDataExt::default()));
        ext.virtualized_view = Some(VirtualizedViewNode {
            callback: callback.into(),
            refany: data,
        });
        nd
    }

    /// Checks whether this node is of the given node type (div, image, text).
    #[inline]
    pub fn is_node_type(&self, searched_type: NodeType) -> bool {
        self.node_type == searched_type
    }

    /// Checks whether this node has the searched ID attached.
    pub fn has_id(&self, id: &str) -> bool {
        self.attributes
            .iter()
            .any(|attr| attr.as_id() == Some(id))
    }

    /// Checks whether this node has the searched class attached.
    pub fn has_class(&self, class: &str) -> bool {
        self.attributes
            .iter()
            .any(|attr| attr.as_class() == Some(class))
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

    pub fn is_virtualized_view_node(&self) -> bool {
        matches!(self.node_type, NodeType::VirtualizedView)
    }

    // NOTE: Getters are used here in order to allow changing the memory allocator for the NodeData
    // in the future (which is why the fields are all private).

    #[inline(always)]
    pub const fn get_node_type(&self) -> &NodeType {
        &self.node_type
    }
    #[inline]
    pub fn get_dataset_mut(&mut self) -> Option<&mut RefAny> {
        self.extra.as_mut().and_then(|e| e.dataset.as_mut())
    }
    #[inline]
    pub fn get_dataset(&self) -> Option<&RefAny> {
        self.extra.as_ref().and_then(|e| e.dataset.as_ref())
    }
    /// Take the dataset out of the node, replacing it with None.
    pub fn take_dataset(&mut self) -> Option<RefAny> {
        self.extra.as_mut().and_then(|e| e.dataset.take())
    }
    /// Returns IDs and classes as a computed `IdOrClassVec`.
    /// Note: this allocates a new vec each time, prefer `has_id()`/`has_class()` for checks.
    #[inline]
    pub fn get_ids_and_classes(&self) -> IdOrClassVec {
        let v: Vec<IdOrClass> = self.attributes.as_ref().iter().filter_map(|attr| {
            match attr {
                AttributeType::Id(s) => Some(IdOrClass::Id(s.clone())),
                AttributeType::Class(s) => Some(IdOrClass::Class(s.clone())),
                _ => None,
            }
        }).collect();
        v.into()
    }
    #[inline(always)]
    pub const fn get_callbacks(&self) -> &CoreCallbackDataVec {
        &self.callbacks
    }
    #[inline(always)]
    pub const fn get_css_props(&self) -> &CssPropertyWithConditionsVec {
        &self.css_props
    }

    #[inline]
    pub fn get_clip_mask(&self) -> Option<&ImageMask> {
        self.extra.as_ref().and_then(|e| e.clip_mask.as_ref())
    }
    #[inline]
    pub fn get_tab_index(&self) -> Option<TabIndex> {
        self.flags.get_tab_index()
    }
    #[inline]
    pub fn get_accessibility_info(&self) -> Option<&Box<AccessibilityInfo>> {
        self.accessibility.as_ref()
    }
    #[inline]
    pub fn get_menu_bar(&self) -> Option<&Box<Menu>> {
        self.extra.as_ref().and_then(|e| e.menu_bar.as_ref())
    }
    #[inline]
    pub fn get_context_menu(&self) -> Option<&Box<Menu>> {
        self.extra.as_ref().and_then(|e| e.context_menu.as_ref())
    }

    /// Returns whether this node is an anonymous box generated for table layout.
    #[inline]
    pub fn is_anonymous(&self) -> bool {
        self.flags.is_anonymous()
    }

    #[inline(always)]
    pub fn set_node_type(&mut self, node_type: NodeType) {
        self.node_type = node_type;
    }
    #[inline]
    pub fn set_dataset(&mut self, data: OptionRefAny) {
        match data {
            OptionRefAny::None => {
                if let Some(ext) = self.extra.as_mut() {
                    ext.dataset = None;
                }
            }
            OptionRefAny::Some(r) => {
                self.extra
                    .get_or_insert_with(|| Box::new(NodeDataExt::default()))
                    .dataset = Some(r);
            }
        }
    }
    /// Sets the IDs and classes by converting `IdOrClassVec` entries into
    /// `AttributeType::Id`/`AttributeType::Class` and merging them into `self.attributes`.
    /// Any existing Id/Class attributes are removed first.
    #[inline]
    pub fn set_ids_and_classes(&mut self, ids_and_classes: IdOrClassVec) {
        // Remove existing Id/Class from attributes
        let mut v: AttributeTypeVec = Vec::new().into();
        mem::swap(&mut v, &mut self.attributes);
        let mut v = v.into_library_owned_vec();
        v.retain(|a| !matches!(a, AttributeType::Id(_) | AttributeType::Class(_)));
        // Convert and append
        for ioc in ids_and_classes.as_ref().iter() {
            match ioc {
                IdOrClass::Id(s) => v.push(AttributeType::Id(s.clone())),
                IdOrClass::Class(s) => v.push(AttributeType::Class(s.clone())),
            }
        }
        self.attributes = v.into();
    }
    #[inline(always)]
    pub fn set_callbacks(&mut self, callbacks: CoreCallbackDataVec) {
        self.callbacks = callbacks;
    }
    #[inline(always)]
    pub fn set_css_props(&mut self, css_props: CssPropertyWithConditionsVec) {
        self.css_props = css_props;
    }
    #[inline]
    pub fn set_clip_mask(&mut self, clip_mask: ImageMask) {
        self.extra
            .get_or_insert_with(|| Box::new(NodeDataExt::default()))
            .clip_mask = Some(clip_mask);
    }
    #[inline]
    pub fn set_tab_index(&mut self, tab_index: TabIndex) {
        self.flags.set_tab_index(Some(tab_index));
    }
    #[inline]
    pub fn set_contenteditable(&mut self, contenteditable: bool) {
        self.flags.set_contenteditable_mut(contenteditable);
    }
    #[inline]
    pub fn is_contenteditable(&self) -> bool {
        self.flags.is_contenteditable()
    }
    #[inline]
    pub fn set_accessibility_info(&mut self, accessibility_info: AccessibilityInfo) {
        self.accessibility = Some(Box::new(accessibility_info));
    }

    /// Marks this node as an anonymous box (generated for table layout).
    #[inline]
    pub fn set_anonymous(&mut self, is_anonymous: bool) {
        self.flags.set_anonymous(is_anonymous);
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

    /// Sets a stable key for this node used in reconciliation.
    ///
    /// This key is used to track node identity across DOM updates, enabling
    /// the framework to distinguish between "moving" a node and "destroying/creating" one.
    /// This is crucial for correct lifecycle events when lists are reordered.
    ///
    /// # Example
    /// ```rust
    /// # use azul_core::dom::NodeData;
    /// # let mut node_data = NodeData::create_div();
    /// node_data.set_key("user-123");
    /// ```
    #[inline]
    pub fn set_key<K: core::hash::Hash>(&mut self, key: K) {
        use highway::{HighwayHash, HighwayHasher, Key};
        let mut hasher = HighwayHasher::new(Key([0; 4]));
        key.hash(&mut hasher);
        self.extra
            .get_or_insert_with(|| Box::new(NodeDataExt::default()))
            .key = Some(hasher.finalize64());
    }

    /// Gets the key for this node, if set.
    #[inline]
    pub fn get_key(&self) -> Option<u64> {
        self.extra.as_ref().and_then(|ext| ext.key)
    }

    /// Sets a dataset merge callback for this node.
    ///
    /// The merge callback is invoked during reconciliation when a node from the
    /// previous frame is matched with a node in the new frame. It allows heavy
    /// resources (video decoders, GL textures, network connections) to be
    /// transferred from the old node to the new node instead of being destroyed.
    ///
    /// # Type Safety
    ///
    /// The callback stores the `TypeId` of `T`. During execution, both the old
    /// and new datasets must match this type, otherwise the merge is skipped.
    ///
    /// # Example
    /// ```rust,ignore
    /// struct VideoPlayer {
    ///     url: String,
    ///     decoder: Option<DecoderHandle>,
    /// }
    /// 
    /// extern "C" fn merge_video(new_data: RefAny, old_data: RefAny) -> RefAny {
    ///     // Transfer the heavy decoder handle from old to new
    ///     if let (Some(mut new), Some(old)) = (
    ///         new_data.downcast_mut::<VideoPlayer>(),
    ///         old_data.downcast_ref::<VideoPlayer>()
    ///     ) {
    ///         new.decoder = old.decoder.take();
    ///     }
    ///     new_data
    /// }
    /// 
    /// node_data.set_merge_callback(merge_video);
    /// ```
    #[inline]
    pub fn set_merge_callback<C: Into<DatasetMergeCallback>>(&mut self, callback: C) {
        self.extra
            .get_or_insert_with(|| Box::new(NodeDataExt::default()))
            .dataset_merge_callback = Some(callback.into());
    }

    /// Gets the merge callback for this node, if set.
    #[inline]
    pub fn get_merge_callback(&self) -> Option<DatasetMergeCallback> {
        self.extra.as_ref().and_then(|ext| ext.dataset_merge_callback.clone())
    }

    /// Sets the component origin for this node.
    ///
    /// This stamps the node with information about which component rendered it,
    /// enabling the debugger to reconstruct the component invocation tree.
    #[inline]
    pub fn set_component_origin(&mut self, origin: ComponentOrigin) {
        self.extra
            .get_or_insert_with(|| Box::new(NodeDataExt::default()))
            .component_origin = Some(origin);
    }

    /// Gets the component origin for this node, if set.
    #[inline]
    pub fn get_component_origin(&self) -> Option<&ComponentOrigin> {
        self.extra.as_ref().and_then(|ext| ext.component_origin.as_ref())
    }

    #[inline]
    pub fn with_menu_bar(mut self, menu_bar: Menu) -> Self {
        self.set_menu_bar(menu_bar);
        self
    }

    #[inline]
    pub fn with_context_menu(mut self, context_menu: Menu) -> Self {
        self.set_context_menu(context_menu);
        self
    }

    #[inline]
    pub fn add_callback<C: Into<CoreCallback>>(
        &mut self,
        event: EventFilter,
        data: RefAny,
        callback: C,
    ) {
        let callback = callback.into();
        let mut v: CoreCallbackDataVec = Vec::new().into();
        mem::swap(&mut v, &mut self.callbacks);
        let mut v = v.into_library_owned_vec();
        v.push(CoreCallbackData {
            event,
            refany: data,
            callback,
        });
        self.callbacks = v.into();
    }

    #[inline]
    pub fn add_id(&mut self, s: AzString) {
        let mut v: AttributeTypeVec = Vec::new().into();
        mem::swap(&mut v, &mut self.attributes);
        let mut v = v.into_library_owned_vec();
        v.push(AttributeType::Id(s));
        self.attributes = v.into();
    }
    #[inline]
    pub fn add_class(&mut self, s: AzString) {
        let mut v: AttributeTypeVec = Vec::new().into();
        mem::swap(&mut v, &mut self.attributes);
        let mut v = v.into_library_owned_vec();
        v.push(AttributeType::Class(s));
        self.attributes = v.into();
    }

    /// Add a CSS property with optional conditions (hover, focus, active, etc.)
    #[inline]
    pub fn add_css_property(&mut self, p: CssPropertyWithConditions) {
        let mut v: CssPropertyWithConditionsVec = Vec::new().into();
        mem::swap(&mut v, &mut self.css_props);
        let mut v = v.into_library_owned_vec();
        v.push(p);
        self.css_props = v.into();
    }

    /// Calculates a deterministic node hash for this node.
    pub fn calculate_node_data_hash(&self) -> DomNodeHash {
        use highway::{HighwayHash, HighwayHasher, Key};
        let mut hasher = HighwayHasher::new(Key([0; 4]));
        self.hash(&mut hasher);
        let h = hasher.finalize64();
        DomNodeHash { inner: h }
    }

    /// Calculates a structural hash for DOM reconciliation that ignores text content.
    /// 
    /// This hash is used for matching nodes across DOM frames where the text content
    /// may have changed (e.g., contenteditable text being edited). It hashes:
    /// - Node type discriminant (but NOT the text content for Text nodes)
    /// - IDs and classes
    /// - Attributes (but NOT contenteditable state which may change with focus)
    /// - Callback events and types
    /// 
    /// This allows a Text("Hello") node to match Text("Hello World") during reconciliation,
    /// preserving cursor position and selection state.
    pub fn calculate_structural_hash(&self) -> DomNodeHash {
        use highway::{HighwayHash, HighwayHasher, Key};
        use core::hash::Hasher as StdHasher;
        
        let mut hasher = HighwayHasher::new(Key([0; 4]));
        
        // Hash node type discriminant only, not content
        // This means Text("A") and Text("B") have the same structural hash
        core::mem::discriminant(&self.node_type).hash(&mut hasher);
        
        // For VirtualizedView nodes, hash the callback to distinguish different virtualized views
        if let NodeType::VirtualizedView = self.node_type {
            if let Some(ext) = self.extra.as_ref() {
                if let Some(vv) = ext.virtualized_view.as_ref() {
                    vv.hash(&mut hasher);
                }
            }
        }
        
        // For Image nodes, hash the image reference to distinguish different images.
        // For callback images, hash the callback function pointer and RefAny type ID
        // instead of the heap pointer, so that the same callback produces the same
        // structural hash across frames (the heap pointer differs each frame because
        // ImageRef::new() does Box::into_raw(Box::new(...))).
        if let NodeType::Image(ref img_ref) = self.node_type {
            match img_ref.get_data() {
                crate::resources::DecodedImage::Callback(cb) => {
                    // Hash callback function pointer (stable across frames)
                    cb.callback.cb.hash(&mut hasher);
                    // Hash RefAny type ID (not instance pointer)
                    cb.refany.get_type_id().hash(&mut hasher);
                }
                _ => {
                    // Raw images / GL textures: hash normally (pointer identity)
                    img_ref.hash(&mut hasher);
                }
            }
        }
        
        // Hash IDs and classes - these are structural and shouldn't change
        // (They are now stored as AttributeType::Id / AttributeType::Class in attributes)
        for attr in self.attributes.as_ref().iter() {
            match attr {
                AttributeType::Id(s) => { 0u8.hash(&mut hasher); s.as_str().hash(&mut hasher); }
                AttributeType::Class(s) => { 1u8.hash(&mut hasher); s.as_str().hash(&mut hasher); }
                _ => {}
            }
        }
        
        // Hash other attributes - but skip contenteditable since that might change
        // Also skip Id/Class since they were already hashed above
        for attr in self.attributes.as_ref().iter() {
            if !matches!(attr, AttributeType::ContentEditable(_) | AttributeType::Id(_) | AttributeType::Class(_)) {
                attr.hash(&mut hasher);
            }
        }
        
        // Hash callback events (not the actual callback function pointers)
        for callback in self.callbacks.as_ref().iter() {
            callback.event.hash(&mut hasher);
        }
        
        let h = hasher.finalize64();
        DomNodeHash { inner: h }
    }

    #[inline(always)]
    pub fn with_tab_index(mut self, tab_index: TabIndex) -> Self {
        self.set_tab_index(tab_index);
        self
    }
    #[inline(always)]
    pub fn with_contenteditable(mut self, contenteditable: bool) -> Self {
        self.set_contenteditable(contenteditable);
        self
    }
    #[inline(always)]
    pub fn with_node_type(mut self, node_type: NodeType) -> Self {
        self.set_node_type(node_type);
        self
    }
    #[inline(always)]
    pub fn with_callback<C: Into<CoreCallback>>(
        mut self,
        event: EventFilter,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.add_callback(event, data, callback);
        self
    }
    #[inline(always)]
    #[inline]
    pub fn with_dataset(mut self, data: OptionRefAny) -> Self {
        self.set_dataset(data);
        self
    }
    #[inline(always)]
    pub fn with_ids_and_classes(mut self, ids_and_classes: IdOrClassVec) -> Self {
        self.set_ids_and_classes(ids_and_classes);
        self
    }
    #[inline(always)]
    pub fn with_callbacks(mut self, callbacks: CoreCallbackDataVec) -> Self {
        self.callbacks = callbacks;
        self
    }
    #[inline(always)]
    pub fn with_css_props(mut self, css_props: CssPropertyWithConditionsVec) -> Self {
        self.css_props = css_props;
        self
    }

    /// Assigns a stable key to this node for reconciliation.
    ///
    /// This is crucial for performance and correct state preservation when
    /// lists of items change order or items are inserted/removed. Without keys,
    /// the reconciliation algorithm falls back to hash-based matching.
    ///
    /// # Example
    /// ```rust
    /// # use azul_core::dom::NodeData;
    /// NodeData::create_div()
    ///     .with_key("user-avatar-123");
    /// ```
    #[inline]
    pub fn with_key<K: core::hash::Hash>(mut self, key: K) -> Self {
        self.set_key(key);
        self
    }

    /// Registers a callback to merge dataset state from the previous frame.
    ///
    /// This is used for components that maintain heavy internal state (video players,
    /// WebGL contexts, network connections) that should not be destroyed and recreated
    /// on every render frame.
    ///
    /// The callback receives both datasets as `RefAny` (cheap shallow clones) and
    /// returns the `RefAny` that should be used for the new node.
    ///
    /// # Example
    /// ```rust,ignore
    /// struct VideoPlayer {
    ///     url: String,
    ///     decoder_handle: Option<DecoderHandle>,
    /// }
    ///
    /// extern "C" fn merge_video(new_data: RefAny, old_data: RefAny) -> RefAny {
    ///     if let (Some(mut new), Some(old)) = (
    ///         new_data.downcast_mut::<VideoPlayer>(),
    ///         old_data.downcast_ref::<VideoPlayer>()
    ///     ) {
    ///         new.decoder_handle = old.decoder_handle.take();
    ///     }
    ///     new_data
    /// }
    ///
    /// NodeData::create_div()
    ///     .with_dataset(RefAny::new(VideoPlayer::new("movie.mp4")).into())
    ///     .with_merge_callback(merge_video)
    /// ```
    #[inline]
    pub fn with_merge_callback<C: Into<DatasetMergeCallback>>(mut self, callback: C) -> Self {
        self.set_merge_callback(callback);
        self
    }

    /// Parse CSS from a string and add as unconditional properties
    /// 
    /// Deprecated: Use `set_css()` for full selector support
    pub fn set_inline_style(&mut self, style: &str) {
        let parsed = CssPropertyWithConditionsVec::parse(style);
        let parsed_vec = parsed.into_library_owned_vec();
        let mut current = Vec::new().into();
        mem::swap(&mut current, &mut self.css_props);
        let mut v = current.into_library_owned_vec();
        v.extend(parsed_vec);
        self.css_props = v.into();
    }

    /// Builder method for setting inline CSS styles for the normal state
    /// 
    /// Deprecated: Use `with_css()` for full selector support
    pub fn with_inline_style(mut self, style: &str) -> Self {
        self.set_inline_style(style);
        self
    }
    
    /// Parse and set CSS styles with full selector support.
    /// 
    /// This is the unified API for setting inline CSS on a node. It supports:
    /// - Simple properties: `color: red; font-size: 14px;`
    /// - Pseudo-selectors: `:hover { background: blue; }`
    /// - @-rules: `@os linux { font-size: 14px; }`
    /// - Nesting: `@os linux { font-size: 14px; :hover { color: red; }}`
    /// 
    /// # Examples
    /// ```rust
    /// # use azul_core::dom::NodeData;
    /// NodeData::create_div().with_css("
    ///     color: blue;
    ///     :hover { color: red; }
    ///     @os linux { font-size: 14px; }
    /// ");
    /// ```
    pub fn set_css(&mut self, style: &str) {
        let parsed = CssPropertyWithConditionsVec::parse(style);
        let mut current = Vec::new().into();
        mem::swap(&mut current, &mut self.css_props);
        let mut v = current.into_library_owned_vec();
        v.extend(parsed.into_library_owned_vec());
        self.css_props = v.into();
    }
    
    /// Builder method for `set_css`
    pub fn with_css(mut self, style: &str) -> Self {
        self.set_css(style);
        self
    }

    /// Sets inline CSS styles for the hover state, parsing from a CSS string
    /// 
    /// Deprecated: Use `with_css(":hover { ... }")` instead
    pub fn set_inline_hover_style(&mut self, style: &str) {
        let parsed = CssPropertyWithConditionsVec::parse_hover(style);
        let mut current = Vec::new().into();
        mem::swap(&mut current, &mut self.css_props);
        let mut v = current.into_library_owned_vec();
        v.extend(parsed.into_library_owned_vec());
        self.css_props = v.into();
    }

    /// Builder method for setting inline CSS styles for the hover state
    /// 
    /// Deprecated: Use `with_css(":hover { ... }")` instead
    pub fn with_inline_hover_style(mut self, style: &str) -> Self {
        self.set_inline_hover_style(style);
        self
    }

    /// Sets inline CSS styles for the active state, parsing from a CSS string
    /// 
    /// Deprecated: Use `with_css(":active { ... }")` instead
    pub fn set_inline_active_style(&mut self, style: &str) {
        let parsed = CssPropertyWithConditionsVec::parse_active(style);
        let mut current = Vec::new().into();
        mem::swap(&mut current, &mut self.css_props);
        let mut v = current.into_library_owned_vec();
        v.extend(parsed.into_library_owned_vec());
        self.css_props = v.into();
    }

    /// Builder method for setting inline CSS styles for the active state
    /// 
    /// Deprecated: Use `with_css(":active { ... }")` instead
    pub fn with_inline_active_style(mut self, style: &str) -> Self {
        self.set_inline_active_style(style);
        self
    }

    /// Sets inline CSS styles for the focus state, parsing from a CSS string
    /// 
    /// Deprecated: Use `with_css(":focus { ... }")` instead
    pub fn set_inline_focus_style(&mut self, style: &str) {
        let parsed = CssPropertyWithConditionsVec::parse_focus(style);
        let mut current = Vec::new().into();
        mem::swap(&mut current, &mut self.css_props);
        let mut v = current.into_library_owned_vec();
        v.extend(parsed.into_library_owned_vec());
        self.css_props = v.into();
    }

    /// Builder method for setting inline CSS styles for the focus state
    /// 
    /// Deprecated: Use `with_css(":focus { ... }")` instead
    pub fn with_inline_focus_style(mut self, style: &str) -> Self {
        self.set_inline_focus_style(style);
        self
    }

    #[inline(always)]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = NodeData::create_div();
        mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn copy_special(&self) -> Self {
        Self {
            node_type: self.node_type.into_library_owned_nodetype(),
            attributes: self.attributes.clone(),
            css_props: self.css_props.clone(),
            callbacks: self.callbacks.clone(),
            flags: self.flags,
            accessibility: self.accessibility.clone(),
            extra: self.extra.clone(),
        }
    }

    pub fn is_focusable(&self) -> bool {
        // Element is focusable if it has a tab index or any focus-related callback
        self.get_tab_index().is_some()
            || self
                .get_callbacks()
                .iter()
                .any(|cb| cb.event.is_focus_callback())
    }

    /// Returns true if this element has "activation behavior" per HTML5 spec.
    ///
    /// Elements with activation behavior can be activated via Enter or Space key
    /// when focused, which generates a synthetic click event.
    ///
    /// Per HTML5 spec, elements with activation behavior include:
    /// - Button elements
    /// - Input elements (submit, button, reset, checkbox, radio)
    /// - Anchor elements with href
    /// - Any element with a click callback (implicit activation)
    ///
    /// See: https://html.spec.whatwg.org/multipage/interaction.html#activation-behavior
    pub fn has_activation_behavior(&self) -> bool {
        use crate::events::{EventFilter, HoverEventFilter};
        
        // Check for click callback (most common case for Azul)
        // In Azul, "click" is typically LeftMouseUp
        let has_click_callback = self
            .get_callbacks()
            .iter()
            .any(|cb| matches!(
                cb.event, 
                EventFilter::Hover(HoverEventFilter::MouseUp) 
                | EventFilter::Hover(HoverEventFilter::LeftMouseUp)
            ));

        if has_click_callback {
            return true;
        }

        // Check accessibility role for button-like elements
        if let Some(ref accessibility) = self.accessibility {
            use crate::dom::AccessibilityRole;
            match accessibility.role {
                AccessibilityRole::PushButton  // Button
                | AccessibilityRole::Link
                | AccessibilityRole::CheckButton  // Checkbox
                | AccessibilityRole::RadioButton  // Radio
                | AccessibilityRole::MenuItem
                | AccessibilityRole::PageTab  // Tab
                => return true,
                _ => {}
            }
        }

        false
    }

    /// Returns true if this element is currently activatable.
    ///
    /// An element is activatable if it has activation behavior AND is not disabled.
    /// This checks for common disability patterns (aria-disabled, disabled attribute).
    pub fn is_activatable(&self) -> bool {
        if !self.has_activation_behavior() {
            return false;
        }

        // Check for disabled state in accessibility info
        if let Some(ref accessibility) = self.accessibility {
            // Check if explicitly marked as unavailable
            if accessibility
                .states
                .as_ref()
                .iter()
                .any(|s| matches!(s, AccessibilityState::Unavailable))
            {
                return false;
            }
        }

        // Not disabled, so activatable
        true
    }

    /// Returns the tab index for this element.
    ///
    /// Tab index determines keyboard navigation order:
    /// - `None`: Not in tab order (unless naturally focusable)
    /// - `Some(-1)`: Focusable programmatically but not via Tab
    /// - `Some(0)`: In natural tab order
    /// - `Some(n > 0)`: In tab order with priority n (higher = later)
    pub fn get_effective_tabindex(&self) -> Option<i32> {
        match self.flags.get_tab_index() {
            None => {
                // Check if naturally focusable (has focus callback)
                if self.get_callbacks().iter().any(|cb| cb.event.is_focus_callback()) {
                    Some(0)
                } else {
                    None
                }
            }
            Some(tab_idx) => {
                match tab_idx {
                    TabIndex::Auto => Some(0),
                    TabIndex::OverrideInParent(n) => Some(n as i32),
                    TabIndex::NoKeyboardFocus => Some(-1),
                }
            }
        }
    }

    pub fn get_virtualized_view_node(&mut self) -> Option<&mut VirtualizedViewNode> {
        self.extra.as_mut()?.virtualized_view.as_mut()
    }

    pub fn get_virtualized_view_node_ref(&self) -> Option<&VirtualizedViewNode> {
        self.extra.as_ref()?.virtualized_view.as_ref()
    }

    pub fn get_render_image_callback_node<'a>(
        &'a mut self,
    ) -> Option<(&'a mut CoreImageCallback, ImageRefHash)> {
        match &mut self.node_type {
            NodeType::Image(img) => {
                let hash = image_ref_get_hash(&img);
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

/// A unique, runtime-generated identifier for a single `Dom` instance.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct DomId {
    pub inner: usize,
}

impl fmt::Display for DomId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl DomId {
    pub const ROOT_ID: DomId = DomId { inner: 0 };
}

impl Default for DomId {
    fn default() -> DomId {
        DomId::ROOT_ID
    }
}

impl_option!(
    DomId,
    OptionDomId,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(DomId, DomIdVec, DomIdVecDestructor, DomIdVecDestructorType, DomIdVecSlice, OptionDomId);
impl_vec_debug!(DomId, DomIdVec);
impl_vec_clone!(DomId, DomIdVec, DomIdVecDestructor);
impl_vec_partialeq!(DomId, DomIdVec);
impl_vec_partialord!(DomId, DomIdVec);

/// A UUID for a DOM node within a `LayoutWindow`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct DomNodeId {
    /// The ID of the `Dom` this node belongs to.
    pub dom: DomId,
    /// The hierarchical ID of the node within its `Dom`.
    pub node: NodeHierarchyItemId,
}

impl_option!(
    DomNodeId,
    OptionDomNodeId,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl DomNodeId {
    pub const ROOT: DomNodeId = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::NONE,
    };
}

/// The document model, similar to HTML. This is a create-only structure, you don't actually read
/// anything back from it. It's designed for ease of construction.
#[repr(C)]
#[derive(PartialEq, Clone, PartialOrd)]
pub struct Dom {
    /// The data for the root node of this DOM (or sub-DOM).
    pub root: NodeData,
    /// The children of this DOM node.
    pub children: DomVec,
    /// Ordered list of CSS stylesheets to apply to this DOM subtree.
    /// Stylesheets are applied in push order during the single deferred cascade pass.
    /// Later entries override earlier ones (higher cascade priority).
    pub css: azul_css::css::CssVec,
    // Tracks the number of sub-children of the current children, so that
    // the `Dom` can be converted into a `CompactDom`.
    pub estimated_total_children: usize,
}

// Manual Eq/Hash/Ord impls that skip the transient `css` field,
// since CssVec does not implement Eq/Hash/Ord.
impl Eq for Dom {}

impl core::hash::Hash for Dom {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.root.hash(state);
        self.children.hash(state);
        self.estimated_total_children.hash(state);
    }
}

impl Ord for Dom {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.root.cmp(&other.root)
            .then_with(|| self.children.cmp(&other.children))
            .then_with(|| self.estimated_total_children.cmp(&other.estimated_total_children))
    }
}

impl_option!(
    Dom,
    OptionDom,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(Dom, DomVec, DomVecDestructor, DomVecDestructorType, DomVecSlice, OptionDom);
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
    pub fn create_node(node_type: NodeType) -> Self {
        Self {
            root: NodeData::create_node(node_type),
            children: Vec::new().into(),
            css: Vec::new().into(),
            estimated_total_children: 0,
        }
    }
    #[inline(always)]
    pub fn from_data(node_data: NodeData) -> Self {
        Self {
            root: node_data,
            children: Vec::new().into(),
            css: Vec::new().into(),
            estimated_total_children: 0,
        }
    }

    // Document Structure Elements

    /// Creates the root HTML element.
    ///
    /// **Accessibility**: The `<html>` element is the root of an HTML document and should have a
    /// `lang` attribute.
    #[inline(always)]
    pub const fn create_html() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Html),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates the document head element.
    ///
    /// **Accessibility**: The `<head>` contains metadata. Use `<title>` for page titles.
    #[inline(always)]
    pub const fn create_head() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Head),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    #[inline(always)]
    pub const fn create_body() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Body),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a generic block-level container.
    ///
    /// **Accessibility**: Prefer semantic elements like `<article>`, `<section>`, `<nav>` when
    /// applicable.
    #[inline(always)]
    pub const fn create_div() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Div),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    // Semantic Structure Elements

    /// Creates an article element.
    ///
    /// **Accessibility**: Represents self-contained content that could be distributed
    /// independently. Screen readers can navigate by articles. Consider adding aria-label for
    /// multiple articles.
    #[inline(always)]
    pub const fn create_article() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Article),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a section element.
    ///
    /// **Accessibility**: Represents a thematic grouping of content with a heading.
    /// Should typically have a heading (h1-h6) as a child. Consider aria-labelledby.
    #[inline(always)]
    pub const fn create_section() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Section),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a navigation element.
    ///
    /// **Accessibility**: Represents navigation links. Screen readers can jump to navigation.
    /// Use aria-label to distinguish multiple nav elements (e.g., "Main navigation", "Footer
    /// links").
    #[inline(always)]
    pub const fn create_nav() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Nav),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates an aside element.
    ///
    /// **Accessibility**: Represents content tangentially related to main content (sidebars,
    /// callouts). Screen readers announce this as complementary content.
    #[inline(always)]
    pub const fn create_aside() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Aside),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a header element.
    ///
    /// **Accessibility**: Represents introductory content or navigational aids.
    /// Can be used for page headers or section headers.
    #[inline(always)]
    pub const fn create_header() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Header),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a footer element.
    ///
    /// **Accessibility**: Represents footer for nearest section or page.
    /// Typically contains copyright, author info, or related links.
    #[inline(always)]
    pub const fn create_footer() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Footer),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a main content element.
    ///
    /// **Accessibility**: Represents the dominant content. There should be only ONE main per page.
    /// Screen readers can jump directly to main content. Do not nest inside
    /// article/aside/footer/header/nav.
    #[inline(always)]
    pub const fn create_main() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Main),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a figure element.
    ///
    /// **Accessibility**: Represents self-contained content like diagrams, photos, code listings.
    /// Use with `<figcaption>` to provide a caption. Screen readers associate caption with figure.
    #[inline(always)]
    pub const fn create_figure() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Figure),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a figure caption element.
    ///
    /// **Accessibility**: Provides a caption for `<figure>`. Screen readers announce this as the
    /// figure description.
    #[inline(always)]
    pub const fn create_figcaption() -> Self {
        Self {
            root: NodeData::create_node(NodeType::FigCaption),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    // Interactive Elements

    /// Creates a details disclosure element.
    ///
    /// **Accessibility**: Creates a disclosure widget. Screen readers announce expanded/collapsed
    /// state. Must contain a `<summary>` element. Keyboard accessible by default.
    #[inline(always)]
    pub const fn create_details() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Details),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a summary element for details.
    ///
    /// **Accessibility**: The visible heading/label for `<details>`.
    /// Must be the first child of details. Keyboard accessible (Enter/Space to toggle).
    #[inline]
    pub fn summary<S: Into<AzString>>(text: S) -> Self {
        Self {
            root: NodeData::create_node(NodeType::Summary),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
        .with_child(Self::create_text(text))
    }

    /// Creates a dialog element.
    ///
    /// **Accessibility**: Represents a modal or non-modal dialog.
    /// When opened as modal, focus is trapped. Use aria-label or aria-labelledby.
    /// Escape key should close modal dialogs.
    #[inline(always)]
    pub const fn create_dialog() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Dialog),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    // Basic Structural Elements

    #[inline(always)]
    pub const fn create_br() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Br),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }
    #[inline(always)]
    pub fn create_text<S: Into<AzString>>(value: S) -> Self {
        Self::create_node(NodeType::Text(value.into()))
    }
    #[inline(always)]
    pub fn create_image(image: ImageRef) -> Self {
        Self::create_node(NodeType::Image(image))
    }
    /// Creates an icon node with the given icon name.
    ///
    /// The icon name should match names from the icon provider (e.g., "home", "settings", "search").
    /// Icons are resolved to actual content (font glyph, image, etc.) during StyledDom creation
    /// based on the configured IconProvider.
    ///
    /// # Example
    /// ```rust,ignore
    /// Dom::create_icon("home")
    ///     .with_class("nav-icon")
    /// ```
    #[inline(always)]
    pub fn create_icon<S: Into<AzString>>(icon_name: S) -> Self {
        Self::create_node(NodeType::Icon(icon_name.into()))
    }

    #[inline(always)]
    pub fn create_virtualized_view(data: RefAny, callback: impl Into<VirtualizedViewCallback>) -> Self {
        Self::from_data(NodeData::create_virtualized_view(data, callback))
    }

    // Semantic HTML Elements with Accessibility Guidance

    /// Creates a paragraph element.
    ///
    /// **Accessibility**: Paragraphs provide semantic structure for screen readers.
    #[inline(always)]
    pub const fn create_p() -> Self {
        Self {
            root: NodeData::create_node(NodeType::P),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a heading level 1 element.
    ///
    /// **Accessibility**: Use `h1` for the main page title. There should typically be only one `h1`
    /// per page.
    ///
    /// **Parameters:**
    /// - `text`: Heading text
    #[inline]
    pub fn h1<S: Into<AzString>>(text: S) -> Self {
        Self {
            root: NodeData::create_node(NodeType::H1),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
        .with_child(Self::create_text(text))
    }

    /// Creates a heading level 2 element.
    ///
    /// **Accessibility**: Use `h2` for major section headings under `h1`.
    ///
    /// **Parameters:**
    /// - `text`: Heading text
    #[inline]
    pub fn h2<S: Into<AzString>>(text: S) -> Self {
        Self {
            root: NodeData::create_node(NodeType::H2),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
        .with_child(Self::create_text(text))
    }

    /// Creates a heading level 3 element.
    ///
    /// **Accessibility**: Use `h3` for subsections under `h2`.
    ///
    /// **Parameters:**
    /// - `text`: Heading text
    #[inline]
    pub fn h3<S: Into<AzString>>(text: S) -> Self {
        Self {
            root: NodeData::create_node(NodeType::H3),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
        .with_child(Self::create_text(text))
    }

    /// Creates a heading level 4 element.
    ///
    /// **Parameters:**
    /// - `text`: Heading text
    #[inline]
    pub fn h4<S: Into<AzString>>(text: S) -> Self {
        Self {
            root: NodeData::create_node(NodeType::H4),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
        .with_child(Self::create_text(text))
    }

    /// Creates a heading level 5 element.
    ///
    /// **Parameters:**
    /// - `text`: Heading text
    #[inline]
    pub fn h5<S: Into<AzString>>(text: S) -> Self {
        Self {
            root: NodeData::create_node(NodeType::H5),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
        .with_child(Self::create_text(text))
    }

    /// Creates a heading level 6 element.
    ///
    /// **Parameters:**
    /// - `text`: Heading text
    #[inline]
    pub fn h6<S: Into<AzString>>(text: S) -> Self {
        Self {
            root: NodeData::create_node(NodeType::H6),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
        .with_child(Self::create_text(text))
    }

    /// Creates a generic inline container (span).
    ///
    /// **Accessibility**: Prefer semantic elements like `strong`, `em`, `code`, etc. when
    /// applicable.
    ///
    /// **Parameters:**
    /// - `text`: Span content
    #[inline]
    pub fn span<S: Into<AzString>>(text: S) -> Self {
        Self {
            root: NodeData::create_node(NodeType::Span),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
        .with_child(Self::create_text(text))
    }

    /// Creates a strongly emphasized text element (strong importance).
    ///
    /// **Accessibility**: Use `strong` instead of `b` for semantic meaning. Screen readers can
    /// convey the importance. Use for text that has strong importance, seriousness, or urgency.
    ///
    /// **Parameters:**
    /// - `text`: Text to emphasize
    #[inline]
    pub fn strong<S: Into<AzString>>(text: S) -> Self {
        Self {
            root: NodeData::create_node(NodeType::Strong),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
        .with_child(Self::create_text(text))
    }

    /// Creates an emphasized text element (stress emphasis).
    ///
    /// **Accessibility**: Use `em` instead of `i` for semantic meaning. Screen readers can
    /// convey the emphasis. Use for text that has stress emphasis.
    ///
    /// **Parameters:**
    /// - `text`: Text to emphasize
    #[inline]
    pub fn em<S: Into<AzString>>(text: S) -> Self {
        Self {
            root: NodeData::create_node(NodeType::Em),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
        .with_child(Self::create_text(text))
    }

    /// Creates a code/computer code element.
    ///
    /// **Accessibility**: Represents a fragment of computer code. Screen readers can identify
    /// this as code content.
    ///
    /// **Parameters:**
    /// - `code`: Code content
    #[inline]
    pub fn code<S: Into<AzString>>(code: S) -> Self {
        Self::create_node(NodeType::Code).with_child(Self::create_text(code))
    }

    /// Creates a preformatted text element.
    ///
    /// **Accessibility**: Preserves whitespace and line breaks. Useful for code blocks or
    /// ASCII art. Screen readers will read the content as-is.
    ///
    /// **Parameters:**
    /// - `text`: Preformatted content
    #[inline]
    pub fn pre<S: Into<AzString>>(text: S) -> Self {
        Self::create_node(NodeType::Pre).with_child(Self::create_text(text))
    }

    /// Creates a blockquote element.
    ///
    /// **Accessibility**: Represents a section quoted from another source. Screen readers
    /// can identify quoted content. Consider adding a `cite` attribute.
    ///
    /// **Parameters:**
    /// - `text`: Quote content
    #[inline]
    pub fn blockquote<S: Into<AzString>>(text: S) -> Self {
        Self::create_node(NodeType::BlockQuote).with_child(Self::create_text(text))
    }

    /// Creates a citation element.
    ///
    /// **Accessibility**: Represents a reference to a creative work. Screen readers can
    /// identify citations.
    ///
    /// **Parameters:**
    /// - `text`: Citation text
    #[inline]
    pub fn cite<S: Into<AzString>>(text: S) -> Self {
        Self::create_node(NodeType::Cite).with_child(Self::create_text(text))
    }

    /// Creates an abbreviation element.
    ///
    /// **Accessibility**: Represents an abbreviation or acronym. Use with a `title` attribute
    /// to provide the full expansion for screen readers.
    ///
    /// **Parameters:**
    /// - `abbr_text`: Abbreviated text
    /// - `title`: Full expansion
    #[inline]
    pub fn create_abbr(abbr_text: AzString, title: AzString) -> Self {
        Self::create_node(NodeType::Abbr)
            .with_attribute(AttributeType::Title(title))
            .with_child(Self::create_text(abbr_text))
    }

    /// Creates a keyboard input element.
    ///
    /// **Accessibility**: Represents keyboard input or key combinations. Screen readers can
    /// identify keyboard instructions.
    ///
    /// **Parameters:**
    /// - `text`: Keyboard instruction
    #[inline]
    pub fn kbd<S: Into<AzString>>(text: S) -> Self {
        Self::create_node(NodeType::Kbd).with_child(Self::create_text(text))
    }

    /// Creates a sample output element.
    ///
    /// **Accessibility**: Represents sample output from a program or computing system.
    ///
    /// **Parameters:**
    /// - `text`: Sample text
    #[inline]
    pub fn samp<S: Into<AzString>>(text: S) -> Self {
        Self::create_node(NodeType::Samp).with_child(Self::create_text(text))
    }

    /// Creates a variable element.
    ///
    /// **Accessibility**: Represents a variable in mathematical expressions or programming.
    ///
    /// **Parameters:**
    /// - `text`: Variable name
    #[inline]
    pub fn var<S: Into<AzString>>(text: S) -> Self {
        Self::create_node(NodeType::Var).with_child(Self::create_text(text))
    }

    /// Creates a subscript element.
    ///
    /// **Accessibility**: Screen readers may announce subscript formatting.
    ///
    /// **Parameters:**
    /// - `text`: Subscript content
    #[inline]
    pub fn sub<S: Into<AzString>>(text: S) -> Self {
        Self::create_node(NodeType::Sub).with_child(Self::create_text(text))
    }

    /// Creates a superscript element.
    ///
    /// **Accessibility**: Screen readers may announce superscript formatting.
    ///
    /// **Parameters:**
    /// - `text`: Superscript content
    #[inline]
    pub fn sup<S: Into<AzString>>(text: S) -> Self {
        Self::create_node(NodeType::Sup).with_child(Self::create_text(text))
    }

    /// Creates an underline text element.
    ///
    /// **Accessibility**: Screen readers typically don't announce underline formatting.
    /// Use semantic elements when possible (e.g., `<em>` for emphasis).
    #[inline]
    pub fn u<S: Into<AzString>>(text: S) -> Self {
        Self::create_node(NodeType::U).with_child(Self::create_text(text))
    }

    /// Creates a strikethrough text element.
    ///
    /// **Accessibility**: Represents text that is no longer accurate or relevant.
    /// Consider using `<del>` for deleted content with datetime attribute.
    #[inline]
    pub fn s<S: Into<AzString>>(text: S) -> Self {
        Self::create_node(NodeType::S).with_child(Self::create_text(text))
    }

    /// Creates a marked/highlighted text element.
    ///
    /// **Accessibility**: Represents text marked for reference or notation purposes.
    /// Screen readers may announce this as "highlighted".
    #[inline]
    pub fn mark<S: Into<AzString>>(text: S) -> Self {
        Self::create_node(NodeType::Mark).with_child(Self::create_text(text))
    }

    /// Creates a deleted text element.
    ///
    /// **Accessibility**: Represents deleted content in document edits.
    /// Use with `datetime` and `cite` attributes for edit tracking.
    #[inline]
    pub fn del<S: Into<AzString>>(text: S) -> Self {
        Self::create_node(NodeType::Del).with_child(Self::create_text(text))
    }

    /// Creates an inserted text element.
    ///
    /// **Accessibility**: Represents inserted content in document edits.
    /// Use with `datetime` and `cite` attributes for edit tracking.
    #[inline]
    pub fn ins<S: Into<AzString>>(text: S) -> Self {
        Self::create_node(NodeType::Ins).with_child(Self::create_text(text))
    }

    /// Creates a definition element.
    ///
    /// **Accessibility**: Represents the defining instance of a term.
    /// Often used within a definition list or with `<abbr>`.
    #[inline]
    pub fn dfn<S: Into<AzString>>(text: S) -> Self {
        Self::create_node(NodeType::Dfn).with_child(Self::create_text(text))
    }

    /// Creates a time element.
    ///
    /// **Accessibility**: Represents a specific time or date.
    /// Use `datetime` attribute for machine-readable format.
    ///
    /// **Parameters:**
    /// - `text`: Human-readable time/date
    /// - `datetime`: Optional machine-readable datetime
    #[inline]
    pub fn create_time(text: AzString, datetime: OptionString) -> Self {
        let mut element = Self::create_node(NodeType::Time).with_child(Self::create_text(text));
        if let OptionString::Some(dt) = datetime {
            element = element.with_attribute(AttributeType::Custom(AttributeNameValue {
                attr_name: "datetime".into(),
                value: dt,
            }));
        }
        element
    }

    /// Creates a bi-directional override element.
    ///
    /// **Accessibility**: Overrides text direction. Use `dir` attribute (ltr/rtl).
    #[inline]
    pub fn bdo<S: Into<AzString>>(text: S) -> Self {
        Self::create_node(NodeType::Bdo).with_child(Self::create_text(text))
    }

    /// Creates an anchor/hyperlink element.
    ///
    /// **Accessibility**: Always provide meaningful link text. Avoid "click here" or "read more".
    /// Screen readers often navigate by links, so descriptive text is crucial.
    ///
    /// **Parameters:**
    /// - `href`: Link destination URL
    /// - `label`: Link text (pass `None` for image-only links with alt text)
    #[inline]
    pub fn create_a(href: AzString, label: OptionString) -> Self {
        let mut link = Self::create_node(NodeType::A).with_attribute(AttributeType::Href(href));
        if let OptionString::Some(text) = label {
            link = link.with_child(Self::create_text(text));
        }
        link
    }

    /// Creates a button element.
    ///
    /// **Accessibility**: Buttons are keyboard accessible by default. Always provide clear
    /// button text or an `aria-label` for icon-only buttons.
    ///
    /// **Parameters:**
    /// - `text`: Button label text
    #[inline]
    pub fn create_button(text: AzString) -> Self {
        Self::create_node(NodeType::Button).with_child(Self::create_text(text))
    }

    /// Creates a label element for form controls.
    ///
    /// **Accessibility**: Always associate labels with form controls using `for` attribute
    /// or by wrapping the control. This is critical for screen reader users.
    ///
    /// **Parameters:**
    /// - `for_id`: ID of the associated form control
    /// - `text`: Label text
    #[inline]
    pub fn create_label(for_id: AzString, text: AzString) -> Self {
        Self::create_node(NodeType::Label)
            .with_attribute(AttributeType::Custom(AttributeNameValue {
                attr_name: "for".into(),
                value: for_id,
            }))
            .with_child(Self::create_text(text))
    }

    /// Creates an input element.
    ///
    /// **Accessibility**: Always provide a label or `aria-label`. Set appropriate `type`
    /// and `aria-` attributes for the input's purpose.
    ///
    /// **Parameters:**
    /// - `input_type`: Input type (text, password, email, etc.)
    /// - `name`: Form field name
    /// - `label`: Accessibility label (required)
    #[inline]
    pub fn create_input(input_type: AzString, name: AzString, label: AzString) -> Self {
        Self::create_node(NodeType::Input)
            .with_attribute(AttributeType::InputType(input_type))
            .with_attribute(AttributeType::Name(name))
            .with_attribute(AttributeType::AriaLabel(label))
    }

    /// Creates a textarea element.
    ///
    /// **Accessibility**: Always provide a label or `aria-label`. Consider `aria-describedby`
    /// for additional instructions.
    ///
    /// **Parameters:**
    /// - `name`: Form field name
    /// - `label`: Accessibility label (required)
    #[inline]
    pub fn create_textarea(name: AzString, label: AzString) -> Self {
        Self::create_node(NodeType::TextArea)
            .with_attribute(AttributeType::Name(name))
            .with_attribute(AttributeType::AriaLabel(label))
    }

    /// Creates a select dropdown element.
    ///
    /// **Accessibility**: Always provide a label. Group related options with `optgroup`.
    ///
    /// **Parameters:**
    /// - `name`: Form field name
    /// - `label`: Accessibility label (required)
    #[inline]
    pub fn create_select(name: AzString, label: AzString) -> Self {
        Self::create_node(NodeType::Select)
            .with_attribute(AttributeType::Name(name))
            .with_attribute(AttributeType::AriaLabel(label))
    }

    /// Creates an option element for select dropdowns.
    ///
    /// **Parameters:**
    /// - `value`: Option value
    /// - `text`: Display text
    #[inline]
    pub fn create_option(value: AzString, text: AzString) -> Self {
        Self::create_node(NodeType::SelectOption)
            .with_attribute(AttributeType::Value(value))
            .with_child(Self::create_text(text))
    }

    /// Creates an unordered list element.
    ///
    /// **Accessibility**: Screen readers announce lists and item counts, helping users
    /// understand content structure.
    #[inline(always)]
    pub fn create_ul() -> Self {
        Self::create_node(NodeType::Ul)
    }

    /// Creates an ordered list element.
    ///
    /// **Accessibility**: Screen readers announce lists and item counts, helping users
    /// understand content structure and numbering.
    #[inline(always)]
    pub fn create_ol() -> Self {
        Self::create_node(NodeType::Ol)
    }

    /// Creates a list item element.
    ///
    /// **Accessibility**: Must be a child of `ul`, `ol`, or `menu`. Screen readers announce
    /// list item position (e.g., "2 of 5").
    #[inline(always)]
    pub fn create_li() -> Self {
        Self::create_node(NodeType::Li)
    }

    /// Creates a table element.
    ///
    /// **Accessibility**: Use proper table structure with `thead`, `tbody`, `th`, and `td`.
    /// Provide a `caption` for table purpose. Use `scope` attribute on header cells.
    #[inline(always)]
    pub fn create_table() -> Self {
        Self::create_node(NodeType::Table)
    }

    /// Creates a table caption element.
    ///
    /// **Accessibility**: Describes the purpose of the table. Screen readers announce this first.
    #[inline(always)]
    pub fn create_caption() -> Self {
        Self::create_node(NodeType::Caption)
    }

    /// Creates a table header element.
    ///
    /// **Accessibility**: Groups header rows. Screen readers can navigate table structure.
    #[inline(always)]
    pub fn create_thead() -> Self {
        Self::create_node(NodeType::THead)
    }

    /// Creates a table body element.
    ///
    /// **Accessibility**: Groups body rows. Screen readers can navigate table structure.
    #[inline(always)]
    pub fn create_tbody() -> Self {
        Self::create_node(NodeType::TBody)
    }

    /// Creates a table footer element.
    ///
    /// **Accessibility**: Groups footer rows. Screen readers can navigate table structure.
    #[inline(always)]
    pub fn create_tfoot() -> Self {
        Self::create_node(NodeType::TFoot)
    }

    /// Creates a table row element.
    #[inline(always)]
    pub fn create_tr() -> Self {
        Self::create_node(NodeType::Tr)
    }

    /// Creates a table header cell element.
    ///
    /// **Accessibility**: Use `scope` attribute ("col" or "row") to associate headers with
    /// data cells. Screen readers use this to announce cell context.
    #[inline(always)]
    pub fn create_th() -> Self {
        Self::create_node(NodeType::Th)
    }

    /// Creates a table data cell element.
    #[inline(always)]
    pub fn create_td() -> Self {
        Self::create_node(NodeType::Td)
    }

    /// Creates a form element.
    ///
    /// **Accessibility**: Group related form controls with `fieldset` and `legend`.
    /// Provide clear labels for all inputs. Consider `aria-describedby` for instructions.
    #[inline(always)]
    pub fn create_form() -> Self {
        Self::create_node(NodeType::Form)
    }

    /// Creates a fieldset element for grouping form controls.
    ///
    /// **Accessibility**: Groups related form controls. Always include a `legend` as the
    /// first child to describe the group. Screen readers announce the legend when entering
    /// the fieldset.
    #[inline(always)]
    pub fn create_fieldset() -> Self {
        Self::create_node(NodeType::FieldSet)
    }

    /// Creates a legend element for fieldsets.
    ///
    /// **Accessibility**: Describes the purpose of a fieldset. Must be the first child of
    /// a fieldset. Screen readers announce this when entering the fieldset.
    #[inline(always)]
    pub fn create_legend() -> Self {
        Self::create_node(NodeType::Legend)
    }

    /// Creates a horizontal rule element.
    ///
    /// **Accessibility**: Represents a thematic break. Screen readers may announce this as
    /// a separator. Consider using CSS borders for purely decorative lines.
    #[inline(always)]
    pub fn create_hr() -> Self {
        Self::create_node(NodeType::Hr)
    }

    // Additional Element Constructors

    /// Creates an address element.
    ///
    /// **Accessibility**: Represents contact information. Screen readers identify this
    /// as address content.
    #[inline(always)]
    pub const fn create_address() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Address),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a definition list element.
    ///
    /// **Accessibility**: Screen readers announce definition lists and their structure.
    #[inline(always)]
    pub const fn create_dl() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Dl),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a definition term element.
    ///
    /// **Accessibility**: Must be a child of `dl`. Represents the term being defined.
    #[inline(always)]
    pub const fn create_dt() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Dt),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a definition description element.
    ///
    /// **Accessibility**: Must be a child of `dl`. Provides the definition for the term.
    #[inline(always)]
    pub const fn create_dd() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Dd),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a table column group element.
    #[inline(always)]
    pub const fn create_colgroup() -> Self {
        Self {
            root: NodeData::create_node(NodeType::ColGroup),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a table column element.
    #[inline]
    pub fn create_col(span: i32) -> Self {
        Self::create_node(NodeType::Col).with_attribute(AttributeType::ColSpan(span))
    }

    /// Creates an optgroup element for grouping select options.
    ///
    /// **Parameters:**
    /// - `label`: Label for the option group
    #[inline]
    pub fn create_optgroup(label: AzString) -> Self {
        Self::create_node(NodeType::OptGroup).with_attribute(AttributeType::AriaLabel(label))
    }

    /// Creates a quotation element.
    ///
    /// **Accessibility**: Represents an inline quotation.
    #[inline(always)]
    pub const fn create_q() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Q),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates an acronym element.
    ///
    /// **Note**: Deprecated in HTML5. Consider using `abbr()` instead.
    #[inline(always)]
    pub const fn acronym() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Acronym),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a menu element.
    ///
    /// **Accessibility**: Represents a list of commands. Similar to `<ul>` but semantic for
    /// toolbars/menus.
    #[inline(always)]
    pub const fn create_menu() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Menu),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a menu item element.
    ///
    /// **Accessibility**: Represents a command in a menu. Use with appropriate role/aria
    /// attributes.
    #[inline(always)]
    pub const fn menuitem() -> Self {
        Self {
            root: NodeData::create_node(NodeType::MenuItem),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates an output element.
    ///
    /// **Accessibility**: Represents the result of a calculation or user action.
    /// Use `for` attribute to associate with input elements. Screen readers announce updates.
    #[inline(always)]
    pub const fn create_output() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Output),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a progress indicator element.
    ///
    /// **Accessibility**: Represents task progress. Use `value` and `max` attributes.
    /// Screen readers announce progress percentage. Use aria-label to describe the task.
    #[inline]
    pub fn create_progress(value: f32, max: f32) -> Self {
        Self::create_node(NodeType::Progress)
            .with_attribute(AttributeType::Custom(AttributeNameValue {
                attr_name: "value".into(),
                value: value.to_string().into(),
            }))
            .with_attribute(AttributeType::Custom(AttributeNameValue {
                attr_name: "max".into(),
                value: max.to_string().into(),
            }))
    }

    /// Creates a meter gauge element.
    ///
    /// **Accessibility**: Represents a scalar measurement within a known range.
    /// Use `value`, `min`, `max`, `low`, `high`, `optimum` attributes.
    /// Screen readers announce the measurement. Provide aria-label for context.
    #[inline]
    pub fn create_meter(value: f32, min: f32, max: f32) -> Self {
        Self::create_node(NodeType::Meter)
            .with_attribute(AttributeType::Custom(AttributeNameValue {
                attr_name: "value".into(),
                value: value.to_string().into(),
            }))
            .with_attribute(AttributeType::Custom(AttributeNameValue {
                attr_name: "min".into(),
                value: min.to_string().into(),
            }))
            .with_attribute(AttributeType::Custom(AttributeNameValue {
                attr_name: "max".into(),
                value: max.to_string().into(),
            }))
    }

    /// Creates a datalist element for input suggestions.
    ///
    /// **Accessibility**: Provides autocomplete options for inputs.
    /// Associate with input using `list` attribute. Screen readers announce available options.
    #[inline(always)]
    pub const fn create_datalist() -> Self {
        Self {
            root: NodeData::create_node(NodeType::DataList),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    // Embedded Content Elements

    /// Creates a canvas element for graphics.
    ///
    /// **Accessibility**: Canvas content is not accessible by default.
    /// Always provide fallback content as children and/or detailed aria-label.
    /// Consider using SVG for accessible graphics when possible.
    #[inline(always)]
    pub const fn create_canvas() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Canvas),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates an object element for embedded content.
    ///
    /// **Accessibility**: Provide fallback content as children. Use aria-label to describe content.
    #[inline(always)]
    pub const fn create_object() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Object),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a param element for object parameters.
    ///
    /// **Parameters:**
    /// - `name`: Parameter name
    /// - `value`: Parameter value
    #[inline]
    pub fn create_param(name: AzString, value: AzString) -> Self {
        Self::create_node(NodeType::Param)
            .with_attribute(AttributeType::Name(name))
            .with_attribute(AttributeType::Value(value))
    }

    /// Creates an embed element.
    ///
    /// **Accessibility**: Provide alternative content or link. Use aria-label to describe embedded
    /// content.
    #[inline(always)]
    pub const fn create_embed() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Embed),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates an audio element.
    ///
    /// **Accessibility**: Always provide controls. Use `<track>` for captions/subtitles.
    /// Provide fallback text for unsupported browsers.
    #[inline(always)]
    pub const fn create_audio() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Audio),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a video element.
    ///
    /// **Accessibility**: Always provide controls. Use `<track>` for
    /// captions/subtitles/descriptions. Provide fallback text. Consider providing transcript.
    #[inline(always)]
    pub const fn create_video() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Video),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a source element for media.
    ///
    /// **Parameters:**
    /// - `src`: Media source URL
    /// - `media_type`: MIME type (e.g., "video/mp4", "audio/ogg")
    #[inline]
    pub fn create_source(src: AzString, media_type: AzString) -> Self {
        Self::create_node(NodeType::Source)
            .with_attribute(AttributeType::Src(src))
            .with_attribute(AttributeType::Custom(AttributeNameValue {
                attr_name: "type".into(),
                value: media_type,
            }))
    }

    /// Creates a track element for media captions/subtitles.
    ///
    /// **Accessibility**: Essential for deaf/hard-of-hearing users and non-native speakers.
    /// Use `kind` (subtitles/captions/descriptions), `srclang`, and `label` attributes.
    ///
    /// **Parameters:**
    /// - `src`: Track file URL (WebVTT format)
    /// - `kind`: Track kind ("subtitles", "captions", "descriptions", "chapters", "metadata")
    #[inline]
    pub fn create_track(src: AzString, kind: AzString) -> Self {
        Self::create_node(NodeType::Track)
            .with_attribute(AttributeType::Src(src))
            .with_attribute(AttributeType::Custom(AttributeNameValue {
                attr_name: "kind".into(),
                value: kind,
            }))
    }

    /// Creates a map element for image maps.
    ///
    /// **Accessibility**: Provide text alternatives. Ensure all areas have alt text.
    #[inline(always)]
    pub const fn create_map() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Map),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates an area element for image map regions.
    ///
    /// **Accessibility**: Always provide `alt` text describing the region/link purpose.
    /// Keyboard users should be able to navigate areas.
    #[inline(always)]
    pub const fn create_area() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Area),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    // Metadata Elements

    /// Creates a title element for document title.
    ///
    /// **Accessibility**: Required for all pages. Screen readers announce this first.
    /// Should be unique and descriptive. Keep under 60 characters.
    #[inline]
    pub fn title<S: Into<AzString>>(text: S) -> Self {
        Self::create_node(NodeType::Title).with_child(Self::create_text(text))
    }

    /// Creates a meta element.
    ///
    /// **Accessibility**: Use for charset, viewport, description. Crucial for proper text display.
    #[inline(always)]
    pub const fn meta() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Meta),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a link element for external resources.
    ///
    /// **Accessibility**: Use for stylesheets, icons, alternate versions.
    /// Provide meaningful `title` attribute for alternate stylesheets.
    #[inline(always)]
    pub const fn create_link() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Link),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a script element.
    ///
    /// **Accessibility**: Ensure scripted content is accessible.
    /// Provide noscript fallbacks for critical functionality.
    #[inline(always)]
    pub const fn create_script() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Script),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a style element for embedded CSS.
    ///
    /// **Note**: In Azul, use the `.style()` method instead for styling.
    /// This creates a `<style>` HTML element for embedded stylesheets.
    #[inline(always)]
    pub const fn style_element() -> Self {
        Self {
            root: NodeData::create_node(NodeType::Style),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        }
    }

    /// Creates a base element for document base URL.
    ///
    /// **Parameters:**
    /// - `href`: Base URL for relative URLs in the document
    #[inline]
    pub fn base(href: AzString) -> Self {
        Self::create_node(NodeType::Base).with_attribute(AttributeType::Href(href))
    }

    // Advanced Constructors with Parameters

    /// Creates a table header cell with scope.
    ///
    /// **Parameters:**
    /// - `scope`: "col", "row", "colgroup", or "rowgroup"
    /// - `text`: Header text
    ///
    /// **Accessibility**: The scope attribute is crucial for associating headers with data cells.
    #[inline]
    pub fn th_with_scope(scope: AzString, text: AzString) -> Self {
        Self::create_node(NodeType::Th)
            .with_attribute(AttributeType::Scope(scope))
            .with_child(Self::create_text(text))
    }

    /// Creates a table data cell with text.
    ///
    /// **Parameters:**
    /// - `text`: Cell content
    #[inline]
    pub fn td_with_text<S: Into<AzString>>(text: S) -> Self {
        Self::create_td().with_child(Self::create_text(text))
    }

    /// Creates a table header cell with text.
    ///
    /// **Parameters:**
    /// - `text`: Header text
    #[inline]
    pub fn th_with_text<S: Into<AzString>>(text: S) -> Self {
        Self::create_th().with_child(Self::create_text(text))
    }

    /// Creates a list item with text.
    ///
    /// **Parameters:**
    /// - `text`: List item content
    #[inline]
    pub fn li_with_text<S: Into<AzString>>(text: S) -> Self {
        Self::create_li().with_child(Self::create_text(text))
    }

    /// Creates a paragraph with text.
    ///
    /// **Parameters:**
    /// - `text`: Paragraph content
    #[inline]
    pub fn p_with_text<S: Into<AzString>>(text: S) -> Self {
        Self::create_p().with_child(Self::create_text(text))
    }

    // Accessibility-Aware Constructors
    // These constructors require explicit accessibility information.

    /// Creates a button with text content and accessibility information.
    ///
    /// **Parameters:**
    /// - `text`: The visible button text
    /// - `aria`: Accessibility information (role, description, etc.)
    #[inline]
    pub fn button_with_aria<S: Into<AzString>>(text: S, aria: SmallAriaInfo) -> Self {
        let mut btn = Self::create_button(text.into());
        btn.root.set_accessibility_info(aria.to_full_info());
        btn
    }

    /// Creates a link (anchor) with href, text, and accessibility information.
    ///
    /// **Parameters:**
    /// - `href`: The link destination
    /// - `text`: The visible link text
    /// - `aria`: Accessibility information (expanded description, etc.)
    #[inline]
    pub fn link_with_aria<S1: Into<AzString>, S2: Into<AzString>>(
        href: S1,
        text: S2,
        aria: SmallAriaInfo,
    ) -> Self {
        let mut link = Self::create_a(href.into(), OptionString::Some(text.into()));
        link.root.set_accessibility_info(aria.to_full_info());
        link
    }

    /// Creates an input element with type, name, and accessibility information.
    ///
    /// **Parameters:**
    /// - `input_type`: The input type (text, password, email, etc.)
    /// - `name`: The form field name
    /// - `label`: Base accessibility label
    /// - `aria`: Additional accessibility information (description, etc.)
    #[inline]
    pub fn input_with_aria<S1: Into<AzString>, S2: Into<AzString>, S3: Into<AzString>>(
        input_type: S1,
        name: S2,
        label: S3,
        aria: SmallAriaInfo,
    ) -> Self {
        let mut input = Self::create_input(input_type.into(), name.into(), label.into());
        input.root.set_accessibility_info(aria.to_full_info());
        input
    }

    /// Creates a textarea with name and accessibility information.
    ///
    /// **Parameters:**
    /// - `name`: The form field name
    /// - `label`: Base accessibility label
    /// - `aria`: Additional accessibility information (description, etc.)
    #[inline]
    pub fn textarea_with_aria<S1: Into<AzString>, S2: Into<AzString>>(
        name: S1,
        label: S2,
        aria: SmallAriaInfo,
    ) -> Self {
        let mut textarea = Self::create_textarea(name.into(), label.into());
        textarea.root.set_accessibility_info(aria.to_full_info());
        textarea
    }

    /// Creates a select dropdown with name and accessibility information.
    ///
    /// **Parameters:**
    /// - `name`: The form field name
    /// - `label`: Base accessibility label
    /// - `aria`: Additional accessibility information (description, etc.)
    #[inline]
    pub fn select_with_aria<S1: Into<AzString>, S2: Into<AzString>>(
        name: S1,
        label: S2,
        aria: SmallAriaInfo,
    ) -> Self {
        let mut select = Self::create_select(name.into(), label.into());
        select.root.set_accessibility_info(aria.to_full_info());
        select
    }

    /// Creates a table with caption and accessibility information.
    ///
    /// **Parameters:**
    /// - `caption`: Table caption (visible title)
    /// - `aria`: Accessibility information describing table purpose
    #[inline]
    pub fn table_with_aria<S: Into<AzString>>(caption: S, aria: SmallAriaInfo) -> Self {
        let mut table = Self::create_table()
            .with_child(Self::create_caption().with_child(Self::create_text(caption)));
        table.root.set_accessibility_info(aria.to_full_info());
        table
    }

    /// Creates a label for a form control with additional accessibility information.
    ///
    /// **Parameters:**
    /// - `for_id`: The ID of the associated form control
    /// - `text`: The visible label text
    /// - `aria`: Additional accessibility information (description, etc.)
    #[inline]
    pub fn label_with_aria<S1: Into<AzString>, S2: Into<AzString>>(
        for_id: S1,
        text: S2,
        aria: SmallAriaInfo,
    ) -> Self {
        let mut label = Self::create_label(for_id.into(), text.into());
        label.root.set_accessibility_info(aria.to_full_info());
        label
    }

    /// Parse XML/XHTML string into a DOM
    ///
    /// This is a simple wrapper that parses XML and converts it to a DOM.
    /// For now, it just creates a text node with the content since full XML parsing
    /// requires the xml feature and more complex parsing logic.
    #[cfg(feature = "xml")]
    pub fn from_xml<S: AsRef<str>>(xml_str: S) -> Self {
        // TODO: Implement full XML parsing
        // For now, just create a text node showing that XML was loaded
        Self::create_text(format!(
            "XML content loaded ({} bytes)",
            xml_str.as_ref().len()
        ))
    }

    /// Parse XML/XHTML string into a DOM (fallback without xml feature)
    #[cfg(not(feature = "xml"))]
    pub fn from_xml<S: AsRef<str>>(xml_str: S) -> Self {
        Self::create_text(format!(
            "XML parsing requires 'xml' feature ({} bytes)",
            xml_str.as_ref().len()
        ))
    }

    // Swaps `self` with a default DOM, necessary for builder methods
    #[inline(always)]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self {
            root: NodeData::create_div(),
            children: DomVec::from_const_slice(&[]),
            css: azul_css::css::CssVec::from_const_slice(&[]),
            estimated_total_children: 0,
        };
        mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn add_child(&mut self, child: Dom) {
        let estimated = child.estimated_total_children;
        let mut v: DomVec = Vec::new().into();
        mem::swap(&mut v, &mut self.children);
        let mut v = v.into_library_owned_vec();
        v.push(child);
        self.children = v.into();
        self.estimated_total_children += estimated + 1;
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
            css: self.css.clone(),
            estimated_total_children: self.estimated_total_children,
        }
    }
    pub fn node_count(&self) -> usize {
        self.estimated_total_children + 1
    }

    /// Push a CSS stylesheet onto this DOM node's stylesheet list.
    /// The CSS will be applied during the deferred cascade pass in `regenerate_layout()`.
    /// Later `.style()` calls have higher cascade priority (override earlier ones).
    pub fn style(&mut self, css: azul_css::css::Css) {
        let mut v = Vec::new().into();
        core::mem::swap(&mut v, &mut self.css);
        let mut v: Vec<azul_css::css::Css> = v.into_library_owned_vec();
        v.push(css);
        self.css = v.into();
    }
    #[inline(always)]
    pub fn with_children(mut self, children: DomVec) -> Self {
        self.set_children(children);
        self
    }
    #[inline(always)]
    pub fn with_child(mut self, child: Self) -> Self {
        self.add_child(child);
        self
    }
    #[inline(always)]
    pub fn with_node_type(mut self, node_type: NodeType) -> Self {
        self.root.set_node_type(node_type);
        self
    }
    #[inline(always)]
    pub fn with_id(mut self, id: AzString) -> Self {
        self.root.add_id(id);
        self
    }
    #[inline(always)]
    pub fn with_class(mut self, class: AzString) -> Self {
        self.root.add_class(class);
        self
    }
    #[inline(always)]
    pub fn with_callback<C: Into<CoreCallback>>(
        mut self,
        event: EventFilter,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.root.add_callback(event, data, callback);
        self
    }
    /// Add a CSS property with optional conditions (hover, focus, active, etc.)
    #[inline(always)]
    pub fn with_css_property(mut self, prop: CssPropertyWithConditions) -> Self {
        self.root.add_css_property(prop);
        self
    }
    /// Add a CSS property with optional conditions (hover, focus, active, etc.)
    #[inline(always)]
    pub fn add_css_property(&mut self, prop: CssPropertyWithConditions) {
        self.root.add_css_property(prop);
    }
    #[inline(always)]
    pub fn add_class(&mut self, class: AzString) {
        self.root.add_class(class);
    }
    #[inline(always)]
    pub fn add_callback<C: Into<CoreCallback>>(
        &mut self,
        event: EventFilter,
        data: RefAny,
        callback: C,
    ) {
        self.root.add_callback(event, data, callback);
    }
    #[inline(always)]
    pub fn set_tab_index(&mut self, tab_index: TabIndex) {
        self.root.set_tab_index(tab_index);
    }
    #[inline(always)]
    pub fn set_contenteditable(&mut self, contenteditable: bool) {
        self.root.set_contenteditable(contenteditable);
    }
    #[inline(always)]
    pub fn with_tab_index(mut self, tab_index: TabIndex) -> Self {
        self.root.set_tab_index(tab_index);
        self
    }
    #[inline(always)]
    pub fn with_contenteditable(mut self, contenteditable: bool) -> Self {
        self.root.set_contenteditable(contenteditable);
        self
    }
    #[inline]
    pub fn with_dataset(mut self, data: OptionRefAny) -> Self {
        self.root.set_dataset(data);
        self
    }
    #[inline(always)]
    pub fn with_ids_and_classes(mut self, ids_and_classes: IdOrClassVec) -> Self {
        self.root.set_ids_and_classes(ids_and_classes);
        self
    }

    /// Adds an attribute to this DOM element.
    #[inline(always)]
    pub fn with_attribute(mut self, attr: AttributeType) -> Self {
        let mut attrs = self.root.attributes.clone();
        let mut v = attrs.into_library_owned_vec();
        v.push(attr);
        self.root.attributes = v.into();
        self
    }

    /// Adds multiple attributes to this DOM element.
    #[inline(always)]
    pub fn with_attributes(mut self, attributes: AttributeTypeVec) -> Self {
        self.root.attributes = attributes;
        self
    }

    #[inline(always)]
    pub fn with_callbacks(mut self, callbacks: CoreCallbackDataVec) -> Self {
        self.root.callbacks = callbacks;
        self
    }
    #[inline(always)]
    pub fn with_css_props(mut self, css_props: CssPropertyWithConditionsVec) -> Self {
        self.root.css_props = css_props;
        self
    }

    /// Assigns a stable key to the root node of this DOM for reconciliation.
    ///
    /// This is crucial for performance and correct state preservation when
    /// lists of items change order or items are inserted/removed.
    ///
    /// # Example
    /// ```rust
    /// # use azul_core::dom::Dom;
    /// Dom::create_div()
    ///     .with_key("user-avatar-123");
    /// ```
    #[inline]
    pub fn with_key<K: core::hash::Hash>(mut self, key: K) -> Self {
        self.root.set_key(key);
        self
    }

    /// Registers a callback to merge dataset state from the previous frame.
    ///
    /// This is used for components that maintain heavy internal state (video players,
    /// WebGL contexts, network connections) that should not be destroyed and recreated
    /// on every render frame.
    ///
    /// The callback receives both datasets as `RefAny` (cheap shallow clones) and
    /// returns the `RefAny` that should be used for the new node.
    #[inline]
    pub fn with_merge_callback<C: Into<DatasetMergeCallback>>(mut self, callback: C) -> Self {
        self.root.set_merge_callback(callback);
        self
    }

    pub fn set_inline_style(&mut self, style: &str) {
        self.root.set_inline_style(style);
    }

    pub fn with_inline_style(mut self, style: &str) -> Self {
        self.set_inline_style(style);
        self
    }
    
    /// Parse and set CSS styles with full selector support.
    /// 
    /// This is the unified API for setting inline CSS on a DOM node. It supports:
    /// - Simple properties: `color: red; font-size: 14px;`
    /// - Pseudo-selectors: `:hover { background: blue; }`
    /// - @-rules: `@os linux { font-size: 14px; }`
    /// - Nesting: `@os linux { font-size: 14px; :hover { color: red; }}`
    /// 
    /// # Examples
    /// ```rust
    /// # use azul_core::dom::Dom;
    /// // Simple inline styles
    /// Dom::create_div().with_css("color: red; font-size: 14px;");
    /// 
    /// // With hover and active states
    /// Dom::create_div().with_css("
    ///     color: blue;
    ///     :hover { color: red; }
    ///     :active { color: green; }
    /// ");
    /// 
    /// // OS-specific with nested hover
    /// Dom::create_div().with_css("
    ///     font-size: 12px;
    ///     @os linux { font-size: 14px; :hover { color: red; }}
    ///     @os windows { font-size: 13px; }
    /// ");
    /// ```
    pub fn set_css(&mut self, style: &str) {
        let parsed = CssPropertyWithConditionsVec::parse(style);
        let mut current = Vec::new().into();
        mem::swap(&mut current, &mut self.root.css_props);
        let mut v = current.into_library_owned_vec();
        v.extend(parsed.into_library_owned_vec());
        self.root.css_props = v.into();
    }
    
    /// Builder method for `set_css`
    pub fn with_css(mut self, style: &str) -> Self {
        self.set_css(style);
        self
    }

    /// Sets inline CSS styles for the hover state on the root node
    /// 
    /// Deprecated: Use `with_css(":hover { ... }")` instead
    pub fn set_inline_hover_style(&mut self, style: &str) {
        self.root.set_inline_hover_style(style);
    }

    /// Builder method for setting inline CSS styles for the hover state
    pub fn with_inline_hover_style(mut self, style: &str) -> Self {
        self.set_inline_hover_style(style);
        self
    }

    /// Sets inline CSS styles for the active state on the root node
    pub fn set_inline_active_style(&mut self, style: &str) {
        self.root.set_inline_active_style(style);
    }

    /// Builder method for setting inline CSS styles for the active state
    pub fn with_inline_active_style(mut self, style: &str) -> Self {
        self.set_inline_active_style(style);
        self
    }

    /// Sets inline CSS styles for the focus state on the root node
    pub fn set_inline_focus_style(&mut self, style: &str) {
        self.root.set_inline_focus_style(style);
    }

    /// Builder method for setting inline CSS styles for the focus state
    pub fn with_inline_focus_style(mut self, style: &str) -> Self {
        self.set_inline_focus_style(style);
        self
    }

    /// Sets the context menu for the root node
    #[inline]
    pub fn set_context_menu(&mut self, context_menu: Menu) {
        self.root.set_context_menu(context_menu);
    }

    #[inline]
    pub fn with_context_menu(mut self, context_menu: Menu) -> Self {
        self.set_context_menu(context_menu);
        self
    }

    /// Sets the menu bar for the root node
    #[inline]
    pub fn set_menu_bar(&mut self, menu_bar: Menu) {
        self.root.set_menu_bar(menu_bar);
    }

    #[inline]
    pub fn with_menu_bar(mut self, menu_bar: Menu) -> Self {
        self.set_menu_bar(menu_bar);
        self
    }

    #[inline]
    pub fn with_clip_mask(mut self, clip_mask: ImageMask) -> Self {
        self.root.set_clip_mask(clip_mask);
        self
    }

    #[inline]
    pub fn with_accessibility_info(mut self, accessibility_info: AccessibilityInfo) -> Self {
        self.root.set_accessibility_info(accessibility_info);
        self
    }

    pub fn fixup_children_estimated(&mut self) -> usize {
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
            root: NodeData::create_div(),
            children: children.into(),
            css: azul_css::css::CssVec::from_const_slice(&[]),
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
