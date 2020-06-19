use std::{
    fmt,
    hash::{Hash, Hasher},
    sync::atomic::{AtomicUsize, Ordering},
    iter::FromIterator,
};
use crate::{
    callbacks::{
        Callback, CallbackType,
        IFrameCallback, IFrameCallbackType,
        RefAny,
    },
    app_resources::{ImageId, TextId},
    id_tree::{Arena, NodeDataContainer},
};
#[cfg(feature = "opengl")]
use crate::callbacks::{GlCallback, GlCallbackType};
use azul_css::{AzString, StringVec, NodeTypePath, CssProperty};
pub use crate::id_tree::{NodeHierarchy, Node, NodeId};

static TAG_ID: AtomicUsize = AtomicUsize::new(1);

/// Unique Ttag" that is used to annotate which rectangles are relevant for hit-testing
#[derive(Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct TagId(pub u64);

impl ::std::fmt::Display for TagId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ScrollTagId({})", self.0)
    }
}

impl ::std::fmt::Debug for TagId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}


/// Same as the `TagId`, but only for scrollable nodes
#[derive(Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct ScrollTagId(pub TagId);

impl ::std::fmt::Display for ScrollTagId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ScrollTagId({})", (self.0).0)
    }
}

impl ::std::fmt::Debug for ScrollTagId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl TagId {
    pub fn new() -> Self {
        TagId(TAG_ID.fetch_add(1, Ordering::SeqCst) as u64)
    }
    pub fn reset() {
        TAG_ID.swap(1, Ordering::SeqCst);
    }
}

impl ScrollTagId {
    pub fn new() -> ScrollTagId {
        ScrollTagId(TagId::new())
    }
}

static DOM_ID: AtomicUsize = AtomicUsize::new(1);

/// DomID - used for identifying different DOMs (for example IFrameCallbacks)
/// have a different DomId than the root DOM
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DomId {
    /// Unique ID for this DOM
    id: usize,
    /// If this DOM was generated from an IFrameCallback, stores the parents
    /// DomId + the NodeId (from the parent DOM) which the IFrameCallback
    /// was attached to.
    parent: Option<(Box<DomId>, NodeId)>,
}

impl DomId {

    /// ID for the top-level DOM (of a window)
    pub const ROOT_ID: DomId = Self { id: 0, parent: None };

    /// Creates a new, unique DOM ID.
    #[inline(always)]
    pub fn new(parent: Option<(DomId, NodeId)>) -> DomId {
        DomId {
            id: DOM_ID.fetch_add(1, Ordering::SeqCst),
            parent: parent.map(|(p, node_id)| (Box::new(p), node_id)),
        }
    }

    /// Reset the DOM ID to 0, usually done once-per-frame for the root DOM
    #[inline(always)]
    pub fn reset() {
        DOM_ID.swap(0, Ordering::SeqCst);
    }

    /// Returns if this is the root node
    #[inline(always)]
    pub fn is_root(&self) -> bool {
        *self == Self::ROOT_ID
    }
}

/// Calculated hash of a DOM node, used for querying attributes of the DOM node
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct DomHash(pub u64);

/// List of core DOM node types built-into by `azul`.
#[derive(Debug, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum NodeType {
    /// Regular div with no particular type of data attached
    Div,
    /// Same as div, but only for the root node
    Body,
    /// A small label that can be (optionally) be selectable with the mouse
    Label(AzString),
    /// Larger amount of text, that has to be cached
    Text(TextId),
    /// An image that is rendered by WebRender. The id is acquired by the
    /// `AppState::add_image()` function
    Image(ImageId),
    /// OpenGL texture. The `Svg` widget deserizalizes itself into a texture
    /// Equality and Hash values are only checked by the OpenGl texture ID,
    /// Azul does not check that the contents of two textures are the same
    #[cfg(feature = "opengl")]
    GlTexture(GlTextureNode),
    /// DOM that gets passed its width / height during the layout
    IFrame(IFrameNode),
}

impl NodeType {
    fn get_text_content(&self) -> Option<String> {
        use self::NodeType::*;
        match self {
            Div | Body => None,
            Label(s) => Some(format!("{}", s)),
            Image(id) => Some(format!("image({:?})", id)),
            Text(t) => Some(format!("textid({:?})", t)),
            #[cfg(feature = "opengl")]
            GlTexture(g) => Some(format!("gltexture({:?})", g)),
            IFrame(i) => Some(format!("iframe({:?})", i)),
        }
    }
}

