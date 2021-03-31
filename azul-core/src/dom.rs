use core::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
    iter::FromIterator,
};
use alloc::vec::Vec;
use alloc::string::String;
use alloc::collections::btree_map::BTreeMap;
use crate::{
    styled_dom::{CssPropertyCache, StyledNodeState},
    callbacks::{
        Callback,
        IFrameCallback, IFrameCallbackType,
        RefAny, OptionRefAny,
    },
    app_resources::{ImageId, OptionImageMask},
    id_tree::{
        NodeDataContainer, NodeDataContainerRef,
        NodeDataContainerRefMut
    },
    styled_dom::StyledDom,
};
#[cfg(feature = "opengl")]
use crate::callbacks::{GlCallback, GlCallbackType};
use azul_css::{Css, AzString, NodeTypeTag, CssProperty};

pub use crate::id_tree::{NodeHierarchy, Node, NodeId};

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
#[derive(Debug, PartialEq, Hash, Eq, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum NodeType {
    /// Same as div, but only for the root node
    Body,
    /// Regular div with no particular type of data attached
    Div,
    /// Creates a line break in an inline text layout
    Br,
    /// A string of text
    Label(AzString),
    /// An image of an opaque type. Images can be cached
    /// in the AppResources or recreated on every redraw.
    Image(ImageRef),
    /// Callback that renders a DOM which gets passed its
    /// width / height after the layout step, necessary to render
    /// infinite datastructures
    IFrame(IFrameNode),
}

impl NodeType {
    fn into_library_owned_nodetype(&mut self) -> Self {
        use self::NodeType::*;
        match self {
            Div => Div,
            Body => Body,
            Br => Br,
            Label(s) => Label(s.clone_self()),
            Image(i) => Image(*i),
            IFrame(i) => IFrame(IFrameNode {
                callback: i.callback,
                data: i.data.clone_into_library_memory(),
            }),
            #[cfg(feature = "opengl")]
            GlTexture(gl) => GlTexture(GlTextureNode {
                callback: gl.callback,
                data: gl.data.clone_into_library_memory(),
            })
        }
    }

    pub(crate) fn format(&self) -> Option<String> {
        use self::NodeType::*;
        match self {
            Div | Body | Br => None,
            Label(s) => Some(format!("{}", s)),
            Image(id) => Some(format!("image({:?})", id)),
            IFrame(i) => Some(format!("iframe({:?})", i)),
            #[cfg(feature = "opengl")]
            GlTexture(g) => Some(format!("gltexture({:?})", g)),
        }
    }

