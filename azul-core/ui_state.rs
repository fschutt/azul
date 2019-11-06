use std::{
    fmt,
    collections::BTreeMap,
};
use azul_css::CssProperty;
use crate::{
    FastHashMap,
    id_tree::NodeId,
    dom::{
        Dom, CompactDom, DomId, TagId, TabIndex, DomString,
        HoverEventFilter, FocusEventFilter, NotEventFilter,
        WindowEventFilter,
    },
    callbacks::{
        LayoutInfo, Callback, LayoutCallback, DefaultCallback,
        IFrameCallback, GlCallback, RefAny,
    },
};

pub struct UiState<T> {
    /// Unique identifier for the DOM
    pub dom_id: DomId,
    /// The actual DOM, rendered from the .layout() function
    pub(crate) dom: CompactDom<T>,
    /// The style properties that should be overridden for this frame, cloned from the `Css`
    pub dynamic_css_overrides: BTreeMap<NodeId, FastHashMap<DomString, CssProperty>>,
    /// Stores all tags for nodes that need to activate on a `:hover` or `:active` event.
    pub tag_ids_to_hover_active_states: BTreeMap<TagId, (NodeId, HoverGroup)>,

    /// Tags -> Focusable nodes
    pub tab_index_tags: BTreeMap<TagId, (NodeId, TabIndex)>,
    /// Tags -> Draggable nodes
    pub draggable_tags: BTreeMap<TagId, NodeId>,
    /// Tag IDs -> Node IDs
    pub tag_ids_to_node_ids: BTreeMap<TagId, NodeId>,
    /// Reverse of `tag_ids_to_node_ids`.
    pub node_ids_to_tag_ids: BTreeMap<NodeId, TagId>,

    // For hover, focus and not callbacks, there needs to be a tag generated
    // for hit-testing. Since window and desktop callbacks are not attached to
    // any element, they only store the NodeId (where the event came from), but have
    // no tag themselves.
    //
    // There are two maps per event, one for the regular callbacks and one for
    // the default callbacks. This is done for consistency, since otherwise the
    // event filtering logic gets much more complicated than it already is.
    pub hover_callbacks:                BTreeMap<NodeId, BTreeMap<HoverEventFilter, Callback<T>>>,
    pub hover_default_callbacks:        BTreeMap<NodeId, BTreeMap<HoverEventFilter, DefaultCallback<T>>>,
    pub focus_callbacks:                BTreeMap<NodeId, BTreeMap<FocusEventFilter, Callback<T>>>,
    pub focus_default_callbacks:        BTreeMap<NodeId, BTreeMap<FocusEventFilter, DefaultCallback<T>>>,
    pub not_callbacks:                  BTreeMap<NodeId, BTreeMap<NotEventFilter, Callback<T>>>,
    pub not_default_callbacks:          BTreeMap<NodeId, BTreeMap<NotEventFilter, DefaultCallback<T>>>,
    pub window_callbacks:               BTreeMap<NodeId, BTreeMap<WindowEventFilter, Callback<T>>>,
    pub window_default_callbacks:       BTreeMap<NodeId, BTreeMap<WindowEventFilter, DefaultCallback<T>>>,
}

impl<T> UiState<T> {
    #[inline(always)]
    pub const fn get_dom(&self) -> &CompactDom<T> {
        &self.dom
    }
}

impl<T> fmt::Debug for UiState<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
            "UiState {{ \

                dom: {:?}, \
                dynamic_css_overrides: {:?}, \
                tag_ids_to_hover_active_states: {:?}, \
                tab_index_tags: {:?}, \
                draggable_tags: {:?}, \
                tag_ids_to_node_ids: {:?}, \
                node_ids_to_tag_ids: {:?}, \
                hover_callbacks: {:?}, \
                hover_default_callbacks: {:?}, \
                focus_callbacks: {:?}, \
                focus_default_callbacks: {:?}, \
                not_callbacks: {:?}, \
                not_default_callbacks: {:?}, \
                window_callbacks: {:?}, \
                window_default_callbacks: {:?}, \
            }}",

            self.dom,
            self.dynamic_css_overrides,
            self.tag_ids_to_hover_active_states,
            self.tab_index_tags,
            self.draggable_tags,
            self.tag_ids_to_node_ids,
            self.node_ids_to_tag_ids,
            self.hover_callbacks,
            self.hover_default_callbacks,
            self.focus_callbacks,
            self.focus_default_callbacks,
            self.not_callbacks,
            self.not_default_callbacks,
            self.window_callbacks,
            self.window_default_callbacks,
        )
    }
}