impl NodeType {
    #[inline]
    pub fn get_path(&self) -> NodeTypePath {
        use self::NodeType::*;
        match self {
            Div => NodeTypePath::Div,
            Body => NodeTypePath::Body,
            Label(_) | Text(_) => NodeTypePath::P,
            Image(_) => NodeTypePath::Img,
            #[cfg(feature = "opengl")]
            GlTexture(_) => NodeTypePath::Texture,
            IFrame(_) => NodeTypePath::IFrame,
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
}

impl HoverEventFilter {
    pub fn to_focus_event_filter(&self) -> Option<FocusEventFilter> {
        use self::HoverEventFilter::*;
        match self {
            MouseOver => Some(FocusEventFilter::MouseOver),
            MouseDown => Some(FocusEventFilter::MouseDown),
            LeftMouseDown => Some(FocusEventFilter::LeftMouseDown),
            RightMouseDown => Some(FocusEventFilter::RightMouseDown),
            MiddleMouseDown => Some(FocusEventFilter::MiddleMouseDown),
            MouseUp => Some(FocusEventFilter::MouseUp),
            LeftMouseUp => Some(FocusEventFilter::LeftMouseUp),
            RightMouseUp => Some(FocusEventFilter::RightMouseUp),
            MiddleMouseUp => Some(FocusEventFilter::MiddleMouseUp),
            MouseEnter => Some(FocusEventFilter::MouseEnter),
            MouseLeave => Some(FocusEventFilter::MouseLeave),
            Scroll => Some(FocusEventFilter::Scroll),
            ScrollStart => Some(FocusEventFilter::ScrollStart),
            ScrollEnd => Some(FocusEventFilter::ScrollEnd),
            TextInput => Some(FocusEventFilter::TextInput),
            VirtualKeyDown => Some(FocusEventFilter::VirtualKeyDown),
            VirtualKeyUp => Some(FocusEventFilter::VirtualKeyDown),
            HoveredFile => None,
            DroppedFile => None,
            HoveredFileCancelled => None,
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
}

impl WindowEventFilter {
    pub fn to_hover_event_filter(&self) -> Option<HoverEventFilter> {
        use self::WindowEventFilter::*;
        match self {
            MouseOver => Some(HoverEventFilter::MouseOver),
            MouseDown => Some(HoverEventFilter::MouseDown),
            LeftMouseDown => Some(HoverEventFilter::LeftMouseDown),
            RightMouseDown => Some(HoverEventFilter::RightMouseDown),
            MiddleMouseDown => Some(HoverEventFilter::MiddleMouseDown),
            MouseUp => Some(HoverEventFilter::MouseUp),
            LeftMouseUp => Some(HoverEventFilter::LeftMouseUp),
            RightMouseUp => Some(HoverEventFilter::RightMouseUp),
            MiddleMouseUp => Some(HoverEventFilter::MiddleMouseUp),
            Scroll => Some(HoverEventFilter::Scroll),
            ScrollStart => Some(HoverEventFilter::ScrollStart),
            ScrollEnd => Some(HoverEventFilter::ScrollEnd),
            TextInput => Some(HoverEventFilter::TextInput),
            VirtualKeyDown => Some(HoverEventFilter::VirtualKeyDown),
            VirtualKeyUp => Some(HoverEventFilter::VirtualKeyDown),
            HoveredFile => Some(HoverEventFilter::HoveredFile),
            DroppedFile => Some(HoverEventFilter::DroppedFile),
            HoveredFileCancelled => Some(HoverEventFilter::HoveredFileCancelled),
            // MouseEnter and MouseLeave on the **window** - does not mean a mouseenter
            // and a mouseleave on the hovered element
            MouseEnter => None,
            MouseLeave => None,
        }
    }
}

#[cfg(feature = "opengl")]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct GlTextureNode {
    pub callback: GlCallback,
    pub data: RefAny,
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct OverrideProperty {
    pub property_id: AzString,
    pub override_value: CssProperty,
}

/// Represents one single DOM node (node type, classes, ids and callbacks are stored here)
#[repr(C)]
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeData {
    /// `div`
    node_type: NodeType,
    /// `#main #something`
    ids: StringVec,
    /// `.myclass .otherclass`
    classes: StringVec,
    /// `On::MouseUp` -> `Callback(my_button_click_handler)`
    callbacks: CallbackDataVec,
    /// Override certain dynamic styling properties in this frame. For this,
    /// these properties have to have a name (the ID).
    ///
    /// For example, in the CSS stylesheet:
    ///
    /// ```css,ignore
    /// #my_item { width: [[ my_custom_width | 200px ]] }
    /// ```
    ///
    /// ```rust,ignore
    /// let node = NodeData {
    ///     id: Some("my_item".into()),
    ///     dynamic_css_overrides: vec![("my_custom_width".into(), CssProperty::Width(LayoutWidth::px(500.0)))]
    /// }
    /// ```
    dynamic_css_overrides: OverridePropertyVec,
    /// Whether this div can be dragged or not, similar to `draggable = "true"` in HTML, .
    ///
    /// **TODO**: Currently doesn't do anything, since the drag & drop implementation is missing, API stub.
    is_draggable: bool,
    /// Whether this div can be focused, and if yes, in what default to `None` (not focusable).
    /// Note that without this, there can be no `On::FocusReceived` (equivalent to onfocus),
    /// `On::FocusLost` (equivalent to onblur), etc. events.
    tab_index: OptionTabIndex,
}

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
    OverrideInParent(usize),
    /// Elements can be focused in callbacks, but are not accessible via
    /// keyboard / tab navigation (-1)
    NoKeyboardFocus,
}

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

impl fmt::Display for NodeData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {

        let html_type = self.node_type.get_path();
        let attributes_string = node_data_to_string(&self);

        match self.node_type.get_text_content() {
            Some(content) => write!(f, "<{}{}>{}</{}>", html_type, attributes_string, content, html_type),
            None => write!(f, "<{}{}/>", html_type, attributes_string)
        }
    }
}

impl NodeData {
    pub fn debug_print_start(&self, close_self: bool) -> String {
        let html_type = self.node_type.get_path();
        let attributes_string = node_data_to_string(&self);
        format!("<{}{}{}>", html_type, attributes_string, if close_self { " /" } else { "" })
    }

    pub fn debug_print_end(&self) -> String {
        let html_type = self.node_type.get_path();
        format!("</{}>", html_type)
    }
}

fn node_data_to_string(node_data: &NodeData) -> String {

    let id_string = if node_data.ids.is_empty() {
        String::new()
    } else {
        format!(" id=\"{}\"", node_data.ids.iter().map(|s| s.as_str().to_string()).collect::<Vec<String>>().join(" "))
    };

    let class_string = if node_data.classes.is_empty() {
        String::new()
    } else {
        format!(" class=\"{}\"", node_data.classes.iter().map(|s| s.as_str().to_string()).collect::<Vec<String>>().join(" "))
    };

    let draggable = if node_data.is_draggable {
        format!(" draggable=\"true\"")
    } else {
        String::new()
    };

    let tabindex = if let OptionTabIndex::Some(tab_index) = node_data.tab_index {
        format!(" tabindex=\"{}\"", tab_index.get_index())
    } else {
        String::new()
    };

    let callbacks = if node_data.callbacks.is_empty() {
        String::new()
    } else {
        format!(" callbacks=\"{}\"", node_data.callbacks.iter().map(|callbackdata| format!("({:?}={:?})", callbackdata.event, callbackdata.callback)).collect::<Vec<String>>().join(" "))
    };

    let css_overrides = if node_data.dynamic_css_overrides.is_empty() {
        String::new()
    } else {
        format!(" css-overrides=\"{}\"", node_data.dynamic_css_overrides.iter().map(|overrideprop| format!("{}={:?};", overrideprop.property_id, overrideprop.override_value)).collect::<Vec<String>>().join(" "))
    };

    format!("{}{}{}{}{}{}", id_string, class_string, tabindex, draggable, callbacks, css_overrides)
}

impl fmt::Debug for NodeData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "NodeData {{")?;
        write!(f, "\tnode_type: {:?}", self.node_type)?;

