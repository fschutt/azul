    #![allow(dead_code, unused_imports)]
    //! `Dom` construction and configuration
    use crate::dll::*;
    use std::ffi::c_void;
    impl std::iter::FromIterator<Dom> for Dom {
        fn from_iter<I: IntoIterator<Item=Dom>>(iter: I) -> Self {

            let mut estimated_total_children = 0;
            let children = iter.into_iter().map(|c| {
                estimated_total_children += c.estimated_total_children + 1;
                c
            }).collect();

            Dom {
                root: NodeData::new(NodeType::Div),
                children,
                estimated_total_children,
            }
        }
    }

    impl std::iter::FromIterator<NodeData> for Dom {
        fn from_iter<I: IntoIterator<Item=NodeData>>(iter: I) -> Self {
            use crate::vec::DomVec;
            let children = iter.into_iter().map(|c| Dom { root: c, children: DomVec::new(), estimated_total_children: 0 }).collect::<DomVec>();
            let estimated_total_children = children.len();

            Dom {
                root: NodeData::new(NodeType::Div),
                children: children,
                estimated_total_children,
            }
        }
    }

    impl std::iter::FromIterator<NodeType> for Dom {
        fn from_iter<I: IntoIterator<Item=NodeType>>(iter: I) -> Self {
            iter.into_iter().map(|i| {
                let mut nd = NodeData::default();
                nd.node_type = i;
                nd
            }).collect()
        }
    }

    impl From<On> for AzEventFilter {
        fn from(on: On) -> AzEventFilter {
            on.into_event_filter()
        }
    }    use crate::str::String;
    use crate::resources::{ImageId, TextId};
    use crate::callbacks::{CallbackType, GlCallbackType, IFrameCallbackType, RefAny};
    use crate::vec::StringVec;
    use crate::css::CssProperty;
    use crate::option::{OptionImageMask, OptionTabIndex};


    /// `Dom` struct
    pub use crate::dll::AzDom as Dom;

    impl Dom {
        /// Creates a new node with the given `NodeType`
        pub fn new(node_type: NodeType) -> Self { (crate::dll::get_azul_dll().az_dom_new)(node_type) }
        /// Creates a new `div` node
        pub fn div() -> Self { (crate::dll::get_azul_dll().az_dom_div)() }
        /// Creates a new `body` node
        pub fn body() -> Self { (crate::dll::get_azul_dll().az_dom_body)() }
        /// Creates a new `p` node with a given `String` as the text contents
        pub fn label(text: String) -> Self { (crate::dll::get_azul_dll().az_dom_label)(text) }
        /// Creates a new `p` node from a (cached) text referenced by a `TextId`
        pub fn text(text_id: TextId) -> Self { (crate::dll::get_azul_dll().az_dom_text)(text_id) }
        /// Creates a new `img` node from a (cached) text referenced by a `ImageId`
        pub fn image(image_id: ImageId) -> Self { (crate::dll::get_azul_dll().az_dom_image)(image_id) }
        /// Creates a new node which will render an OpenGL texture after the layout step is finished. See the documentation for [GlCallback]() for more info about OpenGL rendering callbacks.
        pub fn gl_texture(data: RefAny, callback: GlCallbackType) -> Self { (crate::dll::get_azul_dll().az_dom_gl_texture)(data, callback) }
        /// Creates a new node with a callback that will return a `Dom` after being layouted. See the documentation for [IFrameCallback]() for more info about iframe callbacks.
        pub fn iframe(data: RefAny, callback: IFrameCallbackType) -> Self { (crate::dll::get_azul_dll().az_dom_iframe)(data, callback) }
        /// Adds a CSS ID (`#something`) to the DOM node
        pub fn add_id(&mut self, id: String)  { (crate::dll::get_azul_dll().az_dom_add_id)(self, id) }
        /// Same as [`Dom::add_id`](#method.add_id), but as a builder method
        pub fn with_id(self, id: String)  -> crate::dom::Dom { (crate::dll::get_azul_dll().az_dom_with_id)(self, id) }
        /// Same as calling [`Dom::add_id`](#method.add_id) for each CSS ID, but this function **replaces** all current CSS IDs
        pub fn set_ids(&mut self, ids: StringVec)  { (crate::dll::get_azul_dll().az_dom_set_ids)(self, ids) }
        /// Same as [`Dom::set_ids`](#method.set_ids), but as a builder method
        pub fn with_ids(self, ids: StringVec)  -> crate::dom::Dom { (crate::dll::get_azul_dll().az_dom_with_ids)(self, ids) }
        /// Adds a CSS class (`.something`) to the DOM node
        pub fn add_class(&mut self, class: String)  { (crate::dll::get_azul_dll().az_dom_add_class)(self, class) }
        /// Same as [`Dom::add_class`](#method.add_class), but as a builder method
        pub fn with_class(self, class: String)  -> crate::dom::Dom { (crate::dll::get_azul_dll().az_dom_with_class)(self, class) }
        /// Same as calling [`Dom::add_class`](#method.add_class) for each class, but this function **replaces** all current classes
        pub fn set_classes(&mut self, classes: StringVec)  { (crate::dll::get_azul_dll().az_dom_set_classes)(self, classes) }
        /// Same as [`Dom::set_classes`](#method.set_classes), but as a builder method
        pub fn with_classes(self, classes: StringVec)  -> crate::dom::Dom { (crate::dll::get_azul_dll().az_dom_with_classes)(self, classes) }
        /// Adds a [`Callback`](callbacks/type.Callback) that acts on the `data` the `event` happens
        pub fn add_callback(&mut self, event: EventFilter, data: RefAny, callback: CallbackType)  { (crate::dll::get_azul_dll().az_dom_add_callback)(self, event, data, callback) }
        /// Same as [`Dom::add_callback`](#method.add_callback), but as a builder method
        pub fn with_callback(self, event: EventFilter, data: RefAny, callback: CallbackType)  -> crate::dom::Dom { (crate::dll::get_azul_dll().az_dom_with_callback)(self, event, data, callback) }
        /// Overrides the CSS property of this DOM node with a value (for example `"width = 200px"`)
        pub fn add_inline_css(&mut self, prop: CssProperty)  { (crate::dll::get_azul_dll().az_dom_add_inline_css)(self, prop) }
        /// Same as [`Dom::add_inline_css`](#method.add_inline_css), but as a builder method
        pub fn with_inline_css(self, prop: CssProperty)  -> crate::dom::Dom { (crate::dll::get_azul_dll().az_dom_with_inline_css)(self, prop) }
        /// Overrides the CSS property of this DOM node with a value (for example `"width = 200px"`)
        pub fn add_inline_hover_css(&mut self, prop: CssProperty)  { (crate::dll::get_azul_dll().az_dom_add_inline_hover_css)(self, prop) }
        /// Same as [`Dom::add_inline_hover_css`](#method.add_inline_hover_css), but as a builder method
        pub fn with_inline_hover_css(self, prop: CssProperty)  -> crate::dom::Dom { (crate::dll::get_azul_dll().az_dom_with_inline_hover_css)(self, prop) }
        /// Overrides the CSS property of this DOM node with a value (for example `"width = 200px"`)
        pub fn add_inline_active_css(&mut self, prop: CssProperty)  { (crate::dll::get_azul_dll().az_dom_add_inline_active_css)(self, prop) }
        /// Same as [`Dom::add_inline_active_css`](#method.add_inline_active_css), but as a builder method
        pub fn with_inline_active_css(self, prop: CssProperty)  -> crate::dom::Dom { (crate::dll::get_azul_dll().az_dom_with_inline_active_css)(self, prop) }
        /// Overrides the CSS property of this DOM node with a value (for example `"width = 200px"`)
        pub fn add_inline_focus_css(&mut self, prop: CssProperty)  { (crate::dll::get_azul_dll().az_dom_add_inline_focus_css)(self, prop) }
        /// Same as [`Dom::add_inline_focus_css`](#method.add_inline_active_css), but as a builder method
        pub fn with_inline_focus_css(self, prop: CssProperty)  -> crate::dom::Dom { (crate::dll::get_azul_dll().az_dom_with_inline_focus_css)(self, prop) }
        /// Sets the `is_draggable` attribute of this DOM node (default: false)
        pub fn set_is_draggable(&mut self, is_draggable: bool)  { (crate::dll::get_azul_dll().az_dom_set_is_draggable)(self, is_draggable) }
        /// Same as [`Dom::set_clip_mask`](#method.set_clip_mask), but as a builder method
        pub fn with_clip_mask(self, clip_mask: OptionImageMask)  -> crate::dom::Dom { (crate::dll::get_azul_dll().az_dom_with_clip_mask)(self, clip_mask) }
        /// Sets the `clip_mask` attribute of this DOM node (default: None)
        pub fn set_clip_mask(&mut self, clip_mask: OptionImageMask)  { (crate::dll::get_azul_dll().az_dom_set_clip_mask)(self, clip_mask) }
        /// Same as [`Dom::set_is_draggable`](#method.set_is_draggable), but as a builder method
        pub fn is_draggable(self, is_draggable: bool)  -> crate::dom::Dom { (crate::dll::get_azul_dll().az_dom_is_draggable)(self, is_draggable) }
        /// Sets the `tabindex` attribute of this DOM node (makes an element focusable - default: None)
        pub fn set_tab_index(&mut self, tab_index: OptionTabIndex)  { (crate::dll::get_azul_dll().az_dom_set_tab_index)(self, tab_index) }
        /// Same as [`Dom::set_tab_index`](#method.set_tab_index), but as a builder method
        pub fn with_tab_index(self, tab_index: OptionTabIndex)  -> crate::dom::Dom { (crate::dll::get_azul_dll().az_dom_with_tab_index)(self, tab_index) }
        /// Returns if the DOM node has a certain CSS ID
        pub fn has_id(&mut self, id: String)  -> bool { (crate::dll::get_azul_dll().az_dom_has_id)(self, id) }
        /// Returns if the DOM node has a certain CSS class
        pub fn has_class(&mut self, class: String)  -> bool { (crate::dll::get_azul_dll().az_dom_has_class)(self, class) }
        /// Reparents another `Dom` to be the child node of this `Dom`
        pub fn add_child(&mut self, child: Dom)  { (crate::dll::get_azul_dll().az_dom_add_child)(self, child) }
        /// Same as [`Dom::add_child`](#method.add_child), but as a builder method
        pub fn with_child(self, child: Dom)  -> crate::dom::Dom { (crate::dll::get_azul_dll().az_dom_with_child)(self, child) }
        /// Returns the HTML String for this DOM
        pub fn get_html_string(&self)  -> crate::str::String { (crate::dll::get_azul_dll().az_dom_get_html_string)(self) }
    }

    impl std::fmt::Debug for Dom { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_dom_fmt_debug)(self)) } }
    impl Clone for Dom { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_dom_deep_copy)(self) } }
    impl Drop for Dom { fn drop(&mut self) { (crate::dll::get_azul_dll().az_dom_delete)(self); } }


    /// `GlTextureNode` struct
    pub use crate::dll::AzGlTextureNode as GlTextureNode;

    impl std::fmt::Debug for GlTextureNode { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_gl_texture_node_fmt_debug)(self)) } }
    impl Clone for GlTextureNode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_gl_texture_node_deep_copy)(self) } }
    impl Drop for GlTextureNode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gl_texture_node_delete)(self); } }


    /// `IFrameNode` struct
    pub use crate::dll::AzIFrameNode as IFrameNode;

    impl std::fmt::Debug for IFrameNode { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_i_frame_node_fmt_debug)(self)) } }
    impl Clone for IFrameNode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_i_frame_node_deep_copy)(self) } }
    impl Drop for IFrameNode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_i_frame_node_delete)(self); } }


    /// `CallbackData` struct
    pub use crate::dll::AzCallbackData as CallbackData;

    impl std::fmt::Debug for CallbackData { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_callback_data_fmt_debug)(self)) } }
    impl Clone for CallbackData { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_callback_data_deep_copy)(self) } }
    impl Drop for CallbackData { fn drop(&mut self) { (crate::dll::get_azul_dll().az_callback_data_delete)(self); } }


    /// `ImageMask` struct
    pub use crate::dll::AzImageMask as ImageMask;

    impl std::fmt::Debug for ImageMask { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_image_mask_fmt_debug)(self)) } }
    impl Clone for ImageMask { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_image_mask_deep_copy)(self) } }
    impl Drop for ImageMask { fn drop(&mut self) { (crate::dll::get_azul_dll().az_image_mask_delete)(self); } }


    /// Represents one single DOM node (node type, classes, ids and callbacks are stored here)
    pub use crate::dll::AzNodeData as NodeData;

    impl NodeData {
        /// Creates a new node without any classes or ids from a NodeType
        pub fn new(node_type: NodeType) -> Self { (crate::dll::get_azul_dll().az_node_data_new)(node_type) }
        /// Creates a new `div` node
        pub fn div() -> Self { (crate::dll::get_azul_dll().az_node_data_div)() }
        /// Creates a new `body` node
        pub fn body() -> Self { (crate::dll::get_azul_dll().az_node_data_body)() }
        /// Creates a new `p` node with a given `String` as the text contents
        pub fn label(text: String) -> Self { (crate::dll::get_azul_dll().az_node_data_label)(text) }
        /// Creates a new `p` node from a (cached) text referenced by a `TextId`
        pub fn text(text_id: TextId) -> Self { (crate::dll::get_azul_dll().az_node_data_text)(text_id) }
        /// Creates a new `img` node from a (cached) text referenced by a `ImageId`
        pub fn image(image_id: ImageId) -> Self { (crate::dll::get_azul_dll().az_node_data_image)(image_id) }
        /// Creates a new node which will render an OpenGL texture after the layout step is finished. See the documentation for [GlCallback]() for more info about OpenGL rendering callbacks.
        pub fn gl_texture(data: RefAny, callback: GlCallbackType) -> Self { (crate::dll::get_azul_dll().az_node_data_gl_texture)(data, callback) }
        /// Creates a `NodeData` with a callback that will return a `Dom` after being layouted. See the documentation for [IFrameCallback]() for more info about iframe callbacks.
        pub fn iframe(data: RefAny, callback: IFrameCallbackType) -> Self { (crate::dll::get_azul_dll().az_node_data_iframe)(data, callback) }
        /// Creates a default (div) node without any classes
        pub fn default() -> Self { (crate::dll::get_azul_dll().az_node_data_default)() }
        /// Adds a CSS ID (`#something`) to the `NodeData`
        pub fn add_id(&mut self, id: String)  { (crate::dll::get_azul_dll().az_node_data_add_id)(self, id) }
        /// Same as [`NodeData::add_id`](#method.add_id), but as a builder method
        pub fn with_id(self, id: String)  -> crate::dom::NodeData { (crate::dll::get_azul_dll().az_node_data_with_id)(self, id) }
        /// Same as calling [`NodeData::add_id`](#method.add_id) for each CSS ID, but this function **replaces** all current CSS IDs
        pub fn set_ids(&mut self, ids: StringVec)  { (crate::dll::get_azul_dll().az_node_data_set_ids)(self, ids) }
        /// Same as [`NodeData::set_ids`](#method.set_ids), but as a builder method
        pub fn with_ids(self, ids: StringVec)  -> crate::dom::NodeData { (crate::dll::get_azul_dll().az_node_data_with_ids)(self, ids) }
        /// Adds a CSS class (`.something`) to the `NodeData`
        pub fn add_class(&mut self, class: String)  { (crate::dll::get_azul_dll().az_node_data_add_class)(self, class) }
        /// Same as [`NodeData::add_class`](#method.add_class), but as a builder method
        pub fn with_class(self, class: String)  -> crate::dom::NodeData { (crate::dll::get_azul_dll().az_node_data_with_class)(self, class) }
        /// Same as calling [`NodeData::add_class`](#method.add_class) for each class, but this function **replaces** all current classes
        pub fn set_classes(&mut self, classes: StringVec)  { (crate::dll::get_azul_dll().az_node_data_set_classes)(self, classes) }
        /// Same as [`NodeData::set_classes`](#method.set_classes), but as a builder method
        pub fn with_classes(self, classes: StringVec)  -> crate::dom::NodeData { (crate::dll::get_azul_dll().az_node_data_with_classes)(self, classes) }
        /// Adds a [`Callback`](callbacks/type.Callback) that acts on the `data` the `event` happens
        pub fn add_callback(&mut self, event: EventFilter, data: RefAny, callback: CallbackType)  { (crate::dll::get_azul_dll().az_node_data_add_callback)(self, event, data, callback) }
        /// Same as [`NodeData::add_callback`](#method.add_callback), but as a builder method
        pub fn with_callback(self, event: EventFilter, data: RefAny, callback: CallbackType)  -> crate::dom::NodeData { (crate::dll::get_azul_dll().az_node_data_with_callback)(self, event, data, callback) }
        /// Overrides the CSS property of this `NodeData` node with a value (for example `"width = 200px"`)
        pub fn add_inline_css(&mut self, prop: CssProperty)  { (crate::dll::get_azul_dll().az_node_data_add_inline_css)(self, prop) }
        /// Same as [`NodeData::add_inline_focus_css`](#method.add_inline_focus_css), but as a builder method
        pub fn with_inline_css(self, prop: CssProperty)  -> crate::dom::NodeData { (crate::dll::get_azul_dll().az_node_data_with_inline_css)(self, prop) }
        /// Overrides the CSS property of this `NodeData` node with a value (for example `"width = 200px"`)
        pub fn add_inline_hover_css(&mut self, prop: CssProperty)  { (crate::dll::get_azul_dll().az_node_data_add_inline_hover_css)(self, prop) }
        /// Overrides the CSS property of this `NodeData` node with a value (for example `"width = 200px"`)
        pub fn add_inline_active_css(&mut self, prop: CssProperty)  { (crate::dll::get_azul_dll().az_node_data_add_inline_active_css)(self, prop) }
        /// Overrides the CSS property of this `NodeData` node with a value (for example `"width = 200px"`)
        pub fn add_inline_focus_css(&mut self, prop: CssProperty)  { (crate::dll::get_azul_dll().az_node_data_add_inline_focus_css)(self, prop) }
        /// Same as [`NodeData::set_clip_mask`](#method.set_clip_mask), but as a builder method
        pub fn with_clip_mask(self, clip_mask: OptionImageMask)  -> crate::dom::NodeData { (crate::dll::get_azul_dll().az_node_data_with_clip_mask)(self, clip_mask) }
        /// Sets the `clip_mask` attribute of this `NodeData` (default: None)
        pub fn set_clip_mask(&mut self, clip_mask: OptionImageMask)  { (crate::dll::get_azul_dll().az_node_data_set_clip_mask)(self, clip_mask) }
        /// Sets the `is_draggable` attribute of this `NodeData` (default: false)
        pub fn set_is_draggable(&mut self, is_draggable: bool)  { (crate::dll::get_azul_dll().az_node_data_set_is_draggable)(self, is_draggable) }
        /// Same as [`NodeData::set_is_draggable`](#method.set_is_draggable), but as a builder method
        pub fn is_draggable(self, is_draggable: bool)  -> crate::dom::NodeData { (crate::dll::get_azul_dll().az_node_data_is_draggable)(self, is_draggable) }
        /// Sets the `tabindex` attribute of this `NodeData` (makes an element focusable - default: None)
        pub fn set_tab_index(&mut self, tab_index: OptionTabIndex)  { (crate::dll::get_azul_dll().az_node_data_set_tab_index)(self, tab_index) }
        /// Same as [`NodeData::set_tab_index`](#method.set_tab_index), but as a builder method
        pub fn with_tab_index(self, tab_index: OptionTabIndex)  -> crate::dom::NodeData { (crate::dll::get_azul_dll().az_node_data_with_tab_index)(self, tab_index) }
        /// Returns if the `NodeData` has a certain CSS ID
        pub fn has_id(&mut self, id: String)  -> bool { (crate::dll::get_azul_dll().az_node_data_has_id)(self, id) }
        /// Returns if the `NodeData` has a certain CSS class
        pub fn has_class(&mut self, class: String)  -> bool { (crate::dll::get_azul_dll().az_node_data_has_class)(self, class) }
    }

    impl std::fmt::Debug for NodeData { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_node_data_fmt_debug)(self)) } }
    impl Clone for NodeData { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_node_data_deep_copy)(self) } }
    impl Drop for NodeData { fn drop(&mut self) { (crate::dll::get_azul_dll().az_node_data_delete)(self); } }


    /// List of core DOM node types built-into by `azul`
    pub use crate::dll::AzNodeType as NodeType;

    impl std::fmt::Debug for NodeType { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_node_type_fmt_debug)(self)) } }
    impl Clone for NodeType { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_node_type_deep_copy)(self) } }
    impl Drop for NodeType { fn drop(&mut self) { (crate::dll::get_azul_dll().az_node_type_delete)(self); } }


    /// When to call a callback action - `On::MouseOver`, `On::MouseOut`, etc.
    pub use crate::dll::AzOn as On;

    impl On {
        /// Converts the `On` shorthand into a `EventFilter`
        pub fn into_event_filter(self)  -> crate::dom::EventFilter { (crate::dll::get_azul_dll().az_on_into_event_filter)(self) }
    }

    impl std::fmt::Debug for On { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_on_fmt_debug)(self)) } }
    impl Clone for On { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_on_deep_copy)(self) } }
    impl Drop for On { fn drop(&mut self) { (crate::dll::get_azul_dll().az_on_delete)(self); } }


    /// `EventFilter` struct
    pub use crate::dll::AzEventFilter as EventFilter;

    impl std::fmt::Debug for EventFilter { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_event_filter_fmt_debug)(self)) } }
    impl Clone for EventFilter { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_event_filter_deep_copy)(self) } }
    impl Drop for EventFilter { fn drop(&mut self) { (crate::dll::get_azul_dll().az_event_filter_delete)(self); } }


    /// `HoverEventFilter` struct
    pub use crate::dll::AzHoverEventFilter as HoverEventFilter;

    impl std::fmt::Debug for HoverEventFilter { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_hover_event_filter_fmt_debug)(self)) } }
    impl Clone for HoverEventFilter { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_hover_event_filter_deep_copy)(self) } }
    impl Drop for HoverEventFilter { fn drop(&mut self) { (crate::dll::get_azul_dll().az_hover_event_filter_delete)(self); } }


    /// `FocusEventFilter` struct
    pub use crate::dll::AzFocusEventFilter as FocusEventFilter;

    impl std::fmt::Debug for FocusEventFilter { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_focus_event_filter_fmt_debug)(self)) } }
    impl Clone for FocusEventFilter { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_focus_event_filter_deep_copy)(self) } }
    impl Drop for FocusEventFilter { fn drop(&mut self) { (crate::dll::get_azul_dll().az_focus_event_filter_delete)(self); } }


    /// `NotEventFilter` struct
    pub use crate::dll::AzNotEventFilter as NotEventFilter;

    impl std::fmt::Debug for NotEventFilter { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_not_event_filter_fmt_debug)(self)) } }
    impl Clone for NotEventFilter { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_not_event_filter_deep_copy)(self) } }
    impl Drop for NotEventFilter { fn drop(&mut self) { (crate::dll::get_azul_dll().az_not_event_filter_delete)(self); } }


    /// `WindowEventFilter` struct
    pub use crate::dll::AzWindowEventFilter as WindowEventFilter;

    impl std::fmt::Debug for WindowEventFilter { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_window_event_filter_fmt_debug)(self)) } }
    impl Clone for WindowEventFilter { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_window_event_filter_deep_copy)(self) } }
    impl Drop for WindowEventFilter { fn drop(&mut self) { (crate::dll::get_azul_dll().az_window_event_filter_delete)(self); } }


    /// `TabIndex` struct
    pub use crate::dll::AzTabIndex as TabIndex;

    impl std::fmt::Debug for TabIndex { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_tab_index_fmt_debug)(self)) } }
    impl Clone for TabIndex { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_tab_index_deep_copy)(self) } }
    impl Drop for TabIndex { fn drop(&mut self) { (crate::dll::get_azul_dll().az_tab_index_delete)(self); } }