/// In order to support :hover, the element must have a TagId, otherwise it
/// will be disregarded in the hit-testing. A hover group
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct HoverGroup {
    /// Whether any property in the hover group will trigger a re-layout.
    /// This is important for creating
    pub affects_layout: bool,
    /// Whether this path ends with `:active` or with `:hover`
    pub active_or_hover: ActiveHover,
}

/// Sets whether an element needs to be selected for `:active` or for `:hover`
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum ActiveHover {
    Active,
    Hover,
}

impl<T> UiState<T> {

    /// The UiState contains all the tags (for hit-testing) as well as the mapping
    /// from Hit-testing tags to NodeIds (which are important for filtering input events
    /// and routing input events to the callbacks).
    pub fn new(mut dom: Dom<T>, parent_dom: Option<(DomId, NodeId)>) -> UiState<T> {

        use crate::dom::NodeType;

        // NOTE: root node has to have the type "body"
        if *dom.root.get_node_type() != NodeType::Body {
            dom = Dom::body().with_child(dom);
        }

        let dom: CompactDom<T> = dom.into();

        // NOTE: Originally it was allowed to create a DOM with
        // multiple root elements using `add_sibling()` and `with_sibling()`.
        //
        // However, it was decided to remove these functions (in commit #586933),
        // as they aren't practical (you can achieve the same thing with one
        // wrapper div and multiple add_child() calls) and they create problems
        // when laying out elements since add_sibling() essentially modifies the
        // space that the parent can distribute, which in code, simply looks weird
        // and led to bugs.
        //
        // It is assumed that the DOM returned by the user has exactly one root node
        // with no further siblings and that the root node is the Node with the ID 0.

        // All tags that have can be focused (necessary for hit-testing)
        let mut tab_index_tags = BTreeMap::new();
        // All tags that have can be dragged & dropped (necessary for hit-testing)
        let mut draggable_tags = BTreeMap::new();

        // Mapping from tags to nodes (necessary so that the hit-testing can resolve the NodeId from any given tag)
        let mut tag_ids_to_node_ids = BTreeMap::new();
        // Mapping from nodes to tags, reverse mapping (not used right now, may be useful in the future)
        let mut node_ids_to_tag_ids = BTreeMap::new();
        // Which nodes have extra dynamic CSS overrides?
        let mut dynamic_css_overrides = BTreeMap::new();

        let mut hover_callbacks = BTreeMap::new();
        let mut hover_default_callbacks = BTreeMap::new();
        let mut focus_callbacks = BTreeMap::new();
        let mut focus_default_callbacks = BTreeMap::new();
        let mut not_callbacks = BTreeMap::new();
        let mut not_default_callbacks = BTreeMap::new();
        let mut window_callbacks = BTreeMap::new();
        let mut window_default_callbacks = BTreeMap::new();

        macro_rules! filter_step_0 {
            ($event_filter:ident, $callback_type:ty, $data_source:expr, $filter_func:ident) => {{
                let node_hover_callbacks: BTreeMap<$event_filter, $callback_type> = $data_source.iter()
                .filter_map(|(event_filter, cb)| event_filter.$filter_func().map(|not_evt| (not_evt, *cb)))
                .collect();
                node_hover_callbacks
            }};
        };

        macro_rules! filter_and_insert_callbacks {(
                $node_id:ident,
                $data_source:expr,
                $event_filter:ident,
                $callback_type:ty,
                $filter_func:ident,
                $final_callback_list:ident,
        ) => {
            let node_hover_callbacks = filter_step_0!($event_filter, $callback_type, $data_source, $filter_func);
            if !node_hover_callbacks.is_empty() {
                $final_callback_list.insert($node_id, node_hover_callbacks);
            }
        };(
            $node_id:ident,
            $data_source:expr,
            $event_filter:ident,
            $callback_type:ty,
            $filter_func:ident,
            $final_callback_list:ident,
            $node_tag_id:ident,
        ) => {
            let node_hover_callbacks = filter_step_0!($event_filter, $callback_type, $data_source, $filter_func);
            if !node_hover_callbacks.is_empty() {
                $final_callback_list.insert($node_id, node_hover_callbacks);
                let tag_id = $node_tag_id.unwrap_or_else(|| TagId::new());
                $node_tag_id = Some(tag_id);
            }
        };}

        macro_rules! filter_step_0_default {
            ($event_filter:ident, $callback_type:ty, $data_source:expr, $filter_func:ident) => {{
                let node_hover_callbacks: BTreeMap<$event_filter, $callback_type> = $data_source.iter()
                .filter_map(|(event_filter, cb)| event_filter.$filter_func().map(|not_evt| (not_evt, cb.0)))
                .collect();
                node_hover_callbacks
            }};
        };

        macro_rules! filter_and_insert_default_callbacks {(
                $node_id:ident,
                $data_source:expr,
                $event_filter:ident,
                $callback_type:ty,
                $filter_func:ident,
                $final_callback_list:ident,
        ) => {
            let node_hover_callbacks = filter_step_0_default!($event_filter, $callback_type, $data_source, $filter_func);
            if !node_hover_callbacks.is_empty() {
                $final_callback_list.insert($node_id, node_hover_callbacks);
            }
        };(
            $node_id:ident,
            $data_source:expr,
            $event_filter:ident,
            $callback_type:ty,
            $filter_func:ident,
            $final_callback_list:ident,
            $node_tag_id:ident,
        ) => {
            let node_hover_callbacks = filter_step_0_default!($event_filter, $callback_type, $data_source, $filter_func);
            if !node_hover_callbacks.is_empty() {
                $final_callback_list.insert($node_id, node_hover_callbacks);
                let tag_id = $node_tag_id.unwrap_or_else(|| TagId::new());
                $node_tag_id = Some(tag_id);
            }
        };}

        TagId::reset();

        {
            let arena = &dom.arena;

            debug_assert!(arena.node_hierarchy[NodeId::new(0)].next_sibling.is_none());

            for node_id in arena.linear_iter() {

                let node = &arena.node_data[node_id];

                let mut node_tag_id = None;

                // Optimization since on most nodes, the callbacks will be empty
                if !node.get_callbacks().is_empty() {

                    // Filter and insert HoverEventFilter callbacks
                    filter_and_insert_callbacks!(
                        node_id,
                        node.get_callbacks(),
                        HoverEventFilter,
                        Callback<T>,
                        as_hover_event_filter,
                        hover_callbacks,
                        node_tag_id,
                    );

                    // Filter and insert FocusEventFilter callbacks
                    filter_and_insert_callbacks!(
                        node_id,
                        node.get_callbacks(),
                        FocusEventFilter,
                        Callback<T>,
                        as_focus_event_filter,
                        focus_callbacks,
                        node_tag_id,
                    );

                    filter_and_insert_callbacks!(
                        node_id,
                        node.get_callbacks(),
                        NotEventFilter,
                        Callback<T>,
                        as_not_event_filter,
                        not_callbacks,
                        node_tag_id,
                    );

                    filter_and_insert_callbacks!(
                        node_id,
                        node.get_callbacks(),
                        WindowEventFilter,
                        Callback<T>,
                        as_window_event_filter,
                        window_callbacks,
                    );
                }

                if !node.get_default_callbacks().is_empty() {

                    // Filter and insert HoverEventFilter callbacks
                    filter_and_insert_default_callbacks!(
                        node_id,
                        node.get_default_callbacks(),
                        HoverEventFilter,
                        DefaultCallback<T>,
                        as_hover_event_filter,
                        hover_default_callbacks,
                        node_tag_id,
                    );

                    // Filter and insert FocusEventFilter callbacks
                    filter_and_insert_default_callbacks!(
                        node_id,
                        node.get_default_callbacks(),
                        FocusEventFilter,
                        DefaultCallback<T>,
                        as_focus_event_filter,
                        focus_default_callbacks,
                        node_tag_id,
                    );

                    filter_and_insert_default_callbacks!(
                        node_id,
                        node.get_default_callbacks(),
                        NotEventFilter,
                        DefaultCallback<T>,
                        as_not_event_filter,
                        not_default_callbacks,
                        node_tag_id,
                    );

                    filter_and_insert_default_callbacks!(
                        node_id,
                        node.get_default_callbacks(),
                        WindowEventFilter,
                        DefaultCallback<T>,
                        as_window_event_filter,
                        window_default_callbacks,
                    );
                }

                if node.get_is_draggable() {
                    let tag_id = node_tag_id.unwrap_or_else(|| TagId::new());
                    draggable_tags.insert(tag_id, node_id);
                    node_tag_id = Some(tag_id);
                }

                // It's a very common mistake is to set a default callback, but not to call
                // .with_tab_index() - so this "fixes" this behaviour so that if at least one FocusEventFilter
                // is set, the item automatically gets a tabindex attribute assigned.
                let should_insert_tabindex_auto = !focus_callbacks.is_empty() || !focus_default_callbacks.is_empty();
                let node_tab_index = node.get_tab_index().or(if should_insert_tabindex_auto { Some(TabIndex::Auto) } else { None });

                if let Some(tab_index) = node_tab_index {
                    let tag_id = node_tag_id.unwrap_or_else(|| TagId::new());
                    tab_index_tags.insert(tag_id, (node_id, tab_index));
                    node_tag_id = Some(tag_id);
                }

                if let Some(tag_id) = node_tag_id {
                    tag_ids_to_node_ids.insert(tag_id, node_id);
                    node_ids_to_tag_ids.insert(node_id, tag_id);
                }

                // Collect all the styling overrides into one hash map
                if !node.get_dynamic_css_overrides().is_empty() {
                    dynamic_css_overrides.insert(node_id, node.get_dynamic_css_overrides().iter().cloned().collect());
                }
            }
        }

        UiState {

            dom_id: DomId::new(parent_dom),
            dom,
            dynamic_css_overrides,
            tag_ids_to_hover_active_states: BTreeMap::new(),

            tab_index_tags,
            draggable_tags,
            node_ids_to_tag_ids,
            tag_ids_to_node_ids,

            hover_callbacks,
            hover_default_callbacks,
            focus_callbacks,
            focus_default_callbacks,
            not_callbacks,
            not_default_callbacks,
            window_callbacks,
            window_default_callbacks,

        }
    }