        if !self.ids.is_empty() { write!(f, "\tids: {:?}", self.ids)?; }
        if !self.classes.is_empty() { write!(f, "\tclasses: {:?}", self.classes)?; }
        if !self.callbacks.is_empty() { write!(f, "\tcallbacks: {:?}", self.callbacks)?; }
        if !self.dynamic_css_overrides.is_empty() { write!(f, "\tdynamic_css_overrides: {:?}", self.dynamic_css_overrides)?; }
        if self.is_draggable { write!(f, "\tis_draggable: {:?}", self.is_draggable)?; }
        if let OptionTabIndex::Some(t) = self.tab_index { write!(f, "\ttab_index: {:?}", t)?; }
        write!(f, "}}")?;
        Ok(())
    }
}

impl NodeData {

    /// Creates a new `NodeData` instance from a given `NodeType`
    #[inline]
    pub fn new(node_type: NodeType) -> Self {
        Self {
            node_type,
            ids: StringVec::new(),
            classes: StringVec::new(),
            callbacks: CallbackDataVec::new(),
            dynamic_css_overrides: OverridePropertyVec::new(),
            is_draggable: false,
            tab_index: OptionTabIndex::None,
        }
    }

    #[inline]
    pub fn add_callback<O: Into<EventFilter>>(&mut self, on: O, callback: CallbackType, data: RefAny) {
        self.callbacks.push(CallbackData { event: on.into(), callback: Callback { cb: callback }, data });
    }