    #[inline]
    pub fn get_path(&self) -> NodeTypeTag {
        use self::NodeType::*;
        match self {
            Div => NodeTypeTag::Div,
            Body => NodeTypeTag::Body,
            Br => NodeTypeTag::Br,
            Label(_) => NodeTypeTag::P,
            Image(_) => NodeTypeTag::Img,
            #[cfg(feature = "opengl")]
            GlTexture(_) => NodeTypeTag::Texture,
            IFrame(_) => NodeTypeTag::IFrame,
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
    /// You can get all currently pressed virtual keycodes in the `keyboard_state.current_virtual_keycodes`
    /// and / or just the last keycode in the `keyboard_state.latest_virtual_keycode`.
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
///
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
/// ```rust
/// enum A {
///    Abc(AbcType),
/// }
///
/// struct AbcType { }
///
/// impl A {
///     // fn as_abc_type(&self) -> Option<AbcType>
///     get_single_enum_type!(as_abc_type, A::Abc(AbcType));
/// }
/// ```
macro_rules! get_single_enum_type {
    ($fn_name:ident, $enum_name:ident::$variant:ident($return_type:ty)) => (
        pub fn $fn_name(&self) -> Option<$return_type> {
            use self::$enum_name::*;
            match self {
                $variant(e) => Some(*e),
                _ => None,
            }
        }
    )
}

impl EventFilter {
    get_single_enum_type!(as_hover_event_filter, EventFilter::Hover(HoverEventFilter));
    get_single_enum_type!(as_focus_event_filter, EventFilter::Focus(FocusEventFilter));
    get_single_enum_type!(as_not_event_filter, EventFilter::Not(NotEventFilter));
    get_single_enum_type!(as_window_event_filter, EventFilter::Window(WindowEventFilter));
}

impl From<On> for EventFilter {
    fn from(input: On) -> EventFilter {
        use self::On::*;
        match input {
            MouseOver            => EventFilter::Hover(HoverEventFilter::MouseOver),
            MouseDown            => EventFilter::Hover(HoverEventFilter::MouseDown),
            LeftMouseDown        => EventFilter::Hover(HoverEventFilter::LeftMouseDown),
            MiddleMouseDown      => EventFilter::Hover(HoverEventFilter::MiddleMouseDown),
            RightMouseDown       => EventFilter::Hover(HoverEventFilter::RightMouseDown),
            MouseUp              => EventFilter::Hover(HoverEventFilter::MouseUp),
            LeftMouseUp          => EventFilter::Hover(HoverEventFilter::LeftMouseUp),
            MiddleMouseUp        => EventFilter::Hover(HoverEventFilter::MiddleMouseUp),
            RightMouseUp         => EventFilter::Hover(HoverEventFilter::RightMouseUp),

            MouseEnter           => EventFilter::Hover(HoverEventFilter::MouseEnter),
            MouseLeave           => EventFilter::Hover(HoverEventFilter::MouseLeave),
            Scroll               => EventFilter::Hover(HoverEventFilter::Scroll),
            TextInput            => EventFilter::Focus(FocusEventFilter::TextInput),            // focus!
            VirtualKeyDown       => EventFilter::Window(WindowEventFilter::VirtualKeyDown),     // window!
            VirtualKeyUp         => EventFilter::Window(WindowEventFilter::VirtualKeyUp),       // window!
            HoveredFile          => EventFilter::Hover(HoverEventFilter::HoveredFile),
            DroppedFile          => EventFilter::Hover(HoverEventFilter::DroppedFile),
            HoveredFileCancelled => EventFilter::Hover(HoverEventFilter::HoveredFileCancelled),
            FocusReceived        => EventFilter::Focus(FocusEventFilter::FocusReceived),        // focus!
            FocusLost            => EventFilter::Focus(FocusEventFilter::FocusLost),            // focus!
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
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum ComponentEventFilter {
    AfterMount,
    BeforeUnmount,
    NodeResized,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum ApplicationEventFilter {
    DeviceConnected,
    DeviceDisconnected,
    // ... TODO: more events
}

#[cfg(feature = "opengl")]
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct GlTextureNode {
    pub callback: GlCallback,
    pub data: RefAny,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct IFrameNode {
    pub callback: IFrameCallback,
    pub data: RefAny,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CallbackData {
    pub event: EventFilter,
    pub callback: Callback,
    pub data: RefAny,
}

impl CallbackData {
    // Copies the internal RefAny
    pub(crate) fn copy_special(&mut self) -> Self {
        Self {
            event: self.event,
            callback: self.callback.clone(),
            data: self.data.clone_into_library_memory(),
        }
    }
}

impl_vec!(CallbackData, CallbackDataVec, CallbackDataVecDestructor);
impl_vec_mut!(CallbackData, CallbackDataVec);
impl_vec_debug!(CallbackData, CallbackDataVec);
impl_vec_partialord!(CallbackData, CallbackDataVec);
impl_vec_ord!(CallbackData, CallbackDataVec);
impl_vec_partialeq!(CallbackData, CallbackDataVec);
impl_vec_eq!(CallbackData, CallbackDataVec);
impl_vec_hash!(CallbackData, CallbackDataVec);

impl CallbackDataVec {
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, CallbackData> {
        NodeDataContainerRef { internal: self.as_ref() }
    }
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, CallbackData> {
        NodeDataContainerRefMut { internal: self.as_mut() }
    }
    pub(crate) fn into_library_owned_vec(&mut self) -> Vec<CallbackData> {
        let mut vec = Vec::with_capacity(self.as_ref().len());
        for item in self.as_mut().iter_mut() {
            vec.push(item.copy_special());
        }
        vec
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum NodeDataInlineCssProperty {
    Normal(CssProperty),
    Active(CssProperty),
    Focus(CssProperty),
    Hover(CssProperty),
}

impl_vec!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec, NodeDataInlineCssPropertyVecDestructor);
impl_vec_debug!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec);
impl_vec_partialord!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec);
impl_vec_ord!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec);
impl_vec_clone!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec, NodeDataInlineCssPropertyVecDestructor);
impl_vec_partialeq!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec);
impl_vec_eq!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec);
impl_vec_hash!(NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec);

/// Represents one single DOM node (node type, classes, ids and callbacks are stored here)
#[repr(C)]
#[derive(PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    /// Optional clip mask for this DOM node
    pub(crate) clip_mask: OptionImageMask,
    /// Whether this div can be focused, and if yes, in what default to `None` (not focusable).
    /// Note that without this, there can be no `On::FocusReceived` (equivalent to onfocus),
    /// `On::FocusLost` (equivalent to onblur), etc. events.
    pub(crate) tab_index: OptionTabIndex,
}

impl NodeData {
    #[inline]
    pub fn copy_special(&mut self) -> Self {
        Self {
            node_type: self.node_type.into_library_owned_nodetype(),
            dataset: match &mut self.dataset {
                OptionRefAny::None => OptionRefAny::None,
                OptionRefAny::Some(s) => OptionRefAny::Some(s.clone_into_library_memory()),
            },
            ids_and_classes: self.ids_and_classes.clone(), // do not clone the IDs and classes if they are &'static
            inline_css_props: self.inline_css_props.clone(), // do not clone the inline CSS props if they are &'static
            callbacks: self.callbacks.into_library_owned_vec().into(),
            clip_mask: self.clip_mask.clone(),
            tab_index: self.tab_index.clone(),
        }
    }

    pub fn is_focusable(&self) -> bool {
        // TODO: do some better analysis of next / first / item
        self.tab_index.is_some() || self.get_callbacks().iter().any(|cb| cb.event.is_focus_callback())
    }

    pub fn get_iframe_node(&mut self) -> Option<&mut IFrameNode> {
        match &mut self.node_type {
            NodeType::IFrame(i) => Some(i),
            _ => None,
        }
    }
}

// Clone, PartialEq, Eq, Hash, PartialOrd, Ord
impl_vec!(NodeData, NodeDataVec, NodeDataVecDestructor);
impl_vec_mut!(NodeData, NodeDataVec);
impl_vec_debug!(NodeData, NodeDataVec);
impl_vec_partialord!(NodeData, NodeDataVec);
impl_vec_ord!(NodeData, NodeDataVec);
impl_vec_partialeq!(NodeData, NodeDataVec);
impl_vec_eq!(NodeData, NodeDataVec);
impl_vec_hash!(NodeData, NodeDataVec);

impl NodeDataVec {
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, NodeData> {
        NodeDataContainerRef { internal: self.as_ref() }
    }
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, NodeData> {
        NodeDataContainerRefMut { internal: self.as_mut() }
    }
    pub fn into_library_owned_vec(&mut self) -> Vec<NodeData> {
        let mut vec = Vec::with_capacity(self.as_ref().len());
        for item in self.as_mut().iter_mut() {
            vec.push(item.copy_special());
        }
        vec
    }

    // necessary so that the callbacks have mutable access to the NodeType while
    // at the same time the library has mutable access to the CallbackDataVec
    pub fn split_into_callbacks_and_dataset<'a>(&'a mut self) -> (BTreeMap<NodeId, &'a mut CallbackDataVec>, BTreeMap<NodeId, &'a mut RefAny>) {

        let mut a = BTreeMap::new();
        let mut b = BTreeMap::new();

        for (node_id, node_data) in self.as_mut().iter_mut().enumerate() {
            let a_map = &mut node_data.callbacks;
            let b_map = &mut node_data.dataset;
            if !a_map.is_empty() {
                a.insert(NodeId::new(node_id), a_map);
            }
            if let OptionRefAny::Some(s) = b_map {
                b.insert(NodeId::new(node_id), s);
            }
        }

        (a, b)
    }
}

unsafe impl Send for NodeData { }

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
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

impl_option!(TabIndex, OptionTabIndex, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

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

impl fmt::Debug for NodeData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl fmt::Display for NodeData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {

        let html_type = self.node_type.get_path();
        let attributes_string = node_data_to_string(&self);

        match self.node_type.format() {
            Some(content) => write!(f, "<{}{}>{}</{}>", html_type, attributes_string, content, html_type),
            None => write!(f, "<{}{}/>", html_type, attributes_string)
        }
    }
}

fn node_data_to_string(node_data: &NodeData) -> String {

    let mut id_string = String::new();
    let ids = node_data.ids_and_classes.as_ref().iter().filter_map(|s| s.as_id()).collect::<Vec<_>>().join(" ");
    if !ids.is_empty() {
        id_string = format!(" id=\"{}\" ", ids);
    }

    let mut class_string = String::new();
    let classes = node_data.ids_and_classes.as_ref().iter().filter_map(|s| s.as_class()).collect::<Vec<_>>().join(" ");
    if !classes.is_empty() {
        class_string = format!(" class=\"{}\" ", classes);
    }

    let mut tabindex_string = String::new();
    if let OptionTabIndex::Some(tab_index) = node_data.tab_index {
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
            clip_mask: OptionImageMask::None,
            tab_index: OptionTabIndex::None,
        }
    }

    /// Checks whether this node is of the given node type (div, image, text)
    #[inline]
    pub fn is_node_type(&self, searched_type: NodeType) -> bool {
        self.node_type == searched_type
    }

    /// Checks whether this node has the searched ID attached
    pub fn has_id(&self, id: &str) -> bool {
        self.ids_and_classes.iter().any(|id_or_class| id_or_class.as_id() == Some(id))
    }

    /// Checks whether this node has the searched class attached
    pub fn has_class(&self, class: &str) -> bool {
        self.ids_and_classes.iter().any(|id_or_class| id_or_class.as_class() == Some(class))
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
        Self::new(NodeType::Div)
    }

    /// Shorthand for `NodeData::new(NodeType::Label(value.into()))`
    #[inline(always)]
    pub fn label<S: Into<AzString>>(value: S) -> Self {
        Self::new(NodeType::Label(value.into()))
    }

    /// Shorthand for `NodeData::new(NodeType::Image(image_id))`
    #[inline(always)]
    pub fn image(image: ImageId) -> Self {
        Self::new(NodeType::Image(image))
    }

    #[inline(always)]
    #[cfg(feature = "opengl")]
    pub fn gl_texture(data: RefAny, callback: GlCallbackType) -> Self {
        Self::new(NodeType::GlTexture(GlTextureNode { callback: GlCallback { cb: callback }, data }))
    }

    #[inline(always)]
    pub fn iframe(data: RefAny, callback: IFrameCallbackType) -> Self {
        Self::new(NodeType::IFrame(IFrameNode { callback: IFrameCallback { cb: callback }, data }))
    }

    // NOTE: Getters are used here in order to allow changing the memory allocator for the NodeData
    // in the future (which is why the fields are all private).

    #[inline(always)]
    pub const fn get_node_type(&self) -> &NodeType { &self.node_type }
    #[inline(always)]
    pub const fn get_dataset(&self) -> &OptionRefAny { &self.dataset }
    #[inline(always)]
    pub const fn get_ids_and_classes(&self) -> &IdOrClassVec { &self.ids_and_classes }
    #[inline(always)]
    pub const fn get_callbacks(&self) -> &CallbackDataVec { &self.callbacks }
    #[inline(always)]
    pub const fn get_inline_css_props(&self) -> &NodeDataInlineCssPropertyVec { &self.inline_css_props }
    #[inline(always)]
    pub const fn get_clip_mask(&self) -> &OptionImageMask { &self.clip_mask }
    #[inline(always)]
    pub const fn get_tab_index(&self) -> OptionTabIndex { self.tab_index }

    #[inline(always)]
    pub fn set_node_type(&mut self, node_type: NodeType) { self.node_type = node_type; }
    #[inline(always)]
    pub fn set_dataset(&mut self, data: OptionRefAny) { self.dataset = data; }
    #[inline(always)]
    pub fn set_ids_and_classes(&mut self, ids_and_classes: IdOrClassVec) { self.ids_and_classes = ids_and_classes; }
    #[inline(always)]
    pub fn set_callbacks(&mut self, callbacks: CallbackDataVec) { self.callbacks = callbacks; }
    #[inline(always)]
    pub fn set_inline_css_props(&mut self, inline_css_props: NodeDataInlineCssPropertyVec) { self.inline_css_props = inline_css_props; }
    #[inline(always)]
    pub fn set_clip_mask(&mut self, clip_mask: OptionImageMask) { self.clip_mask = clip_mask; }
    #[inline(always)]
    pub fn set_tab_index(&mut self, tab_index: OptionTabIndex) { self.tab_index = tab_index; }

    #[inline(always)]
    pub fn with_node_type(self, node_type: NodeType) -> Self { Self { node_type, .. self } }
    #[inline(always)]
    pub fn with_dataset(self, data: OptionRefAny) -> Self { Self { dataset: data, .. self } }
    #[inline(always)]
    pub fn with_ids_and_classes(self, ids_and_classes: IdOrClassVec) -> Self { Self { ids_and_classes, .. self } }
    #[inline(always)]
    pub fn with_callbacks(self, callbacks: CallbackDataVec) -> Self { Self { callbacks, .. self } }
    #[inline(always)]
    pub fn with_inline_css_props(self, inline_css_props: NodeDataInlineCssPropertyVec) -> Self { Self { inline_css_props, .. self } }
    #[inline(always)]
    pub fn with_clip_mask(self, clip_mask: OptionImageMask) -> Self { Self { clip_mask, .. self } }
    #[inline(always)]
    pub fn with_tab_index(self, tab_index: OptionTabIndex) -> Self { Self { tab_index, .. self } }

    pub fn calculate_node_data_hash(&self) -> DomNodeHash {

        use ahash::AHasher as HashAlgorithm;
        use core::hash::{Hash, Hasher};

        let mut hasher = HashAlgorithm::default();
        self.hash(&mut hasher);

        DomNodeHash(hasher.finish())
    }

    pub fn debug_print_start(&self, css_cache: &CssPropertyCache, node_id: &NodeId, node_state: &StyledNodeState) -> String {
        let html_type = self.node_type.get_path();
        let attributes_string = node_data_to_string(&self);
        let style = css_cache.get_computed_css_style_string(&self, node_id, node_state);
        format!("<{} data-az-node-id=\"{}\" {} style=\"{}\">", html_type, node_id.index(), attributes_string, style)
    }

    pub fn debug_print_end(&self) -> String {
        let html_type = self.node_type.get_path();
        format!("</{}>", html_type)
    }
}

/// The document model, similar to HTML. This is a create-only structure, you don't actually read anything back
#[repr(C)]
#[derive(PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Dom {
    pub root: NodeData,
    pub children: DomVec,
    // Tracks the number of sub-children of the current children, so that
    // the `Dom` can be converted into a `CompactDom`
    estimated_total_children: usize,
}

