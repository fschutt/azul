use std::{
    fmt,
    hash::{Hash, Hasher},
    sync::atomic::{AtomicUsize, Ordering},
    cmp::Ordering as CmpOrdering,
    iter::FromIterator,
};
use azul_css::{NodeTypePath, CssProperty};
use {
    callbacks::{
        DefaultCallbackId, StackCheckedPointer,
        Callback, GlTextureCallback, IFrameCallback,
    },
    app_resources::{ImageId, TextId},
    id_tree::{Arena, NodeDataContainer},
};

pub use id_tree::{NodeHierarchy, Node, NodeId};

static TAG_ID: AtomicUsize = AtomicUsize::new(1);

pub type TagId = u64;

/// Same as the `TagId`, but only for scrollable nodes
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct ScrollTagId(pub TagId);

pub fn new_tag_id() -> TagId {
    TAG_ID.fetch_add(1, Ordering::SeqCst) as TagId
}

pub fn reset_tag_id() {
    TAG_ID.swap(1, Ordering::SeqCst);
}

impl ScrollTagId {
    pub fn new() -> ScrollTagId {
        ScrollTagId(new_tag_id())
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
    pub fn new(parent: Option<(DomId, NodeId)>) -> DomId {
        DomId {
            id: DOM_ID.fetch_add(1, Ordering::SeqCst),
            parent: parent.map(|(p, node_id)| (Box::new(p), node_id)),
        }
    }

    /// Reset the DOM ID to 1, usually done once-per-frame for the root DOM
    pub fn reset() {
        DOM_ID.swap(1, Ordering::SeqCst);
    }

    /// Creates an ID for the root node
    #[inline]
    pub const fn create_root_dom_id() -> Self  {
        Self {
            id: 0,
            parent: None,
        }
    }

    /// Returns if this is the root node
    pub fn is_root(&self) -> bool {
        *self == Self::create_root_dom_id()
    }
}

/// Calculated hash of a DOM node, used for querying attributes of the DOM node
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct DomHash(pub u64);

/// List of core DOM node types built-into by `azul`.
pub enum NodeType<T> {
    /// Regular div with no particular type of data attached
    Div,
    /// A small label that can be (optionally) be selectable with the mouse
    Label(DomString),
    /// Larger amount of text, that has to be cached
    Text(TextId),
    /// An image that is rendered by WebRender. The id is acquired by the
    /// `AppState::add_image()` function
    Image(ImageId),
    /// OpenGL texture. The `Svg` widget deserizalizes itself into a texture
    /// Equality and Hash values are only checked by the OpenGl texture ID,
    /// Azul does not check that the contents of two textures are the same
    GlTexture((GlTextureCallback<T>, StackCheckedPointer<T>)),
    /// DOM that gets passed its width / height during the layout
    IFrame((IFrameCallback<T>, StackCheckedPointer<T>)),
}

impl<T> NodeType<T> {
    fn get_text_content(&self) -> Option<String> {
        use self::NodeType::*;
        match self {
            Div => None,
            Label(s) => Some(format!("{}", s)),
            Image(id) => Some(format!("image({:?})", id)),
            Text(t) => Some(format!("textid({:?})", t)),
            GlTexture(g) => Some(format!("gltexture({:?})", g)),
            IFrame(i) => Some(format!("iframe({:?})", i)),
        }
    }
}

// #[derive(Debug, Clone, PartialEq, Hash, Eq)] for NodeType<T>

impl<T> fmt::Debug for NodeType<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::NodeType::*;
        match self {
            Div => write!(f, "NodeType::Div"),
            Label(a) => write!(f, "NodeType::Label {{ {:?} }}", a),
            Text(a) => write!(f, "NodeType::Text {{ {:?} }}", a),
            Image(a) => write!(f, "NodeType::Image {{ {:?} }}", a),
            GlTexture((ptr, cb)) => write!(f, "NodeType::GlTexture {{ ptr: {:?}, callback: {:?} }}", ptr, cb),
            IFrame((ptr, cb)) => write!(f, "NodeType::IFrame {{ ptr: {:?}, callback: {:?} }}", ptr, cb),
        }
    }
}

impl<T> Clone for NodeType<T> {
    fn clone(&self) -> Self {
        use self::NodeType::*;
        match self {
            Div => Div,
            Label(a) => Label(a.clone()),
            Text(a) => Text(a.clone()),
            Image(a) => Image(a.clone()),
            GlTexture((ptr, a)) => GlTexture((ptr.clone(), a.clone())),
            IFrame((ptr, a)) => IFrame((ptr.clone(), a.clone())),
        }
    }
}