    #[inline]
    pub fn add_css_override<S: Into<AzString>, P: Into<CssProperty>>(&mut self, override_id: S, property: P) {
        self.dynamic_css_overrides.push(OverrideProperty { property_id: override_id.into(), override_value: property.into() });
    }

    #[inline]
    pub fn add_id<S: Into<AzString>>(&mut self, id: S) {
        self.ids.push(id.into());
    }

    #[inline]
    pub fn add_class<S: Into<AzString>>(&mut self, class: S) {
        self.classes.push(class.into());
    }

    /// Checks whether this node is of the given node type (div, image, text)
    #[inline]
    pub fn is_node_type(&self, searched_type: NodeType) -> bool {
        self.node_type == searched_type
    }

    /// Checks whether this node has the searched ID attached
    pub fn has_id(&self, id: &str) -> bool {
        self.ids.iter().any(|self_id| self_id.as_str() == id)
    }

    /// Checks whether this node has the searched class attached
    pub fn has_class(&self, class: &str) -> bool {
        self.classes.iter().any(|self_class| self_class.as_str() == class)
    }

    pub fn calculate_node_data_hash(&self) -> DomHash {

        use std::collections::hash_map::DefaultHasher as HashAlgorithm;

        let mut hasher = HashAlgorithm::default();
        self.hash(&mut hasher);

        DomHash(hasher.finish())
    }

    /// Shorthand for `NodeData::new(NodeType::Body)`.
    #[inline(always)]
    pub fn body() -> Self {
        Self::new(NodeType::Body)
    }

    /// Shorthand for `NodeData::new(NodeType::Div)`.
    #[inline(always)]
    pub fn div() -> Self {
        Self::new(NodeType::Div)
    }

    /// Shorthand for `NodeData::new(NodeType::Label(value.into()))`
    #[inline(always)]
    pub fn label<S: Into<AzString>>(value: S) -> Self {
        Self::new(NodeType::Label(value.into()))
    }

    /// Shorthand for `NodeData::new(NodeType::Text(text_id))`
    #[inline(always)]
    pub fn text(text_id: TextId) -> Self {
        Self::new(NodeType::Text(text_id))
    }

    /// Shorthand for `NodeData::new(NodeType::Image(image_id))`
    #[inline(always)]
    pub fn image(image: ImageId) -> Self {
        Self::new(NodeType::Image(image))
    }

    #[inline(always)]
    #[cfg(feature = "opengl")]
    pub fn gl_texture(callback: GlCallbackType, data: RefAny) -> Self {
        Self::new(NodeType::GlTexture(GlTextureNode { callback: GlCallback { cb: callback }, data }))
    }

    #[inline(always)]
    pub fn iframe(callback: IFrameCallbackType, data: RefAny) -> Self {
        Self::new(NodeType::IFrame(IFrameNode { callback: IFrameCallback { cb: callback }, data }))
    }

    // NOTE: Getters are used here in order to allow changing the memory allocator for the NodeData
    // in the future (which is why the fields are all private).