impl Dom {
    pub fn copy_except_for_root(&mut self) -> Self {
        Self {
            root: self.root.copy_special(),
            children: self.children.into_library_owned_vec().into(),
            estimated_total_children: self.estimated_total_children,
        }
    }
    pub fn node_count(&self) -> usize {
        self.estimated_total_children + 1
    }
}

impl DomVec {
    #[inline(always)]
    pub fn into_library_owned_vec(&mut self) -> Vec<Dom> {
        let mut vec = Vec::with_capacity(self.as_ref().len());
        for item in self.as_mut().iter_mut() {
            vec.push(item.copy_except_for_root());
        }
        vec
    }
}

impl_option!(Dom, OptionDom, copy = false, clone = false, [Debug, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl_vec!(Dom, DomVec, DomVecDestructor);
impl_vec_mut!(Dom, DomVec);
impl_vec_debug!(Dom, DomVec);
impl_vec_partialord!(Dom, DomVec);
impl_vec_ord!(Dom, DomVec);
impl_vec_partialeq!(Dom, DomVec);
impl_vec_eq!(Dom, DomVec);
impl_vec_hash!(Dom, DomVec);

impl Dom {

    /// Creates an empty DOM with a give `NodeType`. Note: This is a `const fn` and
    /// doesn't allocate, it only allocates once you add at least one child node.
    #[inline]
    pub const fn new(node_type: NodeType) -> Self {
        const DEFAULT_VEC: DomVec = DomVec::from_const_slice(&[]);
        Self {
            root: NodeData::new(node_type),
            children: DEFAULT_VEC,
            estimated_total_children: 0,
        }
    }

    #[inline(always)]
    pub const fn div() -> Self { Self::new(NodeType::Div) }
    #[inline(always)]
    pub const fn body() -> Self { Self::new(NodeType::Body) }
    #[inline(always)]
    pub const fn br() -> Self { Self::new(NodeType::Br) }
    #[inline(always)]
    pub fn label<S: Into<AzString>>(value: S) -> Self { Self::new(NodeType::Label(value.into())) }
    #[inline(always)]
    pub const fn image(image: ImageId) -> Self { Self::new(NodeType::Image(image)) }
    #[inline(always)]
    #[cfg(feature = "opengl")]
    pub fn gl_texture(data: RefAny, callback: GlCallbackType) -> Self { Self::new(NodeType::GlTexture(GlTextureNode { callback: GlCallback { cb: callback }, data })) }
    #[inline(always)]
    pub fn iframe(data: RefAny, callback: IFrameCallbackType) -> Self { Self::new(NodeType::IFrame(IFrameNode { callback: IFrameCallback { cb: callback }, data })) }

    #[inline]
    pub fn with_dataset(mut self, data: RefAny) -> Self { self.set_dataset(data); self }
    #[inline]
    pub fn with_ids_and_classes(mut self, ids: IdOrClassVec) -> Self { self.set_ids_and_classes(ids); self }
    #[inline]
    pub fn with_inline_css_props(mut self, properties: NodeDataInlineCssPropertyVec) -> Self { self.set_inline_css_props(properties); self }
    #[inline]
    pub fn with_callbacks(mut self, callbacks: CallbackDataVec) -> Self { self.set_callbacks(callbacks); self }
    #[inline]
    pub fn with_children(mut self, children: DomVec) -> Self { self.set_children(children); self }
    #[inline]
    pub fn with_clip_mask(mut self, clip_mask: OptionImageMask) -> Self { self.set_clip_mask(clip_mask); self }
    #[inline]
    pub fn with_tab_index(mut self, tab_index: OptionTabIndex) -> Self { self.set_tab_index(tab_index); self }

    #[inline]
    pub fn set_dataset(&mut self, data: RefAny) { self.root.set_dataset(Some(data).into()); }
    #[inline(always)]
    pub fn set_ids_and_classes(&mut self, ids: IdOrClassVec) { self.root.set_ids_and_classes(ids); }
    #[inline]
    pub fn set_inline_css_props(&mut self, properties: NodeDataInlineCssPropertyVec) { self.root.set_inline_css_props(properties); }
    #[inline]
    pub fn set_callbacks(&mut self, callbacks: CallbackDataVec) { self.root.set_callbacks(callbacks); }
    #[inline]
    pub fn set_children(&mut self, children: DomVec) {
        self.estimated_total_children = 0;
        for c in children.iter() {
            self.estimated_total_children += c.estimated_total_children + 1;
        }
        self.children = children;
    }
    #[inline(always)]
    pub fn set_clip_mask(&mut self, clip_mask: OptionImageMask) { self.root.set_clip_mask(clip_mask); }
    #[inline]
    pub fn set_tab_index(&mut self, tab_index: OptionTabIndex) { self.root.set_tab_index(tab_index); }

    #[cfg(feature = "multithreading")]
    pub fn style(self, css: Css) -> StyledDom {
        StyledDom::new(self, css)
    }
}

impl fmt::Debug for Dom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {

        fn print_dom(d: &Dom, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "Dom {{\r\n")?;
            write!(f, "\troot: {:#?}", d.root)?;
            write!(f, "\testimated_total_children: {:#?}", d.estimated_total_children)?;
            write!(f, "\tchildren: [")?;
            for c in d.children.iter() {
                print_dom(c, f)?;
            }
            write!(f, "\t]")?;
            write!(f, "}}")?;
            Ok(())
        }

        print_dom(self, f)
    }
}