impl<T> Hash for NodeType<T> {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        use self::NodeType::*;
        use std::mem;
        mem::discriminant(&self).hash(state);
        match self {
            Div => { },
            Label(a) => a.hash(state),
            Text(a) => a.hash(state),
            Image(a) => a.hash(state),
            GlTexture((ptr, a)) => {
                ptr.hash(state);
                a.hash(state);
            },
            IFrame((ptr, a)) => {
                ptr.hash(state);
                a.hash(state);
            },
        }
    }
}

impl<T> PartialEq for NodeType<T> {
    fn eq(&self, rhs: &Self) -> bool {
        use self::NodeType::*;
        match (self, rhs) {
            (Div, Div) => true,
            (Label(a), Label(b)) => a == b,
            (Text(a), Text(b)) => a == b,
            (Image(a), Image(b)) => a == b,
            (GlTexture((ptr_a, a)), GlTexture((ptr_b, b))) => {
                a == b && ptr_a == ptr_b
            },
            (IFrame((ptr_a, a)), IFrame((ptr_b, b))) => {
                a == b && ptr_a == ptr_b
            },
            _ => false,
        }
    }
}

impl<T> Eq for NodeType<T> { }

impl<T> NodeType<T> {
    #[inline]
    pub fn get_path(&self) -> NodeTypePath {
        use self::NodeType::*;
        match self {
            Div => NodeTypePath::Div,
            Label(_) | Text(_) => NodeTypePath::P,
            Image(_) => NodeTypePath::Img,
            GlTexture(_) => NodeTypePath::Texture,
            IFrame(_) => NodeTypePath::IFrame,
        }
    }
}

/// When to call a callback action - `On::MouseOver`, `On::MouseOut`, etc.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
pub enum NotEventFilter {
    Hover(HoverEventFilter),
    Focus(FocusEventFilter),
}

/// Event filter similar to `HoverEventFilter` that only fires when the element is focused
///
/// **Important**: In order for this to fire, the item must have a `tabindex` attribute
/// (to indicate that the item is focus-able).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    TextInput,
    VirtualKeyDown,
    VirtualKeyUp,
    FocusReceived,
    FocusLost,
}

/// Event filter that fires when any action fires on the entire window
/// (regardless of whether any element is hovered or focused over).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

/// Represents one single DOM node (node type, classes, ids and callbacks are stored here)
pub struct NodeData<T> {
    /// `div`
    node_type: NodeType<T>,
    /// `#main #something`
    ids: Vec<DomString>,
    /// `.myclass .otherclass`
    classes: Vec<DomString>,
    /// `On::MouseUp` -> `Callback(my_button_click_handler)`
    callbacks: Vec<(EventFilter, Callback<T>)>,
    /// Usually not set by the user directly - `FakeWindow::add_default_callback`
    /// returns a callback ID, so that we know which default callback(s) are attached
    /// to this node.
    ///
    /// This is only important if this node has any default callbacks.
    default_callback_ids: Vec<(EventFilter, DefaultCallbackId)>,
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
    dynamic_css_overrides: Vec<(DomString, CssProperty)>,
    /// Whether this div can be dragged or not, similar to `draggable = "true"` in HTML, .
    ///
    /// **TODO**: Currently doesn't do anything, since the drag & drop implementation is missing, API stub.
    is_draggable: bool,
    /// Whether this div can be focused, and if yes, in what default to `None` (not focusable).
    /// Note that without this, there can be no `On::FocusReceived` (equivalent to onfocus),
    /// `On::FocusLost` (equivalent to onblur), etc. events.
    tab_index: Option<TabIndex>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
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

impl<T> PartialEq for NodeData<T> {
    fn eq(&self, other: &Self) -> bool {
        self.node_type == other.node_type &&
        self.ids == other.ids &&
        self.classes == other.classes &&
        self.callbacks == other.callbacks &&
        self.default_callback_ids == other.default_callback_ids &&
        self.dynamic_css_overrides == other.dynamic_css_overrides &&
        self.is_draggable == other.is_draggable &&
        self.tab_index == other.tab_index
    }
}

impl<T> Eq for NodeData<T> { }

impl<T> Default for NodeData<T> {
    fn default() -> Self {
        NodeData::new(NodeType::Div)
    }
}

impl<T> Hash for NodeData<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node_type.hash(state);
        for id in &self.ids {
            id.hash(state);
        }
        for class in &self.classes {
            class.hash(state);
        }
        for callback in &self.callbacks {
            callback.hash(state);
        }
        for default_callback_id in &self.default_callback_ids {
            default_callback_id.hash(state);
        }
        for dynamic_css_override in &self.dynamic_css_overrides {
            dynamic_css_override.hash(state);
        }
        self.is_draggable.hash(state);
        self.tab_index.hash(state);
    }
}