    #[inline(always)]
    pub const fn get_node_type(&self) -> &NodeType { &self.node_type }
    #[inline(always)]
    pub const fn get_ids(&self) -> &StringVec { &self.ids }
    #[inline(always)]
    pub const fn get_classes(&self) -> &StringVec { &self.classes }
    #[inline(always)]
    pub const fn get_callbacks(&self) -> &CallbackDataVec { &self.callbacks }
    #[inline(always)]
    pub const fn get_dynamic_css_overrides(&self) -> &OverridePropertyVec { &self.dynamic_css_overrides }
    #[inline(always)]
    pub const fn get_is_draggable(&self) -> bool { self.is_draggable }
    #[inline(always)]
    pub const fn get_tab_index(&self) -> OptionTabIndex { self.tab_index }

    #[inline(always)]
    pub fn set_node_type(&mut self, node_type: NodeType) { self.node_type = node_type; }
    #[inline(always)]
    pub fn set_ids(&mut self, ids: StringVec) { self.ids = ids; }
    #[inline(always)]
    pub fn set_classes(&mut self, classes: StringVec) { self.classes = classes; }
    #[inline(always)]
    pub fn set_callbacks(&mut self, callbacks: CallbackDataVec) { self.callbacks = callbacks; }
    #[inline(always)]
    pub fn set_dynamic_css_overrides(&mut self, dynamic_css_overrides: OverridePropertyVec) { self.dynamic_css_overrides = dynamic_css_overrides; }
    #[inline(always)]
    pub fn set_is_draggable(&mut self, is_draggable: bool) { self.is_draggable = is_draggable; }
    #[inline(always)]
    pub fn set_tab_index(&mut self, tab_index: OptionTabIndex) { self.tab_index = tab_index; }

    #[inline(always)]
    pub fn with_node_type(self, node_type: NodeType) -> Self { Self { node_type, .. self } }
    #[inline(always)]
    pub fn with_ids(self, ids: StringVec) -> Self { Self { ids, .. self } }
    #[inline(always)]
    pub fn with_classes(self, classes: StringVec) -> Self { Self { classes, .. self } }
    #[inline(always)]
    pub fn with_callbacks(self, callbacks: CallbackDataVec) -> Self { Self { callbacks, .. self } }
    #[inline(always)]
    pub fn with_dynamic_css_overrides(self, dynamic_css_overrides: OverridePropertyVec) -> Self { Self { dynamic_css_overrides, .. self } }
    #[inline(always)]
    pub fn is_draggable(self, is_draggable: bool) -> Self { Self { is_draggable, .. self } }
    #[inline(always)]
    pub fn with_tab_index(self, tab_index: OptionTabIndex) -> Self { Self { tab_index, .. self } }
}

/// The document model, similar to HTML. This is a create-only structure, you don't actually read anything back
#[repr(C)]
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Dom {
    pub root: NodeData,
    pub children: DomVec,
    // Tracks the number of sub-children of the current children, so that
    // the `Dom` can be converted into a `CompactDom`
    estimated_total_children: usize,
}

impl_vec!(Dom, DomVec);
impl_vec_debug!(Dom, DomVec);
impl_vec_partialord!(Dom, DomVec);
impl_vec_ord!(Dom, DomVec);
impl_vec_clone!(Dom, DomVec);
impl_vec_partialeq!(Dom, DomVec);
impl_vec_eq!(Dom, DomVec);
impl_vec_hash!(Dom, DomVec);

impl_vec!(OverrideProperty, OverridePropertyVec);
impl_vec_debug!(OverrideProperty, OverridePropertyVec);
impl_vec_partialord!(OverrideProperty, OverridePropertyVec);
impl_vec_ord!(OverrideProperty, OverridePropertyVec);
impl_vec_clone!(OverrideProperty, OverridePropertyVec);
impl_vec_partialeq!(OverrideProperty, OverridePropertyVec);
impl_vec_eq!(OverrideProperty, OverridePropertyVec);
impl_vec_hash!(OverrideProperty, OverridePropertyVec);

impl_vec!(CallbackData, CallbackDataVec);
impl_vec_debug!(CallbackData, CallbackDataVec);
impl_vec_partialord!(CallbackData, CallbackDataVec);
impl_vec_ord!(CallbackData, CallbackDataVec);
impl_vec_clone!(CallbackData, CallbackDataVec);
impl_vec_partialeq!(CallbackData, CallbackDataVec);
impl_vec_eq!(CallbackData, CallbackDataVec);
impl_vec_hash!(CallbackData, CallbackDataVec);