impl FromIterator<Dom> for Dom {
    fn from_iter<I: IntoIterator<Item=Dom>>(iter: I) -> Self {

        let mut estimated_total_children = 0;
        let children = iter.into_iter().map(|c| {
            estimated_total_children += c.estimated_total_children + 1;
            c
        }).collect();

        Dom {
            root: NodeData::div(),
            children,
            estimated_total_children,
        }
    }
}

impl FromIterator<NodeData> for Dom {
    fn from_iter<I: IntoIterator<Item=NodeData>>(iter: I) -> Self {

        let children = iter.into_iter().map(|c| Dom { root: c, children: DomVec::new(), estimated_total_children: 0 }).collect::<DomVec>();
        let estimated_total_children = children.len();

        Dom {
            root: NodeData::div(),
            children: children,
            estimated_total_children,
        }
    }
}

impl FromIterator<NodeType> for Dom {
    fn from_iter<I: IntoIterator<Item=NodeType>>(iter: I) -> Self {
        iter.into_iter().map(|i| NodeData { node_type: i, .. Default::default() }).collect()
    }
}


/// Same as `Dom`, but arena-based for more efficient memory layout
#[derive(Debug, PartialEq, PartialOrd, Eq)]
pub(crate) struct CompactDom {
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
        fn convert_dom_into_compact_dom(mut dom: Dom) -> CompactDom {

            // note: somehow convert this into a non-recursive form later on!
            fn convert_dom_into_compact_dom_internal(
                dom: &mut Dom,
                node_hierarchy: &mut [Node],
                node_data: &mut Vec<NodeData>,
                parent_node_id: NodeId,
                node: Node,
                cur_node_id: &mut usize
            ) {

                // - parent [0]
                //    - child [1]
                //    - child [2]
                //        - child of child 2 [2]
                //        - child of child 2 [4]
                //    - child [5]
                //    - child [6]
                //        - child of child 4 [7]

                // Write node into the arena here!
                node_hierarchy[parent_node_id.index()] = node.clone();
                node_data[parent_node_id.index()] = dom.root.copy_special();
                *cur_node_id += 1;

                let mut previous_sibling_id = None;
                let children_len = dom.children.len();
                for (child_index, child_dom) in dom.children.as_mut().iter_mut().enumerate() {
                    let child_node_id = NodeId::new(*cur_node_id);
                    let is_last_child = (child_index + 1) == children_len;
                    let child_dom_is_empty = child_dom.children.is_empty();
                    let child_node = Node {
                        parent: Some(parent_node_id),
                        previous_sibling: previous_sibling_id,
                        next_sibling: if is_last_child { None } else { Some(child_node_id + child_dom.estimated_total_children + 1) },
                        last_child: if child_dom_is_empty { None } else { Some(child_node_id + child_dom.estimated_total_children) },
                    };
                    previous_sibling_id = Some(child_node_id);
                    // recurse BEFORE adding the next child
                    convert_dom_into_compact_dom_internal(child_dom, node_hierarchy, node_data, child_node_id, child_node, cur_node_id);
                }
            }

            // Pre-allocate all nodes (+ 1 root node)
            const DEFAULT_NODE_DATA: NodeData = NodeData::div();

            let mut node_hierarchy = vec![Node::ROOT; dom.estimated_total_children + 1];
            let mut node_data = (0..dom.estimated_total_children + 1).map(|_| DEFAULT_NODE_DATA).collect::<Vec<_>>();
            let mut cur_node_id = 0;

            let root_node_id = NodeId::ZERO;
            let root_node = Node {
                parent: None,
                previous_sibling: None,
                next_sibling: None,
                last_child: if dom.children.is_empty() { None } else { Some(root_node_id + dom.estimated_total_children) },
            };

            convert_dom_into_compact_dom_internal(&mut dom, &mut node_hierarchy, &mut node_data, root_node_id, root_node, &mut cur_node_id);

            CompactDom {
                node_hierarchy: NodeHierarchy { internal: node_hierarchy },
                node_data: NodeDataContainer { internal: node_data },
                root: root_node_id,
            }
        }