impl<T> Clone for NodeData<T> {
    fn clone(&self) -> Self {
        Self {
            node_type: self.node_type.clone(),
            ids: self.ids.clone(),
            classes: self.classes.clone(),
            callbacks: self.callbacks.clone(),
            default_callback_ids: self.default_callback_ids.clone(),
            dynamic_css_overrides: self.dynamic_css_overrides.clone(),
            is_draggable: self.is_draggable.clone(),
            tab_index: self.tab_index.clone(),
        }
    }
}

impl<T> fmt::Display for NodeData<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {

        let html_type = self.node_type.get_path();
        let text_content = self.node_type.get_text_content();

        let id_string = if self.ids.is_empty() {
            String::new()
        } else {
            format!(" id=\"{}\"", self.ids.iter().map(|s| s.as_str().to_string()).collect::<Vec<String>>().join(" "))
        };

        let class_string = if self.classes.is_empty() {
            String::new()
        } else {
            format!(" class=\"{}\"", self.classes.iter().map(|s| s.as_str().to_string()).collect::<Vec<String>>().join(" "))
        };

        let draggable = if self.is_draggable {
            format!(" draggable=\"true\"")
        } else {
            String::new()
        };

        let tabindex = if let Some(tab_index) = self.tab_index {
            format!(" tabindex=\"{}\"", tab_index.get_index())
        } else {
            String::new()
        };

        let callbacks = if self.callbacks.is_empty() {
            String::new()
        } else {
            format!(" callbacks=\"{}\"", self.callbacks.iter().map(|(evt, cb)| format!("({:?}={:?})", evt, cb)).collect::<Vec<String>>().join(" "))
        };

        let default_callbacks = if self.default_callback_ids.is_empty() {
            String::new()
        } else {
            format!(" default-callbacks=\"{}\"", self.default_callback_ids.iter().map(|(evt, cb)| format!("({:?}={:?})", evt, cb)).collect::<Vec<String>>().join(" "))
        };

        let css_overrides = if self.dynamic_css_overrides.is_empty() {
            String::new()
        } else {
            format!(" css-overrides=\"{}\"", self.dynamic_css_overrides.iter().map(|(id, prop)| format!("{}={:?};", id, prop)).collect::<Vec<String>>().join(" "))
        };

        if let Some(content) = text_content {
            write!(f, "<{}{}{}{}{}{}{}{}>{}</{}>",
                html_type, id_string, class_string, tabindex, draggable, callbacks, default_callbacks, css_overrides, content, html_type
            )
        } else {
            write!(f, "<{}{}{}{}{}{}{}{}/>",
                html_type, id_string, class_string, tabindex, draggable, callbacks, default_callbacks, css_overrides,
            )
        }
    }
}

impl<T> fmt::Debug for NodeData<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
            "NodeData {{ \
                \tnode_type: {:?}, \
                \tids: {:?}, \
                \tclasses: {:?}, \
                \tcallbacks: {:?}, \
                \tdefault_callback_ids: {:?}, \
                \tdynamic_css_overrides: {:?}, \
                \tis_draggable: {:?}, \
                \ttab_index: {:?}, \
            }}",
            self.node_type,
            self.ids,
            self.classes,
            self.callbacks,
            self.default_callback_ids,
            self.dynamic_css_overrides,
            self.is_draggable,
            self.tab_index,
        )
    }
}

impl<T> NodeData<T> {

    /// Creates a new `NodeData` instance from a given `NodeType`
    ///
    /// TODO: promote to const fn once `const_vec_new` is stable!
    #[inline]
    pub fn new(node_type: NodeType<T>) -> Self {
        Self {
            node_type,
            ids: Vec::new(),
            classes: Vec::new(),
            callbacks: Vec::new(),
            default_callback_ids: Vec::new(),
            dynamic_css_overrides: Vec::new(),
            is_draggable: false,
            tab_index: None,
        }
    }

    /// Checks whether this node is of the given node type (div, image, text)
    #[inline]
    pub fn is_node_type(&self, searched_type: NodeType<T>) -> bool {
        self.node_type == searched_type
    }

    /// Checks whether this node has the searched ID attached
    pub fn has_id(&self, id: &str) -> bool {
        self.ids.iter().any(|self_id| self_id.equals_str(id))
    }

    /// Checks whether this node has the searched class attached
    pub fn has_class(&self, class: &str) -> bool {
        self.classes.iter().any(|self_class| self_class.equals_str(class))
    }