impl_option!(TabIndex, OptionTabIndex, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
impl_option!(Dom, OptionDom, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

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

impl fmt::Debug for Dom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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

pub(crate) fn convert_dom_into_compact_dom(dom: Dom) -> CompactDom {

    // Pre-allocate all nodes (+ 1 root node)
    let default_node_data = NodeData::div();

    let mut arena = Arena {
        node_hierarchy: NodeHierarchy { internal: vec![Node::ROOT; dom.estimated_total_children + 1] },
        node_data: NodeDataContainer { internal: vec![default_node_data; dom.estimated_total_children + 1] },
    };

    let root_node_id = NodeId::ZERO;
    let mut cur_node_id = 0;
    let root_node = Node {
        parent: None,
        previous_sibling: None,
        next_sibling: None,
        first_child: if dom.children.is_empty() { None } else { Some(root_node_id + 1) },
        last_child: if dom.children.is_empty() { None } else { Some(root_node_id + dom.estimated_total_children) },
    };

    convert_dom_into_compact_dom_internal(dom, &mut arena, root_node_id, root_node, &mut cur_node_id);

    CompactDom {
        arena,
        root: root_node_id,
    }
}

// note: somehow convert this into a non-recursive form later on!
fn convert_dom_into_compact_dom_internal(
    dom: Dom,
    arena: &mut Arena<NodeData>,
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
    arena.node_hierarchy[parent_node_id] = node;
    arena.node_data[parent_node_id] = dom.root;
    *cur_node_id += 1;

    let mut previous_sibling_id = None;
    let children_len = dom.children.len();
    for (child_index, child_dom) in dom.children.into_iter().enumerate() {
        let child_node_id = NodeId::new(*cur_node_id);
        let is_last_child = (child_index + 1) == children_len;
        let child_dom_is_empty = child_dom.children.is_empty();
        let child_node = Node {
            parent: Some(parent_node_id),
            previous_sibling: previous_sibling_id,
            next_sibling: if is_last_child { None } else { Some(child_node_id + child_dom.estimated_total_children + 1) },
            first_child: if child_dom_is_empty { None } else { Some(child_node_id + 1) },
            last_child: if child_dom_is_empty { None } else { Some(child_node_id + child_dom.estimated_total_children) },
        };
        previous_sibling_id = Some(child_node_id);
        // recurse BEFORE adding the next child
        convert_dom_into_compact_dom_internal(child_dom, arena, child_node_id, child_node, cur_node_id);
    }
}

#[test]
fn test_compact_dom_conversion() {

    use crate::dom::String::Static;

    struct Dummy;

    let dom: Dom<Dummy> = Dom::body()
        .with_child(Dom::div().with_class("class1"))
        .with_child(Dom::div().with_class("class1")
            .with_child(Dom::div().with_id("child_2"))
        )
        .with_child(Dom::div().with_class("class1"));

    let expected_dom: CompactDom<Dummy> = CompactDom {
        root: NodeId::ZERO,
        arena: Arena {
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
                /* 1 */    NodeData::div().with_classes(vec![Static("class1")]),
                /* 2 */    NodeData::div().with_classes(vec![Static("class1")]),
                /* 3 */    NodeData::div().with_ids(vec![Static("child_2")]),
                /* 4 */    NodeData::div().with_classes(vec![Static("class1")]),
            ]},
        },
    };

    let got_dom = convert_dom_into_compact_dom(dom);
    if got_dom != expected_dom {
        panic!("{}", format!("expected compact dom: ----\r\n{:#?}\r\n\r\ngot compact dom: ----\r\n{:#?}\r\n", expected_dom, got_dom));
    }
}

/// Same as `Dom`, but arena-based for more efficient memory layout
pub struct CompactDom {
    pub arena: Arena<NodeData>,
    pub root: NodeId,
}

impl From<Dom> for CompactDom {
    fn from(dom: Dom) -> Self {
        convert_dom_into_compact_dom(dom)
    }
}

impl CompactDom {
    /// Returns the number of nodes in this DOM
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.arena.len()
    }
}

impl Clone for CompactDom {
    fn clone(&self) -> Self {
        CompactDom {
            arena: self.arena.clone(),
            root: self.root,
        }
    }
}

impl PartialEq for CompactDom {
    fn eq(&self, rhs: &Self) -> bool {
        self.arena == rhs.arena &&
        self.root == rhs.root
    }
}

impl Eq for CompactDom { }