    pub fn new_from_app_state<'a>(
        data: &T,
        layout_info: LayoutInfo<'a>,
        parent_dom: Option<(DomId, NodeId)>,
        layout_callback: LayoutCallback<T>,
    ) -> UiState<T> {

        // Only shortly lock the data to get the dom out
        let dom = (layout_callback)(data, layout_info);

        Self::new(dom, parent_dom)
    }

    pub fn create_tags_for_hover_nodes(&mut self, hover_nodes: &BTreeMap<NodeId, HoverGroup>) {

        for (hover_node_id, hover_group) in hover_nodes {
            let hover_tag = match self.node_ids_to_tag_ids.get(hover_node_id) {
                Some(tag_id) => *tag_id,
                None => TagId::new(),
            };

            self.node_ids_to_tag_ids.insert(*hover_node_id, hover_tag);
            self.tag_ids_to_node_ids.insert(hover_tag, *hover_node_id);
            self.tag_ids_to_hover_active_states.insert(hover_tag, (*hover_node_id, *hover_group));
        }
    }

    pub fn scan_for_iframe_callbacks(&self) -> Vec<(NodeId, &(IFrameCallback<T>, RefAny))> {
        use crate::dom::NodeType::IFrame;
        self.dom.arena.node_hierarchy.linear_iter().filter_map(|node_id| {
            let node_data = &self.dom.arena.node_data[node_id];
            match node_data.get_node_type() {
                IFrame(cb) => Some((node_id, cb)),
                _ => None,
            }
        }).collect()
    }

    pub fn scan_for_gltexture_callbacks(&self) -> Vec<(NodeId, &(GlCallback, RefAny))> {
        use crate::dom::NodeType::GlTexture;
        self.dom.arena.node_hierarchy.linear_iter().filter_map(|node_id| {
            let node_data = &self.dom.arena.node_data[node_id];
            match node_data.get_node_type() {
                GlTexture(cb) => Some((node_id, cb)),
                _ => None,
            }
        }).collect()
    }
}