        convert_dom_into_compact_dom(dom)
    }
}

#[test]
fn test_compact_dom_conversion() {

    let dom: Dom = Dom::body()
        .with_child(Dom::div().with_class("class1"))
        .with_child(Dom::div().with_class("class1")
            .with_child(Dom::div().with_id("child_2"))
        )
        .with_child(Dom::div().with_class("class1"));

    let c0: Vec<AzString> = vec!["class1".to_string().into()];
    let c0: StringVec = c0.into();
    let c1: Vec<AzString> = vec!["class1".to_string().into()];
    let c1: StringVec = c1.into();
    let c2: Vec<AzString> = vec!["child_2".to_string().into()];
    let c2: StringVec = c2.into();
    let c3: Vec<AzString> = vec!["class1".to_string().into()];
    let c3: StringVec = c3.into();

    let expected_dom: CompactDom = CompactDom {
        root: NodeId::ZERO,
        node_hierarchy: NodeHierarchy { internal: vec![
            Node /* 0 */ {
                parent: None,
                previous_sibling: None,
                next_sibling: None,
                first_child: Some(NodeId::new(1)),
                last_child: Some(NodeId::new(4)),
            },
            Node /* 1 */ {
                parent: Some(NodeId::new(0)),
                previous_sibling: None,
                next_sibling: Some(NodeId::new(2)),
                first_child: None,
                last_child: None,
            },
            Node /* 2 */ {
                parent: Some(NodeId::new(0)),
                previous_sibling: Some(NodeId::new(1)),
                next_sibling: Some(NodeId::new(4)),
                first_child: Some(NodeId::new(3)),
                last_child: Some(NodeId::new(3)),
            },
            Node /* 3 */ {
                parent: Some(NodeId::new(2)),
                previous_sibling: None,
                next_sibling: None,
                first_child: None,
                last_child: None,
            },
            Node /* 4 */ {
                parent: Some(NodeId::new(0)),
                previous_sibling: Some(NodeId::new(2)),
                next_sibling: None,
                first_child: None,
                last_child: None,
            },
        ]},
        node_data: NodeDataContainer { internal: vec![
            /* 0 */    NodeData::body(),
            /* 1 */    NodeData::div().with_classes(c0),
            /* 2 */    NodeData::div().with_classes(c1),
            /* 3 */    NodeData::div().with_ids(c2),
            /* 4 */    NodeData::div().with_classes(c3),
        ]},
    };

    let got_dom = convert_dom_into_compact_dom(dom);
    if got_dom != expected_dom {
        panic!("{}", format!("expected compact dom: ----\r\n{:#?}\r\n\r\ngot compact dom: ----\r\n{:#?}\r\n", expected_dom, got_dom));
    }
}