impl fmt::Debug for CompactDom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "CompactDom {{ arena: {:?}, root: {:?} }}", self.arena, self.root)
    }
}

impl Dom {

    /// Creates an empty DOM with a give `NodeType`. Note: This is a `const fn` and
    /// doesn't allocate, it only allocates once you add at least one child node.
    #[inline]
    pub fn new(node_type: NodeType) -> Self {
        Self {
            root: NodeData::new(node_type),
            children: DomVec::new(),
            estimated_total_children: 0,
        }
    }

    /// Creates an empty DOM with space reserved for `cap` nodes
    #[inline]
    pub fn with_capacity(node_type: NodeType, cap: usize) -> Self {
        Self {
            root: NodeData::new(node_type),
            children: DomVec::with_capacity(cap),
            estimated_total_children: 0,
        }
    }

    /// Shorthand for `Dom::new(NodeType::Div)`.
    #[inline(always)]
    pub fn div() -> Self {
        Self::new(NodeType::Div)
    }

    /// Shorthand for `Dom::new(NodeType::Body)`.
    #[inline(always)]
    pub fn body() -> Self {
        Self::new(NodeType::Body)
    }

    /// Shorthand for `Dom::new(NodeType::Label(value.into()))`
    #[inline(always)]
    pub fn label<S: Into<AzString>>(value: S) -> Self {
        Self::new(NodeType::Label(value.into()))
    }

    /// Shorthand for `Dom::new(NodeType::Text(text_id))`
    #[inline(always)]
    pub fn text(text_id: TextId) -> Self {
        Self::new(NodeType::Text(text_id))
    }

    /// Shorthand for `Dom::new(NodeType::Image(image_id))`
    #[inline(always)]
    pub fn image(image: ImageId) -> Self {
        Self::new(NodeType::Image(image))
    }

    /// Shorthand for `Dom::new(NodeType::GlTexture((callback, ptr)))`
    #[inline(always)]
    #[cfg(feature = "opengl")]
    pub fn gl_texture(callback: GlCallbackType, data: RefAny) -> Self {
        Self::new(NodeType::GlTexture(GlTextureNode { callback: GlCallback { cb: callback }, data }))
    }

    /// Shorthand for `Dom::new(NodeType::IFrame((callback, ptr)))`
    #[inline(always)]
    pub fn iframe(callback: IFrameCallbackType, data: RefAny) -> Self {
        Self::new(NodeType::IFrame(IFrameNode { callback: IFrameCallback { cb: callback }, data }))
    }

    /// Adds a child DOM to the current DOM
    #[inline]
    pub fn add_child(&mut self, child: Self) {
        self.estimated_total_children += child.estimated_total_children;
        self.estimated_total_children += 1;
        self.children.push(child);
    }

    /// Same as `id`, but easier to use for method chaining in a builder-style pattern
    #[inline]
    pub fn with_id<S: Into<AzString>>(mut self, id: S) -> Self {
        self.add_id(id);
        self
    }

    /// Same as `id`, but easier to use for method chaining in a builder-style pattern
    #[inline]
    pub fn with_class<S: Into<AzString>>(mut self, class: S) -> Self {
        self.add_class(class);
        self
    }

    /// Same as `event`, but easier to use for method chaining in a builder-style pattern
    #[inline]
    pub fn with_callback<O: Into<EventFilter>>(mut self, on: O, callback: CallbackType, ptr: RefAny) -> Self {
        self.add_callback(on, callback, ptr);
        self
    }

    #[inline]
    pub fn with_child(mut self, child: Self) -> Self {
        self.add_child(child);
        self
    }

    #[inline]
    pub fn with_css_override<S: Into<AzString>>(mut self, id: S, property: CssProperty) -> Self {
        self.add_css_override(id, property);
        self
    }

    #[inline]
    pub fn with_tab_index(mut self, tab_index: OptionTabIndex) -> Self {
        self.set_tab_index(tab_index);
        self
    }

    #[inline]
    pub fn is_draggable(mut self, draggable: bool) -> Self {
        self.set_is_draggable(draggable);
        self
    }

    #[inline]
    pub fn add_id<S: Into<AzString>>(&mut self, id: S) {
        self.root.add_id(id);
    }

    #[inline(always)]
    pub fn set_ids(&mut self, ids: StringVec) { self.root.set_ids(ids); }