    pub fn calculate_node_data_hash(&self) -> DomHash {

        use std::collections::hash_map::DefaultHasher as HashAlgorithm;

        let mut hasher = HashAlgorithm::default();
        self.hash(&mut hasher);

        DomHash(hasher.finish())
    }

    /// Shorthand for `NodeData::new(NodeType::Div)`.
    #[inline(always)]
    pub fn div() -> Self {
        Self::new(NodeType::Div)
    }

    /// Shorthand for `NodeData::new(NodeType::Label(value.into()))`
    #[inline(always)]
    pub fn label<S: Into<DomString>>(value: S) -> Self {
        Self::new(NodeType::Label(value.into()))
    }

    /// Shorthand for `NodeData::new(NodeType::Text(text_id))`
    #[inline(always)]
    pub fn text_id(text_id: TextId) -> Self {
        Self::new(NodeType::Text(text_id))
    }

    /// Shorthand for `NodeData::new(NodeType::Image(image_id))`
    #[inline(always)]
    pub fn image(image: ImageId) -> Self {
        Self::new(NodeType::Image(image))
    }

    /// Shorthand for `NodeData::new(NodeType::GlTexture((callback, ptr)))`
    #[inline(always)]
    pub fn gl_texture(callback: GlTextureCallback<T>, ptr: StackCheckedPointer<T>) -> Self {
        Self::new(NodeType::GlTexture((callback, ptr)))
    }

    /// Shorthand for `NodeData::new(NodeType::IFrame((callback, ptr)))`
    #[inline(always)]
    pub fn iframe(callback: IFrameCallback<T>, ptr: StackCheckedPointer<T>) -> Self {
        Self::new(NodeType::IFrame((callback, ptr)))
    }

    // NOTE: Getters are used here in order to allow changing the memory allocator for the NodeData
    // in the future (which is why the fields are all private).

    #[inline(always)]
    pub const fn get_node_type(&self) -> &NodeType<T> { &self.node_type }
    #[inline(always)]
    pub const fn get_ids(&self) -> &Vec<DomString> { &self.ids }
    #[inline(always)]
    pub const fn get_classes(&self) -> &Vec<DomString> { &self.classes }
    #[inline(always)]
    pub const fn get_callbacks(&self) -> &Vec<(EventFilter, Callback<T>)> { &self.callbacks }
    #[inline(always)]
    pub const fn get_default_callback_ids(&self) -> &Vec<(EventFilter, DefaultCallbackId)> { &self.default_callback_ids }
    #[inline(always)]
    pub const fn get_dynamic_css_overrides(&self) -> &Vec<(DomString, CssProperty)> { &self.dynamic_css_overrides }
    #[inline(always)]
    pub const fn get_is_draggable(&self) -> bool { self.is_draggable }
    #[inline(always)]
    pub const fn get_tab_index(&self) -> Option<TabIndex> { self.tab_index }

    #[inline(always)]
    pub fn set_node_type(&mut self, node_type: NodeType<T>) { self.node_type = node_type; }
    #[inline(always)]
    pub fn set_ids(&mut self, ids: Vec<DomString>) { self.ids = ids; }
    #[inline(always)]
    pub fn set_classes(&mut self, classes: Vec<DomString>) { self.classes = classes; }
    #[inline(always)]
    pub fn set_callbacks(&mut self, callbacks: Vec<(EventFilter, Callback<T>)>) { self.callbacks = callbacks; }
    #[inline(always)]
    pub fn set_default_callback_ids(&mut self, default_callback_ids: Vec<(EventFilter, DefaultCallbackId)>) { self.default_callback_ids = default_callback_ids; }
    #[inline(always)]
    pub fn set_dynamic_css_overrides(&mut self, dynamic_css_overrides: Vec<(DomString, CssProperty)>) { self.dynamic_css_overrides = dynamic_css_overrides; }
    #[inline(always)]
    pub fn set_is_draggable(&mut self, is_draggable: bool) { self.is_draggable = is_draggable; }
    #[inline(always)]
    pub fn set_tab_index(&mut self, tab_index: Option<TabIndex>) { self.tab_index = tab_index; }