#[test]
fn test_dom_sibling_1() {

    let dom: Dom =
        Dom::div()
            .with_child(
                Dom::div()
                .with_id("sibling-1")
                .with_child(Dom::div()
                    .with_id("sibling-1-child-1")))
            .with_child(Dom::div()
                .with_id("sibling-2")
                .with_child(Dom::div()
                    .with_id("sibling-2-child-1")));

    let dom = convert_dom_into_compact_dom(dom);

    let arena = &dom.arena;

    assert_eq!(NodeId::new(0), dom.root);

    let v: Vec<AzString> = vec!["sibling-1".to_string().into()];
    let v: StringVec = v.into();
    assert_eq!(v,
        arena.node_data[
            arena.node_hierarchy[dom.root]
            .first_child.expect("root has no first child")
        ].ids);

    let v: Vec<AzString> = vec!["sibling-2".to_string().into()];
    let v: StringVec = v.into();
    assert_eq!(v,
        arena.node_data[
            arena.node_hierarchy[
                arena.node_hierarchy[dom.root]
                .first_child.expect("root has no first child")
            ].next_sibling.expect("root has no second sibling")
        ].ids);

    let v: Vec<AzString> = vec!["sibling-1-child-1".to_string().into()];
    let v: StringVec = v.into();
    assert_eq!(v,
        arena.node_data[
            arena.node_hierarchy[
                arena.node_hierarchy[dom.root]
                .first_child.expect("root has no first child")
            ].first_child.expect("first child has no first child")
        ].ids);

    let v: Vec<AzString> = vec!["sibling-2-child-1".to_string().into()];
    let v: StringVec = v.into();
    assert_eq!(v,
        arena.node_data[
            arena.node_hierarchy[
                arena.node_hierarchy[
                    arena.node_hierarchy[dom.root]
                    .first_child.expect("root has no first child")
                ].next_sibling.expect("first child has no second sibling")
            ].first_child.expect("second sibling has no first child")
        ].ids);
}