    #[inline]
    pub fn add_class<S: Into<AzString>>(&mut self, class: S) {
        self.root.add_class(class);
    }

    #[inline(always)]
    pub fn set_classes(&mut self, classes: StringVec) { self.root.set_classes(classes); }

    #[inline]
    pub fn add_callback<O: Into<EventFilter>>(&mut self, on: O, callback: CallbackType, data: RefAny) {
        self.root.add_callback(on, callback, data);
    }

    #[inline]
    pub fn add_css_override<S: Into<AzString>, P: Into<CssProperty>>(&mut self, override_id: S, property: P) {
        self.root.add_css_override(override_id, property);
    }

    #[inline]
    pub fn set_tab_index(&mut self, tab_index: OptionTabIndex) {
        self.root.set_tab_index(tab_index);
    }

    #[inline]
    pub fn set_is_draggable(&mut self, draggable: bool) {
        self.root.set_is_draggable(draggable);
    }

    /// Returns a HTML-formatted version of the DOM for easier debugging, i.e.
    ///
    /// ```rust,no_run,ignore
    /// Dom::div().with_id("hello")
    ///     .with_child(Dom::div().with_id("test"))
    /// ```
    ///
    /// will return:
    ///
    /// ```xml,no_run,ignore
    /// <div id="hello">
    ///      <div id="test" />
    /// </div>
    /// ```
    pub fn get_html_string(&self) -> String {
        let mut output = String::new();
        get_html_string_inner(self, &mut output, 0);
        output.trim().to_string()
    }

    /// Checks whether this node is of the given node type (div, image, text)
    #[inline]
    pub fn is_node_type(&self, searched_type: NodeType) -> bool { self.root.is_node_type(searched_type) }
    /// Checks whether this node has the searched ID attached
    pub fn has_id(&self, id: &str) -> bool { self.root.has_id(id) }
    /// Checks whether this node has the searched class attached
    pub fn has_class(&self, class: &str) -> bool { self.root.has_class(class) }

}

fn get_html_string_inner(dom: &Dom, output: &mut String, indent: usize) {
    let tabs = String::from("    ").repeat(indent);

    let content = dom.root.node_type.get_text_content();
    let print_self_closing_tag = dom.children.is_empty() && content.is_none();

    output.push_str("\r\n");
    output.push_str(&tabs);
    output.push_str(&dom.root.debug_print_start(print_self_closing_tag));

    if let Some(content) = &content {
        output.push_str("\r\n");
        output.push_str(&tabs);
        output.push_str(&"    ");
        output.push_str(content);
    }

    if !print_self_closing_tag {

        for c in dom.children.iter() {
            get_html_string_inner(c, output, indent + 1);
        }

        output.push_str("\r\n");
        output.push_str(&tabs);
        output.push_str(&dom.root.debug_print_end());
    }
}

#[test]
fn test_dom_sibling_1() {

    struct TestLayout;

    let dom: Dom<TestLayout> =
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

    assert_eq!(vec![String::Static("sibling-1")],
        arena.node_data[
            arena.node_hierarchy[dom.root]
            .first_child.expect("root has no first child")
        ].ids);

    assert_eq!(vec![String::Static("sibling-2")],
        arena.node_data[
            arena.node_hierarchy[
                arena.node_hierarchy[dom.root]
                .first_child.expect("root has no first child")
            ].next_sibling.expect("root has no second sibling")
        ].ids);

    assert_eq!(vec![String::Static("sibling-1-child-1")],
        arena.node_data[
            arena.node_hierarchy[
                arena.node_hierarchy[dom.root]
                .first_child.expect("root has no first child")
            ].first_child.expect("first child has no first child")
        ].ids);

    assert_eq!(vec![String::Static("sibling-2-child-1")],
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

    struct TestLayout;

    let dom: Dom<TestLayout> = (0..5).map(|e| NodeData::new(NodeType::Label(format!("{}", e + 1).into()))).collect();
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
        node_type: NodeType::Label(String::Heap(String::from("5"))),
        .. Default::default()
    }));
}

/// Test that there shouldn't be a DOM that has 0 nodes
#[test]
fn test_zero_size_dom() {

    struct TestLayout;

    let null_dom: Dom<TestLayout> = (0..0).map(|_| NodeData::default()).collect();
    let null_dom = convert_dom_into_compact_dom(null_dom);

    assert!(null_dom.arena.len() == 1);
}