    #[inline(always)]
    pub fn with_node_type(self, node_type: NodeType<T>) -> Self { Self { node_type, .. self } }
    #[inline(always)]
    pub fn with_ids(self, ids: Vec<DomString>) -> Self { Self { ids, .. self } }
    #[inline(always)]
    pub fn with_classes(self, classes: Vec<DomString>) -> Self { Self { classes, .. self } }
    #[inline(always)]
    pub fn with_callbacks(self, callbacks: Vec<(EventFilter, Callback<T>)>) -> Self { Self { callbacks, .. self } }
    #[inline(always)]
    pub fn with_default_callback_ids(self, default_callback_ids: Vec<(EventFilter, DefaultCallbackId)>) -> Self { Self { default_callback_ids, .. self } }
    #[inline(always)]
    pub fn with_dynamic_css_overrides(self, dynamic_css_overrides: Vec<(DomString, CssProperty)>) -> Self { Self { dynamic_css_overrides, .. self } }
    #[inline(always)]
    pub fn is_draggable(self, is_draggable: bool) -> Self { Self { is_draggable, .. self } }
    #[inline(always)]
    pub fn with_tab_index(self, tab_index: Option<TabIndex>) -> Self { Self { tab_index, .. self } }
}

/// Most strings are known at compile time, spares a bit of
/// heap allocations - for `&'static str`, simply stores the pointer,
/// instead of converting it into a String. This is good for class names
/// or IDs, whose content rarely changes.
#[derive(Debug, Clone)]
pub enum DomString {
    Static(&'static str),
    Heap(String),
}

impl Eq for DomString { }

impl PartialEq for DomString {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialOrd for DomString {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.as_str().cmp(other.as_str()))
    }
}

impl Ord for DomString {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        self.as_str().cmp(other.as_str())
    }
}

impl Hash for DomString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl DomString {

    pub fn equals_str(&self, target: &str) -> bool {
        use self::DomString::*;
        match &self {
            Static(s) => *s == target,
            Heap(h) => h == target,
        }
    }

    pub fn as_str(&self) -> &str {
        use self::DomString::*;
        match &self {
            Static(s) => s,
            Heap(h) => h,
        }
    }
}

impl fmt::Display for DomString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::DomString::*;
        match &self {
            Static(s) => write!(f, "{}", s),
            Heap(h) => write!(f, "{}", h),
        }
    }
}

impl From<String> for DomString {
    fn from(e: String) -> Self {
        DomString::Heap(e)
    }
}

impl From<&'static str> for DomString {
    fn from(e: &'static str) -> Self {
        DomString::Static(e)
    }
}

/// The document model, similar to HTML. This is a create-only structure, you don't actually read anything back
pub struct Dom<T> {
    pub arena: Arena<NodeData<T>>,
    pub root: NodeId,
    pub(crate) head: NodeId,
}

impl<T> Clone for Dom<T> {
    fn clone(&self) -> Self {
        Dom {
            arena: self.arena.clone(),
            root: self.root.clone(),
            head: self.head.clone(),
        }
    }
}

impl<T> PartialEq for Dom<T> {
    fn eq(&self, rhs: &Self) -> bool {
        self.arena == rhs.arena &&
        self.root == rhs.root &&
        self.head == rhs.head
    }
}

impl<T> Eq for Dom<T> { }

impl<T> fmt::Debug for Dom<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
        "Dom {{ arena: {:?}, root: {:?}, head: {:?} }}",
        self.arena,
        self.root,
        self.head)
    }
}

impl<T> FromIterator<Dom<T>> for Dom<T> {
    fn from_iter<I: IntoIterator<Item=Dom<T>>>(iter: I) -> Self {
        let mut c = Dom::new(NodeType::Div);
        for i in iter {
            c.add_child(i);
        }
        c
    }
}

impl<T> FromIterator<NodeData<T>> for Dom<T> {
    fn from_iter<I: IntoIterator<Item=NodeData<T>>>(iter: I) -> Self {

        // We have to use a "root" node, otherwise we run into problems if
        // the iterator executes 0 times (and therefore pushes 0 nodes)

        // "Root" node of this DOM
        let mut node_data = vec![NodeData::new(NodeType::Div)];
        let mut node_layout = vec![Node {
            parent: None,
            previous_sibling: None,
            next_sibling: None,
            last_child: None,
            first_child: None,
        }];

        let mut idx = 0;

        for item in iter {
            let node = Node {
                parent: Some(NodeId::new(0)),
                previous_sibling: if idx == 0 { None } else { Some(NodeId::new(idx)) },
                next_sibling: Some(NodeId::new(idx + 2)),
                last_child: None,
                first_child: None,
            };
            node_layout.push(node);
            node_data.push(item);

            idx += 1;
        }

        let nodes_len = node_layout.len();

        // nodes_len is always at least 1, since we pushed the original root node
        // Check if there is a child DOM
        if nodes_len > 1 {
            if let Some(last) = node_layout.get_mut(nodes_len - 1) {
                last.next_sibling = None;
            }
            node_layout[0].last_child = Some(NodeId::new(nodes_len - 1));
            node_layout[0].first_child = Some(NodeId::new(1));
        }

        Dom {
            head: NodeId::new(0),
            root: NodeId::new(0),
            arena: Arena {
                node_data: NodeDataContainer::new(node_data),
                node_layout: NodeHierarchy::new(node_layout),
            },
        }
    }
}