#[test]
fn test_dom_from_iter_1() {

    use crate::id_tree::Node;

    let dom: Dom = (0..5).map(|e| NodeData::new(NodeType::Label(format!("{}", e + 1).into()))).collect();
    let dom = convert_dom_into_compact_dom(dom);

    let arena = &dom.arena;

    // We need to have 6 nodes:
    //
    // root                 NodeId(0)
    //   |-> 1              NodeId(1)
    //   |-> 2              NodeId(2)
    //   |-> 3              NodeId(3)
    //   |-> 4              NodeId(4)
    //   '-> 5              NodeId(5)

    assert_eq!(arena.len(), 6);

    // Check root node
    assert_eq!(arena.node_hierarchy.get(NodeId::new(0)), Some(&Node {
        parent: None,
        previous_sibling: None,
        next_sibling: None,
        first_child: Some(NodeId::new(1)),
        last_child: Some(NodeId::new(5)),
    }));
    assert_eq!(arena.node_data.get(NodeId::new(0)), Some(&NodeData::new(NodeType::Div)));

    assert_eq!(arena.node_hierarchy.get(NodeId::new(arena.node_hierarchy.len() - 1)), Some(&Node {
        parent: Some(NodeId::new(0)),
        previous_sibling: Some(NodeId::new(4)),
        next_sibling: None,
        first_child: None,
        last_child: None,
    }));

    assert_eq!(arena.node_data.get(NodeId::new(arena.node_data.len() - 1)), Some(&NodeData {
        node_type: NodeType::Label("5".to_string().into()),
        .. Default::default()
    }));
}

/// Test that there shouldn't be a DOM that has 0 nodes
#[test]
fn test_zero_size_dom() {

    let null_dom: Dom = (0..0).map(|_| NodeData::default()).collect();
    let null_dom = convert_dom_into_compact_dom(null_dom);

    assert!(null_dom.arena.len() == 1);
}