impl<T> FromIterator<NodeType<T>> for Dom<T> {
    fn from_iter<I: IntoIterator<Item=NodeType<T>>>(iter: I) -> Self {
        iter.into_iter().map(|i| NodeData { node_type: i, .. Default::default() }).collect()
    }
}

/// TODO: promote to const fn once `const_vec_new` is stable
fn init_arena_with_node_data<T>(node_data: NodeData<T>) -> Arena<NodeData<T>> {
    use id_tree::ROOT_NODE;
    Arena {
        node_layout: NodeHierarchy { internal: vec![ROOT_NODE] },
        node_data: NodeDataContainer { internal: vec![node_data] },
    }
}

/// Prints the debug version of the arena, without printing the actual arena
pub(crate) fn print_tree<T, F: Fn(&NodeData<T>) -> String + Copy>(arena: &Arena<NodeData<T>>, format_cb: F) -> String {
    let mut s = String::new();
    if arena.len() > 0 {
        print_tree_recursive(arena, format_cb, &mut s, NodeId::new(0), 0);
    }
    s
}

fn print_tree_recursive<T, F: Fn(&NodeData<T>) -> String + Copy>(arena: &Arena<NodeData<T>>, format_cb: F, string: &mut String, current_node_id: NodeId, indent: usize) {
    let node = &arena.node_layout[current_node_id];
    let tabs = String::from("    ").repeat(indent);
    string.push_str(&format!("{}{}\n", tabs, format_cb(&arena.node_data[current_node_id])));

    if let Some(first_child) = node.first_child {
        print_tree_recursive(arena, format_cb, string, first_child, indent + 1);
        if node.last_child.is_some() {
            string.push_str(&format!("{}</{}>\n", tabs, arena.node_data[current_node_id].node_type.get_path()));
        }
    }

    if let Some(next_sibling) = node.next_sibling {
        print_tree_recursive(arena, format_cb, string, next_sibling, indent);
    }
}

impl<T> Dom<T> {

    /// Creates an empty DOM with a give `NodeType`. Note: This is a `const fn` and
    /// doesn't allocate, it only allocates once you add at least one child node.
    ///
    /// TODO: promote to const fn once `const_vec_new` is stable
    #[inline]
    pub fn new(node_type: NodeType<T>) -> Self {
        use id_tree::ROOT_NODE_ID;
        let node_data = NodeData::new(node_type); // not const fn yet
        let arena = init_arena_with_node_data(node_data); // not const fn yet
        Self {
            arena,
            root: ROOT_NODE_ID,
            head: ROOT_NODE_ID,
        }
    }

    /// Creates an empty DOM with space reserved for `cap` nodes
    #[inline]
    pub fn with_capacity(node_type: NodeType<T>, cap: usize) -> Self {
        let mut arena = Arena::with_capacity(cap.saturating_add(1));
        let root = arena.new_node(NodeData::new(node_type));
        Self {
            arena: arena,
            root: root,
            head: root,
        }
    }

    /// Shorthand for `Dom::new(NodeType::Div)`.
    #[inline]
    pub fn div() -> Self {
        Self::new(NodeType::Div)
    }

    /// Shorthand for `Dom::new(NodeType::Label(value.into()))`
    #[inline]
    pub fn label<S: Into<DomString>>(value: S) -> Self {
        Self::new(NodeType::Label(value.into()))
    }

    /// Shorthand for `Dom::new(NodeType::Text(text_id))`
    #[inline]
    pub fn text_id(text_id: TextId) -> Self {
        Self::new(NodeType::Text(text_id))
    }

    /// Shorthand for `Dom::new(NodeType::Image(image_id))`
    #[inline]
    pub fn image(image: ImageId) -> Self {
        Self::new(NodeType::Image(image))
    }

    /// Shorthand for `Dom::new(NodeType::GlTexture((callback, ptr)))`
    #[inline]
    pub fn gl_texture(callback: GlTextureCallback<T>, ptr: StackCheckedPointer<T>) -> Self {
        Self::new(NodeType::GlTexture((callback, ptr)))
    }

    /// Shorthand for `Dom::new(NodeType::IFrame((callback, ptr)))`
    #[inline]
    pub fn iframe(callback: IFrameCallback<T>, ptr: StackCheckedPointer<T>) -> Self {
        Self::new(NodeType::IFrame((callback, ptr)))
    }

    /// Returns the number of nodes in this DOM
    #[inline]
    pub fn len(&self) -> usize {
        self.arena.len()
    }

    /// Returns an immutable reference to the current HEAD of the DOM structure (the last inserted element)
    #[inline]
    pub fn get_head_node(&self) -> &NodeData<T> {
        &self.arena.node_data[self.head]
    }

    /// Returns a mutable reference to the current HEAD of the DOM structure (the last inserted element)
    #[inline]
    pub fn get_head_node_mut(&mut self) -> &mut NodeData<T> {
        &mut self.arena.node_data[self.head]
    }

    /// Adds a child DOM to the current DOM
    pub fn add_child(&mut self, mut child: Self) {

        // Note: for a more readable Python version of this algorithm,
        // see: https://gist.github.com/fschutt/4b3bd9a2654b548a6eb0b6a8623bdc8a#file-dow_new_2-py-L65-L107

        let self_len = self.arena.len();
        let child_len = child.arena.len();

        if child_len == 0 {
            // No nodes to append, nothing to do
            return;
        }

        if self_len == 0 {
            // Self has no nodes, therefore all child nodes will
            // replace the self nodes, so
            *self = child;
            return;
        }

        let self_arena = &mut self.arena;
        let child_arena = &mut child.arena;

        let mut last_sibling = None;

        for node_id in 0..child_len {
            let node_id = NodeId::new(node_id);
            let node_id_child: &mut Node = &mut child_arena.node_layout[node_id];

            // WARNING: Order of these blocks is important!

            if node_id_child.previous_sibling.as_mut().and_then(|previous_sibling| {
                // Some(previous_sibling) - increase the parent ID by the current arena length
                *previous_sibling += self_len;
                Some(previous_sibling)
            }).is_none() {
                // None - set the current heads' last child as the new previous sibling
                let last_child = self_arena.node_layout[self.head].last_child;
                if last_child.is_some() && node_id_child.parent.is_none() {
                    node_id_child.previous_sibling = last_child;
                    self_arena.node_layout[last_child.unwrap()].next_sibling = Some(node_id + self_len);
                }
            }

            if node_id_child.parent.as_mut().map(|parent| { *parent += self_len; parent }).is_none() {
                // Have we encountered the last root item?
                if node_id_child.next_sibling.is_none() {
                    last_sibling = Some(node_id);
                }
                node_id_child.parent = Some(self.head);
            }

            if let Some(next_sibling) = node_id_child.next_sibling.as_mut() {
                *next_sibling += self_len;
            }

            if let Some(first_child) = node_id_child.first_child.as_mut() {
                *first_child += self_len;
            }

            if let Some(last_child) = node_id_child.last_child.as_mut() {
                *last_child += self_len;
            }
        }

        self_arena.node_layout[self.head].first_child.get_or_insert(NodeId::new(self_len));
        self_arena.node_layout[self.head].last_child = Some(last_sibling.unwrap() + self_len);

        (&mut *self_arena).append_arena(child_arena);
    }

    /// Same as `id`, but easier to use for method chaining in a builder-style pattern
    #[inline]
    pub fn with_id<S: Into<DomString>>(mut self, id: S) -> Self {
        self.add_id(id);
        self
    }

    /// Same as `id`, but easier to use for method chaining in a builder-style pattern
    #[inline]
    pub fn with_class<S: Into<DomString>>(mut self, class: S) -> Self {
        self.add_class(class);
        self
    }

    /// Same as `event`, but easier to use for method chaining in a builder-style pattern
    #[inline]
    pub fn with_callback<O: Into<EventFilter>>(mut self, on: O, callback: Callback<T>) -> Self {
        self.add_callback(on, callback);
        self
    }

    #[inline]
    pub fn with_child(mut self, child: Self) -> Self {
        self.add_child(child);
        self
    }

    #[inline]
    pub fn with_css_override<S: Into<DomString>>(mut self, id: S, property: CssProperty) -> Self {
        self.add_css_override(id, property);
        self
    }

    #[inline]
    pub fn with_tab_index(mut self, tab_index: TabIndex) -> Self {
        self.set_tab_index(tab_index);
        self
    }

    #[inline]
    pub fn is_draggable(mut self, draggable: bool) -> Self {
        self.set_draggable(draggable);
        self
    }

    #[inline]
    pub fn add_id<S: Into<DomString>>(&mut self, id: S) {
        self.arena.node_data[self.head].ids.push(id.into());
    }

    #[inline]
    pub fn add_class<S: Into<DomString>>(&mut self, class: S) {
        self.arena.node_data[self.head].classes.push(class.into());
    }

    #[inline]
    pub fn add_callback<O: Into<EventFilter>>(&mut self, on: O, callback: Callback<T>) {
        self.arena.node_data[self.head].callbacks.push((on.into(), callback));
    }

    #[inline]
    pub fn add_default_callback_id<O: Into<EventFilter>>(&mut self, on: O, id: DefaultCallbackId) {
        self.arena.node_data[self.head].default_callback_ids.push((on.into(), id));
    }

    #[inline]
    pub fn add_css_override<S: Into<DomString>>(&mut self, override_id: S, property: CssProperty) {
        self.arena.node_data[self.head].dynamic_css_overrides.push((override_id.into(), property));
    }

    #[inline]
    pub fn set_tab_index(&mut self, tab_index: TabIndex) {
        self.arena.node_data[self.head].tab_index = Some(tab_index);
    }

    #[inline]
    pub fn set_draggable(&mut self, draggable: bool) {
        self.arena.node_data[self.head].is_draggable = draggable;
    }

    /// Returns a debug formatted version of the DOM for easier debugging
    pub fn debug_dump(&self) -> String {
        format!("{}", print_tree(&self.arena, |t| format!("{}", t)))
    }
}

#[test]
fn test_dom_sibling_1() {

    struct TestLayout;

    let dom: Dom<TestLayout> =
        Dom::new(NodeType::Div)
            .with_child(
                Dom::new(NodeType::Div)
                .with_id("sibling-1")
                .with_child(Dom::new(NodeType::Div)
                    .with_id("sibling-1-child-1")))
            .with_child(Dom::new(NodeType::Div)
                .with_id("sibling-2")
                .with_child(Dom::new(NodeType::Div)
                    .with_id("sibling-2-child-1")));

    let arena = &dom.arena;

    assert_eq!(NodeId::new(0), dom.root);

    assert_eq!(vec![DomString::Static("sibling-1")],
        arena.node_data[
            arena.node_layout[dom.root]
            .first_child.expect("root has no first child")
        ].ids);

    assert_eq!(vec![DomString::Static("sibling-2")],
        arena.node_data[
            arena.node_layout[
                arena.node_layout[dom.root]
                .first_child.expect("root has no first child")
            ].next_sibling.expect("root has no second sibling")
        ].ids);

    assert_eq!(vec![DomString::Static("sibling-1-child-1")],
        arena.node_data[
            arena.node_layout[
                arena.node_layout[dom.root]
                .first_child.expect("root has no first child")
            ].first_child.expect("first child has no first child")
        ].ids);

    assert_eq!(vec![DomString::Static("sibling-2-child-1")],
        arena.node_data[
            arena.node_layout[
                arena.node_layout[
                    arena.node_layout[dom.root]
                    .first_child.expect("root has no first child")
                ].next_sibling.expect("first child has no second sibling")
            ].first_child.expect("second sibling has no first child")
        ].ids);
}

#[test]
fn test_dom_from_iter_1() {

    use id_tree::Node;

    struct TestLayout;

    let dom: Dom<TestLayout> = (0..5).map(|e| NodeData::new(NodeType::Label(format!("{}", e + 1).into()))).collect();
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
    assert_eq!(arena.node_layout.get(NodeId::new(0)), Some(&Node {
        parent: None,
        previous_sibling: None,
        next_sibling: None,
        first_child: Some(NodeId::new(1)),
        last_child: Some(NodeId::new(5)),
    }));
    assert_eq!(arena.node_data.get(NodeId::new(0)), Some(&NodeData::new(NodeType::Div)));

    assert_eq!(arena.node_layout.get(NodeId::new(arena.node_layout.len() - 1)), Some(&Node {
        parent: Some(NodeId::new(0)),
        previous_sibling: Some(NodeId::new(4)),
        next_sibling: None,
        first_child: None,
        last_child: None,
    }));

    assert_eq!(arena.node_data.get(NodeId::new(arena.node_data.len() - 1)), Some(&NodeData {
        node_type: NodeType::Label(DomString::Heap(String::from("5"))),
        .. Default::default()
    }));
}

/// Test that there shouldn't be a DOM that has 0 nodes
#[test]
fn test_zero_size_dom() {

    struct TestLayout;

    let mut null_dom: Dom<TestLayout> = (0..0).map(|_| NodeData::default()).collect();

    assert!(null_dom.arena.len() == 1);

    null_dom.add_class("hello"); // should not panic
    null_dom.add_id("id-hello"); // should not panic
}
